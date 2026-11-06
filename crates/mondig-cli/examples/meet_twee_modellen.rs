//! Meet wat twee modellen tegelijk kosten (sprint 5-A, ADR 0012): het dicteermodel (Parakeet
//! Ultra) staat warm in het dicteerwerkproces, terwijl het achtergrondwerkproces een
//! bestand uitschrijft met het bestandsmodel (Canary). Zo gaat het in de app als iemand
//! dicteert terwijl er een bestand loopt. `scripts/meet-twee-modellen.ps1` bemonstert het
//! geheugen van beide werkprocessen.
//!
//! ```text
//! cargo run --release -p mondig-cli --example meet_twee_modellen -- <audio> <mondig-engine.exe> [herhalingen] [rust_s]
//! ```
//!
//! Uitvoer op stdout, één meting per regel (`sleutel=waarde`), nooit transcripttekst.

#![expect(clippy::print_stdout, reason = "meetuitvoer voor het script")]

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use mondig_core::aanbieder::{Invoer, LokaleEngine};
use mondig_core::audio;
use mondig_core::engine::protocol::Apparaat;
use mondig_core::engine::supervisor::Instellingen;
use mondig_core::filter::FilterInstellingen;
use mondig_core::model;
use mondig_core::modelkeuze::{STANDAARD_BESTANDEN, STANDAARD_DICTEREN};

fn maak_engine(id: &str, engine_pad: &str, achtergrond: bool) -> Result<LokaleEngine, String> {
    let model = model::zoek(id).map_err(|f| f.to_string())?;
    let map = model::modelmap(&model.id).map_err(|f| f.to_string())?;
    let pad = PathBuf::from(engine_pad);
    let instellingen = if achtergrond {
        Instellingen::achtergrond(pad)
    } else {
        Instellingen::standaard(pad)
    };
    Ok(LokaleEngine::nieuw(
        instellingen,
        &model,
        map,
        Apparaat::Cpu,
        Some("nl".to_string()),
    ))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(audio_pad), Some(engine_pad)) = (args.first(), args.get(1)) else {
        eprintln!("Gebruik: meet_twee_modellen <audio> <mondig-engine.exe> [herhalingen] [rust_s]");
        return ExitCode::FAILURE;
    };
    let herhalingen: u32 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(12);
    let rust_s: u64 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(10);

    let geluid = match audio::laad(&PathBuf::from(audio_pad)) {
        Ok(geluid) => Arc::new(geluid),
        Err(fout) => {
            eprintln!("{fout}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "audio_s={:.1}",
        audio::duur_van_samples(geluid.samples.len()).as_secs_f64()
    );

    // Eerst het dicteermodel, warm, zoals na een eerste dictaat. Het script ziet dit werkproces
    // als eerste kind van dit proces.
    let mut dicteren = match maak_engine(STANDAARD_DICTEREN, engine_pad, false) {
        Ok(engine) => engine,
        Err(fout) => {
            eprintln!("{fout}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(fout) = dicteren.laad() {
        eprintln!("{fout}");
        return ExitCode::FAILURE;
    }
    let filter = FilterInstellingen::standaard();
    if let Err(fout) = dicteren.transcribeer_gefilterd(Invoer::Geluid(&geluid.samples), &filter) {
        eprintln!("{fout}");
        return ExitCode::FAILURE;
    }
    println!("dicteren_warm=1");
    // Het script bemonstert nu het werkproces van alleen het dicteermodel.
    std::thread::sleep(Duration::from_secs(4));

    // Dan het bestandsmodel in het achtergrondwerkproces, met werk voor meerdere uitschrijvingen.
    let mut bestanden = match maak_engine(STANDAARD_BESTANDEN, engine_pad, true) {
        Ok(engine) => engine,
        Err(fout) => {
            eprintln!("{fout}");
            return ExitCode::FAILURE;
        }
    };
    let klaar = Arc::new(AtomicBool::new(false));
    let klaar_voor_thread = Arc::clone(&klaar);
    let geluid_voor_thread = Arc::clone(&geluid);
    let begin = Instant::now();
    let werker = std::thread::spawn(move || {
        let filter = FilterInstellingen::bestand();
        if let Err(fout) = bestanden.laad() {
            eprintln!("{fout}");
            klaar_voor_thread.store(true, Ordering::SeqCst);
            return;
        }
        println!("bestanden_geladen_ms={}", begin.elapsed().as_millis());
        for keer in 1..=herhalingen {
            let start = Instant::now();
            if let Err(fout) = bestanden
                .transcribeer_gefilterd(Invoer::Geluid(&geluid_voor_thread.samples), &filter)
            {
                eprintln!("{fout}");
                break;
            }
            println!("bestand_{keer}_ms={}", start.elapsed().as_millis());
        }
        klaar_voor_thread.store(true, Ordering::SeqCst);
        // Het script bemonstert nu beide werkprocessen met beide modellen geladen.
        println!("rust_s={rust_s}");
        std::thread::sleep(Duration::from_secs(rust_s));
        bestanden.stop();
    });

    // Intussen dicteren: elke paar seconden een dictaat op het dicteermodel.
    let mut keer = 0;
    while !klaar.load(Ordering::SeqCst) {
        keer += 1;
        let start = Instant::now();
        match dicteren.transcribeer_gefilterd(Invoer::Geluid(&geluid.samples), &filter) {
            Ok(_) => println!(
                "dictaat_tijdens_bestand_{keer}_ms={}",
                start.elapsed().as_millis()
            ),
            Err(fout) => {
                eprintln!("{fout}");
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    let _ = werker.join();
    dicteren.stop();
    println!("gestopt");
    ExitCode::SUCCESS
}
