//! De spraakmotor van het werkproces: transcribe.cpp (ggml/GGUF, ADR 0011).
//!
//! De module vertaalt de protocoltypen naar transcribe-cpp en terug; de rest van het
//! werkproces weet niet hoe een model draait.

use std::path::Path;
use std::sync::OnceLock;

use mondig_core::engine::protocol::{
    Apparaat, FoutSoort, KaartSoort, Segment, SprekerSegment, Videokaart, Woord,
};
use mondig_core::model::Engine;
use mondig_core::spoor::uitingen;
use mondig_core::stilte::spraakbereik;
use transcribe_cpp::{
    Backend, BackendMask, DeviceType, DiarizeExtension, DiarizeOptions, Model, ModelOptions,
    RunOptions, Session, SortformerDiarizeOptions, SortformerPreset, VadOptions, VadSession,
};

type LaadFout = (FoutSoort, String);

/// Een geladen model met zijn sessie.
pub(crate) struct Motor {
    session: Session,
    /// Het apparaat waarop het spraakmodel draait; het sprekermodel krijgt hetzelfde.
    apparaat: Apparaat,
    /// Brontaal als hint, alleen voor families die er iets mee doen (Whisper, Canary).
    taal: Option<String>,
    /// Silero, om stilte aan begin en eind weg te knippen. `None` als het model niet naast
    /// het spraakmodel staat of niet te laden was; uitschrijven werkt dan zonder knippen.
    vad: Option<VadSession>,
}

/// Wat Silero over een opname zegt.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Knip {
    /// Er is spraak; dit deel van de samples (met marge) is wat het model hoort.
    Bereik(std::ops::Range<usize>),
    /// Silero vond geen spraak.
    GeenSpraak,
    /// Er kan niet geknipt worden (geen Silero, of de detectie faalde): gewoon alles uitschrijven.
    NietBeschikbaar,
}

/// De videokaart-backend van dit platform met het masker dat hij nodig heeft, of `None`
/// als dit platform er geen heeft (Mac Intel, Windows op ARM).
const fn gpu_backend() -> Option<Backend> {
    if cfg!(any(
        all(windows, target_arch = "x86_64"),
        target_os = "linux"
    )) {
        Some(Backend::Vulkan)
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(Backend::Metal)
    } else {
        None
    }
}

/// Het masker dat bij het gevraagde apparaat hoort: bij `Cpu` alleen de processor, zodat
/// de videodriver niet eens geladen wordt (ADR 0009), bij `Gpu` ook de videokaart-backend.
fn masker(apparaat: Apparaat) -> BackendMask {
    match apparaat {
        Apparaat::Cpu => BackendMask::CPU,
        Apparaat::Gpu => gpu_backend().map_or(BackendMask::CPU, BackendMask::for_backend),
    }
}

/// Registreert de backends, één keer per proces. Het masker staat daarna vast: de uitkomst
/// (het masker of de foutmelding) wordt bewaard, ook voor latere `laad`-aanroepen.
fn init_backends(apparaat: Apparaat) -> Result<BackendMask, LaadFout> {
    static INIT: OnceLock<Result<BackendMask, String>> = OnceLock::new();
    let toegestaan = masker(apparaat);
    let uitkomst = INIT.get_or_init(|| {
        transcribe_cpp::init_backends_with(None::<&Path>, toegestaan)
            .map(|()| toegestaan)
            .map_err(|f| format!("De rekenmodules konden niet geladen worden: {f}"))
    });
    uitkomst
        .clone()
        .map_err(|melding| (FoutSoort::ModelLaden, melding))
}

/// De videokaarten die ggml ziet, voor de schakelaar "Videokaart gebruiken". Registreert
/// daarvoor de videokaart-backend van dit platform; dat kan alleen als eerste opdracht aan
/// het proces (het masker staat daarna vast). Alleen apparaten van die backend tellen: een
/// processor of een rekenversneller in het werkgeheugen is geen videokaart.
pub(crate) fn videokaarten() -> Result<Vec<Videokaart>, LaadFout> {
    let Some(backend) = gpu_backend() else {
        return Ok(Vec::new());
    };
    let gestart = init_backends(Apparaat::Gpu)?;
    if !gestart.contains(BackendMask::for_backend(backend)) {
        return Err((
            FoutSoort::Verzoek,
            "Het werkproces is zonder videokaart gestart en kan niet meer zoeken.".to_string(),
        ));
    }
    Ok(transcribe_cpp::devices()
        .into_iter()
        .filter_map(|apparaat| {
            let soort = kaart_soort(apparaat.device_type)?;
            eprintln!(
                "[mondig-engine] videokaart: {} ({}, {:?})",
                apparaat.description, apparaat.kind, apparaat.device_type
            );
            Some(Videokaart {
                naam: kaart_naam(&apparaat.description, &apparaat.name),
                soort,
            })
        })
        .collect())
}

/// Los of ingebouwd; `None` voor alles wat geen videokaart is.
fn kaart_soort(soort: DeviceType) -> Option<KaartSoort> {
    match soort {
        DeviceType::Gpu => Some(KaartSoort::Los),
        DeviceType::Igpu => Some(KaartSoort::Ingebouwd),
        DeviceType::Cpu | DeviceType::Accel | DeviceType::Unknown => None,
    }
}

/// De naam voor de interface: de beschrijving van de driver (`NVIDIA GeForce RTX 4090`),
/// of de naam van ggml (`Vulkan0`) als die leeg is.
fn kaart_naam(beschrijving: &str, naam: &str) -> String {
    let beschrijving = beschrijving.trim();
    if beschrijving.is_empty() {
        naam.trim().to_string()
    } else {
        beschrijving.to_string()
    }
}

/// Laadt een GGUF-model. `Gpu` vraagt strikt om de videokaart-backend van dit platform: is
/// die er niet, of is het proces al met alleen de processor gestart, dan komt er een fout
/// en valt de supervisor terug op de processor.
pub(crate) fn laad(
    engine: Engine,
    pad: &Path,
    apparaat: Apparaat,
    taal: Option<&str>,
    vad_pad: Option<&Path>,
) -> Result<Motor, LaadFout> {
    let backend = match apparaat {
        Apparaat::Cpu => Backend::Cpu,
        Apparaat::Gpu => gpu_backend().ok_or_else(|| {
            (
                FoutSoort::ModelLaden,
                "Dit platform heeft geen ondersteunde videokaart-backend.".to_string(),
            )
        })?,
    };
    let gestart = init_backends(apparaat)?;
    if !gestart.contains(BackendMask::for_backend(backend)) {
        return Err((
            FoutSoort::ModelLaden,
            "Het werkproces is zonder videokaart gestart en kan die niet meer laden.".to_string(),
        ));
    }
    let fout = |f: transcribe_cpp::Error| {
        (
            FoutSoort::ModelLaden,
            format!("Het model kon niet geladen worden: {f}"),
        )
    };
    let model = Model::load_with(
        pad,
        &ModelOptions {
            backend,
            ..Default::default()
        },
    )
    .map_err(fout)?;
    eprintln!(
        "[mondig-engine] ggml: {} op {}",
        model.arch(),
        model.backend()
    );
    let session = model.session().map_err(fout)?;
    let taal = match engine {
        Engine::Whisper | Engine::Canary => taal.map(str::to_string),
        // Silero en Sortformer schrijven niets uit en komen hier niet langs (zie `Echt::laden`).
        Engine::Parakeet | Engine::Silero | Engine::Sortformer => None,
    };
    let vad = vad_pad.and_then(laad_vad);
    Ok(Motor {
        session,
        apparaat,
        taal,
        vad,
    })
}

/// Laadt Silero op de processor (het model is 1,2 MB en rekent in milliseconden). Faalt
/// het laden, dan werkt uitschrijven zonder knippen; dat is geen fout voor de gebruiker.
fn laad_vad(pad: &Path) -> Option<VadSession> {
    let sessie = Model::load_with(
        pad,
        &ModelOptions {
            backend: Backend::Cpu,
            ..Default::default()
        },
    )
    .and_then(|model| model.vad_session());
    match sessie {
        Ok(sessie) => {
            eprintln!("[mondig-engine] stiltedetectie geladen");
            Some(sessie)
        }
        Err(f) => {
            eprintln!("[mondig-engine] stiltedetectie niet geladen, we knippen niet: {f}");
            None
        }
    }
}

impl Motor {
    /// Vraagt Silero waar in de opname gesproken wordt (16 kHz mono).
    pub(crate) fn knip(&mut self, samples: &[f32]) -> Knip {
        let Some(vad) = self.vad.as_mut() else {
            return Knip::NietBeschikbaar;
        };
        match vad.run(samples, &VadOptions::default()) {
            Ok(resultaat) => {
                let segmenten: Vec<(i64, i64)> = resultaat
                    .segments
                    .iter()
                    .map(|s| (s.start_sample, s.end_sample))
                    .collect();
                spraakbereik(&segmenten, samples.len()).map_or(Knip::GeenSpraak, Knip::Bereik)
            }
            Err(f) => {
                eprintln!("[mondig-engine] stiltedetectie mislukt, we knippen niet: {f}");
                Knip::NietBeschikbaar
            }
        }
    }

    /// Staat Silero klaar?
    pub(crate) fn heeft_vad(&self) -> bool {
        self.vad.is_some()
    }

    /// De spraakgebieden in geluid (16 kHz mono) als `(begin, eind)` in samples, met de
    /// instellingen voor uitingen ([`uitingen::STILTE_MS`] en [`uitingen::MARGE_MS`]).
    /// Anders dan [`Motor::knip`] is een mislukte detectie hier een fout: zonder Silero
    /// valt een spoor niet in uitingen te verdelen.
    pub(crate) fn spraakgebieden(&mut self, samples: &[f32]) -> Result<Vec<(i64, i64)>, String> {
        let vad = self
            .vad
            .as_mut()
            .ok_or_else(|| "de stiltedetectie is niet geladen".to_string())?;
        let opties = VadOptions {
            min_silence_ms: uitingen::STILTE_MS,
            speech_pad_ms: uitingen::MARGE_MS,
            ..VadOptions::default()
        };
        let resultaat = vad.run(samples, &opties).map_err(|f| f.to_string())?;
        Ok(resultaat
            .segments
            .iter()
            .map(|s| (s.start_sample, s.end_sample))
            .collect())
    }

    /// Zet het token waarmee een lopende berekening wordt afgebroken (annuleren van een
    /// bestand). Goedkoop: het deelt alleen een vlag.
    pub(crate) fn zet_annulering(&mut self, token: &mondig_engine::CancelToken) {
        self.session.set_cancel_token(token);
    }

    /// Schrijft 16 kHz mono samples uit.
    pub(crate) fn transcribeer(
        &mut self,
        samples: &[f32],
        duur_ms: u64,
    ) -> Result<Vec<Segment>, String> {
        let resultaat = self
            .session
            .run(
                samples,
                &RunOptions {
                    language: self.taal.clone(),
                    ..Default::default()
                },
            )
            .map_err(|f| f.to_string())?;
        let ms = |t: i64| u64::try_from(t).unwrap_or(0);
        // Woordtijden waar het model ze geeft (Parakeet en Whisper; Canary niet, ADR 0011).
        // Elk woord hoort bij een segment via `seg_index`.
        let woorden_van = |index: Option<usize>| -> Vec<Woord> {
            resultaat
                .words
                .iter()
                .filter(|w| index.is_none_or(|i| usize::try_from(w.seg_index).ok() == Some(i)))
                .filter(|w| !w.text.trim().is_empty())
                .map(|w| Woord {
                    start_ms: ms(w.t0_ms),
                    eind_ms: ms(w.t1_ms.max(w.t0_ms)),
                    tekst: w.text.trim().to_string(),
                })
                .collect()
        };
        let segmenten: Vec<Segment> = resultaat
            .segments
            .iter()
            .enumerate()
            .map(|(i, s)| Segment {
                woorden: woorden_van(Some(i)),
                ..Segment::nieuw(ms(s.t0_ms), ms(s.t1_ms), s.text.trim())
            })
            .collect();
        if segmenten.is_empty() {
            return Ok(vec![Segment {
                woorden: woorden_van(None),
                ..Segment::nieuw(0, duur_ms, resultaat.text.trim())
            }]);
        }
        Ok(segmenten)
    }

    /// Herkent de sprekers in geluid (16 kHz mono) met Sortformer uit `pad` (ADR 0018). Het
    /// sprekermodel komt op hetzelfde apparaat als het spraakmodel, naast het spraakmodel in
    /// het geheugen, en gaat na afloop weer weg. De standaardstand: in de meting van D-0 was
    /// de stand voor live 30 keer trager en niet beter. `token` breekt de berekening af.
    ///
    /// Tijden in milliseconden vanaf het begin van `samples`, op volgorde van begin.
    ///
    /// Faalt het op de videokaart, dan probeert het werkproces het zelf nog eens op de
    /// processor: het spraakmodel werkt immers, en een herstart van het hele werkproces op de
    /// processor (wat de supervisor bij een videokaartfout doet) zou de rest van de sessie
    /// trager maken. Faalt ook dat, dan is de fout [`FoutSoort::Sprekermodel`], die geen nieuwe
    /// poging uitlokt; het verslag komt dan zonder sprekers.
    pub(crate) fn herken_sprekers(
        &self,
        pad: &Path,
        samples: &[f32],
        token: &mondig_engine::CancelToken,
    ) -> Result<Vec<SprekerSegment>, (FoutSoort, String)> {
        let op = |backend| Self::sprekers_op(backend, pad, samples, token);
        let uitkomst = match self.apparaat {
            Apparaat::Cpu => op(Backend::Cpu),
            // Het spraakmodel draait al op de videokaart, dus de backend bestaat.
            Apparaat::Gpu => match op(gpu_backend().unwrap_or(Backend::Cpu)) {
                Err(f) if !token.is_cancelled() => {
                    eprintln!(
                        "[mondig-engine] sprekerherkenning op de videokaart mislukt, nu op de processor: {f}"
                    );
                    op(Backend::Cpu)
                }
                uitkomst => uitkomst,
            },
        };
        uitkomst.map_err(|melding| (FoutSoort::Sprekermodel, melding))
    }

    /// Eén poging van [`Motor::herken_sprekers`] op `backend`.
    fn sprekers_op(
        backend: Backend,
        pad: &Path,
        samples: &[f32],
        token: &mondig_engine::CancelToken,
    ) -> Result<Vec<SprekerSegment>, String> {
        let laadfout = |f: transcribe_cpp::Error| {
            format!("Het model voor sprekerherkenning kon niet geladen worden: {f}")
        };
        let model = Model::load_with(
            pad,
            &ModelOptions {
                backend,
                ..Default::default()
            },
        )
        .map_err(laadfout)?;
        let mut sessie = model.diarize_session().map_err(laadfout)?;
        sessie.set_cancel_token(token);
        eprintln!("[mondig-engine] sprekerherkenning op {}", model.backend());
        let ruw = sessie
            .run(
                samples,
                &DiarizeOptions {
                    family: Some(DiarizeExtension::Sortformer(SortformerDiarizeOptions {
                        preset: Some(SortformerPreset::Default),
                    })),
                },
            )
            .map_err(|f| format!("Het herkennen van de sprekers is mislukt: {f}"))?;
        let ms = |t: i64| u64::try_from(t).unwrap_or(0);
        let mut segmenten: Vec<SprekerSegment> = ruw
            .iter()
            .filter_map(|s| {
                let spreker = u8::try_from(s.speaker_id).ok().filter(|n| *n >= 1)?;
                (s.t1_ms > s.t0_ms).then(|| SprekerSegment {
                    spreker,
                    start_ms: ms(s.t0_ms),
                    eind_ms: ms(s.t1_ms),
                })
            })
            .collect();
        segmenten.sort_by_key(|s| (s.start_ms, s.eind_ms, s.spreker));
        Ok(segmenten)
    }
}
