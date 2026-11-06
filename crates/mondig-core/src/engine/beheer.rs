//! De motorbeheerder: het laadbeleid van de spraakengine (ADR 0012).
//!
//! Een eigen thread bezit de motor (in de app de [`LokaleEngine`]). De rest van de app
//! stuurt alleen opdrachten over een kanaal en wacht nooit op het werkproces:
//!
//! - [`Motorbeheerder::nieuw_dictaat`] begint een dictaat en warmt de motor op terwijl
//!   de gebruiker praat;
//! - [`Motorbeheerder::uitschrijven`] schrijft de opname uit en geeft een kanaal terug
//!   waarop de uitslag komt;
//! - [`Motorbeheerder::annuleer`] breekt een dictaat af. Loopt het uitschrijven al, dan
//!   wordt het resultaat genegeerd en blijft het werkproces draaien;
//! - bij `Drop` stopt de thread netjes en daarmee het werkproces.
//!
//! De [`Rusttijd`] bepaalt wanneer het werkproces stopt als er niets te doen is. De klok
//! loopt niet zolang er een dictaat bezig is (luistert of verwerkt). Bij
//! [`Rusttijd::Nooit`] ("Altijd") laadt de beheerder het model al bij het starten.
//!
//! Elke wijziging van een dictaat gaat als [`Gebeurtenis`] naar de interface, volgens
//! het toestandsmodel in [`crate::taak`]. Dat model beslist ook wat er gebeurt als
//! annuleren en klaarmelden elkaar kruisen: een geannuleerde taak wordt nooit meer klaar.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::aanbieder::{AanbiederFout, LokaleEngine};
use crate::engine::protocol::Apparaat;
use crate::engine::supervisor::EngineFout;
use crate::filter::{FilterInstellingen, Gefilterd};
use crate::taak::{Foutmelding, Gebeurtenis, OngeldigeOvergang, Soort, Taak, TaakId, Toestand};

/// De standaard rusttijd: vijf minuten (ADR 0012).
pub const STANDAARD_RUSTTIJD: Duration = Duration::from_mins(5);

/// Hoe lang het werkproces met het model blijft staan zonder dictaat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rusttijd {
    /// Het werkproces stopt zodra een dictaat uitgeschreven is.
    Meteen,
    /// Het werkproces stopt na zoveel tijd zonder dictaat.
    Duur(Duration),
    /// Het werkproces blijft draaien ("Altijd") en het model laadt al bij het starten.
    Nooit,
}

impl Default for Rusttijd {
    fn default() -> Self {
        Self::Duur(STANDAARD_RUSTTIJD)
    }
}

/// De stappen onder "Spraakmodel in het geheugen houden" (ADR 0012), met de sleutel
/// waaronder de keuze wordt bewaard. De interface kent dezelfde sleutels.
pub const RUSTTIJD_KEUZES: [(&str, Rusttijd); 7] = [
    ("meteen", Rusttijd::Meteen),
    ("2m", Rusttijd::Duur(Duration::from_mins(2))),
    ("5m", Rusttijd::Duur(STANDAARD_RUSTTIJD)),
    ("10m", Rusttijd::Duur(Duration::from_mins(10))),
    ("15m", Rusttijd::Duur(Duration::from_mins(15))),
    ("1u", Rusttijd::Duur(Duration::from_hours(1))),
    ("nooit", Rusttijd::Nooit),
];

impl Rusttijd {
    /// De rusttijd bij een bewaarde sleutel, of `None` als de sleutel niet bestaat.
    #[must_use]
    pub fn uit_sleutel(sleutel: &str) -> Option<Self> {
        RUSTTIJD_KEUZES
            .iter()
            .find(|(naam, _)| *naam == sleutel)
            .map(|(_, rusttijd)| *rusttijd)
    }
}

/// De motor die de beheerder bestuurt. In de app is dat de [`LokaleEngine`]; tests
/// gebruiken een eigen motor die telt en wacht.
pub trait Motor: Send + 'static {
    /// Laadt het model als dat nog niet geladen is.
    ///
    /// # Errors
    ///
    /// Geeft een [`EngineFout`] als het werkproces niet start of het model niet laadt.
    fn laad(&mut self) -> Result<Apparaat, EngineFout>;

    /// Draait het werkproces nu?
    fn draait(&self) -> bool;

    /// Stopt het werkproces; het model is daarna ontladen.
    fn stop(&mut self);

    /// Schrijft geluid uit het geheugen uit (16 kHz mono) en past het filter toe. Laadt
    /// het model zelf als dat nodig is. Met `knip_stilte` knipt het werkproces stilte
    /// aan begin en eind weg, als het daarvoor het hulpmodel heeft.
    ///
    /// # Errors
    ///
    /// Geeft een [`AanbiederFout`] in gewone taal als het uitschrijven mislukt.
    fn schrijf_uit(
        &mut self,
        samples: &[f32],
        filter: &FilterInstellingen,
        knip_stilte: bool,
    ) -> Result<Gefilterd, AanbiederFout>;
}

impl Motor for LokaleEngine {
    fn laad(&mut self) -> Result<Apparaat, EngineFout> {
        LokaleEngine::laad(self)
    }

    fn draait(&self) -> bool {
        LokaleEngine::draait(self)
    }

    fn stop(&mut self) {
        LokaleEngine::stop(self);
    }

    fn schrijf_uit(
        &mut self,
        samples: &[f32],
        filter: &FilterInstellingen,
        knip_stilte: bool,
    ) -> Result<Gefilterd, AanbiederFout> {
        self.transcribeer_dictaat(samples, filter, knip_stilte)
    }
}

/// Hoe een uitschrijving afliep.
#[derive(Debug)]
pub enum Uitslag {
    /// Gelukt; het filter kan toch "geen geluid" of "geen spraak" zeggen.
    Klaar(Gefilterd),
    /// De gebruiker heeft het dictaat geannuleerd; er is niets om te plakken.
    Geannuleerd,
    /// Mislukt, met een fout in gewone taal.
    Fout(AanbiederFout),
}

/// Wat er mis kan gaan bij een opdracht aan de beheerder.
#[derive(Debug, thiserror::Error)]
pub enum BeheerFout {
    /// Dit dictaat is niet bekend (of al lang afgelopen).
    #[error("Dit dictaat bestaat niet (meer).")]
    OnbekendeTaak(TaakId),
    /// Het dictaat is niet in een toestand waarin dit mag.
    #[error("Dit kan nu niet met dit dictaat.")]
    VerkeerdeToestand(#[from] OngeldigeOvergang),
    /// De beheerder is gestopt.
    #[error("De spraakengine is gestopt.")]
    Gestopt,
}

/// Wat de beheerder-thread moet doen.
enum Opdracht {
    Opwarmen,
    Uitschrijven {
        id: TaakId,
        samples: Vec<f32>,
        filter: FilterInstellingen,
        knip_stilte: bool,
        antwoord: Sender<Uitslag>,
    },
    /// Maakt de thread wakker, zodat de rusttijdklok opnieuw begint (na een annulering).
    Wakker,
    /// Een andere rusttijd, zonder herstart van het werkproces.
    ZetRusttijd(Rusttijd),
    Stop,
}

/// Wat de caller en de thread delen: de taken en de stroom gebeurtenissen.
struct Gedeeld {
    taken: Mutex<HashMap<TaakId, Taak>>,
    gebeurtenissen: Sender<Gebeurtenis>,
    volgend_id: AtomicU64,
}

impl Gedeeld {
    fn taken(&self) -> std::sync::MutexGuard<'_, HashMap<TaakId, Taak>> {
        self.taken.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Zet een taak in een nieuwe toestand en meldt dat. De melding gaat binnen het slot
    /// de deur uit, zodat de volgorde van de gebeurtenissen klopt.
    fn zet(&self, id: TaakId, naar: Toestand) -> Result<(), BeheerFout> {
        let mut taken = self.taken();
        let taak = taken.get_mut(&id).ok_or(BeheerFout::OnbekendeTaak(id))?;
        let gebeurtenis = taak.ga_naar(naar)?;
        // Is de interface weg, dan is er niemand om te vertellen; de taak gaat gewoon door.
        let _ = self.gebeurtenissen.send(gebeurtenis);
        Ok(())
    }

    fn toestand(&self, id: TaakId) -> Option<Toestand> {
        self.taken().get(&id).map(|taak| taak.toestand().clone())
    }

    /// Is er een dictaat dat nog loopt (luistert of verwerkt)?
    fn is_bezig(&self) -> bool {
        self.taken()
            .values()
            .any(|taak| taak.toestand().is_actief())
    }
}

/// Bestuurt de motor op een eigen thread (zie de module-uitleg).
pub struct Motorbeheerder {
    zender: Sender<Opdracht>,
    gedeeld: Arc<Gedeeld>,
    thread: Option<JoinHandle<()>>,
}

impl Motorbeheerder {
    /// Start de beheerder. Alle wijzigingen van dictaten komen op `gebeurtenissen`.
    ///
    /// Bij [`Rusttijd::Nooit`] begint de thread meteen met het laden van het model; deze
    /// functie wacht daar niet op.
    #[must_use]
    pub fn start<M: Motor>(
        motor: M,
        rusttijd: Rusttijd,
        gebeurtenissen: Sender<Gebeurtenis>,
    ) -> Self {
        let (zender, ontvanger) = mpsc::channel();
        let gedeeld = Arc::new(Gedeeld {
            taken: Mutex::new(HashMap::new()),
            gebeurtenissen,
            volgend_id: AtomicU64::new(1),
        });
        let voor_thread = Arc::clone(&gedeeld);
        let thread = thread::spawn(move || draai(motor, rusttijd, &voor_thread, &ontvanger));
        Self {
            zender,
            gedeeld,
            thread: Some(thread),
        }
    }

    /// Begint een dictaat: de taak gaat naar "luistert" en de motor warmt op terwijl de
    /// gebruiker praat. Wacht niet op het werkproces.
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout::Gestopt`] als de beheerder gestopt is.
    pub fn nieuw_dictaat(&self) -> Result<TaakId, BeheerFout> {
        let id = TaakId(self.gedeeld.volgend_id.fetch_add(1, Ordering::Relaxed));
        {
            let mut taken = self.gedeeld.taken();
            // Afgelopen taken zijn niet meer nodig; zo groeit de lijst niet eindeloos.
            taken.retain(|_, taak| taak.toestand().is_actief());
            taken.insert(id, Taak::nieuw(id, Soort::Dictaat));
        }
        self.gedeeld.zet(id, Toestand::Luistert)?;
        self.opwarmen()?;
        Ok(id)
    }

    /// Start het werkproces en laadt het model, zonder dat de aanroeper wacht. Lukt dat
    /// niet, dan komt de fout bij het uitschrijven, dat het opnieuw probeert.
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout::Gestopt`] als de beheerder gestopt is.
    pub fn opwarmen(&self) -> Result<(), BeheerFout> {
        self.zender
            .send(Opdracht::Opwarmen)
            .map_err(|_| BeheerFout::Gestopt)
    }

    /// Schrijft de opname van een dictaat uit. De taak gaat naar "verwerkt"; de uitslag
    /// komt op het teruggegeven kanaal.
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout`] als het dictaat onbekend is, niet (meer) loopt of als de
    /// beheerder gestopt is.
    pub fn uitschrijven(
        &self,
        id: TaakId,
        samples: Vec<f32>,
        filter: FilterInstellingen,
    ) -> Result<Receiver<Uitslag>, BeheerFout> {
        self.uitschrijven_met(id, samples, filter, false)
    }

    /// Als [`Motorbeheerder::uitschrijven`], met de keuze om stilte aan begin en eind van
    /// de opname weg te knippen voordat het model het hoort (Silero, in het werkproces).
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout`] als het dictaat onbekend is, niet (meer) loopt of als de
    /// beheerder gestopt is.
    pub fn uitschrijven_met(
        &self,
        id: TaakId,
        samples: Vec<f32>,
        filter: FilterInstellingen,
        knip_stilte: bool,
    ) -> Result<Receiver<Uitslag>, BeheerFout> {
        self.gedeeld.zet(id, Toestand::verwerkt(None))?;
        let (antwoord, uitslag) = mpsc::channel();
        let opdracht = Opdracht::Uitschrijven {
            id,
            samples,
            filter,
            knip_stilte,
            antwoord,
        };
        if self.zender.send(opdracht).is_err() {
            // Het dictaat blijft anders eeuwig op "verwerkt" staan.
            let _ = self.gedeeld.zet(
                id,
                Toestand::fout("De spraakengine is gestopt.", crate::taak::Vervolg::Geen),
            );
            return Err(BeheerFout::Gestopt);
        }
        Ok(uitslag)
    }

    /// Annuleert een dictaat. Loopt het uitschrijven al, dan wordt het resultaat genegeerd;
    /// het werkproces blijft draaien, zodat het volgende dictaat niet opnieuw hoeft te laden.
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout`] als het dictaat onbekend of al afgelopen is.
    pub fn annuleer(&self, id: TaakId) -> Result<(), BeheerFout> {
        self.gedeeld.zet(id, Toestand::Geannuleerd)?;
        // Is de thread weg, dan is er ook geen klok om te herstarten.
        let _ = self.zender.send(Opdracht::Wakker);
        Ok(())
    }

    /// Wijzigt de rusttijd van een draaiende beheerder, zonder herstart (de instelling
    /// "Spraakmodel in het geheugen houden"). De klok loopt door vanaf het laatste dictaat:
    /// is de nieuwe rusttijd al voorbij, dan stopt het werkproces meteen. Bij
    /// [`Rusttijd::Nooit`] laadt het model nu.
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout::Gestopt`] als de beheerder gestopt is.
    pub fn zet_rusttijd(&self, rusttijd: Rusttijd) -> Result<(), BeheerFout> {
        self.zender
            .send(Opdracht::ZetRusttijd(rusttijd))
            .map_err(|_| BeheerFout::Gestopt)
    }

    /// Laat een dictaat mislukken voordat het uitgeschreven kan worden, bijvoorbeeld omdat
    /// het opnamebestand niet te lezen is. De taak gaat naar "fout" en de interface krijgt
    /// die gebeurtenis, in plaats van dat het dictaat eeuwig op "luistert" blijft staan
    /// of als "geannuleerd" te boek komt.
    ///
    /// # Errors
    ///
    /// Geeft [`BeheerFout`] als het dictaat onbekend of al afgelopen is.
    pub fn mislukt(&self, id: TaakId, fout: Foutmelding) -> Result<(), BeheerFout> {
        self.gedeeld
            .zet(id, Toestand::fout(fout.melding, fout.vervolg))?;
        // Zoals bij annuleren: de rusttijdklok begint opnieuw.
        let _ = self.zender.send(Opdracht::Wakker);
        Ok(())
    }

    /// De huidige toestand van een dictaat, of `None` als het niet (meer) bekend is.
    #[must_use]
    pub fn toestand(&self, id: TaakId) -> Option<Toestand> {
        self.gedeeld.toestand(id)
    }

    /// Stopt de thread en het werkproces en wacht tot dat gebeurd is.
    pub fn stop(mut self) {
        self.afsluiten();
    }

    fn afsluiten(&mut self) {
        let _ = self.zender.send(Opdracht::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Motorbeheerder {
    fn drop(&mut self) {
        self.afsluiten();
    }
}

/// De lus van de beheerder-thread.
fn draai<M: Motor>(
    mut motor: M,
    mut rusttijd: Rusttijd,
    gedeeld: &Gedeeld,
    ontvanger: &Receiver<Opdracht>,
) {
    if rusttijd == Rusttijd::Nooit {
        // "Altijd": laden bij het starten. Een fout komt bij het eerste dictaat.
        let _ = motor.laad();
    }
    let mut sinds = Instant::now();
    loop {
        // De klok loopt alleen als het model geladen is en er geen dictaat bezig is.
        let wachttijd = if motor.draait() && !gedeeld.is_bezig() {
            match rusttijd {
                Rusttijd::Nooit => None,
                Rusttijd::Meteen => Some(Duration::ZERO),
                Rusttijd::Duur(duur) => Some(duur.saturating_sub(sinds.elapsed())),
            }
        } else {
            None
        };
        let opdracht = match wachttijd {
            None => ontvanger.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(tijd) => ontvanger.recv_timeout(tijd),
        };
        match opdracht {
            Ok(Opdracht::Opwarmen) => {
                let _ = motor.laad();
            }
            Ok(Opdracht::Wakker) => {}
            Ok(Opdracht::ZetRusttijd(nieuw)) => {
                rusttijd = nieuw;
                if rusttijd == Rusttijd::Nooit {
                    // "Altijd" laadt meteen, niet pas bij het volgende dictaat.
                    let _ = motor.laad();
                }
                // De klok blijft lopen vanaf het laatste dictaat: `sinds` blijft staan.
                continue;
            }
            Ok(Opdracht::Uitschrijven {
                id,
                samples,
                filter,
                knip_stilte,
                antwoord,
            }) => {
                let uitslag = voer_uit(&mut motor, gedeeld, id, &samples, &filter, knip_stilte);
                // Is de ontvanger weg, dan is de uitslag niet meer nodig.
                let _ = antwoord.send(uitslag);
            }
            Ok(Opdracht::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => motor.stop(),
        }
        sinds = Instant::now();
    }
    motor.stop();
}

/// Schrijft één dictaat uit en meldt het resultaat. Is het dictaat intussen
/// geannuleerd, dan wordt het resultaat genegeerd.
fn voer_uit<M: Motor>(
    motor: &mut M,
    gedeeld: &Gedeeld,
    id: TaakId,
    samples: &[f32],
    filter: &FilterInstellingen,
    knip_stilte: bool,
) -> Uitslag {
    // Geannuleerd voordat het werk begon: het werkproces hoeft er niet aan te beginnen.
    if !matches!(gedeeld.toestand(id), Some(Toestand::Verwerkt { .. })) {
        return Uitslag::Geannuleerd;
    }
    let resultaat = motor.schrijf_uit(samples, filter, knip_stilte);
    let naar = match &resultaat {
        Ok(_) => Toestand::Klaar,
        Err(fout) => Toestand::uit_aanbieder_fout(fout),
    };
    match (gedeeld.zet(id, naar), resultaat) {
        (Ok(()), Ok(gefilterd)) => Uitslag::Klaar(gefilterd),
        (Ok(()), Err(fout)) => Uitslag::Fout(fout),
        // De overgang mocht niet: de gebruiker annuleerde tijdens het uitschrijven.
        (Err(_), _) => Uitslag::Geannuleerd,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize};

    use super::*;
    use crate::aanbieder::Transcript;
    use crate::filter::{self, Uitkomst};
    use crate::taak::Vervolg;

    /// Wat een nepmotor bijhoudt, zodat een test erbij kan.
    #[derive(Default)]
    struct Stand {
        geladen: AtomicUsize,
        gestopt: AtomicUsize,
        uitgeschreven: AtomicUsize,
        draait: AtomicBool,
        /// Per uitschrijving: was om stilte wegknippen gevraagd?
        knip_aanvragen: Mutex<Vec<bool>>,
    }

    struct NepMotor {
        stand: Arc<Stand>,
        laadduur: Duration,
        schrijfduur: Duration,
        faalt: bool,
    }

    impl Motor for NepMotor {
        fn laad(&mut self) -> Result<Apparaat, EngineFout> {
            if !self.stand.draait.load(Ordering::SeqCst) {
                thread::sleep(self.laadduur);
                self.stand.geladen.fetch_add(1, Ordering::SeqCst);
                self.stand.draait.store(true, Ordering::SeqCst);
            }
            Ok(Apparaat::Cpu)
        }

        fn draait(&self) -> bool {
            self.stand.draait.load(Ordering::SeqCst)
        }

        fn stop(&mut self) {
            if self.stand.draait.swap(false, Ordering::SeqCst) {
                self.stand.gestopt.fetch_add(1, Ordering::SeqCst);
            }
        }

        fn schrijf_uit(
            &mut self,
            _samples: &[f32],
            instellingen: &FilterInstellingen,
            knip_stilte: bool,
        ) -> Result<Gefilterd, AanbiederFout> {
            self.stand.knip_aanvragen.lock().unwrap().push(knip_stilte);
            self.laad()?;
            thread::sleep(self.schrijfduur);
            self.stand.uitgeschreven.fetch_add(1, Ordering::SeqCst);
            if self.faalt {
                return Err(AanbiederFout::Engine(EngineFout::Gecrasht {
                    code: Some(139),
                    laatste_log: Vec::new(),
                }));
            }
            let transcript = Transcript {
                segmenten: vec![crate::engine::protocol::Segment::nieuw(
                    0,
                    1000,
                    "Dit is een dictaat.",
                )],
                audio_duur: Duration::from_secs(1),
            };
            let meting = crate::filter::Meting {
                rms: 0.1,
                frame_rms: vec![0.1; 34],
            };
            Ok(filter::pas_toe(transcript, &meting, instellingen))
        }
    }

    struct Opstelling {
        stand: Arc<Stand>,
        beheerder: Motorbeheerder,
        gebeurtenissen: Receiver<Gebeurtenis>,
    }

    fn opstelling(
        rusttijd: Rusttijd,
        laadduur_ms: u64,
        schrijfduur_ms: u64,
        faalt: bool,
    ) -> Opstelling {
        let stand = Arc::new(Stand::default());
        let motor = NepMotor {
            stand: Arc::clone(&stand),
            laadduur: Duration::from_millis(laadduur_ms),
            schrijfduur: Duration::from_millis(schrijfduur_ms),
            faalt,
        };
        let (zender, gebeurtenissen) = mpsc::channel();
        Opstelling {
            beheerder: Motorbeheerder::start(motor, rusttijd, zender),
            stand,
            gebeurtenissen,
        }
    }

    fn rust_ms(ms: u64) -> Rusttijd {
        Rusttijd::Duur(Duration::from_millis(ms))
    }

    /// Wacht tot de voorwaarde waar is (hooguit vijf seconden).
    fn wacht_tot(voorwaarde: impl Fn() -> bool) -> bool {
        let tot = Instant::now() + Duration::from_secs(5);
        while Instant::now() < tot {
            if voorwaarde() {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        voorwaarde()
    }

    fn toestanden(ontvanger: &Receiver<Gebeurtenis>) -> Vec<Toestand> {
        ontvanger.try_iter().map(|g| g.toestand).collect()
    }

    fn een_seconde() -> Vec<f32> {
        vec![0.1; 16_000]
    }

    #[test]
    fn standaard_rusttijd_is_vijf_minuten() {
        assert_eq!(
            Rusttijd::default(),
            Rusttijd::Duur(Duration::from_secs(300))
        );
    }

    #[test]
    fn opwarmen_wacht_niet_op_het_laden() {
        // Een nepmotor met trage laadtijd: de aanroeper moet er niets van merken.
        let trage = opstelling(rust_ms(60_000), 400, 0, false);
        let begin = Instant::now();
        let id = trage.beheerder.nieuw_dictaat().unwrap();
        assert!(
            begin.elapsed() < Duration::from_millis(250),
            "nieuw_dictaat wachtte op het laden: {:?}",
            begin.elapsed()
        );
        assert_eq!(trage.beheerder.toestand(id), Some(Toestand::Luistert));
        assert!(!trage.stand.draait.load(Ordering::SeqCst));
        assert!(wacht_tot(|| trage.stand.draait.load(Ordering::SeqCst)));
        assert_eq!(trage.stand.geladen.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn uitschrijven_meldt_de_toestanden_en_geeft_de_uitslag() {
        let o = opstelling(rust_ms(60_000), 0, 0, false);
        let id = o.beheerder.nieuw_dictaat().unwrap();
        let uitslag = o
            .beheerder
            .uitschrijven(id, een_seconde(), FilterInstellingen::standaard())
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let Uitslag::Klaar(gefilterd) = uitslag else {
            unreachable!("verwachtte Klaar, kreeg {uitslag:?}");
        };
        assert_eq!(gefilterd.uitkomst, Uitkomst::Tekst);
        assert_eq!(
            toestanden(&o.gebeurtenissen),
            vec![
                Toestand::Luistert,
                Toestand::verwerkt(None),
                Toestand::Klaar
            ]
        );
        assert_eq!(o.beheerder.toestand(id), Some(Toestand::Klaar));
    }

    #[test]
    fn de_keuze_om_stilte_te_knippen_bereikt_de_motor() {
        let o = opstelling(rust_ms(60_000), 0, 0, false);
        for knip in [None, Some(false), Some(true)] {
            let id = o.beheerder.nieuw_dictaat().unwrap();
            let ontvanger = match knip {
                None => {
                    o.beheerder
                        .uitschrijven(id, een_seconde(), FilterInstellingen::standaard())
                }
                Some(knip) => o.beheerder.uitschrijven_met(
                    id,
                    een_seconde(),
                    FilterInstellingen::standaard(),
                    knip,
                ),
            }
            .unwrap();
            ontvanger.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        // Zonder keuze knipt de beheerder niet: de bestaande aanroepers veranderen niet.
        assert_eq!(
            *o.stand.knip_aanvragen.lock().unwrap(),
            vec![false, false, true]
        );
    }

    #[test]
    fn gebeurtenissen_dragen_taak_en_soort() {
        let o = opstelling(rust_ms(60_000), 0, 0, false);
        let id = o.beheerder.nieuw_dictaat().unwrap();
        let gebeurtenis = o
            .gebeurtenissen
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        assert_eq!(gebeurtenis.taak, id);
        assert_eq!(gebeurtenis.soort, Soort::Dictaat);
        // Een tweede dictaat krijgt een ander id.
        let tweede = o.beheerder.nieuw_dictaat().unwrap();
        assert_ne!(id, tweede);
    }

    #[test]
    fn een_fout_wordt_een_melding_met_vervolgactie() {
        let o = opstelling(rust_ms(60_000), 0, 0, true);
        let id = o.beheerder.nieuw_dictaat().unwrap();
        let uitslag = o
            .beheerder
            .uitschrijven(id, een_seconde(), FilterInstellingen::standaard())
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(matches!(uitslag, Uitslag::Fout(_)), "{uitslag:?}");
        let laatste = toestanden(&o.gebeurtenissen).pop().unwrap();
        let Toestand::Fout { melding, vervolg } = laatste else {
            unreachable!("verwachtte Fout, kreeg {laatste:?}");
        };
        assert_eq!(vervolg, Vervolg::OpnieuwProberen);
        assert!(melding.contains("onverwacht gestopt"), "{melding}");
    }

    #[test]
    fn rusttijd_stopt_het_werkproces() {
        let o = opstelling(rust_ms(300), 0, 0, false);
        let id = o.beheerder.nieuw_dictaat().unwrap();
        o.beheerder
            .uitschrijven(id, een_seconde(), FilterInstellingen::standaard())
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(o.stand.draait.load(Ordering::SeqCst), "nog niet gestopt");
        assert!(wacht_tot(|| !o.stand.draait.load(Ordering::SeqCst)));
        assert_eq!(o.stand.gestopt.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn rusttijd_loopt_niet_tijdens_het_uitschrijven() {
        // Het uitschrijven duurt vier keer zo lang als de rusttijd.
        let o = opstelling(rust_ms(100), 0, 400, false);
        let id = o.beheerder.nieuw_dictaat().unwrap();
        let uitslag = o
            .beheerder
            .uitschrijven(id, een_seconde(), FilterInstellingen::standaard())
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(matches!(uitslag, Uitslag::Klaar(_)), "{uitslag:?}");
        assert_eq!(
            o.stand.gestopt.load(Ordering::SeqCst),
            0,
            "te vroeg gestopt"
        );
        assert_eq!(o.stand.geladen.load(Ordering::SeqCst), 1, "opnieuw geladen");
        // Pas daarna loopt de klok.
        assert!(wacht_tot(|| !o.stand.draait.load(Ordering::SeqCst)));
    }

    #[test]
    fn rusttijd_loopt_niet_terwijl_er_geluisterd_wordt() {
        let o = opstelling(rust_ms(100), 0, 0, false);
        let id = o.beheerder.nieuw_dictaat().unwrap();
        assert!(wacht_tot(|| o.stand.draait.load(Ordering::SeqCst)));
        thread::sleep(Duration::from_millis(400));
        assert!(o.stand.draait.load(Ordering::SeqCst));
        assert_eq!(o.stand.gestopt.load(Ordering::SeqCst), 0);
        // Na annuleren begint de klok te lopen.
        o.beheerder.annuleer(id).unwrap();
        assert!(wacht_tot(|| !o.stand.draait.load(Ordering::SeqCst)));
    }

    #[test]
    fn meteen_stopt_na_het_uitschrijven_maar_niet_ervoor() {
        let o = opstelling(Rusttijd::Meteen, 0, 0, false);
        let id = o.beheerder.nieuw_dictaat().unwrap();
        assert!(wacht_tot(|| o.stand.draait.load(Ordering::SeqCst)));
        thread::sleep(Duration::from_millis(100));
        assert!(
            o.stand.draait.load(Ordering::SeqCst),
            "stopte tijdens luisteren"
        );
        o.beheerder
            .uitschrijven(id, een_seconde(), FilterInstellingen::standaard())
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(wacht_tot(|| !o.stand.draait.load(Ordering::SeqCst)));
    }

    #[test]
    fn nooit_laadt_meteen_en_stopt_niet() {
        let o = opstelling(Rusttijd::Nooit, 0, 0, false);
        // Zonder dat er een dictaat is.
        assert!(wacht_tot(|| o.stand.draait.load(Ordering::SeqCst)));
        assert_eq!(o.stand.geladen.load(Ordering::SeqCst), 1);
        let id = o.beheerder.nieuw_dictaat().unwrap();
        o.beheerder
            .uitschrijven(id, een_seconde(), FilterInstellingen::standaard())
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        thread::sleep(Duration::from_millis(200));
        assert!(o.stand.draait.load(Ordering::SeqCst));
        assert_eq!(o.stand.gestopt.load(Ordering::SeqCst), 0);
        assert_eq!(o.stand.geladen.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn annuleren_tijdens_het_uitschrijven_negeert_het_resultaat() {
        let o = opstelling(rust_ms(60_000), 0, 500, false);
        let id = o.beheerder.nieuw_dictaat().unwrap();
        let uitslag = o
            .beheerder
            .uitschrijven(id, een_seconde(), FilterInstellingen::standaard())
            .unwrap();
        thread::sleep(Duration::from_millis(100));
        o.beheerder.annuleer(id).unwrap();
        let uitslag = uitslag.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(uitslag, Uitslag::Geannuleerd), "{uitslag:?}");
        assert_eq!(
            toestanden(&o.gebeurtenissen),
            vec![
                Toestand::Luistert,
                Toestand::verwerkt(None),
                Toestand::Geannuleerd
            ],
            "geen Klaar na annuleren"
        );
        // Het werkproces blijft draaien, zodat het volgende dictaat niet hoeft te laden.
        assert!(o.stand.draait.load(Ordering::SeqCst));
        assert_eq!(o.stand.gestopt.load(Ordering::SeqCst), 0);
        let volgend = o.beheerder.nieuw_dictaat().unwrap();
        o.beheerder
            .uitschrijven(volgend, een_seconde(), FilterInstellingen::standaard())
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(o.stand.geladen.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn annuleren_voor_het_uitschrijven_laat_de_motor_met_rust() {
        let o = opstelling(rust_ms(60_000), 0, 0, false);
        let id = o.beheerder.nieuw_dictaat().unwrap();
        o.beheerder.annuleer(id).unwrap();
        assert_eq!(
            toestanden(&o.gebeurtenissen),
            vec![Toestand::Luistert, Toestand::Geannuleerd]
        );
        // Uitschrijven van een geannuleerd dictaat mag niet, en annuleren nog eens ook niet.
        let fout = o
            .beheerder
            .uitschrijven(id, een_seconde(), FilterInstellingen::standaard())
            .unwrap_err();
        assert!(matches!(fout, BeheerFout::VerkeerdeToestand(_)), "{fout:?}");
        assert!(matches!(
            o.beheerder.annuleer(id),
            Err(BeheerFout::VerkeerdeToestand(_))
        ));
        assert_eq!(o.stand.uitgeschreven.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn mislukt_zet_het_dictaat_op_fout_en_laat_de_motor_met_rust() {
        let o = opstelling(rust_ms(60_000), 0, 0, false);
        let id = o.beheerder.nieuw_dictaat().unwrap();
        o.beheerder
            .mislukt(
                id,
                Foutmelding::nieuw("Dit bestand bevat geen geluid.", Vervolg::Geen),
            )
            .unwrap();
        assert_eq!(
            toestanden(&o.gebeurtenissen),
            vec![
                Toestand::Luistert,
                Toestand::fout("Dit bestand bevat geen geluid.", Vervolg::Geen)
            ]
        );
        // Daarna is het dictaat afgelopen: niet meer uitschrijven of annuleren.
        assert!(matches!(
            o.beheerder.annuleer(id),
            Err(BeheerFout::VerkeerdeToestand(_))
        ));
        assert_eq!(o.stand.uitgeschreven.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn onbekend_dictaat_wordt_geweigerd() {
        let o = opstelling(rust_ms(60_000), 0, 0, false);
        assert!(matches!(
            o.beheerder.annuleer(TaakId(99)),
            Err(BeheerFout::OnbekendeTaak(TaakId(99)))
        ));
        assert!(matches!(
            o.beheerder
                .uitschrijven(TaakId(99), een_seconde(), FilterInstellingen::standaard()),
            Err(BeheerFout::OnbekendeTaak(_))
        ));
        assert_eq!(o.beheerder.toestand(TaakId(99)), None);
    }

    #[test]
    fn drop_stopt_het_werkproces_netjes() {
        let o = opstelling(Rusttijd::Nooit, 0, 0, false);
        assert!(wacht_tot(|| o.stand.draait.load(Ordering::SeqCst)));
        let Opstelling {
            stand, beheerder, ..
        } = o;
        drop(beheerder);
        assert!(!stand.draait.load(Ordering::SeqCst));
        assert_eq!(stand.gestopt.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn stop_wacht_tot_alles_gestopt_is() {
        let o = opstelling(Rusttijd::Nooit, 0, 0, false);
        let Opstelling {
            stand, beheerder, ..
        } = o;
        beheerder.stop();
        assert!(!stand.draait.load(Ordering::SeqCst));
    }

    #[test]
    fn de_keuzes_hebben_unieke_sleutels_en_de_standaard_is_vijf_minuten() {
        let mut sleutels: Vec<&str> = RUSTTIJD_KEUZES.iter().map(|(naam, _)| *naam).collect();
        sleutels.sort_unstable();
        sleutels.dedup();
        assert_eq!(sleutels.len(), RUSTTIJD_KEUZES.len());
        assert_eq!(Rusttijd::uit_sleutel("5m"), Some(Rusttijd::default()));
        assert_eq!(Rusttijd::uit_sleutel("meteen"), Some(Rusttijd::Meteen));
        assert_eq!(Rusttijd::uit_sleutel("nooit"), Some(Rusttijd::Nooit));
        assert_eq!(
            Rusttijd::uit_sleutel("1u"),
            Some(Rusttijd::Duur(Duration::from_secs(3600)))
        );
        assert_eq!(Rusttijd::uit_sleutel("onzin"), None);
    }

    #[test]
    fn kortere_rusttijd_werkt_direct_zonder_herstart() {
        // Eerst een uur: het werkproces blijft staan. Daarna 50 ms: het stopt meteen.
        let o = opstelling(rust_ms(3_600_000), 0, 0, false);
        o.beheerder.opwarmen().unwrap();
        assert!(wacht_tot(|| o.stand.draait.load(Ordering::SeqCst)));
        thread::sleep(Duration::from_millis(150));
        assert!(o.stand.draait.load(Ordering::SeqCst));
        o.beheerder.zet_rusttijd(rust_ms(50)).unwrap();
        assert!(wacht_tot(|| !o.stand.draait.load(Ordering::SeqCst)));
        assert_eq!(o.stand.geladen.load(Ordering::SeqCst), 1);
        assert_eq!(o.stand.gestopt.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn meteen_zetten_stopt_een_wachtend_werkproces_direct() {
        let o = opstelling(rust_ms(3_600_000), 0, 0, false);
        o.beheerder.opwarmen().unwrap();
        assert!(wacht_tot(|| o.stand.draait.load(Ordering::SeqCst)));
        o.beheerder.zet_rusttijd(Rusttijd::Meteen).unwrap();
        assert!(wacht_tot(|| !o.stand.draait.load(Ordering::SeqCst)));
    }

    #[test]
    fn altijd_klaar_zetten_laadt_meteen_en_stopt_niet_meer() {
        let o = opstelling(rust_ms(60), 0, 0, false);
        // Het model is nog niet geladen: er is niets om te laden of te stoppen.
        assert!(!o.stand.draait.load(Ordering::SeqCst));
        o.beheerder.zet_rusttijd(Rusttijd::Nooit).unwrap();
        assert!(wacht_tot(|| o.stand.draait.load(Ordering::SeqCst)));
        thread::sleep(Duration::from_millis(250));
        assert!(o.stand.draait.load(Ordering::SeqCst));
        assert_eq!(o.stand.gestopt.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn van_altijd_klaar_terug_naar_een_rusttijd_laat_het_werkproces_weer_stoppen() {
        let o = opstelling(Rusttijd::Nooit, 0, 0, false);
        assert!(wacht_tot(|| o.stand.draait.load(Ordering::SeqCst)));
        o.beheerder.zet_rusttijd(rust_ms(50)).unwrap();
        assert!(wacht_tot(|| !o.stand.draait.load(Ordering::SeqCst)));
    }

    #[test]
    fn rusttijd_wijzigen_tijdens_een_dictaat_stopt_het_werkproces_niet() {
        let o = opstelling(rust_ms(3_600_000), 0, 200, false);
        let id = o.beheerder.nieuw_dictaat().unwrap();
        assert!(wacht_tot(|| o.stand.draait.load(Ordering::SeqCst)));
        // Het dictaat luistert nog: ook "meteen" mag het werkproces dan niet stoppen.
        o.beheerder.zet_rusttijd(Rusttijd::Meteen).unwrap();
        thread::sleep(Duration::from_millis(150));
        assert!(o.stand.draait.load(Ordering::SeqCst));
        let uitslag = o
            .beheerder
            .uitschrijven(id, een_seconde(), FilterInstellingen::standaard())
            .unwrap();
        assert!(matches!(uitslag.recv().unwrap(), Uitslag::Klaar(_)));
        assert!(wacht_tot(|| !o.stand.draait.load(Ordering::SeqCst)));
    }
}
