//! De wachtrij voor bestanden (fase 4): meerdere bestanden na elkaar uitschrijven, met
//! voortgang en annuleren, op het achtergrondwerkproces (ADR 0012).
//!
//! Een eigen thread bezit de motor (in de app een [`LokaleEngine`] met
//! [`Instellingen::achtergrond`](crate::engine::supervisor::Instellingen::achtergrond)) en
//! werkt de bestanden af in de volgorde waarin ze binnenkwamen. De rest van de app wacht
//! nooit op die thread:
//!
//! - [`Wachtrij::voeg_toe`] zet een bestand achteraan; de taak staat op "wacht" en de
//!   uitslag komt later, op een kanaal of via [`Wachtrij::voeg_toe_met`] naar een functie;
//! - na elk stuk gaat de taak naar "verwerkt" met een percentage, als de duur bekend is;
//! - [`Wachtrij::annuleer`] zet een taak meteen op "geannuleerd". Wacht ze nog, dan begint
//!   ze nooit; loopt ze, dan breekt het werkproces het bestand af en gaat het door met het
//!   volgende, zonder het model opnieuw te laden;
//! - is de wachtrij [`NAHOUDTIJD`] leeg, dan stopt het werkproces: het leeft alleen zolang
//!   er werk is, maar laadt het model niet uit tussen twee bestanden die net na elkaar komen;
//! - een paniek tijdens één bestand maakt dat bestand "fout" (met "opnieuw proberen") en de
//!   rest gaat door; de motor wordt dan eerst gestopt, zodat het volgende schoon begint;
//! - [`Wachtrij::wissel`] geeft de wachtrij een andere motor (een ander model): wat al in de
//!   rij staat, maakt de oude motor af; elk bestand dat daarna komt, gaat door de nieuwe.
//!
//! Alle wijzigingen gaan als [`Gebeurtenis`] (soort [`Soort::Bestand`]) naar de interface,
//! volgens het toestandsmodel in [`crate::taak`].
//!
//! Een vergadering (sprint 4b-C) gaat door dezelfde rij, zodat er één werkproces met één
//! model in het geheugen is: [`Wachtrij::voeg_vergadering_toe_met`] zet een opnamemap
//! achteraan, die [`Bestandsmotor::schrijf_gesprek_uit`] als gesprek uitschrijft. Voortgang,
//! annuleren en gebeurtenissen werken hetzelfde, met soort [`Soort::Vergadering`]. De soort
//! vergadering (online, in de ruimte of Alleen ik) leest het uitschrijven uit `opname.json`.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::aanbieder::{AanbiederFout, LokaleEngine, Uitgeschreven};
use crate::engine::beheer::BeheerFout;
use crate::engine::supervisor::{EngineFout, Voortgang};
use crate::filter::FilterInstellingen;
use crate::gesprek::{self, Gesprek, echo::Ontdubbelen};
use crate::taak::{Gebeurtenis, Soort, Taak, TaakId, Toestand, Vervolg};

/// Zo lang blijft het werkproces met het model staan nadat de rij leeg is, zodat een
/// bestand dat er net na komt niet opnieuw hoeft te laden.
pub const NAHOUDTIJD: Duration = Duration::from_secs(10);

/// De motor die de wachtrij gebruikt. In de app is dat de [`LokaleEngine`]; tests
/// gebruiken een eigen motor.
pub trait Bestandsmotor: Send + 'static {
    /// Schrijft een bestand uit, meldt na elk stuk de voortgang, en stopt zodra
    /// `annuleer` waar wordt (met [`EngineFout::Geannuleerd`]). Past het filter toe als
    /// het er is. Met `sprekers` en een sprekermodel worden daarna de sprekers herkend
    /// ([`gesprek::voeg_sprekers_toe`]).
    ///
    /// # Errors
    ///
    /// Geeft een [`AanbiederFout`] in gewone taal als het bestand of de engine faalt.
    fn schrijf_uit(
        &mut self,
        bestand: &Path,
        filter: Option<&FilterInstellingen>,
        sprekers: bool,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Uitgeschreven, AanbiederFout>;

    /// Stopt het werkproces; het volgende bestand start het opnieuw.
    fn stop(&mut self);

    /// Schrijft een afgesloten vergaderopname uit als gesprek, zoals
    /// [`gesprek::schrijf_gesprek_uit`]: voortgang over beide sporen samen, stoppen zodra
    /// `annuleer` waar wordt. Bewust zonder standaard: een omhulsel om een motor (zoals het
    /// bijhouden van het model in gebruik) moet dit zelf doorgeven, anders faalt elke
    /// vergadering stil met een interne fout.
    ///
    /// # Errors
    ///
    /// Geeft een [`AanbiederFout`] in gewone taal als de opname of de engine faalt.
    fn schrijf_gesprek_uit(
        &mut self,
        opnamemap: &Path,
        filter: Option<&FilterInstellingen>,
        ontdubbelen: Option<&Ontdubbelen>,
        sprekers: bool,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Gesprek, AanbiederFout>;

    /// Schrijft een vergaderopname opnieuw uit, zoals [`gesprek::schrijf_gesprek_opnieuw_uit`]
    /// (ADR 0023): zonder sprekermodel, met wie wanneer sprak uit `oud`. Ook bewust zonder
    /// standaard, om dezelfde reden als [`Bestandsmotor::schrijf_gesprek_uit`].
    ///
    /// # Errors
    ///
    /// Geeft een [`AanbiederFout`] in gewone taal als de opname of de engine faalt.
    fn schrijf_gesprek_opnieuw_uit(
        &mut self,
        opnamemap: &Path,
        filter: Option<&FilterInstellingen>,
        ontdubbelen: Option<&Ontdubbelen>,
        oud: &Gesprek,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Gesprek, AanbiederFout>;
}

impl Bestandsmotor for LokaleEngine {
    fn schrijf_uit(
        &mut self,
        bestand: &Path,
        filter: Option<&FilterInstellingen>,
        sprekers: bool,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Uitgeschreven, AanbiederFout> {
        let mut uitgeschreven = self.transcribeer_bestand(bestand, filter, voortgang, annuleer)?;
        if sprekers {
            gesprek::voeg_sprekers_toe(self, bestand, &mut uitgeschreven, voortgang, annuleer)?;
        }
        Ok(uitgeschreven)
    }

    fn stop(&mut self) {
        LokaleEngine::stop(self);
    }

    fn schrijf_gesprek_uit(
        &mut self,
        opnamemap: &Path,
        filter: Option<&FilterInstellingen>,
        ontdubbelen: Option<&Ontdubbelen>,
        sprekers: bool,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Gesprek, AanbiederFout> {
        gesprek::schrijf_gesprek_uit(
            self,
            opnamemap,
            filter,
            ontdubbelen,
            sprekers,
            voortgang,
            annuleer,
        )
    }

    fn schrijf_gesprek_opnieuw_uit(
        &mut self,
        opnamemap: &Path,
        filter: Option<&FilterInstellingen>,
        ontdubbelen: Option<&Ontdubbelen>,
        oud: &Gesprek,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Gesprek, AanbiederFout> {
        gesprek::schrijf_gesprek_opnieuw_uit(
            self,
            opnamemap,
            filter,
            ontdubbelen,
            oud,
            voortgang,
            annuleer,
        )
    }
}

/// Hoe een bestand afliep.
pub enum Bestandsuitslag {
    /// Gelukt; het filter kan toch "geen geluid" of "geen spraak" zeggen, en er kan een
    /// waarschuwing bij staan.
    Klaar(Uitgeschreven),
    /// De gebruiker heeft het geannuleerd.
    Geannuleerd,
    /// Mislukt, met een fout in gewone taal.
    Fout(AanbiederFout),
}

/// Hoe een vergadering afliep.
pub enum Gespreksuitslag {
    /// Gelukt; meldingen (een spoor ontbrak, echo weggelaten) staan in het gesprek.
    Klaar(Gesprek),
    /// De gebruiker heeft het geannuleerd.
    Geannuleerd,
    /// Mislukt, met een fout in gewone taal.
    Fout(AanbiederFout),
}

/// Waar de uitslag van een bestand heen gaat.
type Antwoord = Box<dyn FnOnce(TaakId, Bestandsuitslag) + Send>;

/// Waar de uitslag van een vergadering heen gaat.
type Gespreksantwoord = Box<dyn FnOnce(TaakId, Gespreksuitslag) + Send>;

/// Wat er uitgeschreven moet worden.
enum Inhoud {
    /// Een bestand.
    Bestand {
        bestand: PathBuf,
        /// Sprekers herkennen als het sprekermodel klaarstaat (4b-D, fase D-2).
        sprekers: bool,
        antwoord: Antwoord,
    },
    /// Een afgesloten vergaderopname (de map met `opname.json` en de sporen).
    Vergadering {
        opnamemap: PathBuf,
        /// Sprekers herkennen als het sprekermodel klaarstaat (4b-D).
        sprekers: bool,
        antwoord: Gespreksantwoord,
    },
    /// Een vergadering opnieuw uitschrijven (ADR 0023): de sprekers komen uit het oude verslag.
    Opnieuw(Opnieuw),
}

/// Een vergadering die opnieuw wordt uitgeschreven (ADR 0023).
struct Opnieuw {
    opnamemap: PathBuf,
    oud: Box<Gesprek>,
    /// Een eigen motor met het gekozen model, of `None` voor de motor van de rij. Met een eigen
    /// motor stopt de rij eerst zijn eigen werkproces, zodat er nooit twee draaien.
    motor: Option<Box<dyn Bestandsmotor>>,
    antwoord: Gespreksantwoord,
}

/// Werk dat op zijn beurt wacht.
struct Werk {
    id: TaakId,
    annuleer: Arc<AtomicBool>,
    inhoud: Inhoud,
}

/// Hoe één stuk werk afliep, voor bestand en vergadering samen.
enum Uitkomst<T> {
    Klaar(T),
    Geannuleerd,
    Fout(AanbiederFout),
}

impl From<Uitkomst<Uitgeschreven>> for Bestandsuitslag {
    fn from(uitkomst: Uitkomst<Uitgeschreven>) -> Self {
        match uitkomst {
            Uitkomst::Klaar(u) => Self::Klaar(u),
            Uitkomst::Geannuleerd => Self::Geannuleerd,
            Uitkomst::Fout(f) => Self::Fout(f),
        }
    }
}

impl From<Uitkomst<Gesprek>> for Gespreksuitslag {
    fn from(uitkomst: Uitkomst<Gesprek>) -> Self {
        match uitkomst {
            Uitkomst::Klaar(g) => Self::Klaar(g),
            Uitkomst::Geannuleerd => Self::Geannuleerd,
            Uitkomst::Fout(f) => Self::Fout(f),
        }
    }
}

/// Wat de thread moet doen.
enum Opdracht {
    Werk(Werk),
    /// Vanaf hier een andere motor; de oude stopt en gaat weg.
    Wissel(Box<dyn Bestandsmotor>),
    Stop,
}

/// Wat de aanroeper en de thread delen.
struct Gedeeld {
    taken: Mutex<HashMap<TaakId, Taak>>,
    vlaggen: Mutex<HashMap<TaakId, Arc<AtomicBool>>>,
    /// Het filter voor het volgende werk; de app kan het tussendoor wijzigen
    /// ([`Wachtrij::zet_filter`]).
    filter: Mutex<Option<FilterInstellingen>>,
    gebeurtenissen: Sender<Gebeurtenis>,
}

fn slot<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Gedeeld {
    /// Zet een taak in een nieuwe toestand en meldt dat, binnen het slot zodat de volgorde
    /// van de gebeurtenissen klopt.
    fn zet(&self, id: TaakId, naar: Toestand) -> Result<(), BeheerFout> {
        let mut taken = slot(&self.taken);
        let taak = taken.get_mut(&id).ok_or(BeheerFout::OnbekendeTaak(id))?;
        let gebeurtenis = taak.ga_naar(naar)?;
        // Is de interface weg, dan is er niemand om te vertellen; het werk gaat door.
        let _ = self.gebeurtenissen.send(gebeurtenis);
        Ok(())
    }

    fn toestand(&self, id: TaakId) -> Option<Toestand> {
        slot(&self.taken)
            .get(&id)
            .map(|taak| taak.toestand().clone())
    }
}

/// Werkt bestanden een voor een af op een eigen thread (zie de module-uitleg). Te delen
/// tussen threads (`Arc`); [`Wachtrij::stop`] werkt via een gedeelde verwijzing.
pub struct Wachtrij {
    zender: Sender<Opdracht>,
    gedeeld: Arc<Gedeeld>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Wachtrij {
    /// Start de wachtrij met [`NAHOUDTIJD`]. Met `filter` gaat elk transcript door het
    /// filter tegen verzonnen tekst; zonder komt het zoals het model het gaf. Alle
    /// wijzigingen komen op `gebeurtenissen`.
    #[must_use]
    pub fn start<M: Bestandsmotor>(
        motor: M,
        filter: Option<FilterInstellingen>,
        gebeurtenissen: Sender<Gebeurtenis>,
    ) -> Self {
        Self::start_met(motor, filter, gebeurtenissen, NAHOUDTIJD)
    }

    /// Als [`Wachtrij::start`], met een eigen nahoudtijd (voor tests).
    #[must_use]
    pub fn start_met<M: Bestandsmotor>(
        motor: M,
        filter: Option<FilterInstellingen>,
        gebeurtenissen: Sender<Gebeurtenis>,
        nahoudtijd: Duration,
    ) -> Self {
        let (zender, ontvanger) = mpsc::channel();
        let gedeeld = Arc::new(Gedeeld {
            taken: Mutex::new(HashMap::new()),
            vlaggen: Mutex::new(HashMap::new()),
            filter: Mutex::new(filter),
            gebeurtenissen,
        });
        let voor_thread = Arc::clone(&gedeeld);
        let thread = thread::spawn(move || {
            draai(Box::new(motor), &voor_thread, &ontvanger, nahoudtijd);
        });
        Self {
            zender,
            gedeeld,
            thread: Mutex::new(Some(thread)),
        }
    }

    /// Wijzigt het filter voor het werk dat daarna begint (zoals de schakelaar
    /// "Stopwoorden weghalen"). Werk dat al loopt, houdt het filter waarmee het begon.
    pub fn zet_filter(&self, filter: Option<FilterInstellingen>) {
        *slot(&self.gedeeld.filter) = filter;
    }

    /// Zet een bestand achteraan in de wachtrij. De taak staat op "wacht"; de uitslag
    /// komt op het teruggegeven kanaal zodra het bestand aan de beurt is geweest.
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout::Gestopt`] als de wachtrij gestopt is.
    pub fn voeg_toe(
        &self,
        bestand: PathBuf,
    ) -> Result<(TaakId, Receiver<Bestandsuitslag>), BeheerFout> {
        let (zender, ontvanger) = mpsc::channel();
        let id = self.voeg_toe_met(bestand, move |_, uitslag| {
            // Is de ontvanger weg, dan is de uitslag niet meer nodig.
            let _ = zender.send(uitslag);
        })?;
        Ok((id, ontvanger))
    }

    /// Als [`Wachtrij::voeg_toe`], maar de uitslag gaat met het taak-id naar `bij_uitslag`
    /// (op de thread van de wachtrij). Zo kan één verzamelaar alle uitslagen ontvangen.
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout::Gestopt`] als de wachtrij gestopt is.
    pub fn voeg_toe_met(
        &self,
        bestand: PathBuf,
        bij_uitslag: impl FnOnce(TaakId, Bestandsuitslag) + Send + 'static,
    ) -> Result<TaakId, BeheerFout> {
        self.voeg_toe_met_sprekers(bestand, false, bij_uitslag)
    }

    /// Als [`Wachtrij::voeg_toe_met`], en `sprekers` zegt of de sprekers herkend worden (de
    /// instelling "Sprekers herkennen"; zonder sprekermodel gebeurt het toch niet).
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout::Gestopt`] als de wachtrij gestopt is.
    pub fn voeg_toe_met_sprekers(
        &self,
        bestand: PathBuf,
        sprekers: bool,
        bij_uitslag: impl FnOnce(TaakId, Bestandsuitslag) + Send + 'static,
    ) -> Result<TaakId, BeheerFout> {
        self.zet_achteraan(
            Soort::Bestand,
            Inhoud::Bestand {
                bestand,
                sprekers,
                antwoord: Box::new(bij_uitslag),
            },
        )
    }

    /// Zet een afgesloten vergaderopname achteraan in de wachtrij, om uit te schrijven als
    /// gesprek, met sprekerherkenning als het sprekermodel klaarstaat. De taak (soort
    /// [`Soort::Vergadering`]) staat op "wacht"; de uitslag komt op het teruggegeven kanaal.
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout::Gestopt`] als de wachtrij gestopt is.
    pub fn voeg_vergadering_toe(
        &self,
        opnamemap: PathBuf,
    ) -> Result<(TaakId, Receiver<Gespreksuitslag>), BeheerFout> {
        let (zender, ontvanger) = mpsc::channel();
        let id = self.voeg_vergadering_toe_met(opnamemap, true, move |_, uitslag| {
            let _ = zender.send(uitslag);
        })?;
        Ok((id, ontvanger))
    }

    /// Als [`Wachtrij::voeg_vergadering_toe`], maar de uitslag gaat met het taak-id naar
    /// `bij_uitslag` (op de thread van de wachtrij), en `sprekers` zegt of de sprekers herkend
    /// worden (de instelling "Sprekers herkennen"; zonder sprekermodel gebeurt het toch niet).
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout::Gestopt`] als de wachtrij gestopt is.
    pub fn voeg_vergadering_toe_met(
        &self,
        opnamemap: PathBuf,
        sprekers: bool,
        bij_uitslag: impl FnOnce(TaakId, Gespreksuitslag) + Send + 'static,
    ) -> Result<TaakId, BeheerFout> {
        self.zet_achteraan(
            Soort::Vergadering,
            Inhoud::Vergadering {
                opnamemap,
                sprekers,
                antwoord: Box::new(bij_uitslag),
            },
        )
    }

    /// Zet een vergadering achteraan om haar opnieuw uit te schrijven (ADR 0023), met soort
    /// [`Soort::Vergadering`], zodat de gebeurtenissen gaan als bij een vergadering. `oud` is het
    /// huidige verslag: daaruit komen de sprekers en namen. Met `motor` (het gekozen model, als
    /// dat een ander is dan dat van de rij) stopt de rij eerst zijn eigen werkproces, en na
    /// afloop stopt en verdwijnt `motor` weer: er draait steeds één werkproces. De uitslag gaat
    /// met het taak-id naar `bij_uitslag` (op de thread van de wachtrij).
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout::Gestopt`] als de wachtrij gestopt is.
    pub fn voeg_opnieuw_toe_met(
        &self,
        opnamemap: PathBuf,
        oud: Gesprek,
        motor: Option<Box<dyn Bestandsmotor>>,
        bij_uitslag: impl FnOnce(TaakId, Gespreksuitslag) + Send + 'static,
    ) -> Result<TaakId, BeheerFout> {
        self.zet_achteraan(
            Soort::Vergadering,
            Inhoud::Opnieuw(Opnieuw {
                opnamemap,
                oud: Box::new(oud),
                motor,
                antwoord: Box::new(bij_uitslag),
            }),
        )
    }

    /// Maakt een taak op "wacht" en zet het werk achteraan.
    fn zet_achteraan(&self, soort: Soort, inhoud: Inhoud) -> Result<TaakId, BeheerFout> {
        let id = TaakId::nieuw();
        let annuleer = Arc::new(AtomicBool::new(false));
        {
            let mut taken = slot(&self.gedeeld.taken);
            // Afgelopen taken zijn niet meer nodig; zo groeit de lijst niet eindeloos.
            taken.retain(|_, taak| taak.toestand().is_actief());
            let taak = Taak::nieuw(id, soort);
            // Een nieuwe taak staat al op "wacht"; de interface hoort dat ook.
            let _ = self.gedeeld.gebeurtenissen.send(Gebeurtenis {
                taak: id,
                soort,
                toestand: taak.toestand().clone(),
            });
            taken.insert(id, taak);
        }
        slot(&self.gedeeld.vlaggen).insert(id, Arc::clone(&annuleer));
        let werk = Werk {
            id,
            annuleer,
            inhoud,
        };
        if self.zender.send(Opdracht::Werk(werk)).is_err() {
            let _ = self
                .gedeeld
                .zet(id, Toestand::fout("De wachtrij is gestopt.", Vervolg::Geen));
            return Err(BeheerFout::Gestopt);
        }
        Ok(id)
    }

    /// Wisselt de motor, bijvoorbeeld omdat de gebruiker een ander model koos. De wissel
    /// staat in de rij zoals een bestand: wat er al in stond, schrijft de oude motor af; elk
    /// bestand dat na deze aanroep komt, gaat door `motor`. Daarna stopt de oude motor en
    /// gaat hij weg, zodat zijn model niet meer in gebruik is.
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout::Gestopt`] als de wachtrij gestopt is.
    pub fn wissel<M: Bestandsmotor>(&self, motor: M) -> Result<(), BeheerFout> {
        self.zender
            .send(Opdracht::Wissel(Box::new(motor)))
            .map_err(|_| BeheerFout::Gestopt)
    }

    /// Annuleert een bestand. Het staat meteen op "geannuleerd"; wachtte het nog, dan
    /// begint het nooit, liep het al, dan breekt het werkproces het af.
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout`] als de taak onbekend of al afgelopen is.
    pub fn annuleer(&self, id: TaakId) -> Result<(), BeheerFout> {
        self.gedeeld.zet(id, Toestand::Geannuleerd)?;
        if let Some(vlag) = slot(&self.gedeeld.vlaggen).get(&id) {
            vlag.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Annuleert alle bestanden die nog wachten of lopen.
    pub fn annuleer_alles(&self) {
        let actief: Vec<TaakId> = slot(&self.gedeeld.taken)
            .iter()
            .filter(|(_, taak)| taak.toestand().is_actief())
            .map(|(id, _)| *id)
            .collect();
        for id in actief {
            let _ = self.annuleer(id);
        }
    }

    /// De huidige toestand van een bestand, of `None` als het niet (meer) bekend is.
    #[must_use]
    pub fn toestand(&self, id: TaakId) -> Option<Toestand> {
        self.gedeeld.toestand(id)
    }

    /// Annuleert alles wat nog wacht of loopt, stopt de thread en het werkproces, en wacht
    /// tot de thread klaar is. Elk bestand krijgt zijn uitslag ("geannuleerd"). Een tweede
    /// keer stoppen doet niets.
    pub fn stop(&self) {
        self.annuleer_alles();
        let _ = self.zender.send(Opdracht::Stop);
        let thread = slot(&self.thread).take();
        if let Some(thread) = thread {
            let _ = thread.join();
        }
    }
}

impl Drop for Wachtrij {
    fn drop(&mut self) {
        self.stop();
    }
}

/// De lus van de thread.
fn draai(
    mut motor: Box<dyn Bestandsmotor>,
    gedeeld: &Gedeeld,
    ontvanger: &Receiver<Opdracht>,
    nahoudtijd: Duration,
) {
    loop {
        let opdracht = match ontvanger.try_recv() {
            Ok(opdracht) => opdracht,
            Err(TryRecvError::Empty) => match ontvanger.recv_timeout(nahoudtijd) {
                Ok(opdracht) => opdracht,
                Err(RecvTimeoutError::Timeout) => {
                    // Al een tijdje niets te doen: het werkproces mag weg tot het volgende bestand.
                    motor.stop();
                    match ontvanger.recv() {
                        Ok(opdracht) => opdracht,
                        Err(_) => break,
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            },
            Err(TryRecvError::Disconnected) => break,
        };
        // Het filter zoals het nu staat: een wijziging geldt vanaf het volgende werk.
        let filter = *slot(&gedeeld.filter);
        match opdracht {
            Opdracht::Werk(Werk {
                id,
                annuleer,
                inhoud,
            }) => match inhoud {
                Inhoud::Bestand {
                    bestand,
                    sprekers,
                    antwoord,
                } => {
                    let uitkomst =
                        voer_veilig_uit(motor.as_mut(), gedeeld, id, |motor, voortgang| {
                            motor.schrijf_uit(
                                &bestand,
                                filter.as_ref(),
                                sprekers,
                                voortgang,
                                &annuleer,
                            )
                        });
                    slot(&gedeeld.vlaggen).remove(&id);
                    antwoord(id, uitkomst.into());
                }
                Inhoud::Vergadering {
                    opnamemap,
                    sprekers,
                    antwoord,
                } => {
                    // Ontdubbelen staat aan; in de ruimte en bij Alleen ik doet het niets.
                    let ontdubbelen = Ontdubbelen::default();
                    let uitkomst =
                        voer_veilig_uit(motor.as_mut(), gedeeld, id, |motor, voortgang| {
                            motor.schrijf_gesprek_uit(
                                &opnamemap,
                                filter.as_ref(),
                                Some(&ontdubbelen),
                                sprekers,
                                voortgang,
                                &annuleer,
                            )
                        });
                    slot(&gedeeld.vlaggen).remove(&id);
                    antwoord(id, uitkomst.into());
                }
                Inhoud::Opnieuw(werk) => {
                    voer_opnieuw_uit(
                        motor.as_mut(),
                        werk,
                        filter.as_ref(),
                        gedeeld,
                        id,
                        &annuleer,
                    );
                }
            },
            Opdracht::Wissel(nieuw) => {
                let mut oud = std::mem::replace(&mut motor, nieuw);
                let _ = catch_unwind(AssertUnwindSafe(|| oud.stop()));
                drop(oud);
            }
            // `stop` annuleerde alles al; wat voor de stop in de rij stond, is afgemeld.
            Opdracht::Stop => break,
        }
    }
    motor.stop();
}

/// Schrijft een vergadering opnieuw uit (ADR 0023), met de eigen motor van het werk als die er
/// is: dan stopt eerst het werkproces van de rij, en na afloop stopt en verdwijnt de eigen motor,
/// zodat er steeds één werkproces draait.
/// De uitslag gaat naar het antwoord van het werk.
fn voer_opnieuw_uit(
    rij: &mut dyn Bestandsmotor,
    werk: Opnieuw,
    filter: Option<&FilterInstellingen>,
    gedeeld: &Gedeeld,
    id: TaakId,
    annuleer: &AtomicBool,
) {
    let Opnieuw {
        opnamemap,
        oud,
        motor: mut eigen,
        antwoord,
    } = werk;
    let ontdubbelen = Ontdubbelen::default();
    // Al geannuleerd terwijl het wachtte: het werkproces van de rij hoeft dan niet weg
    // (`voer_veilig_uit` begint er dan niet aan).
    if eigen.is_some() && !annuleer.load(Ordering::SeqCst) {
        let _ = catch_unwind(AssertUnwindSafe(|| rij.stop()));
    }
    let werkt: &mut dyn Bestandsmotor = match eigen.as_mut() {
        Some(eigen) => eigen.as_mut(),
        None => rij,
    };
    let uitkomst = voer_veilig_uit(werkt, gedeeld, id, |motor, voortgang| {
        motor.schrijf_gesprek_opnieuw_uit(
            &opnamemap,
            filter,
            Some(&ontdubbelen),
            &oud,
            voortgang,
            annuleer,
        )
    });
    if let Some(mut eigen) = eigen {
        let _ = catch_unwind(AssertUnwindSafe(|| eigen.stop()));
        drop(eigen);
    }
    slot(&gedeeld.vlaggen).remove(&id);
    antwoord(id, uitkomst.into());
}

/// Voert één stuk werk uit (zie [`voer_uit`]). Na een paniek wordt het werk "fout" met
/// "opnieuw proberen", en de motor stopt, zodat het volgende met een schoon werkproces begint.
fn voer_veilig_uit<M: Bestandsmotor + ?Sized, T>(
    motor: &mut M,
    gedeeld: &Gedeeld,
    id: TaakId,
    doe: impl FnOnce(&mut M, &mut dyn FnMut(Voortgang)) -> Result<T, AanbiederFout>,
) -> Uitkomst<T> {
    catch_unwind(AssertUnwindSafe(|| voer_uit(motor, gedeeld, id, doe))).unwrap_or_else(|_| {
        let fout = AanbiederFout::Engine(EngineFout::Intern);
        let _ = gedeeld.zet(id, Toestand::uit_aanbieder_fout(&fout));
        let _ = catch_unwind(AssertUnwindSafe(|| motor.stop()));
        Uitkomst::Fout(fout)
    })
}

/// Voert één stuk werk uit en meldt voortgang en uitkomst. Is het intussen geannuleerd, dan
/// wordt de uitkomst genegeerd.
fn voer_uit<M: Bestandsmotor + ?Sized, T>(
    motor: &mut M,
    gedeeld: &Gedeeld,
    id: TaakId,
    doe: impl FnOnce(&mut M, &mut dyn FnMut(Voortgang)) -> Result<T, AanbiederFout>,
) -> Uitkomst<T> {
    // Geannuleerd terwijl het nog wachtte: er niet aan beginnen.
    if gedeeld.zet(id, Toestand::verwerkt(None)).is_err() {
        return Uitkomst::Geannuleerd;
    }
    let mut laatste = None;
    let mut voortgang = |stand: Voortgang| {
        let procent = stand.procent();
        if procent.is_some() && Some((procent, stand.stap)) != laatste {
            laatste = Some((procent, stand.stap));
            // Mislukt dit, dan is de taak geannuleerd; de motor hoort dat via de vlag.
            let _ = gedeeld.zet(
                id,
                Toestand::Verwerkt {
                    voortgang: procent,
                    stap: stand.stap,
                },
            );
        }
    };
    let resultaat = doe(motor, &mut voortgang);
    let naar = match &resultaat {
        Ok(_) => Toestand::Klaar,
        Err(AanbiederFout::Engine(EngineFout::Geannuleerd)) => Toestand::Geannuleerd,
        Err(fout) => Toestand::uit_aanbieder_fout(fout),
    };
    match (gedeeld.zet(id, naar), resultaat) {
        (Ok(()), Ok(klaar)) => Uitkomst::Klaar(klaar),
        // Geannuleerd: door de motor gemeld, of de overgang mocht niet meer omdat de
        // gebruiker tijdens het uitschrijven annuleerde.
        (Ok(()), Err(AanbiederFout::Engine(EngineFout::Geannuleerd))) | (Err(_), _) => {
            Uitkomst::Geannuleerd
        }
        (Ok(()), Err(fout)) => Uitkomst::Fout(fout),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    use super::*;
    use crate::aanbieder::Transcript;
    use crate::audio::AudioFout;
    use crate::filter::{self, Meting};
    use crate::taak::Stap;
    use crate::taak::Vervolg;

    /// Wat de nepmotor bijhoudt.
    #[derive(Default)]
    struct Stand {
        uitgeschreven: Mutex<Vec<PathBuf>>,
        /// Het filter waarmee elk bestand begon.
        filters: Mutex<Vec<Option<FilterInstellingen>>>,
        gestopt: AtomicUsize,
    }

    /// Een motor die per bestand vijf stukken "uitschrijft" en luistert naar annuleren.
    /// Een bestand met "kapot" in de naam faalt; met "traag" duurt elk stuk langer.
    struct NepMotor {
        stand: Arc<Stand>,
    }

    impl Bestandsmotor for NepMotor {
        fn schrijf_uit(
            &mut self,
            bestand: &Path,
            filter: Option<&FilterInstellingen>,
            _sprekers: bool,
            voortgang: &mut dyn FnMut(Voortgang),
            annuleer: &AtomicBool,
        ) -> Result<Uitgeschreven, AanbiederFout> {
            slot(&self.stand.uitgeschreven).push(bestand.to_path_buf());
            slot(&self.stand.filters).push(filter.copied());
            let naam = bestand.to_string_lossy();
            assert!(!naam.contains("paniek"), "nep: paniek in de motor");
            if naam.contains("kapot") {
                return Err(AanbiederFout::Audio(AudioFout::Beschadigd));
            }
            let stuktijd = if naam.contains("traag") { 60 } else { 2 };
            for stuk in 1..=5 {
                if annuleer.load(Ordering::SeqCst) {
                    return Err(AanbiederFout::Engine(EngineFout::Geannuleerd));
                }
                thread::sleep(Duration::from_millis(stuktijd));
                voortgang(Voortgang {
                    klaar: Duration::from_secs(stuk),
                    totaal: Some(Duration::from_secs(5)),
                    stap: Stap::Uitschrijven,
                });
            }
            let transcript = Transcript {
                segmenten: Vec::new(),
                audio_duur: Duration::from_secs(5),
            };
            let meting = Meting::van_samples(&vec![0.1; 16_000]);
            let gefilterd = match filter {
                Some(f) => filter::pas_toe(transcript, &meting, f),
                None => filter::zonder_filter(transcript, &meting),
            };
            Ok(Uitgeschreven {
                gefilterd,
                waarschuwing: None,
                sprekers: None,
                sprekerfout: None,
            })
        }

        fn stop(&mut self) {
            self.stand.gestopt.fetch_add(1, Ordering::SeqCst);
        }

        /// Deze testmotor is alleen voor bestanden; vergaderingen staan in `NepGesprekMotor`.
        fn schrijf_gesprek_uit(
            &mut self,
            _opnamemap: &Path,
            _filter: Option<&FilterInstellingen>,
            _ontdubbelen: Option<&Ontdubbelen>,
            _sprekers: bool,
            _voortgang: &mut dyn FnMut(Voortgang),
            _annuleer: &AtomicBool,
        ) -> Result<Gesprek, AanbiederFout> {
            Err(AanbiederFout::Engine(EngineFout::Intern))
        }

        fn schrijf_gesprek_opnieuw_uit(
            &mut self,
            _opnamemap: &Path,
            _filter: Option<&FilterInstellingen>,
            _ontdubbelen: Option<&Ontdubbelen>,
            _oud: &Gesprek,
            _voortgang: &mut dyn FnMut(Voortgang),
            _annuleer: &AtomicBool,
        ) -> Result<Gesprek, AanbiederFout> {
            Err(AanbiederFout::Engine(EngineFout::Intern))
        }
    }

    struct Opzet {
        wachtrij: Wachtrij,
        gebeurtenissen: Receiver<Gebeurtenis>,
        stand: Arc<Stand>,
    }

    fn opzet() -> Opzet {
        opzet_met(Duration::from_millis(20))
    }

    fn opzet_met(nahoudtijd: Duration) -> Opzet {
        let stand = Arc::new(Stand::default());
        let (zender, gebeurtenissen) = mpsc::channel();
        let wachtrij = Wachtrij::start_met(
            NepMotor {
                stand: Arc::clone(&stand),
            },
            Some(FilterInstellingen::standaard()),
            zender,
            nahoudtijd,
        );
        Opzet {
            wachtrij,
            gebeurtenissen,
            stand,
        }
    }

    fn uitslag(ontvanger: &Receiver<Bestandsuitslag>) -> Bestandsuitslag {
        ontvanger.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    /// De toestanden die een taak doorliep, uit de stroom gebeurtenissen.
    fn toestanden_van(gebeurtenissen: &[Gebeurtenis], id: TaakId) -> Vec<Toestand> {
        gebeurtenissen
            .iter()
            .filter(|g| g.taak == id)
            .map(|g| g.toestand.clone())
            .collect()
    }

    #[test]
    fn een_bestand_gaat_van_wacht_via_voortgang_naar_klaar() {
        let o = opzet();
        let (id, ontvanger) = o.wachtrij.voeg_toe("a.mp4".into()).unwrap();
        assert!(matches!(uitslag(&ontvanger), Bestandsuitslag::Klaar(_)));
        assert_eq!(o.wachtrij.toestand(id), Some(Toestand::Klaar));
        let alles: Vec<_> = o.gebeurtenissen.try_iter().collect();
        assert!(alles.iter().all(|g| g.soort == Soort::Bestand));
        let verwerkt = Toestand::verwerkt;
        assert_eq!(
            toestanden_van(&alles, id),
            vec![
                Toestand::Wacht,
                verwerkt(None),
                verwerkt(Some(20)),
                verwerkt(Some(40)),
                verwerkt(Some(60)),
                verwerkt(Some(80)),
                verwerkt(Some(100)),
                Toestand::Klaar,
            ]
        );
    }

    #[test]
    fn een_gewijzigd_filter_geldt_voor_het_volgende_bestand() {
        let o = opzet();
        let (_, eerste) = o.wachtrij.voeg_toe("a.mp4".into()).unwrap();
        assert!(matches!(uitslag(&eerste), Bestandsuitslag::Klaar(_)));
        o.wachtrij
            .zet_filter(Some(FilterInstellingen::standaard().met_stopwoorden(false)));
        let (_, tweede) = o.wachtrij.voeg_toe("b.mp4".into()).unwrap();
        assert!(matches!(uitslag(&tweede), Bestandsuitslag::Klaar(_)));
        let filters = slot(&o.stand.filters).clone();
        assert_eq!(filters.len(), 2);
        assert!(filters[0].is_some_and(|f| f.stopwoorden));
        assert!(filters[1].is_some_and(|f| !f.stopwoorden));
    }

    #[test]
    fn bestanden_gaan_in_volgorde_en_het_werkproces_stopt_als_de_rij_leeg_is() {
        let o = opzet();
        let (_, eerste) = o.wachtrij.voeg_toe("traag-1.mp4".into()).unwrap();
        let (tweede_id, tweede) = o.wachtrij.voeg_toe("2.mov".into()).unwrap();
        // Het tweede wacht zolang het eerste loopt.
        assert_eq!(o.wachtrij.toestand(tweede_id), Some(Toestand::Wacht));
        assert!(matches!(uitslag(&eerste), Bestandsuitslag::Klaar(_)));
        assert!(matches!(uitslag(&tweede), Bestandsuitslag::Klaar(_)));
        assert_eq!(
            *slot(&o.stand.uitgeschreven),
            vec![PathBuf::from("traag-1.mp4"), PathBuf::from("2.mov")]
        );
        // Na het laatste bestand stopt het werkproces.
        let tot = std::time::Instant::now() + Duration::from_secs(5);
        while o.stand.gestopt.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < tot {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(o.stand.gestopt.load(Ordering::SeqCst) >= 1);
    }

    /// Een tweede nepmotor die laat zien welk bestand hij kreeg, en meldt wanneer hij weg is.
    struct AndereMotor {
        stand: Arc<Stand>,
        weg: Arc<AtomicBool>,
    }

    impl Drop for AndereMotor {
        fn drop(&mut self) {
            self.weg.store(true, Ordering::SeqCst);
        }
    }

    impl Bestandsmotor for AndereMotor {
        fn schrijf_uit(
            &mut self,
            bestand: &Path,
            filter: Option<&FilterInstellingen>,
            sprekers: bool,
            voortgang: &mut dyn FnMut(Voortgang),
            annuleer: &AtomicBool,
        ) -> Result<Uitgeschreven, AanbiederFout> {
            NepMotor {
                stand: Arc::clone(&self.stand),
            }
            .schrijf_uit(bestand, filter, sprekers, voortgang, annuleer)
        }

        fn stop(&mut self) {
            self.stand.gestopt.fetch_add(1, Ordering::SeqCst);
        }

        fn schrijf_gesprek_uit(
            &mut self,
            opnamemap: &Path,
            filter: Option<&FilterInstellingen>,
            ontdubbelen: Option<&Ontdubbelen>,
            sprekers: bool,
            voortgang: &mut dyn FnMut(Voortgang),
            annuleer: &AtomicBool,
        ) -> Result<Gesprek, AanbiederFout> {
            NepMotor {
                stand: Arc::clone(&self.stand),
            }
            .schrijf_gesprek_uit(
                opnamemap,
                filter,
                ontdubbelen,
                sprekers,
                voortgang,
                annuleer,
            )
        }

        fn schrijf_gesprek_opnieuw_uit(
            &mut self,
            _opnamemap: &Path,
            _filter: Option<&FilterInstellingen>,
            _ontdubbelen: Option<&Ontdubbelen>,
            _oud: &Gesprek,
            _voortgang: &mut dyn FnMut(Voortgang),
            _annuleer: &AtomicBool,
        ) -> Result<Gesprek, AanbiederFout> {
            Err(AanbiederFout::Engine(EngineFout::Intern))
        }
    }

    #[test]
    fn na_een_wissel_maakt_de_oude_motor_de_rij_af_en_krijgt_de_nieuwe_wat_daarna_komt() {
        let o = opzet();
        let nieuw = Arc::new(Stand::default());
        let eerste_weg = Arc::new(AtomicBool::new(false));
        // Begin met een eigen motor die we kunnen volgen.
        o.wachtrij
            .wissel(AndereMotor {
                stand: Arc::clone(&o.stand),
                weg: Arc::clone(&eerste_weg),
            })
            .unwrap();
        let (_, eerste) = o.wachtrij.voeg_toe("traag-oud.mp4".into()).unwrap();
        let (_, tweede) = o.wachtrij.voeg_toe("oud.mp4".into()).unwrap();
        let tweede_weg = Arc::new(AtomicBool::new(false));
        o.wachtrij
            .wissel(AndereMotor {
                stand: Arc::clone(&nieuw),
                weg: Arc::clone(&tweede_weg),
            })
            .unwrap();
        // De oude motor is nog niet weg: hij heeft nog werk.
        assert!(!eerste_weg.load(Ordering::SeqCst));
        let (_, derde) = o.wachtrij.voeg_toe("nieuw.mp4".into()).unwrap();
        for ontvanger in [&eerste, &tweede, &derde] {
            assert!(matches!(uitslag(ontvanger), Bestandsuitslag::Klaar(_)));
        }
        assert_eq!(
            *slot(&o.stand.uitgeschreven),
            vec![PathBuf::from("traag-oud.mp4"), PathBuf::from("oud.mp4")]
        );
        assert_eq!(
            *slot(&nieuw.uitgeschreven),
            vec![PathBuf::from("nieuw.mp4")]
        );
        // De oude motor is gestopt en weg, zodat zijn model vrij is; de nieuwe draait nog.
        assert!(eerste_weg.load(Ordering::SeqCst));
        assert!(!tweede_weg.load(Ordering::SeqCst));
        o.wachtrij.stop();
        assert!(tweede_weg.load(Ordering::SeqCst));
    }

    #[test]
    fn een_wachtend_bestand_annuleren_begint_er_nooit_aan() {
        let o = opzet();
        let (_, eerste) = o.wachtrij.voeg_toe("traag.mp4".into()).unwrap();
        let (id, tweede) = o.wachtrij.voeg_toe("nooit.mp4".into()).unwrap();
        o.wachtrij.annuleer(id).unwrap();
        assert_eq!(o.wachtrij.toestand(id), Some(Toestand::Geannuleerd));
        assert!(matches!(uitslag(&eerste), Bestandsuitslag::Klaar(_)));
        assert!(matches!(uitslag(&tweede), Bestandsuitslag::Geannuleerd));
        assert_eq!(
            *slot(&o.stand.uitgeschreven),
            vec![PathBuf::from("traag.mp4")]
        );
    }

    #[test]
    fn een_lopend_bestand_annuleren_breekt_het_af_en_het_volgende_gaat_door() {
        let o = opzet();
        let (id, eerste) = o.wachtrij.voeg_toe("traag.mp4".into()).unwrap();
        let (_, tweede) = o.wachtrij.voeg_toe("daarna.mp4".into()).unwrap();
        // Wacht tot het eerste loopt.
        let tot = std::time::Instant::now() + Duration::from_secs(5);
        while !matches!(o.wachtrij.toestand(id), Some(Toestand::Verwerkt { .. }))
            && std::time::Instant::now() < tot
        {
            thread::sleep(Duration::from_millis(2));
        }
        o.wachtrij.annuleer(id).unwrap();
        assert!(matches!(uitslag(&eerste), Bestandsuitslag::Geannuleerd));
        assert!(matches!(uitslag(&tweede), Bestandsuitslag::Klaar(_)));
        assert_eq!(o.wachtrij.toestand(id), Some(Toestand::Geannuleerd));
        // Een afgelopen taak kan niet nog eens geannuleerd worden.
        assert!(o.wachtrij.annuleer(id).is_err());
        let alles: Vec<_> = o.gebeurtenissen.try_iter().collect();
        assert_eq!(
            toestanden_van(&alles, id).last(),
            Some(&Toestand::Geannuleerd)
        );
    }

    #[test]
    fn een_kapot_bestand_geeft_een_fout_en_de_rest_gaat_door() {
        let o = opzet();
        let (id, eerste) = o.wachtrij.voeg_toe("kapot.mp4".into()).unwrap();
        let (_, tweede) = o.wachtrij.voeg_toe("goed.mp4".into()).unwrap();
        assert!(matches!(uitslag(&eerste), Bestandsuitslag::Fout(_)));
        assert!(matches!(uitslag(&tweede), Bestandsuitslag::Klaar(_)));
        assert!(matches!(
            o.wachtrij.toestand(id),
            Some(Toestand::Fout {
                vervolg: Vervolg::Geen,
                ..
            })
        ));
    }

    #[test]
    fn alles_annuleren_en_stoppen() {
        let o = opzet();
        let (_, eerste) = o.wachtrij.voeg_toe("traag-a.mp4".into()).unwrap();
        let (_, tweede) = o.wachtrij.voeg_toe("traag-b.mp4".into()).unwrap();
        o.wachtrij.annuleer_alles();
        assert!(matches!(uitslag(&eerste), Bestandsuitslag::Geannuleerd));
        assert!(matches!(uitslag(&tweede), Bestandsuitslag::Geannuleerd));
        o.wachtrij.stop();
        assert!(o.stand.gestopt.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn het_werkproces_blijft_even_staan_tussen_twee_bestanden() {
        let o = opzet_met(Duration::from_secs(10));
        let (_, eerste) = o.wachtrij.voeg_toe("a.mp4".into()).unwrap();
        assert!(matches!(uitslag(&eerste), Bestandsuitslag::Klaar(_)));
        thread::sleep(Duration::from_millis(150));
        // De rij is leeg, maar het model blijft nog geladen.
        assert_eq!(o.stand.gestopt.load(Ordering::SeqCst), 0);
        let (_, tweede) = o.wachtrij.voeg_toe("b.mp4".into()).unwrap();
        assert!(matches!(uitslag(&tweede), Bestandsuitslag::Klaar(_)));
        assert_eq!(o.stand.gestopt.load(Ordering::SeqCst), 0);
        // Stoppen stopt het werkproces wel meteen.
        o.wachtrij.stop();
        assert!(o.stand.gestopt.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn een_paniek_maakt_dat_bestand_fout_en_de_rest_gaat_door() {
        let o = opzet();
        let (id, eerste) = o.wachtrij.voeg_toe("paniek.mp4".into()).unwrap();
        let (_, tweede) = o.wachtrij.voeg_toe("daarna.mp4".into()).unwrap();
        assert!(matches!(
            uitslag(&eerste),
            Bestandsuitslag::Fout(AanbiederFout::Engine(EngineFout::Intern))
        ));
        assert!(matches!(uitslag(&tweede), Bestandsuitslag::Klaar(_)));
        assert!(matches!(
            o.wachtrij.toestand(id),
            Some(Toestand::Fout {
                vervolg: Vervolg::OpnieuwProberen,
                ..
            })
        ));
        // De motor is na de paniek gestopt, zodat het volgende bestand schoon begon.
        assert!(o.stand.gestopt.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn een_verzamelaar_krijgt_alle_uitslagen() {
        let o = opzet();
        let (zender, verzameld) = mpsc::channel();
        let mut ids = Vec::new();
        for naam in ["a.mp4", "kapot.mp4", "c.mp4"] {
            let zender = zender.clone();
            let id = o
                .wachtrij
                .voeg_toe_met(naam.into(), move |id, u| {
                    let _ = zender.send((id, u));
                })
                .unwrap();
            ids.push(id);
        }
        let uitslagen: Vec<_> = (0..3)
            .map(|_| verzameld.recv_timeout(Duration::from_secs(5)).unwrap())
            .collect();
        assert_eq!(uitslagen.iter().map(|(id, _)| *id).collect::<Vec<_>>(), ids);
        let uitslagen: Vec<_> = uitslagen.into_iter().map(|(_, u)| u).collect();
        assert!(matches!(uitslagen[0], Bestandsuitslag::Klaar(_)));
        assert!(matches!(uitslagen[1], Bestandsuitslag::Fout(_)));
        assert!(matches!(uitslagen[2], Bestandsuitslag::Klaar(_)));
    }

    #[test]
    fn taakids_van_de_wachtrij_zijn_uniek() {
        let o = opzet();
        let (a, _) = o.wachtrij.voeg_toe("a.mp4".into()).unwrap();
        let (b, _) = o.wachtrij.voeg_toe("b.mp4".into()).unwrap();
        assert_ne!(a, b);
    }

    // -----------------------------------------------------------------------
    // Vergaderingen in dezelfde rij (sprint 4b-C)
    // -----------------------------------------------------------------------

    use crate::gesprek::{Spoorstand, Vergadersoort};

    /// Een motor die bestanden en vergaderingen kent: een vergadering duurt vier stukken; een
    /// map met "kapot" faalt, met "traag" duurt elk stuk langer.
    struct NepGesprekMotor {
        stand: Arc<Stand>,
        binnen: NepMotor,
    }

    impl Bestandsmotor for NepGesprekMotor {
        fn schrijf_uit(
            &mut self,
            bestand: &Path,
            filter: Option<&FilterInstellingen>,
            sprekers: bool,
            voortgang: &mut dyn FnMut(Voortgang),
            annuleer: &AtomicBool,
        ) -> Result<Uitgeschreven, AanbiederFout> {
            self.binnen
                .schrijf_uit(bestand, filter, sprekers, voortgang, annuleer)
        }

        fn stop(&mut self) {
            self.binnen.stop();
        }

        fn schrijf_gesprek_uit(
            &mut self,
            opnamemap: &Path,
            _filter: Option<&FilterInstellingen>,
            ontdubbelen: Option<&Ontdubbelen>,
            sprekers: bool,
            voortgang: &mut dyn FnMut(Voortgang),
            annuleer: &AtomicBool,
        ) -> Result<Gesprek, AanbiederFout> {
            assert!(ontdubbelen.is_some(), "de rij ontdubbelt altijd");
            assert!(sprekers, "voeg_vergadering_toe vraagt om sprekers");
            slot(&self.stand.uitgeschreven).push(opnamemap.to_path_buf());
            let naam = opnamemap.to_string_lossy();
            if naam.contains("kapot") {
                return Err(AanbiederFout::Audio(AudioFout::Beschadigd));
            }
            let stuktijd = if naam.contains("traag") { 60 } else { 2 };
            for stuk in 1..=4 {
                if annuleer.load(Ordering::SeqCst) {
                    return Err(AanbiederFout::Engine(EngineFout::Geannuleerd));
                }
                thread::sleep(Duration::from_millis(stuktijd));
                voortgang(Voortgang {
                    klaar: Duration::from_secs(stuk),
                    totaal: Some(Duration::from_secs(4)),
                    stap: Stap::Uitschrijven,
                });
            }
            Ok(Gesprek {
                beurten: Vec::new(),
                duur: Duration::from_secs(4),
                meldingen: vec!["nep".to_string()],
                jij: Spoorstand::Opgenomen,
                anderen: Spoorstand::Ontbrak,
                weggelaten: 0,
                soort: Vergadersoort::Ruimte,
            })
        }

        /// Geeft het oude gesprek terug met een melding van deze motor erbij.
        fn schrijf_gesprek_opnieuw_uit(
            &mut self,
            opnamemap: &Path,
            _filter: Option<&FilterInstellingen>,
            ontdubbelen: Option<&Ontdubbelen>,
            oud: &Gesprek,
            voortgang: &mut dyn FnMut(Voortgang),
            annuleer: &AtomicBool,
        ) -> Result<Gesprek, AanbiederFout> {
            assert!(ontdubbelen.is_some(), "de rij ontdubbelt altijd");
            slot(&self.stand.uitgeschreven).push(opnamemap.to_path_buf());
            if annuleer.load(Ordering::SeqCst) {
                return Err(AanbiederFout::Engine(EngineFout::Geannuleerd));
            }
            voortgang(Voortgang {
                klaar: Duration::from_secs(1),
                totaal: Some(Duration::from_secs(1)),
                stap: Stap::Uitschrijven,
            });
            let mut nieuw = oud.clone();
            nieuw.meldingen.push("opnieuw".to_string());
            Ok(nieuw)
        }
    }

    fn opzet_gesprek() -> Opzet {
        let stand = Arc::new(Stand::default());
        let (zender, gebeurtenissen) = mpsc::channel();
        let wachtrij = Wachtrij::start_met(
            NepGesprekMotor {
                stand: Arc::clone(&stand),
                binnen: NepMotor {
                    stand: Arc::clone(&stand),
                },
            },
            Some(FilterInstellingen::bestand()),
            zender,
            Duration::from_millis(20),
        );
        Opzet {
            wachtrij,
            gebeurtenissen,
            stand,
        }
    }

    fn gespreksuitslag(ontvanger: &Receiver<Gespreksuitslag>) -> Gespreksuitslag {
        ontvanger.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    #[test]
    fn een_vergadering_gaat_van_wacht_via_voortgang_naar_klaar() {
        let o = opzet_gesprek();
        let (id, ontvanger) = o.wachtrij.voeg_vergadering_toe("opname-a".into()).unwrap();
        let uitslag = gespreksuitslag(&ontvanger);
        assert!(matches!(uitslag, Gespreksuitslag::Klaar(_)), "niet klaar");
        let Gespreksuitslag::Klaar(gesprek) = uitslag else {
            return;
        };
        assert_eq!(gesprek.soort, Vergadersoort::Ruimte);
        assert_eq!(gesprek.meldingen, vec!["nep".to_string()]);
        assert_eq!(o.wachtrij.toestand(id), Some(Toestand::Klaar));
        let alles: Vec<_> = o.gebeurtenissen.try_iter().collect();
        assert!(
            alles
                .iter()
                .filter(|g| g.taak == id)
                .all(|g| g.soort == Soort::Vergadering)
        );
        let verwerkt = Toestand::verwerkt;
        assert_eq!(
            toestanden_van(&alles, id),
            vec![
                Toestand::Wacht,
                verwerkt(None),
                verwerkt(Some(25)),
                verwerkt(Some(50)),
                verwerkt(Some(75)),
                verwerkt(Some(100)),
                Toestand::Klaar,
            ]
        );
    }

    fn leeg_gesprek() -> Gesprek {
        Gesprek {
            beurten: Vec::new(),
            duur: Duration::from_secs(1),
            meldingen: Vec::new(),
            jij: Spoorstand::Opgenomen,
            anderen: Spoorstand::Ontbrak,
            weggelaten: 0,
            soort: Vergadersoort::Ruimte,
        }
    }

    fn opnieuw_uitslag(
        o: &Opzet,
        motor: Option<Box<dyn Bestandsmotor>>,
    ) -> (TaakId, Gespreksuitslag) {
        let (zender, ontvanger) = mpsc::channel();
        let id = o
            .wachtrij
            .voeg_opnieuw_toe_met("opname-c".into(), leeg_gesprek(), motor, move |_, u| {
                let _ = zender.send(u);
            })
            .unwrap();
        (id, gespreksuitslag(&ontvanger))
    }

    #[test]
    fn opnieuw_uitschrijven_met_de_motor_van_de_rij() {
        let o = opzet_gesprek();
        let (id, uitslag) = opnieuw_uitslag(&o, None);
        assert!(matches!(uitslag, Gespreksuitslag::Klaar(_)), "niet klaar");
        let Gespreksuitslag::Klaar(gesprek) = uitslag else {
            return;
        };
        assert_eq!(gesprek.meldingen, vec!["opnieuw".to_string()]);
        assert_eq!(o.wachtrij.toestand(id), Some(Toestand::Klaar));
        let alles: Vec<_> = o.gebeurtenissen.try_iter().collect();
        assert!(
            alles
                .iter()
                .filter(|g| g.taak == id)
                .all(|g| g.soort == Soort::Vergadering)
        );
    }

    #[test]
    fn opnieuw_met_een_eigen_motor_stopt_eerst_de_rij_en_daarna_de_eigen_motor() {
        let o = opzet_gesprek();
        let eigen = Arc::new(Stand::default());
        let motor = NepGesprekMotor {
            stand: Arc::clone(&eigen),
            binnen: NepMotor {
                stand: Arc::clone(&eigen),
            },
        };
        let gestopt_voor = o.stand.gestopt.load(Ordering::SeqCst);
        let (_, uitslag) = opnieuw_uitslag(&o, Some(Box::new(motor)));
        assert!(matches!(uitslag, Gespreksuitslag::Klaar(_)), "niet klaar");
        // De eigen motor deed het werk, de rij niet.
        assert_eq!(*slot(&eigen.uitgeschreven), vec![PathBuf::from("opname-c")]);
        assert!(slot(&o.stand.uitgeschreven).is_empty());
        assert!(o.stand.gestopt.load(Ordering::SeqCst) > gestopt_voor);
        assert!(eigen.gestopt.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn opnieuw_annuleren_geeft_geannuleerd_en_geen_gesprek() {
        let o = opzet_gesprek();
        // De rij bezig houden, zodat het opnieuw uitschrijven nog wacht als we annuleren.
        let (_, eerste) = o.wachtrij.voeg_toe("traag-1.mp4".into()).unwrap();
        let (zender, ontvanger) = mpsc::channel();
        let id = o
            .wachtrij
            .voeg_opnieuw_toe_met("opname-d".into(), leeg_gesprek(), None, move |_, u| {
                let _ = zender.send(u);
            })
            .unwrap();
        o.wachtrij.annuleer(id).unwrap();
        assert!(matches!(uitslag(&eerste), Bestandsuitslag::Klaar(_)));
        assert!(matches!(
            gespreksuitslag(&ontvanger),
            Gespreksuitslag::Geannuleerd
        ));
    }

    #[test]
    fn bestanden_en_vergaderingen_gaan_om_de_beurt_door_een_motor() {
        let o = opzet_gesprek();
        let (_, eerste) = o.wachtrij.voeg_toe("traag-1.mp4".into()).unwrap();
        let (tweede_id, tweede) = o.wachtrij.voeg_vergadering_toe("opname-b".into()).unwrap();
        let (_, derde) = o.wachtrij.voeg_toe("3.mp4".into()).unwrap();
        assert_eq!(o.wachtrij.toestand(tweede_id), Some(Toestand::Wacht));
        assert!(matches!(uitslag(&eerste), Bestandsuitslag::Klaar(_)));
        assert!(matches!(
            gespreksuitslag(&tweede),
            Gespreksuitslag::Klaar(_)
        ));
        assert!(matches!(uitslag(&derde), Bestandsuitslag::Klaar(_)));
        assert_eq!(
            *slot(&o.stand.uitgeschreven),
            vec![
                PathBuf::from("traag-1.mp4"),
                PathBuf::from("opname-b"),
                PathBuf::from("3.mp4")
            ]
        );
    }

    #[test]
    fn een_lopende_vergadering_annuleren_en_een_kapotte_geeft_een_fout() {
        let o = opzet_gesprek();
        let (id, eerste) = o
            .wachtrij
            .voeg_vergadering_toe("traag-opname".into())
            .unwrap();
        let (_, tweede) = o
            .wachtrij
            .voeg_vergadering_toe("kapot-opname".into())
            .unwrap();
        let (_, derde) = o.wachtrij.voeg_toe("daarna.mp4".into()).unwrap();
        let tot = std::time::Instant::now() + Duration::from_secs(5);
        while !matches!(o.wachtrij.toestand(id), Some(Toestand::Verwerkt { .. }))
            && std::time::Instant::now() < tot
        {
            thread::sleep(Duration::from_millis(2));
        }
        o.wachtrij.annuleer(id).unwrap();
        assert!(matches!(
            gespreksuitslag(&eerste),
            Gespreksuitslag::Geannuleerd
        ));
        assert!(matches!(
            gespreksuitslag(&tweede),
            Gespreksuitslag::Fout(AanbiederFout::Audio(AudioFout::Beschadigd))
        ));
        assert!(matches!(uitslag(&derde), Bestandsuitslag::Klaar(_)));
        assert_eq!(o.wachtrij.toestand(id), Some(Toestand::Geannuleerd));
    }

    #[test]
    fn een_motor_zonder_vergaderingen_geeft_een_fout_en_de_rij_gaat_door() {
        let o = opzet();
        let (id, eerste) = o.wachtrij.voeg_vergadering_toe("opname".into()).unwrap();
        let (_, tweede) = o.wachtrij.voeg_toe("a.mp4".into()).unwrap();
        assert!(matches!(
            gespreksuitslag(&eerste),
            Gespreksuitslag::Fout(AanbiederFout::Engine(EngineFout::Intern))
        ));
        assert!(matches!(uitslag(&tweede), Bestandsuitslag::Klaar(_)));
        assert!(matches!(
            o.wachtrij.toestand(id),
            Some(Toestand::Fout { .. })
        ));
    }
}
