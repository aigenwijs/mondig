//! Tests van de supervisor tegen het nep-werkproces `nep-engine`.
//!
//! Het nep-proces crasht, hangt of stuurt een fout, afhankelijk van de omgevingsvariabele
//! `MONDIG_NEP`. Zo testen we de supervisor zonder echt model en zonder videokaart.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mondig_core::aanbieder::{Aanbieder, AanbiederFout, Invoer, LokaleEngine, Rekenstand};
use mondig_core::engine::protocol::{Apparaat, FoutSoort, KaartSoort, Videokaart};
use mondig_core::engine::supervisor::{
    Bouwcontrole, EngineFout, Fase, Instellingen, LaadOpdracht, Supervisor,
};
use mondig_core::filter::{FilterInstellingen, Uitkomst};
use mondig_core::model;
use mondig_core::soevereiniteit::Soevereiniteit;
use mondig_engine::BOUWCODE;

const NEP: &str = env!("CARGO_BIN_EXE_nep-engine");

/// Instellingen met korte time-outs, zodat de tests snel zijn.
fn instellingen(modus: &str) -> Instellingen {
    let mut i = Instellingen::standaard(PathBuf::from(NEP));
    i.hallo_timeout = Duration::from_secs(2);
    i.laad_timeout = Duration::from_secs(2);
    i.transcribeer_basis = Duration::from_secs(2);
    i.transcribeer_factor = 0;
    i.omgeving = vec![("MONDIG_NEP".into(), modus.into())];
    i
}

fn opdracht(apparaat: Apparaat) -> LaadOpdracht {
    LaadOpdracht {
        model_id: "nep".into(),
        modelmap: PathBuf::from("nep"),
        apparaat,
        taal: Some("nl".into()),
    }
}

fn tekst(uitkomst: &mondig_core::engine::supervisor::Uitkomst) -> String {
    uitkomst
        .segmenten
        .iter()
        .map(|s| s.tekst.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

const KORT: Duration = Duration::from_secs(1);

#[test]
fn gewone_opdracht_geeft_een_resultaat() {
    let mut sup = Supervisor::nieuw(instellingen("ok"));
    assert_eq!(sup.laad(&opdracht(Apparaat::Cpu)).unwrap(), Apparaat::Cpu);
    let uitkomst = sup.transcribeer(Path::new("x.wav"), KORT).unwrap();
    assert_eq!(tekst(&uitkomst), "nep op Cpu");
    assert_eq!(uitkomst.duur_ms, 1000);
}

#[test]
fn transcriberen_zonder_model_geeft_niet_geladen() {
    let mut sup = Supervisor::nieuw(instellingen("ok"));
    assert!(matches!(
        sup.transcribeer(Path::new("x.wav"), KORT),
        Err(EngineFout::NietGeladen)
    ));
}

#[test]
fn proces_dat_niet_bestaat_geeft_een_startfout() {
    let mut i = instellingen("ok");
    i.engine_pad = PathBuf::from("bestaat-echt-niet.exe");
    let mut sup = Supervisor::nieuw(i);
    let fout = sup.laad(&opdracht(Apparaat::Cpu)).unwrap_err();
    assert!(matches!(fout, EngineFout::Starten { .. }), "{fout:?}");
    assert!(fout.to_string().contains("niet gestart"));
}

#[test]
fn crash_bij_laden_geeft_een_nette_fout_met_code_en_log() {
    let mut sup = Supervisor::nieuw(instellingen("crash_laad"));
    let fout = sup.laad(&opdracht(Apparaat::Cpu)).unwrap_err();
    let EngineFout::Gecrasht { code, laatste_log } = &fout else {
        unreachable!("verwachtte Gecrasht, kreeg {fout:?}");
    };
    assert_eq!(*code, Some(139));
    assert!(
        laatste_log.iter().any(|r| r.contains("crash")),
        "{laatste_log:?}"
    );
    assert!(fout.to_string().contains("onverwacht gestopt (code 139)"));
}

#[test]
fn crash_bij_transcriberen_op_cpu_geeft_een_fout_zonder_nieuwe_poging() {
    let mut sup = Supervisor::nieuw(instellingen("crash_transcribeer"));
    sup.laad(&opdracht(Apparaat::Cpu)).unwrap();
    let fout = sup.transcribeer(Path::new("x.wav"), KORT).unwrap_err();
    assert!(matches!(fout, EngineFout::Gecrasht { .. }), "{fout:?}");
}

#[test]
fn proces_dat_niet_hallo_zegt_haalt_de_timeout() {
    let mut sup = Supervisor::nieuw(instellingen("hang_hallo"));
    let fout = sup.laad(&opdracht(Apparaat::Cpu)).unwrap_err();
    assert!(
        matches!(
            fout,
            EngineFout::TijdOverschreden {
                fase: Fase::Opstarten,
                ..
            }
        ),
        "{fout:?}"
    );
}

#[test]
fn laden_dat_blijft_hangen_haalt_de_timeout() {
    let mut sup = Supervisor::nieuw(instellingen("hang_laad"));
    let fout = sup.laad(&opdracht(Apparaat::Cpu)).unwrap_err();
    assert!(
        matches!(
            fout,
            EngineFout::TijdOverschreden {
                fase: Fase::Laden,
                ..
            }
        ),
        "{fout:?}"
    );
    assert!(fout.to_string().contains("niet binnen 2 seconden"));
}

#[test]
fn transcriberen_dat_blijft_hangen_haalt_de_timeout_en_ruimt_op() {
    let mut sup = Supervisor::nieuw(instellingen("hang_transcribeer"));
    sup.laad(&opdracht(Apparaat::Cpu)).unwrap();
    let fout = sup.transcribeer(Path::new("x.wav"), KORT).unwrap_err();
    assert!(
        matches!(
            fout,
            EngineFout::TijdOverschreden {
                fase: Fase::Transcriberen,
                ..
            }
        ),
        "{fout:?}"
    );
}

#[test]
fn time_out_voor_uitschrijven_groeit_met_de_duur_van_de_audio() {
    let mut i = Instellingen::standaard(PathBuf::from(NEP));
    i.transcribeer_basis = Duration::from_secs(10);
    i.transcribeer_factor = 5;
    assert_eq!(
        i.transcribeer_timeout(Duration::from_secs(60)),
        Duration::from_secs(310)
    );
}

#[test]
fn enginefout_bij_uitschrijven_komt_door_en_proces_blijft_leven() {
    let mut sup = Supervisor::nieuw(instellingen("fout_transcribeer"));
    sup.laad(&opdracht(Apparaat::Cpu)).unwrap();
    for _ in 0..2 {
        let fout = sup.transcribeer(Path::new("x.wav"), KORT).unwrap_err();
        assert!(
            matches!(
                fout,
                EngineFout::Engine {
                    soort: FoutSoort::Transcriberen,
                    ..
                }
            ),
            "{fout:?}"
        );
    }
}

#[test]
fn gpu_die_crasht_bij_laden_valt_terug_op_cpu() {
    let mut sup = Supervisor::nieuw(instellingen("crash_laad_gpu"));
    assert_eq!(sup.laad(&opdracht(Apparaat::Gpu)).unwrap(), Apparaat::Cpu);
    assert_eq!(sup.apparaat(), Some(Apparaat::Cpu));
    let uitkomst = sup.transcribeer(Path::new("x.wav"), KORT).unwrap();
    assert_eq!(tekst(&uitkomst), "nep op Cpu");
}

#[test]
fn gpu_die_crasht_bij_uitschrijven_valt_terug_op_cpu() {
    let mut sup = Supervisor::nieuw(instellingen("crash_transcribeer_gpu"));
    assert_eq!(sup.laad(&opdracht(Apparaat::Gpu)).unwrap(), Apparaat::Gpu);
    let uitkomst = sup.transcribeer(Path::new("x.wav"), KORT).unwrap();
    assert_eq!(tekst(&uitkomst), "nep op Cpu");
    assert_eq!(sup.apparaat(), Some(Apparaat::Cpu));
}

#[test]
fn gpu_die_hangt_bij_laden_valt_niet_terug_als_cpu_ook_hangt() {
    // Zowel de eerste poging als de terugval hangt: dan krijgt de gebruiker de time-out.
    let mut sup = Supervisor::nieuw(instellingen("hang_laad"));
    let fout = sup.laad(&opdracht(Apparaat::Gpu)).unwrap_err();
    assert!(
        matches!(fout, EngineFout::TijdOverschreden { .. }),
        "{fout:?}"
    );
}

#[test]
fn gpu_en_cpu_die_allebei_crashen_geven_de_fout_na_een_enkele_terugval() {
    let mut sup = Supervisor::nieuw(instellingen("crash_laad"));
    let fout = sup.laad(&opdracht(Apparaat::Gpu)).unwrap_err();
    assert!(matches!(fout, EngineFout::Gecrasht { .. }), "{fout:?}");
}

#[test]
fn kapot_audiobestand_geeft_geen_terugval_naar_cpu() {
    let mut sup = Supervisor::nieuw(instellingen("fout_audio"));
    sup.laad(&opdracht(Apparaat::Gpu)).unwrap();
    let fout = sup.transcribeer(Path::new("x.wav"), KORT).unwrap_err();
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
    assert_eq!(sup.apparaat(), Some(Apparaat::Gpu), "geen nieuwe poging");
}

#[test]
fn uitvoer_die_geen_bericht_is_wordt_overgeslagen() {
    let mut sup = Supervisor::nieuw(instellingen("rommel"));
    sup.laad(&opdracht(Apparaat::Cpu)).unwrap();
    let uitkomst = sup.transcribeer(Path::new("x.wav"), KORT).unwrap();
    assert_eq!(tekst(&uitkomst), "nep op Cpu");
}

#[test]
fn andere_protocolversie_wordt_geweigerd() {
    let mut sup = Supervisor::nieuw(instellingen("foute_versie"));
    let fout = sup.laad(&opdracht(Apparaat::Cpu)).unwrap_err();
    assert!(matches!(fout, EngineFout::Protocol(_)), "{fout:?}");
}

/// Instellingen die afdwingen dat het werkproces de bouwcode van deze testbouw meldt.
fn streng(modus: &str) -> Instellingen {
    let mut i = instellingen(modus);
    i.bouwcontrole = Bouwcontrole::Afdwingen(BOUWCODE.into());
    i
}

#[test]
fn werkproces_met_dezelfde_bouwcode_start_gewoon() {
    let mut sup = Supervisor::nieuw(streng("ok"));
    assert_eq!(sup.laad(&opdracht(Apparaat::Cpu)).unwrap(), Apparaat::Cpu);
}

#[test]
fn werkproces_met_andere_bouwcode_wordt_geweigerd() {
    let mut sup = Supervisor::nieuw(streng("andere_bouw"));
    let fout = sup.laad(&opdracht(Apparaat::Cpu)).unwrap_err();
    assert!(
        matches!(
            fout,
            EngineFout::NietBijgewerkt {
                gevonden: Some(_),
                ..
            }
        ),
        "{fout:?}"
    );
    assert!(!sup.draait(), "het proces hoort gestopt te zijn");
}

#[test]
fn oud_werkproces_zonder_bouwcode_wordt_herkend_en_crasht_niet() {
    let mut sup = Supervisor::nieuw(streng("oud_werkproces"));
    let fout = sup.laad(&opdracht(Apparaat::Cpu)).unwrap_err();
    assert!(
        matches!(fout, EngineFout::NietBijgewerkt { gevonden: None, .. }),
        "{fout:?}"
    );
    assert!(!fout.kan_aan_apparaat_liggen());
    assert!(!sup.draait());
}

#[test]
fn andere_protocolversie_is_ook_niet_bijgewerkt_bij_een_controle() {
    let mut sup = Supervisor::nieuw(streng("foute_versie"));
    let fout = sup.laad(&opdracht(Apparaat::Cpu)).unwrap_err();
    assert!(
        matches!(fout, EngineFout::NietBijgewerkt { .. }),
        "{fout:?}"
    );
}

#[test]
fn in_ontwikkeling_draait_een_afwijkende_bouw_door() {
    let mut i = instellingen("andere_bouw");
    i.bouwcontrole = Bouwcontrole::Waarschuwen(BOUWCODE.into());
    let mut sup = Supervisor::nieuw(i);
    assert_eq!(sup.laad(&opdracht(Apparaat::Cpu)).unwrap(), Apparaat::Cpu);
}

#[test]
fn proces_wordt_netjes_gestopt_bij_drop() {
    let map = tempfile::tempdir().unwrap();
    let stopbestand = map.path().join("gestopt.txt");
    let mut i = instellingen("ok");
    i.omgeving.push((
        "MONDIG_NEP_STOPBESTAND".into(),
        stopbestand.to_string_lossy().into_owned(),
    ));
    let mut sup = Supervisor::nieuw(i);
    sup.laad(&opdracht(Apparaat::Cpu)).unwrap();
    assert!(!stopbestand.exists());
    drop(sup);
    assert!(
        stopbestand.exists(),
        "het werkproces kreeg Stop en sloot zelf af"
    );
}

/// Schrijft een wav van een seconde: stilte, of een toon als `luid` waar is.
#[allow(
    clippy::unwrap_used,
    reason = "testhulp: falen mag hier meteen paniek geven"
)]
fn schrijf_wav(map: &Path, luid: bool) -> PathBuf {
    let wav = map.join("opname.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut schrijver = hound::WavWriter::create(&wav, spec).unwrap();
    for n in 0..16_000_i32 {
        let waarde = if luid && n % 2 == 0 { 8000_i16 } else { 0 };
        schrijver.write_sample(waarde).unwrap();
    }
    schrijver.finalize().unwrap();
    wav
}

#[allow(
    clippy::unwrap_used,
    reason = "testhulp: falen mag hier meteen paniek geven"
)]
fn lokale_engine(modus: &str) -> LokaleEngine {
    let model = model::zoek("whisper-large-v3-turbo-q8").unwrap();
    LokaleEngine::nieuw(
        instellingen(modus),
        &model,
        PathBuf::from("nep"),
        Apparaat::Cpu,
        None,
    )
}

#[test]
fn lokale_engine_levert_een_transcript_met_label_lokaal() {
    let map = tempfile::tempdir().unwrap();
    let wav = schrijf_wav(map.path(), false);
    let mut engine = lokale_engine("ok");
    assert_eq!(engine.soevereiniteit(), Soevereiniteit::Lokaal);
    let transcript = engine.transcribeer(Invoer::Bestand(&wav)).unwrap();
    assert_eq!(transcript.tekst(), "nep op Cpu");
}

#[test]
fn filter_gebruikt_de_meting_van_de_engine_en_niet_de_audio_van_de_app() {
    let map = tempfile::tempdir().unwrap();
    let filter = FilterInstellingen::standaard();

    // De audio is luid, maar de engine meldt stilte en een verzonnen stiltezin: de app
    // vertrouwt de meting (en decodeert de audio dus niet zelf).
    let luid = schrijf_wav(map.path(), true);
    let gefilterd = lokale_engine("stil")
        .transcribeer_gefilterd(Invoer::Bestand(&luid), &filter)
        .unwrap();
    assert_eq!(gefilterd.uitkomst, Uitkomst::GeenGeluid);
    assert!(gefilterd.transcript.segmenten.is_empty());

    // Andersom: de audio is stil, maar de engine meldt een hoorbare opname.
    let stil = schrijf_wav(map.path(), false);
    let gefilterd = lokale_engine("ok")
        .transcribeer_gefilterd(Invoer::Bestand(&stil), &filter)
        .unwrap();
    assert_eq!(gefilterd.uitkomst, Uitkomst::Tekst);
    assert_eq!(gefilterd.transcript.tekst(), "nep op Cpu");
    assert!(gefilterd.aandeel_spraak > 0.9);
}

#[test]
fn lokale_engine_geeft_audiofout_zonder_het_proces_te_starten() {
    let model = model::zoek("whisper-large-v3-turbo-q8").unwrap();
    let mut i = instellingen("ok");
    i.engine_pad = PathBuf::from("bestaat-echt-niet.exe");
    let mut engine = LokaleEngine::nieuw(i, &model, PathBuf::from("nep"), Apparaat::Cpu, None);
    let fout = engine
        .transcribeer(Invoer::Bestand(Path::new("bestaat-niet.wav")))
        .unwrap_err();
    assert!(matches!(fout, AanbiederFout::Audio(_)), "{fout:?}");
}

#[test]
fn ongeldige_utf8_op_stdout_en_stderr_stopt_de_lezers_niet() {
    // Het nep-proces schrijft bytes die geen UTF-8 zijn, voor elk antwoord. Een lezer die
    // daarop afbreekt, liet stdout als crash zien en stderr volstromen.
    let mut sup = Supervisor::nieuw(instellingen("ongeldig_utf8"));
    sup.laad(&opdracht(Apparaat::Cpu)).unwrap();
    for _ in 0..2 {
        let uitkomst = sup.transcribeer(Path::new("x.wav"), KORT).unwrap();
        assert_eq!(tekst(&uitkomst), "nep op Cpu");
    }
}

#[test]
fn tekst_zonder_regeleinde_voor_een_antwoord_haalt_het_antwoord_niet_weg() {
    // Een native bibliotheek schrijft een fragment zonder regeleinde op stdout.
    let mut sup = Supervisor::nieuw(instellingen("fragment"));
    sup.laad(&opdracht(Apparaat::Cpu)).unwrap();
    let uitkomst = sup.transcribeer(Path::new("x.wav"), KORT).unwrap();
    assert_eq!(tekst(&uitkomst), "nep op Cpu");
}

#[test]
fn onverwacht_antwoord_bij_uitschrijven_stopt_het_proces() {
    let map = tempfile::tempdir().unwrap();
    let stopbestand = map.path().join("gestopt.txt");
    let mut i = instellingen("verkeerd_antwoord");
    i.omgeving.push((
        "MONDIG_NEP_STOPBESTAND".into(),
        stopbestand.to_string_lossy().into_owned(),
    ));
    let mut sup = Supervisor::nieuw(i);
    sup.laad(&opdracht(Apparaat::Cpu)).unwrap();
    let fout = sup.transcribeer(Path::new("x.wav"), KORT).unwrap_err();
    assert!(matches!(fout, EngineFout::Protocol(_)), "{fout:?}");
    assert!(
        stopbestand.exists(),
        "het proces is meteen gestopt, niet pas bij Drop"
    );
}

#[test]
fn ontbrekend_model_geeft_geen_terugval_naar_cpu() {
    // Op Cpu zou dit model wel laden: lukt het toch niet op Gpu, dan is er dus (onterecht)
    // teruggevallen. Een ontbrekend model is geen apparaatprobleem.
    let mut sup = Supervisor::nieuw(instellingen("ontbreekt_gpu"));
    let fout = sup.laad(&opdracht(Apparaat::Gpu)).unwrap_err();
    assert!(
        matches!(
            fout,
            EngineFout::Engine {
                soort: FoutSoort::ModelOntbreekt,
                ..
            }
        ),
        "{fout:?}"
    );
    assert!(!fout.kan_aan_apparaat_liggen());
}

#[test]
fn fout_van_de_engine_op_de_videokaart_valt_wel_terug_naar_cpu() {
    let mut sup = Supervisor::nieuw(instellingen("fout_laad_gpu"));
    assert_eq!(sup.laad(&opdracht(Apparaat::Gpu)).unwrap(), Apparaat::Cpu);
}

/// Een seconde geluid uit het geheugen (16 kHz).
fn een_seconde() -> Vec<f32> {
    vec![0.1; 16_000]
}

#[test]
fn geluid_uit_het_geheugen_geeft_een_resultaat() {
    let mut sup = Supervisor::nieuw(instellingen("ok"));
    sup.laad(&opdracht(Apparaat::Cpu)).unwrap();
    let uitkomst = sup.transcribeer_geluid(&een_seconde(), false).unwrap();
    assert_eq!(tekst(&uitkomst), "nep op Cpu");
}

#[test]
fn de_keuze_om_stilte_te_knippen_komt_bij_het_werkproces_aan() {
    let mut sup = Supervisor::nieuw(instellingen("ok"));
    sup.laad(&opdracht(Apparaat::Cpu)).unwrap();
    let zonder = sup.transcribeer_geluid(&een_seconde(), false).unwrap();
    assert_eq!(tekst(&zonder), "nep op Cpu");
    let met = sup.transcribeer_geluid(&een_seconde(), true).unwrap();
    assert_eq!(tekst(&met), "nep op Cpu met knippen");
}

#[test]
fn geen_spraak_na_knippen_geeft_een_resultaat_zonder_segmenten() {
    let mut sup = Supervisor::nieuw(instellingen("geen_spraak"));
    sup.laad(&opdracht(Apparaat::Cpu)).unwrap();
    let uitkomst = sup.transcribeer_geluid(&een_seconde(), true).unwrap();
    assert!(uitkomst.segmenten.is_empty());
    // Het filter in de app maakt er geen lege tekst van maar "geen tekst herkend".
    let transcript = mondig_core::aanbieder::Transcript {
        segmenten: uitkomst.segmenten,
        audio_duur: std::time::Duration::from_millis(uitkomst.duur_ms),
    };
    let gefilterd = mondig_core::filter::pas_toe(
        transcript,
        &uitkomst.meting,
        &mondig_core::filter::FilterInstellingen::standaard(),
    );
    assert_eq!(gefilterd.uitkomst, mondig_core::filter::Uitkomst::GeenTekst);
}

#[test]
fn geluid_zonder_model_geeft_niet_geladen() {
    let mut sup = Supervisor::nieuw(instellingen("ok"));
    assert!(matches!(
        sup.transcribeer_geluid(&een_seconde(), false),
        Err(EngineFout::NietGeladen)
    ));
}

#[test]
fn geluid_dat_crasht_op_de_videokaart_valt_terug_op_de_processor() {
    let mut sup = Supervisor::nieuw(instellingen("crash_transcribeer_gpu"));
    assert_eq!(sup.laad(&opdracht(Apparaat::Gpu)).unwrap(), Apparaat::Gpu);
    let uitkomst = sup.transcribeer_geluid(&een_seconde(), false).unwrap();
    assert_eq!(tekst(&uitkomst), "nep op Cpu");
    assert_eq!(sup.apparaat(), Some(Apparaat::Cpu));
}

#[test]
fn geluid_dat_crasht_op_de_processor_geeft_een_fout() {
    let mut sup = Supervisor::nieuw(instellingen("crash_transcribeer"));
    sup.laad(&opdracht(Apparaat::Cpu)).unwrap();
    let fout = sup.transcribeer_geluid(&een_seconde(), false).unwrap_err();
    assert!(matches!(fout, EngineFout::Gecrasht { .. }), "{fout:?}");
}

#[test]
fn geluid_dat_blijft_hangen_haalt_de_timeout() {
    let mut sup = Supervisor::nieuw(instellingen("hang_transcribeer"));
    sup.laad(&opdracht(Apparaat::Cpu)).unwrap();
    let fout = sup.transcribeer_geluid(&een_seconde(), false).unwrap_err();
    assert!(
        matches!(
            fout,
            EngineFout::TijdOverschreden {
                fase: Fase::Transcriberen,
                ..
            }
        ),
        "{fout:?}"
    );
}

#[test]
fn te_lang_dictaat_wordt_geweigerd_zonder_het_werkproces_te_raken() {
    // Het pad bestaat niet: werd het proces gestart, dan kregen we een startfout.
    let mut i = instellingen("ok");
    i.engine_pad = PathBuf::from("bestaat-echt-niet.exe");
    let te_lang = vec![0.0_f32; 600 * 16_000 + 1];

    let mut sup = Supervisor::nieuw(i.clone());
    let fout = sup.transcribeer_geluid(&te_lang, false).unwrap_err();
    assert!(matches!(fout, EngineFout::TeLang { .. }), "{fout:?}");
    assert!(!sup.draait());

    let model = model::zoek("whisper-large-v3-turbo-q8").unwrap();
    let mut engine = LokaleEngine::nieuw(i, &model, PathBuf::from("nep"), Apparaat::Cpu, None);
    let fout = engine.transcribeer(Invoer::Geluid(&te_lang)).unwrap_err();
    assert!(
        matches!(fout, AanbiederFout::Engine(EngineFout::TeLang { .. })),
        "{fout:?}"
    );
    assert!(!engine.draait());
}

#[test]
fn lokale_engine_schrijft_geluid_uit_het_geheugen_uit() {
    let mut engine = lokale_engine("ok");
    let transcript = engine.transcribeer(Invoer::Geluid(&een_seconde())).unwrap();
    assert_eq!(transcript.tekst(), "nep op Cpu");
}

/// Een lokale engine op dit apparaat die elke gemelde stand in een lijst zet.
#[allow(
    clippy::unwrap_used,
    reason = "testhulp: falen mag hier meteen paniek geven"
)]
fn engine_met_melder(
    modus: &str,
    apparaat: Apparaat,
) -> (LokaleEngine, Arc<Mutex<Vec<Rekenstand>>>) {
    let model = model::zoek("whisper-large-v3-turbo-q8").unwrap();
    let gemeld = Arc::new(Mutex::new(Vec::new()));
    let lijst = Arc::clone(&gemeld);
    let engine = LokaleEngine::nieuw(
        instellingen(modus),
        &model,
        PathBuf::from("nep"),
        apparaat,
        None,
    )
    .met_melder(move |stand| lijst.lock().unwrap().push(stand));
    (engine, gemeld)
}

#[test]
fn lokale_engine_meldt_de_videokaart_als_die_werkt() {
    let (mut engine, gemeld) = engine_met_melder("ok", Apparaat::Gpu);
    assert_eq!(engine.rekenstand(), None, "nog niets geladen");
    assert_eq!(engine.laad().unwrap(), Apparaat::Gpu);
    assert_eq!(engine.rekenstand(), Some(Rekenstand::Videokaart));
    // Nog een opdracht op hetzelfde apparaat meldt niets nieuws.
    engine.laad().unwrap();
    assert_eq!(*gemeld.lock().unwrap(), vec![Rekenstand::Videokaart]);
}

#[test]
fn lokale_engine_op_de_processor_meldt_de_processor() {
    let (mut engine, gemeld) = engine_met_melder("ok", Apparaat::Cpu);
    assert_eq!(engine.laad().unwrap(), Apparaat::Cpu);
    assert_eq!(*gemeld.lock().unwrap(), vec![Rekenstand::Processor]);
}

#[test]
fn lokale_engine_meldt_een_terugval_bij_laden() {
    let (mut engine, gemeld) = engine_met_melder("crash_laad_gpu", Apparaat::Gpu);
    assert_eq!(engine.laad().unwrap(), Apparaat::Cpu);
    assert_eq!(*gemeld.lock().unwrap(), vec![Rekenstand::Terugval]);
}

#[test]
fn na_een_terugval_blijft_de_lokale_engine_op_de_processor_ook_na_ontladen() {
    let map = tempfile::tempdir().unwrap();
    let wav = schrijf_wav(map.path(), false);
    // Laden op de videokaart lukt, uitschrijven crasht daar: de supervisor valt terug.
    let (mut engine, gemeld) = engine_met_melder("crash_transcribeer_gpu", Apparaat::Gpu);
    assert_eq!(engine.laad().unwrap(), Apparaat::Gpu);
    let transcript = engine.transcribeer(Invoer::Bestand(&wav)).unwrap();
    assert_eq!(transcript.tekst(), "nep op Cpu");
    assert_eq!(engine.rekenstand(), Some(Rekenstand::Terugval));

    // Ontladen (rusttijd) en opnieuw laden: niet weer de videokaart proberen.
    engine.stop();
    assert_eq!(engine.laad().unwrap(), Apparaat::Cpu);
    assert_eq!(engine.rekenstand(), Some(Rekenstand::Terugval));
    assert_eq!(
        *gemeld.lock().unwrap(),
        vec![Rekenstand::Videokaart, Rekenstand::Terugval]
    );
}

#[test]
fn het_werkproces_meldt_een_losse_videokaart_en_stopt_daarna() {
    let mut sup = Supervisor::nieuw(instellingen("ok"));
    let kaarten = sup.apparaten().unwrap();
    assert_eq!(
        kaarten,
        vec![Videokaart {
            naam: "Nep losse kaart".into(),
            soort: KaartSoort::Los,
        }]
    );
    assert!(!sup.draait(), "het zoekproces mag niet blijven draaien");
    // Daarna laadt een model gewoon in een nieuw proces.
    assert_eq!(sup.laad(&opdracht(Apparaat::Cpu)).unwrap(), Apparaat::Cpu);
}

#[test]
fn het_werkproces_meldt_een_ingebouwde_chip_of_geen_kaart() {
    let mut sup = Supervisor::nieuw(instellingen("ingebouwd"));
    assert_eq!(sup.apparaten().unwrap()[0].soort, KaartSoort::Ingebouwd);
    let mut sup = Supervisor::nieuw(instellingen("geen_kaart"));
    assert!(sup.apparaten().unwrap().is_empty());
}

#[test]
fn een_crash_bij_het_zoeken_is_een_gewone_fout() {
    let mut sup = Supervisor::nieuw(instellingen("crash_apparaten"));
    let fout = sup.apparaten().unwrap_err();
    assert!(matches!(fout, EngineFout::Gecrasht { .. }), "{fout:?}");
    assert!(!sup.draait());
}
