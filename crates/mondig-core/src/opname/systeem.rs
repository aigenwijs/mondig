//! De tweede bron van de opnamelaag: het geluid dat de computer zelf afspeelt (de anderen in
//! een vergadering), via cpal (ADR 0013, aanvulling 4b-A).
//!
//! `handy-recorder` kan dit niet: hij somt alleen invoerapparaten op en vraagt de
//! invoerinstelling van een apparaat, die een afspeelapparaat niet heeft. cpal neemt op
//! Windows het geluid van een afspeelapparaat op als je dat als invoer opent
//! (WASAPI-loopback). We gebruiken dezelfde cpal als `handy-recorder` (`handy-cpal`
//! 0.18.2), zodat er één versie in de app zit.
//!
//! Wat deze bron anders doet dan de microfoon:
//!
//! - Hij geeft bij elk blok mee wanneer het eerste sample werd opgenomen
//!   ([`Uitgang::ontvang_op`]), volgens de klok van de geluidskaart omgerekend naar
//!   [`Instant`]. Zo komt het spoor exact op de tijdlijn van de opname.
//! - Speelt de computer niets af, dan levert Windows niets. De schrijver vult zulke gaten
//!   met stilte (zie [`stukken`](super::stukken)).
//! - Opent hij het standaardapparaat en kiest de gebruiker tijdens de opname een ander
//!   (koptelefoon in), dan gaat hij verder op het nieuwe standaardapparaat. Is dat even
//!   niet te openen (een Bluetooth-koptelefoon is vaak pas na enkele seconden klaar), dan
//!   blijft hij tot [`HEROPEN_MAX`] met oplopende wachttijd proberen; het gat wordt stilte.
//!
//! Op macOS en Linux kan dit nog niet (fase 4b, later): daar geeft [`SysteemApparatuur`]
//! een fout van de soort [`FoutSoort::SysteemgeluidNietMogelijk`].
//!
//! Daarnaast speelt [`SysteemApparatuur::speel_af`] een kort testgeluid op een gekozen
//! afspeelapparaat (fase UX-C), via dezelfde cpal: zo hoort de gebruiker of het juiste
//! apparaat gekozen is, en beweegt de meter van het systeemgeluid.

#[cfg(any(windows, test))]
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
#[cfg(any(windows, test))]
use std::time::Instant;

use super::{Bron, FoutSoort, OpnameFout, UitvalMelder, kies_op_naam};
#[cfg(windows)]
use super::{Einde, Uitgang};
use crate::taak::{Foutmelding, Vervolg};

/// Een luidspreker of koptelefoon waarvan Mondig het geluid kan opnemen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Afspeelapparaat {
    /// Herkenning om het apparaat te openen.
    pub id: String,
    /// Naam in gewone taal.
    pub naam: String,
    /// Is dit het apparaat dat het systeem nu als standaard kiest?
    pub standaard: bool,
}

/// Zoekt een afspeelapparaat op id of naam, zoals [`kies_microfoon`](super::kies_microfoon).
///
/// # Errors
///
/// Geeft een [`Foutmelding`] als er niets of meer dan één past.
pub fn kies_afspeelapparaat<'a>(
    lijst: &'a [Afspeelapparaat],
    zoek: &str,
) -> Result<&'a Afspeelapparaat, Foutmelding> {
    kies_op_naam(
        lijst,
        zoek,
        |a| (a.id.as_str(), a.naam.as_str()),
        "afspeelapparaat",
        Vervolg::Geen,
    )
}

/// De afspeelapparaten van deze computer, om hun geluid op te nemen.
#[derive(Debug, Clone, Copy, Default)]
pub struct SysteemApparatuur;

impl SysteemApparatuur {
    /// De afspeelapparaten die nu aangesloten zijn.
    ///
    /// # Errors
    ///
    /// Geeft een [`OpnameFout`] als het systeem ze niet wil opsommen, of als dit
    /// besturingssysteem het nog niet kan.
    pub fn afspeelapparaten(&self) -> Result<Vec<Afspeelapparaat>, OpnameFout> {
        imp::afspeelapparaten()
    }

    /// Opent het geluid van een afspeelapparaat (`None` is de standaard van het systeem;
    /// die volgt de keuze van de gebruiker tijdens de opname).
    ///
    /// # Errors
    ///
    /// Geeft een [`OpnameFout`] met [`FoutSoort::GeenAfspeelapparaat`] als er geen (zo'n)
    /// apparaat is.
    pub fn open(
        &self,
        apparaat: Option<&str>,
        bij_uitval: UitvalMelder,
    ) -> Result<Box<dyn Bron>, OpnameFout> {
        imp::open(apparaat, bij_uitval)
    }

    /// Speelt een geluid af op een afspeelapparaat (`None` is de standaard) en wacht tot
    /// het klaar is. `maak` krijgt de samplerate van het apparaat en geeft de samples (mono,
    /// -1 tot 1); elk kanaal krijgt hetzelfde. Bedoeld voor een kort testgeluid: de samples
    /// staan in het geheugen tot het afspelen klaar is.
    ///
    /// # Errors
    ///
    /// Geeft een [`OpnameFout`] met [`FoutSoort::TestgeluidAnders`] als het apparaat er niet
    /// is of niet wil afspelen, en [`FoutSoort::TestgeluidNietMogelijk`] op een systeem waar
    /// dit nog niet kan.
    pub fn speel_af(
        &self,
        apparaat: Option<&str>,
        maak: impl FnOnce(u32) -> Vec<f32>,
    ) -> Result<(), OpnameFout> {
        imp::speel_af(apparaat, maak)
    }
}

/// Zo lang blijft de bron na een wissel van het standaardafspeelapparaat proberen het nieuwe
/// te openen, met oplopende wachttijd. Daarna valt het systeemspoor uit. Een
/// Bluetooth-koptelefoon is vaak pas na 2 tot 5 seconden te openen; 15 seconden laat ruimte
/// voor een trage.
pub const HEROPEN_MAX: Duration = Duration::from_secs(15);

/// Wachttijd voor de eerste poging; die verdubbelt tot [`HEROPEN_STAP_MAX`].
#[cfg(any(windows, test))]
const HEROPEN_BEGIN: Duration = Duration::from_millis(250);

/// De langste wachttijd tussen twee pogingen.
#[cfg(any(windows, test))]
const HEROPEN_STAP_MAX: Duration = Duration::from_secs(2);

/// Hoe vaak we tijdens het wachten kijken of de gebruiker stopte.
#[cfg(any(windows, test))]
const STOP_CONTROLE: Duration = Duration::from_millis(50);

/// Uitkomst van [`heropen_met`].
#[cfg(any(windows, test))]
#[derive(Debug)]
enum Heropend<T> {
    /// Het nieuwe apparaat is open.
    Open(T),
    /// De gebruiker stopte tijdens het wachten: een gewone stop, geen uitval.
    Gestopt,
    /// Het lukte niet binnen de tijd; de laatste fout.
    Opgegeven(OpnameFout),
}

/// Probeert `poging` met oplopende wachttijd tot `max` verstreken is, en kijkt tijdens het
/// wachten of er gestopt moet worden.
#[cfg(any(windows, test))]
fn heropen_met<T>(
    stoppen: &AtomicBool,
    max: Duration,
    begin_wacht: Duration,
    mut poging: impl FnMut() -> Result<T, OpnameFout>,
) -> Heropend<T> {
    let tot = Instant::now() + max;
    let mut wacht = begin_wacht;
    let mut laatste = loop {
        let wakker = Instant::now() + wacht;
        while let Some(rest) = wakker.checked_duration_since(Instant::now()) {
            if stoppen.load(Ordering::SeqCst) {
                return Heropend::Gestopt;
            }
            std::thread::sleep(rest.min(STOP_CONTROLE));
        }
        if stoppen.load(Ordering::SeqCst) {
            return Heropend::Gestopt;
        }
        let fout = match poging() {
            Ok(gelukt) => return Heropend::Open(gelukt),
            Err(fout) => fout,
        };
        let rest = tot.saturating_duration_since(Instant::now());
        if rest.is_zero() {
            break fout;
        }
        wacht = (wacht * 2).min(HEROPEN_STAP_MAX).min(rest);
    };
    if laatste.soort == FoutSoort::GeenAfspeelapparaat {
        laatste.soort = FoutSoort::AfspeelapparaatWeg;
    }
    Heropend::Opgegeven(laatste)
}

/// Een omrekenfout bij het starten: er is dan nog niets opgenomen, dus "bewaard tot dat
/// moment" klopt niet en de CLI kan terugvallen op alleen de microfoon.
#[cfg(any(windows, test))]
fn bij_start(mut fout: OpnameFout) -> OpnameFout {
    if fout.soort == FoutSoort::SysteemgeluidOmrekenen {
        fout.soort = FoutSoort::SysteemgeluidOmrekenenStart;
    }
    fout
}

/// Een fout bij het openen van een apparaat: `AfspeelapparaatWeg` ("viel weg tijdens het
/// opnemen") is dan onjuist, er was nog niets om weg te vallen. Het is gewoon geen
/// afspeelapparaat, en de CLI valt dan terug op alleen de microfoon.
#[cfg(any(windows, test))]
fn bij_openen(mut fout: OpnameFout) -> OpnameFout {
    if fout.soort == FoutSoort::AfspeelapparaatWeg {
        fout.soort = FoutSoort::GeenAfspeelapparaat;
    }
    fout
}

#[cfg(not(windows))]
mod imp {
    use super::{Afspeelapparaat, Bron, FoutSoort, OpnameFout, UitvalMelder};

    pub(super) fn speel_af(
        _apparaat: Option<&str>,
        _maak: impl FnOnce(u32) -> Vec<f32>,
    ) -> Result<(), OpnameFout> {
        Err(OpnameFout::nieuw(FoutSoort::TestgeluidNietMogelijk))
    }

    pub(super) fn afspeelapparaten() -> Result<Vec<Afspeelapparaat>, OpnameFout> {
        Err(OpnameFout::nieuw(FoutSoort::SysteemgeluidNietMogelijk))
    }

    pub(super) fn open(
        _apparaat: Option<&str>,
        _bij_uitval: UitvalMelder,
    ) -> Result<Box<dyn Bron>, OpnameFout> {
        Err(OpnameFout::nieuw(FoutSoort::SysteemgeluidNietMogelijk))
    }
}

#[cfg(windows)]
mod imp {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    use super::{
        Afspeelapparaat, Bron, Einde, FoutSoort, HEROPEN_BEGIN, HEROPEN_MAX, Heropend, OpnameFout,
        Uitgang, UitvalMelder, bij_openen, bij_start, heropen_met,
    };
    use crate::audio::{AudioFout, DOEL_SAMPLERATE, Omrekenaar};
    use crate::opname::slot;
    use crate::opname::stukken::GAT_DREMPEL;

    /// Zoveel pakketten (van ongeveer 10 ms) kunnen wachten tussen de audiothread van
    /// Windows en de verwerking: vijf seconden.
    const WACHTRIJ: usize = 512;

    /// Zoveel buffers liggen klaar voor de audiothread, zodat die in de normale toestand
    /// niets hoeft te alloceren. Een pakket is ongeveer 10 ms; de verwerking geeft de
    /// buffers meteen terug.
    const POOL: usize = 64;

    /// Frames per klaarliggende buffer (ruim 10 ms bij 192 kHz). Een groter pakket laat de
    /// buffer eenmalig groeien.
    const POOL_FRAMES: usize = 2048;

    fn naam(apparaat: &cpal::Device) -> String {
        apparaat
            .description()
            .map_or_else(|_| "Onbekend apparaat".to_string(), |d| d.name().to_owned())
    }

    fn vertaal(fout: &cpal::Error) -> OpnameFout {
        let soort = match fout.kind() {
            cpal::ErrorKind::DeviceNotAvailable => FoutSoort::AfspeelapparaatWeg,
            cpal::ErrorKind::DeviceBusy => FoutSoort::Bezet,
            cpal::ErrorKind::PermissionDenied => FoutSoort::ToegangGeweigerd,
            _ => FoutSoort::SysteemgeluidAnders,
        };
        OpnameFout {
            soort,
            detail: Some(fout.to_string()),
        }
    }

    fn omreken_fout(fout: &AudioFout) -> OpnameFout {
        OpnameFout {
            soort: FoutSoort::SysteemgeluidOmrekenen,
            detail: Some(fout.to_string()),
        }
    }

    pub(super) fn afspeelapparaten() -> Result<Vec<Afspeelapparaat>, OpnameFout> {
        let host = cpal::default_host();
        let standaard = host.default_output_device().and_then(|d| d.id().ok());
        let lijst = host.output_devices().map_err(|f| vertaal(&f))?;
        Ok(lijst
            .filter_map(|apparaat| {
                let id = apparaat.id().ok()?;
                Some(Afspeelapparaat {
                    standaard: standaard.as_ref() == Some(&id),
                    id: id.to_string(),
                    naam: naam(&apparaat),
                })
            })
            .collect())
    }

    fn standaard_apparaat() -> Result<cpal::Device, OpnameFout> {
        cpal::default_host()
            .default_output_device()
            .ok_or_else(|| OpnameFout::nieuw(FoutSoort::GeenAfspeelapparaat))
    }

    /// Het afspeelapparaat met dit id.
    fn zoek(gezocht: &str) -> Result<cpal::Device, OpnameFout> {
        cpal::default_host()
            .output_devices()
            .map_err(|f| bij_openen(vertaal(&f)))?
            .find(|a| a.id().is_ok_and(|id| id.to_string() == gezocht))
            .ok_or_else(|| OpnameFout::nieuw(FoutSoort::GeenAfspeelapparaat))
    }

    /// Zo lang mag het afspelen langer duren dan het geluid zelf, voor het opstarten van het
    /// apparaat. Daarna stopt het afspelen hoe dan ook.
    const AFSPEEL_MARGE: Duration = Duration::from_secs(2);

    /// Zo lang blijft de stream open nadat de laatste samples zijn aangeleverd: die moeten
    /// nog door de buffer van Windows naar de luidspreker.
    const AFSPEEL_NALOOP: Duration = Duration::from_millis(150);

    fn als_testgeluid(fout: OpnameFout) -> OpnameFout {
        OpnameFout {
            soort: FoutSoort::TestgeluidAnders,
            detail: fout.detail.or_else(|| Some(format!("{:?}", fout.soort))),
        }
    }

    pub(super) fn speel_af(
        apparaat: Option<&str>,
        maak: impl FnOnce(u32) -> Vec<f32>,
    ) -> Result<(), OpnameFout> {
        let apparaat = match apparaat {
            None => standaard_apparaat(),
            Some(gezocht) => zoek(gezocht),
        }
        .map_err(als_testgeluid)?;
        let indeling = apparaat
            .default_output_config()
            .map_err(|f| als_testgeluid(vertaal(&f)))?;
        let config = indeling.config();
        let kanalen = usize::from(config.channels.max(1));
        let samples = maak(config.sample_rate);
        let duur = duur_van(
            u64::try_from(samples.len()).unwrap_or(u64::MAX),
            config.sample_rate,
        );
        let (klaar_zender, klaar) = mpsc::sync_channel::<()>(1);
        let stream = match indeling.sample_format() {
            cpal::SampleFormat::F32 => {
                speel_met::<f32>(&apparaat, config, kanalen, samples, klaar_zender)
            }
            cpal::SampleFormat::I16 => {
                speel_met::<i16>(&apparaat, config, kanalen, samples, klaar_zender)
            }
            cpal::SampleFormat::I32 => {
                speel_met::<i32>(&apparaat, config, kanalen, samples, klaar_zender)
            }
            _ => {
                return Err(OpnameFout {
                    soort: FoutSoort::TestgeluidAnders,
                    detail: Some(format!("indeling {:?}", indeling.sample_format())),
                });
            }
        }
        .map_err(|f| als_testgeluid(vertaal(&f)))?;
        stream.play().map_err(|f| als_testgeluid(vertaal(&f)))?;
        // Wacht tot alle samples zijn aangeleverd; valt het apparaat weg, dan na de marge.
        let _ = klaar.recv_timeout(duur + AFSPEEL_MARGE);
        std::thread::sleep(AFSPEEL_NALOOP);
        drop(stream);
        Ok(())
    }

    fn speel_met<T>(
        apparaat: &cpal::Device,
        config: cpal::StreamConfig,
        kanalen: usize,
        samples: Vec<f32>,
        klaar: SyncSender<()>,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        T: cpal::SizedSample + cpal::FromSample<f32>,
    {
        let mut positie = 0usize;
        apparaat.build_output_stream(
            config,
            move |uit: &mut [T], _info: &cpal::OutputCallbackInfo| {
                for frame in uit.chunks_exact_mut(kanalen) {
                    let sample = samples.get(positie).copied().unwrap_or(0.0);
                    frame.fill(<T as cpal::FromSample<f32>>::from_sample_(sample));
                    positie = positie.saturating_add(1);
                }
                if positie >= samples.len() {
                    let _ = klaar.try_send(());
                }
            },
            // Een fout tijdens het afspelen (apparaat weg) laat het wachten op de marge lopen.
            |_fout: cpal::Error| {},
            None,
        )
    }

    pub(super) fn open(
        apparaat: Option<&str>,
        bij_uitval: UitvalMelder,
    ) -> Result<Box<dyn Bron>, OpnameFout> {
        let (apparaat, volg_standaard) = match apparaat {
            None => (standaard_apparaat()?, true),
            Some(gezocht) => (zoek(gezocht)?, false),
        };
        // Meteen nagaan of het apparaat een indeling heeft, zodat een fout bij het openen
        // komt en niet pas bij het starten. Een apparaat dat hier niet beschikbaar is, is
        // "geen afspeelapparaat" (er is niets dat wegviel), zodat de CLI kan terugvallen.
        let indeling = apparaat
            .default_output_config()
            .map_err(|f| bij_openen(vertaal(&f)))?;
        // Ook meteen nagaan of het geluid is om te rekenen, zodat dit bij het openen faalt
        // (met terugval op alleen de microfoon) en niet halverwege het starten.
        Verwerking::nieuw(indeling.config().sample_rate).map_err(bij_start)?;
        Ok(Box::new(SysteemBron {
            apparaat: Some(apparaat),
            volg_standaard,
            bij_uitval: Some(bij_uitval),
            stoppen: Arc::new(AtomicBool::new(false)),
            draad: None,
        }))
    }

    /// Het geluid van een afspeelapparaat, geopend.
    struct SysteemBron {
        apparaat: Option<cpal::Device>,
        volg_standaard: bool,
        bij_uitval: Option<UitvalMelder>,
        stoppen: Arc<AtomicBool>,
        draad: Option<JoinHandle<Einde>>,
    }

    impl Bron for SysteemBron {
        fn start(&mut self, uitgang: Box<dyn Uitgang>) -> Result<(), OpnameFout> {
            let (Some(apparaat), Some(bij_uitval)) = (self.apparaat.take(), self.bij_uitval.take())
            else {
                return Err(OpnameFout::nieuw(FoutSoort::Programma));
            };
            let (klaar_zender, klaar) = mpsc::sync_channel(1);
            let stoppen = Arc::clone(&self.stoppen);
            let volg_standaard = self.volg_standaard;
            let draad = std::thread::Builder::new()
                .name("mondig-systeemgeluid".into())
                .spawn(move || {
                    verwerk(
                        &apparaat,
                        volg_standaard,
                        uitgang,
                        &stoppen,
                        bij_uitval,
                        &klaar_zender,
                    )
                })
                .map_err(|_| OpnameFout::nieuw(FoutSoort::Programma))?;
            match klaar.recv() {
                Ok(Ok(())) => {
                    self.draad = Some(draad);
                    Ok(())
                }
                Ok(Err(fout)) => {
                    let _ = draad.join();
                    Err(fout)
                }
                Err(_) => {
                    let _ = draad.join();
                    Err(OpnameFout::nieuw(FoutSoort::Programma))
                }
            }
        }

        fn stop(&mut self) -> Result<Einde, OpnameFout> {
            let draad = self
                .draad
                .take()
                .ok_or_else(|| OpnameFout::nieuw(FoutSoort::Programma))?;
            self.stoppen.store(true, Ordering::SeqCst);
            draad
                .join()
                .map_err(|_| OpnameFout::nieuw(FoutSoort::Programma))
        }
    }

    impl Drop for SysteemBron {
        fn drop(&mut self) {
            if let Some(draad) = self.draad.take() {
                self.stoppen.store(true, Ordering::SeqCst);
                let _ = draad.join();
            }
        }
    }

    /// Een pakket van de audiothread: mono, op de samplerate van het apparaat.
    struct Pakket {
        mono: Vec<f32>,
        opgenomen: Instant,
    }

    /// Wat de audiothread deelt met de verwerking.
    #[derive(Clone)]
    struct Gedeeld {
        zender: SyncSender<Pakket>,
        /// Frames (op de samplerate van het apparaat) die niet in de wachtrij pasten.
        verloren: Arc<AtomicU64>,
        /// De laatste fout van de stream, als die er is.
        fout: Arc<Mutex<Option<cpal::Error>>>,
        /// Lege buffers voor de audiothread. De verwerking legt ze hier terug; de audiothread
        /// pakt er een met `try_lock` en alloceert alleen als er geen klaarligt.
        pool: Arc<Mutex<Vec<Vec<f32>>>>,
    }

    impl Gedeeld {
        fn nieuw(zender: SyncSender<Pakket>) -> Self {
            let mut buffers = Vec::with_capacity(POOL);
            buffers.extend((0..POOL).map(|_| Vec::with_capacity(POOL_FRAMES)));
            Self {
                zender,
                verloren: Arc::new(AtomicU64::new(0)),
                fout: Arc::new(Mutex::new(None)),
                pool: Arc::new(Mutex::new(buffers)),
            }
        }

        /// Als [`Gedeeld::geef_terug`], voor de audiothread: wacht nooit op het slot.
        fn geef_terug_zonder_wachten(&self, buffer: Vec<f32>) {
            if let Ok(mut pool) = self.pool.try_lock()
                && pool.len() < pool.capacity()
            {
                pool.push(buffer);
            }
        }

        /// Legt een gebruikte buffer terug voor de audiothread.
        fn geef_terug(&self, buffer: Vec<f32>) {
            let mut pool = slot(&self.pool);
            // De capaciteit van de pool is vooraf gereserveerd, dus dit alloceert niet.
            if pool.len() < pool.capacity() {
                pool.push(buffer);
            }
        }
    }

    /// Een lopende stream met zijn samplerate.
    struct Stroom {
        _stream: cpal::Stream,
        samplerate: u32,
    }

    fn bouw(apparaat: &cpal::Device, gedeeld: &Gedeeld) -> Result<Stroom, OpnameFout> {
        let indeling = apparaat.default_output_config().map_err(|f| vertaal(&f))?;
        let config = indeling.config();
        let kanalen = usize::from(config.channels.max(1));
        let stream = match indeling.sample_format() {
            cpal::SampleFormat::F32 => bouw_met::<f32>(apparaat, config, kanalen, gedeeld),
            cpal::SampleFormat::I16 => bouw_met::<i16>(apparaat, config, kanalen, gedeeld),
            cpal::SampleFormat::I32 => bouw_met::<i32>(apparaat, config, kanalen, gedeeld),
            _ => {
                return Err(OpnameFout {
                    soort: FoutSoort::SysteemgeluidAnders,
                    detail: Some(format!("indeling {:?}", indeling.sample_format())),
                });
            }
        }
        .map_err(|f| vertaal(&f))?;
        stream.play().map_err(|f| vertaal(&f))?;
        Ok(Stroom {
            _stream: stream,
            samplerate: config.sample_rate,
        })
    }

    fn bouw_met<T>(
        apparaat: &cpal::Device,
        config: cpal::StreamConfig,
        kanalen: usize,
        gedeeld: &Gedeeld,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        T: cpal::SizedSample,
        f32: cpal::FromSample<T>,
    {
        let data = gedeeld.clone();
        let fouten = Arc::clone(&gedeeld.fout);
        // Een afspeelapparaat als invoer openen is in cpal op Windows loopback.
        apparaat.build_input_stream(
            config,
            move |samples: &[T], info: &cpal::InputCallbackInfo| {
                let nu = Instant::now();
                let tijd = info.timestamp();
                let vertraging = tijd.callback.duration_since(tijd.capture);
                let opgenomen = nu.checked_sub(vertraging).unwrap_or(nu);
                #[allow(clippy::cast_precision_loss, reason = "hooguit een handvol kanalen")]
                let deler = kanalen as f32;
                // Een klaarliggende buffer hergebruiken: geen allocatie op de audiothread.
                let mut mono = data
                    .pool
                    .try_lock()
                    .ok()
                    .and_then(|mut pool| pool.pop())
                    .unwrap_or_default();
                mono.clear();
                mono.extend(samples.chunks_exact(kanalen).map(|frame| {
                    frame
                        .iter()
                        .map(|&s| <f32 as cpal::FromSample<T>>::from_sample_(s))
                        .sum::<f32>()
                        / deler
                }));
                let frames = mono.len() as u64;
                if let Err(
                    mpsc::TrySendError::Full(pakket) | mpsc::TrySendError::Disconnected(pakket),
                ) = data.zender.try_send(Pakket { mono, opgenomen })
                {
                    data.verloren.fetch_add(frames, Ordering::Relaxed);
                    // De buffer terug in de pool, niet droppen op de audiothread.
                    data.geef_terug_zonder_wachten(pakket.mono);
                }
            },
            move |fout: cpal::Error| {
                // Een gemist pakket (xrun) is geen reden om te stoppen; het gat wordt stilte.
                if fout.kind() != cpal::ErrorKind::Xrun {
                    *slot(&fouten) = Some(fout);
                }
            },
            None,
        )
    }

    /// Rekent pakketten om naar 16 kHz en geeft ze met hun opnametijd door.
    struct Verwerking {
        samplerate: u32,
        omrekenaar: Omrekenaar,
        /// Opnametijd van het eerste frame sinds het laatste herbegin.
        eerste: Option<Instant>,
        in_totaal: u64,
        uit_totaal: u64,
        /// Per pakket: index van het eerste frame en zijn opnametijd.
        tijden: VecDeque<(u64, Instant)>,
        uit: Vec<f32>,
    }

    fn duur_van(frames: u64, samplerate: u32) -> Duration {
        Duration::from_nanos(
            u64::try_from(u128::from(frames) * 1_000_000_000 / u128::from(samplerate.max(1)))
                .unwrap_or(u64::MAX),
        )
    }

    impl Verwerking {
        /// Geeft een fout als er voor deze samplerate geen omrekening te maken is.
        fn nieuw(samplerate: u32) -> Result<Self, OpnameFout> {
            Ok(Self {
                samplerate,
                omrekenaar: Omrekenaar::nieuw(samplerate).map_err(|f| omreken_fout(&f))?,
                eerste: None,
                in_totaal: 0,
                uit_totaal: 0,
                tijden: VecDeque::new(),
                uit: Vec::new(),
            })
        }

        fn pakket(&mut self, pakket: &Pakket, uitgang: &mut dyn Uitgang) -> Result<(), OpnameFout> {
            // Na een gat (er speelde niets, of de klok van het apparaat loopt achter) begint
            // de omrekening opnieuw: de schrijver vult het gat met stilte, zonder dat er
            // stilte door de wachtrij hoeft.
            if let Some(eerste) = self.eerste {
                let verwacht = eerste + duur_van(self.in_totaal, self.samplerate);
                if pakket.opgenomen > verwacht + GAT_DREMPEL {
                    self.sluit(uitgang)?;
                    *self = Self::nieuw(self.samplerate)?;
                }
            }
            self.eerste.get_or_insert(pakket.opgenomen);
            self.tijden.push_back((self.in_totaal, pakket.opgenomen));
            self.in_totaal += pakket.mono.len() as u64;
            let uitslag = self.omrekenaar.voeg_toe(&pakket.mono, &mut self.uit);
            // Wat al omgerekend is, gaat in elk geval door; dan pas de fout.
            self.lever(uitgang);
            uitslag.map_err(|f| omreken_fout(&f))
        }

        /// Geeft de omgerekende samples door, met de opnametijd van het eerste.
        fn lever(&mut self, uitgang: &mut dyn Uitgang) {
            if self.uit.is_empty() {
                return;
            }
            let invoer = self.uit_totaal * u64::from(self.samplerate) / u64::from(DOEL_SAMPLERATE);
            while self.tijden.len() >= 2 && self.tijden[1].0 <= invoer {
                self.tijden.pop_front();
            }
            let opgenomen = self
                .tijden
                .front()
                .map_or_else(Instant::now, |&(index, tijd)| {
                    tijd + duur_van(invoer.saturating_sub(index), self.samplerate)
                });
            uitgang.ontvang_op(&self.uit, opgenomen);
            self.uit_totaal += self.uit.len() as u64;
            self.uit.clear();
        }

        fn sluit(&mut self, uitgang: &mut dyn Uitgang) -> Result<(), OpnameFout> {
            let uitslag = self
                .omrekenaar
                .sluit(&mut self.uit)
                .map_err(|f| omreken_fout(&f));
            // Wat al omgerekend was, gaat in elk geval door.
            self.lever(uitgang);
            uitslag
        }
    }

    fn naar_16k(frames: u64, samplerate: u32) -> u64 {
        frames * u64::from(DOEL_SAMPLERATE) / u64::from(samplerate.max(1))
    }

    /// Bouwt de eerste stream en de omrekening erbij; elke fout is een fout bij het starten.
    fn start_stroom(
        apparaat: &cpal::Device,
        gedeeld: &Gedeeld,
    ) -> Result<(Stroom, Verwerking), OpnameFout> {
        let stroom = bouw(apparaat, gedeeld).map_err(bij_openen)?;
        let verwerking = Verwerking::nieuw(stroom.samplerate).map_err(bij_start)?;
        Ok((stroom, verwerking))
    }

    /// Verwerkt een pakket en legt de buffer terug in de pool.
    fn werk(
        verwerking: &mut Verwerking,
        pakket: Pakket,
        uitgang: &mut dyn Uitgang,
        gedeeld: &Gedeeld,
    ) -> Result<(), OpnameFout> {
        let uitslag = verwerking.pakket(&pakket, uitgang);
        gedeeld.geef_terug(pakket.mono);
        uitslag
    }

    /// De verwerkingsthread: bouwt de stream, rekent om, volgt een nieuw standaardapparaat
    /// en ruimt op bij het stoppen.
    ///
    /// Een gat door een wissel van apparaat wordt stilte: het volgende pakket heeft een
    /// opnametijd na het gat, en de schrijver vult het verschil (zie
    /// [`stukken`](crate::opname::stukken)); geeft deze bron op, dan vult de schrijver bij het
    /// afsluiten tot het einde van de opname.
    fn verwerk(
        apparaat: &cpal::Device,
        volg_standaard: bool,
        mut uitgang: Box<dyn Uitgang>,
        stoppen: &AtomicBool,
        bij_uitval: UitvalMelder,
        klaar: &SyncSender<Result<(), OpnameFout>>,
    ) -> Einde {
        let (zender, ontvanger) = mpsc::sync_channel(WACHTRIJ);
        let gedeeld = Gedeeld::nieuw(zender);
        let start = start_stroom(apparaat, &gedeeld);
        let (stroom, mut verwerking) = match start {
            Ok(klaar_om_te_gaan) => klaar_om_te_gaan,
            Err(fout) => {
                let _ = klaar.send(Err(fout));
                return Einde {
                    uitgevallen: None,
                    verloren: 0,
                };
            }
        };
        let mut stroom = Some(stroom);
        let _ = klaar.send(Ok(()));
        // Staat de verwerking nog open (niet afgesloten en niet kapot)?
        let mut actief = true;
        let mut verloren = 0u64;
        let mut uitgevallen = None;
        'lus: while !stoppen.load(Ordering::SeqCst) {
            match ontvanger.recv_timeout(Duration::from_millis(100)) {
                Ok(pakket) => {
                    if let Err(fout) = werk(&mut verwerking, pakket, uitgang.as_mut(), &gedeeld) {
                        uitgevallen = Some(fout);
                        actief = false;
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            let Some(fout) = slot(&gedeeld.fout).take() else {
                continue;
            };
            // Een standaardapparaat dat even niet bestaat (de enige uitgang valt weg) meldt
            // zich als `DeviceNotAvailable`; in de modus "volg het standaardapparaat" is dat
            // ook een wissel.
            let wissel = matches!(
                fout.kind(),
                cpal::ErrorKind::StreamInvalidated
                    | cpal::ErrorKind::DeviceChanged
                    | cpal::ErrorKind::DeviceNotAvailable
            );
            if !(volg_standaard && wissel) {
                uitgevallen = Some(vertaal(&fout));
                break;
            }
            // Het standaardapparaat is gewisseld: verder op het nieuwe.
            drop(stroom.take());
            while let Ok(pakket) = ontvanger.try_recv() {
                if let Err(fout) = werk(&mut verwerking, pakket, uitgang.as_mut(), &gedeeld) {
                    uitgevallen = Some(fout);
                    actief = false;
                    break 'lus;
                }
            }
            actief = false;
            if let Err(fout) = verwerking.sluit(uitgang.as_mut()) {
                uitgevallen = Some(fout);
                break;
            }
            verloren += naar_16k(
                gedeeld.verloren.swap(0, Ordering::Relaxed),
                verwerking.samplerate,
            );
            let nieuw = match heropen(&gedeeld, stoppen) {
                Heropend::Open(nieuw) => nieuw,
                // Een stop van de gebruiker tijdens het wachten is een gewone stop.
                Heropend::Gestopt => break,
                Heropend::Opgegeven(fout) => {
                    uitgevallen = Some(fout);
                    break;
                }
            };
            match Verwerking::nieuw(nieuw.samplerate) {
                Ok(v) => {
                    verwerking = v;
                    stroom = Some(nieuw);
                    actief = true;
                }
                Err(fout) => {
                    uitgevallen = Some(fout);
                    break;
                }
            }
        }
        drop(stroom);
        if actief {
            while let Ok(pakket) = ontvanger.try_recv() {
                if let Err(fout) = werk(&mut verwerking, pakket, uitgang.as_mut(), &gedeeld) {
                    uitgevallen = Some(fout);
                    actief = false;
                    break;
                }
            }
        }
        if actief && let Err(fout) = verwerking.sluit(uitgang.as_mut()) {
            uitgevallen = Some(fout);
        }
        verloren += naar_16k(
            gedeeld.verloren.load(Ordering::Relaxed),
            verwerking.samplerate,
        );
        afronden(uitgevallen, verloren, bij_uitval)
    }

    fn afronden(uitgevallen: Option<OpnameFout>, verloren: u64, bij_uitval: UitvalMelder) -> Einde {
        if let Some(fout) = &uitgevallen {
            bij_uitval(fout.clone());
        }
        Einde {
            uitgevallen,
            verloren,
        }
    }

    /// Opent het nieuwe standaardapparaat, met oplopende wachttijd tot [`HEROPEN_MAX`].
    fn heropen(gedeeld: &Gedeeld, stoppen: &AtomicBool) -> Heropend<Stroom> {
        heropen_met(stoppen, HEROPEN_MAX, HEROPEN_BEGIN, || {
            // Een fout van een vorige stream of een mislukte poging hoort niet bij de nieuwe
            // stream: wis ze voor elke poging.
            *slot(&gedeeld.fout) = None;
            standaard_apparaat().and_then(|a| bouw(&a, gedeeld))
        })
    }

    #[cfg(test)]
    mod tests {
        use std::sync::mpsc::Receiver;

        use super::*;

        /// Verzamelt wat de verwerking doorgeeft.
        struct Vangen(mpsc::Sender<(usize, Instant)>);

        impl Uitgang for Vangen {
            fn ontvang(&mut self, _: &[f32]) {
                unreachable!("de systeembron geeft altijd een tijd mee");
            }

            fn ontvang_op(&mut self, samples: &[f32], opgenomen: Instant) {
                self.0.send((samples.len(), opgenomen)).unwrap();
            }
        }

        fn vangen() -> (Vangen, Receiver<(usize, Instant)>) {
            let (z, o) = mpsc::channel();
            (Vangen(z), o)
        }

        fn pakket(frames: usize, opgenomen: Instant) -> Pakket {
            Pakket {
                mono: vec![0.1; frames],
                opgenomen,
            }
        }

        #[test]
        fn omrekenen_geeft_de_opnametijd_van_het_eerste_sample() {
            let begin = Instant::now();
            let (mut uit, ontvangen) = vangen();
            let mut verwerking = Verwerking::nieuw(48_000).unwrap();
            // Twee seconden in pakketten van 10 ms.
            for i in 0..200u64 {
                verwerking
                    .pakket(
                        &pakket(480, begin + Duration::from_millis(i * 10)),
                        &mut uit,
                    )
                    .unwrap();
            }
            verwerking.sluit(&mut uit).unwrap();
            drop(uit);
            let mut positie = 0u64;
            for (aantal, opgenomen) in ontvangen {
                let verwacht = begin + duur_van(positie, DOEL_SAMPLERATE);
                let afwijking = if opgenomen > verwacht {
                    opgenomen - verwacht
                } else {
                    verwacht - opgenomen
                };
                assert!(afwijking < Duration::from_millis(1), "{afwijking:?}");
                positie += aantal as u64;
            }
            assert_eq!(positie, 32_000);
        }

        #[test]
        fn een_omrekening_die_niet_te_maken_is_geeft_een_eigen_fout() {
            // Een samplerate van 0 kan niet worden omgerekend.
            let fout = Verwerking::nieuw(0)
                .err()
                .expect("omrekenen van 0 Hz hoort te falen");
            assert_eq!(fout.soort, FoutSoort::SysteemgeluidOmrekenen);
            let melding = crate::taak::Foutmelding::from(&fout).melding;
            assert!(melding.contains("omgezet"), "{melding}");
            assert!(!melding.contains("ander apparaat"), "{melding}");
        }

        #[test]
        fn de_pool_geeft_buffers_terug_zonder_te_groeien() {
            let (zender, _ontvanger) = mpsc::sync_channel(1);
            let gedeeld = Gedeeld::nieuw(zender);
            let capaciteit = slot(&gedeeld.pool).capacity();
            let buffer = slot(&gedeeld.pool).pop().unwrap();
            assert_eq!(slot(&gedeeld.pool).len(), POOL - 1);
            gedeeld.geef_terug(buffer);
            gedeeld.geef_terug(Vec::new());
            assert!(slot(&gedeeld.pool).len() <= capaciteit);
            assert_eq!(slot(&gedeeld.pool).capacity(), capaciteit);
            assert!(slot(&gedeeld.pool)[0].capacity() >= POOL_FRAMES);
        }

        #[test]
        fn na_een_gat_begint_de_tijd_opnieuw_bij_het_nieuwe_pakket() {
            let begin = Instant::now();
            let (mut uit, ontvangen) = vangen();
            let mut verwerking = Verwerking::nieuw(48_000).unwrap();
            for i in 0..50u64 {
                verwerking
                    .pakket(
                        &pakket(480, begin + Duration::from_millis(i * 10)),
                        &mut uit,
                    )
                    .unwrap();
            }
            // Drie seconden niets, dan weer geluid.
            let na_gat = begin + Duration::from_millis(3_500);
            for i in 0..50u64 {
                verwerking
                    .pakket(
                        &pakket(480, na_gat + Duration::from_millis(i * 10)),
                        &mut uit,
                    )
                    .unwrap();
            }
            verwerking.sluit(&mut uit).unwrap();
            drop(uit);
            let blokken: Vec<(usize, Instant)> = ontvangen.into_iter().collect();
            let voor: usize = blokken
                .iter()
                .take_while(|(_, t)| *t < na_gat)
                .map(|(n, _)| n)
                .sum();
            // Het eerste stuk is precies een halve seconde; daarna begint de tijd bij het gat.
            assert_eq!(voor, 8_000);
            let eerste_na = blokken.iter().find(|(_, t)| *t >= na_gat).unwrap();
            assert_eq!(eerste_na.1, na_gat);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicU32;

    use super::*;

    #[test]
    fn heropenen_probeert_door_tot_het_apparaat_opengaat() {
        let stoppen = AtomicBool::new(false);
        let pogingen = AtomicU32::new(0);
        let uitslag = heropen_met(
            &stoppen,
            Duration::from_secs(10),
            Duration::from_millis(5),
            || {
                if pogingen.fetch_add(1, Ordering::SeqCst) < 3 {
                    Err(OpnameFout::nieuw(FoutSoort::AfspeelapparaatWeg))
                } else {
                    Ok("open")
                }
            },
        );
        assert!(matches!(uitslag, Heropend::Open("open")), "{uitslag:?}");
        assert_eq!(pogingen.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn heropenen_geeft_na_de_maximale_tijd_op_met_de_laatste_fout() {
        let stoppen = AtomicBool::new(false);
        let pogingen = AtomicU32::new(0);
        let begin = Instant::now();
        let uitslag = heropen_met::<()>(
            &stoppen,
            Duration::from_millis(300),
            Duration::from_millis(10),
            || {
                pogingen.fetch_add(1, Ordering::SeqCst);
                Err(OpnameFout::nieuw(FoutSoort::GeenAfspeelapparaat))
            },
        );
        let Heropend::Opgegeven(fout) = uitslag else {
            unreachable!("{uitslag:?}");
        };
        // Een bron die het apparaat kwijtraakte meldt dat het wegviel, niet "geen apparaat".
        assert_eq!(fout.soort, FoutSoort::AfspeelapparaatWeg);
        assert!(pogingen.load(Ordering::SeqCst) >= 3);
        assert!(begin.elapsed() >= Duration::from_millis(300));
        assert!(begin.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn de_wachttijd_loopt_op_en_de_grens_staat_in_een_constante() {
        // De ADR noemt ongeveer 15 seconden; een Bluetooth-koptelefoon heeft 2 tot 5 nodig.
        assert_eq!(HEROPEN_MAX, Duration::from_secs(15));
        assert!(HEROPEN_BEGIN < HEROPEN_STAP_MAX);
        let stoppen = AtomicBool::new(false);
        let momenten = std::sync::Mutex::new(Vec::new());
        let begin = Instant::now();
        let _ = heropen_met::<()>(
            &stoppen,
            Duration::from_millis(700),
            Duration::from_millis(20),
            || {
                momenten.lock().unwrap().push(begin.elapsed());
                Err(OpnameFout::nieuw(FoutSoort::AfspeelapparaatWeg))
            },
        );
        let momenten = momenten.into_inner().unwrap();
        let tussenpozen: Vec<Duration> = momenten
            .windows(2)
            .map(|p| p[1].saturating_sub(p[0]))
            .collect();
        assert!(tussenpozen.len() >= 3, "{momenten:?}");
        assert!(tussenpozen[2] > tussenpozen[0], "{tussenpozen:?}");
    }

    #[test]
    fn een_stop_tijdens_het_wachten_is_een_gewone_stop() {
        let stoppen = Arc::new(AtomicBool::new(false));
        let vlag = Arc::clone(&stoppen);
        let draad = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            vlag.store(true, Ordering::SeqCst);
        });
        let begin = Instant::now();
        let uitslag = heropen_met::<()>(
            &stoppen,
            Duration::from_secs(30),
            Duration::from_secs(5),
            || Err(OpnameFout::nieuw(FoutSoort::AfspeelapparaatWeg)),
        );
        draad.join().unwrap();
        assert!(matches!(uitslag, Heropend::Gestopt), "{uitslag:?}");
        assert!(begin.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn een_stop_voor_de_eerste_poging_probeert_niets() {
        let stoppen = AtomicBool::new(true);
        let uitslag = heropen_met::<()>(
            &stoppen,
            Duration::from_secs(30),
            Duration::from_millis(10),
            || unreachable!("er hoort niet geprobeerd te worden"),
        );
        assert!(matches!(uitslag, Heropend::Gestopt));
    }

    #[test]
    fn een_omrekenfout_bij_de_start_is_een_eigen_soort_met_terugval_melding() {
        let start = bij_start(OpnameFout::nieuw(FoutSoort::SysteemgeluidOmrekenen));
        assert_eq!(start.soort, FoutSoort::SysteemgeluidOmrekenenStart);
        let melding = Foutmelding::from(&start).melding;
        assert!(melding.contains("Alleen de microfoon"), "{melding}");
        assert!(!melding.contains("bewaard"), "{melding}");
        // Tijdens de opname blijft het gewone uitval.
        assert_eq!(
            bij_start(OpnameFout::nieuw(FoutSoort::Bezet)).soort,
            FoutSoort::Bezet
        );
    }

    #[test]
    fn bij_het_openen_is_weggevallen_geen_afspeelapparaat() {
        let wegvallen = bij_openen(OpnameFout::nieuw(FoutSoort::AfspeelapparaatWeg));
        assert_eq!(wegvallen.soort, FoutSoort::GeenAfspeelapparaat);
        // Andere fouten blijven wat ze waren.
        for soort in [FoutSoort::Bezet, FoutSoort::ToegangGeweigerd] {
            assert_eq!(bij_openen(OpnameFout::nieuw(soort)).soort, soort);
        }
    }
}
