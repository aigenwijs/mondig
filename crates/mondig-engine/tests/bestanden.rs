//! Tests van bestanden in stukken (fase 4) tegen het nep-werkproces: voortgang, de
//! wachttijd per stuk, annuleren, de lagere procesprioriteit en de wachtrij met de echte
//! `LokaleEngine`.

#![allow(clippy::unwrap_used, reason = "tests: falen mag meteen paniek geven")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use mondig_core::aanbieder::{AanbiederFout, LokaleEngine};
use mondig_core::bestanden::{Bestandsmotor, Bestandsuitslag, Wachtrij};
use mondig_core::engine::protocol::{Apparaat, FoutSoort};
use mondig_core::engine::supervisor::{
    EngineFout, Instellingen, LaadOpdracht, Supervisor, Voortgang,
};
use mondig_core::filter::FilterInstellingen;
use mondig_core::model;
use mondig_core::taak::{Soort, Stap, Toestand};

const NEP: &str = env!("CARGO_BIN_EXE_nep-engine");

/// Instellingen voor het nep-werkproces met deze modus en omgeving.
fn instellingen(modus: &str, extra: &[(&str, String)]) -> Instellingen {
    let mut i = Instellingen::achtergrond(PathBuf::from(NEP));
    i.hallo_timeout = Duration::from_secs(5);
    i.laad_timeout = Duration::from_secs(5);
    i.transcribeer_basis = Duration::from_secs(2);
    i.transcribeer_factor = 0;
    i.annuleer_wachttijd = Duration::from_millis(500);
    i.omgeving = vec![("MONDIG_NEP".into(), modus.into())];
    i.omgeving
        .extend(extra.iter().map(|(n, w)| ((*n).to_string(), w.clone())));
    i
}

fn opdracht() -> LaadOpdracht {
    LaadOpdracht {
        model_id: "nep".into(),
        modelmap: PathBuf::from("nep"),
        apparaat: Apparaat::Cpu,
        taal: None,
    }
}

fn geladen(i: Instellingen) -> Supervisor {
    let mut sup = Supervisor::nieuw(i);
    sup.laad(&opdracht()).unwrap();
    sup
}

const UUR: Duration = Duration::from_secs(3600);

/// Zet na `na` de vlag op waar, op een eigen draad.
fn annuleer_na(na: Duration) -> Arc<AtomicBool> {
    let vlag = Arc::new(AtomicBool::new(false));
    let voor_draad = Arc::clone(&vlag);
    std::thread::spawn(move || {
        std::thread::sleep(na);
        voor_draad.store(true, Ordering::SeqCst);
    });
    vlag
}

#[test]
fn na_elk_stuk_een_voortgang_en_dan_het_resultaat() {
    let mut sup = geladen(instellingen("stukken", &[]));
    let mut standen = Vec::new();
    let uitkomst = sup
        .transcribeer_bestand(
            Path::new("x.mp4"),
            Some(UUR),
            &mut |v: Voortgang| standen.push(v.procent()),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(
        standen,
        vec![Some(20), Some(40), Some(60), Some(80), Some(100)]
    );
    assert_eq!(uitkomst.segmenten.len(), 5);
    assert_eq!(uitkomst.duur_ms, 5000);
}

#[test]
fn zonder_totaal_van_het_werkproces_telt_de_duur_uit_de_container() {
    let extra = [("MONDIG_NEP_ZONDER_TOTAAL", "1".to_string())];
    let mut sup = geladen(instellingen("stukken", &extra));
    let mut standen = Vec::new();
    sup.transcribeer_bestand(
        Path::new("x.mp4"),
        Some(std::time::Duration::from_secs(10)),
        &mut |v: Voortgang| standen.push(v.procent()),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        standen,
        vec![Some(10), Some(20), Some(30), Some(40), Some(50)]
    );
}

#[test]
fn de_wachttijd_geldt_per_stuk_en_niet_voor_het_hele_bestand() {
    // Zes stukken van 500 ms zijn samen 3 s, langer dan de wachttijd van 2 s.
    let extra = [
        ("MONDIG_NEP_STUKKEN", "6".to_string()),
        ("MONDIG_NEP_STUKTIJD_MS", "500".to_string()),
    ];
    let mut sup = geladen(instellingen("stukken", &extra));
    let begin = Instant::now();
    let uitkomst = sup.transcribeer(Path::new("x.mp4"), UUR).unwrap();
    assert!(begin.elapsed() > Duration::from_secs(2));
    assert_eq!(uitkomst.segmenten.len(), 6);
}

#[test]
fn annuleren_breekt_het_bestand_af_en_het_werkproces_draait_door() {
    let map = tempfile::tempdir().unwrap();
    let pidbestand = map.path().join("pid.txt");
    let extra = [
        ("MONDIG_NEP_STUKKEN", "4".to_string()),
        ("MONDIG_NEP_STUKTIJD_MS", "150".to_string()),
        (
            "MONDIG_NEP_PIDBESTAND",
            pidbestand.to_string_lossy().into_owned(),
        ),
    ];
    let mut sup = geladen(instellingen("stukken", &extra));
    let pid = std::fs::read_to_string(&pidbestand).unwrap();

    let begin = Instant::now();
    let vlag = annuleer_na(Duration::from_millis(200));
    let fout = sup
        .transcribeer_bestand(Path::new("x.mp4"), Some(UUR), &mut |_| {}, &vlag)
        .unwrap_err();
    assert!(matches!(fout, EngineFout::Geannuleerd), "{fout:?}");
    assert!(!fout.kan_aan_apparaat_liggen());
    // Na hooguit één stuk extra, ruim binnen de afbreektijd.
    assert!(
        begin.elapsed() < Duration::from_millis(450),
        "{:?}",
        begin.elapsed()
    );
    assert!(sup.draait());

    // Het volgende bestand gaat in hetzelfde proces, zonder opnieuw te laden.
    let uitkomst = sup.transcribeer(Path::new("y.mp4"), UUR).unwrap();
    assert_eq!(uitkomst.segmenten.len(), 4);
    assert_eq!(std::fs::read_to_string(&pidbestand).unwrap(), pid);
}

#[test]
fn een_werkproces_dat_niet_luistert_wordt_na_de_afbreektijd_gestopt() {
    let extra = [
        ("MONDIG_NEP_STUKKEN", "50".to_string()),
        ("MONDIG_NEP_STUKTIJD_MS", "100".to_string()),
    ];
    let mut sup = geladen(instellingen("stukken_doof", &extra));
    let begin = Instant::now();
    let vlag = annuleer_na(Duration::from_millis(100));
    let fout = sup
        .transcribeer_bestand(Path::new("x.mp4"), Some(UUR), &mut |_| {}, &vlag)
        .unwrap_err();
    assert!(matches!(fout, EngineFout::Geannuleerd), "{fout:?}");
    // 100 ms tot de annulering plus 500 ms afbreektijd, ruim binnen de 5 s van het bestand.
    assert!(
        begin.elapsed() < Duration::from_secs(2),
        "{:?}",
        begin.elapsed()
    );
    assert!(!sup.draait());
}

#[test]
fn een_fout_halverwege_het_bestand_laat_het_werkproces_draaien() {
    let mut sup = geladen(instellingen("stukken_fout_audio", &[]));
    let mut standen = 0;
    let fout = sup
        .transcribeer_bestand(
            Path::new("x.mp4"),
            Some(UUR),
            &mut |_| standen += 1,
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert!(
        matches!(
            fout,
            EngineFout::Engine {
                soort: FoutSoort::Audio,
                ..
            }
        ),
        "{fout:?}"
    );
    assert_eq!(standen, 2);
    assert!(sup.draait());
}

/// De prioriteit van een proces zoals het besturingssysteem die meldt.
#[cfg(windows)]
fn prioriteit(pid: &str) -> String {
    let uit = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("(Get-Process -Id {pid}).PriorityClass"),
        ])
        .output()
        .unwrap();
    String::from_utf8_lossy(&uit.stdout).trim().to_string()
}

#[cfg(unix)]
fn prioriteit(pid: &str) -> String {
    let uit = std::process::Command::new("ps")
        .args(["-o", "ni=", "-p", pid])
        .output()
        .unwrap();
    String::from_utf8_lossy(&uit.stdout).trim().to_string()
}

#[test]
fn het_achtergrondwerkproces_draait_op_een_lagere_prioriteit() {
    let map = tempfile::tempdir().unwrap();
    let pidbestand = map.path().join("pid.txt");
    let extra = [(
        "MONDIG_NEP_PIDBESTAND",
        pidbestand.to_string_lossy().into_owned(),
    )];
    let sup = geladen(instellingen("ok", &extra));
    let pid = std::fs::read_to_string(&pidbestand).unwrap();
    let verwacht = if cfg!(windows) { "BelowNormal" } else { "10" };
    assert_eq!(prioriteit(&pid), verwacht);

    // Ter vergelijking: het dicteerwerkproces draait normaal.
    let mut dicteren = instellingen("ok", &extra);
    dicteren.rol = mondig_core::engine::supervisor::Rol::Dicteren;
    let sup_dicteren = geladen(dicteren);
    let pid_dicteren = std::fs::read_to_string(&pidbestand).unwrap();
    assert_ne!(pid, pid_dicteren);
    let normaal = if cfg!(windows) { "Normal" } else { "0" };
    assert_eq!(prioriteit(&pid_dicteren), normaal);
    drop((sup, sup_dicteren));
}

/// Een synthetisch testbestand uit `mondig-core` (de nep-engine leest het niet, maar
/// `LokaleEngine` vraagt eerst de duur op).
fn testbestand(naam: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mondig-core/tests/bestanden")
        .join(naam)
}

#[test]
fn de_wachtrij_met_het_echte_werkproces_annuleert_en_gaat_door_en_stopt_daarna() {
    let map = tempfile::tempdir().unwrap();
    let stopbestand = map.path().join("gestopt.txt");
    let extra = [
        ("MONDIG_NEP_STUKKEN", "6".to_string()),
        ("MONDIG_NEP_STUKTIJD_MS", "100".to_string()),
        (
            "MONDIG_NEP_STOPBESTAND",
            stopbestand.to_string_lossy().into_owned(),
        ),
    ];
    let model = model::zoek("canary-1b-v2-q8").unwrap();
    let engine = LokaleEngine::nieuw(
        instellingen("stukken", &extra),
        &model,
        PathBuf::from("nep"),
        Apparaat::Cpu,
        None,
    );
    let (zender, gebeurtenissen) = mpsc::channel();
    let wachtrij = Wachtrij::start_met(
        engine,
        Some(FilterInstellingen::bestand()),
        zender,
        Duration::from_millis(50),
    );
    let (eerste_id, eerste) = wachtrij.voeg_toe(testbestand("sinus.mp4")).unwrap();
    let (_, tweede) = wachtrij.voeg_toe(testbestand("sinus.mov")).unwrap();

    // Annuleer het eerste zodra het voortgang meldt.
    let tot = Instant::now() + Duration::from_secs(10);
    while !matches!(
        wachtrij.toestand(eerste_id),
        Some(Toestand::Verwerkt {
            voortgang: Some(_),
            ..
        })
    ) && Instant::now() < tot
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    wachtrij.annuleer(eerste_id).unwrap();
    let wacht = Duration::from_secs(10);
    assert!(matches!(
        eerste.recv_timeout(wacht).unwrap(),
        Bestandsuitslag::Geannuleerd
    ));
    let Bestandsuitslag::Klaar(uitgeschreven) = tweede.recv_timeout(wacht).unwrap() else {
        unreachable!("het tweede bestand hoort klaar te komen");
    };
    assert_eq!(uitgeschreven.gefilterd.transcript.segmenten.len(), 6);

    // Is de rij leeg, dan stopt het werkproces netjes.
    let tot = Instant::now() + Duration::from_secs(5);
    while !stopbestand.exists() && Instant::now() < tot {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(stopbestand.exists(), "het werkproces is niet gestopt");
    wachtrij.stop();
    let alles: Vec<_> = gebeurtenissen.try_iter().collect();
    assert!(alles.iter().all(|g| g.soort == Soort::Bestand));
    assert!(
        alles
            .iter()
            .any(|g| g.toestand == Toestand::verwerkt(Some(50)))
    );
}

// ---------- sprekerherkenning bij een bestand (4b-D, fase D-2) ----------

/// Een motor waarvan het spraakmodel een sprekermodel naast zich heeft (een leeg bestand van
/// de goede grootte: het nep-werkproces leest het niet). De map moet blijven bestaan zolang
/// de motor leeft.
fn engine_met_sprekermodel(modus: &str) -> (tempfile::TempDir, LokaleEngine) {
    let modellen = tempfile::tempdir().unwrap();
    let sortformer = model::zoek(model::SPREKERMODEL).unwrap();
    let map = modellen.path().join(model::SPREKERMODEL);
    std::fs::create_dir_all(&map).unwrap();
    std::fs::File::create(map.join(&sortformer.bestanden[0].pad))
        .unwrap()
        .set_len(sortformer.bestanden[0].grootte)
        .unwrap();
    let spraak = model::zoek(model::STANDAARD_MODEL).unwrap();
    let engine = LokaleEngine::nieuw(
        instellingen(modus, &[]),
        &spraak,
        modellen.path().join(model::STANDAARD_MODEL),
        Apparaat::Cpu,
        None,
    );
    (modellen, engine)
}

/// Schrijft het testbestand uit zoals de wachtrij dat doet, zonder filter.
fn schrijf_bestand_uit(
    engine: &mut LokaleEngine,
    sprekers: bool,
    stappen: &mut Vec<Stap>,
    annuleer: &AtomicBool,
) -> Result<mondig_core::aanbieder::Uitgeschreven, mondig_core::aanbieder::AanbiederFout> {
    Bestandsmotor::schrijf_uit(
        engine,
        &testbestand("sinus.mp4"),
        None,
        sprekers,
        &mut |v: Voortgang| stappen.push(v.stap),
        annuleer,
    )
}

#[test]
fn een_bestand_krijgt_sprekers_op_elke_uiting() {
    let (_modellen, mut engine) = engine_met_sprekermodel("stukken");
    let mut stappen = Vec::new();
    let uitgeschreven =
        schrijf_bestand_uit(&mut engine, true, &mut stappen, &AtomicBool::new(false)).unwrap();
    // Het nep-werkproces schrijft vijf stukken van een seconde en geeft per seconde een spreker
    // die om de twee seconden wisselt: 1, 1, 2, 2, 1.
    let sprekers: Vec<(&str, Option<u8>)> = uitgeschreven
        .uitingen()
        .iter()
        .map(|(s, spreker)| (s.tekst.as_str(), *spreker))
        .collect();
    assert_eq!(
        sprekers,
        vec![
            ("stuk 1", Some(1)),
            ("stuk 2", Some(1)),
            ("stuk 3", Some(2)),
            ("stuk 4", Some(2)),
            ("stuk 5", Some(1)),
        ]
    );
    assert_eq!(uitgeschreven.sprekerfout, None);
    // Eerst uitschrijven, dan de stap Sprekers herkennen, in die volgorde.
    let eerste_sprekers = stappen.iter().position(|s| *s == Stap::Sprekers).unwrap();
    assert!(
        stappen[..eerste_sprekers]
            .iter()
            .all(|s| *s == Stap::Uitschrijven)
    );
    assert!(
        stappen[eerste_sprekers..]
            .iter()
            .all(|s| *s == Stap::Sprekers)
    );
}

#[test]
fn zonder_de_instelling_of_het_model_blijft_een_bestand_zoals_het_was() {
    // Instelling uit: het sprekermodel wordt niet gevraagd (deze modus zou falen).
    let (_modellen, mut engine) = engine_met_sprekermodel("fout_sprekers");
    let mut stappen = Vec::new();
    let uit =
        schrijf_bestand_uit(&mut engine, false, &mut stappen, &AtomicBool::new(false)).unwrap();
    assert!(uit.sprekers.is_none() && uit.sprekerfout.is_none());
    assert!(!stappen.contains(&Stap::Sprekers));

    // Instelling aan, maar geen sprekermodel naast het spraakmodel: ook niets, zonder melding.
    let spraak = model::zoek(model::STANDAARD_MODEL).unwrap();
    let mut zonder_model = LokaleEngine::nieuw(
        instellingen("stukken", &[]),
        &spraak,
        PathBuf::from("nep"),
        Apparaat::Cpu,
        None,
    );
    let mut stappen = Vec::new();
    let uit = schrijf_bestand_uit(
        &mut zonder_model,
        true,
        &mut stappen,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(uit.sprekers.is_none() && uit.sprekerfout.is_none());
    assert!(!stappen.contains(&Stap::Sprekers));
}

#[test]
fn een_gevonden_spreker_in_een_bestand_geeft_geen_labels() {
    let (_modellen, mut engine) = engine_met_sprekermodel("een_spreker");
    let uit =
        schrijf_bestand_uit(&mut engine, true, &mut Vec::new(), &AtomicBool::new(false)).unwrap();
    // Eén spreker: er valt niets te onderscheiden, dus het transcript blijft zoals het was.
    assert!(uit.sprekers.is_none() && uit.sprekerfout.is_none());
    // Deze modus schrijft het bestand in één stuk uit.
    assert_eq!(uit.gefilterd.transcript.segmenten.len(), 1);
}

#[test]
fn mislukt_het_herkennen_dan_komt_het_transcript_met_een_melding() {
    let (_modellen, mut engine) = engine_met_sprekermodel("fout_sprekers");
    let uit =
        schrijf_bestand_uit(&mut engine, true, &mut Vec::new(), &AtomicBool::new(false)).unwrap();
    assert!(uit.sprekers.is_none());
    // Deze modus schrijft het bestand in één stuk uit.
    assert_eq!(uit.gefilterd.transcript.segmenten.len(), 1);
    let fout = uit.sprekerfout.unwrap();
    assert!(
        fout.starts_with(mondig_core::gesprek::MELDING_SPREKERS_MISLUKT),
        "{fout}"
    );
}

#[test]
fn annuleren_tijdens_het_herkennen_in_een_bestand_geeft_geen_half_resultaat() {
    let (_modellen, mut engine) = engine_met_sprekermodel("stukken");
    // Het uitschrijven (vijf stukken) duurt ongeveer 250 ms; het herkennen daarna ook. Annuleer
    // zodra de stap Sprekers begint.
    let vlag = AtomicBool::new(false);
    let uitslag = Bestandsmotor::schrijf_uit(
        &mut engine,
        &testbestand("sinus.mp4"),
        None,
        true,
        &mut |v: Voortgang| {
            if v.stap == Stap::Sprekers {
                vlag.store(true, Ordering::SeqCst);
            }
        },
        &vlag,
    );
    assert!(matches!(
        uitslag,
        Err(AanbiederFout::Engine(EngineFout::Geannuleerd))
    ));
}
