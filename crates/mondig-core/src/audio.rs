//! Audiobestanden inlezen en omzetten naar het formaat dat de spraakmodellen willen:
//! 16 kHz, mono, `f32` tussen -1 en 1.
//!
//! Ondersteund (ADR 0015): wav, mp3, flac, ogg (vorbis en opus), losse `.opus`, aac in
//! m4a, en het geluid uit video's in mp4, mov, mkv en webm (aac, opus of vorbis; het
//! eerste audiospoor). Alles gebeurt lokaal: symphonia voor containers en codecs,
//! libopus (via `symphonia-adapter-libopus`) voor Opus, rubato voor het omrekenen.
//!
//! Lange bestanden lees je met [`Stukken`]: die geeft stukken van hooguit
//! [`STUK_DUUR`], gesneden op een stil moment, zodat het hele bestand nooit tegelijk in
//! het geheugen staat. [`laad`] leest alles in één keer, voor korte bestanden.

use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};
use serde::{Deserialize, Serialize};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::errors::Error as SymphoniaFout;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Time;

/// Samplefrequentie waarmee alle spraakmodellen werken.
pub const DOEL_SAMPLERATE: u32 = 16_000;

/// Hoe lang een stuk hooguit is als een bestand in stukken wordt uitgeschreven.
///
/// Gemeten op 21 minuten MLS achter elkaar (sprint 4-A, 1-11-2026), woordfouten bij
/// stukken van 15, 20, 30 en 60 s: Parakeet 12,5, 12,6, 11,9 en 12,8 %, Canary 10,7,
/// 10,4, 10,0 en 19,4 %. Canary wordt bij lange stukken veel slechter (los gemeten ook
/// zonder snijden: 9,8 % bij bestanden van 15 s, 13,8 % bij 47 s, 25,7 % bij 63 s), en
/// Whisper werkt van zichzelf in vensters van 30 s. Dus 30 s voor alle modellen; dat
/// houdt ook het geheugen klein en geeft vaak een nieuwe voortgang.
pub const STUK_DUUR: Duration = Duration::from_secs(30);

/// In dit laatste deel van een stuk zoeken we het stilste moment om te snijden.
const SNEDE_ZOEKVENSTER: Duration = Duration::from_secs(10);

/// Zo lang is het stille moment waarop we snijden: kort genoeg om tussen twee zinnen
/// te passen, lang genoeg om niet in een plofklank midden in een woord te vallen.
const SNEDE_VENSTER_MS: usize = 210;

/// Een analyseframe bij het zoeken naar een snede.
const SNEDE_FRAME_MS: usize = 30;

/// Blokgrootte (in frames) voor de resampler.
const RESAMPLE_BLOK: usize = 1024;

/// Gegevens over het bestand zoals het was voor de omzetting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BronInfo {
    /// Korte naam van het containerformaat, bijvoorbeeld `wave`, `mp3`, `flac`, `ogg`,
    /// `isomp4` of `matroska`.
    pub formaat: String,
    /// Samplefrequentie van het bestand in Hz.
    pub samplerate: u32,
    /// Aantal kanalen in het bestand.
    pub kanalen: usize,
    /// Duur van het bestand.
    pub duur: Duration,
}

/// Ingelezen geluid, klaar voor een spraakmodel.
#[derive(Debug, Clone)]
pub struct Geluid {
    /// Mono, 16 kHz, `f32`.
    pub samples: Vec<f32>,
    /// Duur van `samples`.
    pub duur: Duration,
    /// Hoe het bestand er oorspronkelijk uitzag.
    pub bron: BronInfo,
}

/// De extensies die Mondig kan lezen (ADR 0015). Eén lijst voor de bestandskiezer, slepen
/// en de bench, zodat ze niet uit elkaar lopen.
pub const EXTENSIES: &[&str] = &[
    "mp3", "m4a", "wav", "ogg", "opus", "flac", "mp4", "mov", "mkv", "webm",
];

/// De extensies uit [`EXTENSIES`] die een video zijn (een filmpictogram in de lijst).
pub const VIDEO_EXTENSIES: &[&str] = &["mp4", "mov", "mkv", "webm"];

/// Is dit pad een video volgens de extensie?
#[must_use]
pub fn is_video(pad: &Path) -> bool {
    pad.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTENSIES.contains(&e.to_lowercase().as_str()))
}

/// Iets wat bij het lezen opviel, maar het uitschrijven niet tegenhield.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Leeswaarschuwing {
    /// Het bestand stopt eerder dan de container zegt: waarschijnlijk afgebroken.
    Afgebroken {
        /// Tot waar het bestand te lezen was, in milliseconden.
        tot_ms: u64,
    },
    /// Een deel van de pakketten was kapot en is overgeslagen.
    Overgeslagen {
        /// Hoeveel pakketten.
        aantal: usize,
    },
}

impl Leeswaarschuwing {
    /// De waarschuwing in gewone taal.
    #[must_use]
    pub fn melding(self) -> String {
        match self {
            Self::Afgebroken { tot_ms } => format!(
                "Het bestand lijkt afgebroken; de tekst stopt bij {}.",
                mm_ss(tot_ms)
            ),
            Self::Overgeslagen { .. } => {
                "Een deel van het bestand was beschadigd en is overgeslagen; daar kan tekst ontbreken."
                    .to_string()
            }
        }
    }
}

/// Een tijd als mm:ss, boven het uur als u:mm:ss.
fn mm_ss(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

/// Wat er mis kan gaan bij het inlezen van een audiobestand.
#[derive(Debug, thiserror::Error)]
pub enum AudioFout {
    /// Het bestand bestaat niet.
    #[error("Het bestand is niet gevonden: {}", .0.display())]
    NietGevonden(PathBuf),
    /// Het pad is een map, geen bestand.
    #[error("Dit is een map. Sleep losse bestanden.")]
    IsMap,
    /// Het besturingssysteem weigert de toegang.
    #[error("Mondig mag dit bestand niet openen. Kijk of je er toegang toe hebt.")]
    GeenToegang,
    /// Een ander programma houdt het bestand vast.
    #[error(
        "Het bestand is in gebruik door een ander programma. Sluit dat programma en probeer het opnieuw."
    )]
    InGebruik,
    /// Het bestand bestaat maar kon niet gelezen worden. De technische reden staat er
    /// alleen voor diagnose bij en komt niet in de melding.
    #[error("Het bestand kon niet gelezen worden.")]
    Leesfout(String),
    /// Het formaat of de codec wordt niet herkend.
    #[error(
        "Mondig kan dit formaat niet lezen. Sla het bestand op als mp4 of wav, bijvoorbeeld met Clipchamp, en voeg het opnieuw toe."
    )]
    FormaatNietOndersteund,
    /// Het bestand bevat geen geluidsspoor (bijvoorbeeld een video zonder geluid).
    #[error("Dit bestand bevat geen geluid.")]
    GeenAudiospoor,
    /// Het bestand is kapot of onvolledig.
    #[error("Het bestand is beschadigd en kon niet (volledig) worden gelezen.")]
    Beschadigd,
    /// Omrekenen naar 16 kHz mislukte. De technische reden staat er alleen voor diagnose bij
    /// en komt niet in de melding, net als bij `Leesfout`.
    #[error("Mondig kon het geluid van dit bestand niet voorbereiden. Probeer een ander bestand.")]
    Omzetten(String),
    /// Samplefrequentie of aantal kanalen verandert halverwege het bestand. De waarden staan
    /// erbij voor diagnose (Debug) en komen niet in de melding.
    #[error(
        "Het geluid in dit bestand verandert halverwege, en dat kan Mondig niet lezen. Sla het bestand opnieuw op als mp4 of wav, bijvoorbeeld met Clipchamp, en voeg het opnieuw toe."
    )]
    WisselendeIndeling {
        /// Samplefrequentie aan het begin.
        van_hz: u32,
        /// Kanalen aan het begin.
        van_kanalen: usize,
        /// Samplefrequentie van het afwijkende stuk.
        naar_hz: u32,
        /// Kanalen van het afwijkende stuk.
        naar_kanalen: usize,
    },
}

/// Duur van een aantal frames bij een samplefrequentie.
fn duur_van(frames: usize, samplerate: u32) -> Duration {
    if samplerate == 0 {
        return Duration::ZERO;
    }
    let micro = frames as u64 * 1_000_000 / u64::from(samplerate);
    Duration::from_micros(micro)
}

/// Duur van een aantal samples op 16 kHz (de samplefrequentie van [`Geluid`]).
#[must_use]
pub fn duur_van_samples(aantal: usize) -> Duration {
    duur_van(aantal, DOEL_SAMPLERATE)
}

/// De codecs die Mondig kent: die van symphonia (met de features uit `Cargo.toml`)
/// plus Opus via libopus, dat symphonia zelf nog niet heeft (ADR 0015).
fn codecs() -> &'static CodecRegistry {
    static REGISTER: OnceLock<CodecRegistry> = OnceLock::new();
    REGISTER.get_or_init(|| {
        let mut register = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut register);
        register.register_audio_decoder::<symphonia_adapter_libopus::OpusDecoder>();
        register
    })
}

/// Een fout van het besturingssysteem bij het openen, in gewone taal.
fn openfout(pad: &Path, fout: &std::io::Error) -> AudioFout {
    // Windows: ERROR_SHARING_VIOLATION (32) en ERROR_LOCK_VIOLATION (33).
    if cfg!(windows) && matches!(fout.raw_os_error(), Some(32 | 33)) {
        return AudioFout::InGebruik;
    }
    match fout.kind() {
        ErrorKind::NotFound => AudioFout::NietGevonden(pad.to_path_buf()),
        ErrorKind::PermissionDenied => AudioFout::GeenToegang,
        ErrorKind::IsADirectory => AudioFout::IsMap,
        _ => AudioFout::Leesfout(fout.to_string()),
    }
}

/// Opent een bestand en laat symphonia het formaat herkennen. Geeft de lezer en de korte
/// naam van het containerformaat terug.
fn open(pad: &Path) -> Result<(Box<dyn FormatReader>, String), AudioFout> {
    // Op Windows geeft een map openen "toegang geweigerd"; eerst kijken of het een map is.
    if pad.is_dir() {
        return Err(AudioFout::IsMap);
    }
    let bestand = File::open(pad).map_err(|fout| openfout(pad, &fout))?;
    let stroom = MediaSourceStream::new(Box::new(bestand), MediaSourceStreamOptions::default());

    let mut hint = Hint::new();
    if let Some(extensie) = pad.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extensie);
    }

    // Ook een video waarvan symphonia het videospoor afwijst (bijvoorbeeld een `hvcC`-blok
    // groter dan 1 KB, ADR 0015) komt hier terecht: voor de gebruiker is dat hetzelfde.
    let lezer = symphonia::default::get_probe()
        .probe(
            &hint,
            stroom,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|_| AudioFout::FormaatNietOndersteund)?;
    let formaat = lezer.format_info().short_name.to_string();
    Ok((lezer, formaat))
}

/// Duur van het eerste audiospoor volgens de container, zonder te decoderen. `None` als
/// de container het niet zegt (bijvoorbeeld een mp3 zonder kop met lengte).
#[allow(clippy::cast_precision_loss)]
fn duur_uit_metadata(lezer: &dyn FormatReader) -> Result<Option<Duration>, AudioFout> {
    let spoor = lezer
        .default_track(TrackType::Audio)
        .ok_or(AudioFout::GeenAudiospoor)?;
    let samplerate = spoor
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .and_then(|a| a.sample_rate);
    // Eerst de duur in tijdbasis (precies zoals de container die opgeeft), anders het
    // aantal frames gedeeld door de samplefrequentie.
    let uit_tijdbasis = spoor
        .duration
        .zip(spoor.time_base)
        .and_then(|(duur, basis)| basis.calc_duration(duur))
        .map(|tijd| tijd.as_secs_f64())
        .filter(|s| *s > 0.0);
    let uit_frames = spoor
        .num_frames
        .zip(samplerate)
        .filter(|(frames, hz)| *frames > 0 && *hz > 0)
        .map(|(frames, hz)| frames as f64 / f64::from(hz));
    Ok(uit_tijdbasis.or(uit_frames).map(Duration::from_secs_f64))
}

/// Duur van een audiobestand volgens de container, zonder te decoderen: alleen de kop
/// wordt gelezen en er draait geen codec (ADR 0009 en 0015: decoderen hoort in het
/// werkproces). `None` als de container de duur niet opgeeft, bijvoorbeeld een webm uit
/// een browser of een mp3 zonder kop met lengte.
///
/// # Errors
///
/// Geeft een [`AudioFout`] als het bestand ontbreekt, een map is, niet te openen is,
/// een onbekende container heeft of geen geluidsspoor.
pub fn duur(pad: &Path) -> Result<Option<Duration>, AudioFout> {
    let (lezer, _) = open(pad)?;
    duur_uit_metadata(lezer.as_ref())
}

/// De duur door het hele bestand te decoderen en de samples te tellen, voor een bestand
/// waarvan de container de duur niet zegt. Hoort in het werkproces. Stopt zodra `stop`
/// waar geeft en geeft dan `None`.
///
/// # Errors
///
/// Als [`Lezer::lees_blok`].
pub fn duur_door_te_tellen(
    pad: &Path,
    stop: &dyn Fn() -> bool,
) -> Result<Option<Duration>, AudioFout> {
    let mut lezer = Lezer::open(pad)?;
    let mut aantal = 0;
    while let Some(blok) = lezer.lees_blok()? {
        if stop() {
            return Ok(None);
        }
        aantal += blok.len();
    }
    Ok(Some(duur_van_samples(aantal)))
}

/// Controleert dat samplefrequentie en kanalen gelijk blijven. De eerste waarde wordt
/// onthouden; een afwijkend stuk daarna geeft [`AudioFout::WisselendeIndeling`].
fn controleer_indeling(
    eerste: &mut Option<(u32, usize)>,
    samplerate: u32,
    kanalen: usize,
) -> Result<(), AudioFout> {
    match *eerste {
        None => {
            *eerste = Some((samplerate, kanalen));
            Ok(())
        }
        Some((hz, k)) if hz == samplerate && k == kanalen => Ok(()),
        Some((van_hz, van_kanalen)) => Err(AudioFout::WisselendeIndeling {
            van_hz,
            van_kanalen,
            naar_hz: samplerate,
            naar_kanalen: kanalen,
        }),
    }
}

/// Rekent mono-audio stukje voor stukje om naar 16 kHz.
///
/// Gebruikt dezelfde blokken als `Resampler::process_all` van rubato, maar haalt de
/// vertraging van de resampler er zelf af (de eerste `output_delay` frames vallen weg) en
/// geeft precies `ceil(verhouding × invoer)` frames. Zo hangt de uitkomst niet af van hoe
/// een versie van rubato dat afkapt: 5.0.0 laat de vertraging bij een heel kort fragment
/// staan. Voor gewone bestanden is de uitkomst gelijk aan `process_all`.
pub(crate) struct Omrekenaar {
    /// `None` als het bestand al 16 kHz is.
    resampler: Option<Fft<f32>>,
    /// Invoer die nog niet omgerekend is.
    invoer: Vec<f32>,
    /// Uitvoerbuffer voor één blok.
    blok: Vec<f32>,
    /// Zoveel frames vertraging moeten nog van het begin af.
    te_trimmen: usize,
    /// Alle invoer tot nu toe.
    invoer_totaal: usize,
    /// Alle uitvoer tot nu toe die meegegeven is (na de vertraging).
    uitvoer_totaal: usize,
}

impl Omrekenaar {
    pub(crate) fn nieuw(samplerate: u32) -> Result<Self, AudioFout> {
        let resampler = if samplerate == DOEL_SAMPLERATE {
            None
        } else {
            let mut resampler = Fft::<f32>::new(
                samplerate as usize,
                DOEL_SAMPLERATE as usize,
                RESAMPLE_BLOK,
                1,
                FixedSync::Both,
            )
            .map_err(|e| AudioFout::Omzetten(e.to_string()))?;
            resampler.reset();
            Some(resampler)
        };
        let (te_trimmen, blok) = resampler.as_ref().map_or((0, Vec::new()), |r| {
            (r.output_delay(), vec![0.0; r.output_frames_max()])
        });
        Ok(Self {
            resampler,
            invoer: Vec::new(),
            blok,
            te_trimmen,
            invoer_totaal: 0,
            uitvoer_totaal: 0,
        })
    }

    /// Eén aanroep van de resampler op het begin van `invoer`. Geeft het aantal gebruikte
    /// invoerframes terug; de uitvoer gaat, na de vertraging, achter `uit`.
    fn stap(&mut self, partial: Option<usize>, uit: &mut Vec<f32>) -> Result<usize, AudioFout> {
        let fout = |e: &dyn std::fmt::Display| AudioFout::Omzetten(e.to_string());
        let Some(resampler) = self.resampler.as_mut() else {
            return Ok(0);
        };
        let frames = self.invoer.len();
        let invoer = InterleavedSlice::new(&self.invoer[..], 1, frames).map_err(|e| fout(&e))?;
        let ruimte = self.blok.len();
        let mut uitvoer =
            InterleavedSlice::new_mut(&mut self.blok[..], 1, ruimte).map_err(|e| fout(&e))?;
        let indexing = Indexing {
            input_offset: 0,
            output_offset: 0,
            active_channels_mask: None,
            partial_len: partial,
        };
        let (gebruikt, gemaakt) = resampler
            .process_into_buffer(&invoer, &mut uitvoer, Some(&indexing))
            .map_err(|e| fout(&e))?;
        let weg = self.te_trimmen.min(gemaakt);
        self.te_trimmen -= weg;
        uit.extend_from_slice(&self.blok[weg..gemaakt]);
        self.uitvoer_totaal += gemaakt - weg;
        Ok(gebruikt)
    }

    /// Neemt mono-invoer op de bronfrequentie aan en zet wat klaar is achter `uit`.
    pub(crate) fn voeg_toe(&mut self, mono: &[f32], uit: &mut Vec<f32>) -> Result<(), AudioFout> {
        self.invoer_totaal += mono.len();
        if self.resampler.is_none() {
            uit.extend_from_slice(mono);
            self.uitvoer_totaal += mono.len();
            return Ok(());
        }
        self.invoer.extend_from_slice(mono);
        let volgende = self
            .resampler
            .as_ref()
            .map_or(RESAMPLE_BLOK, Resampler::input_frames_next);
        // Net als `process_all`: zolang er méér dan één blok over is, een vol blok. Het
        // laatste stuk (hooguit één blok) wacht op `sluit`.
        while self.invoer.len() > volgende {
            let gebruikt = self.stap(None, uit)?;
            self.invoer.drain(..gebruikt);
        }
        Ok(())
    }

    /// Rekent het laatste stuk om, pompt nullen tot alles eruit is en kapt af op
    /// `ceil(verhouding × invoer)` frames.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "zo rekent rubato de verwachte lengte uit"
    )]
    pub(crate) fn sluit(&mut self, uit: &mut Vec<f32>) -> Result<(), AudioFout> {
        let Some(verhouding) = self.resampler.as_ref().map(Resampler::resample_ratio) else {
            return Ok(());
        };
        let verwacht = (verhouding * self.invoer_totaal as f64).ceil() as usize;
        let begin = uit.len();
        let al_gegeven = self.uitvoer_totaal;
        if !self.invoer.is_empty() {
            let over = self.invoer.len();
            self.stap(Some(over), uit)?;
            self.invoer.clear();
        }
        let mut pompen = 0;
        while self.uitvoer_totaal < verwacht {
            self.stap(Some(0), uit)?;
            pompen += 1;
            if pompen > 64 {
                return Err(AudioFout::Omzetten(
                    "de omrekening levert niet genoeg samples".into(),
                ));
            }
        }
        // Wat er boven de verwachte lengte uitkomt, is opvulling.
        uit.truncate(begin + verwacht.saturating_sub(al_gegeven));
        self.uitvoer_totaal = verwacht;
        Ok(())
    }
}

/// Leest een bestand blok voor blok als 16 kHz mono, zonder het hele bestand in het
/// geheugen te zetten.
pub struct Lezer {
    container: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    spoor_id: u32,
    formaat: String,
    duur: Option<Duration>,
    indeling: Option<(u32, usize)>,
    omrekenaar: Option<Omrekenaar>,
    tussen: Vec<f32>,
    mono: Vec<f32>,
    bron_frames: usize,
    uitgegeven: usize,
    /// Pakketten van het audiospoor, en hoeveel daarvan kapot waren en overgeslagen.
    pakketten: usize,
    kapotte_pakketten: usize,
    klaar: bool,
}

impl std::fmt::Debug for Lezer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lezer")
            .field("formaat", &self.formaat)
            .field("duur", &self.duur)
            .field("uitgegeven", &self.uitgegeven)
            .finish_non_exhaustive()
    }
}

impl Lezer {
    /// Opent een bestand en zoekt het eerste audiospoor.
    ///
    /// # Errors
    ///
    /// Geeft een [`AudioFout`] als het bestand ontbreekt, niet ondersteund is of geen
    /// geluid bevat.
    pub fn open(pad: &Path) -> Result<Self, AudioFout> {
        let (lezer, formaat) = open(pad)?;
        let duur = duur_uit_metadata(lezer.as_ref())?;
        let spoor = lezer
            .default_track(TrackType::Audio)
            .ok_or(AudioFout::GeenAudiospoor)?;
        let spoor_id = spoor.id;
        let parameters = spoor
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or(AudioFout::GeenAudiospoor)?;
        let decoder = codecs()
            .make_audio_decoder(parameters, &AudioDecoderOptions::default())
            .map_err(|_| AudioFout::FormaatNietOndersteund)?;
        Ok(Self {
            container: lezer,
            decoder,
            spoor_id,
            formaat,
            duur,
            indeling: None,
            omrekenaar: None,
            tussen: Vec::new(),
            mono: Vec::new(),
            bron_frames: 0,
            uitgegeven: 0,
            pakketten: 0,
            kapotte_pakketten: 0,
            klaar: false,
        })
    }

    /// Wat er na het lezen van het hele bestand opviel, of `None`. Een bestand dat korter
    /// is dan de container zegt (meer dan een seconde en meer dan 2 procent), is
    /// waarschijnlijk afgebroken; zijn minstens drie pakketten en minstens 1 procent kapot,
    /// dan ontbreekt er tekst.
    #[must_use]
    pub fn waarschuwing(&self) -> Option<Leeswaarschuwing> {
        if !self.klaar {
            return None;
        }
        let gelezen = duur_van_samples(self.uitgegeven);
        if let Some(verwacht) = self.duur {
            let tekort = verwacht.saturating_sub(gelezen);
            if tekort > Duration::from_secs(1) && tekort.as_millis() * 50 > verwacht.as_millis() {
                return Some(Leeswaarschuwing::Afgebroken {
                    tot_ms: u64::try_from(gelezen.as_millis()).unwrap_or(u64::MAX),
                });
            }
        }
        if self.kapotte_pakketten >= 3 && self.kapotte_pakketten * 100 >= self.pakketten {
            return Some(Leeswaarschuwing::Overgeslagen {
                aantal: self.kapotte_pakketten,
            });
        }
        None
    }

    /// De duur volgens de container, zonder te decoderen; `None` als die er niet in staat.
    #[must_use]
    pub fn duur_volgens_container(&self) -> Option<Duration> {
        self.duur
    }

    /// Hoe het bestand eruitzag, voor zover nu gelezen. `None` voordat er iets
    /// gedecodeerd is.
    #[must_use]
    pub fn bron(&self) -> Option<BronInfo> {
        self.indeling.map(|(samplerate, kanalen)| BronInfo {
            formaat: self.formaat.clone(),
            samplerate,
            kanalen,
            duur: duur_van(self.bron_frames, samplerate),
        })
    }

    /// Springt in het bestand naar vlak voor `ms` zonder alles ervoor te decoderen (voor het
    /// afspelen van een regel diep in een lang bestand). Geeft de plek in samples van 16 kHz waar het
    /// lezen nu begint, die hooguit even voor `ms` ligt, of `None` als het formaat niet kan
    /// springen. Na `None` is de lezer onbetrouwbaar: open het bestand dan opnieuw.
    pub fn spring_naar(&mut self, ms: u64) -> Option<u64> {
        let time = Time::try_from_nanos_u128(u128::from(ms) * 1_000_000)?;
        let gevonden = self
            .container
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.spoor_id),
                },
            )
            .ok()?;
        let tijdbasis = self
            .container
            .tracks()
            .iter()
            .find(|t| t.id == self.spoor_id)?
            .time_base?;
        let begin = tijdbasis.calc_time(gevonden.actual_ts)?;
        // De decoder en de omrekenaar beginnen opnieuw: ze horen bij een andere plek.
        self.decoder.reset();
        self.omrekenaar = None;
        self.klaar = false;
        let samples = begin.as_nanos().checked_mul(i128::from(DOEL_SAMPLERATE))? / 1_000_000_000;
        u64::try_from(samples).ok()
    }

    /// Het volgende blok samples (16 kHz mono), of `None` aan het einde. Een blok is
    /// meestal één pakket uit het bestand; het kan leeg zijn.
    ///
    /// # Errors
    ///
    /// Geeft een [`AudioFout`] als het bestand beschadigd is, halverwege van indeling
    /// wisselt, of geen geluid blijkt te bevatten.
    pub fn lees_blok(&mut self) -> Result<Option<Vec<f32>>, AudioFout> {
        if self.klaar {
            return Ok(None);
        }
        let mut uit = Vec::new();
        loop {
            let pakket = match self.container.next_packet() {
                Ok(Some(pakket)) => pakket,
                Ok(None) => return self.einde(uit),
                // Sommige containers melden het einde als een voortijdig einde van het bestand.
                Err(SymphoniaFout::IoError(fout)) if fout.kind() == ErrorKind::UnexpectedEof => {
                    return self.einde(uit);
                }
                Err(_) => return Err(AudioFout::Beschadigd),
            };
            if pakket.track_id != self.spoor_id {
                continue;
            }
            self.pakketten += 1;
            let buffer = match self.decoder.decode(&pakket) {
                Ok(buffer) => buffer,
                // Een los kapot pakket slaan we over; is er daarna niets bruikbaars, dan melden
                // we het. Zijn het er veel, dan geeft `waarschuwing` dat aan.
                Err(SymphoniaFout::DecodeError(_) | SymphoniaFout::IoError(_)) => {
                    self.kapotte_pakketten += 1;
                    continue;
                }
                Err(_) => return Err(AudioFout::Beschadigd),
            };
            let aantal_kanalen = buffer.spec().channels().count();
            if aantal_kanalen == 0 {
                continue;
            }
            let samplerate = buffer.spec().rate();
            controleer_indeling(&mut self.indeling, samplerate, aantal_kanalen)?;
            self.tussen.resize(buffer.samples_interleaved(), 0.0);
            buffer.copy_to_slice_interleaved(&mut self.tussen);
            // Mono maken door de kanalen per frame te middelen.
            let deler = u16::try_from(aantal_kanalen).map_or(1.0, f32::from);
            self.mono.clear();
            self.mono.extend(
                self.tussen
                    .chunks_exact(aantal_kanalen)
                    .map(|frame| frame.iter().sum::<f32>() / deler),
            );
            self.bron_frames += self.mono.len();
            if self.omrekenaar.is_none() {
                self.omrekenaar = Some(Omrekenaar::nieuw(samplerate)?);
            }
            if let Some(omrekenaar) = self.omrekenaar.as_mut() {
                omrekenaar.voeg_toe(&self.mono, &mut uit)?;
            }
            if !uit.is_empty() {
                self.uitgegeven += uit.len();
                return Ok(Some(uit));
            }
        }
    }

    /// Het einde van het bestand: de rest uit de omrekenaar, of een fout als er nooit
    /// geluid uitkwam.
    fn einde(&mut self, mut uit: Vec<f32>) -> Result<Option<Vec<f32>>, AudioFout> {
        self.klaar = true;
        if self.bron_frames == 0 {
            return Err(if self.kapotte_pakketten > 0 {
                AudioFout::Beschadigd
            } else {
                AudioFout::GeenAudiospoor
            });
        }
        if let Some(omrekenaar) = self.omrekenaar.as_mut() {
            omrekenaar.sluit(&mut uit)?;
        }
        self.uitgegeven += uit.len();
        Ok((!uit.is_empty()).then_some(uit))
    }
}

/// Een stuk van een bestand, 16 kHz mono.
#[derive(Debug, Clone, PartialEq)]
pub struct Stuk {
    /// Waar het stuk begint, in samples vanaf het begin van het bestand.
    pub begin: usize,
    /// De samples.
    pub samples: Vec<f32>,
}

impl Stuk {
    /// Het begin van het stuk in milliseconden vanaf het begin van het bestand.
    #[must_use]
    pub fn begin_ms(&self) -> u64 {
        u64::try_from(duur_van_samples(self.begin).as_millis()).unwrap_or(u64::MAX)
    }

    /// Het einde van het stuk in milliseconden vanaf het begin van het bestand.
    #[must_use]
    pub fn eind_ms(&self) -> u64 {
        u64::try_from(duur_van_samples(self.begin + self.samples.len()).as_millis())
            .unwrap_or(u64::MAX)
    }
}

/// Leest een bestand in stukken van hooguit een vaste lengte, gesneden op een stil
/// moment. Er staat nooit meer dan ongeveer één stuk in het geheugen.
#[derive(Debug)]
pub struct Stukken {
    lezer: Lezer,
    buffer: Vec<f32>,
    begin: usize,
    doel: usize,
    zoekvenster: usize,
    einde: bool,
}

impl Stukken {
    /// Opent een bestand voor lezen in stukken van [`STUK_DUUR`].
    ///
    /// # Errors
    ///
    /// Geeft een [`AudioFout`] als het bestand ontbreekt, niet ondersteund is of geen
    /// geluid bevat.
    pub fn open(pad: &Path) -> Result<Self, AudioFout> {
        Self::open_met(pad, STUK_DUUR)
    }

    /// Als [`Stukken::open`], met een eigen stuklengte (voor tests). Het zoekvenster
    /// voor de snede is hooguit een zesde van de stuklengte.
    ///
    /// # Errors
    ///
    /// Zie [`Stukken::open`].
    pub fn open_met(pad: &Path, stuk: Duration) -> Result<Self, AudioFout> {
        let doel = samples_in(stuk).max(1);
        let zoekvenster = samples_in(SNEDE_ZOEKVENSTER).min(doel / 6);
        Ok(Self {
            lezer: Lezer::open(pad)?,
            buffer: Vec::new(),
            begin: 0,
            doel,
            zoekvenster,
            einde: false,
        })
    }

    /// De duur volgens de container; `None` als die er niet in staat.
    #[must_use]
    pub fn duur_volgens_container(&self) -> Option<Duration> {
        self.lezer.duur_volgens_container()
    }

    /// Wat er na het lezen van het hele bestand opviel (zie [`Lezer::waarschuwing`]).
    #[must_use]
    pub fn waarschuwing(&self) -> Option<Leeswaarschuwing> {
        self.lezer.waarschuwing()
    }

    /// Hoe het bestand eruitzag, voor zover nu gelezen.
    #[must_use]
    pub fn bron(&self) -> Option<BronInfo> {
        self.lezer.bron()
    }

    /// Het volgende stuk, of `None` als het bestand op is.
    ///
    /// # Errors
    ///
    /// Geeft een [`AudioFout`] als het bestand halverwege niet verder te lezen is.
    pub fn volgende(&mut self) -> Result<Option<Stuk>, AudioFout> {
        while !self.einde && self.buffer.len() < self.doel {
            match self.lezer.lees_blok()? {
                Some(blok) => self.buffer.extend_from_slice(&blok),
                None => self.einde = true,
            }
        }
        if self.buffer.is_empty() {
            return Ok(None);
        }
        let snede = if self.buffer.len() <= self.doel {
            self.buffer.len()
        } else {
            stilste_snede(&self.buffer[..self.doel], self.zoekvenster)
        };
        let rest = self.buffer.split_off(snede);
        let samples = std::mem::replace(&mut self.buffer, rest);
        let stuk = Stuk {
            begin: self.begin,
            samples,
        };
        self.begin += snede;
        Ok(Some(stuk))
    }
}

/// Deelt geluid (16 kHz mono) op in aaneengesloten delen van hooguit `max`, gesneden op
/// het stilste moment, met dezelfde regel als [`Stukken`]: gezocht wordt in het laatste
/// deel van elk stuk (hooguit [`SNEDE_ZOEKVENSTER`], en hooguit een zesde van `max`). Geeft
/// de delen als bereiken in `samples`, op volgorde; leeg geluid geeft geen delen.
#[must_use]
pub fn deel_op_stilte(samples: &[f32], max: Duration) -> Vec<std::ops::Range<usize>> {
    let doel = samples_in(max).max(1);
    let zoekvenster = samples_in(SNEDE_ZOEKVENSTER).min(doel / 6);
    let mut delen = Vec::new();
    let mut begin = 0;
    while samples.len() - begin > doel {
        // Een snede op 0 schiet niet op; dan snijden we hard op de grens.
        let snede = match stilste_snede(&samples[begin..begin + doel], zoekvenster) {
            0 => doel,
            snede => snede,
        };
        delen.push(begin..begin + snede);
        begin += snede;
    }
    if begin < samples.len() {
        delen.push(begin..samples.len());
    }
    delen
}

/// Aantal samples (16 kHz) in een duur.
fn samples_in(duur: Duration) -> usize {
    usize::try_from(duur.as_millis() * u128::from(DOEL_SAMPLERATE) / 1000).unwrap_or(usize::MAX)
}

/// Waar we een stuk snijden: midden in het stilste venster van [`SNEDE_VENSTER_MS`] binnen
/// de laatste `zoekvenster` samples. Het venster loopt in frames van 30 ms; bij gelijke
/// stilte wint het laatste, zodat stukken zo lang mogelijk blijven.
fn stilste_snede(samples: &[f32], zoekvenster: usize) -> usize {
    let frame = SNEDE_FRAME_MS * DOEL_SAMPLERATE as usize / 1000;
    let per_venster = (SNEDE_VENSTER_MS / SNEDE_FRAME_MS).max(1);
    let begin = samples.len().saturating_sub(zoekvenster);
    let energie: Vec<f64> = samples[begin..]
        .chunks_exact(frame)
        .map(|f| f.iter().map(|&s| f64::from(s) * f64::from(s)).sum())
        .collect();
    if energie.len() < per_venster {
        return samples.len();
    }
    let mut beste = (f64::INFINITY, 0);
    let mut som: f64 = energie[..per_venster].iter().sum();
    for start in 0..=energie.len() - per_venster {
        if start > 0 {
            som += energie[start + per_venster - 1] - energie[start - 1];
        }
        if som <= beste.0 {
            beste = (som, start);
        }
    }
    begin + (beste.1 * frame) + (per_venster * frame) / 2
}

/// Leest een audiobestand in één keer en zet het om naar 16 kHz mono. Voor lange
/// bestanden is [`Stukken`] zuiniger.
///
/// # Errors
///
/// Geeft een [`AudioFout`] in gewone taal als het bestand ontbreekt, niet ondersteund
/// of beschadigd is, of geen geluid bevat.
pub fn laad(pad: &Path) -> Result<Geluid, AudioFout> {
    let mut lezer = Lezer::open(pad)?;
    let mut samples = Vec::new();
    while let Some(blok) = lezer.lees_blok()? {
        samples.extend_from_slice(&blok);
    }
    let bron = lezer.bron().ok_or(AudioFout::GeenAudiospoor)?;
    Ok(Geluid {
        duur: duur_van(samples.len(), DOEL_SAMPLERATE),
        samples,
        bron,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    /// Schrijft een sinus als 16-bit wav in een tijdelijke map.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "de amplitude past altijd in een i16"
    )]
    fn schrijf_sinus(
        map: &Path,
        naam: &str,
        samplerate: u32,
        kanalen: u16,
        hz: f64,
        seconden: u32,
    ) -> PathBuf {
        let pad = map.join(naam);
        let spec = hound::WavSpec {
            channels: kanalen,
            sample_rate: samplerate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut schrijver = hound::WavWriter::create(&pad, spec).unwrap();
        for i in 0..samplerate * seconden {
            let t = f64::from(i) / f64::from(samplerate);
            let waarde = ((TAU * hz * t).sin() * f64::from(i16::MAX) * 0.5) as i16;
            for _ in 0..kanalen {
                schrijver.write_sample(waarde).unwrap();
            }
        }
        schrijver.finalize().unwrap();
        pad
    }

    /// Telt hoe vaak het signaal van negatief naar positief gaat (één per periode).
    fn nuldoorgangen(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count()
    }

    #[test]
    fn stereo_44k_wordt_16k_mono_met_juiste_lengte_en_frequentie() {
        let map = tempfile::tempdir().unwrap();
        let pad = schrijf_sinus(map.path(), "sinus.wav", 44_100, 2, 440.0, 2);
        let geluid = laad(&pad).unwrap();

        let lengte = geluid.samples.len();
        assert!(
            (31_680..=32_320).contains(&lengte),
            "lengte {lengte} wijkt meer dan 1% af van 32000"
        );

        // 440 Hz gedurende 2 seconden geeft 880 periodes.
        let doorgangen = nuldoorgangen(&geluid.samples);
        assert!(
            (870..=890).contains(&doorgangen),
            "frequentie klopt niet: {doorgangen} nuldoorgangen"
        );
        assert_eq!(geluid.bron.samplerate, 44_100);
        assert_eq!(geluid.bron.kanalen, 2);
        assert_eq!(geluid.bron.duur, Duration::from_secs(2));
        assert!(geluid.duur.as_secs_f64() > 1.97 && geluid.duur.as_secs_f64() < 2.03);
    }

    #[test]
    fn mono_16k_blijft_gelijk() {
        let map = tempfile::tempdir().unwrap();
        let pad = schrijf_sinus(map.path(), "sinus.wav", 16_000, 1, 300.0, 1);
        let geluid = laad(&pad).unwrap();
        assert_eq!(geluid.samples.len(), 16_000);
        assert_eq!(geluid.duur, Duration::from_secs(1));
        let doorgangen = nuldoorgangen(&geluid.samples);
        assert!((298..=301).contains(&doorgangen), "{doorgangen}");
    }

    #[test]
    fn ontbrekend_bestand_geeft_niet_gevonden() {
        let map = tempfile::tempdir().unwrap();
        let fout = laad(&map.path().join("bestaat-niet.wav")).unwrap_err();
        assert!(matches!(fout, AudioFout::NietGevonden(_)), "{fout:?}");
        assert!(fout.to_string().contains("niet gevonden"));
    }

    #[test]
    fn tekstbestand_geeft_formaat_niet_ondersteund() {
        let map = tempfile::tempdir().unwrap();
        let pad = map.path().join("tekst.mp3");
        std::fs::write(&pad, "dit is gewoon tekst, geen geluid. ".repeat(200)).unwrap();
        let fout = laad(&pad).unwrap_err();
        assert!(
            matches!(fout, AudioFout::FormaatNietOndersteund),
            "{fout:?}"
        );
    }

    #[test]
    fn duur_komt_uit_de_metadata() {
        let map = tempfile::tempdir().unwrap();
        let pad = schrijf_sinus(map.path(), "sinus.wav", 44_100, 2, 440.0, 3);
        assert_eq!(duur(&pad).unwrap(), Some(Duration::from_secs(3)));
        let pad = schrijf_sinus(map.path(), "mono.wav", 16_000, 1, 300.0, 2);
        assert_eq!(duur(&pad).unwrap(), Some(Duration::from_secs(2)));
        // Tellen in het werkproces geeft hetzelfde, en stopt als dat gevraagd wordt.
        assert_eq!(
            duur_door_te_tellen(&pad, &|| false).unwrap(),
            Some(Duration::from_secs(2))
        );
        assert_eq!(duur_door_te_tellen(&pad, &|| true).unwrap(), None);
    }

    #[test]
    fn een_map_en_geen_toegang_in_gewone_taal() {
        let map = tempfile::tempdir().unwrap();
        let fout = duur(map.path()).unwrap_err();
        assert!(matches!(fout, AudioFout::IsMap), "{fout:?}");
        assert_eq!(fout.to_string(), "Dit is een map. Sleep losse bestanden.");
        let pad = map.path().join("x.wav");
        let geweigerd = std::io::Error::from(ErrorKind::PermissionDenied);
        assert!(matches!(openfout(&pad, &geweigerd), AudioFout::GeenToegang));
        // Een onbekende fout van het besturingssysteem komt niet in de melding.
        let ander = std::io::Error::other("os error 5");
        let melding = openfout(&pad, &ander).to_string();
        assert!(!melding.contains("os error"), "{melding}");
        #[cfg(windows)]
        assert!(matches!(
            openfout(&pad, &std::io::Error::from_raw_os_error(32)),
            AudioFout::InGebruik
        ));
    }

    #[test]
    fn een_afgekapt_bestand_geeft_een_waarschuwing_met_de_tijd() {
        let map = tempfile::tempdir().unwrap();
        let pad = schrijf_sinus(map.path(), "lang.wav", 16_000, 1, 300.0, 10);
        // Afgekapt na ongeveer 4 seconden; de kop zegt nog steeds 10 seconden.
        let bytes = std::fs::read(&pad).unwrap();
        std::fs::write(&pad, &bytes[..44 + 4 * 16_000 * 2]).unwrap();
        let mut stukken = Stukken::open(&pad).unwrap();
        while stukken.volgende().unwrap().is_some() {}
        let waarschuwing = stukken.waarschuwing().unwrap();
        assert_eq!(waarschuwing, Leeswaarschuwing::Afgebroken { tot_ms: 4000 });
        assert_eq!(
            waarschuwing.melding(),
            "Het bestand lijkt afgebroken; de tekst stopt bij 00:04."
        );
        // Een heel bestand geeft geen waarschuwing.
        let heel = schrijf_sinus(map.path(), "heel.wav", 16_000, 1, 300.0, 3);
        let mut stukken = Stukken::open(&heel).unwrap();
        while stukken.volgende().unwrap().is_some() {}
        assert_eq!(stukken.waarschuwing(), None);
    }

    #[test]
    fn video_herkennen_aan_de_extensie() {
        assert!(is_video(Path::new("Teams-overleg.MP4")));
        assert!(!is_video(Path::new("spraakbericht.opus")));
        assert!(!is_video(Path::new("zonder-extensie")));
        assert!(VIDEO_EXTENSIES.iter().all(|v| EXTENSIES.contains(v)));
        assert_eq!(mm_ss(3_760_000), "1:02:40");
    }

    #[test]
    fn duur_geeft_dezelfde_fouten_als_laden() {
        let map = tempfile::tempdir().unwrap();
        let fout = duur(&map.path().join("bestaat-niet.wav")).unwrap_err();
        assert!(matches!(fout, AudioFout::NietGevonden(_)), "{fout:?}");
        let pad = map.path().join("tekst.mp3");
        std::fs::write(&pad, "dit is gewoon tekst, geen geluid. ".repeat(200)).unwrap();
        assert!(matches!(
            duur(&pad).unwrap_err(),
            AudioFout::FormaatNietOndersteund
        ));
    }

    #[test]
    fn wisselende_indeling_wordt_herkend() {
        let mut eerste = None;
        controleer_indeling(&mut eerste, 44_100, 2).unwrap();
        // Hetzelfde formaat blijft goed.
        controleer_indeling(&mut eerste, 44_100, 2).unwrap();
        let fout = controleer_indeling(&mut eerste, 48_000, 2).unwrap_err();
        assert!(
            matches!(
                fout,
                AudioFout::WisselendeIndeling {
                    van_hz: 44_100,
                    naar_hz: 48_000,
                    ..
                }
            ),
            "{fout:?}"
        );
        let fout = controleer_indeling(&mut eerste, 44_100, 1).unwrap_err();
        assert!(fout.to_string().contains("verandert halverwege"));
    }

    #[test]
    fn afgekapt_wav_geeft_geen_paniek() {
        let map = tempfile::tempdir().unwrap();
        let pad = schrijf_sinus(map.path(), "sinus.wav", 16_000, 1, 300.0, 1);
        let bytes = std::fs::read(&pad).unwrap();
        // Alleen de kop, zonder geluidsdata.
        std::fs::write(&pad, &bytes[..44]).unwrap();
        assert!(laad(&pad).is_err());
    }

    /// Omrekenen zoals het vroeger ging: alles in één keer met `process_all`.
    fn in_een_keer(mono: &[f32], samplerate: u32) -> Vec<f32> {
        let mut resampler = Fft::<f32>::new(
            samplerate as usize,
            DOEL_SAMPLERATE as usize,
            RESAMPLE_BLOK,
            1,
            FixedSync::Both,
        )
        .unwrap();
        let invoer = InterleavedSlice::new(mono, 1, mono.len()).unwrap();
        resampler
            .process_all(&invoer, mono.len(), None)
            .unwrap()
            .take_data()
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "testsignaal"
    )]
    fn testsignaal(lengte: usize) -> Vec<f32> {
        (0..lengte)
            .map(|i| ((i as f64 * 0.031).sin() * 0.4 + (i as f64 * 0.0007).cos() * 0.1) as f32)
            .collect()
    }

    /// Rekent om met de omrekenaar, in blokken van de gegeven groottes (herhaald).
    fn omrekenen(mono: &[f32], samplerate: u32, groottes: &[usize]) -> Vec<f32> {
        let mut omrekenaar = Omrekenaar::nieuw(samplerate).unwrap();
        let mut uit = Vec::new();
        let mut rest = mono;
        for grootte in groottes.iter().cycle() {
            if rest.is_empty() {
                break;
            }
            let (blok, over) = rest.split_at((*grootte).min(rest.len()));
            omrekenaar.voeg_toe(blok, &mut uit).unwrap();
            rest = over;
        }
        omrekenaar.sluit(&mut uit).unwrap();
        uit
    }

    fn bitgelijk(a: &[f32], b: &[f32]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "verwachte lengte zoals rubato die uitrekent"
    )]
    fn omrekenen_hangt_niet_af_van_blokken_of_van_hoe_rubato_afkapt() {
        for samplerate in [8_000, 22_050, 44_100, 48_000] {
            for lengte in [0, 1, 500, 1024, 1025, 3000, 20_000, 100_003] {
                let mono = testsignaal(lengte);
                // Wisselende blokgroottes, zoals pakketten uit een bestand, en alles in één keer.
                let in_blokken = omrekenen(&mono, samplerate, &[1152, 1, 4096, 7, 960]);
                let in_een_blok = omrekenen(&mono, samplerate, &[usize::MAX]);
                assert!(
                    bitgelijk(&in_blokken, &in_een_blok),
                    "{samplerate} Hz, {lengte}"
                );
                // Precies ceil(verhouding × invoer) frames, voor elke lengte.
                let verwacht = (f64::from(DOEL_SAMPLERATE) / f64::from(samplerate) * lengte as f64)
                    .ceil() as usize;
                assert_eq!(in_blokken.len(), verwacht, "{samplerate} Hz, {lengte}");
                // Lang genoeg om bij elke versie de vertraging af te kappen: gelijk aan
                // `process_all`, dus gelijk aan het vroegere inlezen in één keer.
                if lengte >= 20_000 {
                    assert!(
                        bitgelijk(&in_blokken, &in_een_keer(&mono, samplerate)),
                        "{samplerate} Hz, {lengte}: anders dan process_all"
                    );
                }
            }
        }
    }

    #[test]
    fn de_vertraging_gaat_er_ook_bij_een_heel_kort_fragment_af() {
        // Een stap van stilte naar 1: bij 48 kHz naar 16 kHz hoort de stap rond invoerframe
        // 300 te liggen, dus rond uitvoerframe 100, niet `output_delay` frames later.
        let mut mono = vec![0.0_f32; 300];
        mono.extend(vec![1.0_f32; 300]);
        let uit = omrekenen(&mono, 48_000, &[usize::MAX]);
        let stap = uit.iter().position(|&s| s > 0.5).unwrap();
        assert!((95..=105).contains(&stap), "stap op {stap}");
    }

    /// Schrijft 16 kHz mono met een toon, behalve in de stille stukken (in seconden).
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "testsignaal"
    )]
    fn schrijf_met_stiltes(map: &Path, seconden: f64, stil: &[(f64, f64)]) -> PathBuf {
        let pad = map.join("met-stiltes.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut schrijver = hound::WavWriter::create(&pad, spec).unwrap();
        for i in 0..(seconden * 16_000.0) as usize {
            let t = i as f64 / 16_000.0;
            let is_stil = stil.iter().any(|(van, tot)| t >= *van && t < *tot);
            let waarde = if is_stil {
                0.0
            } else {
                (TAU * 300.0 * t).sin() * 8000.0
            };
            schrijver.write_sample(waarde as i16).unwrap();
        }
        schrijver.finalize().unwrap();
        pad
    }

    #[test]
    #[expect(clippy::cast_precision_loss, reason = "test")]
    fn stukken_snijden_in_de_stilte_en_vormen_samen_het_hele_bestand() {
        let map = tempfile::tempdir().unwrap();
        let stil = [(4.5, 4.8), (9.2, 9.5)];
        let pad = schrijf_met_stiltes(map.path(), 12.0, &stil);
        let mut stukken = Stukken::open_met(&pad, Duration::from_secs(5)).unwrap();
        assert_eq!(
            stukken.duur_volgens_container(),
            Some(Duration::from_secs(12))
        );
        let mut alles = Vec::new();
        let mut begins = Vec::new();
        while let Some(stuk) = stukken.volgende().unwrap() {
            assert_eq!(stuk.begin, alles.len(), "stukken sluiten op elkaar aan");
            assert!(stuk.samples.len() <= 5 * 16_000);
            begins.push(stuk.begin);
            alles.extend_from_slice(&stuk.samples);
        }
        assert_eq!(alles, laad(&pad).unwrap().samples);
        // Elke snede valt in een stilte.
        assert_eq!(begins.len(), 3, "{begins:?}");
        for (snede, (van, tot)) in begins[1..].iter().zip(stil) {
            let t = *snede as f64 / 16_000.0;
            assert!(
                t > van && t < tot,
                "snede op {t} s valt niet in {van}..{tot}"
            );
        }
    }

    #[test]
    fn zonder_stilte_wordt_er_op_de_stuklengte_gesneden() {
        let map = tempfile::tempdir().unwrap();
        let pad = schrijf_met_stiltes(map.path(), 7.0, &[]);
        let mut stukken = Stukken::open_met(&pad, Duration::from_secs(3)).unwrap();
        let mut lengtes = Vec::new();
        while let Some(stuk) = stukken.volgende().unwrap() {
            lengtes.push(stuk.samples.len());
        }
        assert_eq!(lengtes.iter().sum::<usize>(), 7 * 16_000);
        assert!(lengtes.iter().all(|l| *l <= 3 * 16_000), "{lengtes:?}");
        assert!(lengtes[0] > 2 * 16_000, "{lengtes:?}");
    }

    fn testbestand(naam: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("bestanden")
            .join(naam)
    }

    #[test]
    fn video_en_opus_worden_gelezen() {
        for (naam, formaat) in [
            ("sinus.ogg", "ogg"),
            ("sinus.opus", "ogg"),
            ("sinus.webm", "matroska"),
            ("sinus.mkv", "matroska"),
            ("sinus.mp4", "isomp4"),
            ("sinus.mov", "isomp4"),
        ] {
            let geluid = laad(&testbestand(naam));
            assert!(geluid.is_ok(), "{naam}: {geluid:?}");
            let geluid = geluid.unwrap();
            assert_eq!(geluid.bron.formaat, formaat, "{naam}");
            let seconden = geluid.duur.as_secs_f64();
            assert!((1.9..2.2).contains(&seconden), "{naam}: {seconden} s");
            // 440 Hz gedurende ongeveer 2 seconden.
            let doorgangen = nuldoorgangen(&geluid.samples);
            assert!((850..=900).contains(&doorgangen), "{naam}: {doorgangen}");
        }
    }

    #[test]
    fn avi_en_een_te_groot_videoblok_geven_een_melding_met_wat_wel_kan() {
        for naam in ["sinus.avi", "hvcc-te-groot.mov"] {
            let fout = laad(&testbestand(naam)).unwrap_err();
            assert!(
                matches!(fout, AudioFout::FormaatNietOndersteund),
                "{naam}: {fout:?}"
            );
            assert!(fout.to_string().contains("als mp4 of wav"), "{fout}");
        }
    }

    #[test]
    fn stilste_snede_kiest_het_stille_venster() {
        let mut samples = vec![0.5_f32; 16_000];
        samples[10_000..12_000].fill(0.0);
        let snede = stilste_snede(&samples, 8_000);
        assert!((10_000..12_000).contains(&snede), "{snede}");
        // Te kort om te zoeken: aan het eind.
        assert_eq!(stilste_snede(&samples[..100], 8_000), 100);
    }

    #[test]
    fn deel_op_stilte_snijdt_lang_geluid_op_een_stil_moment() {
        // 5 s geluid met een stil moment rond 3,5 s, in delen van hooguit 4 s.
        let mut samples = vec![0.5_f32; 80_000];
        samples[55_000..57_000].fill(0.0);
        let delen = deel_op_stilte(&samples, Duration::from_secs(4));
        assert_eq!(delen.len(), 2);
        assert_eq!(delen[0].start, 0);
        assert!((55_000..57_000).contains(&delen[0].end), "{delen:?}");
        assert_eq!(delen[1], delen[0].end..80_000);
        // Kort geluid blijft heel; leeg geluid geeft niets.
        assert_eq!(
            deel_op_stilte(&samples[..100], Duration::from_secs(4)),
            vec![0..100]
        );
        assert!(deel_op_stilte(&[], Duration::from_secs(4)).is_empty());
        // Elk deel is hooguit de grens, en samen beslaan ze alles.
        let lang = vec![0.5_f32; 16_000 * 95];
        let delen = deel_op_stilte(&lang, Duration::from_secs(30));
        assert!(delen.iter().all(|d| d.len() <= 16_000 * 30), "{delen:?}");
        assert!(delen.windows(2).all(|p| p[0].end == p[1].start));
        assert_eq!(delen.last().map(|d| d.end), Some(lang.len()));
    }
}
