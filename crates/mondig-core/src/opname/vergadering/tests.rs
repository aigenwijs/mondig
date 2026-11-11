//! Tests van de vergaderopname met nepbronnen: geen geluidskaart nodig.

use std::sync::atomic::AtomicBool;
use std::thread::JoinHandle;

use super::*;
use crate::opname::Einde;
use crate::opname::stukken::lees_logboek;

/// Een nepbron die blokken van 30 ms levert zolang hij loopt, in echte tijd (versneld).
struct NepBron {
    /// Niveau van de samples (0 = stilte).
    niveau: f32,
    /// Geeft hij een opnametijd mee, zoals de systeembron?
    met_tijd: bool,
    /// Faalt `start`?
    start_faalt: bool,
    stoppen: Arc<AtomicBool>,
    draad: Option<JoinHandle<()>>,
}

impl NepBron {
    #[expect(
        clippy::unnecessary_box_returns,
        reason = "de opname neemt bronnen als Box"
    )]
    fn nieuw(niveau: f32, met_tijd: bool) -> Box<Self> {
        Box::new(Self {
            niveau,
            met_tijd,
            start_faalt: false,
            stoppen: Arc::new(AtomicBool::new(false)),
            draad: None,
        })
    }
}

impl Bron for NepBron {
    fn start(&mut self, mut uitgang: Box<dyn Uitgang>) -> Result<(), OpnameFout> {
        if self.start_faalt {
            return Err(OpnameFout::nieuw(FoutSoort::GeenAfspeelapparaat));
        }
        let stoppen = Arc::clone(&self.stoppen);
        let (niveau, met_tijd) = (self.niveau, self.met_tijd);
        self.draad = Some(std::thread::spawn(move || {
            let begin = Instant::now();
            let blok = vec![niveau; 480];
            let mut n = 0u32;
            while !stoppen.load(Ordering::SeqCst) {
                // Bij stilte levert de systeembron niets, net als Windows.
                if niveau > 0.0 || !met_tijd {
                    if met_tijd {
                        uitgang.ontvang_op(&blok, begin + Duration::from_millis(u64::from(n) * 30));
                    } else {
                        uitgang.ontvang(&blok);
                    }
                }
                n += 1;
                let volgende = begin + Duration::from_millis(u64::from(n) * 30);
                std::thread::sleep(volgende.saturating_duration_since(Instant::now()));
            }
        }));
        Ok(())
    }

    fn stop(&mut self) -> Result<Einde, OpnameFout> {
        self.stoppen.store(true, Ordering::SeqCst);
        if let Some(draad) = self.draad.take() {
            draad.join().unwrap();
        }
        Ok(Einde {
            uitgevallen: None,
            verloren: 0,
        })
    }
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[test]
fn twee_sporen_in_stukken_met_een_gedeelde_tijdlijn() {
    let basis = tempfile::tempdir().unwrap();
    let opname = Vergaderopname::start_met(
        basis.path(),
        NepBron::nieuw(0.2, false),
        Some(NepBron::nieuw(0.3, true)),
        ms(500),
    )
    .unwrap();
    let map = opname.map().to_path_buf();
    assert!(loopt(&map), "het slot hoort vast te zitten");
    assert!(
        onafgemaakt(basis.path()).is_empty(),
        "een lopende opname is niet onaf"
    );
    std::thread::sleep(ms(1_600));
    let uitslag = opname.stop().unwrap();
    assert!(!loopt(&map));

    let mic = &uitslag.microfoon;
    let sys = uitslag.systeem.as_ref().unwrap();
    // Ongeveer 1,6 s: drie à vier stukken van 0,5 s (een vertraagde runner mag er een extra
    // opleveren).
    assert!((3..=5).contains(&mic.uitslag.stukken), "{mic:?}");
    assert!(!mic.uitslag.stil());
    assert!(uitslag.melding_stil_systeem().is_none());
    // Het systeemspoor is aangevuld tot het einde van de opname. De nepbron kan nog blokken
    // leveren tussen het vastleggen van het einde en het stoppen (onder belasting een tijdje),
    // dus het spoor is niet korter dan de opname en hooguit een seconde langer.
    let lengte = samples_in_ms(uitslag.duur.as_millis());
    assert!(
        sys.uitslag.samples + 160 >= lengte && sys.uitslag.samples <= lengte + samples_in_ms(1_000),
        "{} {lengte}",
        sys.uitslag.samples
    );

    // De ankers van de stukken liggen op de tijdlijn. Waar de nepbron begint hangt af van de
    // planner (onder belasting kan zijn thread later starten), dus toetsen we de afstand
    // tussen de ankers: stukken van 500 ms liggen exact 500 ms uit elkaar.
    let ankers: Vec<i64> = lees_logboek(&map.join(SYSTEEM))
        .unwrap()
        .iter()
        .map(|s| s.begin_ms)
        .collect();
    assert!(ankers.len() >= 3, "{ankers:?}");
    assert!((0..1_000).contains(&ankers[0]), "{ankers:?}");
    for paar in ankers.windows(2) {
        assert!((paar[1] - paar[0] - 500).abs() <= 2, "{ankers:?}");
    }
    // De microfoon begint kort na het nulpunt (de nepbron levert meteen).
    let mic_ankers = lees_logboek(&map.join(MICROFOON)).unwrap();
    assert!(
        (-50..1_000).contains(&mic_ankers[0].begin_ms),
        "{mic_ankers:?}"
    );

    let afsluiting: Afsluiting = lees_json(&map.join(EINDE)).unwrap();
    assert!(!afsluiting.hersteld);
    assert_eq!(afsluiting.microfoon.stukken, mic.uitslag.stukken);
    let opzet = opzet(&map).unwrap();
    assert!(opzet.systeemgeluid);
    assert_eq!(opzet.samplerate, 16_000);
}

fn samples_in_ms(ms: u128) -> u64 {
    u64::try_from(ms * u128::from(DOEL_SAMPLERATE) / 1000).unwrap()
}

#[test]
fn stil_systeemspoor_geeft_een_melding_en_geen_fout() {
    let basis = tempfile::tempdir().unwrap();
    let opname = Vergaderopname::start_met(
        basis.path(),
        NepBron::nieuw(0.2, false),
        Some(NepBron::nieuw(0.0, true)),
        ms(500),
    )
    .unwrap();
    std::thread::sleep(ms(700));
    let uitslag = opname.stop().unwrap();
    let sys = uitslag.systeem.as_ref().unwrap();
    assert!(sys.uitslag.stil());
    assert_eq!(sys.uitslag.opgevuld, sys.uitslag.samples);
    assert!(sys.uitslag.samples > 10_000);
    assert!(uitslag.melding_stil_systeem().is_some());
}

#[test]
fn zonder_systeemgeluid_alleen_de_microfoon() {
    let basis = tempfile::tempdir().unwrap();
    let opname =
        Vergaderopname::start_met(basis.path(), NepBron::nieuw(0.2, false), None, ms(500)).unwrap();
    std::thread::sleep(ms(200));
    let uitslag = opname.stop().unwrap();
    assert!(uitslag.systeem.is_none());
    assert!(!uitslag.map.join(SYSTEEM).exists());
    assert!(!opzet(&uitslag.map).unwrap().systeemgeluid);
}

#[test]
fn mislukte_start_laat_niets_achter() {
    let basis = tempfile::tempdir().unwrap();
    let mut systeem = NepBron::nieuw(0.2, true);
    systeem.start_faalt = true;
    let fout = Vergaderopname::start_met(
        basis.path(),
        NepBron::nieuw(0.2, false),
        Some(systeem),
        ms(500),
    )
    .err()
    .unwrap();
    assert!(fout.melding.contains("luidspreker"), "{}", fout.melding);
    assert_eq!(fs::read_dir(basis.path()).unwrap().count(), 0);
}

#[test]
fn weggooien_zonder_stop_sluit_netjes_af() {
    let basis = tempfile::tempdir().unwrap();
    let opname = Vergaderopname::start_met(
        basis.path(),
        NepBron::nieuw(0.2, false),
        Some(NepBron::nieuw(0.2, true)),
        ms(500),
    )
    .unwrap();
    let map = opname.map().to_path_buf();
    std::thread::sleep(ms(200));
    drop(opname);
    assert!(map.join(EINDE).exists());
    assert!(onafgemaakt(basis.path()).is_empty());
}

/// Bootst een afgebroken opname na zoals een harde stop die achterlaat: kop, logboek met
/// afgesloten stukken, een laatste stuk met een kop die achterloopt, en geen `einde.json`.
fn afgebroken_opname(basis: &Path) -> PathBuf {
    let opname = Vergaderopname::start_met(
        basis,
        NepBron::nieuw(0.2, false),
        Some(NepBron::nieuw(0.2, true)),
        ms(500),
    )
    .unwrap();
    let map = opname.map().to_path_buf();
    std::thread::sleep(ms(1_200));
    drop(opname);
    fs::remove_file(map.join(EINDE)).unwrap();
    for spoor in [MICROFOON, SYSTEEM] {
        let spoormap = map.join(spoor);
        let mut logboek = lees_logboek(&spoormap).unwrap();
        let laatste = logboek.pop().unwrap();
        // Het laatste stuk staat niet in het logboek en de kop zegt dat het leeg is.
        let regels: String = logboek
            .iter()
            .map(|s| serde_json::to_string(s).unwrap() + "\n")
            .collect();
        fs::write(spoormap.join(stukken::LOGBOEK), regels).unwrap();
        let pad = spoormap.join(&laatste.bestand);
        let mut bytes = fs::read(&pad).unwrap();
        bytes[4..8].copy_from_slice(&36u32.to_le_bytes());
        bytes[40..44].copy_from_slice(&0u32.to_le_bytes());
        fs::write(&pad, bytes).unwrap();
    }
    map
}

#[test]
fn afgebroken_opname_wordt_gevonden_en_hersteld() {
    let basis = tempfile::tempdir().unwrap();
    let map = afgebroken_opname(basis.path());
    assert_eq!(onafgemaakt(basis.path()), vec![map.clone()]);

    let afsluiting = herstel(&map).unwrap();
    assert!(afsluiting.hersteld);
    let systeem = afsluiting.systeem.unwrap();
    assert!(systeem.stukken >= 2);
    assert!(!systeem.stil);
    // Alles is terug: ongeveer 1,2 s per spoor.
    assert!(
        afsluiting.microfoon.samples > 16_000,
        "{:?}",
        afsluiting.microfoon
    );
    assert!(afsluiting.duur_ms >= 1_100, "{}", afsluiting.duur_ms);
    for spoor in [MICROFOON, SYSTEEM] {
        let logboek = lees_logboek(&map.join(spoor)).unwrap();
        let laatste = logboek.last().unwrap();
        assert!(laatste.hersteld);
        let lezer = hound::WavReader::open(map.join(spoor).join(&laatste.bestand)).unwrap();
        assert_eq!(u64::from(lezer.len()), laatste.samples);
    }
    assert!(onafgemaakt(basis.path()).is_empty());
    // Opnieuw herstellen geeft hetzelfde.
    assert_eq!(herstel(&map).unwrap().duur_ms, afsluiting.duur_ms);
}

#[test]
fn wissen_haalt_alle_audio_weg_en_weigert_vreemde_mappen() {
    let basis = tempfile::tempdir().unwrap();
    let map = afgebroken_opname(basis.path());
    let vreemd = basis.path().join("iets-anders");
    fs::create_dir(&vreemd).unwrap();
    assert!(wis(&vreemd).is_err());
    assert!(vreemd.exists());
    wis(&map).unwrap();
    assert!(!map.exists());
}

#[test]
fn een_lopende_opname_is_niet_te_wissen_of_te_herstellen() {
    let basis = tempfile::tempdir().unwrap();
    let opname =
        Vergaderopname::start_met(basis.path(), NepBron::nieuw(0.2, false), None, ms(500)).unwrap();
    let map = opname.map().to_path_buf();
    assert!(wis(&map).is_err());
    assert!(herstel(&map).is_err());
    opname.stop().unwrap();
    wis(&map).unwrap();
}

#[test]
fn alleen_een_ontbrekend_slot_betekent_dat_een_opname_niet_loopt() {
    let basis = tempfile::tempdir().unwrap();
    let map = afgebroken_opname(basis.path());
    assert!(!loopt(&map), "zonder vergrendeling loopt ze niet");
    // Het slot weg: er loopt niets.
    fs::remove_file(map.join(SLOT)).unwrap();
    assert!(!loopt(&map));
    // Het slot is er maar niet te openen (hier: een map op die plek): Mondig kan niet
    // nagaan of de opname nog loopt. Dat is niet "loopt" en niet "loopt niet".
    fs::create_dir(map.join(SLOT)).unwrap();
    assert!(!loopt(&map));
    assert_eq!(slotstand(&map), Slotstand::Onbekend);
    // Hij wordt getoond met die status, maar telt niet als zeker afgebroken.
    assert!(onafgemaakt(basis.path()).is_empty());
    assert_eq!(
        onafgemaakt_met_stand(basis.path()),
        vec![Onafgemaakt {
            map: map.clone(),
            stand: Slotstand::Onbekend
        }]
    );
    // Herstellen en wissen weigeren met een melding in gewone taal ...
    let fout = herstel(&map).unwrap_err();
    assert!(fout.melding.contains("niet nagaan"), "{}", fout.melding);
    assert!(!map.join(EINDE).exists());
    let fout = wis(&map).unwrap_err();
    assert!(fout.melding.contains("niet nagaan"), "{}", fout.melding);
    assert!(map.exists());
    // ... behalve met een expliciete keuze om toch te wissen.
    wis_toch(&map).unwrap();
    assert!(!map.exists());
}

#[test]
fn mislukt_het_slot_dan_blijft_er_geen_lege_map_achter() {
    let basis = tempfile::tempdir().unwrap();
    let fout = maak_map_met(basis.path(), &|_| Err(io::Error::other("geen slot"))).unwrap_err();
    assert_eq!(fout.kind(), io::ErrorKind::Other);
    assert_eq!(fs::read_dir(basis.path()).unwrap().count(), 0);
}

#[test]
fn het_nulpunt_ligt_na_het_aanmaken_en_voor_het_starten_van_de_bronnen() {
    let basis = tempfile::tempdir().unwrap();
    let genomen = std::cell::Cell::new(false);
    // Een klok die bij elke aanroep vastlegt wat er dan al op schijf staat.
    let klok = || {
        let mappen: Vec<PathBuf> = fs::read_dir(basis.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(mappen.len(), 1);
        // De voorlopige kop is al geschreven (dus een trage schijf zit er al op) ...
        assert!(mappen[0].join(KOP).is_file());
        // ... en de bronnen zijn nog niet gestart: er is nog geen spoor aangemaakt.
        assert!(!mappen[0].join(MICROFOON).exists());
        assert!(!mappen[0].join(SYSTEEM).exists());
        genomen.set(true);
        4_242_424
    };
    let opname = Vergaderopname::start_met_klok(
        basis.path(),
        Vergadersoort::Online,
        NepBron::nieuw(0.2, false),
        Some(NepBron::nieuw(0.2, true)),
        ms(500),
        &klok,
    )
    .unwrap();
    assert!(genomen.get());
    // Het nulpunt uit de klok staat in opname.json, na het starten weggeschreven.
    assert_eq!(opzet(opname.map()).unwrap().begin_unix_ms, 4_242_424);
    opname.stop().unwrap();
}

#[test]
fn stil_systeemspoor_dat_daarna_uitviel_houdt_de_hint_over_een_ander_apparaat() {
    fn spoor(piek: f32, onderbroken: Option<Foutmelding>) -> Spoor {
        Spoor {
            uitslag: SpoorUitslag {
                map: PathBuf::new(),
                stukken: 1,
                samples: 16_000,
                opgevuld: 0,
                piek,
            },
            verloren: 0,
            onderbroken,
        }
    }
    let uitslag_met = |piek: f32| Vergaderuitslag {
        map: PathBuf::new(),
        duur: ms(1_000),
        microfoon: spoor(0.5, None),
        systeem: Some(spoor(
            piek,
            Some(Foutmelding::from(OpnameFout::nieuw(
                FoutSoort::AfspeelapparaatWeg,
            ))),
        )),
    };
    assert!(uitslag_met(0.0).melding_stil_systeem().is_some());
    assert!(uitslag_met(0.5).melding_stil_systeem().is_none());
}

#[test]
fn de_soort_staat_in_de_opzet_en_een_oude_opzet_is_online() {
    let basis = tempfile::tempdir().unwrap();
    // In de ruimte: een systeembron die toch meekomt, wordt niet opgenomen.
    let opname = Vergaderopname::start_soort_met(
        basis.path(),
        Vergadersoort::Ruimte,
        NepBron::nieuw(0.2, false),
        Some(NepBron::nieuw(0.2, true)),
        ms(500),
    )
    .unwrap();
    let map = opname.map().to_path_buf();
    assert!(opname.systeem_stil_sinds().is_none());
    assert_eq!(opname.niveaus().systeem, None);
    let uitslag = opname.stop().unwrap();
    assert!(uitslag.systeem.is_none());
    let o = opzet(&map).unwrap();
    assert_eq!(o.soort, Vergadersoort::Ruimte);
    assert!(!o.systeemgeluid);
    assert_eq!(crate::spoor::lees_soort(&map), Vergadersoort::Ruimte);

    // Een opname.json van voor de soort: online.
    let tekst = fs::read_to_string(map.join(KOP)).unwrap();
    let oud = tekst.replace("\"soort\": \"ruimte\"", "\"x\": 0");
    assert_ne!(oud, tekst, "het veld stond erin");
    fs::write(map.join(KOP), oud).unwrap();
    assert_eq!(opzet(&map).unwrap().soort, Vergadersoort::Online);
    assert_eq!(crate::spoor::lees_soort(&map), Vergadersoort::Online);
    assert_eq!(
        crate::spoor::lees_soort(basis.path()),
        Vergadersoort::Online
    );
}

#[test]
fn alleen_ik_staat_in_de_opzet_zonder_systeemgeluid() {
    let basis = tempfile::tempdir().unwrap();
    // Ook bij Alleen ik komt een systeembron die toch meekomt niet in de opname.
    let opname = Vergaderopname::start_soort_met(
        basis.path(),
        Vergadersoort::Alleen,
        NepBron::nieuw(0.2, false),
        Some(NepBron::nieuw(0.2, true)),
        ms(500),
    )
    .unwrap();
    let map = opname.map().to_path_buf();
    assert_eq!(opname.niveaus().systeem, None);
    let uitslag = opname.stop().unwrap();
    assert!(uitslag.systeem.is_none());
    let tekst = fs::read_to_string(map.join(KOP)).unwrap();
    assert!(tekst.contains("\"soort\": \"alleen\""), "{tekst}");
    let o = opzet(&map).unwrap();
    assert_eq!(o.soort, Vergadersoort::Alleen);
    assert!(!o.systeemgeluid);
    assert_eq!(crate::spoor::lees_soort(&map), Vergadersoort::Alleen);
}

#[test]
fn niveaus_per_spoor_en_hoe_lang_het_systeem_al_stil_is() {
    let basis = tempfile::tempdir().unwrap();
    // Het systeem speelt niets af: de nepbron levert dan niets, net als Windows.
    let opname = Vergaderopname::start_met(
        basis.path(),
        NepBron::nieuw(0.5, false),
        Some(NepBron::nieuw(0.0, true)),
        ms(500),
    )
    .unwrap();
    std::thread::sleep(ms(300));
    let niveaus = opname.niveaus();
    // -6 dBFS op een schaal van -60 tot 0: ongeveer 0,9.
    assert!(
        niveaus.microfoon > 0.85 && niveaus.microfoon < 0.95,
        "{niveaus:?}"
    );
    assert_eq!(niveaus.systeem, Some(0.0));
    // De piek gaat na het lezen terug naar nul; er komt daarna weer geluid binnen.
    std::thread::sleep(ms(100));
    assert!(opname.niveaus().microfoon > 0.85);
    let stil = opname.systeem_stil_sinds().unwrap();
    assert!(stil >= ms(300), "{stil:?}");
    assert!(stil < SYSTEEM_STIL_NA);
    opname.stop().unwrap();

    // Met geluid van de computer is het spoor niet stil.
    let opname = Vergaderopname::start_met(
        basis.path(),
        NepBron::nieuw(0.5, false),
        Some(NepBron::nieuw(0.2, true)),
        ms(500),
    )
    .unwrap();
    std::thread::sleep(ms(300));
    assert!(opname.systeem_stil_sinds().unwrap() < ms(200));
    assert!(opname.niveaus().systeem.unwrap() > 0.5);
    opname.stop().unwrap();
}

#[test]
fn een_wis_die_halverwege_faalt_is_te_herhalen() {
    let basis = tempfile::tempdir().unwrap();
    // Een restmap: opname.json is al weg en de map is leeg.
    let rest = basis.path().join("vergadering-123");
    fs::create_dir(&rest).unwrap();
    wis(&rest).unwrap();
    assert!(!rest.exists());
    // Een lege map met een andere naam blijft geweigerd.
    let vreemd = basis.path().join("iets-anders");
    fs::create_dir(&vreemd).unwrap();
    assert!(wis(&vreemd).is_err());
}

#[cfg(windows)]
#[test]
fn faalt_een_stuk_dan_blijven_de_opzet_en_het_einde_staan() {
    let basis = tempfile::tempdir().unwrap();
    let map = afgebroken_opname(basis.path());
    herstel(&map).unwrap();
    // Een stuk dat open staat zonder recht om te verwijderen (share mode 0): Windows
    // weigert het te wissen, zoals bij een werkproces dat het nog leest.
    let stuk = fs::read_dir(map.join(MICROFOON))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|x| x == "wav"))
        .unwrap();
    let open = {
        use std::os::windows::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&stuk)
            .unwrap()
    };
    assert!(wis(&map).is_err());
    assert!(map.join(KOP).is_file(), "de opzet staat er nog");
    assert!(map.join(EINDE).is_file(), "het einde staat er nog");
    // Na het sluiten lukt het opnieuw proberen.
    drop(open);
    wis(&map).unwrap();
    assert!(!map.exists());
}

#[test]
fn een_vergrendeld_slot_houdt_het_opruimen_tegen_en_een_tweede_vergrendeling_weg() {
    let basis = tempfile::tempdir().unwrap();
    let map = afgebroken_opname(basis.path());
    let nu = nu_unix_ms() + 400 * 86_400_000;
    let slot_bestand = vergrendel(&map).unwrap();
    // Zolang het werk loopt (uitschrijven vanaf de opdrachtregel) gebeurt er niets mee.
    let fout = vergrendel(&map).unwrap_err();
    assert!(fout.melding.contains("in gebruik"), "{}", fout.melding);
    let uitslag = wis_verlopen(basis.path(), bewaartijd(7), nu, &[]);
    assert!(!uitslag.veranderd());
    assert!(wis(&map).is_err());
    drop(slot_bestand);
    // Daarna is ze weer gewoon een opname die niet loopt: eerst markeren.
    let uitslag = wis_verlopen(basis.path(), bewaartijd(7), nu, &[]);
    assert_eq!(uitslag.gemarkeerd, vec![map]);
}

#[cfg(windows)]
#[test]
fn een_slot_dat_een_map_is_laat_zich_niet_vergrendelen() {
    let basis = tempfile::tempdir().unwrap();
    let map = afgebroken_opname(basis.path());
    fs::remove_file(map.join(SLOT)).unwrap();
    fs::create_dir(map.join(SLOT)).unwrap();
    assert!(vergrendel(&map).is_err());
}

#[test]
fn een_map_met_verslag_verloopt_en_wordt_gewist_zoals_een_andere() {
    use crate::gesprek::{Gesprek, Spoorstand};
    use crate::persoonsgegevens::Markeringen;
    use crate::verslag::{self, Bewaren};

    let basis = tempfile::tempdir().unwrap();
    let map = afgebroken_opname(basis.path());
    herstel(&map).unwrap();
    let gesprek = Gesprek {
        beurten: Vec::new(),
        duur: Duration::from_secs(1),
        meldingen: Vec::new(),
        jij: Spoorstand::Opgenomen,
        anderen: Spoorstand::Ontbrak,
        weggelaten: 0,
        soort: Vergadersoort::Online,
    };
    verslag::schrijf(
        &map,
        &Bewaren::nieuw(
            &gesprek,
            "m",
            "M",
            &verslag::Naamvelden::default(),
            &Markeringen::default(),
            None,
        ),
    )
    .unwrap();
    assert!(verslag::lees(&map).is_some());
    // Binnen de termijn blijft alles staan, verslag en geluid.
    let nu = nu_unix_ms();
    assert!(!wis_verlopen(basis.path(), bewaartijd(7), nu, &[]).veranderd());
    assert!(map.join(verslag::BESTAND).is_file());
    // Over de termijn: eerst markeren (het verslag blijft), een dag later gaat de hele map weg.
    let later = nu + 8 * 86_400_000;
    let uitslag = wis_verlopen(basis.path(), bewaartijd(7), later, &[]);
    assert_eq!(uitslag.gemarkeerd, vec![map.clone()]);
    assert!(map.join(verslag::BESTAND).is_file());
    let uitslag = wis_verlopen(basis.path(), bewaartijd(7), later + 86_400_000, &[]);
    assert_eq!(uitslag.gewist, vec![map.clone()]);
    assert!(!map.exists(), "de hele map is weg, ook het verslag");
}
