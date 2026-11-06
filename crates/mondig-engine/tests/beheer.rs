//! Tests van de motorbeheerder met de echte `LokaleEngine` tegen het nep-werkproces.
//!
//! De logica van het laadbeleid zelf (rusttijd, annuleren, `nooit`) staat in de unittests
//! van `mondig-core` met een nepmotor. Hier gaat het erom dat het echte werkproces ook
//! werkelijk stopt en weer start.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use mondig_core::aanbieder::LokaleEngine;
use mondig_core::engine::beheer::{Motorbeheerder, Rusttijd, Uitslag};
use mondig_core::engine::protocol::Apparaat;
use mondig_core::engine::supervisor::Instellingen;
use mondig_core::filter::FilterInstellingen;
use mondig_core::model;
use mondig_core::taak::Toestand;

const NEP: &str = env!("CARGO_BIN_EXE_nep-engine");

#[allow(
    clippy::unwrap_used,
    reason = "testhulp: falen mag hier meteen paniek geven"
)]
fn engine(modus: &str, stopbestand: &std::path::Path) -> LokaleEngine {
    let mut i = Instellingen::standaard(PathBuf::from(NEP));
    i.hallo_timeout = Duration::from_secs(5);
    i.laad_timeout = Duration::from_secs(5);
    i.transcribeer_basis = Duration::from_secs(5);
    i.transcribeer_factor = 0;
    i.omgeving = vec![
        ("MONDIG_NEP".into(), modus.into()),
        (
            "MONDIG_NEP_STOPBESTAND".into(),
            stopbestand.to_string_lossy().into_owned(),
        ),
    ];
    let model = model::zoek("whisper-large-v3-turbo-q8").unwrap();
    LokaleEngine::nieuw(i, &model, PathBuf::from("nep"), Apparaat::Cpu, None)
}

fn wacht_tot(voorwaarde: impl Fn() -> bool) -> bool {
    let tot = Instant::now() + Duration::from_secs(5);
    while Instant::now() < tot {
        if voorwaarde() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    voorwaarde()
}

#[test]
#[allow(clippy::unwrap_used, reason = "test")]
fn echt_werkproces_stopt_na_de_rusttijd_en_start_weer_bij_het_volgende_dictaat() {
    let map = tempfile::tempdir().unwrap();
    let stopbestand = map.path().join("gestopt.txt");
    let (zender, gebeurtenissen) = mpsc::channel();
    let beheerder = Motorbeheerder::start(
        engine("ok", &stopbestand),
        Rusttijd::Duur(Duration::from_millis(300)),
        zender,
    );

    let schrijf = |beheerder: &Motorbeheerder| {
        let id = beheerder.nieuw_dictaat().unwrap();
        let uitslag = beheerder
            .uitschrijven(id, vec![0.1; 16_000], FilterInstellingen::standaard())
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        let Uitslag::Klaar(gefilterd) = uitslag else {
            unreachable!("verwachtte Klaar, kreeg {uitslag:?}");
        };
        assert_eq!(gefilterd.transcript.tekst(), "nep op Cpu");
    };

    schrijf(&beheerder);
    assert!(
        wacht_tot(|| stopbestand.exists()),
        "het werkproces stopte niet"
    );
    // Na het stoppen laadt het volgende dictaat het model weer.
    std::fs::remove_file(&stopbestand).unwrap();
    schrijf(&beheerder);
    assert!(wacht_tot(|| stopbestand.exists()));

    let alle: Vec<_> = gebeurtenissen.try_iter().map(|g| g.toestand).collect();
    assert_eq!(alle.iter().filter(|t| **t == Toestand::Klaar).count(), 2);
}

#[test]
#[allow(clippy::unwrap_used, reason = "test")]
fn echt_werkproces_blijft_staan_tijdens_een_traag_uitschrijven() {
    let map = tempfile::tempdir().unwrap();
    let stopbestand = map.path().join("gestopt.txt");
    let (zender, _gebeurtenissen) = mpsc::channel();
    // Het uitschrijven duurt 400 ms, de rusttijd 100 ms.
    let beheerder = Motorbeheerder::start(
        engine("traag_transcribeer", &stopbestand),
        Rusttijd::Duur(Duration::from_millis(100)),
        zender,
    );
    let id = beheerder.nieuw_dictaat().unwrap();
    let uitslag = beheerder
        .uitschrijven(id, vec![0.1; 16_000], FilterInstellingen::standaard())
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap();
    assert!(matches!(uitslag, Uitslag::Klaar(_)), "{uitslag:?}");
    assert!(
        !stopbestand.exists(),
        "het werkproces stopte tijdens het uitschrijven"
    );
    assert!(wacht_tot(|| stopbestand.exists()));
}

#[test]
#[allow(clippy::unwrap_used, reason = "test")]
fn drop_van_de_beheerder_stopt_het_echte_werkproces() {
    let map = tempfile::tempdir().unwrap();
    let stopbestand = map.path().join("gestopt.txt");
    let (zender, _gebeurtenissen) = mpsc::channel();
    let beheerder = Motorbeheerder::start(engine("ok", &stopbestand), Rusttijd::Nooit, zender);
    // "Altijd": het model laadt zonder dictaat; wacht tot het werkproces er is door
    // een dictaat uit te schrijven.
    let id = beheerder.nieuw_dictaat().unwrap();
    beheerder
        .uitschrijven(id, vec![0.1; 16_000], FilterInstellingen::standaard())
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap();
    assert!(!stopbestand.exists());
    drop(beheerder);
    assert!(stopbestand.exists(), "het werkproces kreeg Stop");
}
