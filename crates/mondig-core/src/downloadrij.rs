//! De wachtrij voor modeldownloads: de gebruiker kan meerdere modellen tegelijk vragen, ze
//! komen na elkaar binnen.
//!
//! Waarom na elkaar en niet twee tegelijk: de verbinding wordt toch gedeeld, dus samen duurt het
//! niet korter, terwijl het eerste model bij één tegelijk wel eerder bruikbaar is. Daarnaast delen
//! spraakmodellen hun hulpmodel (Silero): twee downloads tegelijk zouden hetzelfde deelbestand
//! kunnen beschrijven. Eén tegelijk sluit dat uit, net als twee downloads van hetzelfde model
//! (valkuil Vibe #1662): een model dat al in de rij staat,
//! komt er niet nog eens bij.
//!
//! De rij weet niets van het netwerk of van de app: het echte werk doet een werker die de
//! aanroeper meegeeft (in de app: [`crate::download::voer_download_uit`]). Elke wijziging gaat
//! naar `bij_wijziging` met de hele lijst, de byte-voortgang naar `bij_voortgang`. Beide worden
//! aangeroepen terwijl de rij vergrendeld is, zodat de volgorde van de meldingen altijd klopt;
//! ze mogen de rij dus zelf niet aanroepen.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use serde::Serialize;

use crate::download::{Melding, Voortgang};
use crate::taak::{Toestand, Vervolg};

/// Het nummer van één item in de rij. Een nieuwe download van hetzelfde model krijgt een nieuw
/// nummer.
pub type ItemId = u64;

/// Waar een download in de rij staat. Serialiseert als getagde enum (`type`), zoals
/// [`Toestand`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RijToestand {
    /// Wacht tot de downloads ervoor klaar zijn.
    Wacht,
    /// Komt nu binnen.
    Bezig,
    /// De verbinding viel weg; de download probeert het zelf opnieuw.
    NieuwePoging {
        /// De hoeveelste nieuwe poging (vanaf 1).
        poging: u32,
        /// Hoeveel nieuwe pogingen er hooguit komen.
        van: u32,
    },
    /// Binnen en gecontroleerd.
    Klaar,
    /// Mislukt, ook na de nieuwe pogingen.
    Fout {
        /// Wat er misging, in gewone taal.
        melding: String,
        /// Wat de gebruiker kan doen.
        vervolg: Vervolg,
    },
    /// De gebruiker heeft de download gestopt; wat binnen was, blijft staan.
    Gestopt,
    /// Mondig werd afgesloten terwijl de download liep of wachtte; hij kan verder waar hij bleef.
    Onderbroken,
}

impl RijToestand {
    /// Wacht de download nog of loopt hij?
    #[must_use]
    pub fn is_actief(&self) -> bool {
        matches!(self, Self::Wacht | Self::Bezig | Self::NieuwePoging { .. })
    }

    /// Komt de download nu binnen (ook tijdens een nieuwe poging)?
    #[must_use]
    pub fn is_bezig(&self) -> bool {
        matches!(self, Self::Bezig | Self::NieuwePoging { .. })
    }
}

/// Eén download in de rij, zoals de interface hem krijgt (camelCase).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RijItem {
    /// Nummer van dit item.
    pub id: ItemId,
    /// Id van het model uit de catalogus.
    pub model: String,
    /// Waar de download staat.
    pub toestand: RijToestand,
    /// Hoe ver hij is, zodra dat bekend is.
    pub voortgang: Option<Voortgang>,
}

/// Wat [`Downloadrij::voeg_toe`] deed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toevoeging {
    /// Het model staat nu in de rij, met dit nummer.
    Nieuw(ItemId),
    /// Het model stond al in de rij (wachtend of bezig): er is niets bijgekomen.
    AlInDeRij(ItemId),
}

impl Toevoeging {
    /// Het nummer van het item, nieuw of al bestaand.
    #[must_use]
    pub fn id(self) -> ItemId {
        match self {
            Self::Nieuw(id) | Self::AlInDeRij(id) => id,
        }
    }
}

/// De hele rij met een volgnummer, zoals de interface hem krijgt. Het volgnummer loopt op bij
/// elke wijziging, zodat de interface een oudere stand die later binnenkomt (een antwoord op
/// `downloads_lijst` dat een event inhaalt) kan negeren.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RijStand {
    /// Loopt op bij elke wijziging van de rij.
    pub versie: u64,
    /// De items, in de volgorde van de rij.
    pub items: Vec<RijItem>,
}

type Waarnemer = Box<dyn Fn(&RijStand) + Send + Sync>;
type VoortgangWaarnemer = Box<dyn Fn(&RijItem) + Send + Sync>;

struct Intern {
    item: RijItem,
    annuleer: Arc<AtomicBool>,
}

#[derive(Default)]
struct Staat {
    items: Vec<Intern>,
    volgend: ItemId,
    versie: u64,
    sluit: bool,
}

impl Staat {
    fn stand(&self) -> RijStand {
        RijStand {
            versie: self.versie,
            items: self.items.iter().map(|i| i.item.clone()).collect(),
        }
    }
}

struct Gedeeld {
    staat: Mutex<Staat>,
    wekker: Condvar,
    bij_wijziging: Waarnemer,
    bij_voortgang: VoortgangWaarnemer,
}

fn slot<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Gedeeld {
    fn meld_wijziging(&self, staat: &mut Staat) {
        staat.versie += 1;
        (self.bij_wijziging)(&staat.stand());
    }
}

/// Wat de werker meekrijgt voor één download.
pub struct Opdracht<'a> {
    id: ItemId,
    model: String,
    annuleer: Arc<AtomicBool>,
    gedeeld: &'a Gedeeld,
}

impl Opdracht<'_> {
    /// Het nummer van het item.
    #[must_use]
    pub fn id(&self) -> ItemId {
        self.id
    }

    /// Het model dat binnen moet komen.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Gaat op `true` als de gebruiker dit item stopt of Mondig afsluit.
    #[must_use]
    pub fn annuleer(&self) -> &Arc<AtomicBool> {
        &self.annuleer
    }

    /// Geeft een melding van de download door aan de rij: voortgang of een nieuwe poging.
    pub fn meld(&self, melding: Melding) {
        let mut staat = slot(&self.gedeeld.staat);
        let Some(item) = staat.items.iter_mut().find(|i| i.item.id == self.id) else {
            return;
        };
        match melding {
            Melding::Voortgang(stand) => {
                item.item.voortgang = Some(stand);
                // Na een nieuwe poging komen er weer bytes binnen: weer gewoon bezig.
                let terug = matches!(item.item.toestand, RijToestand::NieuwePoging { .. });
                if terug {
                    item.item.toestand = RijToestand::Bezig;
                }
                let kopie = item.item.clone();
                if terug {
                    self.gedeeld.meld_wijziging(&mut staat);
                }
                (self.gedeeld.bij_voortgang)(&kopie);
            }
            Melding::NieuwePoging { poging, van } => {
                item.item.toestand = RijToestand::NieuwePoging { poging, van };
                self.gedeeld.meld_wijziging(&mut staat);
            }
        }
    }
}

/// De wachtrij, met één werkdraad die de downloads na elkaar uitvoert.
pub struct Downloadrij {
    gedeeld: Arc<Gedeeld>,
}

impl Downloadrij {
    /// Start de rij met een werker voor één download. De werker geeft de eindtoestand:
    /// [`Toestand::Klaar`], [`Toestand::Fout`] of [`Toestand::Geannuleerd`]. Een werker die in
    /// paniek raakt, geeft een fout en houdt de rij niet tegen.
    pub fn start(
        mut werker: impl FnMut(&Opdracht<'_>) -> Toestand + Send + 'static,
        bij_wijziging: impl Fn(&RijStand) + Send + Sync + 'static,
        bij_voortgang: impl Fn(&RijItem) + Send + Sync + 'static,
    ) -> Self {
        let gedeeld = Arc::new(Gedeeld {
            staat: Mutex::new(Staat::default()),
            wekker: Condvar::new(),
            bij_wijziging: Box::new(bij_wijziging),
            bij_voortgang: Box::new(bij_voortgang),
        });
        let voor_draad = Arc::clone(&gedeeld);
        std::thread::spawn(move || werk(&voor_draad, &mut werker));
        Self { gedeeld }
    }

    /// Zet een model achteraan in de rij. Staat het er al (wachtend of bezig), dan gebeurt er
    /// niets: zo starten een dubbele klik of twee knoppen nooit twee downloads van hetzelfde
    /// model. Een eerder afgelopen item van dit model maakt plaats voor het nieuwe.
    #[must_use = "het nummer zegt of het model al in de rij stond"]
    pub fn voeg_toe(&self, model: &str) -> Toevoeging {
        let mut staat = slot(&self.gedeeld.staat);
        if let Some(bestaand) = staat
            .items
            .iter()
            .find(|i| i.item.model == model && i.item.toestand.is_actief())
        {
            return Toevoeging::AlInDeRij(bestaand.item.id);
        }
        staat.items.retain(|i| i.item.model != model);
        staat.volgend += 1;
        let id = staat.volgend;
        staat.items.push(Intern {
            item: RijItem {
                id,
                model: model.to_string(),
                toestand: RijToestand::Wacht,
                voortgang: None,
            },
            annuleer: Arc::new(AtomicBool::new(false)),
        });
        self.gedeeld.meld_wijziging(&mut staat);
        self.gedeeld.wekker.notify_all();
        Toevoeging::Nieuw(id)
    }

    /// Stopt een item: wacht het nog, dan komt het niet meer aan de beurt; loopt het, dan stopt
    /// de download (wat binnen is, blijft staan). Een onderbroken item wordt "gestopt": de
    /// gebruiker wil het niet hervatten, en na een herstart staat het er niet meer. Geeft `false`
    /// als er niets te stoppen was.
    #[must_use = "`false` als er niets te stoppen was"]
    pub fn stop(&self, id: ItemId) -> bool {
        let mut staat = slot(&self.gedeeld.staat);
        let Some(item) = staat.items.iter_mut().find(|i| i.item.id == id) else {
            return false;
        };
        match item.item.toestand {
            RijToestand::Wacht | RijToestand::Onderbroken => {
                item.item.toestand = RijToestand::Gestopt;
                self.gedeeld.meld_wijziging(&mut staat);
                true
            }
            RijToestand::Bezig | RijToestand::NieuwePoging { .. } => {
                // De werker ziet de vlag en meldt daarna "gestopt".
                item.annuleer.store(true, Ordering::Relaxed);
                true
            }
            _ => false,
        }
    }

    /// Stopt het actieve of onderbroken item van dit model, als er een is.
    #[must_use = "`false` als er niets te stoppen was"]
    pub fn stop_model(&self, model: &str) -> bool {
        let id = slot(&self.gedeeld.staat)
            .items
            .iter()
            .find(|i| {
                i.item.model == model
                    && (i.item.toestand.is_actief() || i.item.toestand == RijToestand::Onderbroken)
            })
            .map(|i| i.item.id);
        id.is_some_and(|id| self.stop(id))
    }

    /// Haalt een afgelopen of onderbroken item van dit model uit de rij, bijvoorbeeld omdat het
    /// model van de computer is verwijderd: dan valt er niets meer te hervatten. Een item dat
    /// wacht of loopt, blijft. Geeft `true` als er iets weg is.
    #[must_use = "`false` als er niets te vergeten was"]
    pub fn vergeet(&self, model: &str) -> bool {
        let mut staat = slot(&self.gedeeld.staat);
        let voor = staat.items.len();
        staat
            .items
            .retain(|i| i.item.model != model || i.item.toestand.is_actief());
        let weg = staat.items.len() != voor;
        if weg {
            self.gedeeld.meld_wijziging(&mut staat);
        }
        weg
    }

    /// De rij met zijn volgnummer, voor de interface.
    #[must_use]
    pub fn stand(&self) -> RijStand {
        slot(&self.gedeeld.staat).stand()
    }

    /// Alle items, in de volgorde van de rij.
    #[must_use]
    pub fn lijst(&self) -> Vec<RijItem> {
        slot(&self.gedeeld.staat)
            .items
            .iter()
            .map(|i| i.item.clone())
            .collect()
    }

    /// Het item dat nu binnenkomt, als er een is.
    #[must_use]
    pub fn bezig(&self) -> Option<RijItem> {
        slot(&self.gedeeld.staat)
            .items
            .iter()
            .find(|i| i.item.toestand.is_bezig())
            .map(|i| i.item.clone())
    }

    /// Wacht dit model in de rij of komt het nu binnen?
    #[must_use]
    pub fn is_actief(&self, model: &str) -> bool {
        slot(&self.gedeeld.staat)
            .items
            .iter()
            .any(|i| i.item.model == model && i.item.toestand.is_actief())
    }

    /// Zet modellen die bij de vorige keer onderbroken werden in de rij als "onderbroken", zodat
    /// de gebruiker ze kan hervatten. Een model dat er al in staat, blijft zoals het is.
    pub fn zet_onderbroken(&self, modellen: &[String]) {
        let mut staat = slot(&self.gedeeld.staat);
        let mut iets = false;
        for model in modellen {
            if staat.items.iter().any(|i| &i.item.model == model) {
                continue;
            }
            staat.volgend += 1;
            let id = staat.volgend;
            staat.items.push(Intern {
                item: RijItem {
                    id,
                    model: model.clone(),
                    toestand: RijToestand::Onderbroken,
                    voortgang: None,
                },
                annuleer: Arc::new(AtomicBool::new(false)),
            });
            iets = true;
        }
        if iets {
            self.gedeeld.meld_wijziging(&mut staat);
        }
    }

    /// Mondig sluit af: de lopende download stopt en alles wat nog actief was, wordt
    /// "onderbroken". Wacht niet op de werkdraad; die stopt zelf na de lopende download.
    pub fn sluit(&self) {
        let mut staat = slot(&self.gedeeld.staat);
        staat.sluit = true;
        for item in &mut staat.items {
            if item.item.toestand == RijToestand::Wacht {
                item.item.toestand = RijToestand::Onderbroken;
            } else if item.item.toestand.is_bezig() {
                item.annuleer.store(true, Ordering::Relaxed);
            }
        }
        self.gedeeld.meld_wijziging(&mut staat);
        self.gedeeld.wekker.notify_all();
    }
}

impl Drop for Downloadrij {
    fn drop(&mut self) {
        self.sluit();
    }
}

/// De werkdraad: pakt steeds het eerste wachtende item en voert het uit.
fn werk(gedeeld: &Gedeeld, werker: &mut dyn FnMut(&Opdracht<'_>) -> Toestand) {
    loop {
        let (id, model, annuleer) = {
            let mut staat = slot(&gedeeld.staat);
            let plek = loop {
                if staat.sluit {
                    return;
                }
                if let Some(plek) = staat
                    .items
                    .iter()
                    .position(|i| i.item.toestand == RijToestand::Wacht)
                {
                    break plek;
                }
                staat = gedeeld
                    .wekker
                    .wait(staat)
                    .unwrap_or_else(PoisonError::into_inner);
            };
            let intern = &mut staat.items[plek];
            intern.item.toestand = RijToestand::Bezig;
            intern.item.voortgang = None;
            let opdracht = (
                intern.item.id,
                intern.item.model.clone(),
                Arc::clone(&intern.annuleer),
            );
            gedeeld.meld_wijziging(&mut staat);
            opdracht
        };
        let opdracht = Opdracht {
            id,
            model,
            annuleer,
            gedeeld,
        };
        let uitkomst = catch_unwind(AssertUnwindSafe(|| werker(&opdracht))).unwrap_or_else(|_| {
            Toestand::fout(
                "Er ging onverwacht iets mis bij het downloaden. Probeer het opnieuw.",
                Vervolg::OpnieuwProberen,
            )
        });
        let mut staat = slot(&gedeeld.staat);
        let sluit = staat.sluit;
        if let Some(intern) = staat.items.iter_mut().find(|i| i.item.id == id) {
            intern.item.toestand = match uitkomst {
                Toestand::Klaar => RijToestand::Klaar,
                Toestand::Fout { melding, vervolg } => RijToestand::Fout { melding, vervolg },
                _ if sluit => RijToestand::Onderbroken,
                _ => RijToestand::Gestopt,
            };
        }
        gedeeld.meld_wijziging(&mut staat);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// Een nepwerker die per download wacht op een uitkomst van de test, of stopt als het item
    /// gestopt wordt. Onthoudt welke modellen hij kreeg.
    struct Nep {
        rij: Downloadrij,
        geef: mpsc::Sender<Toestand>,
        gezien: Arc<Mutex<Vec<String>>>,
        meldingen: Arc<Mutex<Vec<Vec<RijItem>>>>,
    }

    fn nep() -> Nep {
        let (geef, neem) = mpsc::channel::<Toestand>();
        let gezien = Arc::new(Mutex::new(Vec::new()));
        let meldingen = Arc::new(Mutex::new(Vec::new()));
        let (g, m) = (Arc::clone(&gezien), Arc::clone(&meldingen));
        let rij = Downloadrij::start(
            move |opdracht| {
                g.lock().unwrap().push(opdracht.model().to_string());
                loop {
                    if opdracht.annuleer().load(Ordering::Relaxed) {
                        return Toestand::Geannuleerd;
                    }
                    if let Ok(uitkomst) = neem.recv_timeout(Duration::from_millis(5)) {
                        // `Wacht` is hier het teken voor een paniek.
                        assert!(uitkomst != Toestand::Wacht, "de test vraagt om een paniek");
                        return uitkomst;
                    }
                }
            },
            move |stand: &RijStand| m.lock().unwrap().push(stand.items.clone()),
            |_| {},
        );
        Nep {
            rij,
            geef,
            gezien,
            meldingen,
        }
    }

    fn toestand_van(rij: &Downloadrij, model: &str) -> Option<RijToestand> {
        rij.lijst()
            .into_iter()
            .find(|i| i.model == model)
            .map(|i| i.toestand)
    }

    /// Wacht (hooguit twee seconden) tot een model in deze toestand staat.
    fn wacht_op(rij: &Downloadrij, model: &str, toestand: &RijToestand) {
        let tot = Instant::now() + Duration::from_secs(2);
        while toestand_van(rij, model).as_ref() != Some(toestand) {
            assert!(
                Instant::now() < tot,
                "{model} kwam niet in {toestand:?}: {:?}",
                rij.lijst()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn downloads_komen_na_elkaar_in_de_volgorde_van_vragen() {
        let n = nep();
        for model in ["a", "b", "c"] {
            assert!(matches!(n.rij.voeg_toe(model), Toevoeging::Nieuw(_)));
        }
        wacht_op(&n.rij, "a", &RijToestand::Bezig);
        // Eén tegelijk: de rest wacht.
        assert_eq!(toestand_van(&n.rij, "b"), Some(RijToestand::Wacht));
        assert_eq!(toestand_van(&n.rij, "c"), Some(RijToestand::Wacht));
        for model in ["a", "b", "c"] {
            wacht_op(&n.rij, model, &RijToestand::Bezig);
            n.geef.send(Toestand::Klaar).unwrap();
            wacht_op(&n.rij, model, &RijToestand::Klaar);
        }
        assert_eq!(*n.gezien.lock().unwrap(), vec!["a", "b", "c"]);
        // Nooit twee tegelijk bezig, in geen enkele melding.
        assert!(
            n.meldingen.lock().unwrap().iter().all(|lijst| lijst
                .iter()
                .filter(|i| i.toestand.is_bezig())
                .count()
                <= 1)
        );
    }

    #[test]
    fn hetzelfde_model_komt_er_nooit_twee_keer_in() {
        let n = nep();
        let eerste = n.rij.voeg_toe("a");
        let Toevoeging::Nieuw(id) = eerste else {
            unreachable!("{eerste:?}");
        };
        // Een dubbele klik terwijl hij wacht of al loopt: hetzelfde item, niets erbij.
        assert_eq!(n.rij.voeg_toe("a"), Toevoeging::AlInDeRij(id));
        wacht_op(&n.rij, "a", &RijToestand::Bezig);
        assert_eq!(n.rij.voeg_toe("a"), Toevoeging::AlInDeRij(id));
        assert_eq!(n.rij.lijst().len(), 1);
        n.geef.send(Toestand::Klaar).unwrap();
        wacht_op(&n.rij, "a", &RijToestand::Klaar);
        // Daarna mag het weer, als nieuw item op dezelfde plek in de lijst.
        let tweede = n.rij.voeg_toe("a");
        assert!(
            matches!(tweede, Toevoeging::Nieuw(nieuw) if nieuw != id),
            "{tweede:?}"
        );
        assert_eq!(n.rij.lijst().len(), 1);
        wacht_op(&n.rij, "a", &RijToestand::Bezig);
        n.geef.send(Toestand::Klaar).unwrap();
        wacht_op(&n.rij, "a", &RijToestand::Klaar);
        assert_eq!(*n.gezien.lock().unwrap(), vec!["a", "a"]);
    }

    #[test]
    fn stoppen_werkt_voor_een_wachtend_en_een_lopend_item() {
        let n = nep();
        let a = n.rij.voeg_toe("a").id();
        let b = n.rij.voeg_toe("b").id();
        let _ = n.rij.voeg_toe("c");
        wacht_op(&n.rij, "a", &RijToestand::Bezig);
        // b wacht nog: hij komt niet meer aan de beurt.
        assert!(n.rij.stop(b));
        assert_eq!(toestand_van(&n.rij, "b"), Some(RijToestand::Gestopt));
        // a loopt: de werker ziet de vlag en stopt.
        assert!(n.rij.stop(a));
        wacht_op(&n.rij, "a", &RijToestand::Gestopt);
        wacht_op(&n.rij, "c", &RijToestand::Bezig);
        assert!(n.rij.stop_model("c"));
        wacht_op(&n.rij, "c", &RijToestand::Gestopt);
        assert_eq!(*n.gezien.lock().unwrap(), vec!["a", "c"]);
        // Een afgelopen item stop je niet nog eens.
        assert!(!n.rij.stop(a));
        assert!(!n.rij.is_actief("a"));
    }

    #[test]
    fn meldingen_van_de_werker_komen_in_het_item() {
        let (klaar, wacht) = mpsc::channel::<()>();
        let rij = Downloadrij::start(
            move |opdracht| {
                opdracht.meld(Melding::NieuwePoging { poging: 2, van: 5 });
                let _ = wacht.recv();
                opdracht.meld(Melding::Voortgang(Voortgang {
                    binnen: 40,
                    totaal: 100,
                }));
                let _ = wacht.recv();
                Toestand::fout("weg", Vervolg::OpnieuwProberen)
            },
            |_| {},
            |_| {},
        );
        let _ = rij.voeg_toe("a");
        wacht_op(&rij, "a", &RijToestand::NieuwePoging { poging: 2, van: 5 });
        assert!(rij.is_actief("a"));
        assert_eq!(rij.bezig().map(|i| i.model), Some("a".to_string()));
        klaar.send(()).unwrap();
        // Weer bytes binnen: weer gewoon bezig, met de voortgang erbij.
        wacht_op(&rij, "a", &RijToestand::Bezig);
        assert_eq!(
            rij.lijst()[0].voortgang,
            Some(Voortgang {
                binnen: 40,
                totaal: 100
            })
        );
        klaar.send(()).unwrap();
        wacht_op(
            &rij,
            "a",
            &RijToestand::Fout {
                melding: "weg".into(),
                vervolg: Vervolg::OpnieuwProberen,
            },
        );
        assert!(rij.bezig().is_none());
    }

    #[test]
    fn onderbroken_downloads_staan_klaar_om_te_hervatten() {
        let n = nep();
        n.rij.zet_onderbroken(&["a".to_string(), "b".to_string()]);
        assert_eq!(toestand_van(&n.rij, "a"), Some(RijToestand::Onderbroken));
        // Onderbroken loopt niet vanzelf.
        std::thread::sleep(Duration::from_millis(30));
        assert!(n.gezien.lock().unwrap().is_empty());
        // Hervatten: het item maakt plaats voor een nieuw, wachtend item.
        let _ = n.rij.voeg_toe("a");
        wacht_op(&n.rij, "a", &RijToestand::Bezig);
        assert_eq!(n.rij.lijst().len(), 2);
        // Afsluiten: wat liep of wachtte, wordt onderbroken.
        let _ = n.rij.voeg_toe("c");
        n.rij.sluit();
        wacht_op(&n.rij, "a", &RijToestand::Onderbroken);
        assert_eq!(toestand_van(&n.rij, "c"), Some(RijToestand::Onderbroken));
        assert_eq!(toestand_van(&n.rij, "b"), Some(RijToestand::Onderbroken));
    }

    #[test]
    fn een_onderbroken_download_is_weg_te_klikken_en_te_vergeten() {
        let n = nep();
        n.rij.zet_onderbroken(&["a".to_string(), "b".to_string()]);
        // "Niet hervatten": gestopt, dus na een herstart niet meer onderbroken.
        assert!(n.rij.stop_model("a"));
        assert_eq!(toestand_van(&n.rij, "a"), Some(RijToestand::Gestopt));
        // Het model is verwijderd: het item gaat uit de rij.
        assert!(n.rij.vergeet("b"));
        assert_eq!(toestand_van(&n.rij, "b"), None);
        assert!(!n.rij.vergeet("b"));
        // Wat wacht of loopt, vergeet de rij niet.
        let _ = n.rij.voeg_toe("c");
        wacht_op(&n.rij, "c", &RijToestand::Bezig);
        assert!(!n.rij.vergeet("c"));
        assert!(n.rij.is_actief("c"));
        assert!(n.rij.stop_model("c"));
        wacht_op(&n.rij, "c", &RijToestand::Gestopt);
        // De werker kreeg alleen c.
        assert_eq!(*n.gezien.lock().unwrap(), vec!["c"]);
    }

    #[test]
    fn elke_wijziging_krijgt_een_hoger_volgnummer() {
        let n = nep();
        let voor = n.rij.stand().versie;
        let _ = n.rij.voeg_toe("a");
        wacht_op(&n.rij, "a", &RijToestand::Bezig);
        let midden = n.rij.stand().versie;
        assert!(midden > voor);
        n.geef.send(Toestand::Klaar).unwrap();
        wacht_op(&n.rij, "a", &RijToestand::Klaar);
        assert!(n.rij.stand().versie > midden);
        // Het volgnummer in de meldingen loopt mee.
        assert_eq!(
            n.meldingen.lock().unwrap().len() as u64,
            n.rij.stand().versie
        );
    }

    #[test]
    fn een_werker_in_paniek_geeft_een_fout_en_de_rij_gaat_door() {
        let n = nep();
        let _ = n.rij.voeg_toe("a");
        let _ = n.rij.voeg_toe("b");
        wacht_op(&n.rij, "a", &RijToestand::Bezig);
        n.geef.send(Toestand::Wacht).unwrap();
        wacht_op(&n.rij, "b", &RijToestand::Bezig);
        assert!(matches!(
            toestand_van(&n.rij, "a"),
            Some(RijToestand::Fout { .. })
        ));
        n.geef.send(Toestand::Klaar).unwrap();
        wacht_op(&n.rij, "b", &RijToestand::Klaar);
    }

    #[test]
    fn het_item_serialiseert_voor_de_interface() {
        let item = RijItem {
            id: 3,
            model: "a".into(),
            toestand: RijToestand::NieuwePoging { poging: 1, van: 5 },
            voortgang: Some(Voortgang {
                binnen: 1,
                totaal: 2,
            }),
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["toestand"]["type"], "nieuwe_poging");
        assert_eq!(json["toestand"]["poging"], 1);
        assert_eq!(json["voortgang"]["binnen"], 1);
        let fout = serde_json::to_value(RijToestand::Fout {
            melding: "x".into(),
            vervolg: Vervolg::OpnieuwProberen,
        })
        .unwrap();
        assert_eq!(fout["vervolg"], "opnieuw_proberen");
        assert_eq!(
            serde_json::to_value(RijToestand::Onderbroken).unwrap()["type"],
            "onderbroken"
        );
    }
}
