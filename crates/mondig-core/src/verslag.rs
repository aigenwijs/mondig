//! Een uitgeschreven vergadering bewaren naast haar opname (ADR 0022).
//!
//! Het verslag staat als `verslag.json` in de map van de opname. Zo valt het vanzelf onder
//! dezelfde bewaartermijn en hetzelfde wismechanisme (`opname::vergadering::wis_verlopen`) als
//! het geluid, en gaat "Verslag wissen" met één `wis` over beide.
//!
//! Wat erin staat: de versie van het bestand, het gesprek (beurten met tekst, tijden, kant,
//! spreker en naam), het model waarmee het is uitgeschreven, de naam die de gebruiker het
//! gaf, de markeringen van persoonsgegevens, en als ze er zijn de notulen (ADR 0026, optioneel
//! veld `notulen`; een bestand zonder dat veld blijft gewoon leesbaar). Er staat niets in wat niet al in de tekst of
//! het geluid zit.
//!
//! Lezen is mild: een bestand dat ontbreekt, onleesbaar is of van een nieuwere versie komt,
//! geeft [`lees`] `None`. De opname blijft dan staan onder "Nog uit te schrijven", waar het
//! geluid opnieuw kan worden uitgeschreven. Bewust zonder `Debug`: er staat tekst in.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::gesprek::{Gesprek, Kant, Vergadersoort, schone_verslagnaam};
use crate::notulen::OpgeslagenNotulen;
use crate::persoonsgegevens::Markeringen;
use crate::spoor::stukken::schrijf_atomair;

/// De naam van het bestand in de map van de opname.
pub const BESTAND: &str = "verslag.json";

/// Wat een beschadigd bestand krijgt als het opzij wordt gezet ([`zet_opzij`]).
const OPZIJ: &str = "verslag.kapot.json";

/// De versie van het bestand dat deze code schrijft en nog kan lezen.
pub const VERSIE: u32 = 1;

/// Wat de lijst "Eerder opgenomen" van een verslag nodig heeft. Staat vooraan in het bestand,
/// naast de versie, zodat de lijst de tekst niet hoeft in te lezen ([`lees_samenvatting`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Samenvatting {
    /// De naam die de gebruiker het verslag gaf.
    pub naam: Option<String>,
    /// Online, in de ruimte of Alleen ik.
    pub soort: Vergadersoort,
    /// De duur van het gesprek in milliseconden.
    pub duur_ms: u64,
    /// Hoeveel sprekers er zijn, als ze herkend zijn ([`aantal_sprekers`]).
    pub sprekers: Option<usize>,
}

/// Hoeveel sprekers een gesprek heeft, als ze herkend zijn; anders `None`, want "Jij en Anderen"
/// is geen telling van mensen. Dezelfde telling als `aantalSprekers` in de interface.
#[must_use]
pub fn aantal_sprekers(gesprek: &Gesprek) -> Option<usize> {
    if !gesprek.beurten.iter().any(|b| b.sprekernummer.is_some()) {
        return None;
    }
    let sprekers: BTreeSet<(bool, Option<u8>)> = gesprek
        .beurten
        .iter()
        .map(|b| (b.kant == Kant::Anderen, b.sprekernummer))
        .collect();
    Some(sprekers.len())
}

/// Een verslag zoals het op schijf komt, door verwijzing: schrijven kost zo geen kopie van de
/// tekst. De versie en de samenvatting maakt [`Bewaren::nieuw`] zelf.
#[derive(Serialize)]
pub struct Bewaren<'a> {
    versie: u32,
    samenvatting: Samenvatting,
    /// Het gesprek.
    pub gesprek: &'a Gesprek,
    /// De id van het model waarmee het is uitgeschreven.
    pub model_id: &'a str,
    /// De korte naam van dat model, voor in het verslag.
    pub model: &'a str,
    /// De naam die de gebruiker het verslag gaf; `None` is de titel die Mondig zelf maakt.
    pub naam: Option<&'a str>,
    /// De markeringen van persoonsgegevens.
    pub markeringen: &'a Markeringen,
    /// De notulen, als die er zijn (ADR 0026 punt 5), ook als deze versie ze niet kan lezen.
    /// Zonder notulen staat het veld niet in het bestand.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notulen: Option<&'a OpgeslagenNotulen>,
}

impl<'a> Bewaren<'a> {
    /// Een verslag om te schrijven, met de huidige versie en de samenvatting voor de lijst. De
    /// notulen zijn verplicht mee te geven (ook als `None`): wie het verslag opnieuw schrijft (een
    /// naam, een verbeterde regel, opnieuw uitschrijven, terugzetten), geeft de bestaande notulen
    /// mee, zodat ze nooit stil verdwijnen.
    #[must_use]
    pub fn nieuw(
        gesprek: &'a Gesprek,
        model_id: &'a str,
        model: &'a str,
        naam: Option<&'a str>,
        markeringen: &'a Markeringen,
        notulen: Option<&'a OpgeslagenNotulen>,
    ) -> Self {
        Self {
            versie: VERSIE,
            samenvatting: Samenvatting {
                naam: naam.map(str::to_string),
                soort: gesprek.soort,
                duur_ms: u64::try_from(gesprek.duur.as_millis()).unwrap_or(u64::MAX),
                sprekers: aantal_sprekers(gesprek),
            },
            gesprek,
            model_id,
            model,
            naam,
            markeringen,
            notulen,
        }
    }
}

/// Een verslag zoals [`lees`] het teruggeeft.
#[derive(Deserialize)]
pub struct Bewaard {
    /// Zie [`VERSIE`].
    pub versie: u32,
    /// Het gesprek.
    pub gesprek: Gesprek,
    /// De id van het model waarmee het is uitgeschreven.
    pub model_id: String,
    /// De korte naam van dat model.
    pub model: String,
    /// De naam die de gebruiker het verslag gaf, schoongemaakt en begrensd.
    #[serde(default)]
    pub naam: Option<String>,
    /// De markeringen van persoonsgegevens, hersteld tot wat [`Markeringen::markeer`] toestaat.
    pub markeringen: Markeringen,
    /// De notulen, als die er zijn. Ontbreekt het veld (een verslag van voor de notulen), dan
    /// `None`. Notulen die deze versie niet kan lezen, komen terug als
    /// [`OpgeslagenNotulen::Onleesbaar`] en gaan zo ook weer naar schijf.
    #[serde(default)]
    pub notulen: Option<OpgeslagenNotulen>,
}

/// De bytes van `verslag.json`. Aparte stap, zodat de aanroeper onder zijn slot kan
/// serialiseren en buiten het slot schrijven ([`schrijf_bytes`]).
///
/// # Errors
///
/// Geeft een I/O-fout als het verslag niet te serialiseren is.
pub fn naar_bytes(verslag: &Bewaren<'_>) -> io::Result<Vec<u8>> {
    serde_json::to_vec(verslag).map_err(io::Error::other)
}

/// Schrijft de bytes in één keer en alles-of-niets naar `verslag.json` in `map` (de map van de
/// opname): een crash laat het vorige verslag heel.
///
/// # Errors
///
/// Geeft een I/O-fout als de map niet beschrijfbaar is of de schijf vol.
pub fn schrijf_bytes(map: &Path, bytes: &[u8]) -> io::Result<()> {
    schrijf_atomair(&map.join(BESTAND), bytes)
}

/// Serialiseert en schrijft het verslag in één stap ([`naar_bytes`] en [`schrijf_bytes`]).
///
/// # Errors
///
/// Zie [`schrijf_bytes`].
pub fn schrijf(map: &Path, verslag: &Bewaren<'_>) -> io::Result<()> {
    schrijf_bytes(map, &naar_bytes(verslag)?)
}

/// Leest het verslag uit `map`. `None` als er geen leesbaar verslag staat: het bestand ontbreekt,
/// is onleesbaar of beschadigd, of komt van een nieuwere versie dan deze code kent. Wat er staat
/// is hersteld: de naam is schoon en begrensd, de markeringen volgen de regels van
/// [`Markeringen::markeer`].
#[must_use]
pub fn lees(map: &Path) -> Option<Bewaard> {
    let tekst = fs::read_to_string(map.join(BESTAND)).ok()?;
    let mut verslag: Bewaard = serde_json::from_str(&tekst).ok()?;
    if !(1..=VERSIE).contains(&verslag.versie) {
        return None;
    }
    verslag.naam = verslag.naam.as_deref().and_then(schone_verslagnaam);
    verslag.markeringen.herstel();
    Some(verslag)
}

/// Alleen het begin van het bestand voor de lijst: de versie en de samenvatting. De rest slaat
/// serde over zonder het in te lezen. `None` onder dezelfde voorwaarden als [`lees`], voor zover
/// die uit het begin of de opbouw van het bestand te zien zijn.
#[must_use]
pub fn lees_samenvatting(map: &Path) -> Option<Samenvatting> {
    #[derive(Deserialize)]
    struct Kop {
        versie: u32,
        samenvatting: Samenvatting,
    }
    let tekst = fs::read_to_string(map.join(BESTAND)).ok()?;
    let kop: Kop = serde_json::from_str(&tekst).ok()?;
    if !(1..=VERSIE).contains(&kop.versie) {
        return None;
    }
    let mut samenvatting = kop.samenvatting;
    samenvatting.naam = samenvatting.naam.as_deref().and_then(schone_verslagnaam);
    Some(samenvatting)
}

/// Zet een verslag dat niet te lezen is opzij (`verslag.kapot.json`), zodat de opname weer onder
/// "Nog uit te schrijven" staat en niet steeds als verslag in de lijst terugkomt.
pub fn zet_opzij(map: &Path) {
    let _ = fs::rename(map.join(BESTAND), map.join(OPZIJ));
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::gesprek::{Beurt, Kant, Spoorstand, Vergadersoort};

    fn gesprek() -> Gesprek {
        Gesprek {
            beurten: vec![
                Beurt {
                    kant: Kant::Jij,
                    sprekernummer: None,
                    start_ms: 0,
                    eind_ms: 1_000,
                    tekst: "Mijn mail is jan@example.com, bel 06 12345678.".to_string(),
                    naam: None,
                    verbeterd: false,
                },
                Beurt {
                    kant: Kant::Anderen,
                    sprekernummer: Some(2),
                    start_ms: 2_000,
                    eind_ms: 3_500,
                    tekst: "Dag, ik ben Jan de Vries.".to_string(),
                    naam: Some("Wethouder".to_string()),
                    verbeterd: false,
                },
            ],
            duur: Duration::from_millis(3_600),
            meldingen: vec!["let op".to_string()],
            jij: Spoorstand::Opgenomen,
            anderen: Spoorstand::Uitgevallen,
            weggelaten: 2,
            soort: Vergadersoort::Online,
        }
    }

    fn markeringen() -> Markeringen {
        let mut m = Markeringen::default();
        m.zet_vervangen(true);
        m.markeer("Jan de Vries").unwrap();
        m.haal_weg("jan@example.com");
        m
    }

    fn bewaar(map: &Path, g: &Gesprek, m: &Markeringen, naam: Option<&str>) {
        schrijf(map, &Bewaren::nieuw(g, "canary", "Canary", naam, m, None)).unwrap();
    }

    #[test]
    fn een_verslag_komt_na_schrijven_en_lezen_gelijk_terug() {
        let map = tempfile::tempdir().unwrap();
        let (g, m) = (gesprek(), markeringen());
        bewaar(map.path(), &g, &m, Some("Overleg speelveld"));
        let terug = lees(map.path()).unwrap();
        assert_eq!(terug.versie, VERSIE);
        assert!(terug.gesprek == g, "het gesprek is niet gelijk");
        assert_eq!(terug.gesprek.beurten[1].naam.as_deref(), Some("Wethouder"));
        assert_eq!(terug.gesprek.beurten[1].sprekernummer, Some(2));
        assert_eq!(terug.gesprek.duur, Duration::from_millis(3_600));
        assert_eq!(terug.model_id, "canary");
        assert_eq!(terug.model, "Canary");
        assert_eq!(terug.naam.as_deref(), Some("Overleg speelveld"));
        // De markeringen: Vervangen, het zelf gemarkeerde stuk en het weggehaalde gegeven.
        assert!(terug.markeringen == m);
        assert!(terug.markeringen.vervangen());
        let tekst = &g.beurten[0].tekst;
        assert_eq!(
            terug.markeringen.voor_uitvoer(tekst),
            m.voor_uitvoer(tekst),
            "dezelfde plaatsvervangers"
        );
        assert!(terug.markeringen.voor_uitvoer(tekst).contains("[TELEFOON]"));
        // Geen tijdelijk bestand achtergebleven.
        let namen: Vec<_> = fs::read_dir(map.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(namen, vec![BESTAND.to_string()]);
    }

    #[test]
    fn zonder_eigen_naam_blijft_de_naam_leeg() {
        let map = tempfile::tempdir().unwrap();
        bewaar(map.path(), &gesprek(), &Markeringen::default(), None);
        assert_eq!(lees(map.path()).unwrap().naam, None);
    }

    #[test]
    fn schrijven_vervangt_het_vorige_verslag() {
        let map = tempfile::tempdir().unwrap();
        let (mut g, m) = (gesprek(), Markeringen::default());
        bewaar(map.path(), &g, &m, None);
        g.zet_tekst(1, "Goedemiddag.").unwrap();
        bewaar(map.path(), &g, &m, None);
        assert_eq!(
            lees(map.path()).unwrap().gesprek.beurten[1].tekst,
            "Goedemiddag."
        );
    }

    #[test]
    fn niets_onleesbaars_of_nieuwers_geeft_een_verslag() {
        let map = tempfile::tempdir().unwrap();
        // Geen bestand.
        assert!(lees(map.path()).is_none());
        // Onleesbaar of afgebroken.
        for rommel in ["", "{", "geen json", "{\"versie\":1}", "[1,2,3]"] {
            fs::write(map.path().join(BESTAND), rommel).unwrap();
            assert!(lees(map.path()).is_none(), "{rommel:?}");
        }
        // Een hogere versie of versie 0 telt niet, ook als de rest klopt.
        bewaar(map.path(), &gesprek(), &Markeringen::default(), None);
        let json = fs::read_to_string(map.path().join(BESTAND)).unwrap();
        assert!(lees(map.path()).is_some());
        for versie in [0, VERSIE + 1, 99] {
            let aangepast = json.replacen(
                &format!("\"versie\":{VERSIE}"),
                &format!("\"versie\":{versie}"),
                1,
            );
            assert_ne!(aangepast, json);
            fs::write(map.path().join(BESTAND), aangepast).unwrap();
            assert!(lees(map.path()).is_none(), "versie {versie}");
        }
    }

    #[test]
    fn schrijven_in_een_map_die_er_niet_is_geeft_een_fout() {
        let map = tempfile::tempdir().unwrap();
        let (g, m) = (gesprek(), Markeringen::default());
        let resultaat = schrijf(
            &map.path().join("bestaat-niet"),
            &Bewaren::nieuw(&g, "x", "x", None, &m, None),
        );
        assert!(resultaat.is_err());
    }

    #[test]
    fn de_lijst_leest_alleen_de_samenvatting_en_slaat_de_rest_over() {
        let map = tempfile::tempdir().unwrap();
        let mut g = gesprek();
        g.beurten[1].sprekernummer = Some(1);
        bewaar(
            map.path(),
            &g,
            &Markeringen::default(),
            Some("Overleg speelveld"),
        );
        let samenvatting = lees_samenvatting(map.path()).unwrap();
        assert_eq!(
            samenvatting,
            Samenvatting {
                naam: Some("Overleg speelveld".to_string()),
                soort: Vergadersoort::Online,
                duur_ms: 3_600,
                sprekers: Some(2),
            }
        );
        // De samenvatting staat vooraan, direct na de versie.
        let tekst = fs::read_to_string(map.path().join(BESTAND)).unwrap();
        assert!(
            tekst.starts_with("{\"versie\":1,\"samenvatting\":{"),
            "{tekst}"
        );
        // Een afgebroken bestand of een te nieuwe versie telt ook voor de lijst niet.
        fs::write(map.path().join(BESTAND), &tekst[..tekst.len() - 5]).unwrap();
        assert!(lees_samenvatting(map.path()).is_none());
        fs::write(
            map.path().join(BESTAND),
            tekst.replacen("\"versie\":1", "\"versie\":9", 1),
        )
        .unwrap();
        assert!(lees_samenvatting(map.path()).is_none());
        assert!(lees_samenvatting(&map.path().join("bestaat-niet")).is_none());
    }

    #[test]
    fn een_met_de_hand_bewerkt_bestand_wordt_bij_het_lezen_hersteld() {
        let map = tempfile::tempdir().unwrap();
        let mut m = Markeringen::default();
        m.markeer("Jan de Vries").unwrap();
        bewaar(map.path(), &gesprek(), &m, Some("x"));
        let tekst = fs::read_to_string(map.path().join(BESTAND)).unwrap();
        let lang = "a".repeat(150);
        let kapot = tekst
            .replace(
                "\"naam\":\"x\"",
                &format!("\"naam\":\"  Naam \\n  met   {}  \"", "b".repeat(150)),
            )
            .replace(
                "\"eigen\":[\"jan de vries\"]",
                &format!("\"eigen\":[\"  Jan   DE Vries \",\"x\",\"{lang}\",\"\"]"),
            )
            .replace("\"genegeerd\":[]", "\"genegeerd\":[\" BSN  12 \",\"   \"]");
        assert_ne!(kapot, tekst);
        fs::write(map.path().join(BESTAND), kapot).unwrap();
        let hersteld = lees(map.path()).unwrap();
        // De naam is schoon en begrensd (de samenvatting in de lijst ook).
        let naam = hersteld.naam.unwrap();
        assert!(naam.starts_with("Naam met bbb"), "{naam}");
        assert_eq!(naam.chars().count(), 100);
        assert_eq!(
            lees_samenvatting(map.path())
                .unwrap()
                .naam
                .unwrap()
                .chars()
                .count(),
            100
        );
        // De markeringen: genormaliseerd, te korte, te lange en lege eigen stukken zijn weg.
        assert_eq!(
            hersteld.markeringen.eigen(),
            vec!["jan de vries".to_string()]
        );
        let genegeerd_weg = hersteld.markeringen.vondsten("bsn 12");
        assert!(genegeerd_weg.is_empty());
    }

    #[test]
    fn een_verslag_dat_niet_te_lezen_is_gaat_opzij() {
        let map = tempfile::tempdir().unwrap();
        fs::write(map.path().join(BESTAND), "{ kapot").unwrap();
        zet_opzij(map.path());
        assert!(!map.path().join(BESTAND).exists());
        assert!(map.path().join(OPZIJ).is_file());
        // Zonder bestand gebeurt er niets.
        zet_opzij(&map.path().join("bestaat-niet"));
    }

    #[test]
    fn een_verslag_met_en_zonder_verbeterd_is_te_lezen() {
        let map = tempfile::tempdir().unwrap();
        let mut g = gesprek();
        g.zet_tekst(1, "Dag, ik ben Jan de Fries.").unwrap();
        bewaar(map.path(), &g, &markeringen(), None);
        let terug = lees(map.path()).unwrap();
        assert!(terug.gesprek.beurten[1].verbeterd);
        assert!(!terug.gesprek.beurten[0].verbeterd);
        // Een bestand van voor ADR 0023 heeft het veld niet: dan is niets verbeterd.
        let tekst = fs::read_to_string(map.path().join(BESTAND)).unwrap();
        let oud = tekst
            .replace(",\"verbeterd\":true", "")
            .replace(",\"verbeterd\":false", "");
        assert!(!oud.contains("verbeterd"));
        fs::write(map.path().join(BESTAND), oud).unwrap();
        let terug = lees(map.path()).unwrap();
        assert_eq!(terug.gesprek.verbeterde_regels(), 0);
        assert_eq!(terug.gesprek.beurten[1].tekst, "Dag, ik ben Jan de Fries.");
    }
}
