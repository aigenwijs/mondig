//! Tests van het uitschrijven van een vergaderspoor (sprint 4b-B) tegen het nep-werkproces:
//! de route via supervisor en `LokaleEngine`, tijden op de tijdlijn, voortgang, annuleren en
//! de weigering van een opname die niet is afgesloten, het samenvoegen tot een gesprek en het
//! herkennen van sprekers (4b-D).

#![allow(clippy::unwrap_used, reason = "tests: falen mag meteen paniek geven")]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use mondig_core::aanbieder::{AanbiederFout, LokaleEngine, Rekenstand};
use mondig_core::bestanden::{Gespreksuitslag, Wachtrij};
use mondig_core::engine::protocol::{Apparaat, FoutSoort, Segment};
use mondig_core::engine::supervisor::{EngineFout, Instellingen, Voortgang};
use mondig_core::gesprek::echo::Ontdubbelen;
use mondig_core::gesprek::{
    Gesprek, Kant, MELDING_MICROFOON_MISLUKT, MELDING_SPREKERS_MISLUKT, MELDING_SYSTEEM_MISLUKT,
    MELDING_SYSTEEM_UITGEVALLEN, MELDING_ZONDER_SYSTEEM, Spoorstand, Vergadersoort,
    schrijf_gesprek_uit,
};
use mondig_core::model;
use mondig_core::spoor::stukken::{LOGBOEK, StukInfo, stuknaam};
use mondig_core::spoor::{EINDE, MELDING_STIL_SYSTEEM, MICROFOON, OPZET, SYSTEEM};
use mondig_core::taak::{Soort, Stap, Toestand};

const NEP: &str = env!("CARGO_BIN_EXE_nep-engine");

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

fn engine(modus: &str, extra: &[(&str, String)]) -> LokaleEngine {
    let model = model::zoek(model::STANDAARD_MODEL).unwrap();
    LokaleEngine::nieuw(
        instellingen(modus, extra),
        &model,
        PathBuf::from("nep"),
        Apparaat::Cpu,
        None,
    )
}

/// Schrijft een stuk zoals de opname dat doet: 44 bytes kop, dan 16-bit samples.
fn schrijf_stuk(pad: &Path, samples: &[i16]) {
    let data = u32::try_from(samples.len() * 2).unwrap();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(data + 36).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&16_000u32.to_le_bytes());
    bytes.extend_from_slice(&32_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    fs::write(pad, bytes).unwrap();
}

/// Geluid per seconde: `true` is een seconde geluid, `false` een seconde stilte.
fn seconden(patroon: &[bool]) -> Vec<i16> {
    patroon
        .iter()
        .flat_map(|&luid| std::iter::repeat_n(if luid { 8000 } else { 0 }, 16_000))
        .collect()
}

/// Schrijft een spoor: per stuk de samples en het anker op de tijdlijn.
fn schrijf_spoor(map: &Path, stukken: &[(Vec<i16>, i64)]) {
    fs::create_dir_all(map).unwrap();
    let mut logboek = String::new();
    for (nummer, (samples, begin_ms)) in stukken.iter().enumerate() {
        let nummer = u32::try_from(nummer).unwrap();
        let info = StukInfo {
            stuk: nummer,
            bestand: stuknaam(nummer),
            samples: samples.len() as u64,
            begin_ms: *begin_ms,
            hersteld: false,
        };
        schrijf_stuk(&map.join(&info.bestand), samples);
        writeln!(
            logboek,
            "{{\"stuk\":{},\"bestand\":\"{}\",\"samples\":{},\"begin_ms\":{}}}",
            info.stuk, info.bestand, info.samples, info.begin_ms
        )
        .unwrap();
    }
    fs::write(map.join(LOGBOEK), logboek).unwrap();
}

/// Schrijft `einde.json` voor een opname van `duur_ms`, met of zonder systeemspoor.
fn schrijf_einde(opnamemap: &Path, duur_ms: u64, systeem: Option<(bool, bool)>) {
    let spoor = |stil: bool, onderbroken: bool| {
        format!(
            "{{\"stukken\":1,\"samples\":0,\"verloren\":0,\"stil\":{stil},\"onderbroken\":{onderbroken}}}"
        )
    };
    let systeem = systeem.map_or_else(|| "null".to_string(), |(s, o)| spoor(s, o));
    fs::write(
        opnamemap.join(EINDE),
        format!(
            "{{\"duur_ms\":{duur_ms},\"hersteld\":false,\"microfoon\":{},\"systeem\":{systeem}}}",
            spoor(false, false)
        ),
    )
    .unwrap();
}

fn segmenten(engine: &mut LokaleEngine, map: &Path) -> Vec<Segment> {
    engine
        .transcribeer_spoor(map, None, &mut |_| {}, &AtomicBool::new(false))
        .unwrap()
        .gefilterd
        .transcript
        .segmenten
}

#[test]
fn een_spoor_komt_terug_op_de_tijdlijn_van_de_opname() {
    let opname = tempfile::tempdir().unwrap();
    let map = opname.path().join(MICROFOON);
    // Twee stukken van twee seconden; het tweede begint pas op 2500 ms (een gat).
    schrijf_spoor(
        &map,
        &[
            (seconden(&[false, true]), 0),
            (seconden(&[true, false]), 2500),
        ],
    );
    schrijf_einde(opname.path(), 4500, None);
    let mut engine = engine("ok", &[]);
    let mut standen = Vec::new();
    let uit = engine
        .transcribeer_spoor(
            &map,
            None,
            &mut |v: Voortgang| standen.push(v),
            &AtomicBool::new(false),
        )
        .unwrap();
    let segmenten = uit.gefilterd.transcript.segmenten;
    let tijden: Vec<(u64, u64, &str)> = segmenten
        .iter()
        .map(|s| (s.start_ms, s.eind_ms, s.tekst.as_str()))
        .collect();
    assert_eq!(
        tijden,
        vec![(1000, 2000, "microfoon 1"), (2500, 3500, "microfoon 2")]
    );
    // De duur is waar het spoor op de tijdlijn ophoudt.
    assert_eq!(
        uit.gefilterd.transcript.audio_duur,
        Duration::from_millis(4500)
    );
    // Een voortgang per seconde, met het totaal van het spoor zelf (4 s).
    assert_eq!(standen.len(), 4);
    assert!(
        standen
            .iter()
            .all(|v| v.totaal == Some(Duration::from_secs(4)))
    );
    assert_eq!(standen.last().unwrap().procent(), Some(100));
}

#[test]
fn een_opname_die_niet_is_afgesloten_wordt_geweigerd_zonder_werkproces() {
    let opname = tempfile::tempdir().unwrap();
    let map = opname.path().join(MICROFOON);
    schrijf_spoor(&map, &[(seconden(&[true]), 0)]);
    let mut engine = engine("ok", &[]);
    let fout = engine
        .transcribeer_spoor(&map, None, &mut |_| {}, &AtomicBool::new(false))
        .err()
        .unwrap();
    assert!(
        matches!(
            fout,
            AanbiederFout::Engine(EngineFout::Engine {
                soort: FoutSoort::Audio,
                ..
            })
        ),
        "{fout:?}"
    );
    assert!(fout.to_string().contains("nog niet afgesloten"), "{fout}");
    assert!(!engine.draait());
}

#[test]
fn zonder_stiltedetectie_komt_er_een_duidelijke_fout() {
    let opname = tempfile::tempdir().unwrap();
    let map = opname.path().join(SYSTEEM);
    schrijf_spoor(&map, &[(seconden(&[true]), 0)]);
    schrijf_einde(opname.path(), 1000, Some((false, false)));
    let mut engine = engine("geen_silero", &[]);
    let fout = engine
        .transcribeer_spoor(&map, None, &mut |_| {}, &AtomicBool::new(false))
        .err()
        .unwrap();
    assert!(
        matches!(
            fout,
            AanbiederFout::Engine(EngineFout::Engine {
                soort: FoutSoort::ModelOntbreekt,
                ..
            })
        ),
        "{fout:?}"
    );
}

#[test]
fn annuleren_breekt_het_spoor_af_en_het_werkproces_draait_door() {
    let opname = tempfile::tempdir().unwrap();
    let map = opname.path().join(MICROFOON);
    schrijf_spoor(&map, &[(seconden(&[true; 20]), 0)]);
    schrijf_einde(opname.path(), 20_000, None);
    let extra = [("MONDIG_NEP_STUKTIJD_MS", "100".to_string())];
    let mut engine = engine("ok", &extra);
    let vlag = Arc::new(AtomicBool::new(false));
    let voor_draad = Arc::clone(&vlag);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        voor_draad.store(true, Ordering::SeqCst);
    });
    let fout = engine
        .transcribeer_spoor(&map, None, &mut |_| {}, &vlag)
        .err()
        .unwrap();
    assert!(
        matches!(fout, AanbiederFout::Engine(EngineFout::Geannuleerd)),
        "{fout:?}"
    );
    assert!(engine.draait());
    // Het volgende spoor gaat gewoon.
    let kort = tempfile::tempdir().unwrap();
    let kort_map = kort.path().join(MICROFOON);
    schrijf_spoor(&kort_map, &[(seconden(&[true]), 0)]);
    schrijf_einde(kort.path(), 1000, None);
    assert_eq!(segmenten(&mut engine, &kort_map).len(), 1);
}

#[test]
fn een_vergadering_wordt_een_gesprek_in_beurten() {
    let opname = tempfile::tempdir().unwrap();
    schrijf_spoor(
        &opname.path().join(MICROFOON),
        &[(seconden(&[true, false, false, false, false, true]), 0)],
    );
    schrijf_spoor(
        &opname.path().join(SYSTEEM),
        &[(seconden(&[false, false, true, true, false, false]), 0)],
    );
    schrijf_einde(opname.path(), 6000, Some((false, false)));
    let mut engine = engine("ok", &[]);
    let mut standen = Vec::new();
    let gesprek = schrijf_gesprek_uit(
        &mut engine,
        opname.path(),
        None,
        Some(&Ontdubbelen::default()),
        true,
        &mut |v: Voortgang| standen.push(v),
        &AtomicBool::new(false),
    )
    .unwrap();
    let beurten: Vec<(Kant, u64, u64, &str)> = gesprek
        .beurten
        .iter()
        .map(|b| (b.kant, b.start_ms, b.eind_ms, b.tekst.as_str()))
        .collect();
    assert_eq!(
        beurten,
        vec![
            (Kant::Jij, 0, 900, "microfoon 0"),
            (Kant::Anderen, 2000, 4000, "systeem 2 systeem 3"),
            (Kant::Jij, 4900, 5900, "microfoon 5"),
        ]
    );
    assert_eq!(gesprek.duur, Duration::from_secs(6));
    assert_eq!(
        (gesprek.jij, gesprek.anderen),
        (Spoorstand::Opgenomen, Spoorstand::Opgenomen)
    );
    assert!(gesprek.meldingen.is_empty());
    assert_eq!(gesprek.weggelaten, 0);
    // De voortgang loopt over beide sporen samen.
    assert_eq!(standen.len(), 12);
    assert!(
        standen
            .iter()
            .all(|v| v.totaal == Some(Duration::from_secs(12)))
    );
    assert!(standen.windows(2).all(|p| p[0].klaar < p[1].klaar));
    assert_eq!(standen[5].procent(), Some(50));
    assert_eq!(standen.last().unwrap().procent(), Some(100));
}

#[test]
fn zonder_systeemspoor_komt_er_alleen_jij_in() {
    let opname = tempfile::tempdir().unwrap();
    schrijf_spoor(&opname.path().join(MICROFOON), &[(seconden(&[true]), 0)]);
    schrijf_einde(opname.path(), 1000, None);
    let mut engine = engine("ok", &[]);
    let gesprek = schrijf_gesprek_uit(
        &mut engine,
        opname.path(),
        None,
        None,
        true,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(gesprek.beurten.len(), 1);
    assert_eq!(gesprek.beurten[0].kant, Kant::Jij);
    assert_eq!(gesprek.anderen, Spoorstand::Ontbrak);
    assert_eq!(gesprek.meldingen, vec![MELDING_ZONDER_SYSTEEM.to_string()]);
}

#[test]
fn een_stil_en_uitgevallen_systeemspoor_geeft_meldingen() {
    let opname = tempfile::tempdir().unwrap();
    schrijf_spoor(&opname.path().join(MICROFOON), &[(seconden(&[true]), 0)]);
    schrijf_spoor(&opname.path().join(SYSTEEM), &[(seconden(&[false]), 0)]);
    schrijf_einde(opname.path(), 1000, Some((true, true)));
    let mut engine = engine("ok", &[]);
    let gesprek = schrijf_gesprek_uit(
        &mut engine,
        opname.path(),
        None,
        None,
        true,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(gesprek.anderen, Spoorstand::Uitgevallen);
    assert_eq!(
        gesprek.meldingen,
        vec![
            MELDING_SYSTEEM_UITGEVALLEN.to_string(),
            MELDING_STIL_SYSTEEM.to_string()
        ]
    );
    assert!(gesprek.beurten.iter().all(|b| b.kant == Kant::Jij));
}

#[test]
fn een_gesprek_van_een_opname_die_niet_is_afgesloten_wordt_geweigerd() {
    let opname = tempfile::tempdir().unwrap();
    schrijf_spoor(&opname.path().join(MICROFOON), &[(seconden(&[true]), 0)]);
    let mut engine = engine("ok", &[]);
    let fout = schrijf_gesprek_uit(
        &mut engine,
        opname.path(),
        None,
        None,
        true,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .err()
    .unwrap();
    assert!(fout.to_string().contains("nog niet afgesloten"), "{fout}");
    assert!(!engine.draait());
}

/// Haalt het eerste stuk van een spoor weg, zodat het logboek naar een bestand wijst dat er
/// niet is en het uitschrijven van dat spoor mislukt.
fn breek_spoor(map: &Path) {
    fs::remove_file(map.join(stuknaam(0))).unwrap();
}

#[test]
fn een_systeemspoor_dat_mislukt_laat_jij_staan() {
    let opname = tempfile::tempdir().unwrap();
    schrijf_spoor(&opname.path().join(MICROFOON), &[(seconden(&[true]), 0)]);
    schrijf_spoor(&opname.path().join(SYSTEEM), &[(seconden(&[true]), 0)]);
    breek_spoor(&opname.path().join(SYSTEEM));
    schrijf_einde(opname.path(), 1000, Some((false, false)));
    let mut engine = engine("ok", &[]);
    let gesprek = schrijf_gesprek_uit(
        &mut engine,
        opname.path(),
        None,
        None,
        true,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(gesprek.beurten.len(), 1);
    assert_eq!(gesprek.beurten[0].kant, Kant::Jij);
    assert_eq!(
        (gesprek.jij, gesprek.anderen),
        (Spoorstand::Opgenomen, Spoorstand::Mislukt)
    );
    assert_eq!(gesprek.meldingen.len(), 1);
    assert!(
        gesprek.meldingen[0].starts_with(MELDING_SYSTEEM_MISLUKT),
        "{:?}",
        gesprek.meldingen
    );
}

#[test]
fn een_microfoonspoor_dat_mislukt_laat_anderen_staan() {
    let opname = tempfile::tempdir().unwrap();
    schrijf_spoor(&opname.path().join(MICROFOON), &[(seconden(&[true]), 0)]);
    schrijf_spoor(&opname.path().join(SYSTEEM), &[(seconden(&[true]), 0)]);
    breek_spoor(&opname.path().join(MICROFOON));
    schrijf_einde(opname.path(), 1000, Some((false, false)));
    let mut engine = engine("ok", &[]);
    let gesprek = schrijf_gesprek_uit(
        &mut engine,
        opname.path(),
        None,
        None,
        true,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(gesprek.beurten.len(), 1);
    assert_eq!(gesprek.beurten[0].kant, Kant::Anderen);
    assert_eq!(gesprek.jij, Spoorstand::Mislukt);
    assert!(gesprek.meldingen[0].starts_with(MELDING_MICROFOON_MISLUKT));
}

#[test]
fn als_geen_spoor_lukt_is_er_geen_gesprek() {
    let opname = tempfile::tempdir().unwrap();
    schrijf_spoor(&opname.path().join(MICROFOON), &[(seconden(&[true]), 0)]);
    schrijf_spoor(&opname.path().join(SYSTEEM), &[(seconden(&[true]), 0)]);
    schrijf_einde(opname.path(), 1000, Some((false, false)));
    let mut engine = engine("geen_silero", &[]);
    let fout = schrijf_gesprek_uit(
        &mut engine,
        opname.path(),
        None,
        None,
        true,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .err()
    .unwrap();
    assert!(
        matches!(
            fout,
            AanbiederFout::Engine(EngineFout::Engine {
                soort: FoutSoort::ModelOntbreekt,
                ..
            })
        ),
        "{fout}"
    );
}

/// Zet de soort in `opname.json`, zoals de opname dat doet.
fn schrijf_soort(opnamemap: &Path, soort: &str) {
    fs::write(
        opnamemap.join(OPZET),
        format!(
            "{{\"versie\":1,\"begin_unix_ms\":1,\"samplerate\":16000,\"stuk_seconden\":60,\"systeemgeluid\":false,\"soort\":\"{soort}\"}}"
        ),
    )
    .unwrap();
}

#[test]
fn in_de_ruimte_alinea_s_zonder_systeemmelding_en_zonder_echo() {
    let opname = tempfile::tempdir().unwrap();
    // Geluid op 0 en 2 s (pauze 1 s: een alinea), en op 7 s (pauze 4 s: een nieuwe).
    schrijf_spoor(
        &opname.path().join(MICROFOON),
        &[(
            seconden(&[true, false, true, false, false, false, false, true]),
            0,
        )],
    );
    schrijf_einde(opname.path(), 8000, None);
    schrijf_soort(opname.path(), "ruimte");
    let mut engine = engine("ok", &[]);
    let gesprek = schrijf_gesprek_uit(
        &mut engine,
        opname.path(),
        None,
        Some(&Ontdubbelen::default()),
        true,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(gesprek.soort, Vergadersoort::Ruimte);
    let beurten: Vec<(Kant, u64, u64, &str)> = gesprek
        .beurten
        .iter()
        .map(|b| (b.kant, b.start_ms, b.eind_ms, b.tekst.as_str()))
        .collect();
    // Geen verschuiving van 100 ms: er is geen tweede spoor.
    assert_eq!(
        beurten,
        vec![
            (Kant::Jij, 0, 3000, "microfoon 0 microfoon 2"),
            (Kant::Jij, 7000, 8000, "microfoon 7"),
        ]
    );
    assert!(
        !gesprek
            .meldingen
            .iter()
            .any(|m| m == MELDING_ZONDER_SYSTEEM),
        "{:?}",
        gesprek.meldingen
    );
    assert!(gesprek.meldingen.is_empty());
    assert_eq!(gesprek.anderen, Spoorstand::Ontbrak);
    assert_eq!(gesprek.weggelaten, 0);

    // Dezelfde opname online: wel de melding dat het geluid van de computer ontbrak.
    schrijf_soort(opname.path(), "online");
    let online = schrijf_gesprek_uit(
        &mut engine,
        opname.path(),
        None,
        Some(&Ontdubbelen::default()),
        true,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(online.soort, Vergadersoort::Online);
    assert_eq!(online.meldingen, vec![MELDING_ZONDER_SYSTEEM.to_string()]);
}

#[test]
fn in_de_ruimte_telt_een_systeemspoor_niet_mee() {
    let opname = tempfile::tempdir().unwrap();
    schrijf_spoor(&opname.path().join(MICROFOON), &[(seconden(&[true]), 0)]);
    schrijf_spoor(&opname.path().join(SYSTEEM), &[(seconden(&[true]), 0)]);
    schrijf_einde(opname.path(), 1000, Some((false, false)));
    schrijf_soort(opname.path(), "ruimte");
    let mut engine = engine("ok", &[]);
    let gesprek = schrijf_gesprek_uit(
        &mut engine,
        opname.path(),
        None,
        Some(&Ontdubbelen::default()),
        true,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(gesprek.beurten.len(), 1);
    assert_eq!(gesprek.beurten[0].tekst, "microfoon 0");
}

#[test]
fn de_wachtrij_schrijft_een_vergadering_uit_met_het_echte_werkproces() {
    let opname = tempfile::tempdir().unwrap();
    schrijf_spoor(
        &opname.path().join(MICROFOON),
        &[(seconden(&[true, false, false, false, false, true]), 0)],
    );
    schrijf_spoor(
        &opname.path().join(SYSTEEM),
        &[(seconden(&[false, false, true, true, false, false]), 0)],
    );
    schrijf_einde(opname.path(), 6000, Some((false, false)));
    let (zender, gebeurtenissen) = mpsc::channel();
    let wachtrij = Wachtrij::start(engine("ok", &[]), None, zender);
    let (id, ontvanger) = wachtrij
        .voeg_vergadering_toe(opname.path().to_path_buf())
        .unwrap();
    let uitslag = ontvanger.recv_timeout(Duration::from_secs(20)).unwrap();
    assert!(
        matches!(uitslag, Gespreksuitslag::Klaar(_)),
        "de vergadering is niet uitgeschreven"
    );
    let Gespreksuitslag::Klaar(gesprek) = uitslag else {
        return;
    };
    assert_eq!(gesprek.soort, Vergadersoort::Online);
    let kanten: Vec<Kant> = gesprek.beurten.iter().map(|b| b.kant).collect();
    assert_eq!(kanten, vec![Kant::Jij, Kant::Anderen, Kant::Jij]);
    assert_eq!(wachtrij.toestand(id), Some(Toestand::Klaar));
    let alles: Vec<_> = gebeurtenissen.try_iter().filter(|g| g.taak == id).collect();
    assert!(alles.iter().all(|g| g.soort == Soort::Vergadering));
    assert!(
        alles
            .iter()
            .any(|g| g.toestand == Toestand::verwerkt(Some(50)))
    );
    wachtrij.stop();
}

// ---------- sprekerherkenning (4b-D) ----------

/// Een motor waarvan het spraakmodel een sprekermodel naast zich heeft (een leeg bestand van
/// de goede grootte: het nep-werkproces leest het niet). De map moet blijven bestaan zolang
/// de motor leeft.
fn engine_met_sprekermodel(modus: &str, apparaat: Apparaat) -> (tempfile::TempDir, LokaleEngine) {
    let modellen = tempfile::tempdir().unwrap();
    let sortformer = model::zoek(model::SPREKERMODEL).unwrap();
    let map = modellen.path().join(model::SPREKERMODEL);
    fs::create_dir_all(&map).unwrap();
    fs::File::create(map.join(&sortformer.bestanden[0].pad))
        .unwrap()
        .set_len(sortformer.bestanden[0].grootte)
        .unwrap();
    let spraak = model::zoek(model::STANDAARD_MODEL).unwrap();
    let engine = LokaleEngine::nieuw(
        instellingen(modus, &[]),
        &spraak,
        modellen.path().join(model::STANDAARD_MODEL),
        apparaat,
        None,
    );
    (modellen, engine)
}

/// Een online opname: Jij op 9 s, Anderen van 0 tot 6 s. Het nep-werkproces geeft Anderen
/// spreker 1 (0-2 s), 2 (2-4 s) en weer 1 (4-6 s).
fn online_opname_met_drie_beurten_van_anderen() -> tempfile::TempDir {
    let opname = tempfile::tempdir().unwrap();
    let mut jij = vec![false; 9];
    jij.push(true);
    schrijf_spoor(&opname.path().join(MICROFOON), &[(seconden(&jij), 0)]);
    schrijf_spoor(&opname.path().join(SYSTEEM), &[(seconden(&[true; 6]), 0)]);
    schrijf_einde(opname.path(), 10_000, Some((false, false)));
    opname
}

fn gesprek_van(engine: &mut LokaleEngine, opname: &Path, sprekers: bool) -> Gesprek {
    schrijf_gesprek_uit(
        engine,
        opname,
        None,
        Some(&Ontdubbelen::default()),
        sprekers,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn sprekers_van(gesprek: &Gesprek) -> Vec<(Kant, Option<u8>, String, &str)> {
    gesprek
        .beurten
        .iter()
        .map(|b| (b.kant, b.sprekernummer, b.spreker(), b.tekst.as_str()))
        .collect()
}

#[test]
fn online_wordt_anderen_gesplitst_in_sprekers() {
    let opname = online_opname_met_drie_beurten_van_anderen();
    let (_modellen, mut engine) = engine_met_sprekermodel("ok", Apparaat::Cpu);
    assert!(engine.heeft_sprekermodel());
    let gesprek = gesprek_van(&mut engine, opname.path(), true);
    assert_eq!(
        sprekers_van(&gesprek),
        vec![
            (
                Kant::Anderen,
                Some(1),
                "Spreker 1".into(),
                "systeem 0 systeem 1"
            ),
            (
                Kant::Anderen,
                Some(2),
                "Spreker 2".into(),
                "systeem 2 systeem 3"
            ),
            (
                Kant::Anderen,
                Some(1),
                "Spreker 1".into(),
                "systeem 4 systeem 5"
            ),
            (Kant::Jij, None, "Jij".into(), "microfoon 9"),
        ]
    );
    assert!(gesprek.meldingen.is_empty(), "{:?}", gesprek.meldingen);
    // Met de instelling uit: zoals zonder sprekermodel.
    let zonder = gesprek_van(&mut engine, opname.path(), false);
    assert!(zonder.beurten.iter().all(|b| b.sprekernummer.is_none()));
    assert_eq!(zonder.beurten.len(), 2);
}

#[test]
fn zonder_sprekermodel_blijft_alles_zoals_het_was() {
    let opname = online_opname_met_drie_beurten_van_anderen();
    // De gewone testmotor heeft geen sprekermodel naast zich: het werkproces wordt niet eens
    // gevraagd (deze modus zou anders een fout geven).
    let mut engine = engine("geen_sprekermodel", &[]);
    assert!(!engine.heeft_sprekermodel());
    let gesprek = gesprek_van(&mut engine, opname.path(), true);
    assert!(gesprek.beurten.iter().all(|b| b.sprekernummer.is_none()));
    assert_eq!(gesprek.beurten[0].spreker(), "Anderen");
    assert!(gesprek.meldingen.is_empty());
}

#[test]
fn een_gevonden_spreker_geeft_geen_nummers() {
    let opname = online_opname_met_drie_beurten_van_anderen();
    let (_modellen, mut engine) = engine_met_sprekermodel("een_spreker", Apparaat::Cpu);
    let gesprek = gesprek_van(&mut engine, opname.path(), true);
    assert!(gesprek.beurten.iter().all(|b| b.sprekernummer.is_none()));
    assert_eq!(
        gesprek.beurten[0].tekst,
        "systeem 0 systeem 1 systeem 2 systeem 3 systeem 4 systeem 5"
    );
}

#[test]
fn mislukt_herkennen_geeft_het_verslag_zonder_sprekers_en_een_melding() {
    let opname = online_opname_met_drie_beurten_van_anderen();
    let (_modellen, mut engine) = engine_met_sprekermodel("fout_sprekers", Apparaat::Cpu);
    let gesprek = gesprek_van(&mut engine, opname.path(), true);
    assert!(gesprek.beurten.iter().all(|b| b.sprekernummer.is_none()));
    assert_eq!(gesprek.beurten.len(), 2);
    assert_eq!(gesprek.meldingen.len(), 1);
    assert!(
        gesprek.meldingen[0].starts_with(MELDING_SPREKERS_MISLUKT),
        "{:?}",
        gesprek.meldingen
    );
}

#[test]
fn een_falend_sprekermodel_op_de_videokaart_laat_het_spraakmodel_daar() {
    let opname = online_opname_met_drie_beurten_van_anderen();
    let (_modellen, mut engine) = engine_met_sprekermodel("fout_sprekers", Apparaat::Gpu);
    let gesprek = gesprek_van(&mut engine, opname.path(), true);
    assert!(gesprek.beurten.iter().all(|b| b.sprekernummer.is_none()));
    assert_eq!(gesprek.meldingen.len(), 1, "{:?}", gesprek.meldingen);
    // Geen herstart op de processor: de motor rekent nog op de videokaart.
    assert_eq!(engine.rekenstand(), Some(Rekenstand::Videokaart));
}

#[test]
fn het_herkennen_meldt_een_eigen_stap_in_de_voortgang() {
    let opname = online_opname_met_drie_beurten_van_anderen();
    let (_modellen, mut engine) = engine_met_sprekermodel("ok", Apparaat::Cpu);
    let mut standen = Vec::new();
    schrijf_gesprek_uit(
        &mut engine,
        opname.path(),
        None,
        Some(&Ontdubbelen::default()),
        true,
        &mut |v: Voortgang| standen.push(v),
        &AtomicBool::new(false),
    )
    .unwrap();
    let eerste = standen
        .iter()
        .position(|v| v.stap == Stap::Sprekers)
        .unwrap();
    // Eerst uitschrijven, dan alleen nog sprekers herkennen.
    assert!(
        standen[..eerste]
            .iter()
            .all(|v| v.stap == Stap::Uitschrijven)
    );
    assert!(standen[eerste..].iter().all(|v| v.stap == Stap::Sprekers));
    assert_eq!(standen.last().unwrap().procent(), Some(100));
}

#[test]
fn een_spoor_zonder_tekst_gaat_niet_naar_het_sprekermodel() {
    let opname = tempfile::tempdir().unwrap();
    schrijf_spoor(&opname.path().join(MICROFOON), &[(seconden(&[true]), 0)]);
    schrijf_spoor(&opname.path().join(SYSTEEM), &[(seconden(&[false; 3]), 0)]);
    schrijf_einde(opname.path(), 3000, Some((false, false)));
    // Deze modus zou falen als het sprekermodel gevraagd werd.
    let (_modellen, mut engine) = engine_met_sprekermodel("fout_sprekers", Apparaat::Cpu);
    let gesprek = gesprek_van(&mut engine, opname.path(), true);
    assert!(gesprek.meldingen.is_empty(), "{:?}", gesprek.meldingen);
}

#[test]
fn een_crash_op_de_videokaart_valt_terug_op_de_processor() {
    let opname = online_opname_met_drie_beurten_van_anderen();
    let (_modellen, mut engine) = engine_met_sprekermodel("crash_sprekers_gpu", Apparaat::Gpu);
    let gesprek = gesprek_van(&mut engine, opname.path(), true);
    assert_eq!(gesprek.sprekernummers(), vec![1, 2]);
    assert!(gesprek.meldingen.is_empty(), "{:?}", gesprek.meldingen);
}

#[test]
fn annuleren_tijdens_het_herkennen_breekt_af_en_het_werkproces_draait_door() {
    let opname = online_opname_met_drie_beurten_van_anderen();
    let (_modellen, mut engine) = engine_met_sprekermodel("ok", Apparaat::Cpu);
    let vlag = AtomicBool::new(false);
    let fout = engine
        .herken_sprekers(
            &opname.path().join(SYSTEEM),
            // Na de eerste seconde: annuleren.
            &mut |_| vlag.store(true, Ordering::SeqCst),
            &vlag,
        )
        .err()
        .unwrap();
    assert!(
        matches!(fout, AanbiederFout::Engine(EngineFout::Geannuleerd)),
        "{fout}"
    );
    // Het werkproces draait door: de volgende vraag lukt.
    vlag.store(false, Ordering::SeqCst);
    let sprekers = engine
        .herken_sprekers(&opname.path().join(SYSTEEM), &mut |_| {}, &vlag)
        .unwrap()
        .unwrap();
    assert_eq!(sprekers.len(), 6);
    // De tijden staan op de tijdlijn van de opname.
    assert_eq!((sprekers[2].start_ms, sprekers[2].eind_ms), (2000, 3000));
}

#[test]
fn in_de_ruimte_wordt_de_microfoon_gesplitst_in_sprekers() {
    let opname = tempfile::tempdir().unwrap();
    schrijf_spoor(&opname.path().join(MICROFOON), &[(seconden(&[true; 4]), 0)]);
    schrijf_einde(opname.path(), 4000, None);
    schrijf_soort(opname.path(), "ruimte");
    let (_modellen, mut engine) = engine_met_sprekermodel("ok", Apparaat::Cpu);
    let gesprek = gesprek_van(&mut engine, opname.path(), true);
    assert!(gesprek.met_sprekers());
    assert_eq!(
        sprekers_van(&gesprek),
        vec![
            (
                Kant::Jij,
                Some(1),
                "Spreker 1".into(),
                "microfoon 0 microfoon 1"
            ),
            (
                Kant::Jij,
                Some(2),
                "Spreker 2".into(),
                "microfoon 2 microfoon 3"
            ),
        ]
    );
    assert_eq!(
        gesprek.als_tekst(),
        "Spreker 1: microfoon 0 microfoon 1\n\nSpreker 2: microfoon 2 microfoon 3"
    );
    // Alleen ik: nooit sprekers.
    schrijf_soort(opname.path(), "alleen");
    let alleen = gesprek_van(&mut engine, opname.path(), true);
    assert!(!alleen.met_sprekers());
}
