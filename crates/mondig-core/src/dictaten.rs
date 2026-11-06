//! Dictaatgeschiedenis op schijf (ADR 0021).
//!
//! Elk dictaat is een eigen klein bestand `dictaat-<unix_ms>-<teller>.json` in de map
//! `dictaten` onder de lokale gegevensmap van Mondig. Het bestand bevat alleen de versie, het
//! moment waarop de tekst klaar was en de tekst zelf; bij een opgeschoond dictaat (ADR 0027
//! punt 4) ook de letterlijke tekst, onder dezelfde termijn. Dat veld is optioneel: bestanden
//! zonder dat veld blijven gewoon te lezen, en een dictaat zonder opschonen krijgt het niet. Mondig wist een dictaat zodra het ouder
//! is dan de gekozen termijn, zonder markering vooraf (anders dan bij opnames, zie
//! `opname::vergadering::verlopen`): bij een dictaat heeft een melding vooraf geen doel en
//! maakt een dag wachten van 24 uur in de praktijk 48 uur.
//!
//! Bij twijfel wissen (ADR 0021 punt 5): een dictaat met een tijd meer dan een uur in de
//! toekomst (klok teruggezet) telt als verlopen. Dictaattekst en bestandsnamen komen nooit in
//! het logboek; deze module logt niets.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::model::IDENTIFIER;
use crate::soevereiniteit::Soevereiniteit;
use crate::spoor::stukken::schrijf_atomair;

/// De map in de gegevensmap van Mondig waarin de dictaten staan.
const MAPNAAM: &str = "dictaten";

/// Het voorvoegsel van elk bestand dat van ons is. Wat er niet mee begint, blijft ongemoeid.
const VOORVOEGSEL: &str = "dictaat-";

/// De extensie van een dictaat.
const EXTENSIE: &str = ".json";

/// De extensie van het tijdelijke bestand van `schrijf_atomair`; blijft hij na een crash
/// liggen, dan staat er dictaattekst in en ruimen we hem op.
const RESTEXTENSIE: &str = ".json.nieuw";

/// Zoveel mag de tijd van een dictaat in de toekomst liggen (kloktoleranties) voor het telt
/// als verlopen.
const TOEKOMST_TOLERANTIE_MS: u64 = 3_600_000;

/// De versie van het bestandsformaat.
const VERSIE: u32 = 1;

/// Telt dictaten binnen dezelfde milliseconde, zodat twee namen nooit samenvallen.
static TELLER: AtomicU64 = AtomicU64::new(0);

/// Hoe lang Mondig dictaten bewaart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termijn {
    /// Niets naar schijf; het laatste dictaat blijft alleen in het geheugen.
    Niet,
    /// 24 uur.
    Dag,
    /// 7 dagen.
    Week,
}

impl Termijn {
    /// De standaard (ADR 0021 punt 5): 24 uur. Ook wat geldt als de instelling ontbreekt of
    /// niet te lezen is.
    pub const STANDAARD: Self = Self::Dag;

    /// De waarde in de instellingen: `niet`, `24u` of `7d`.
    #[must_use]
    pub const fn sleutel(self) -> &'static str {
        match self {
            Self::Niet => "niet",
            Self::Dag => "24u",
            Self::Week => "7d",
        }
    }

    /// De termijn bij een waarde uit de instellingen, of `None` als die onbekend is.
    #[must_use]
    pub fn uit_sleutel(sleutel: &str) -> Option<Self> {
        [Self::Niet, Self::Dag, Self::Week]
            .into_iter()
            .find(|t| t.sleutel() == sleutel)
    }

    /// De bewaartijd; bij [`Termijn::Niet`] nul, dus alles telt als verlopen.
    #[must_use]
    pub const fn duur(self) -> Duration {
        match self {
            Self::Niet => Duration::ZERO,
            Self::Dag => Duration::from_hours(24),
            Self::Week => Duration::from_hours(7 * 24),
        }
    }
}

/// Wat er in een bestand staat.
#[derive(Debug, Serialize, Deserialize)]
struct Bestand {
    versie: u32,
    /// Het moment waarop de tekst klaar was (milliseconden sinds 1970).
    tijd_unix_ms: u64,
    tekst: String,
    /// De letterlijke tekst als `tekst` opgeschoond is (ADR 0027 punt 4); anders afwezig.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    letterlijk: Option<String>,
    /// Wie het dictaat opschoonde; alleen bij een opgeschoond dictaat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    opgeschoond_door: Option<OpgeschoondDoor>,
}

/// Wie een dictaat opschoonde: het model en de aanbieder, voor de regel "Opgeschoond door
/// Mistral Small (Europees)" in de geschiedenis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpgeschoondDoor {
    /// De modelnaam bij de aanbieder (`mistral-small-latest`, `qwen3:8b`).
    pub model: String,
    /// De code van de aanbieder: `mistral` of `ollama`.
    pub aanbieder: String,
    /// Het label van de aanbieder (Europees, Lokaal), zodat de interface het nooit zelf afleidt.
    /// Optioneel: een bestand zonder label blijft leesbaar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<Soevereiniteit>,
}

/// Een bewaard dictaat, zoals de interface het ziet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dictaat {
    /// De bestandsnaam; de interface geeft hem terug om het dictaat te verwijderen.
    pub id: String,
    /// Het moment waarop de tekst klaar was (milliseconden sinds 1970).
    pub tijd_unix_ms: u64,
    /// De tekst zoals hij geplakt is.
    pub tekst: String,
    /// De letterlijke tekst als `tekst` opgeschoond is, voor "Letterlijke tekst kopiëren"
    /// (ADR 0027 punt 4). Ontbreekt in de JSON voor de interface als het dictaat niet
    /// opgeschoond is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub letterlijk: Option<String>,
    /// Wie het dictaat opschoonde; ontbreekt bij een dictaat zonder opschonen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opgeschoond_door: Option<OpgeschoondDoor>,
}

/// Waarom een dictaat niet te verwijderen was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VerwijderFout {
    /// Het id is geen gewone bestandsnaam van een dictaat.
    #[error("Dit dictaat bestaat niet.")]
    OngeldigId,
    /// Het bestand ging niet weg.
    #[error("Mondig kon het dictaat niet wissen.")]
    Mislukt,
}

/// De map van de dictaten, of `None` als het systeem geen lokale gegevensmap kent. Dezelfde
/// plek als de modellen: op Windows `%LOCALAPPDATA%`, niet de zwervende map.
#[must_use]
pub fn standaard_map() -> Option<PathBuf> {
    dirs::data_local_dir().map(|basis| basis.join(IDENTIFIER).join(MAPNAAM))
}

/// Het huidige moment in milliseconden sinds 1970.
#[must_use]
pub fn nu_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn ms(duur: Duration) -> u64 {
    u64::try_from(duur.as_millis()).unwrap_or(u64::MAX)
}

/// Is dit een gewone bestandsnaam van een dictaat? Streng: alleen kleine letters, cijfers,
/// streepjes en punten, het eigen voorvoegsel en de extensie, en geen `..`. Zo kan een id uit
/// de interface nooit naar een ander pad wijzen.
fn is_dictaatnaam(naam: &str) -> bool {
    naam.starts_with(VOORVOEGSEL)
        && naam.ends_with(EXTENSIE)
        && !naam.contains("..")
        && naam
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
}

/// Hoort dit bestand bij ons? Een dictaat of een restant van het atomair schrijven.
fn is_eigen(naam: &str) -> bool {
    is_dictaatnaam(naam)
        || (naam.starts_with(VOORVOEGSEL)
            && naam.ends_with(RESTEXTENSIE)
            && !naam.contains("..")
            && !naam.contains(['/', '\\', ':']))
}

/// De eigen bestanden in `basis`. `DirEntry::file_type` volgt geen links: een symlink of
/// junction is hier geen gewoon bestand en blijft staan.
fn eigen_bestanden(basis: &Path) -> Vec<PathBuf> {
    let Ok(lijst) = fs::read_dir(basis) else {
        return Vec::new();
    };
    let mut paden: Vec<PathBuf> = lijst
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| e.file_name().to_str().is_some_and(is_eigen))
        .map(|e| e.path())
        .collect();
    paden.sort();
    paden
}

fn naam_van(pad: &Path) -> Option<&str> {
    pad.file_name().and_then(|n| n.to_str())
}

fn lees_bestand(pad: &Path) -> Option<Dictaat> {
    let naam = naam_van(pad).filter(|n| is_dictaatnaam(n))?;
    let inhoud = fs::read(pad).ok()?;
    let bestand: Bestand = serde_json::from_slice(&inhoud).ok()?;
    Some(Dictaat {
        id: naam.to_string(),
        tijd_unix_ms: bestand.tijd_unix_ms,
        tekst: bestand.tekst,
        letterlijk: bestand.letterlijk,
        opgeschoond_door: bestand.opgeschoond_door,
    })
}

/// Is een dictaat met deze tijd verlopen? Ouder dan de termijn (`tijd + termijn <= nu`), of
/// meer dan een uur in de toekomst.
fn is_verlopen(tijd_ms: u64, termijn: Termijn, nu_ms: u64) -> bool {
    tijd_ms.saturating_add(ms(termijn.duur())) <= nu_ms
        || tijd_ms > nu_ms.saturating_add(TOEKOMST_TOLERANTIE_MS)
}

/// De tijd van een eigen bestand dat geen leesbaar dictaat is (beschadigd, of een restant van
/// het schrijven): de wijzigingstijd, zodat ook dat na de termijn verdwijnt.
fn wijzigingstijd_ms(pad: &Path) -> u64 {
    fs::metadata(pad)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, ms)
}

/// Bewaart een dictaat en geeft zijn id. De map wordt zo nodig gemaakt. Schrijft atomair: een
/// crash laat geen half bestand achter.
///
/// # Errors
///
/// Geeft een I/O-fout als de map of het bestand niet te maken is.
pub fn bewaar(basis: &Path, tekst: &str, nu_ms: u64) -> io::Result<String> {
    bewaar_met(basis, tekst, None, nu_ms)
}

/// Bewaart een opgeschoond dictaat: de geplakte tekst, de letterlijke tekst (ADR 0027 punt 4)
/// en wie het opschoonde. Verder als [`bewaar`].
///
/// # Errors
///
/// Geeft een I/O-fout als de map of het bestand niet te maken is.
pub fn bewaar_opgeschoond(
    basis: &Path,
    tekst: &str,
    letterlijk: &str,
    door: &OpgeschoondDoor,
    nu_ms: u64,
) -> io::Result<String> {
    bewaar_met(basis, tekst, Some((letterlijk, door)), nu_ms)
}

fn bewaar_met(
    basis: &Path,
    tekst: &str,
    opgeschoond: Option<(&str, &OpgeschoondDoor)>,
    nu_ms: u64,
) -> io::Result<String> {
    fs::create_dir_all(basis)?;
    let teller = TELLER.fetch_add(1, Ordering::Relaxed);
    let id = format!("{VOORVOEGSEL}{nu_ms}-{teller}{EXTENSIE}");
    let bestand = Bestand {
        versie: VERSIE,
        tijd_unix_ms: nu_ms,
        tekst: tekst.to_string(),
        letterlijk: opgeschoond.map(|(letterlijk, _)| letterlijk.to_string()),
        opgeschoond_door: opgeschoond.map(|(_, door)| door.clone()),
    };
    let json = serde_json::to_vec(&bestand).map_err(io::Error::other)?;
    schrijf_atomair(&basis.join(&id), &json)?;
    Ok(id)
}

/// De bewaarde dictaten, nieuwste eerst. Wat verlopen is, staat er niet in, ook als het
/// bestand er nog ligt (ADR 0021 punt 4). Onleesbare bestanden worden overgeslagen.
#[must_use]
pub fn lijst(basis: &Path, termijn: Termijn, nu_ms: u64) -> Vec<Dictaat> {
    let mut dictaten: Vec<Dictaat> = eigen_bestanden(basis)
        .iter()
        .filter_map(|pad| lees_bestand(pad))
        .filter(|d| !is_verlopen(d.tijd_unix_ms, termijn, nu_ms))
        .collect();
    // Nieuwste eerst; bij dezelfde milliseconde beslist de naam (met de teller).
    dictaten.sort_by(|a, b| {
        b.tijd_unix_ms
            .cmp(&a.tijd_unix_ms)
            .then_with(|| b.id.cmp(&a.id))
    });
    dictaten
}

/// Het aantal eigen dictaatbestanden op schijf, verlopen of niet. Voor de bevestigvraag
/// bij "Niet bewaren".
#[must_use]
pub fn aantal(basis: &Path) -> usize {
    eigen_bestanden(basis)
        .iter()
        .filter(|pad| naam_van(pad).is_some_and(is_dictaatnaam))
        .count()
}

/// Verwijdert één dictaat. Een id dat geen gewone bestandsnaam met het eigen voorvoegsel is,
/// wordt geweigerd. Geeft `true` als er een bestand verdween, `false` als het er al niet meer
/// was.
///
/// # Errors
///
/// [`VerwijderFout::OngeldigId`] bij een id dat geen dictaatnaam is; [`VerwijderFout::Mislukt`]
/// als het bestand er is maar niet weg wil.
pub fn verwijder(basis: &Path, id: &str) -> Result<bool, VerwijderFout> {
    if !is_dictaatnaam(id) {
        return Err(VerwijderFout::OngeldigId);
    }
    let pad = basis.join(id);
    // Een link of een map is geen dictaat van ons.
    let is_bestand = fs::symlink_metadata(&pad).is_ok_and(|m| m.file_type().is_file());
    if !is_bestand {
        return Ok(false);
    }
    fs::remove_file(&pad).map_err(|_| VerwijderFout::Mislukt)?;
    Ok(true)
}

/// Wat een wisronde deed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Gewist {
    /// De id's van de gewiste dictaten, voor het geheugen van de app.
    pub ids: Vec<String>,
    /// Het aantal bestanden dat is verdwenen.
    pub aantal: usize,
    /// Het aantal bestanden dat weg moest maar niet wilde.
    pub mislukt: usize,
}

fn wis_paden(paden: &[PathBuf]) -> Gewist {
    let mut uitslag = Gewist::default();
    for pad in paden {
        if fs::remove_file(pad).is_ok() {
            uitslag.aantal += 1;
            uitslag.ids.extend(
                naam_van(pad)
                    .filter(|n| is_dictaatnaam(n))
                    .map(str::to_string),
            );
        } else {
            uitslag.mislukt += 1;
        }
    }
    uitslag
}

/// Wist alle dictaten (en restanten van het schrijven). Alleen bestanden met ons voorvoegsel.
#[must_use]
pub fn wis_alles(basis: &Path) -> Gewist {
    wis_paden(&eigen_bestanden(basis))
}

/// Wist wat verlopen is, zonder markering of wachttijd: weg als `tijd + termijn <= nu`, of als
/// de tijd meer dan een uur in de toekomst ligt. Bij [`Termijn::Niet`] is alles verlopen.
#[must_use]
pub fn wis_verlopen(basis: &Path, termijn: Termijn, nu_ms: u64) -> Gewist {
    let te_wissen: Vec<PathBuf> = eigen_bestanden(basis)
        .into_iter()
        .filter(|pad| {
            // Eén keer lezen: de tijd uit het bestand, anders de wijzigingstijd.
            let tijd = lees_bestand(pad).map_or_else(|| wijzigingstijd_ms(pad), |d| d.tijd_unix_ms);
            is_verlopen(tijd, termijn, nu_ms)
        })
        .collect();
    wis_paden(&te_wissen)
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUR: u64 = 3_600_000;
    const DAG: u64 = 24 * UUR;

    fn namen(basis: &Path) -> Vec<String> {
        let mut namen: Vec<String> = fs::read_dir(basis)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        namen.sort();
        namen
    }

    #[test]
    fn de_sleutels_gaan_heen_en_terug() {
        for t in [Termijn::Niet, Termijn::Dag, Termijn::Week] {
            assert_eq!(Termijn::uit_sleutel(t.sleutel()), Some(t));
        }
        assert_eq!(Termijn::uit_sleutel("1u"), None);
        assert_eq!(Termijn::uit_sleutel(""), None);
        assert_eq!(Termijn::STANDAARD, Termijn::Dag);
    }

    #[test]
    fn bewaren_schrijft_een_bestand_met_de_juiste_inhoud() {
        let basis = tempfile::tempdir().unwrap();
        let map = basis.path().join("dictaten");
        let id = bewaar(&map, "Hallo wereld.", 1_000).unwrap();
        assert!(id.starts_with("dictaat-1000-"));
        assert!(is_dictaatnaam(&id));
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(map.join(&id)).unwrap()).unwrap();
        assert_eq!(json["versie"], 1);
        assert_eq!(json["tijd_unix_ms"], 1_000);
        assert_eq!(json["tekst"], "Hallo wereld.");
        // Geen tijdelijk bestand achtergelaten.
        assert_eq!(namen(&map), vec![id]);
    }

    #[test]
    fn een_opgeschoond_dictaat_bewaart_ook_de_letterlijke_tekst() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        let door = OpgeschoondDoor {
            model: "mistral-small-latest".into(),
            aanbieder: "mistral".into(),
            label: Some(Soevereiniteit::Europees),
        };
        let id = bewaar_opgeschoond(
            basis.path(),
            "We zien elkaar woensdag.",
            "ehm we zien elkaar dinsdag nee woensdag",
            &door,
            nu,
        )
        .unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(basis.path().join(&id)).unwrap()).unwrap();
        assert_eq!(
            json["letterlijk"],
            "ehm we zien elkaar dinsdag nee woensdag"
        );
        let dictaat = &lijst(basis.path(), Termijn::Dag, nu)[0];
        assert_eq!(dictaat.tekst, "We zien elkaar woensdag.");
        assert_eq!(
            dictaat.letterlijk.as_deref(),
            Some("ehm we zien elkaar dinsdag nee woensdag")
        );
        // Naar de interface in camelCase, met het veld.
        let voor_interface = serde_json::to_value(dictaat).unwrap();
        assert_eq!(
            voor_interface["letterlijk"],
            "ehm we zien elkaar dinsdag nee woensdag"
        );
        assert_eq!(dictaat.opgeschoond_door.as_ref(), Some(&door));
        assert_eq!(
            voor_interface["opgeschoondDoor"],
            serde_json::json!({
                "model": "mistral-small-latest",
                "aanbieder": "mistral",
                "label": "europees"
            })
        );
        assert_eq!(json["opgeschoond_door"]["aanbieder"], "mistral");
    }

    #[test]
    fn een_opgeschoond_dictaat_zonder_label_blijft_leesbaar() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        fs::write(
            basis.path().join("dictaat-6-0.json"),
            format!(
                "{{\"versie\":1,\"tijd_unix_ms\":{nu},\"tekst\":\"Schoon.\",\"letterlijk\":\"ehm schoon\",\"opgeschoond_door\":{{\"model\":\"m\",\"aanbieder\":\"mistral\"}}}}"
            ),
        )
        .unwrap();
        let dictaat = &lijst(basis.path(), Termijn::Dag, nu)[0];
        let door = dictaat.opgeschoond_door.as_ref().unwrap();
        assert_eq!(door.aanbieder, "mistral");
        assert_eq!(door.label, None);
    }

    #[test]
    fn een_bestand_zonder_letterlijke_tekst_blijft_leesbaar() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        // Zoals versie 1 het schreef voor er opschonen was.
        fs::write(
            basis.path().join("dictaat-5-0.json"),
            format!("{{\"versie\":1,\"tijd_unix_ms\":{nu},\"tekst\":\"Oud dictaat.\"}}"),
        )
        .unwrap();
        let id = bewaar(basis.path(), "Nieuw, niet opgeschoond.", nu).unwrap();
        let dictaten = lijst(basis.path(), Termijn::Dag, nu);
        assert_eq!(dictaten.len(), 2);
        assert!(dictaten.iter().all(|d| d.letterlijk.is_none()));
        // Zonder opschonen komt het veld niet in het bestand en niet in de JSON voor de
        // interface: precies zoals voorheen.
        let inhoud = fs::read_to_string(basis.path().join(&id)).unwrap();
        assert!(!inhoud.contains("letterlijk"), "{inhoud}");
        let voor_interface = serde_json::to_string(&dictaten[0]).unwrap();
        assert!(!voor_interface.contains("letterlijk"), "{voor_interface}");
        assert!(!inhoud.contains("opgeschoond_door"), "{inhoud}");
        assert!(dictaten.iter().all(|d| d.opgeschoond_door.is_none()));
    }

    #[test]
    fn dezelfde_milliseconde_geeft_twee_namen() {
        let basis = tempfile::tempdir().unwrap();
        let a = bewaar(basis.path(), "een", 5).unwrap();
        let b = bewaar(basis.path(), "twee", 5).unwrap();
        assert_ne!(a, b);
        assert_eq!(lijst(basis.path(), Termijn::Dag, 5).len(), 2);
    }

    #[test]
    fn de_lijst_is_nieuwste_eerst() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        bewaar(basis.path(), "oudste", nu - 3 * UUR).unwrap();
        bewaar(basis.path(), "nieuwste", nu - UUR).unwrap();
        bewaar(basis.path(), "midden", nu - 2 * UUR).unwrap();
        let teksten: Vec<String> = lijst(basis.path(), Termijn::Dag, nu)
            .into_iter()
            .map(|d| d.tekst)
            .collect();
        assert_eq!(teksten, ["nieuwste", "midden", "oudste"]);
    }

    #[test]
    fn net_voor_de_termijn_blijft_en_net_erna_is_weg() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        // Precies op de grens telt als verlopen (`tijd + termijn <= nu`).
        let id_erna = bewaar(basis.path(), "net erna", nu - DAG).unwrap();
        bewaar(basis.path(), "net ervoor", nu - DAG + 1).unwrap();
        // De lijst laat het verlopen dictaat al weg, ook zonder opruimronde.
        let zichtbaar = lijst(basis.path(), Termijn::Dag, nu);
        assert_eq!(zichtbaar.len(), 1);
        assert_eq!(zichtbaar[0].tekst, "net ervoor");
        assert_eq!(namen(basis.path()).len(), 2, "het bestand ligt er nog");

        let uitslag = wis_verlopen(basis.path(), Termijn::Dag, nu);
        assert_eq!(uitslag.aantal, 1);
        assert_eq!(uitslag.ids, [id_erna]);
        assert_eq!(namen(basis.path()).len(), 1);
        assert_eq!(lijst(basis.path(), Termijn::Dag, nu)[0].tekst, "net ervoor");
    }

    #[test]
    fn een_week_houdt_langer_vast_dan_een_dag() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        bewaar(basis.path(), "drie dagen oud", nu - 3 * DAG).unwrap();
        assert!(lijst(basis.path(), Termijn::Dag, nu).is_empty());
        assert_eq!(lijst(basis.path(), Termijn::Week, nu).len(), 1);
        // Korter zetten wist meteen wat ouder is.
        assert_eq!(wis_verlopen(basis.path(), Termijn::Dag, nu).aantal, 1);
        assert!(namen(basis.path()).is_empty());
    }

    #[test]
    fn een_tijd_ver_in_de_toekomst_telt_als_verlopen() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        bewaar(basis.path(), "iets later", nu + UUR).unwrap();
        let id_ver = bewaar(basis.path(), "te ver", nu + UUR + 1).unwrap();
        let zichtbaar = lijst(basis.path(), Termijn::Dag, nu);
        assert_eq!(zichtbaar.len(), 1);
        assert_eq!(zichtbaar[0].tekst, "iets later");
        let uitslag = wis_verlopen(basis.path(), Termijn::Dag, nu);
        assert_eq!(uitslag.ids, [id_ver]);
    }

    #[test]
    fn niet_bewaren_laat_alles_verlopen() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        bewaar(basis.path(), "net gedicteerd", nu).unwrap();
        assert!(lijst(basis.path(), Termijn::Niet, nu).is_empty());
        assert_eq!(wis_verlopen(basis.path(), Termijn::Niet, nu).aantal, 1);
        assert!(namen(basis.path()).is_empty());
    }

    #[test]
    fn vreemde_bestanden_en_links_blijven_staan() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        fs::write(basis.path().join("notities.txt"), "van de gebruiker").unwrap();
        fs::write(basis.path().join("dictaat.json"), "{}").unwrap();
        fs::write(basis.path().join("ander-dictaat-1-0.json"), "{}").unwrap();
        fs::create_dir(basis.path().join("dictaat-1-0.json")).unwrap();
        bewaar(basis.path(), "oud", nu - 10 * DAG).unwrap();

        let w = wis_verlopen(basis.path(), Termijn::Dag, nu);
        assert_eq!(w.aantal, 1);
        let w = wis_alles(basis.path());
        assert_eq!(w.aantal, 0);
        assert_eq!(
            namen(basis.path()),
            [
                "ander-dictaat-1-0.json",
                "dictaat-1-0.json",
                "dictaat.json",
                "notities.txt"
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn een_link_met_ons_voorvoegsel_wordt_niet_gevolgd() {
        let basis = tempfile::tempdir().unwrap();
        let elders = tempfile::tempdir().unwrap();
        let doel = elders.path().join("belangrijk.txt");
        fs::write(&doel, "blijf van mij af").unwrap();
        std::os::unix::fs::symlink(&doel, basis.path().join("dictaat-1-0.json")).unwrap();
        let _ = wis_alles(basis.path());
        assert!(doel.exists());
        assert_eq!(verwijder(basis.path(), "dictaat-1-0.json"), Ok(false));
        assert!(doel.exists());
    }

    #[test]
    fn onleesbare_bestanden_worden_overgeslagen_en_na_de_termijn_gewist() {
        let basis = tempfile::tempdir().unwrap();
        let nu = nu_ms();
        fs::write(basis.path().join("dictaat-5-0.json"), "geen json").unwrap();
        bewaar(basis.path(), "goed", nu).unwrap();
        let zichtbaar = lijst(basis.path(), Termijn::Dag, nu);
        assert_eq!(zichtbaar.len(), 1);
        assert_eq!(zichtbaar[0].tekst, "goed");
        // Het kapotte bestand is zojuist gemaakt (wijzigingstijd nu): blijft tot het verloopt.
        assert_eq!(wis_verlopen(basis.path(), Termijn::Dag, nu).aantal, 0);
        assert_eq!(
            wis_verlopen(basis.path(), Termijn::Dag, nu + 2 * DAG).aantal,
            2
        );
    }

    #[test]
    fn een_restant_van_het_schrijven_wordt_opgeruimd() {
        let basis = tempfile::tempdir().unwrap();
        fs::write(
            basis.path().join("dictaat-5-0.json.nieuw"),
            "{\"tekst\":\"half\"}",
        )
        .unwrap();
        assert!(lijst(basis.path(), Termijn::Dag, nu_ms()).is_empty());
        assert_eq!(wis_alles(basis.path()).aantal, 1);
        assert!(namen(basis.path()).is_empty());
    }

    #[test]
    fn verwijderen_haalt_een_dictaat_weg_en_geeft_de_tekst() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        let a = bewaar(basis.path(), "een", nu).unwrap();
        bewaar(basis.path(), "twee", nu).unwrap();
        assert_eq!(verwijder(basis.path(), &a), Ok(true));
        assert_eq!(lijst(basis.path(), Termijn::Dag, nu).len(), 1);
        // Nog een keer: het is er al niet meer, dat is geen fout.
        assert_eq!(verwijder(basis.path(), &a), Ok(false));
    }

    #[test]
    fn verwijderen_weigert_een_kwaadaardig_id() {
        let basis = tempfile::tempdir().unwrap();
        let map = basis.path().join("dictaten");
        fs::create_dir(&map).unwrap();
        let buiten = basis.path().join("dictaat-1-0.json");
        fs::write(&buiten, "{}").unwrap();
        for id in [
            "",
            "..",
            "../dictaat-1-0.json",
            "..\\dictaat-1-0.json",
            "dictaat-..-0.json",
            "dictaat-1-0.json/",
            "sub/dictaat-1-0.json",
            "C:dictaat-1-0.json",
            "C:\\Windows\\win.ini",
            "/etc/passwd",
            "notities.txt",
            "dictaat-1-0.JSON",
            "dictaat-1-0.json.nieuw",
            "Dictaat-1-0.json",
            "dictaat-1-0.json\0",
        ] {
            assert_eq!(
                verwijder(&map, id),
                Err(VerwijderFout::OngeldigId),
                "id {id:?}"
            );
        }
        assert!(buiten.exists());
    }

    #[test]
    fn alles_wissen_haalt_elk_dictaat_weg() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        bewaar(basis.path(), "een", nu).unwrap();
        bewaar(basis.path(), "twee", nu - DAG).unwrap();
        fs::write(basis.path().join("blijft.txt"), "x").unwrap();
        assert_eq!(aantal(basis.path()), 2);
        let w = wis_alles(basis.path());
        assert_eq!(w.aantal, 2);
        assert_eq!(w.mislukt, 0);
        assert_eq!(w.ids.len(), 2);
        assert_eq!(aantal(basis.path()), 0);
        assert_eq!(namen(basis.path()), ["blijft.txt"]);
    }

    #[test]
    fn een_ontbrekende_map_geeft_een_lege_lijst() {
        let basis = tempfile::tempdir().unwrap();
        let weg = basis.path().join("bestaat-niet");
        assert!(lijst(&weg, Termijn::Dag, 1).is_empty());
        assert_eq!(wis_alles(&weg), Gewist::default());
        assert_eq!(wis_verlopen(&weg, Termijn::Dag, 1), Gewist::default());
        assert_eq!(aantal(&weg), 0);
    }
}
