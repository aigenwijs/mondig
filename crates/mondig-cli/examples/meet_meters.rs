//! Handmatige proef van de niveaumeters zonder opname (fase UX-C).
//!
//! Opent de standaardmicrofoon en het geluid van het standaardafspeelapparaat (loopback),
//! toont een paar seconden twintig keer per seconde het niveau van beide als tien
//! streepjes, en speelt halverwege een zacht testgeluid af op het standaardafspeelapparaat.
//! Dan hoort de meter van het systeemgeluid te bewegen. Er wordt niets bewaard.
//!
//! ```text
//! cargo run -p mondig-cli --example meet_meters -- [seconden]
//! ```

#![expect(clippy::print_stdout, reason = "meetuitvoer")]

use std::f32::consts::PI;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use mondig_core::opname::handy::HandyApparatuur;
use mondig_core::opname::niveau::Niveaumeting;
use mondig_core::opname::systeem::SysteemApparatuur;

fn streepjes(niveau: f32) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "0 tot 10"
    )]
    let aantal = (niveau.clamp(0.0, 1.0) * 10.0).round() as usize;
    format!("{}{}", "#".repeat(aantal), ".".repeat(10 - aantal))
}

/// Een toon van 0,9 s op ongeveer -21 dBFS, zoals het testgeluid van de app.
#[expect(clippy::cast_precision_loss, reason = "kleine aantallen samples")]
fn toon(samplerate: u32) -> Vec<f32> {
    let aantal = samplerate as usize * 9 / 10;
    (0..aantal)
        .map(|i| 0.0875 * (2.0 * PI * 660.0 * i as f32 / samplerate as f32).sin())
        .collect()
}

fn main() -> ExitCode {
    let seconden: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let begin = Instant::now();
    let microfoon = match Niveaumeting::microfoon(&HandyApparatuur, None, |f| {
        println!("microfoon viel uit: {}", f.melding);
    }) {
        Ok(m) => m,
        Err(f) => {
            println!("microfoon opent niet: {}", f.melding);
            return ExitCode::FAILURE;
        }
    };
    println!("microfoon open na {} ms", begin.elapsed().as_millis());
    let begin = Instant::now();
    let systeem = match Niveaumeting::systeem(&SysteemApparatuur, None, |f| {
        println!("systeemgeluid viel uit: {}", f.melding);
    }) {
        Ok(m) => m,
        Err(f) => {
            println!("systeemgeluid opent niet: {}", f.melding);
            return ExitCode::FAILURE;
        }
    };
    println!("systeemgeluid open na {} ms", begin.elapsed().as_millis());

    let speler = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1_500));
        let begin = Instant::now();
        let uitslag = SysteemApparatuur.speel_af(None, toon);
        println!(
            "testgeluid: {:?} in {} ms",
            uitslag.map_err(|f| f.soort),
            begin.elapsed().as_millis()
        );
    });

    let einde = Instant::now() + Duration::from_secs(seconden);
    let start = Instant::now();
    while Instant::now() < einde {
        std::thread::sleep(Duration::from_millis(50));
        let m = microfoon.neem_niveau();
        let s = systeem.neem_niveau();
        println!(
            "{:>5} ms  jij {} {m:.2}  computer {} {s:.2}",
            start.elapsed().as_millis(),
            streepjes(m),
            streepjes(s)
        );
    }
    let _ = speler.join();
    drop(microfoon);
    drop(systeem);
    println!("meters dicht");
    ExitCode::SUCCESS
}
