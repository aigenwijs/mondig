//! Tests van de opnamelaag met een nepbron: geen echte microfoon nodig.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::*;

/// Wat de nepbron doet bij `start`.
#[derive(Clone, Default)]
struct Script {
    /// De blokken die hij levert, op een eigen thread.
    blokken: Vec<Vec<f32>>,
    /// Pauze voor het eerste blok, om een trage microfoon na te bootsen.
    vertraging: Duration,
    /// Als gezet: hierna valt de bron uit met deze fout.
    uitval: Option<OpnameFout>,
}

#[derive(Default)]
struct NepStand {
    gestart: AtomicUsize,
    gestopt: AtomicUsize,
}

struct NepApparatuur {
    lijst: Vec<Microfoon>,
    open_fout: Option<OpnameFout>,
    script: Script,
    stand: Arc<NepStand>,
    geopend: Mutex<Vec<Option<String>>>,
}

impl NepApparatuur {
    fn nieuw(script: Script) -> Self {
        Self {
            lijst: vec![],
            open_fout: None,
            script,
            stand: Arc::new(NepStand::default()),
            geopend: Mutex::new(Vec::new()),
        }
    }
}

impl Apparatuur for NepApparatuur {
    fn microfoons(&self) -> Result<Vec<Microfoon>, OpnameFout> {
        Ok(self.lijst.clone())
    }

    fn open(
        &self,
        microfoon: Option<&str>,
        bij_uitval: UitvalMelder,
    ) -> Result<Box<dyn Bron>, OpnameFout> {
        self.geopend
            .lock()
            .unwrap()
            .push(microfoon.map(str::to_string));
        if let Some(fout) = &self.open_fout {
            return Err(fout.clone());
        }
        Ok(Box::new(NepBron {
            script: self.script.clone(),
            stand: Arc::clone(&self.stand),
            bij_uitval: Arc::new(Mutex::new(Some(bij_uitval))),
            draad: None,
        }))
    }
}

struct NepBron {
    script: Script,
    stand: Arc<NepStand>,
    bij_uitval: Arc<Mutex<Option<UitvalMelder>>>,
    draad: Option<JoinHandle<()>>,
}

impl Bron for NepBron {
    fn start(&mut self, mut uitgang: Box<dyn Uitgang>) -> Result<(), OpnameFout> {
        if self.draad.is_some() {
            return Err(OpnameFout::nieuw(FoutSoort::Programma));
        }
        self.stand.gestart.fetch_add(1, Ordering::SeqCst);
        let script = self.script.clone();
        let bij_uitval = Arc::clone(&self.bij_uitval);
        self.draad = Some(std::thread::spawn(move || {
            std::thread::sleep(script.vertraging);
            for blok in &script.blokken {
                uitgang.ontvang(blok);
            }
            if let Some(fout) = script.uitval
                && let Some(melder) = bij_uitval.lock().unwrap().take()
            {
                melder(fout);
            }
        }));
        Ok(())
    }

    fn stop(&mut self) -> Result<Einde, OpnameFout> {
        let Some(draad) = self.draad.take() else {
            return Err(OpnameFout::nieuw(FoutSoort::Programma));
        };
        draad.join().unwrap();
        self.stand.gestopt.fetch_add(1, Ordering::SeqCst);
        Ok(Einde {
            uitgevallen: self.script.uitval.clone(),
            verloren: 0,
        })
    }
}

/// Een melder die alle gebeurtenissen bewaart.
fn opvangen() -> (Melder, Arc<Mutex<Vec<OpnameGebeurtenis>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let voor_melder = Arc::clone(&log);
    let melder: Melder = Arc::new(move |g| voor_melder.lock().unwrap().push(g));
    (melder, log)
}

fn blok(aantal: usize, waarde: f32) -> Vec<f32> {
    vec![waarde; aantal]
}

#[test]
fn opname_verzamelt_de_samples_in_het_geheugen() {
    let script = Script {
        blokken: vec![blok(480, 0.1), blok(480, -0.25), blok(160, 0.05)],
        ..Script::default()
    };
    let apparatuur = NepApparatuur::nieuw(script);
    let mut opname = Opname::open(&apparatuur, None, geen_melder()).unwrap();
    opname.start().unwrap();
    let opgenomen = opname.stop().unwrap();
    assert_eq!(opgenomen.samples().len(), 1120);
    assert!((opgenomen.piek() - 0.25).abs() < f32::EPSILON);
    assert_eq!(opgenomen.duur(), Duration::from_millis(70));
    assert!(opgenomen.onderbroken.is_none());
    assert!(!opgenomen.vol);
    assert_eq!(apparatuur.stand.gestopt.load(Ordering::SeqCst), 1);
}

#[test]
fn gekozen_microfoon_gaat_door_naar_de_bron() {
    let apparatuur = NepApparatuur::nieuw(Script::default());
    Opname::open(&apparatuur, Some("usb-headset"), geen_melder()).unwrap();
    Opname::open(&apparatuur, None, geen_melder()).unwrap();
    assert_eq!(
        *apparatuur.geopend.lock().unwrap(),
        vec![Some("usb-headset".to_string()), None]
    );
}

#[test]
fn tijd_tot_het_eerste_blok_wordt_gemeten_vanaf_start() {
    let script = Script {
        blokken: vec![blok(480, 0.1)],
        vertraging: Duration::from_millis(80),
        ..Script::default()
    };
    let apparatuur = NepApparatuur::nieuw(script);
    let (melder, log) = opvangen();
    let mut opname = Opname::open(&apparatuur, None, melder).unwrap();
    assert!(opname.tijd_tot_eerste_blok().is_none());
    opname.start().unwrap();
    let opgenomen = opname.stop().unwrap();
    let tijd = opgenomen.tijd_tot_eerste_blok.unwrap();
    assert!(tijd >= Duration::from_millis(80), "{tijd:?}");
    assert!(tijd < Duration::from_secs(5), "{tijd:?}");
    // Het eerste blok wordt precies één keer gemeld, met dezelfde tijd.
    let log = log.lock().unwrap();
    assert_eq!(*log, vec![OpnameGebeurtenis::EersteBlok(tijd)]);
}

#[test]
fn grens_geeft_eerst_bijna_vol_en_dan_vol_en_houdt_het_geheugen_begrensd() {
    // Kleine grenzen: waarschuwing na 1000 samples, einde na 1600.
    let grenzen = Grenzen {
        bijna_vol: 1000,
        max: 1600,
    };
    let script = Script {
        blokken: vec![
            blok(900, 0.1),
            blok(300, 0.1),
            blok(300, 0.1),
            blok(300, 0.1),
        ],
        ..Script::default()
    };
    let apparatuur = NepApparatuur::nieuw(script);
    let (melder, log) = opvangen();
    let mut opname = Opname::open_met_grenzen(&apparatuur, None, melder, grenzen).unwrap();
    opname.start().unwrap();
    let opgenomen = opname.stop().unwrap();
    assert_eq!(
        opgenomen.samples().len(),
        1600,
        "meer dan de grens wordt niet bewaard"
    );
    assert!(opgenomen.vol);
    let gebeurtenissen: Vec<_> = log
        .lock()
        .unwrap()
        .iter()
        .filter(|g| !matches!(g, OpnameGebeurtenis::EersteBlok(_)))
        .cloned()
        .collect();
    assert_eq!(
        gebeurtenissen,
        vec![OpnameGebeurtenis::BijnaVol, OpnameGebeurtenis::Vol]
    );
}

#[test]
fn standaardgrenzen_zijn_negen_en_tien_minuten_op_zestien_kilohertz() {
    let grenzen = Grenzen::standaard();
    assert_eq!(grenzen.bijna_vol, 9 * 60 * 16_000);
    assert_eq!(grenzen.max, 10 * 60 * 16_000);
    assert_eq!(MAX_OPNAME, crate::engine::supervisor::MAX_DICTAAT);
}

#[test]
fn bijna_vol_komt_ook_als_de_grens_in_een_keer_voorbij_gaat() {
    let grenzen = Grenzen {
        bijna_vol: 500,
        max: 1000,
    };
    let script = Script {
        blokken: vec![blok(1200, 0.1)],
        ..Script::default()
    };
    let apparatuur = NepApparatuur::nieuw(script);
    let (melder, log) = opvangen();
    let mut opname = Opname::open_met_grenzen(&apparatuur, None, melder, grenzen).unwrap();
    opname.start().unwrap();
    let opgenomen = opname.stop().unwrap();
    assert_eq!(opgenomen.samples().len(), 1000);
    let log = log.lock().unwrap();
    assert!(log.contains(&OpnameGebeurtenis::BijnaVol));
    assert!(log.contains(&OpnameGebeurtenis::Vol));
}

#[test]
fn fout_bij_openen_wordt_een_melding_in_gewone_taal() {
    let mut apparatuur = NepApparatuur::nieuw(Script::default());
    apparatuur.open_fout = Some(OpnameFout::nieuw(FoutSoort::GeenMicrofoon));
    let Err(melding) = Opname::open(&apparatuur, None, geen_melder()) else {
        unreachable!("openen hoort te mislukken");
    };
    assert_eq!(
        melding.melding,
        "Mondig vindt geen microfoon. Sluit een microfoon of headset aan."
    );
    assert_eq!(melding.vervolg, Vervolg::MicrofooninstellingOpenen);
}

#[test]
fn uitvallende_headset_meldt_en_bewaart_wat_er_was() {
    let script = Script {
        blokken: vec![blok(480, 0.1), blok(480, 0.1)],
        uitval: Some(OpnameFout {
            soort: FoutSoort::Losgekoppeld,
            detail: Some("WASAPI HRESULT 0x88890004".into()),
        }),
        ..Script::default()
    };
    let apparatuur = NepApparatuur::nieuw(script);
    let (melder, log) = opvangen();
    let mut opname = Opname::open(&apparatuur, None, melder).unwrap();
    opname.start().unwrap();
    let opgenomen = opname.stop().unwrap();
    assert_eq!(opgenomen.samples().len(), 960);
    let onderbroken = opgenomen.onderbroken.clone().unwrap();
    assert_eq!(onderbroken.vervolg, Vervolg::MicrofooninstellingOpenen);
    assert!(onderbroken.melding.contains("losgekoppeld"));
    // De tekst van het systeem komt niet in de melding.
    assert!(!onderbroken.melding.contains("HRESULT"));
    assert!(
        log.lock()
            .unwrap()
            .contains(&OpnameGebeurtenis::Uitgevallen(onderbroken))
    );
}

#[test]
fn uitval_zonder_geluid_komt_toch_terug_bij_stop() {
    let script = Script {
        uitval: Some(OpnameFout::nieuw(FoutSoort::Losgekoppeld)),
        ..Script::default()
    };
    let apparatuur = NepApparatuur::nieuw(script);
    let mut opname = Opname::open(&apparatuur, None, geen_melder()).unwrap();
    opname.start().unwrap();
    // Wacht tot de uitval gemeld is: stop levert dan de fout.
    std::thread::sleep(Duration::from_millis(50));
    let opgenomen = opname.stop().unwrap();
    assert!(opgenomen.onderbroken.is_some());
}

#[test]
fn stoppen_zonder_starten_en_dubbel_starten_zijn_fouten() {
    let apparatuur = NepApparatuur::nieuw(Script::default());
    let opname = Opname::open(&apparatuur, None, geen_melder()).unwrap();
    assert!(opname.stop().is_err());

    let mut opname = Opname::open(&apparatuur, None, geen_melder()).unwrap();
    opname.start().unwrap();
    assert!(opname.start().is_err());
}

#[test]
fn weggooien_tijdens_het_opnemen_stopt_de_bron() {
    let script = Script {
        blokken: vec![blok(480, 0.1)],
        ..Script::default()
    };
    let apparatuur = NepApparatuur::nieuw(script);
    let mut opname = Opname::open(&apparatuur, None, geen_melder()).unwrap();
    opname.start().unwrap();
    drop(opname);
    assert_eq!(apparatuur.stand.gestart.load(Ordering::SeqCst), 1);
    assert_eq!(apparatuur.stand.gestopt.load(Ordering::SeqCst), 1);
}

#[test]
fn elke_fout_geeft_een_melding_zonder_jargon_en_met_een_vervolg() {
    let soorten = [
        (FoutSoort::GeenMicrofoon, Vervolg::MicrofooninstellingOpenen),
        (FoutSoort::Bezet, Vervolg::OpnieuwProberen),
        (
            FoutSoort::ToegangGeweigerd,
            Vervolg::MicrofooninstellingOpenen,
        ),
        (FoutSoort::Reageert, Vervolg::OpnieuwProberen),
        (FoutSoort::GeenGeluid, Vervolg::MicrofooninstellingOpenen),
        (FoutSoort::Losgekoppeld, Vervolg::MicrofooninstellingOpenen),
        (FoutSoort::Veranderd, Vervolg::OpnieuwProberen),
        (FoutSoort::Gestopt, Vervolg::OpnieuwProberen),
        (FoutSoort::Anders, Vervolg::OpnieuwProberen),
        (FoutSoort::Programma, Vervolg::Geen),
        (FoutSoort::GeenAfspeelapparaat, Vervolg::OpnieuwProberen),
        (FoutSoort::AfspeelapparaatWeg, Vervolg::OpnieuwProberen),
        (FoutSoort::SysteemgeluidNietMogelijk, Vervolg::Geen),
        (FoutSoort::SysteemgeluidAnders, Vervolg::OpnieuwProberen),
        (FoutSoort::SysteemgeluidOmrekenen, Vervolg::OpnieuwProberen),
        (FoutSoort::SysteemgeluidOmrekenenStart, Vervolg::Geen),
    ];
    for (soort, vervolg) in soorten {
        let melding = Foutmelding::from(OpnameFout {
            soort,
            detail: Some("technische details".into()),
        });
        assert_eq!(melding.vervolg, vervolg, "{soort:?}");
        assert!(melding.melding.ends_with('.'), "{soort:?}");
        assert!(!melding.melding.contains("technische details"), "{soort:?}");
        assert!(
            !melding.melding.contains(['\u{2013}', '\u{2014}']),
            "{soort:?}"
        );
    }
}

fn lijst() -> Vec<Microfoon> {
    vec![
        Microfoon {
            id: "a1".into(),
            naam: "Microfoon (Realtek Audio)".into(),
            standaard: true,
        },
        Microfoon {
            id: "b2".into(),
            naam: "Headset (Jabra Evolve2 65)".into(),
            standaard: false,
        },
        Microfoon {
            id: "c3".into(),
            naam: "Headset (Jabra Link 380)".into(),
            standaard: false,
        },
    ]
}

#[test]
fn microfoon_kiezen_op_id_naam_of_deel_van_de_naam() {
    let lijst = lijst();
    assert_eq!(kies_microfoon(&lijst, "b2").unwrap().id, "b2");
    assert_eq!(
        kies_microfoon(&lijst, "microfoon (realtek audio)")
            .unwrap()
            .id,
        "a1"
    );
    assert_eq!(kies_microfoon(&lijst, "evolve2").unwrap().id, "b2");
}

#[test]
fn onduidelijke_of_onbekende_microfoon_geeft_een_melding() {
    let lijst = lijst();
    let dubbel = kies_microfoon(&lijst, "jabra").unwrap_err();
    assert!(dubbel.melding.contains("Meer dan één"));
    let onbekend = kies_microfoon(&lijst, "bestaat niet").unwrap_err();
    assert!(onbekend.melding.contains("Er is geen microfoon"));
    assert_eq!(onbekend.vervolg, Vervolg::MicrofooninstellingOpenen);
}

#[test]
fn nepapparatuur_somt_microfoons_op() {
    let mut apparatuur = NepApparatuur::nieuw(Script::default());
    apparatuur.lijst = lijst();
    assert_eq!(apparatuur.microfoons().unwrap().len(), 3);
}

#[test]
fn opgeslagen_microfoon_geldt_alleen_als_hij_er_nog_is() {
    let lijst = vec![
        Microfoon {
            id: "a1".into(),
            naam: "Microfoon (Realtek Audio)".into(),
            standaard: true,
        },
        Microfoon {
            id: "b2".into(),
            naam: "Headset (EVOLVE2 65)".into(),
            standaard: false,
        },
    ];
    assert_eq!(opgeslagen_microfoon(Some("b2"), &lijst), Some("b2".into()));
    // Losgekoppeld: de standaard van het systeem.
    assert_eq!(opgeslagen_microfoon(Some("weg"), &lijst), None);
    assert_eq!(opgeslagen_microfoon(None, &lijst), None);
    assert_eq!(opgeslagen_microfoon(Some("b2"), &[]), None);
}

// ---------------------------------------------------------------------------
// Microfoontest: alleen een niveau, geen geluid bewaard
// ---------------------------------------------------------------------------

use niveau::{MEETVENSTER, MicrofoonTest, NiveauUitgang, niveau_van_rms};

#[test]
fn niveau_loopt_van_stilte_naar_vol() {
    assert!(niveau_van_rms(0.0).abs() < f32::EPSILON);
    // -80 dBFS valt onder de stilte van de schaal.
    assert!(niveau_van_rms(0.0001).abs() < f32::EPSILON);
    assert!((niveau_van_rms(1.0) - 1.0).abs() < f32::EPSILON);
    // -30 dBFS (gewone spraak) is de helft van de schaal.
    assert!((niveau_van_rms(0.031_622_78) - 0.5).abs() < 0.001);
    // Boven vol blijft vol.
    assert!((niveau_van_rms(2.0) - 1.0).abs() < f32::EPSILON);
}

#[test]
fn de_uitgang_meet_twintig_keer_per_seconde() {
    let gemeten = Arc::new(Mutex::new(Vec::new()));
    let voor_uitgang = Arc::clone(&gemeten);
    let mut uitgang = NiveauUitgang::nieuw(move |n| voor_uitgang.lock().unwrap().push(n));
    // Een seconde stilte in blokken van een andere grootte dan het venster.
    for _ in 0..100 {
        uitgang.ontvang(&[0.0; 160]);
    }
    assert_eq!(gemeten.lock().unwrap().len(), 20);
    assert!(
        gemeten
            .lock()
            .unwrap()
            .iter()
            .all(|n| n.abs() < f32::EPSILON)
    );
    // Een hard blok geeft een hoog niveau; een halfvol venster levert nog niets.
    gemeten.lock().unwrap().clear();
    uitgang.ontvang(&vec![0.5; MEETVENSTER + 10]);
    let nu = gemeten.lock().unwrap().clone();
    assert_eq!(nu.len(), 1);
    assert!(nu[0] > 0.8, "niveau {}", nu[0]);
}

#[test]
fn de_test_geeft_alleen_niveaus_door_en_stopt_bij_weggooien() {
    let niveaus = Arc::new(Mutex::new(Vec::new()));
    let voor_test = Arc::clone(&niveaus);
    let script = Script {
        blokken: vec![blok(MEETVENSTER * 3, 0.2)],
        ..Script::default()
    };
    let apparatuur = NepApparatuur::nieuw(script);
    let test = MicrofoonTest::start(
        &apparatuur,
        Some("b2"),
        move |n| voor_test.lock().unwrap().push(n),
        |_| {},
    )
    .unwrap();
    assert_eq!(
        *apparatuur.geopend.lock().unwrap(),
        vec![Some("b2".to_string())]
    );
    test.stop();
    // De nepbron levert zijn blokken op een eigen thread en wordt bij stop afgewacht.
    assert_eq!(niveaus.lock().unwrap().len(), 3);
    assert_eq!(apparatuur.stand.gestopt.load(Ordering::SeqCst), 1);
}

#[test]
fn de_test_meldt_een_microfoon_die_uitvalt() {
    let melding = Arc::new(Mutex::new(None));
    let voor_test = Arc::clone(&melding);
    let script = Script {
        uitval: Some(OpnameFout::nieuw(FoutSoort::Losgekoppeld)),
        ..Script::default()
    };
    let apparatuur = NepApparatuur::nieuw(script);
    let test = MicrofoonTest::start(
        &apparatuur,
        None,
        |_| {},
        move |m| *voor_test.lock().unwrap() = Some(m),
    )
    .unwrap();
    test.stop();
    let melding = melding.lock().unwrap().clone().unwrap();
    assert_eq!(melding.vervolg, Vervolg::MicrofooninstellingOpenen);
}

#[test]
fn de_test_geeft_een_melding_als_de_microfoon_niet_opent() {
    let mut apparatuur = NepApparatuur::nieuw(Script::default());
    apparatuur.open_fout = Some(OpnameFout::nieuw(FoutSoort::Bezet));
    let fout = MicrofoonTest::start(&apparatuur, None, |_| {}, |_| {})
        .err()
        .unwrap();
    assert_eq!(fout.vervolg, Vervolg::OpnieuwProberen);
}

#[test]
fn opname_geeft_het_niveau_door_zonder_iets_extra_te_bewaren() {
    // Twee meetvensters van 50 ms: één stil, één op volle uitsturing.
    let venster = niveau::MEETVENSTER;
    let script = Script {
        blokken: vec![blok(venster, 0.0), blok(venster, 1.0)],
        ..Script::default()
    };
    let apparatuur = NepApparatuur::nieuw(script);
    let niveaus = Arc::new(Mutex::new(Vec::new()));
    let voor_meter = Arc::clone(&niveaus);
    let mut opname = Opname::open(&apparatuur, None, geen_melder()).unwrap();
    opname.bij_niveau(move |niveau| voor_meter.lock().unwrap().push(niveau));
    opname.start().unwrap();
    let opgenomen = opname.stop().unwrap();
    assert_eq!(opgenomen.samples().len(), venster * 2);
    let niveaus = niveaus.lock().unwrap();
    assert_eq!(niveaus.len(), 2);
    assert!(niveaus[0].abs() < f32::EPSILON);
    assert!((niveaus[1] - 1.0).abs() < 1e-3);
}

// ---------------------------------------------------------------------------
// Niveaumeting zonder opname (fase UX-C): alleen de piek, op de schaal van de opname
// ---------------------------------------------------------------------------

use niveau::{Niveaumeting, Piekmeter};

#[test]
fn de_piekmeter_houdt_de_hoogste_piek_bij_tot_hij_wordt_uitgelezen() {
    let meter = Piekmeter::default();
    assert!((meter.meet(&[0.1, -0.4, 0.2]) - 0.4).abs() < f32::EPSILON);
    meter.meet(&[0.05]);
    assert!((meter.neem_piek() - 0.4).abs() < f32::EPSILON);
    // Daarna weer stil, tot er iets binnenkomt; boven vol telt als vol.
    assert!(meter.neem_niveau().abs() < f32::EPSILON);
    meter.meet(&[-3.0]);
    assert!((meter.neem_niveau() - 1.0).abs() < f32::EPSILON);
    // -30 dBFS piek is de helft, zoals bij de meters tijdens een vergaderopname.
    meter.meet(&[0.031_622_78]);
    assert!((meter.neem_niveau() - 0.5).abs() < 0.001);
}

#[test]
fn de_meting_geeft_alleen_een_niveau_en_zet_de_bron_uit_bij_weggooien() {
    let script = Script {
        blokken: vec![blok(MEETVENSTER, 0.2), blok(MEETVENSTER, 0.031_622_78)],
        ..Script::default()
    };
    let apparatuur = NepApparatuur::nieuw(script);
    let meting = Niveaumeting::microfoon(&apparatuur, Some("b2"), |_| {}).unwrap();
    assert_eq!(
        *apparatuur.geopend.lock().unwrap(),
        vec![Some("b2".to_string())]
    );
    // De nepbron levert op een eigen thread; wacht tot alles binnen is.
    let tot = std::time::Instant::now() + Duration::from_secs(5);
    let mut hoogste = 0.0f32;
    while std::time::Instant::now() < tot && hoogste < f32::EPSILON {
        hoogste = meting.neem_niveau();
        std::thread::sleep(Duration::from_millis(5));
    }
    // De piek van 0,2 (-14 dBFS) ligt boven de helft van de meter.
    assert!(hoogste > 0.7, "niveau {hoogste}");
    drop(meting);
    assert_eq!(apparatuur.stand.gestopt.load(Ordering::SeqCst), 1);
}

#[test]
fn de_meting_meldt_een_bron_die_uitvalt_en_een_die_niet_opent() {
    let melding = Arc::new(Mutex::new(None));
    let voor_meting = Arc::clone(&melding);
    let script = Script {
        uitval: Some(OpnameFout::nieuw(FoutSoort::Losgekoppeld)),
        ..Script::default()
    };
    let apparatuur = NepApparatuur::nieuw(script);
    let meting = Niveaumeting::microfoon(&apparatuur, None, move |m| {
        *voor_meting.lock().unwrap() = Some(m);
    })
    .unwrap();
    drop(meting);
    let melding = melding.lock().unwrap().clone().unwrap();
    assert_eq!(melding.vervolg, Vervolg::MicrofooninstellingOpenen);

    let mut apparatuur = NepApparatuur::nieuw(Script::default());
    apparatuur.open_fout = Some(OpnameFout::nieuw(FoutSoort::Bezet));
    let fout = Niveaumeting::microfoon(&apparatuur, None, |_| {})
        .err()
        .unwrap();
    assert_eq!(fout.vervolg, Vervolg::OpnieuwProberen);
}

#[cfg(not(windows))]
#[test]
fn systeemgeluid_meten_en_testgeluid_kunnen_buiten_windows_nog_niet() {
    use systeem::SysteemApparatuur;
    let fout = Niveaumeting::systeem(&SysteemApparatuur, None, |_| {})
        .err()
        .unwrap();
    assert_eq!(
        fout,
        OpnameFout::nieuw(FoutSoort::SysteemgeluidNietMogelijk).into()
    );
    let fout = SysteemApparatuur
        .speel_af(None, |_| Vec::new())
        .unwrap_err();
    assert_eq!(fout.soort, FoutSoort::TestgeluidNietMogelijk);
}
