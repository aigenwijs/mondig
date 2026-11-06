//! Meetinstrument voor sprekerherkenning (4b-D, fase D-0): draait Sortformer via
//! transcribe-cpp op een opname en vergelijkt de uitkomst met een referentie.
//!
//! Het hoort niet bij de app en raakt de motor niet. Het meet:
//!
//! - laadtijd, rekentijd en realtime-factor (rekentijd gedeeld door de duur van de opname);
//! - het aantal gevonden sprekers en de spreektijd per spreker;
//! - het aantal sprekerwissels zonder pauze: opvolgende segmenten van verschillende
//!   sprekers met minder dan 300 ms ertussen, of die elkaar overlappen;
//! - met `--referentie`: de score na de best passende koppeling tussen gevonden en echte
//!   sprekers (brute force, tot 4 gevonden tegen hooguit 8 echte).
//!
//! # Gebruik
//!
//! ```text
//! cargo run --release --example sprekers-meting -p mondig-engine -- \
//!     --model <diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf> \
//!     (--audio <wav 16 kHz mono> | --opname <opnamemap> [--spoor microfoon|systeem|gemengd]) \
//!     [--apparaat cpu|gpu] [--threads <n>] [--stand standaard|zeer-hoog|hoog|laag] \
//!     [--referentie <json>] [--uit <json>] [--json]
//! ```
//!
//! Een opnamemap is de map van een vergaderopname (`opname.json`, `einde.json` en de mappen
//! `microfoon/` en `systeem/` met stukken `00000.wav`, ...). `--spoor gemengd` telt beide
//! sporen op (zoals "opname bewaren"); standaard is `systeem` ("Anderen"). De tijden zijn
//! de positie in de stroom van dat spoor, zonder de ankers van de stukken.
//!
//! # Referentieformaat
//!
//! ```json
//! {
//!   "sprekers": ["A", "B"],
//!   "beurten": [{"spreker": "A", "start_ms": 0, "eind_ms": 1234}],
//!   "woorden": [{"spreker": "A", "start_ms": 10, "eind_ms": 200}]
//! }
//! ```
//!
//! `sprekers` en `woorden` zijn optioneel (zonder `sprekers` volgt de lijst uit de beurten).
//! Beurten van verschillende sprekers mogen overlappen. Met `woorden` meet het ook het deel
//! van de woorden waarvan het midden in een segment van de juiste spreker valt.
//!
//! De score op tijd lijkt op DER, op een raster van 10 ms: van de referentiespreektijd
//! (overlap telt per spreker mee) is een deel goed, gemist (niemand gevonden), verwisseld
//! (wel iemand gevonden, maar de verkeerde) of er is een vals alarm (meer sprekers gevonden
//! dan er zijn).
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "dit is een opdrachtregelprogramma"
)]
#![allow(
    clippy::cast_precision_loss,
    reason = "verhoudingen voor weergave; de aantallen zijn ver onder de 2^52"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use mondig_core::spoor::mengen::meng;
use mondig_core::spoor::{MICROFOON, SYSTEEM, Spoorlezer};
use serde_json::{Value, json};
use transcribe_cpp::{
    Backend, BackendMask, DiarizeExtension, DiarizeOptions, DiarizeSessionOptions, Model,
    ModelOptions, SortformerDiarizeOptions, SortformerPreset,
};

/// Rasterbreedte voor de tijdmaat.
const RASTER_MS: i64 = 10;
/// Wissel zonder pauze: minder dan dit tussen twee opvolgende segmenten.
const PAUZE_DREMPEL_MS: i64 = 300;
/// Samplefrequentie van de modellen.
const SAMPLERATE: u32 = 16_000;
/// Meer sprekers past niet in het bitmasker van het raster.
const MAX_SPREKERS: usize = 8;

/// Een door het model gevonden segment. `spreker` loopt van 1 tot en met het maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Gevonden {
    spreker: i32,
    start_ms: i64,
    eind_ms: i64,
}

impl Gevonden {
    /// Nul-gebaseerd nummer van de spreker, voor rasters en koppelingen.
    fn index(&self) -> usize {
        usize::try_from(self.spreker - 1).unwrap_or(0)
    }
}

/// Een beurt of woord uit de referentie; `spreker` is een index in `Referentie::sprekers`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RefTijd {
    spreker: usize,
    start_ms: i64,
    eind_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Referentie {
    sprekers: Vec<String>,
    beurten: Vec<RefTijd>,
    woorden: Vec<RefTijd>,
}

/// Uitkomst van de tijdmaat, in rasterframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct TijdScore {
    /// Totale referentiespreektijd (overlap telt per spreker).
    referentie: i64,
    goed: i64,
    gemist: i64,
    vals_alarm: i64,
    verwisseling: i64,
}

/// Uitkomst van de woordmaat, in aantallen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct WoordScore {
    totaal: usize,
    goed: usize,
    /// Wel in een segment, maar niet van de juiste spreker.
    verkeerd: usize,
    /// In geen enkel segment.
    ongedekt: usize,
}

// ---------- rekenkunde ----------

/// Spreektijd per spreker in milliseconden.
fn spreektijd(segmenten: &[Gevonden]) -> BTreeMap<i32, i64> {
    let mut uit = BTreeMap::new();
    for s in segmenten {
        *uit.entry(s.spreker).or_insert(0) += (s.eind_ms - s.start_ms).max(0);
    }
    uit
}

/// Het aantal opvolgende segmenten (op beginstand gesorteerd) van verschillende sprekers
/// met minder dan `drempel_ms` ertussen; een overlap is een negatieve tussenruimte.
fn wissels_zonder_pauze(segmenten: &[Gevonden], drempel_ms: i64) -> usize {
    let mut gesorteerd = segmenten.to_vec();
    gesorteerd.sort_by_key(|s| (s.start_ms, s.eind_ms, s.spreker));
    gesorteerd
        .windows(2)
        .filter(|paar| {
            paar[0].spreker != paar[1].spreker && paar[1].start_ms - paar[0].eind_ms < drempel_ms
        })
        .count()
}

/// Het rasterframe-bereik dat een interval raakt.
fn frames(start_ms: i64, eind_ms: i64) -> std::ops::Range<usize> {
    let start = usize::try_from(start_ms.max(0) / RASTER_MS).unwrap_or(0);
    let eind = usize::try_from((eind_ms.max(0) + RASTER_MS - 1) / RASTER_MS).unwrap_or(0);
    start..eind.max(start)
}

/// Zet intervallen `(spreker, start, eind)` op een raster van `RASTER_MS`: per frame een
/// bitmasker van de actieve sprekers (bit `spreker`, dus nul-gebaseerd).
fn raster(lengte_ms: i64, items: impl Iterator<Item = (usize, i64, i64)>) -> Vec<u8> {
    let aantal = usize::try_from((lengte_ms.max(0) + RASTER_MS - 1) / RASTER_MS).unwrap_or(0);
    let mut uit = vec![0u8; aantal];
    for (spreker, start, eind) in items {
        for f in frames(start, eind) {
            if let Some(cel) = uit.get_mut(f) {
                *cel |= 1 << spreker;
            }
        }
    }
    uit
}

/// Voor elke gevonden spreker (rij) en echte spreker (kolom): hoeveel frames ze allebei
/// actief zijn.
fn overlap_matrix(
    referentie: &[u8],
    gevonden: &[u8],
    n_gevonden: usize,
    n_ref: usize,
) -> Vec<Vec<i64>> {
    let mut m = vec![vec![0i64; n_ref]; n_gevonden];
    for (r, g) in referentie.iter().zip(gevonden) {
        for (h, rij) in m.iter_mut().enumerate() {
            if g & (1 << h) == 0 {
                continue;
            }
            for (k, cel) in rij.iter_mut().enumerate() {
                if r & (1 << k) != 0 {
                    *cel += 1;
                }
            }
        }
    }
    m
}

/// De koppeling gevonden -> echte spreker met de meeste gedeelde tijd. Elke echte spreker
/// krijgt hooguit een gevonden spreker; een gevonden spreker mag onbekoppeld blijven.
/// Brute force: voor 4 tegen 8 zijn dat enkele duizenden mogelijkheden.
fn beste_koppeling(matrix: &[Vec<i64>]) -> Vec<Option<usize>> {
    fn zoek(
        matrix: &[Vec<i64>],
        h: usize,
        gebruikt: u32,
        huidig: &mut Vec<Option<usize>>,
        score: i64,
        beste: &mut (i64, Vec<Option<usize>>),
    ) {
        if h == matrix.len() {
            if score > beste.0 {
                *beste = (score, huidig.clone());
            }
            return;
        }
        huidig.push(None);
        zoek(matrix, h + 1, gebruikt, huidig, score, beste);
        huidig.pop();
        for (k, waarde) in matrix[h].iter().enumerate() {
            if gebruikt & (1 << k) != 0 {
                continue;
            }
            huidig.push(Some(k));
            zoek(
                matrix,
                h + 1,
                gebruikt | (1 << k),
                huidig,
                score + waarde,
                beste,
            );
            huidig.pop();
        }
    }
    let mut beste = (-1, vec![None; matrix.len()]);
    zoek(matrix, 0, 0, &mut Vec::new(), 0, &mut beste);
    beste.1
}

/// Het bitmasker van de gevonden sprekers omgezet naar echte sprekers via de koppeling.
fn vertaal(masker: u8, koppeling: &[Option<usize>]) -> u8 {
    koppeling
        .iter()
        .enumerate()
        .filter(|(h, _)| masker & (1 << h) != 0)
        .filter_map(|(_, k)| k.map(|k| 1u8 << k))
        .fold(0, |a, b| a | b)
}

/// De tijdmaat: per frame goed, gemist, vals alarm en verwisseling (zoals DER).
fn beoordeel_tijd(referentie: &[u8], gevonden: &[u8], koppeling: &[Option<usize>]) -> TijdScore {
    let mut s = TijdScore::default();
    for (r, g) in referentie.iter().zip(gevonden) {
        let n_ref = i64::from(r.count_ones());
        let n_gev = i64::from(g.count_ones());
        let goed = i64::from((vertaal(*g, koppeling) & r).count_ones());
        s.referentie += n_ref;
        s.goed += goed;
        s.gemist += (n_ref - n_gev).max(0);
        s.vals_alarm += (n_gev - n_ref).max(0);
        s.verwisseling += n_ref.min(n_gev) - goed;
    }
    s
}

/// De woordmaat: valt het midden van het woord in een segment van de juiste spreker?
fn beoordeel_woorden(
    woorden: &[RefTijd],
    gevonden: &[Gevonden],
    koppeling: &[Option<usize>],
) -> WoordScore {
    let mut s = WoordScore {
        totaal: woorden.len(),
        ..Default::default()
    };
    for w in woorden {
        let midden = i64::midpoint(w.start_ms, w.eind_ms);
        let dekkend: Vec<&Gevonden> = gevonden
            .iter()
            .filter(|g| g.start_ms <= midden && midden <= g.eind_ms)
            .collect();
        if dekkend.is_empty() {
            s.ongedekt += 1;
        } else if dekkend.iter().any(|g| {
            usize::try_from(g.spreker - 1)
                .ok()
                .and_then(|h| koppeling.get(h).copied().flatten())
                == Some(w.spreker)
        }) {
            s.goed += 1;
        } else {
            s.verkeerd += 1;
        }
    }
    s
}

fn percentage(deel: i64, totaal: i64) -> f64 {
    if totaal == 0 {
        0.0
    } else {
        100.0 * deel as f64 / totaal as f64
    }
}

// ---------- referentie ----------

fn lees_tijd(v: &Value, wat: &str) -> Result<(String, i64, i64), String> {
    let spreker = v["spreker"]
        .as_str()
        .ok_or_else(|| format!("{wat}: veld \"spreker\" ontbreekt"))?;
    let start = v["start_ms"]
        .as_i64()
        .ok_or_else(|| format!("{wat}: veld \"start_ms\" ontbreekt"))?;
    let eind = v["eind_ms"]
        .as_i64()
        .ok_or_else(|| format!("{wat}: veld \"eind_ms\" ontbreekt"))?;
    Ok((spreker.to_string(), start, eind))
}

fn parse_referentie(v: &Value) -> Result<Referentie, String> {
    let ruw_beurten = v["beurten"]
        .as_array()
        .ok_or("de referentie heeft geen lijst \"beurten\"")?
        .iter()
        .map(|b| lees_tijd(b, "beurt"))
        .collect::<Result<Vec<_>, _>>()?;
    let ruw_woorden = match v.get("woorden") {
        None | Some(Value::Null) => Vec::new(),
        Some(w) => w
            .as_array()
            .ok_or("\"woorden\" moet een lijst zijn")?
            .iter()
            .map(|b| lees_tijd(b, "woord"))
            .collect::<Result<Vec<_>, _>>()?,
    };
    let mut sprekers: Vec<String> = match v.get("sprekers") {
        Some(Value::Array(lijst)) => lijst
            .iter()
            .map(|s| {
                s.as_str()
                    .map(str::to_string)
                    .ok_or("\"sprekers\" moet tekst bevatten")
            })
            .collect::<Result<_, _>>()?,
        _ => Vec::new(),
    };
    if sprekers.is_empty() {
        for (naam, ..) in &ruw_beurten {
            if !sprekers.contains(naam) {
                sprekers.push(naam.clone());
            }
        }
    }
    if sprekers.len() > MAX_SPREKERS {
        return Err(format!(
            "de referentie heeft {} sprekers, het maximum is {MAX_SPREKERS}",
            sprekers.len()
        ));
    }
    let index = |naam: &str| {
        sprekers
            .iter()
            .position(|s| s == naam)
            .ok_or_else(|| format!("spreker \"{naam}\" staat niet in \"sprekers\""))
    };
    let zet = |lijst: Vec<(String, i64, i64)>| -> Result<Vec<RefTijd>, String> {
        lijst
            .into_iter()
            .map(|(naam, start_ms, eind_ms)| {
                Ok(RefTijd {
                    spreker: index(&naam)?,
                    start_ms,
                    eind_ms,
                })
            })
            .collect()
    };
    let beurten = zet(ruw_beurten)?;
    let woorden = zet(ruw_woorden)?;
    Ok(Referentie {
        sprekers,
        beurten,
        woorden,
    })
}

// ---------- audio ----------

fn lees_wav(pad: &Path) -> Result<Vec<f32>, String> {
    let mut lezer = hound::WavReader::open(pad).map_err(|f| format!("{}: {f}", pad.display()))?;
    let spec = lezer.spec();
    if spec.channels != 1 || spec.sample_rate != SAMPLERATE {
        return Err(format!(
            "{}: verwacht 16 kHz mono, dit is {} Hz met {} kanalen",
            pad.display(),
            spec.sample_rate,
            spec.channels
        ));
    }
    let fout = |f: hound::Error| format!("{}: {f}", pad.display());
    match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 16) => lezer
            .samples::<i16>()
            .map(|s| s.map(|s| f32::from(s) / 32768.0).map_err(fout))
            .collect(),
        (hound::SampleFormat::Float, 32) => {
            lezer.samples::<f32>().map(|s| s.map_err(fout)).collect()
        }
        (formaat, bits) => Err(format!(
            "{}: {formaat:?} met {bits} bit wordt niet ondersteund",
            pad.display()
        )),
    }
}

fn lees_spoor(opnamemap: &Path, naam: &str) -> Result<Vec<f32>, String> {
    let mut lezer = Spoorlezer::open(&opnamemap.join(naam))
        .map_err(|f| format!("spoor {naam}: {f} ({f:?})"))?;
    let mut samples = Vec::new();
    loop {
        let blok = lezer
            .lees(usize::try_from(SAMPLERATE).unwrap_or(16_000) * 60)
            .map_err(|f| format!("spoor {naam}: {f} ({f:?})"))?;
        if blok.is_empty() {
            return Ok(samples);
        }
        samples.extend(blok);
    }
}

fn lees_gemengd(opnamemap: &Path) -> Result<Vec<f32>, String> {
    let mut bytes = Vec::new();
    meng(opnamemap, &mut bytes).map_err(|f| format!("mengen: {f} ({f:?})"))?;
    Ok(bytes
        .get(44..)
        .unwrap_or_default()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| f32::from(i16::from_le_bytes(*p)) / 32768.0)
        .collect())
}

// ---------- opdrachtregel ----------

struct Opties {
    model: PathBuf,
    audio: Option<PathBuf>,
    opname: Option<PathBuf>,
    spoor: String,
    gpu: bool,
    threads: i32,
    stand: SortformerPreset,
    stand_naam: String,
    referentie: Option<PathBuf>,
    uit: Option<PathBuf>,
    json: bool,
}

fn lees_opties(args: Vec<String>) -> Result<Opties, String> {
    let mut o = Opties {
        model: PathBuf::new(),
        audio: None,
        opname: None,
        spoor: "systeem".to_string(),
        gpu: false,
        threads: 0,
        stand: SortformerPreset::Default,
        stand_naam: "standaard".to_string(),
        referentie: None,
        uit: None,
        json: false,
    };
    let mut model = None;
    let mut it = args.into_iter();
    while let Some(vlag) = it.next() {
        if vlag == "--json" {
            o.json = true;
            continue;
        }
        let waarde = it
            .next()
            .ok_or_else(|| format!("{vlag} heeft een waarde nodig"))?;
        match vlag.as_str() {
            "--model" => model = Some(PathBuf::from(waarde)),
            "--audio" => o.audio = Some(PathBuf::from(waarde)),
            "--opname" => o.opname = Some(PathBuf::from(waarde)),
            "--spoor" => {
                if !["microfoon", "systeem", "gemengd"].contains(&waarde.as_str()) {
                    return Err("--spoor is microfoon, systeem of gemengd".to_string());
                }
                o.spoor = waarde;
            }
            "--apparaat" => {
                o.gpu = match waarde.as_str() {
                    "cpu" => false,
                    "gpu" => true,
                    _ => return Err("--apparaat is cpu of gpu".to_string()),
                };
            }
            "--threads" => {
                o.threads = waarde
                    .parse()
                    .map_err(|_| "--threads is een getal".to_string())?;
            }
            "--stand" => {
                o.stand = match waarde.as_str() {
                    "standaard" => SortformerPreset::Default,
                    "zeer-hoog" => SortformerPreset::VeryHighLatency,
                    "hoog" => SortformerPreset::HighLatency,
                    "laag" => SortformerPreset::LowLatency,
                    _ => return Err("--stand is standaard, zeer-hoog, hoog of laag".to_string()),
                };
                o.stand_naam = waarde;
            }
            "--referentie" => o.referentie = Some(PathBuf::from(waarde)),
            "--uit" => o.uit = Some(PathBuf::from(waarde)),
            onbekend => return Err(format!("onbekende optie {onbekend}")),
        }
    }
    o.model = model.ok_or("geef --model <gguf>")?;
    if o.audio.is_some() == o.opname.is_some() {
        return Err("geef precies een van --audio en --opname".to_string());
    }
    Ok(o)
}

/// De videokaart-backend van dit platform, zoals in `motor.rs`.
fn gpu_backend() -> Option<Backend> {
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

#[allow(
    clippy::too_many_lines,
    reason = "een rechte lijn van lezen, meten en rapporteren"
)]
fn voer_uit() -> Result<(), String> {
    let o = lees_opties(std::env::args().skip(1).collect())?;
    let referentie = match &o.referentie {
        Some(pad) => {
            let tekst =
                std::fs::read_to_string(pad).map_err(|f| format!("{}: {f}", pad.display()))?;
            let v: Value =
                serde_json::from_str(&tekst).map_err(|f| format!("{}: {f}", pad.display()))?;
            Some(parse_referentie(&v)?)
        }
        None => None,
    };
    let samples = match (&o.audio, &o.opname) {
        (Some(pad), _) => lees_wav(pad)?,
        (None, Some(map)) => match o.spoor.as_str() {
            "microfoon" => lees_spoor(map, MICROFOON)?,
            "systeem" => lees_spoor(map, SYSTEEM)?,
            _ => lees_gemengd(map)?,
        },
        (None, None) => return Err("geen invoer".to_string()),
    };
    let duur_ms =
        i64::try_from(samples.len() as u64 * 1000 / u64::from(SAMPLERATE)).unwrap_or(i64::MAX);

    // Backends en model, zoals het werkproces: eerst het masker, dan Model::load_with.
    let backend = if o.gpu {
        gpu_backend().ok_or("dit platform heeft geen ondersteunde videokaart-backend")?
    } else {
        Backend::Cpu
    };
    let masker = if o.gpu {
        BackendMask::for_backend(backend)
    } else {
        BackendMask::CPU
    };
    let laad_start = Instant::now();
    transcribe_cpp::init_backends_with(None::<&Path>, masker)
        .map_err(|f| format!("de rekenmodules konden niet geladen worden: {f}"))?;
    let model = Model::load_with(
        &o.model,
        &ModelOptions {
            backend,
            ..Default::default()
        },
    )
    .map_err(|f| format!("het model kon niet geladen worden: {f}"))?;
    let info = model
        .diarize_info()
        .map_err(|f| format!("geen sprekermodel: {f}"))?;
    let mut sessie = model
        .diarize_session_with(&DiarizeSessionOptions {
            n_threads: o.threads,
        })
        .map_err(|f| format!("sessie: {f}"))?;
    let laadtijd = laad_start.elapsed();

    let reken_start = Instant::now();
    let ruw = sessie
        .run(
            &samples,
            &DiarizeOptions {
                family: Some(DiarizeExtension::Sortformer(SortformerDiarizeOptions {
                    preset: Some(o.stand),
                })),
            },
        )
        .map_err(|f| format!("sprekerherkenning: {f}"))?;
    let rekentijd = reken_start.elapsed();

    let mut gevonden: Vec<Gevonden> = ruw
        .iter()
        .map(|s| Gevonden {
            spreker: s.speaker_id,
            start_ms: s.t0_ms,
            eind_ms: s.t1_ms,
        })
        .collect();
    gevonden.sort_by_key(|s| (s.start_ms, s.eind_ms, s.spreker));
    if let Some(s) = gevonden
        .iter()
        .find(|s| s.spreker < 1 || s.index() >= MAX_SPREKERS)
    {
        return Err(format!(
            "het model gaf spreker {}, dat past niet in het raster",
            s.spreker
        ));
    }

    let tijden = spreektijd(&gevonden);
    let wissels = wissels_zonder_pauze(&gevonden, PAUZE_DREMPEL_MS);
    let rtf = rekentijd.as_secs_f64() / (duur_ms as f64 / 1000.0).max(f64::EPSILON);

    let mut rapport = json!({
        "model": o.model.file_name().map(|n| n.to_string_lossy().into_owned()),
        "apparaat": if o.gpu { "gpu" } else { "cpu" },
        "backend": model.backend(),
        "threads": o.threads,
        "stand": o.stand_naam,
        "max_sprekers": info.max_speakers,
        "duur_ms": duur_ms,
        "laadtijd_s": laadtijd.as_secs_f64(),
        "rekentijd_s": rekentijd.as_secs_f64(),
        "realtime_factor": rtf,
        "segmenten": gevonden.len(),
        "sprekers_gevonden": tijden.len(),
        "spreektijd_ms": tijden.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>(),
        "wissels_zonder_pauze": wissels,
    });

    if let Some(r) = &referentie {
        let n_hyp = gevonden.iter().map(|s| s.index() + 1).max().unwrap_or(0);
        let lengte = gevonden
            .iter()
            .map(|s| s.eind_ms)
            .chain(r.beurten.iter().chain(&r.woorden).map(|b| b.eind_ms))
            .max()
            .unwrap_or(0);
        let ref_raster = raster(
            lengte,
            r.beurten.iter().map(|b| (b.spreker, b.start_ms, b.eind_ms)),
        );
        let gev_raster = raster(
            lengte,
            gevonden.iter().map(|s| (s.index(), s.start_ms, s.eind_ms)),
        );
        let matrix = overlap_matrix(&ref_raster, &gev_raster, n_hyp, r.sprekers.len());
        let koppeling = beste_koppeling(&matrix);
        let t = beoordeel_tijd(&ref_raster, &gev_raster, &koppeling);
        let koppel_tekst: Vec<Value> = koppeling
            .iter()
            .enumerate()
            .map(|(h, k)| json!({"gevonden": h + 1, "echt": k.map(|k| r.sprekers[k].clone())}))
            .collect();
        rapport["koppeling"] = Value::Array(koppel_tekst);
        rapport["referentie_sprekers"] = json!(r.sprekers);
        rapport["tijd"] = json!({
            "referentie_ms": t.referentie * RASTER_MS,
            "goed_pct": percentage(t.goed, t.referentie),
            "gemist_pct": percentage(t.gemist, t.referentie),
            "vals_alarm_pct": percentage(t.vals_alarm, t.referentie),
            "verwisseling_pct": percentage(t.verwisseling, t.referentie),
            "der_pct": percentage(t.gemist + t.vals_alarm + t.verwisseling, t.referentie),
        });
        if !r.woorden.is_empty() {
            let w = beoordeel_woorden(&r.woorden, &gevonden, &koppeling);
            let p = |n: usize| 100.0 * n as f64 / w.totaal as f64;
            rapport["woorden"] = json!({
                "totaal": w.totaal,
                "goed": w.goed,
                "goed_pct": p(w.goed),
                "verkeerd_pct": p(w.verkeerd),
                "ongedekt_pct": p(w.ongedekt),
            });
        }
    }

    if let Some(pad) = &o.uit {
        let lijst: Vec<Value> = gevonden
            .iter()
            .map(|s| json!({"spreker": s.spreker, "start_ms": s.start_ms, "eind_ms": s.eind_ms}))
            .collect();
        let tekst = serde_json::to_string_pretty(&lijst).map_err(|f| f.to_string())?;
        std::fs::write(pad, tekst).map_err(|f| format!("{}: {f}", pad.display()))?;
    }

    if o.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&rapport).map_err(|f| f.to_string())?
        );
    } else {
        druk_af(&rapport, &tijden);
    }
    Ok(())
}

/// Leesbare weergave van het rapport.
fn druk_af(r: &Value, tijden: &BTreeMap<i32, i64>) {
    let f = |sleutel: &str| r[sleutel].as_f64().unwrap_or(0.0);
    println!(
        "Model {} op {} ({}), stand {}, threads {}",
        r["model"].as_str().unwrap_or("?"),
        r["apparaat"].as_str().unwrap_or("?"),
        r["backend"].as_str().unwrap_or("?"),
        r["stand"].as_str().unwrap_or("?"),
        r["threads"]
    );
    println!(
        "Opname {:.1} s; laden {:.2} s; rekenen {:.2} s; realtime-factor {:.3}",
        f("duur_ms") / 1000.0,
        f("laadtijd_s"),
        f("rekentijd_s"),
        f("realtime_factor")
    );
    println!(
        "{} segmenten, {} sprekers gevonden (model kan {}), {} wissels zonder pauze (< {PAUZE_DREMPEL_MS} ms)",
        r["segmenten"], r["sprekers_gevonden"], r["max_sprekers"], r["wissels_zonder_pauze"]
    );
    for (spreker, ms) in tijden {
        println!("  spreker {spreker}: {:.1} s", *ms as f64 / 1000.0);
    }
    if let Some(k) = r["koppeling"].as_array() {
        let delen: Vec<String> = k
            .iter()
            .map(|e| {
                format!(
                    "{} -> {}",
                    e["gevonden"],
                    e["echt"].as_str().unwrap_or("(geen)")
                )
            })
            .collect();
        println!("Koppeling: {}", delen.join(", "));
    }
    if r["tijd"].is_object() {
        let t = &r["tijd"];
        let g = |s: &str| t[s].as_f64().unwrap_or(0.0);
        println!(
            "Spreektijd bij de juiste spreker {:.1}% (gemist {:.1}%, vals alarm {:.1}%, verwisseld {:.1}%; DER {:.1}%)",
            g("goed_pct"),
            g("gemist_pct"),
            g("vals_alarm_pct"),
            g("verwisseling_pct"),
            g("der_pct")
        );
    }
    if r["woorden"].is_object() {
        let w = &r["woorden"];
        let g = |s: &str| w[s].as_f64().unwrap_or(0.0);
        println!(
            "Woorden bij de juiste spreker {:.1}% van {} (verkeerd {:.1}%, in geen segment {:.1}%)",
            g("goed_pct"),
            w["totaal"],
            g("verkeerd_pct"),
            g("ongedekt_pct")
        );
    }
}

fn main() {
    if let Err(melding) = voer_uit() {
        eprintln!("Fout: {melding}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(spreker: i32, start_ms: i64, eind_ms: i64) -> Gevonden {
        Gevonden {
            spreker,
            start_ms,
            eind_ms,
        }
    }

    fn rt(spreker: usize, start_ms: i64, eind_ms: i64) -> RefTijd {
        RefTijd {
            spreker,
            start_ms,
            eind_ms,
        }
    }

    fn ref_raster(beurten: &[RefTijd], lengte: i64) -> Vec<u8> {
        raster(
            lengte,
            beurten.iter().map(|b| (b.spreker, b.start_ms, b.eind_ms)),
        )
    }

    fn gev_raster(gevonden: &[Gevonden], lengte: i64) -> Vec<u8> {
        raster(
            lengte,
            gevonden.iter().map(|s| (s.index(), s.start_ms, s.eind_ms)),
        )
    }

    #[test]
    fn spreektijd_telt_per_spreker() {
        let t = spreektijd(&[g(1, 0, 1000), g(2, 1000, 1500), g(1, 2000, 2500)]);
        assert_eq!(t.get(&1), Some(&1500));
        assert_eq!(t.get(&2), Some(&500));
    }

    #[test]
    fn wissels_tellen_kleine_pauzes_en_overlap_maar_geen_grote_pauzes() {
        // 1 -> 2 met 100 ms pauze (telt), 2 -> 1 met 500 ms pauze (telt niet),
        // 1 -> 2 met 200 ms overlap (telt), 2 -> 2 (zelfde spreker, telt niet).
        let s = [
            g(1, 0, 1000),
            g(2, 1100, 2000),
            g(1, 2500, 3000),
            g(2, 2800, 3500),
            g(2, 3600, 4000),
        ];
        assert_eq!(wissels_zonder_pauze(&s, PAUZE_DREMPEL_MS), 2);
    }

    #[test]
    fn wissels_sorteren_op_begin() {
        let s = [g(2, 1100, 2000), g(1, 0, 1000)];
        assert_eq!(wissels_zonder_pauze(&s, PAUZE_DREMPEL_MS), 1);
    }

    #[test]
    fn koppeling_vindt_verwisselde_labels() {
        // Echt: A 0-1 s, B 1-2 s. Het model noemt ze andersom (1 = B, 2 = A).
        let beurten = [rt(0, 0, 1000), rt(1, 1000, 2000)];
        let gevonden = [g(2, 0, 1000), g(1, 1000, 2000)];
        let (r, h) = (ref_raster(&beurten, 2000), gev_raster(&gevonden, 2000));
        let koppeling = beste_koppeling(&overlap_matrix(&r, &h, 2, 2));
        assert_eq!(koppeling, vec![Some(1), Some(0)]);
        let s = beoordeel_tijd(&r, &h, &koppeling);
        assert_eq!(s.referentie, 200);
        assert_eq!(s.goed, 200);
        assert_eq!(s.gemist + s.vals_alarm + s.verwisseling, 0);
    }

    #[test]
    fn koppeling_laat_overtollige_gevonden_sprekers_onbekoppeld() {
        // Een echte spreker, twee gevonden: de langste wordt gekoppeld, de andere is vals alarm.
        let beurten = [rt(0, 0, 1000)];
        let gevonden = [g(1, 0, 800), g(2, 800, 1000)];
        let (r, h) = (ref_raster(&beurten, 1000), gev_raster(&gevonden, 1000));
        let koppeling = beste_koppeling(&overlap_matrix(&r, &h, 2, 1));
        assert_eq!(koppeling, vec![Some(0), None]);
        let s = beoordeel_tijd(&r, &h, &koppeling);
        assert_eq!(
            (s.goed, s.verwisseling, s.vals_alarm, s.gemist),
            (80, 20, 0, 0)
        );
    }

    #[test]
    fn tijdmaat_telt_gemist_en_vals_alarm() {
        let beurten = [rt(0, 0, 1000)];
        let gevonden = [g(1, 0, 500), g(1, 800, 1500)];
        let (r, h) = (ref_raster(&beurten, 1500), gev_raster(&gevonden, 1500));
        let koppeling = beste_koppeling(&overlap_matrix(&r, &h, 1, 1));
        let s = beoordeel_tijd(&r, &h, &koppeling);
        assert_eq!(s.referentie, 100);
        assert_eq!(s.goed, 70);
        assert_eq!(s.gemist, 30);
        assert_eq!(s.vals_alarm, 50);
        assert_eq!(s.verwisseling, 0);
    }

    #[test]
    fn tijdmaat_telt_verwisseling() {
        // Echt: A 0-1 s, B 1-2 s. Gevonden: 1 loopt tot 1,5 s en 2 daarna: 0,5 s is verwisseld.
        let beurten = [rt(0, 0, 1000), rt(1, 1000, 2000)];
        let gevonden = [g(1, 0, 1500), g(2, 1500, 2000)];
        let (r, h) = (ref_raster(&beurten, 2000), gev_raster(&gevonden, 2000));
        let koppeling = beste_koppeling(&overlap_matrix(&r, &h, 2, 2));
        assert_eq!(koppeling, vec![Some(0), Some(1)]);
        let s = beoordeel_tijd(&r, &h, &koppeling);
        assert_eq!(
            (s.goed, s.verwisseling, s.gemist, s.vals_alarm),
            (150, 50, 0, 0)
        );
    }

    #[test]
    fn overlappende_referentie_telt_per_spreker() {
        // A en B praten 0,5 s tegelijk; het model vindt maar een van de twee: de rest is gemist.
        let beurten = [rt(0, 0, 1000), rt(1, 500, 1500)];
        let gevonden = [g(1, 0, 1500)];
        let (r, h) = (ref_raster(&beurten, 1500), gev_raster(&gevonden, 1500));
        let koppeling = beste_koppeling(&overlap_matrix(&r, &h, 1, 2));
        let s = beoordeel_tijd(&r, &h, &koppeling);
        assert_eq!(s.referentie, 200);
        assert_eq!(s.gemist, 50);
        assert_eq!(s.goed + s.verwisseling, 150);
    }

    #[test]
    fn woordmaat_telt_goed_verkeerd_en_ongedekt() {
        let gevonden = [g(1, 0, 1000), g(2, 1000, 2000)];
        let koppeling = [Some(0), Some(1)];
        let woorden = [
            rt(0, 100, 300),   // midden 200, in segment 1 (= A): goed
            rt(0, 1100, 1300), // in segment 2 (= B): verkeerd
            rt(1, 3000, 3200), // in geen segment: ongedekt
            rt(1, 1500, 1700), // goed
        ];
        let s = beoordeel_woorden(&woorden, &gevonden, &koppeling);
        assert_eq!(
            s,
            WoordScore {
                totaal: 4,
                goed: 2,
                verkeerd: 1,
                ongedekt: 1
            }
        );
    }

    #[test]
    fn woord_in_overlap_is_goed_als_een_van_de_segmenten_klopt() {
        let gevonden = [g(1, 0, 1000), g(2, 500, 1500)];
        let koppeling = [Some(0), Some(1)];
        let s = beoordeel_woorden(&[rt(1, 700, 900)], &gevonden, &koppeling);
        assert_eq!(s.goed, 1);
    }

    #[test]
    fn referentie_wordt_gelezen_met_en_zonder_sprekerlijst() {
        let v = json!({
            "beurten": [
                {"spreker": "B", "start_ms": 0, "eind_ms": 100},
                {"spreker": "A", "start_ms": 100, "eind_ms": 200}
            ],
            "woorden": [{"spreker": "A", "start_ms": 110, "eind_ms": 150}]
        });
        let r = parse_referentie(&v).unwrap();
        assert_eq!(r.sprekers, vec!["B".to_string(), "A".to_string()]);
        assert_eq!(r.beurten[1], rt(1, 100, 200));
        assert_eq!(r.woorden, vec![rt(1, 110, 150)]);

        let v = json!({"sprekers": ["A", "B"], "beurten": [{"spreker": "B", "start_ms": 0, "eind_ms": 5}]});
        let r = parse_referentie(&v).unwrap();
        assert_eq!(r.beurten[0].spreker, 1);
        assert!(r.woorden.is_empty());
    }

    #[test]
    fn referentie_met_onbekende_spreker_of_zonder_beurten_faalt() {
        let v =
            json!({"sprekers": ["A"], "beurten": [{"spreker": "Z", "start_ms": 0, "eind_ms": 5}]});
        assert!(parse_referentie(&v).is_err());
        assert!(parse_referentie(&json!({"sprekers": ["A"]})).is_err());
    }

    #[test]
    fn opties_eisen_model_en_precies_een_invoer() {
        let args = |l: &[&str]| l.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert!(lees_opties(args(&["--audio", "a.wav"])).is_err());
        assert!(lees_opties(args(&["--model", "m.gguf"])).is_err());
        assert!(lees_opties(args(&["--model", "m", "--audio", "a", "--opname", "o"])).is_err());
        let o = lees_opties(args(&[
            "--model",
            "m",
            "--audio",
            "a",
            "--stand",
            "zeer-hoog",
            "--json",
        ]))
        .unwrap();
        assert!(o.json);
        assert_eq!(o.stand, SortformerPreset::VeryHighLatency);
    }
}
