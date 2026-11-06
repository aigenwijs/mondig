//! Handmatige test met een echt model en een echt audiobestand.
//!
//! Draait niet vanzelf (`#[ignore]`). Starten met bijvoorbeeld:
//!
//! ```text
//! MONDIG_TEST_AUDIO=pad/naar/spraak.wav \
//! MONDIG_TEST_MODELMAP=pad/naar/modellen/whisper-large-v3-turbo-q8 \
//! MONDIG_TEST_MODEL=whisper-large-v3-turbo-q8 \
//! cargo test -p mondig-engine --test echt_model -- --ignored --nocapture
//! ```
//!
//! De test controleert alleen dat er tekst uitkomt; de kwaliteit meet `mondig-cli bench`
//! (sprint 2c). Audio hoort nooit in de repo.

use std::path::PathBuf;
use std::time::Duration;

use mondig_core::aanbieder::{Aanbieder, Invoer, LokaleEngine};
use mondig_core::audio;
use mondig_core::engine::protocol::Apparaat;
use mondig_core::engine::supervisor::Instellingen;
use mondig_core::model;

fn omgevingswaarde(naam: &str) -> String {
    let waarde = std::env::var(naam);
    assert!(waarde.is_ok(), "zet de omgevingsvariabele {naam}");
    waarde.unwrap_or_default()
}

#[test]
#[ignore = "heeft een gedownload model en een audiobestand nodig"]
fn echt_model_schrijft_spraak_uit() {
    let audio = PathBuf::from(omgevingswaarde("MONDIG_TEST_AUDIO"));
    let modelmap = PathBuf::from(omgevingswaarde("MONDIG_TEST_MODELMAP"));
    let model_id = std::env::var("MONDIG_TEST_MODEL")
        .unwrap_or_else(|_| "whisper-large-v3-turbo-q8".to_string());
    let model = model::zoek(&model_id).unwrap();

    let mut instellingen =
        Instellingen::standaard(PathBuf::from(env!("CARGO_BIN_EXE_mondig-engine")));
    instellingen.laad_timeout = Duration::from_secs(300);
    let mut engine = LokaleEngine::nieuw(
        instellingen,
        &model,
        modelmap,
        Apparaat::Cpu,
        Some("nl".into()),
    );
    let transcript = engine.transcribeer(Invoer::Bestand(&audio)).unwrap();
    assert!(!transcript.tekst().is_empty(), "er kwam geen tekst uit");
    assert!(transcript.audio_duur > Duration::ZERO);
}

#[test]
#[ignore = "heeft een gedownload model en een audiobestand nodig"]
fn bestand_en_geluid_uit_het_geheugen_geven_dezelfde_tekst() {
    let audio_pad = PathBuf::from(omgevingswaarde("MONDIG_TEST_AUDIO"));
    let modelmap = PathBuf::from(omgevingswaarde("MONDIG_TEST_MODELMAP"));
    let model_id = std::env::var("MONDIG_TEST_MODEL")
        .unwrap_or_else(|_| "whisper-large-v3-turbo-q8".to_string());
    let model = model::zoek(&model_id).unwrap();

    let mut instellingen =
        Instellingen::standaard(PathBuf::from(env!("CARGO_BIN_EXE_mondig-engine")));
    instellingen.laad_timeout = Duration::from_secs(300);
    let mut engine = LokaleEngine::nieuw(
        instellingen,
        &model,
        modelmap,
        Apparaat::Cpu,
        Some("nl".into()),
    );

    let via_bestand = engine.transcribeer(Invoer::Bestand(&audio_pad)).unwrap();
    let geluid = audio::laad(&audio_pad).unwrap();
    let via_geluid = engine
        .transcribeer(Invoer::Geluid(&geluid.samples))
        .unwrap();

    assert!(!via_bestand.tekst().is_empty(), "er kwam geen tekst uit");
    assert_eq!(
        via_bestand.tekst(),
        via_geluid.tekst(),
        "bestand en geluid uit het geheugen horen dezelfde tekst te geven"
    );
    assert_eq!(via_bestand.audio_duur, via_geluid.audio_duur);
}

/// Zachte, vaste ruis (geen digitale stilte), zodat het stuk "stil maar niet leeg" is.
fn ruis(seconden: usize) -> Vec<f32> {
    let mut x: u32 = 12_345;
    (0..seconden * 16_000)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (f32::from(u8::try_from(x >> 24).unwrap_or(0)) / 255.0 - 0.5) * 0.004
        })
        .collect()
}

#[test]
#[ignore = "heeft een gedownload model, Silero naast de modelmap en een audiobestand met spraak nodig"]
fn stilte_knippen_met_silero_naast_een_spraakmodel() {
    use mondig_core::filter::{FilterInstellingen, Uitkomst};

    let audio_pad = PathBuf::from(omgevingswaarde("MONDIG_TEST_AUDIO"));
    let modelmap = PathBuf::from(omgevingswaarde("MONDIG_TEST_MODELMAP"));
    let model_id =
        std::env::var("MONDIG_TEST_MODEL").unwrap_or_else(|_| "parakeet-ultra-q8".to_string());
    let model = model::zoek(&model_id).unwrap();
    let mut instellingen =
        Instellingen::standaard(PathBuf::from(env!("CARGO_BIN_EXE_mondig-engine")));
    instellingen.laad_timeout = Duration::from_secs(300);
    let mut engine = LokaleEngine::nieuw(instellingen, &model, modelmap, Apparaat::Cpu, None);
    let filter = FilterInstellingen::standaard();

    // Drie seconden ruis voor en na de spraak.
    let spraak = audio::laad(&audio_pad).unwrap().samples;
    let mut geluid = ruis(3);
    geluid.extend_from_slice(&spraak);
    geluid.extend(ruis(3));

    let zonder = engine
        .transcribeer_dictaat(&geluid, &filter, false)
        .unwrap();
    let met = engine.transcribeer_dictaat(&geluid, &filter, true).unwrap();
    eprintln!(
        "zonder knippen: {} segment(en)",
        zonder.transcript.segmenten.len()
    );
    eprintln!(
        "met knippen: {} segment(en)",
        met.transcript.segmenten.len()
    );
    assert_eq!(met.uitkomst, Uitkomst::Tekst);
    let eerste = met.transcript.segmenten.first().unwrap();
    // De spraak begint na 3 s; de tijdcodes horen bij de hele opname.
    eprintln!(
        "eerste segment met knippen begint op {} ms",
        eerste.start_ms
    );
    eprintln!(
        "eerste segment zonder knippen begint op {} ms",
        zonder.transcript.segmenten[0].start_ms
    );
    assert!(eerste.start_ms >= 2_500, "{}", eerste.start_ms);
    assert_eq!(met.transcript.audio_duur, zonder.transcript.audio_duur);

    // Alleen ruis: Silero vindt geen spraak, het model hoort niets en er komt geen verzonnen tekst.
    let ruis_zonder = engine
        .transcribeer_dictaat(&ruis(4), &filter, false)
        .unwrap();
    eprintln!(
        "alleen ruis zonder knippen: {} segment(en), uitkomst {:?}",
        ruis_zonder.transcript.segmenten.len(),
        ruis_zonder.uitkomst
    );
    let leeg = engine
        .transcribeer_dictaat(&ruis(4), &filter, true)
        .unwrap();
    assert!(leeg.transcript.segmenten.is_empty());
    assert_eq!(leeg.uitkomst, Uitkomst::GeenTekst);
}

#[test]
#[ignore = "heeft een gedownload model, Silero naast de modelmap en een audiobestand met spraak nodig"]
fn spoor_van_een_vergadering_per_uiting() {
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;

    use mondig_core::spoor::stukken::SpoorSchrijver;
    use mondig_core::spoor::{EINDE, MICROFOON};

    let audio_pad = PathBuf::from(omgevingswaarde("MONDIG_TEST_AUDIO"));
    let modelmap = PathBuf::from(omgevingswaarde("MONDIG_TEST_MODELMAP"));
    let model_id =
        std::env::var("MONDIG_TEST_MODEL").unwrap_or_else(|_| model::STANDAARD_MODEL.to_string());
    let model = model::zoek(&model_id).unwrap();
    let mut instellingen =
        Instellingen::achtergrond(PathBuf::from(env!("CARGO_BIN_EXE_mondig-engine")));
    instellingen.laad_timeout = Duration::from_secs(300);
    let mut engine = LokaleEngine::nieuw(instellingen, &model, modelmap, Apparaat::Cpu, None);

    // Vijf keer de spraak met 2 s ruis ertussen, in stukken van 20 s: zo valt er spraak op
    // de stukgrenzen en over de rand van een venster van een minuut.
    let spraak = audio::laad(&audio_pad).unwrap().samples;
    let mut geluid = ruis(2);
    for _ in 0..5 {
        geluid.extend_from_slice(&spraak);
        geluid.extend(ruis(2));
    }
    let opname = tempfile::tempdir().unwrap();
    let map = opname.path().join(MICROFOON);
    let begin = Instant::now();
    let mut schrijver = SpoorSchrijver::nieuw(&map, begin, false, Duration::from_secs(20)).unwrap();
    for (i, blok) in geluid.chunks(480).enumerate() {
        let tot = Duration::from_millis(u64::try_from((i + 1) * 30).unwrap());
        schrijver.blok(blok, begin + tot, None).unwrap();
    }
    schrijver.sluit(None).unwrap();
    std::fs::write(opname.path().join(EINDE), "{}").unwrap();

    let mut standen = 0;
    let uit = engine
        .transcribeer_spoor(&map, None, &mut |_| standen += 1, &AtomicBool::new(false))
        .unwrap();
    let segmenten = uit.gefilterd.transcript.segmenten;
    for s in &segmenten {
        eprintln!(
            "{:>7} - {:>7} ms: {} tekens",
            s.start_ms,
            s.eind_ms,
            s.tekst.len()
        );
    }
    eprintln!("{standen} voortgangen");
    assert!(segmenten.len() >= 5, "{} segmenten", segmenten.len());
    assert!(segmenten.windows(2).all(|p| p[0].eind_ms <= p[1].start_ms));
    assert!(segmenten.iter().all(|s| s.eind_ms - s.start_ms <= 30_000));
    let duur = u64::try_from(audio::duur_van_samples(geluid.len()).as_millis()).unwrap();
    assert!(segmenten.last().unwrap().eind_ms <= duur + 100);
    assert!(segmenten.first().unwrap().start_ms >= 1_500);
}
