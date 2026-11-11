//! De Mac-kant van plakken (mijlpaal M1). Naar het voorbeeld van Handy
//! (`src-tauri/src/paste_tx/macos.rs`, MIT, commit `73cc857`), met de beslissingen uit
//! `plakken.rs`.
//!
//! - **De tekst als belofte.** Op het klembord staat een belofte met de markeringen tegen
//!   klembordbeheerders (`mondig_mac::VERBERG_TYPEN`). Vraagt een programma de tekst op nadat
//!   ⌘V verstuurd is, dan is dat het leesbewijs; daarna wordt het oude klembord teruggezet, net
//!   als op Windows. Een lezing van vóór ⌘V (een klembordbeheerder) telt niet.
//! - **Eén plakactie tegelijk.** Beginnen (bewaren, publiceren, ⌘V) en afronden lopen allebei
//!   onder het slot `beginnen`, in dezelfde volgorde (eerst `beginnen`, dan `lopend`). Afronden
//!   zelf is één stap op de hoofdthread (`Bord::herstel`): de controle of het klembord nog van
//!   deze plakactie is en het terugzetten kunnen niet uit elkaar vallen.
//! - **Nooit de hoofdthread laten wachten.** De belofte wordt op de hoofdthread beantwoord, en
//!   een plakactie die `beginnen` vasthoudt, wacht soms op de hoofdthread. Wie op de
//!   hoofdthread draait (afsluiten), wacht daarom maar kort op `beginnen` en gaat anders door:
//!   het afronden blijft ook dan één veilige stap.
//! - **Terugvallen.** Zonder het recht op toegankelijkheid komt ⌘V niet aan: dan zet Mondig de
//!   tekst op het klembord en zegt het waarom (ADR 0012, "Tekst nooit kwijt"). Beveiligde invoer
//!   geldt systeembreed, ook als een programma op de achtergrond hem aanzet; daarom plakt Mondig
//!   toch, en beslist het leesbewijs. Komt er geen, dan blijft de tekst op het klembord staan.
//! - **Typen als terugval.** Komt er binnen `GRENS_TYPEN` geen leesbewijs, staat beveiligde
//!   invoer niet aan en meldt de toegankelijkheid een bewerkbaar tekstveld met de focus
//!   (`Bord::tekstveld`), dan trekt Mondig de belofte in en laat het klembord leeg in één stap op
//!   de hoofdthread (`Bord::trek_in`), en typt daarna de tekst met
//!   `CGEventKeyboardSetUnicodeString` in blokken van `mondig_mac::TYPBLOK` eenheden. Het typen
//!   stopt als er een ander programma vooraan komt, beveiligde invoer aangaat of de gebruiker
//!   een modifier vasthoudt; de rest staat dan op het klembord, anders komt het oude klembord
//!   terug (`Bord::na_typen`). Een ander venster binnen hetzelfde programma ziet Mondig hier
//!   niet: daarvoor zou het de vensters van andere programma's moeten uitlezen.
//! - **Wie las er?** Op Windows telt alleen een lezing door het doelproces als leesbewijs. Op de
//!   Mac zegt `pasteboard:provideDataForType:` niet wie leest; een klembordbeheerder die na ⌘V
//!   leest, telt hier dus als leesbewijs (dan typt Mondig niet, dus nooit dubbel).
//!
//! Dit bestand wordt overal gecompileerd, zodat de tests de volgorde op Windows nalopen met een
//! nep-systeem. Alleen op de Mac gebruikt de app het, met `mondig_mac::klembord::Systeem`.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::{Duration, Instant};

use mondig_mac::{Afsluiting, Bord, Herstel, Intrekking, Kopie, Signalen, Voorgrond};

use super::{
    Afloop, Afweging, PlakUitkomst, Stopreden, Transactiestand, Typafloop, Typbesluit,
    VorigKlembord, afweging, met_spatie, rest_van, typ_in_blokken, typbesluit, typstukken,
    uitkomst_na_typen, vorig_na_terugzetten, wacht_op_losse_modifiers,
};

/// De pauze tussen twee typblokken, zodat het doel de vorige invoer verwerkt.
const TYPPAUZE: Duration = Duration::from_millis(8);

/// Tijdbudget voor het bewaren van het oude klembord, zoals op Windows. Wat erover gaat, wordt
/// niet bewaard: plakken gaat voor.
const BEWAAR_BUDGET: Duration = Duration::from_millis(300);

/// Om de zoveel tijd kijkt de wachter of de plakactie af kan ronden.
const WACHT_STAP: Duration = Duration::from_millis(15);

/// Zo lang wacht de hoofdthread hooguit op een plakactie die nog bezig is (bijvoorbeeld ⌘V
/// tijdens het afsluiten). Langer kan niet: die plakactie wacht misschien zelf op de
/// hoofdthread.
const HOOFDTHREAD_WACHT: Duration = Duration::from_millis(300);

/// Zo lang wachten we hooguit tot het programma van vóór de klik op de pil weer vooraan staat.
const VOORGROND_WACHT: Duration = Duration::from_millis(300);

// ---------------------------------------------------------------------------
// De beslissingen
// ---------------------------------------------------------------------------

/// Hoe het dictaat op de Mac bij de gebruiker komt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MacPlan {
    /// Plakken met ⌘V in het programma vooraan.
    Plakken,
    /// Niet plakken: de tekst op het klembord, met deze uitkomst als uitleg.
    Klembord(PlakUitkomst),
    /// Niets doen: Mondig staat vooraan en toont de tekst al.
    Overslaan,
    /// Niet plakken, wel melden.
    Niet(PlakUitkomst),
}

/// Kiest wat er met een gelukt dictaat gebeurt. De terugval van de gebruiker gaat voor alles;
/// dan het programma vooraan, en pas daarna het recht (in Mondig zelf is dat niet nodig).
/// Beveiligde invoer telt hier niet mee: die is systeembreed (zie de uitleg bovenaan).
pub(crate) fn kies_plan(
    alleen_klembord: bool,
    voorgrond: Voorgrond,
    toegankelijk: bool,
) -> MacPlan {
    if alleen_klembord {
        return MacPlan::Klembord(PlakUitkomst::OpKlembord);
    }
    match voorgrond {
        Voorgrond::Geen => MacPlan::Niet(PlakUitkomst::GeenVenster),
        Voorgrond::Eigen => MacPlan::Overslaan,
        Voorgrond::Ander if !toegankelijk => MacPlan::Klembord(PlakUitkomst::GeenToegang),
        Voorgrond::Ander => MacPlan::Plakken,
    }
}

/// Wat er bij het afronden met een klembord gebeurt dat nog van ons is. Kwam er na ⌘V niets
/// gelezen, dan is ⌘V waarschijnlijk niet aangekomen: de tekst blijft dan staan, zodat de
/// gebruiker hem zelf plakt. Niet als een volgende actie de plakactie afbrak (een nieuw
/// dictaat, Kopiëren, afsluiten): dan was er geen tijd om te lezen, en hoort het oude klembord
/// terug.
pub(crate) fn afsluiting_voor(uitkomst: PlakUitkomst, afgebroken: bool) -> Afsluiting {
    match uitkomst {
        PlakUitkomst::NietGelezen | PlakUitkomst::NietGeplakt if !afgebroken => {
            Afsluiting::TekstLaten
        }
        _ => Afsluiting::Terugzetten,
    }
}

/// De afloop voor de interface. Bleef de tekst staan omdat er niets gelezen werd terwijl
/// beveiligde invoer aanstond, dan noemen we dat als mogelijke oorzaak.
pub(crate) fn afloop_van(
    uitkomst: PlakUitkomst,
    afsluiting: Afsluiting,
    herstel: Herstel,
    beveiligd: bool,
    bewaard_volledig: bool,
) -> Afloop {
    match herstel {
        Herstel::NietVanOns => Afloop {
            uitkomst,
            vorig_klembord: VorigKlembord::Bewaard,
        },
        Herstel::Teruggezet => Afloop {
            uitkomst,
            vorig_klembord: vorig_na_terugzetten(bewaard_volledig, true),
        },
        Herstel::TekstGezet => Afloop {
            uitkomst: if afsluiting == Afsluiting::TekstLaten && beveiligd {
                PlakUitkomst::Wachtwoordveld
            } else {
                uitkomst
            },
            vorig_klembord: VorigKlembord::Kwijt,
        },
    }
}

// ---------------------------------------------------------------------------
// Een plakactie
// ---------------------------------------------------------------------------

type Melder = Arc<dyn Fn(Afloop) + Send + Sync>;

/// Een plakactie die nog wacht op het leesbewijs.
struct Transactie {
    /// Om de wachter van deze plakactie te herkennen.
    id: u64,
    /// `doel_onzeker` staat aan als beveiligde invoer aanstond: dan geldt de korte grens.
    stand: Transactiestand,
    signalen: Arc<Signalen>,
    /// Het wijzigingsnummer na het publiceren.
    nummer: isize,
    bewaard: Kopie,
    /// Voor als het oude klembord niet terug kan, of er niets gelezen werd, en om te typen.
    tekst: String,
    /// Het programma dat bij ⌘V vooraan stond.
    doel: Option<i32>,
    melder: Melder,
}

/// Plakken op de Mac, met één lopende plakactie tegelijk.
pub(crate) struct Plakker<B: Bord> {
    bord: B,
    lopend: Mutex<Option<Transactie>>,
    /// Beginnen en afronden van plakacties na elkaar. Altijd vóór `lopend` pakken.
    beginnen: Mutex<()>,
    volgnummer: AtomicU64,
}

impl<B: Bord + 'static> Plakker<B> {
    pub(crate) const fn nieuw(bord: B) -> Self {
        Self {
            bord,
            lopend: Mutex::new(None),
            beginnen: Mutex::new(()),
            volgnummer: AtomicU64::new(0),
        }
    }

    /// Het slot `beginnen`. Buiten de hoofdthread wachtend; op de hoofdthread hooguit
    /// [`HOOFDTHREAD_WACHT`], en anders `None` (doorgaan zonder slot).
    fn slot(&self) -> Option<MutexGuard<'_, ()>> {
        if !self.bord.op_hoofdthread() {
            return Some(self.beginnen.lock().unwrap_or_else(PoisonError::into_inner));
        }
        let begin = Instant::now();
        loop {
            match self.beginnen.try_lock() {
                Ok(slot) => return Some(slot),
                Err(TryLockError::Poisoned(fout)) => return Some(fout.into_inner()),
                Err(TryLockError::WouldBlock) if begin.elapsed() >= HOOFDTHREAD_WACHT => {
                    return None;
                }
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }

    /// Neemt de lopende plakactie over (met `id`: alleen als het die is).
    fn neem(&self, id: Option<u64>) -> Option<Transactie> {
        let mut lopend = self.lopend.lock().unwrap_or_else(PoisonError::into_inner);
        if id.is_some_and(|id| lopend.as_ref().map(|t| t.id) != Some(id)) {
            return None;
        }
        lopend.take()
    }

    /// Rondt een plakactie af in één stap ([`Bord::herstel`]) en meldt de afloop. Met
    /// `afgebroken` rondt een volgende actie hem af, niet de wachter.
    fn rond_af(&self, mut transactie: Transactie, afgebroken: bool) {
        let stand = &mut transactie.stand;
        stand.leesbewijzen = transactie.signalen.leesbewijzen();
        stand.eigendom_kwijt = transactie.signalen.eigendom_kwijt();
        stand.afgebroken = afgebroken;
        let uitkomst = stand.uitkomst();
        let afsluiting = afsluiting_voor(uitkomst, afgebroken);
        let herstel = self.bord.herstel(
            &transactie.signalen,
            transactie.nummer,
            afsluiting,
            &transactie.bewaard,
            &transactie.tekst,
        );
        (transactie.melder)(afloop_van(
            uitkomst,
            afsluiting,
            herstel,
            transactie.stand.doel_onzeker,
            transactie.bewaard.volledig,
        ));
    }

    /// Rondt de lopende plakactie af, als die er is. Wie dit aanroept, houdt `beginnen` vast
    /// (of kreeg het op de hoofdthread niet).
    fn rond_lopend_af(&self) {
        if let Some(transactie) = self.neem(None) {
            self.rond_af(transactie, true);
        }
    }

    /// Het programma vooraan. Staat Mondig vooraan omdat er op de pil geklikt is, dan eerst
    /// het programma van daarvoor terug en daarop wachten (kort).
    fn voorgrond(&self) -> Voorgrond {
        let voorgrond = self.bord.voorgrond();
        if voorgrond != Voorgrond::Eigen || !self.bord.herstel_voorgrond() {
            return voorgrond;
        }
        let begin = Instant::now();
        loop {
            let nu = self.bord.voorgrond();
            if nu != Voorgrond::Eigen || begin.elapsed() >= VOORGROND_WACHT {
                return nu;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Zie [`super::na_dictaat`]. `meld` krijgt altijd precies één afloop.
    pub(crate) fn na_dictaat(
        &'static self,
        tekst: &str,
        alleen_klembord: bool,
        meld: impl Fn(Afloop) + Send + Sync + 'static,
    ) {
        let tekst = met_spatie(tekst);
        if tekst.is_empty() {
            meld(PlakUitkomst::InMondig.into());
            return;
        }
        let kies = |alleen_klembord| {
            kies_plan(alleen_klembord, self.voorgrond(), self.bord.toegankelijk())
        };
        let mut plan = kies(alleen_klembord);
        if plan == MacPlan::Plakken {
            // Eerst wachten tot de toetsen los zijn; daarna opnieuw kijken, want intussen kan
            // er een ander programma vooraan staan.
            if !wacht_op_losse_modifiers(|| self.bord.modifier_ingedrukt()) {
                meld(PlakUitkomst::ToetsenIngedrukt.into());
                return;
            }
            plan = kies(false);
        }
        match plan {
            MacPlan::Overslaan => meld(PlakUitkomst::InMondig.into()),
            MacPlan::Niet(uitkomst) => meld(uitkomst.into()),
            MacPlan::Klembord(uitkomst) => meld(
                if self.zet_blijvend(&tekst) {
                    uitkomst
                } else {
                    PlakUitkomst::Mislukt
                }
                .into(),
            ),
            MacPlan::Plakken => self.plak(tekst, Arc::new(meld)),
        }
    }

    /// Bewaren, publiceren, ⌘V, en dan een wachter die afrondt.
    fn plak(&'static self, tekst: String, melder: Melder) {
        let slot = self.beginnen.lock().unwrap_or_else(PoisonError::into_inner);
        // Eerst de vorige plakactie, zodat het bewaarde klembord dat van de gebruiker is.
        self.rond_lopend_af();
        let bewaard = self.bord.bewaar(BEWAAR_BUDGET);
        let signalen = Arc::new(Signalen::default());
        let Some(nummer) = self.bord.publiceer(&tekst, Arc::clone(&signalen)) else {
            // Het klembord kan al leeg zijn: het oude terug, en anders het dictaat erop.
            let vorig = if self.bord.zet_terug(&bewaard) {
                VorigKlembord::Bewaard
            } else {
                let _ = self.bord.zet_tekst(&tekst);
                VorigKlembord::Kwijt
            };
            melder(Afloop {
                uitkomst: PlakUitkomst::Mislukt,
                vorig_klembord: vorig,
            });
            return;
        };
        // Alleen als mogelijke uitleg: met beveiligde invoer komt ⌘V misschien niet aan, dus
        // dan wachten we maar kort op het leesbewijs (`GRENS_ONZEKER_DOEL`).
        let beveiligd = self.bord.beveiligde_invoer();
        let mut stand = Transactiestand::nieuw(Instant::now(), beveiligd);
        let doel = self.bord.voorgrond_id();
        if let Some(verstuurd) = self.bord.stuur_cmd_v() {
            // Pas lezingen vanaf het versturen van V tellen als leesbewijs.
            stand.ingevoegd = Some(verstuurd);
        } else {
            stand.ingevoegd = Some(Instant::now());
            stand.invoeren_mislukt = true;
        }
        let id = self.volgnummer.fetch_add(1, Ordering::SeqCst);
        *self.lopend.lock().unwrap_or_else(PoisonError::into_inner) = Some(Transactie {
            id,
            stand,
            signalen,
            nummer,
            bewaard,
            tekst,
            doel,
            melder,
        });
        drop(slot);
        std::thread::spawn(move || self.wacht(id));
    }

    /// De wachter: rondt de plakactie `id` af zodra [`afweging`] dat zegt, onder `beginnen`.
    /// Heeft een ander de plakactie al afgerond (een volgend dictaat, Kopiëren, afsluiten), dan
    /// stopt hij.
    fn wacht(&self, id: u64) {
        loop {
            std::thread::sleep(WACHT_STAP);
            let besluit = {
                let mut lopend = self.lopend.lock().unwrap_or_else(PoisonError::into_inner);
                let Some(transactie) = lopend.as_mut().filter(|t| t.id == id) else {
                    return;
                };
                transactie.stand.leesbewijzen = transactie.signalen.leesbewijzen();
                transactie.stand.eigendom_kwijt = transactie.signalen.eigendom_kwijt();
                afweging(&transactie.stand, Instant::now())
            };
            if besluit == Afweging::Wachten {
                continue;
            }
            // Zelfde volgorde als `plak`: eerst `beginnen`, dan `lopend`. Een plakactie die
            // intussen begon, heeft deze dan al afgerond.
            let _slot = self.beginnen.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(transactie) = self.neem(Some(id)) else {
                return;
            };
            if besluit == Afweging::Afronden {
                self.rond_af(transactie, false);
                return;
            }
            // Typen. Kwam er net toch een lezing, dan gaat de plakactie terug en wacht deze
            // wachter verder, als gewoon plakken.
            match self.typ_terugval(transactie) {
                Some(terug) => {
                    *self.lopend.lock().unwrap_or_else(PoisonError::into_inner) = Some(terug);
                }
                None => return,
            }
        }
    }

    /// Geen leesbewijs binnen de grens: alleen in een tekstveld de belofte intrekken en de tekst
    /// typen (zie de uitleg bovenaan en in `plakken.rs`). Geeft de plakactie terug als het doel
    /// toch nog las; dan is het gewoon plakken. Wie dit aanroept, houdt `beginnen` vast.
    ///
    /// Of het doel hangt, is op de Mac niet goed na te gaan; een programma dat hangt, antwoordt
    /// ook de toegankelijkheid niet op tijd, en dan typt Mondig niet (twijfel).
    fn typ_terugval(&self, mut transactie: Transactie) -> Option<Transactie> {
        if self.bord.beveiligde_invoer() {
            // Beveiligde invoer staat nu aan: getypte tekens komen niet aan. Dezelfde laatste
            // terugval als bij de start: de tekst op het klembord, met de uitleg.
            transactie.stand.doel_onzeker = true;
            self.rond_af(transactie, false);
            return None;
        }
        if typbesluit(true, self.bord.tekstveld()) != Typbesluit::Typen {
            // Geen tekstveld: getypte letters zouden sneltoetsen worden. De tekst op het klembord.
            let herstel = self.bord.herstel(
                &transactie.signalen,
                transactie.nummer,
                Afsluiting::TekstLaten,
                &transactie.bewaard,
                &transactie.tekst,
            );
            (transactie.melder)(if herstel == Herstel::NietVanOns {
                // De gebruiker kopieerde intussen zelf: dat blijft staan.
                PlakUitkomst::NietGelezen.into()
            } else {
                Afloop {
                    uitkomst: PlakUitkomst::GeenTekstveld,
                    vorig_klembord: VorigKlembord::Kwijt,
                }
            });
            return None;
        }
        let sinds = transactie
            .stand
            .ingevoegd
            .unwrap_or(transactie.stand.gepubliceerd);
        let leeg = match self
            .bord
            .trek_in(&transactie.signalen, transactie.nummer, sinds)
        {
            Intrekking::Gelezen => return Some(transactie),
            Intrekking::NietVanOns => {
                self.rond_af(transactie, false);
                return None;
            }
            Intrekking::Ingetrokken { nummer } => nummer,
        };
        // Het klembord is nu leeg: een laat ⌘V krijgt niets.
        let stukken = typstukken(&transactie.tekst, mondig_mac::TYPBLOK);
        let afloop = typ_in_blokken(
            &stukken,
            || {
                if !wacht_op_losse_modifiers(|| self.bord.modifier_ingedrukt()) {
                    Some(Stopreden::Toetsen)
                } else if self.bord.beveiligde_invoer() {
                    Some(Stopreden::Beveiligd)
                } else if self.bord.voorgrond_id() != transactie.doel {
                    Some(Stopreden::AnderVenster)
                } else {
                    None
                }
            },
            |stuk| self.bord.typ_tekst(stuk),
            || std::thread::sleep(TYPPAUZE),
        );
        let mut uitkomst = uitkomst_na_typen(afloop);
        let rest = match afloop {
            Typafloop::Klaar => None,
            Typafloop::Gestopt { bij, .. } => Some(rest_van(&stukken[bij..])),
        };
        // Na het typen: de rest op het klembord, of anders het oude klembord terug. Kopieerde de
        // gebruiker intussen zelf (ook met Kopiëren in Mondig vanaf de hoofdthread), dan blijft
        // dat staan.
        let vorig_klembord = match self
            .bord
            .na_typen(leeg, &transactie.bewaard, rest.as_deref())
        {
            None => VorigKlembord::Bewaard,
            Some(true) if rest.is_none() => vorig_na_terugzetten(transactie.bewaard.volledig, true),
            Some(gelukt) => {
                if rest.is_some() && !gelukt {
                    uitkomst = PlakUitkomst::Mislukt;
                }
                VorigKlembord::Kwijt
            }
        };
        (transactie.melder)(Afloop {
            uitkomst,
            vorig_klembord,
        });
        None
    }

    /// Zet tekst meteen en blijvend op het klembord, buiten klembordbeheerders. Een lopende
    /// plakactie wordt eerst afgerond. Mag ook op de hoofdthread.
    pub(crate) fn zet_blijvend(&self, tekst: &str) -> bool {
        let _slot = self.slot();
        self.rond_lopend_af();
        self.bord.zet_tekst(tekst)
    }

    /// Bij afsluiten: de lopende plakactie afronden, zodat het oude klembord terugkomt. Loopt
    /// ⌘V nog, dan wacht dit daar kort op.
    pub(crate) fn afsluiten(&self) {
        let _slot = self.slot();
        self.rond_lopend_af();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::mpsc;

    use super::super::GRENS_TYPEN;
    use super::*;

    #[test]
    fn plannen() {
        use PlakUitkomst::{GeenToegang, GeenVenster, OpKlembord};
        assert_eq!(kies_plan(false, Voorgrond::Ander, true), MacPlan::Plakken);
        assert_eq!(
            kies_plan(true, Voorgrond::Ander, true),
            MacPlan::Klembord(OpKlembord)
        );
        // De terugval van de gebruiker gaat voor alles, ook zonder recht.
        assert_eq!(
            kies_plan(true, Voorgrond::Eigen, false),
            MacPlan::Klembord(OpKlembord)
        );
        assert_eq!(
            kies_plan(false, Voorgrond::Ander, false),
            MacPlan::Klembord(GeenToegang)
        );
        // In Mondig zelf is het recht niet nodig: de tekst staat al in beeld.
        assert_eq!(
            kies_plan(false, Voorgrond::Eigen, false),
            MacPlan::Overslaan
        );
        assert_eq!(
            kies_plan(false, Voorgrond::Geen, true),
            MacPlan::Niet(GeenVenster)
        );
    }

    #[test]
    fn zonder_lezing_blijft_de_tekst_staan_behalve_na_afbreken() {
        use PlakUitkomst::{Geplakt, Mislukt, NietGelezen, NietGeplakt, WaarschijnlijkGeplakt};
        assert_eq!(afsluiting_voor(NietGelezen, false), Afsluiting::TekstLaten);
        assert_eq!(afsluiting_voor(NietGeplakt, false), Afsluiting::TekstLaten);
        assert_eq!(afsluiting_voor(NietGelezen, true), Afsluiting::Terugzetten);
        for uitkomst in [Geplakt, WaarschijnlijkGeplakt, Mislukt] {
            assert_eq!(afsluiting_voor(uitkomst, false), Afsluiting::Terugzetten);
        }
    }

    #[test]
    fn beveiligde_invoer_alleen_als_mogelijke_oorzaak() {
        use PlakUitkomst::{NietGeplakt, Wachtwoordveld};
        let laten = Afsluiting::TekstLaten;
        assert_eq!(
            afloop_van(NietGeplakt, laten, Herstel::TekstGezet, true, true).uitkomst,
            Wachtwoordveld
        );
        // Zonder beveiligde invoer, of als de gebruiker intussen kopieerde: geen wachtwoordveld.
        assert_eq!(
            afloop_van(NietGeplakt, laten, Herstel::TekstGezet, false, true).uitkomst,
            NietGeplakt
        );
        assert_eq!(
            afloop_van(NietGeplakt, laten, Herstel::NietVanOns, true, true),
            Afloop {
                uitkomst: NietGeplakt,
                vorig_klembord: VorigKlembord::Bewaard
            }
        );
        assert_eq!(
            afloop_van(
                PlakUitkomst::Geplakt,
                Afsluiting::Terugzetten,
                Herstel::Teruggezet,
                false,
                false
            )
            .vorig_klembord,
            VorigKlembord::Onvolledig
        );
    }

    /// Wat het nep-systeem deed, in volgorde.
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Stap {
        Bewaar,
        Publiceer(String),
        CmdV,
        ZetTerug(Kopie),
        ZetTekst(String),
        /// `herstel` liet het klembord ongemoeid (niet meer van deze plakactie).
        NietVanOns,
        HerstelVoorgrond,
        /// De belofte is ingetrokken en het klembord is leeg.
        TrekIn,
        Typ(String),
    }

    /// Een nep-klembord. `lezen` laat het doel na ⌘V lezen, `vooraf_lezen` een
    /// klembordbeheerder vóór ⌘V; `wijzigen` laat de gebruiker intussen zelf kopiëren (het
    /// wijzigingsnummer schuift op). `voorgronden` geeft de antwoorden van `voorgrond` op
    /// volgorde; het laatste blijft.
    #[expect(
        clippy::struct_excessive_bools,
        reason = "losse knoppen voor de testgevallen; geen toestandsmachine"
    )]
    struct Nep {
        voorgronden: Mutex<VecDeque<Voorgrond>>,
        herstelt_voorgrond: bool,
        toegankelijk: bool,
        beveiligd: bool,
        hoofdthread: bool,
        cmd_v_lukt: bool,
        lezen: bool,
        vooraf_lezen: bool,
        wijzigen: bool,
        /// Beveiligde invoer gaat aan na ⌘V (een wachtwoordveld kreeg de focus).
        beveiligd_na_cmd_v: bool,
        /// Het doel leest precies op het moment dat Mondig wil intrekken.
        lezen_bij_intrekken: bool,
        typen_lukt: bool,
        /// Wat `tekstveld` antwoordt.
        tekstveld: Option<bool>,
        /// De gebruiker kopieert tijdens het typen.
        kopieren_tijdens_typen: bool,
        /// De antwoorden van `voorgrond_id` op volgorde; het laatste blijft.
        voorgrond_ids: Mutex<VecDeque<Option<i32>>>,
        cmd_v_verstuurd: Mutex<bool>,
        stappen: Mutex<Vec<Stap>>,
        signalen: Mutex<Option<Arc<Signalen>>>,
        nummer: Mutex<isize>,
    }

    impl Nep {
        fn nieuw() -> Self {
            Self {
                voorgronden: Mutex::new(VecDeque::from([Voorgrond::Ander])),
                herstelt_voorgrond: false,
                toegankelijk: true,
                beveiligd: false,
                hoofdthread: false,
                cmd_v_lukt: true,
                lezen: true,
                vooraf_lezen: false,
                wijzigen: false,
                beveiligd_na_cmd_v: false,
                lezen_bij_intrekken: false,
                typen_lukt: true,
                tekstveld: Some(true),
                kopieren_tijdens_typen: false,
                voorgrond_ids: Mutex::new(VecDeque::from([Some(1)])),
                cmd_v_verstuurd: Mutex::new(false),
                stappen: Mutex::new(Vec::new()),
                signalen: Mutex::new(None),
                nummer: Mutex::new(1),
            }
        }

        /// Een doel dat niet leest: Mondig gaat typen.
        fn zonder_lezing() -> Self {
            let mut nep = Self::nieuw();
            nep.lezen = false;
            nep
        }

        fn met_voorgrond_ids(mut self, ids: &[Option<i32>]) -> Self {
            self.voorgrond_ids = Mutex::new(ids.iter().copied().collect());
            self
        }

        fn met_voorgronden(mut self, voorgronden: &[Voorgrond]) -> Self {
            self.voorgronden = Mutex::new(voorgronden.iter().copied().collect());
            self
        }

        fn stap(&self, stap: Stap) {
            self.stappen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(stap);
        }

        fn stappen(&self) -> Vec<Stap> {
            self.stappen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }

        fn volgend_nummer(&self) -> isize {
            let mut nummer = self.nummer.lock().unwrap_or_else(PoisonError::into_inner);
            *nummer += 1;
            *nummer
        }

        fn huidig_nummer(&self) -> isize {
            *self.nummer.lock().unwrap_or_else(PoisonError::into_inner)
        }

        fn huidige_signalen(&self) -> Option<Arc<Signalen>> {
            self.signalen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    fn oud() -> Kopie {
        Kopie {
            items: vec![vec![("public.utf8-plain-text".into(), b"oud".to_vec())]],
            volledig: true,
        }
    }

    impl Bord for Nep {
        fn voorgrond(&self) -> Voorgrond {
            let mut voorgronden = self
                .voorgronden
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if voorgronden.len() > 1 {
                voorgronden.pop_front().unwrap_or(Voorgrond::Geen)
            } else {
                voorgronden.front().copied().unwrap_or(Voorgrond::Geen)
            }
        }
        fn herstel_voorgrond(&self) -> bool {
            self.stap(Stap::HerstelVoorgrond);
            self.herstelt_voorgrond
        }
        fn toegankelijk(&self) -> bool {
            self.toegankelijk
        }
        fn beveiligde_invoer(&self) -> bool {
            self.beveiligd
                || (self.beveiligd_na_cmd_v
                    && *self
                        .cmd_v_verstuurd
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner))
        }
        fn modifier_ingedrukt(&self) -> bool {
            false
        }
        fn op_hoofdthread(&self) -> bool {
            self.hoofdthread
        }
        fn bewaar(&self, _budget: Duration) -> Kopie {
            self.stap(Stap::Bewaar);
            oud()
        }
        fn publiceer(&self, tekst: &str, signalen: Arc<Signalen>) -> Option<isize> {
            self.stap(Stap::Publiceer(tekst.to_string()));
            if self.vooraf_lezen {
                signalen.lees(Instant::now());
            }
            *self.signalen.lock().unwrap_or_else(PoisonError::into_inner) = Some(signalen);
            Some(self.volgend_nummer())
        }
        fn stuur_cmd_v(&self) -> Option<Instant> {
            self.stap(Stap::CmdV);
            *self
                .cmd_v_verstuurd
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = true;
            // Even later dan publiceren, zodat een lezing vooraf echt eerder ligt.
            std::thread::sleep(Duration::from_millis(2));
            let verstuurd = Instant::now();
            if let Some(signalen) = self.huidige_signalen() {
                if self.lezen {
                    signalen.lees(Instant::now());
                }
                if self.wijzigen {
                    self.volgend_nummer();
                    signalen.verlies_eigendom();
                }
            }
            self.cmd_v_lukt.then_some(verstuurd)
        }
        fn zet_terug(&self, kopie: &Kopie) -> bool {
            self.stap(Stap::ZetTerug(kopie.clone()));
            self.volgend_nummer();
            true
        }
        fn zet_tekst(&self, tekst: &str) -> bool {
            self.stap(Stap::ZetTekst(tekst.to_string()));
            self.volgend_nummer();
            true
        }
        fn herstel(
            &self,
            signalen: &Arc<Signalen>,
            nummer: isize,
            afsluiting: Afsluiting,
            kopie: &Kopie,
            tekst: &str,
        ) -> Herstel {
            if signalen.eigendom_kwijt() || self.huidig_nummer() != nummer {
                self.stap(Stap::NietVanOns);
                return Herstel::NietVanOns;
            }
            match afsluiting {
                Afsluiting::Terugzetten => {
                    let _ = self.zet_terug(kopie);
                    Herstel::Teruggezet
                }
                Afsluiting::TekstLaten => {
                    let _ = self.zet_tekst(tekst);
                    Herstel::TekstGezet
                }
            }
        }
        fn voorgrond_id(&self) -> Option<i32> {
            let mut ids = self
                .voorgrond_ids
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if ids.len() > 1 {
                ids.pop_front().flatten()
            } else {
                ids.front().copied().flatten()
            }
        }
        fn tekstveld(&self) -> Option<bool> {
            self.tekstveld
        }
        fn trek_in(&self, signalen: &Arc<Signalen>, nummer: isize, sinds: Instant) -> Intrekking {
            if self.lezen_bij_intrekken {
                signalen.lees(Instant::now());
            }
            if signalen.leesbewijzen().iter().any(|m| *m >= sinds) {
                return Intrekking::Gelezen;
            }
            if signalen.eigendom_kwijt() || self.huidig_nummer() != nummer {
                return Intrekking::NietVanOns;
            }
            signalen.trek_in();
            self.stap(Stap::TrekIn);
            Intrekking::Ingetrokken {
                nummer: self.volgend_nummer(),
            }
        }
        fn na_typen(&self, nummer: isize, kopie: &Kopie, rest: Option<&str>) -> Option<bool> {
            if self.huidig_nummer() != nummer {
                self.stap(Stap::NietVanOns);
                return None;
            }
            Some(match rest {
                Some(rest) => self.zet_tekst(rest),
                None => self.zet_terug(kopie),
            })
        }
        fn typ_tekst(&self, tekst: &str) -> bool {
            assert!(tekst.encode_utf16().count() <= mondig_mac::TYPBLOK);
            if self.kopieren_tijdens_typen {
                // De gebruiker kopieert iets terwijl Mondig typt.
                self.volgend_nummer();
            }
            if self.typen_lukt {
                self.stap(Stap::Typ(tekst.to_string()));
            }
            self.typen_lukt
        }
    }

    /// Een plakker die zo lang leeft als de test (de wachter heeft `'static` nodig).
    fn plakker(nep: Nep) -> &'static Plakker<Nep> {
        Box::leak(Box::new(Plakker::nieuw(nep)))
    }

    /// Start een plakactie; de afloop komt in de ontvanger.
    fn start(plakker: &'static Plakker<Nep>, tekst: &str) -> mpsc::Receiver<Afloop> {
        let (zender, ontvanger) = mpsc::channel();
        plakker.na_dictaat(tekst, false, move |afloop| {
            let _ = zender.send(afloop);
        });
        ontvanger
    }

    fn ontvang(ontvanger: &mpsc::Receiver<Afloop>, grens: Duration) -> Option<Afloop> {
        ontvanger.recv_timeout(grens).ok()
    }

    /// Plakt en wacht op de afloop.
    fn plak(plakker: &'static Plakker<Nep>, tekst: &str) -> Afloop {
        ontvang(&start(plakker, tekst), Duration::from_secs(5))
            .unwrap_or_else(|| PlakUitkomst::Mislukt.into())
    }

    #[test]
    fn plakken_met_leesbewijs_zet_het_oude_klembord_terug() {
        let plakker = plakker(Nep::nieuw());
        let afloop = plak(plakker, "Hallo");
        assert_eq!(afloop.uitkomst, PlakUitkomst::Geplakt);
        assert_eq!(afloop.vorig_klembord, VorigKlembord::Bewaard);
        assert_eq!(
            plakker.bord.stappen(),
            vec![
                Stap::Bewaar,
                Stap::Publiceer("Hallo ".into()),
                Stap::CmdV,
                Stap::ZetTerug(oud()),
            ]
        );
    }

    #[test]
    fn een_lezing_van_voor_cmd_v_is_geen_leesbewijs() {
        // Een klembordbeheerder leest meteen na het publiceren; het doel leest niet. De
        // gebruiker kopieert daarna zelf, zodat de afloop snel komt.
        let mut nep = Nep::nieuw();
        nep.vooraf_lezen = true;
        nep.lezen = false;
        nep.wijzigen = true;
        let plakker = plakker(nep);
        assert_eq!(
            plak(plakker, "Hallo").uitkomst,
            PlakUitkomst::WaarschijnlijkGeplakt
        );
    }

    #[test]
    fn kopieert_de_gebruiker_intussen_dan_wint_dat() {
        let mut nep = Nep::nieuw();
        nep.lezen = false;
        nep.wijzigen = true;
        let plakker = plakker(nep);
        let afloop = plak(plakker, "Hallo");
        // Geen leesbewijs en het klembord is niet meer van ons: niets terugzetten.
        assert_eq!(afloop.uitkomst, PlakUitkomst::NietGelezen);
        assert_eq!(afloop.vorig_klembord, VorigKlembord::Bewaard);
        let stappen = plakker.bord.stappen();
        assert_eq!(stappen.last(), Some(&Stap::NietVanOns));
        assert!(!stappen.iter().any(|s| matches!(s, Stap::ZetTerug(_))));
    }

    #[test]
    fn mislukte_cmd_v_zet_snel_terug() {
        let mut nep = Nep::nieuw();
        nep.lezen = false;
        nep.cmd_v_lukt = false;
        let plakker = plakker(nep);
        let begin = Instant::now();
        let afloop = plak(plakker, "Hallo");
        assert_eq!(afloop.uitkomst, PlakUitkomst::Mislukt);
        assert!(begin.elapsed() < Duration::from_secs(3));
        assert_eq!(plakker.bord.stappen().last(), Some(&Stap::ZetTerug(oud())));
    }

    #[test]
    fn zonder_toegang_de_tekst_op_het_klembord_met_uitleg() {
        let mut nep = Nep::nieuw();
        nep.toegankelijk = false;
        let plakker = plakker(nep);
        let afloop = plak(plakker, "Hallo");
        assert_eq!(afloop.uitkomst, PlakUitkomst::GeenToegang);
        assert_eq!(
            plakker.bord.stappen(),
            vec![Stap::ZetTekst("Hallo ".into())]
        );
    }

    #[test]
    fn beveiligde_invoer_houdt_plakken_niet_tegen() {
        // Een programma op de achtergrond heeft beveiligde invoer aan, het doel leest gewoon.
        let mut nep = Nep::nieuw();
        nep.beveiligd = true;
        let plakker = plakker(nep);
        assert_eq!(plak(plakker, "Hallo").uitkomst, PlakUitkomst::Geplakt);
        assert_eq!(plakker.bord.stappen().last(), Some(&Stap::ZetTerug(oud())));
    }

    #[test]
    fn beveiligde_invoer_zonder_lezing_laat_de_tekst_staan() {
        let mut nep = Nep::nieuw();
        nep.beveiligd = true;
        nep.lezen = false;
        let plakker = plakker(nep);
        let begin = Instant::now();
        let afloop = plak(plakker, "Hallo");
        // De korte grens van een seconde, niet acht.
        assert!(begin.elapsed() < Duration::from_secs(3));
        assert_eq!(afloop.uitkomst, PlakUitkomst::Wachtwoordveld);
        assert_eq!(afloop.vorig_klembord, VorigKlembord::Kwijt);
        let stappen = plakker.bord.stappen();
        assert!(stappen.contains(&Stap::CmdV));
        assert_eq!(stappen.last(), Some(&Stap::ZetTekst("Hallo ".into())));
    }

    #[test]
    fn mondig_vooraan_raakt_het_klembord_niet() {
        let plakker = plakker(Nep::nieuw().met_voorgronden(&[Voorgrond::Eigen]));
        assert_eq!(plak(plakker, "Hallo").uitkomst, PlakUitkomst::InMondig);
        assert_eq!(plakker.bord.stappen(), vec![Stap::HerstelVoorgrond]);
    }

    #[test]
    fn na_een_klik_op_de_pil_plakt_het_in_het_programma_van_daarvoor() {
        // Mondig staat vooraan door de klik; na het terugzetten staat het andere programma er.
        let mut nep = Nep::nieuw().met_voorgronden(&[Voorgrond::Eigen, Voorgrond::Ander]);
        nep.herstelt_voorgrond = true;
        let plakker = plakker(nep);
        assert_eq!(plak(plakker, "Hallo").uitkomst, PlakUitkomst::Geplakt);
        let stappen = plakker.bord.stappen();
        assert_eq!(stappen.first(), Some(&Stap::HerstelVoorgrond));
        assert!(stappen.contains(&Stap::CmdV));
    }

    #[test]
    fn kopieren_rondt_een_lopende_plakactie_eerst_af() {
        // Het doel leest niet: de plakactie blijft lopen tot Kopiëren hem afrondt.
        let mut nep = Nep::nieuw();
        nep.lezen = false;
        let plakker = plakker(nep);
        let ontvanger = start(plakker, "Eerste");
        assert!(plakker.zet_blijvend("Kopie"));
        let afloop = ontvang(&ontvanger, Duration::from_secs(1));
        assert_eq!(afloop.map(|a| a.uitkomst), Some(PlakUitkomst::NietGelezen));
        let stappen = plakker.bord.stappen();
        // Afgebroken: het oude klembord terug, niet het dictaat laten staan.
        assert_eq!(
            stappen[stappen.len() - 2..],
            [Stap::ZetTerug(oud()), Stap::ZetTekst("Kopie".into())]
        );
        // De wachter van de eerste plakactie doet daarna niets meer.
        std::thread::sleep(WACHT_STAP * 4);
        assert_eq!(plakker.bord.stappen().len(), stappen.len());
    }

    #[test]
    fn de_wachter_rondt_niet_af_terwijl_een_plakactie_begint() {
        // Het doel leest meteen; de wachter wil dus binnen 200 ms afronden. Zolang een andere
        // plakactie `beginnen` vasthoudt, mag dat niet.
        let plakker = plakker(Nep::nieuw());
        let slot = plakker
            .beginnen
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // `plak` pakt `beginnen` zelf, dus zetten we de plakactie hier met de hand klaar, zoals
        // `plak` dat doet.
        let signalen = Arc::new(Signalen::default());
        let nummer = plakker.bord.publiceer("Hallo ", Arc::clone(&signalen));
        let mut stand = Transactiestand::nieuw(Instant::now(), false);
        stand.ingevoegd = Some(Instant::now());
        signalen.lees(Instant::now());
        let (zender, ontvanger) = mpsc::channel();
        *plakker
            .lopend
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Transactie {
            id: 7,
            stand,
            signalen,
            nummer: nummer.unwrap_or_default(),
            bewaard: oud(),
            tekst: "Hallo ".into(),
            doel: Some(1),
            melder: Arc::new(move |afloop| {
                let _ = zender.send(afloop);
            }),
        });
        std::thread::spawn(move || plakker.wacht(7));
        assert!(ontvang(&ontvanger, Duration::from_millis(500)).is_none());
        drop(slot);
        let afloop = ontvang(&ontvanger, Duration::from_secs(2));
        assert_eq!(afloop.map(|a| a.uitkomst), Some(PlakUitkomst::Geplakt));
        assert_eq!(plakker.bord.stappen().last(), Some(&Stap::ZetTerug(oud())));
    }

    /// Houdt `beginnen` even vast op een andere thread, zoals een plakactie tijdens ⌘V.
    fn houd_slot_vast(plakker: &'static Plakker<Nep>, duur: Duration) -> mpsc::Receiver<()> {
        let (vast, is_vast) = mpsc::channel();
        std::thread::spawn(move || {
            let _slot = plakker
                .beginnen
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let _ = vast.send(());
            std::thread::sleep(duur);
        });
        is_vast
    }

    #[test]
    fn afsluiten_wacht_op_cmd_v() {
        for hoofdthread in [false, true] {
            let mut nep = Nep::nieuw();
            nep.lezen = false;
            nep.hoofdthread = hoofdthread;
            let plakker = plakker(nep);
            let ontvanger = start(plakker, "Hallo");
            let is_vast = houd_slot_vast(plakker, Duration::from_millis(100));
            let _ = is_vast.recv_timeout(Duration::from_secs(1));
            let begin = Instant::now();
            plakker.afsluiten();
            // Het wachtte op het slot en zette daarna het oude klembord terug.
            assert!(
                begin.elapsed() >= Duration::from_millis(50),
                "{hoofdthread}"
            );
            assert!(ontvang(&ontvanger, Duration::from_secs(1)).is_some());
            assert_eq!(plakker.bord.stappen().last(), Some(&Stap::ZetTerug(oud())));
        }
    }

    #[test]
    fn de_hoofdthread_wacht_niet_eindeloos_op_een_plakactie() {
        let mut nep = Nep::nieuw();
        nep.hoofdthread = true;
        let plakker = plakker(nep);
        let is_vast = houd_slot_vast(plakker, Duration::from_secs(2));
        let _ = is_vast.recv_timeout(Duration::from_secs(1));
        let begin = Instant::now();
        assert!(plakker.zet_blijvend("Kopie"));
        assert!(begin.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn zonder_lezing_typt_mondig_met_een_leeg_klembord() {
        let plakker = plakker(Nep::zonder_lezing());
        let begin = Instant::now();
        let afloop = plak(plakker, "Hallo\nwereld");
        assert!(begin.elapsed() >= GRENS_TYPEN, "eerst de grens afwachten");
        assert_eq!(afloop.uitkomst, PlakUitkomst::Getypt);
        assert_eq!(afloop.vorig_klembord, VorigKlembord::Bewaard);
        // Eerst intrekken (het klembord leeg), dan typen, en pas daarna het oude klembord terug:
        // een laat ⌘V krijgt niets, niet het dictaat en niet het oude klembord. Het regeleinde
        // gaat als spatie.
        assert_eq!(
            plakker.bord.stappen(),
            vec![
                Stap::Bewaar,
                Stap::Publiceer("Hallo\nwereld ".into()),
                Stap::CmdV,
                Stap::TrekIn,
                Stap::Typ("Hallo wereld ".into()),
                Stap::ZetTerug(oud()),
            ]
        );
    }

    #[test]
    fn zonder_tekstveld_typt_mondig_niet() {
        for tekstveld in [Some(false), None] {
            let mut nep = Nep::zonder_lezing();
            nep.tekstveld = tekstveld;
            let plakker = plakker(nep);
            let afloop = plak(plakker, "Hallo");
            assert_eq!(
                afloop.uitkomst,
                PlakUitkomst::GeenTekstveld,
                "{tekstveld:?}"
            );
            assert_eq!(afloop.vorig_klembord, VorigKlembord::Kwijt);
            let stappen = plakker.bord.stappen();
            assert!(!stappen.contains(&Stap::TrekIn));
            assert!(!stappen.iter().any(|s| matches!(s, Stap::Typ(_))));
            assert_eq!(stappen.last(), Some(&Stap::ZetTekst("Hallo ".into())));
        }
    }

    #[test]
    fn kopieert_de_gebruiker_tijdens_het_typen_dan_blijft_dat_staan() {
        // Ook als Kopiëren vanaf de hoofdthread zonder slot doorging: na het typen overschrijft
        // Mondig niets.
        let mut nep = Nep::zonder_lezing();
        nep.kopieren_tijdens_typen = true;
        let plakker = plakker(nep);
        let afloop = plak(plakker, "Hallo");
        assert_eq!(afloop.uitkomst, PlakUitkomst::Getypt);
        assert_eq!(afloop.vorig_klembord, VorigKlembord::Bewaard);
        let stappen = plakker.bord.stappen();
        assert_eq!(stappen.last(), Some(&Stap::NietVanOns));
        assert!(!stappen.iter().any(|s| matches!(s, Stap::ZetTerug(_))));
    }

    #[test]
    fn lange_tekst_in_blokken_met_een_heel_surrogaatpaar() {
        let tekst = format!("{}\u{1F600}{}", "a".repeat(15), "b".repeat(20));
        let plakker = plakker(Nep::zonder_lezing());
        assert_eq!(plak(plakker, &tekst).uitkomst, PlakUitkomst::Getypt);
        let getypt: Vec<String> = plakker
            .bord
            .stappen()
            .into_iter()
            .filter_map(|s| match s {
                Stap::Typ(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(getypt.concat(), format!("{tekst} "));
        // De emoji past niet meer achter vijftien letters in een blok van zestien.
        assert_eq!(getypt[0], "a".repeat(15));
        assert!(getypt[1].starts_with('\u{1F600}'));
    }

    #[test]
    fn een_lezing_vlak_voor_het_intrekken_is_gewoon_plakken() {
        let mut nep = Nep::zonder_lezing();
        nep.lezen_bij_intrekken = true;
        let plakker = plakker(nep);
        let afloop = plak(plakker, "Hallo");
        assert_eq!(afloop.uitkomst, PlakUitkomst::Geplakt);
        let stappen = plakker.bord.stappen();
        assert!(!stappen.contains(&Stap::TrekIn));
        assert!(!stappen.iter().any(|s| matches!(s, Stap::Typ(_))));
        assert_eq!(stappen.last(), Some(&Stap::ZetTerug(oud())));
    }

    #[test]
    fn typen_stopt_bij_een_ander_programma_en_de_rest_gaat_op_het_klembord() {
        // Bij ⌘V en het eerste blok staat programma 1 vooraan, daarna programma 2.
        let nep = Nep::zonder_lezing().met_voorgrond_ids(&[Some(1), Some(1), Some(2)]);
        let plakker = plakker(nep);
        let tekst = "Een zin die langer is dan een blok";
        let afloop = plak(plakker, tekst);
        assert_eq!(afloop.uitkomst, PlakUitkomst::TypenOnderbroken);
        assert_eq!(afloop.vorig_klembord, VorigKlembord::Kwijt);
        let stappen = plakker.bord.stappen();
        let eerste = "Een zin die lang";
        assert!(stappen.contains(&Stap::Typ(eerste.into())));
        let rest = format!("{} ", &tekst[eerste.len()..]);
        assert_eq!(stappen.last(), Some(&Stap::ZetTekst(rest)));
    }

    #[test]
    fn beveiligde_invoer_bij_het_besluit_typt_niet() {
        let mut nep = Nep::zonder_lezing();
        nep.beveiligd_na_cmd_v = true;
        let plakker = plakker(nep);
        let afloop = plak(plakker, "Hallo");
        assert_eq!(afloop.uitkomst, PlakUitkomst::Wachtwoordveld);
        let stappen = plakker.bord.stappen();
        assert!(!stappen.contains(&Stap::TrekIn));
        assert!(!stappen.iter().any(|s| matches!(s, Stap::Typ(_))));
        assert_eq!(stappen.last(), Some(&Stap::ZetTekst("Hallo ".into())));
    }

    #[test]
    fn lukt_typen_niet_dan_de_tekst_op_het_klembord() {
        let mut nep = Nep::zonder_lezing();
        nep.typen_lukt = false;
        let plakker = plakker(nep);
        let afloop = plak(plakker, "Hallo");
        assert_eq!(afloop.uitkomst, PlakUitkomst::TypenMislukt);
        assert_eq!(afloop.vorig_klembord, VorigKlembord::Kwijt);
        assert_eq!(
            plakker.bord.stappen().last(),
            Some(&Stap::ZetTekst("Hallo ".into()))
        );
    }

    #[test]
    fn lege_tekst_raakt_niets() {
        let plakker = plakker(Nep::nieuw());
        assert_eq!(plak(plakker, "").uitkomst, PlakUitkomst::InMondig);
        assert!(plakker.bord.stappen().is_empty());
    }
}
