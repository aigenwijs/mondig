//! Zoekt de videokaart via het werkproces en meet hetzelfde geluid op de processor en op de
//! videokaart, met dezelfde route als de knop "Snelheid testen" in de app
//! (`mondig_core::snelheid`): laden apart, opwarmen, dan meten.
//!
//! ```text
//! cargo run --release -p mondig-cli --example meet_videokaart -- <mondig-engine.exe> [model] [audio]
//! ```
//!
//! Zonder `audio` gebruikt het het kunstmatige testgeluid van de app (30 seconden). Met
//! `audio` schrijft het dat bestand uit langs de route van het scherm Bestanden
//! (`LokaleEngine::transcribeer_bestand`, in stukken), na laden en opwarmen. Uitvoer op
//! stdout, één meting per regel, nooit transcripttekst.

#![expect(clippy::print_stdout, reason = "meetuitvoer")]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use mondig_core::aanbieder::{Aanbieder, AanbiederFout, Invoer, LokaleEngine};
use mondig_core::engine::protocol::Apparaat;
use mondig_core::engine::supervisor::{Instellingen, Supervisor};
use mondig_core::model;
use mondig_core::snelheid::{self, Meting};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(engine_pad) = args.first().map(PathBuf::from) else {
        eprintln!("Gebruik: meet_videokaart <mondig-engine.exe> [model] [audio]");
        return ExitCode::FAILURE;
    };
    let model_id = args.get(1).map_or("canary-1b-v2-q8", String::as_str);

    match Supervisor::nieuw(Instellingen::achtergrond(engine_pad.clone())).apparaten() {
        Ok(kaarten) if kaarten.is_empty() => println!("videokaart=geen"),
        Ok(kaarten) => {
            for kaart in kaarten {
                println!("videokaart={} ({:?})", kaart.naam, kaart.soort);
            }
        }
        Err(fout) => println!("videokaart_fout={fout}"),
    }

    let (Ok(model), Ok(map)) = (model::zoek(model_id), model::modelmap(model_id)) else {
        eprintln!("Onbekend model of geen gegevensmap: {model_id}");
        return ExitCode::FAILURE;
    };
    let bestand = args.get(2).map(PathBuf::from);
    let samples = snelheid::testgeluid(snelheid::TESTDUUR);
    println!("model={model_id}");

    let mut metingen: Vec<Option<Meting>> = Vec::new();
    for apparaat in [Apparaat::Cpu, Apparaat::Gpu] {
        let mut engine = LokaleEngine::nieuw(
            Instellingen::achtergrond(engine_pad.clone()),
            &model,
            map.clone(),
            apparaat,
            Some("nl".to_string()),
        );
        let meting = match &bestand {
            Some(pad) => meet_bestand(&mut engine, pad, &samples),
            None => snelheid::meet(&mut engine, &samples),
        };
        engine.stop();
        match &meting {
            Ok(m) => println!(
                "{apparaat:?}: stand={:?} laden_ms={} rekenen_ms={} realtime_factor={:.4}",
                m.stand,
                m.laadtijd.as_millis(),
                m.rekentijd.as_millis(),
                m.realtime_factor()
            ),
            Err(fout) => println!("{apparaat:?}: fout={fout}"),
        }
        metingen.push(meting.ok());
    }
    let Some(Some(processor)) = metingen.first() else {
        return ExitCode::FAILURE;
    };
    let videokaart = metingen
        .get(1)
        .and_then(Option::as_ref)
        .and_then(snelheid::op_videokaart);
    let uitkomst = snelheid::vergelijk(processor.realtime_factor(), videokaart);
    println!(
        "oordeel={:?} keer_sneller={:.1}",
        uitkomst.oordeel,
        uitkomst.keer_sneller.unwrap_or(0.0)
    );
    ExitCode::SUCCESS
}

/// Als `snelheid::meet`, maar met een bestand langs de route van Bestanden: laden, opwarmen met
/// twee seconden testgeluid, dan het hele bestand in stukken.
fn meet_bestand(
    engine: &mut LokaleEngine,
    pad: &Path,
    testgeluid: &[f32],
) -> Result<Meting, AanbiederFout> {
    let begin = Instant::now();
    engine.laad()?;
    let laadtijd = begin.elapsed();
    engine.transcribeer(Invoer::Geluid(&testgeluid[..32_000]))?;
    let begin = Instant::now();
    let uitgeschreven =
        engine.transcribeer_bestand(pad, None, &mut |_| {}, &AtomicBool::new(false))?;
    let rekentijd = begin.elapsed();
    let audio = uitgeschreven.gefilterd.transcript.audio_duur;
    println!("audio_s={:.1}", audio.as_secs_f64());
    Ok(Meting {
        stand: engine
            .rekenstand()
            .unwrap_or(mondig_core::aanbieder::Rekenstand::Processor),
        laadtijd,
        rekentijd,
        audio,
    })
}
