//! Verlopen opnames in de tijdelijke map automatisch wissen (ADR 0008, aanvulling 4b-C).
//!
//! De audio is er voor het verslag. Wat na "Verslag sluiten" in de map blijft staan (later
//! uitschrijven, de app afgesloten met een open verslag, een crash), gaat na de bewaartermijn
//! vanzelf weg. De tijd rekent vanaf het begin van de opname (`begin_unix_ms`), of bij een
//! restmap zonder bruikbare `opname.json` vanaf de wijzigingstijd van de map.
//!
//! Eerst markeren, dan pas wissen: een opname die over haar termijn is, krijgt een
//! markering (`verloopt.json`) met het tijdstip van markeren. Pas als die minstens
//! [`WACHT_NA_MARKERING`] oud is en de opname nog steeds verlopen is, gaat ze weg. Zo ziet de
//! gebruiker de melding altijd een dag vooraf, ook na een kortere termijn, na een week zonder
//! de app, of als de klok vooruit springt. Een markering in de toekomst (klok teruggezet) of
//! van een opname die niet meer verlopen is (termijn verlengd) wordt weggehaald.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::{KOP, Slotstand, lees_json, niet_weg, opzet, schrijf_json, slotstand, wis_met};
use crate::taak::Foutmelding;

/// De keuzes voor de bewaartermijn, in dagen.
pub const BEWAAR_KEUZES_DAGEN: [u32; 3] = [1, 7, 30];

/// De standaard bewaartermijn, in dagen.
pub const BEWAAR_STANDAARD_DAGEN: u32 = 7;

/// De markering in de map van een opname die gaat verlopen.
const MARKERING: &str = "verloopt.json";

/// Zo lang na het markeren blijft de opname nog staan.
pub const WACHT_NA_MARKERING: Duration = Duration::from_hours(24);

/// De bewaartermijn voor een aantal dagen.
#[must_use]
pub fn bewaartijd(dagen: u32) -> Duration {
    Duration::from_secs(u64::from(dagen) * 24 * 3600)
}

/// Wat het opruimen deed.
#[derive(Debug, Default)]
pub struct Verlopen {
    /// De mappen die zijn gewist.
    pub gewist: Vec<PathBuf>,
    /// De mappen die weg moesten maar niet lukten, met de reden.
    pub mislukt: Vec<(PathBuf, Foutmelding)>,
    /// De mappen die nu een markering kregen: ze worden na [`WACHT_NA_MARKERING`] gewist.
    pub gemarkeerd: Vec<PathBuf>,
    /// De mappen waarvan een markering is weggehaald (termijn verlengd, of klok teruggezet).
    pub ontmarkeerd: Vec<PathBuf>,
}

impl Verlopen {
    /// Is er iets veranderd dat de lijst met opnames raakt (ook de wistijd)?
    #[must_use]
    pub fn veranderd(&self) -> bool {
        !(self.gewist.is_empty() && self.gemarkeerd.is_empty() && self.ontmarkeerd.is_empty())
    }
}

/// Wat in `verloopt.json` staat.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Markering {
    /// Het moment van markeren (milliseconden sinds 1970).
    gemarkeerd_unix_ms: u64,
    /// Het begin van de opname, voor een restmap zonder `opname.json`: markeren verandert
    /// de wijzigingstijd van de map, die anders het begin zou verschuiven.
    begin_unix_ms: u64,
}

/// Een map die bij de opnames kan horen: de naam begint met `vergadering-`.
fn is_opnamenaam(map: &Path) -> bool {
    map.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("vergadering-"))
}

fn ms(duur: Duration) -> u64 {
    u64::try_from(duur.as_millis()).unwrap_or(u64::MAX)
}

fn lees_markering(map: &Path) -> Option<Markering> {
    lees_json(&map.join(MARKERING)).ok()
}

/// Het begin van een opname: uit `opname.json`, anders uit de markering, anders de
/// wijzigingstijd van de map.
fn begin_ms(map: &Path) -> Option<u64> {
    if let Ok(opzet) = opzet(map) {
        return Some(opzet.begin_unix_ms);
    }
    if let Some(markering) = lees_markering(map) {
        return Some(markering.begin_unix_ms);
    }
    let gewijzigd = fs::metadata(map).ok()?.modified().ok()?;
    gewijzigd.duration_since(UNIX_EPOCH).ok().map(ms)
}

/// De markering die nu telt: een markering in de toekomst (klok teruggezet) telt niet.
fn geldige_markering(map: &Path, nu_ms: u64) -> Option<Markering> {
    lees_markering(map).filter(|m| m.gemarkeerd_unix_ms <= nu_ms)
}

/// Het moment (milliseconden sinds 1970) waarop deze opname wordt gewist, of `None` als de
/// map niet bij de opnames hoort of haar tijd niet te lezen is. Een gemarkeerde opname gaat
/// [`WACHT_NA_MARKERING`] na het markeren weg; een opname die nog moet verlopen of nog
/// gemarkeerd moet worden, een dag na het moment waarop dat gebeurt.
#[must_use]
pub fn wistijd_ms(map: &Path, bewaartijd: Duration, nu_ms: u64) -> Option<u64> {
    if !is_opnamenaam(map) {
        return None;
    }
    let verloopt = begin_ms(map)?.saturating_add(ms(bewaartijd));
    let wacht = ms(WACHT_NA_MARKERING);
    match geldige_markering(map, nu_ms) {
        Some(m) if verloopt <= nu_ms => Some(m.gemarkeerd_unix_ms.saturating_add(wacht)),
        _ => Some(verloopt.max(nu_ms).saturating_add(wacht)),
    }
}

/// Wist de opnames onder `basis` die verlopen zijn: ouder dan `bewaartijd` (gerekend vanaf hun
/// begin tot `nu_ms`, milliseconden sinds 1970) en minstens [`WACHT_NA_MARKERING`] geleden
/// gemarkeerd. Een verlopen opname zonder markering krijgt er een en blijft staan. Een opname
/// die loopt (of waarvan het slot niet te lezen is), een opname in `in_gebruik` (de app werkt
/// er nu mee) en een link naar een map blijven ongemoeid.
///
/// De aanroeper moet de opnames in gebruik vasthouden tot deze functie klaar is: anders kan
/// een opname tussen de controle en het wissen in gebruik raken.
#[must_use]
pub fn wis_verlopen(
    basis: &Path,
    bewaartijd: Duration,
    nu_ms: u64,
    in_gebruik: &[PathBuf],
) -> Verlopen {
    let mut uitslag = Verlopen::default();
    let Ok(lijst) = fs::read_dir(basis) else {
        return uitslag;
    };
    // `DirEntry::file_type` volgt geen links: een symlink of junction is hier geen map.
    let mut mappen: Vec<PathBuf> = lijst
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    mappen.sort();
    for map in mappen {
        if !is_opnamenaam(&map) {
            continue;
        }
        let gebruikt = in_gebruik
            .iter()
            .any(|g| g.file_name().is_some() && g.file_name() == map.file_name());
        if gebruikt || slotstand(&map) != Slotstand::LoopNiet {
            continue;
        }
        let Some(begin) = begin_ms(&map) else {
            continue;
        };
        let markering = geldige_markering(&map, nu_ms);
        let verlopen = begin.saturating_add(ms(bewaartijd)) <= nu_ms;
        if !verlopen {
            // Niet (meer) verlopen: een markering hoort hier niet, ook niet een uit de toekomst.
            let pad = map.join(MARKERING);
            if pad.exists() && fs::remove_file(&pad).is_ok() {
                uitslag.ontmarkeerd.push(map);
            }
            continue;
        }
        let Some(markering) = markering else {
            // Geen (bruikbare) markering: markeer nu, wis pas over een dag.
            let nieuw = Markering {
                gemarkeerd_unix_ms: nu_ms,
                begin_unix_ms: begin,
            };
            if schrijf_json(&map.join(MARKERING), &nieuw).is_ok() {
                uitslag.gemarkeerd.push(map);
            }
            continue;
        };
        if nu_ms.saturating_sub(markering.gemarkeerd_unix_ms) < ms(WACHT_NA_MARKERING) {
            continue;
        }
        let resultaat = if opzet(&map).is_ok() {
            debug_assert!(map.join(KOP).is_file());
            wis_met(&map, false)
        } else {
            // Een restmap zonder bruikbare `opname.json`: de naam en de leeftijd zijn de
            // enige aanwijzing. Het slot is hierboven al gecontroleerd.
            fs::remove_dir_all(&map).map_err(|_| niet_weg())
        };
        match resultaat {
            Ok(()) => uitslag.gewist.push(map),
            Err(fout) => uitslag.mislukt.push((map, fout)),
        }
    }
    uitslag
}

#[cfg(test)]
mod tests {
    use super::super::nu_unix_ms as nu_ms;
    use super::*;
    use crate::spoor::EINDE;

    const DAG_MS: u64 = 86_400_000;

    fn schrijf(basis: &Path, naam: &str, begin: u64) -> PathBuf {
        let map = basis.join(naam);
        fs::create_dir_all(map.join("microfoon")).unwrap();
        fs::write(
            map.join(KOP),
            format!(
                "{{\"versie\":1,\"begin_unix_ms\":{begin},\"samplerate\":16000,\"stuk_seconden\":60,\"systeemgeluid\":false}}"
            ),
        )
        .unwrap();
        fs::write(map.join("microfoon").join("00000.wav"), [0u8; 64]).unwrap();
        map
    }

    /// Markeert en wacht dan een dag en een uur: de opname is nu echt te wissen.
    fn markeer_en_wacht(basis: &Path, nu: u64) -> (Verlopen, u64) {
        let eerste = wis_verlopen(basis, bewaartijd(7), nu, &[]);
        assert!(eerste.gewist.is_empty(), "eerst markeren, nog niet wissen");
        (eerste, nu + DAG_MS + 3_600_000)
    }

    #[test]
    fn de_wistijd_volgt_het_markeren() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG_MS;
        // Nog niet verlopen: verloopt over 2 dagen, dus wissen een dag later.
        let jong = schrijf(basis.path(), "vergadering-1", nu - 5 * DAG_MS);
        assert_eq!(
            wistijd_ms(&jong, bewaartijd(7), nu),
            Some(nu + 2 * DAG_MS + DAG_MS)
        );
        // Wel verlopen maar nog niet gemarkeerd (de app stond uit): over een dag.
        let oud = schrijf(basis.path(), "vergadering-2", nu - 20 * DAG_MS);
        assert_eq!(wistijd_ms(&oud, bewaartijd(7), nu), Some(nu + DAG_MS));
        // Gemarkeerd: het markeertijdstip plus een dag.
        let _ = wis_verlopen(basis.path(), bewaartijd(7), nu, &[]);
        assert_eq!(
            wistijd_ms(&oud, bewaartijd(7), nu + 3_600_000),
            Some(nu + DAG_MS)
        );
        let vreemd = schrijf(basis.path(), "iets-anders", 0);
        assert_eq!(wistijd_ms(&vreemd, bewaartijd(7), nu), None);
    }

    #[test]
    fn eerst_markeren_en_een_dag_later_wissen() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG_MS;
        let oud = schrijf(basis.path(), "vergadering-oud", nu - 8 * DAG_MS);
        let jong = schrijf(basis.path(), "vergadering-jong", nu - 6 * DAG_MS);
        let vreemd = schrijf(basis.path(), "iets-anders", 0);
        fs::write(oud.join(EINDE), "{}").unwrap();

        let (eerste, later) = markeer_en_wacht(basis.path(), nu);
        assert_eq!(eerste.gemarkeerd, vec![oud.clone()]);
        assert!(oud.join(MARKERING).is_file());
        assert!(!jong.join(MARKERING).exists());
        // Een uur later is het nog geen dag.
        let tussen = wis_verlopen(basis.path(), bewaartijd(7), nu + 3_600_000, &[]);
        assert!(!tussen.veranderd());
        assert!(oud.exists());
        // Een dag en een uur later gaat ze weg, de jonge en de vreemde map blijven.
        let tweede = wis_verlopen(basis.path(), bewaartijd(7), later, &[]);
        assert_eq!(tweede.gewist, vec![oud.clone()]);
        assert!(tweede.mislukt.is_empty());
        assert!(!oud.exists());
        assert!(jong.exists());
        assert!(vreemd.exists(), "een vreemde map blijft altijd staan");
    }

    #[test]
    fn een_verlengde_termijn_haalt_de_markering_weg() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG_MS;
        let map = schrijf(basis.path(), "vergadering-1", nu - 8 * DAG_MS);
        let (_, later) = markeer_en_wacht(basis.path(), nu);
        assert!(map.join(MARKERING).is_file());
        // De termijn gaat naar 30 dagen: de opname is niet meer verlopen en blijft staan.
        let uitslag = wis_verlopen(basis.path(), bewaartijd(30), later, &[]);
        assert_eq!(uitslag.ontmarkeerd, vec![map.clone()]);
        assert!(uitslag.gewist.is_empty());
        assert!(!map.join(MARKERING).exists());
        // Weer korter: opnieuw eerst markeren, niet meteen wissen.
        let weer = wis_verlopen(basis.path(), bewaartijd(7), later, &[]);
        assert_eq!(weer.gemarkeerd, vec![map.clone()]);
        assert!(map.exists());
    }

    #[test]
    fn een_markering_in_de_toekomst_telt_niet() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG_MS;
        let map = schrijf(basis.path(), "vergadering-1", nu - 8 * DAG_MS);
        let _ = wis_verlopen(basis.path(), bewaartijd(7), nu, &[]);
        // De klok gaat een maand terug: de opname is dan (nog) niet verlopen en de markering
        // gaat weg.
        let terug = nu - 30 * DAG_MS;
        let uitslag = wis_verlopen(basis.path(), bewaartijd(7), terug, &[]);
        assert_eq!(uitslag.ontmarkeerd, vec![map.clone()]);
        assert!(map.exists() && !map.join(MARKERING).exists());
        // Een markering uit de toekomst bij een wel verlopen opname wordt vervangen, niet gevolgd.
        fs::write(
            map.join(MARKERING),
            format!(
                "{{\"gemarkeerd_unix_ms\":{},\"begin_unix_ms\":0}}",
                nu + 50 * DAG_MS
            ),
        )
        .unwrap();
        let uitslag = wis_verlopen(basis.path(), bewaartijd(7), nu + 10 * DAG_MS, &[]);
        assert!(uitslag.gewist.is_empty());
        assert_eq!(uitslag.gemarkeerd, vec![map.clone()]);
        assert!(map.exists());
    }

    #[test]
    fn een_begin_in_de_toekomst_wist_niet() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG_MS;
        let map = schrijf(basis.path(), "vergadering-1", nu + 400 * DAG_MS);
        let uitslag = wis_verlopen(basis.path(), bewaartijd(1), nu + 10 * DAG_MS, &[]);
        assert!(!uitslag.veranderd());
        assert!(map.exists());
    }

    #[test]
    fn de_keuze_van_1_of_30_dagen_verschuift_de_grens() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG_MS;
        let twee_dagen = schrijf(basis.path(), "vergadering-a", nu - 2 * DAG_MS);
        let veertig = schrijf(basis.path(), "vergadering-b", nu - 40 * DAG_MS);
        let tien = schrijf(basis.path(), "vergadering-c", nu - 10 * DAG_MS);
        let eerste = wis_verlopen(basis.path(), bewaartijd(30), nu, &[]);
        assert_eq!(eerste.gemarkeerd, vec![veertig.clone()]);
        let later = nu + DAG_MS + 1;
        let tweede = wis_verlopen(basis.path(), bewaartijd(30), later, &[]);
        assert_eq!(tweede.gewist, vec![veertig]);
        assert!(twee_dagen.exists() && tien.exists());
        // Met 1 dag: eerst markeren (ook de opname van 10 dagen terug), dan pas wissen.
        let eerste = wis_verlopen(basis.path(), bewaartijd(1), later, &[]);
        assert_eq!(eerste.gemarkeerd.len(), 2);
        assert!(twee_dagen.exists() && tien.exists());
        let tweede = wis_verlopen(basis.path(), bewaartijd(1), later + DAG_MS, &[]);
        assert_eq!(tweede.gewist.len(), 2);
        assert!(!twee_dagen.exists() && !tien.exists());
    }

    #[test]
    fn een_opname_in_gebruik_blijft_staan_en_wordt_niet_gemarkeerd() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG_MS;
        let gebruikt = schrijf(basis.path(), "vergadering-1", 0);
        let andere = schrijf(basis.path(), "vergadering-2", 0);
        let uitslag = wis_verlopen(
            basis.path(),
            bewaartijd(7),
            nu,
            std::slice::from_ref(&gebruikt),
        );
        assert_eq!(uitslag.gemarkeerd, vec![andere.clone()]);
        assert!(!gebruikt.join(MARKERING).exists());
        let later = nu + 2 * DAG_MS;
        let uitslag = wis_verlopen(
            basis.path(),
            bewaartijd(7),
            later,
            std::slice::from_ref(&gebruikt),
        );
        assert_eq!(uitslag.gewist, vec![andere.clone()]);
        assert!(gebruikt.exists());
        // Zonder die aanwijzing wordt ze alsnog gemarkeerd, en een dag later gewist.
        let uitslag = wis_verlopen(basis.path(), bewaartijd(7), later, &[]);
        assert_eq!(uitslag.gemarkeerd, vec![gebruikt.clone()]);
        let uitslag = wis_verlopen(basis.path(), bewaartijd(7), later + 2 * DAG_MS, &[]);
        assert_eq!(uitslag.gewist, vec![gebruikt]);
    }

    #[test]
    fn een_lopende_opname_blijft_staan_ook_als_ze_oud_is() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG_MS;
        let map = schrijf(basis.path(), "vergadering-1", 0);
        // Een vergrendeld slot is wat een lopende opname laat zien.
        let slot_bestand = fs::File::create(map.join(super::super::SLOT)).unwrap();
        slot_bestand.try_lock().unwrap();
        let uitslag = wis_verlopen(basis.path(), bewaartijd(7), nu, &[]);
        assert!(!uitslag.veranderd());
        assert!(uitslag.mislukt.is_empty());
        assert!(map.exists() && !map.join(MARKERING).exists());
        let uitslag = wis_verlopen(basis.path(), bewaartijd(7), nu + 5 * DAG_MS, &[]);
        assert!(!uitslag.veranderd());
        // Losgelaten is ze niet meer vergrendeld: markeren, dan wissen.
        drop(slot_bestand);
        let uitslag = wis_verlopen(basis.path(), bewaartijd(7), nu, &[]);
        assert_eq!(uitslag.gemarkeerd, vec![map.clone()]);
        let uitslag = wis_verlopen(basis.path(), bewaartijd(7), nu + 2 * DAG_MS, &[]);
        assert_eq!(uitslag.gewist, vec![map]);
    }

    #[test]
    fn een_slot_dat_niet_te_lezen_is_houdt_de_opname_tegen() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG_MS;
        let map = schrijf(basis.path(), "vergadering-1", 0);
        // Een map op de plek van het slot: Mondig kan niet nagaan of de opname loopt.
        fs::create_dir(map.join(super::super::SLOT)).unwrap();
        assert_eq!(slotstand(&map), Slotstand::Onbekend);
        for later in [nu, nu + 5 * DAG_MS] {
            let uitslag = wis_verlopen(basis.path(), bewaartijd(7), later, &[]);
            assert!(!uitslag.veranderd());
        }
        assert!(map.exists() && !map.join(MARKERING).exists());
    }

    #[test]
    fn een_restmap_telt_vanaf_haar_wijzigingstijd_en_blijft_die_houden() {
        let basis = tempfile::tempdir().unwrap();
        let rest = basis.path().join("vergadering-123");
        fs::create_dir(&rest).unwrap();
        let met_audio = basis.path().join("vergadering-456");
        fs::create_dir_all(met_audio.join("microfoon")).unwrap();
        fs::write(met_audio.join("microfoon").join("00000.wav"), [0u8; 8]).unwrap();
        // Nu is de map net gemaakt, dus nog niet verlopen ...
        let uitslag = wis_verlopen(basis.path(), bewaartijd(7), nu_ms(), &[]);
        assert!(!uitslag.veranderd());
        // ... een maand later wel: markeren (dat verandert de wijzigingstijd van de map, maar
        // niet haar begin) ...
        let later = nu_ms() + 30 * DAG_MS;
        let uitslag = wis_verlopen(basis.path(), bewaartijd(7), later, &[]);
        assert_eq!(uitslag.gemarkeerd.len(), 2);
        assert!(uitslag.ontmarkeerd.is_empty());
        // ... en een dag later gaat ze weg, ook met audio erin.
        let uitslag = wis_verlopen(basis.path(), bewaartijd(7), later + DAG_MS, &[]);
        assert_eq!(uitslag.gewist.len(), 2);
        assert!(!rest.exists() && !met_audio.exists());
    }

    #[test]
    fn een_restmap_met_een_vergrendeld_slot_blijft_staan() {
        let basis = tempfile::tempdir().unwrap();
        let rest = basis.path().join("vergadering-9");
        fs::create_dir(&rest).unwrap();
        let slot_bestand = fs::File::create(rest.join(super::super::SLOT)).unwrap();
        slot_bestand.try_lock().unwrap();
        let ver = nu_ms() + 90 * DAG_MS;
        for nu in [ver, ver + 5 * DAG_MS] {
            let uitslag = wis_verlopen(basis.path(), bewaartijd(7), nu, &[]);
            assert!(!uitslag.veranderd());
        }
        assert!(rest.exists());
        drop(slot_bestand);
    }

    /// Maakt een link (op Windows een junction, die geen beheerdersrechten vraagt).
    fn maak_link(link: &Path, doel: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(doel, link).unwrap();
        #[cfg(windows)]
        {
            let uitslag = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(doel)
                .output()
                .unwrap();
            assert!(uitslag.status.success(), "{uitslag:?}");
        }
    }

    #[test]
    fn een_link_wordt_nergens_gevolgd() {
        let nu = 100 * DAG_MS;
        let buiten = tempfile::tempdir().unwrap();
        let basis = tempfile::tempdir().unwrap();
        // Een opname buiten de tijdelijke map, met een link ernaartoe erin.
        let doel = schrijf(buiten.path(), "vergadering-doel", 0);
        maak_link(&basis.path().join("vergadering-link"), &doel);
        // Een echte opname met een link naar audio buiten de map.
        let kostbaar = buiten.path().join("kostbaar");
        fs::create_dir(&kostbaar).unwrap();
        fs::write(kostbaar.join("belangrijk.wav"), [1u8; 8]).unwrap();
        let echt = schrijf(basis.path(), "vergadering-echt", 0);
        maak_link(&echt.join("systeem"), &kostbaar);

        let eerste = wis_verlopen(basis.path(), bewaartijd(7), nu, &[]);
        assert_eq!(
            eerste.gemarkeerd,
            vec![echt.clone()],
            "de link telt niet mee"
        );
        let tweede = wis_verlopen(basis.path(), bewaartijd(7), nu + 2 * DAG_MS, &[]);
        assert_eq!(tweede.gewist, vec![echt.clone()]);
        // De link zelf en alles erachter staat er nog.
        assert!(doel.join(KOP).is_file());
        assert!(!doel.join(MARKERING).exists());
        assert!(basis.path().join("vergadering-link").exists());
        assert!(
            kostbaar.join("belangrijk.wav").is_file(),
            "niet via de link gewist"
        );
        assert!(!echt.exists());
    }

    #[test]
    fn een_ontbrekende_basismap_geeft_niets() {
        let basis = tempfile::tempdir().unwrap();
        let weg = basis.path().join("bestaat-niet");
        let uitslag = wis_verlopen(&weg, bewaartijd(7), nu_ms(), &[]);
        assert!(!uitslag.veranderd() && uitslag.mislukt.is_empty());
    }
}
