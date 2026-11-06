//! Meet het laadbeleid van ADR 0012: laadtijd van het standaardmodel, de eerste en de
//! volgende uitschrijvingen met een warm model, en houdt het werkproces daarna een tijd
//! in rust, zodat `scripts/meet-laden.ps1` het geheugen kan bemonsteren.
//!
//! ```text
//! cargo run --release -p mondig-cli --example meet_laden -- <audio> <mondig-engine.exe> [herhalingen] [rust_s]
//! ```
//!
//! Uitvoer op stdout, één meting per regel (`sleutel=waarde`), nooit transcripttekst.

#![expect(clippy::print_stdout, reason = "meetuitvoer voor het script")]

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use mondig_core::aanbieder::{Invoer, LokaleEngine};
use mondig_core::audio;
use mondig_core::engine::protocol::Apparaat;
use mondig_core::engine::supervisor::Instellingen;
use mondig_core::filter::FilterInstellingen;
use mondig_core::model;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(audio_pad), Some(engine_pad)) = (args.first(), args.get(1)) else {
        eprintln!("Gebruik: meet_laden <audio> <mondig-engine.exe> [herhalingen] [rust_s]");
        return ExitCode::FAILURE;
    };
    let herhalingen: u32 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(3);
    let rust_s: u64 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(15);

    let Ok(model) = model::zoek(model::STANDAARD_MODEL) else {
        eprintln!("Het standaardmodel staat niet in de catalogus.");
        return ExitCode::FAILURE;
    };
    let Ok(map) = model::modelmap(&model.id) else {
        eprintln!("Er is geen gegevensmap.");
        return ExitCode::FAILURE;
    };
    // Zoals de proef in de app: eerst naar 16 kHz mono in het geheugen.
    let geluid = match audio::laad(&PathBuf::from(audio_pad)) {
        Ok(geluid) => geluid,
        Err(fout) => {
            eprintln!("{fout}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "audio_s={:.1}",
        audio::duur_van_samples(geluid.samples.len()).as_secs_f64()
    );

    let mut engine = LokaleEngine::nieuw(
        Instellingen::standaard(PathBuf::from(engine_pad)),
        &model,
        map,
        Apparaat::Cpu,
        Some("nl".to_string()),
    );

    let start = Instant::now();
    if let Err(fout) = engine.laad() {
        eprintln!("{fout}");
        return ExitCode::FAILURE;
    }
    println!("laden_ms={}", start.elapsed().as_millis());

    for keer in 1..=herhalingen {
        let start = Instant::now();
        let uitkomst = engine.transcribeer_gefilterd(
            Invoer::Geluid(&geluid.samples),
            &FilterInstellingen::standaard(),
        );
        if let Err(fout) = uitkomst {
            eprintln!("{fout}");
            return ExitCode::FAILURE;
        }
        println!("uitschrijven_{keer}_ms={}", start.elapsed().as_millis());
    }

    // Het script leest deze regel en bemonstert dan het geheugen van het werkproces.
    println!("rust_s={rust_s}");
    std::thread::sleep(Duration::from_secs(rust_s));
    engine.stop();
    println!("gestopt");
    ExitCode::SUCCESS
}
