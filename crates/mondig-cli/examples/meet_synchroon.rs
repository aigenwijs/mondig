//! Meet of de twee sporen van een vergaderopname synchroon lopen (sprint 4b-A).
//!
//! Speel tijdens `mondig-cli vergader` korte piepjes af via de luidspreker (zonder
//! koptelefoon, zodat de microfoon ze ook hoort). Dit voorbeeld zoekt in beide sporen het
//! begin van elke piep, zet dat met de ankers uit `stukken.jsonl` op de gemeenschappelijke
//! tijdlijn en toont het verschil microfoon min systeem. Dat verschil is de afwijking van de
//! tijdbasis plus de weg door de lucht (ongeveer 3 ms per meter) en de vertraging van de
//! microfoon.
//!
//! ```text
//! cargo run --release -p mondig-cli --example meet_synchroon -- <map van de opname>
//! ```
//!
//! Uitvoer op stdout, alleen tijden; nooit audio.

#![expect(clippy::print_stdout, reason = "meetuitvoer")]

use std::path::Path;
use std::process::ExitCode;

use mondig_core::audio;
use mondig_core::opname::stukken::lees_logboek;
use mondig_core::opname::vergadering::{MICROFOON, SYSTEEM};

/// Tussen twee piepjes zit minstens zoveel tijd (ms).
const MIN_AFSTAND_MS: f64 = 2_000.0;

/// Het begin van elke piep op de tijdlijn (ms): het eerste sample boven de drempel, na een
/// stille periode. De drempel is een derde van de piek van het spoor.
fn piepjes(spoormap: &Path) -> Result<Vec<f64>, String> {
    let stukken = lees_logboek(spoormap).map_err(|f| f.to_string())?;
    let mut geluid = Vec::new();
    for stuk in &stukken {
        let samples = audio::laad(&spoormap.join(&stuk.bestand))
            .map_err(|f| f.to_string())?
            .samples;
        geluid.push((stuk.begin_ms, samples));
    }
    let piek = geluid
        .iter()
        .flat_map(|(_, s)| s.iter())
        .fold(0.0_f32, |m, s| m.max(s.abs()));
    let drempel = piek / 3.0;
    let mut gevonden: Vec<f64> = Vec::new();
    for (begin_ms, samples) in &geluid {
        for (i, s) in samples.iter().enumerate() {
            if s.abs() < drempel {
                continue;
            }
            #[allow(clippy::cast_precision_loss, reason = "meting")]
            let t = *begin_ms as f64 + i as f64 / 16.0;
            if gevonden.last().is_none_or(|&l| t - l > MIN_AFSTAND_MS) {
                gevonden.push(t);
            }
        }
    }
    Ok(gevonden)
}

fn main() -> ExitCode {
    let Some(map) = std::env::args().nth(1) else {
        eprintln!("Gebruik: meet_synchroon <map van de opname>");
        return ExitCode::FAILURE;
    };
    let map = Path::new(&map);
    let (mic, sys) = match (piepjes(&map.join(MICROFOON)), piepjes(&map.join(SYSTEEM))) {
        (Ok(m), Ok(s)) => (m, s),
        (Err(f), _) | (_, Err(f)) => {
            eprintln!("Fout: {f}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "piepjes_systeem={} piepjes_microfoon={}",
        sys.len(),
        mic.len()
    );
    let mut verschillen = Vec::new();
    for s in &sys {
        if let Some(m) = mic.iter().find(|&&m| (m - s).abs() < 500.0) {
            println!(
                "systeem_ms={s:.1} microfoon_ms={m:.1} verschil_ms={:.1}",
                m - s
            );
            verschillen.push(m - s);
        }
    }
    if verschillen.is_empty() {
        println!("geen gepaarde piepjes (hoort de microfoon de luidspreker?)");
        return ExitCode::FAILURE;
    }
    #[allow(clippy::cast_precision_loss, reason = "meting")]
    let gemiddeld = verschillen.iter().sum::<f64>() / verschillen.len() as f64;
    let min = verschillen.iter().copied().fold(f64::INFINITY, f64::min);
    let max = verschillen
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    println!(
        "gepaard={} gemiddeld_ms={gemiddeld:.1} min_ms={min:.1} max_ms={max:.1} spreiding_ms={:.1}",
        verschillen.len(),
        max - min
    );
    ExitCode::SUCCESS
}
