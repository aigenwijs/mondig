//! Een uitgeschreven vergadering bewaren naast haar opname (ADR 0022).
//!
//! Het verslag staat als `verslag.json` in de map van de opname. Zo valt het vanzelf onder
//! dezelfde bewaartermijn en hetzelfde wismechanisme (`opname::vergadering::wis_verlopen`) als
//! het geluid, en gaat "Verslag wissen" met één `wis` over beide.
//!
//! Wat erin staat: de versie van het bestand, het gesprek (beurten met tekst, tijden, kant,
//! spreker en naam), het model waarmee het is uitgeschreven, de velden van de bestandsnaam
//! die de gebruiker invulde (klant, onderwerp en datum), de markeringen van persoonsgegevens, en als ze er zijn de notulen (ADR 0026, optioneel
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

/// De velden van de bestandsnaam die de gebruiker bij een verslag invulde (ADR 0022, aanvulling
/// UI-E). Ze staan plat in `verslag.json`: `klant`, `onderwerp` en `datum`. Het onderwerp heette
/// vroeger `naam`; die oude sleutel wordt bij het lezen het onderwerp. Een oudere versie van de
/// app kent de nieuwe velden niet en raakt ze kwijt als zij het verslag herschrijft.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Naamvelden {
    /// Voor wie het is; leeg valt weg uit de naam.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub klant: String,
    /// Waar het over gaat. `None`: nooit aangeraakt, dan geldt het soortwoord ("Vergadering").
    /// `Some("")`: de gebruiker wiste het, dan valt het weg.
    #[serde(default, alias = "naam", skip_serializing_if = "Option::is_none")]
    pub onderwerp: Option<String>,
    /// De datum zoals de gebruiker hem typte. `None`: de datum van het begin van het gesprek.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datum: Option<String>,
}

impl Naamvelden {
    /// Schoon en begrensd, zoals de naam van een verslag altijd was ([`schone_verslagnaam`]).
    /// Een veld dat leeg wordt, blijft een leeg veld: `Some("")` is niet hetzelfde als `None`.
    #[must_use]
    fn hersteld(self) -> Self {
        let schoon = |tekst: &str| schone_verslagnaam(tekst).unwrap_or_default();
        Self {
            klant: schoon(&self.klant),
            onderwerp: self.onderwerp.as_deref().map(schoon),
            datum: self.datum.as_deref().map(schoon),
        }
    }

    /// De titel van het verslag (keuze A van UI-E): de niet-lege waarden van Klant en Onderwerp met
    /// " · " ertussen, bijvoorbeeld "Gemeente Vlaardingen · Intake". `soortwoord` is het onderwerp
    /// zolang de gebruiker dat niet aanraakte. `None` als de gebruiker niets invulde (geen klant
    /// en het onderwerp nooit aangeraakt) of beide leeg maakte: dan blijft de titel die Mondig
    /// zelf maakt.
    #[must_use]
    pub fn titel(&self, soortwoord: &str) -> Option<String> {
        if self.klant.trim().is_empty() && self.onderwerp.is_none() {
            return None;
        }
        let onderwerp = self.onderwerp.as_deref().unwrap_or(soortwoord);
        let delen: Vec<&str> = [self.klant.as_str(), onderwerp]
            .into_iter()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .collect();
        (!delen.is_empty()).then(|| delen.join(" · "))
    }
}

/// Wat de lijst "Eerder opgenomen" van een verslag nodig heeft. Staat vooraan in het bestand,
/// naast de versie, zodat de lijst de tekst niet hoeft in te lezen ([`lees_samenvatting`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Samenvatting {
    /// Klant, onderwerp en datum van de bestandsnaam (zie [`Naamvelden`]).
    #[serde(flatten)]
    pub naamvelden: Naamvelden,
    /// Online, in de ruimte of Alleen ik.
    pub soort: Vergadersoort,
    /// De duur van het gesprek in milliseconden.
    pub duur_ms: u64,
    /// Hoeveel sprekers er zijn, als ze herkend zijn ([`aantal_sprekers`]).
    pub sprekers: Option<usize>,
    /// Het pad van het bestand dat de gebruiker het laatst van dit verslag opsloeg, voor
    /// "Tonen in Verkenner" in de lijst. Optioneel veld: een bestand van voor deze toevoeging
    /// blijft leesbaar (`default`), en een oudere versie van de app negeert het veld. Een pad dat
    /// geen geldige tekst is, wordt niet bewaard. Het pad komt nooit in een logboek.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opgeslagen: Option<String>,
}

/// Hoeveel sprekers een gesprek heeft, als ze herkend zijn; anders `None`, want "Jij en Anderen"
/// is geen telling van mensen. Het telt de verschillende namen die het verslag toont (zonder
/// verschil in hoofdletters). Een kaal "Anderen" (zonder nummer en zonder eigen naam) op een kant
/// met herkende sprekers telt niet mee; een kant zonder nummers (online Jij) wel. Dezelfde
/// telling als `aantalSprekers` in de interface.
#[must_use]
pub fn aantal_sprekers(gesprek: &Gesprek) -> Option<usize> {
    if !gesprek.beurten.iter().any(|b| b.sprekernummer.is_some()) {
        return None;
    }
    let met_nummers = |kant: Kant| {
        gesprek
            .beurten
            .iter()
            .any(|b| b.kant == kant && b.sprekernummer.is_some())
    };
    let namen: BTreeSet<String> = gesprek
        .beurten
        .iter()
        .filter(|b| b.sprekernummer.is_some() || b.naam.is_some() || !met_nummers(b.kant))
        .map(|b| b.spreker().to_lowercase())
        .collect();
    Some(namen.len())
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
    /// Klant, onderwerp en datum van de bestandsnaam.
    #[serde(flatten)]
    pub naamvelden: &'a Naamvelden,
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
        naamvelden: &'a Naamvelden,
        markeringen: &'a Markeringen,
        notulen: Option<&'a OpgeslagenNotulen>,
    ) -> Self {
        Self {
            versie: VERSIE,
            samenvatting: Samenvatting {
                naamvelden: naamvelden.clone(),
                soort: gesprek.soort,
                duur_ms: u64::try_from(gesprek.duur.as_millis()).unwrap_or(u64::MAX),
                sprekers: aantal_sprekers(gesprek),
                opgeslagen: None,
            },
            gesprek,
            model_id,
            model,
            naamvelden,
            markeringen,
            notulen,
        }
    }

    /// Onthoudt waar de gebruiker het verslag het laatst als bestand opsloeg. Wie het verslag
    /// opnieuw schrijft, geeft het bekende pad mee, zodat het nooit stil verdwijnt.
    #[must_use]
    pub fn met_opgeslagen(mut self, pad: Option<&Path>) -> Self {
        self.samenvatting.opgeslagen = pad.and_then(Path::to_str).map(str::to_string);
        self
    }
}

/// Alleen het deel van de samenvatting dat [`lees`] nodig heeft.
#[derive(Default, Deserialize)]
struct SamenvattingPad {
    #[serde(default)]
    opgeslagen: Option<String>,
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
    /// Klant, onderwerp en datum van de bestandsnaam, schoongemaakt en begrensd. Een bestand van
    /// voor UI-E heeft `naam`; dat is nu het onderwerp.
    #[serde(flatten)]
    pub naamvelden: Naamvelden,
    /// De markeringen van persoonsgegevens, hersteld tot wat [`Markeringen::markeer`] toestaat.
    pub markeringen: Markeringen,
    /// De notulen, als die er zijn. Ontbreekt het veld (een verslag van voor de notulen), dan
    /// `None`. Notulen die deze versie niet kan lezen, komen terug als
    /// [`OpgeslagenNotulen::Onleesbaar`] en gaan zo ook weer naar schijf.
    #[serde(default)]
    pub notulen: Option<OpgeslagenNotulen>,
    /// Zie [`Samenvatting::opgeslagen`]; lees het met [`Bewaard::opgeslagen`].
    #[serde(default)]
    samenvatting: SamenvattingPad,
}

impl Bewaard {
    /// Het pad van het bestand dat de gebruiker het laatst van dit verslag opsloeg, als het
    /// bekend is. Of het er nog staat, weet dit niet.
    #[must_use]
    pub fn opgeslagen(&self) -> Option<&Path> {
        self.samenvatting.opgeslagen.as_deref().map(Path::new)
    }
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
/// is hersteld: de velden van de naam zijn schoon en begrensd, de markeringen volgen de regels van
/// [`Markeringen::markeer`].
#[must_use]
pub fn lees(map: &Path) -> Option<Bewaard> {
    let tekst = fs::read_to_string(map.join(BESTAND)).ok()?;
    let mut verslag: Bewaard = serde_json::from_str(&tekst).ok()?;
    if !(1..=VERSIE).contains(&verslag.versie) {
        return None;
    }
    verslag.naamvelden = std::mem::take(&mut verslag.naamvelden).hersteld();
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
    samenvatting.naamvelden = std::mem::take(&mut samenvatting.naamvelden).hersteld();
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

    /// Naamvelden met alleen een onderwerp, zoals een verslag met een eigen naam vroeger had.
    fn onderwerp(tekst: Option<&str>) -> Naamvelden {
        Naamvelden {
            onderwerp: tekst.map(str::to_string),
            ..Naamvelden::default()
        }
    }

    fn bewaar(map: &Path, g: &Gesprek, m: &Markeringen, velden: &Naamvelden) {
        schrijf(map, &Bewaren::nieuw(g, "canary", "Canary", velden, m, None)).unwrap();
    }

    #[test]
    fn een_verslag_komt_na_schrijven_en_lezen_gelijk_terug() {
        let map = tempfile::tempdir().unwrap();
        let (g, m) = (gesprek(), markeringen());
        let velden = Naamvelden {
            klant: "Gemeente Vlaardingen".to_string(),
            onderwerp: Some("Overleg speelveld".to_string()),
            datum: Some("2026-10-30".to_string()),
        };
        bewaar(map.path(), &g, &m, &velden);
        let terug = lees(map.path()).unwrap();
        assert_eq!(terug.versie, VERSIE);
        assert!(terug.gesprek == g, "het gesprek is niet gelijk");
        assert_eq!(terug.gesprek.beurten[1].naam.as_deref(), Some("Wethouder"));
        assert_eq!(terug.gesprek.beurten[1].sprekernummer, Some(2));
        assert_eq!(terug.gesprek.duur, Duration::from_millis(3_600));
        assert_eq!(terug.model_id, "canary");
        assert_eq!(terug.model, "Canary");
        assert_eq!(terug.naamvelden, velden);
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
    fn zonder_eigen_velden_blijven_ze_leeg_en_staan_ze_niet_in_het_bestand() {
        let map = tempfile::tempdir().unwrap();
        bewaar(
            map.path(),
            &gesprek(),
            &Markeringen::default(),
            &Naamvelden::default(),
        );
        assert_eq!(lees(map.path()).unwrap().naamvelden, Naamvelden::default());
        let tekst = fs::read_to_string(map.path().join(BESTAND)).unwrap();
        for sleutel in ["\"klant\"", "\"onderwerp\"", "\"datum\""] {
            assert!(!tekst.contains(sleutel), "{sleutel} in {tekst}");
        }
    }

    #[test]
    fn een_gewist_onderwerp_is_iets_anders_dan_een_onderwerp_dat_nooit_is_aangeraakt() {
        let map = tempfile::tempdir().unwrap();
        let (g, m) = (gesprek(), Markeringen::default());
        bewaar(map.path(), &g, &m, &onderwerp(Some("")));
        assert_eq!(
            lees(map.path()).unwrap().naamvelden.onderwerp.as_deref(),
            Some("")
        );
        bewaar(map.path(), &g, &m, &onderwerp(None));
        assert_eq!(lees(map.path()).unwrap().naamvelden.onderwerp, None);
    }

    #[test]
    fn een_oude_naam_wordt_het_onderwerp() {
        let map = tempfile::tempdir().unwrap();
        let (g, m) = (gesprek(), Markeringen::default());
        bewaar(map.path(), &g, &m, &onderwerp(Some("Overleg speelveld")));
        // Zoals een verslag van voor UI-E: de sleutel heet `naam`, ook in de samenvatting.
        let tekst = fs::read_to_string(map.path().join(BESTAND)).unwrap();
        let oud = tekst.replace("\"onderwerp\"", "\"naam\"");
        assert_ne!(oud, tekst);
        fs::write(map.path().join(BESTAND), &oud).unwrap();
        let terug = lees(map.path()).unwrap();
        assert_eq!(
            terug.naamvelden.onderwerp.as_deref(),
            Some("Overleg speelveld")
        );
        assert_eq!(terug.naamvelden.klant, "");
        assert_eq!(terug.naamvelden.datum, None);
        assert_eq!(
            lees_samenvatting(map.path())
                .unwrap()
                .naamvelden
                .onderwerp
                .as_deref(),
            Some("Overleg speelveld")
        );
        // Een oude naam die `null` was (geen eigen naam) blijft "nooit aangeraakt".
        let leeg = oud.replace("\"naam\":\"Overleg speelveld\"", "\"naam\":null");
        fs::write(map.path().join(BESTAND), leeg).unwrap();
        assert_eq!(lees(map.path()).unwrap().naamvelden.onderwerp, None);
    }

    #[test]
    fn de_titel_volgt_klant_en_onderwerp() {
        let velden = |klant: &str, onderwerp: Option<&str>| Naamvelden {
            klant: klant.to_string(),
            onderwerp: onderwerp.map(str::to_string),
            datum: None,
        };
        // Beide ingevuld: met een middenpunt ertussen.
        assert_eq!(
            velden("Gemeente Vlaardingen", Some("Intake")).titel("Vergadering"),
            Some("Gemeente Vlaardingen · Intake".to_string())
        );
        // Alleen een klant: het onderwerp is dan het soortwoord.
        assert_eq!(
            velden("Gemeente Vlaardingen", None).titel("Vergadering"),
            Some("Gemeente Vlaardingen · Vergadering".to_string())
        );
        // Alleen een onderwerp.
        assert_eq!(
            velden("", Some("Overleg speelveld")).titel("Vergadering"),
            Some("Overleg speelveld".to_string())
        );
        // Niets ingevuld, of alles leeg gemaakt: de titel die Mondig zelf maakt.
        assert_eq!(velden("", None).titel("Vergadering"), None);
        assert_eq!(velden("  ", Some("")).titel("Vergadering"), None);
        // De klant zonder onderwerp (gewist).
        assert_eq!(
            velden("Gemeente", Some("")).titel("Notitie"),
            Some("Gemeente".to_string())
        );
    }

    #[test]
    fn schrijven_vervangt_het_vorige_verslag() {
        let map = tempfile::tempdir().unwrap();
        let (mut g, m) = (gesprek(), Markeringen::default());
        bewaar(map.path(), &g, &m, &Naamvelden::default());
        g.zet_tekst(1, "Goedemiddag.").unwrap();
        bewaar(map.path(), &g, &m, &Naamvelden::default());
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
        bewaar(
            map.path(),
            &gesprek(),
            &Markeringen::default(),
            &Naamvelden::default(),
        );
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
            &Bewaren::nieuw(&g, "x", "x", &Naamvelden::default(), &m, None),
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
            &onderwerp(Some("Overleg speelveld")),
        );
        let samenvatting = lees_samenvatting(map.path()).unwrap();
        assert_eq!(
            samenvatting,
            Samenvatting {
                naamvelden: onderwerp(Some("Overleg speelveld")),
                soort: Vergadersoort::Online,
                duur_ms: 3_600,
                sprekers: Some(2),
                opgeslagen: None,
            }
        );
        // Anderen zonder nummer is geen extra spreker.
        let mut zonder = g.clone();
        zonder.beurten.push(Beurt {
            sprekernummer: None,
            naam: None,
            ..g.beurten[1].clone()
        });
        assert_eq!(aantal_sprekers(&zonder), Some(2));
        // Zonder nummer maar met een eigen naam ("Jan") is wel een spreker; dezelfde naam als
        // een andere spreker niet nog eens.
        zonder.beurten.push(Beurt {
            naam: Some("Jan".to_string()),
            ..zonder.beurten[2].clone()
        });
        assert_eq!(aantal_sprekers(&zonder), Some(3));
        zonder.beurten.push(Beurt {
            naam: Some("wethouder".to_string()),
            ..zonder.beurten[2].clone()
        });
        assert_eq!(aantal_sprekers(&zonder), Some(3));
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
    fn het_opgeslagen_pad_gaat_mee_en_een_oud_bestand_zonder_veld_blijft_leesbaar() {
        #[derive(Deserialize)]
        #[expect(dead_code, reason = "alleen om te lezen zoals een oude versie")]
        struct OudeSamenvatting {
            naam: Option<String>,
            soort: Vergadersoort,
            duur_ms: u64,
            sprekers: Option<usize>,
        }
        #[derive(Deserialize)]
        #[expect(dead_code, reason = "alleen om te lezen zoals een oude versie")]
        struct OudeKop {
            versie: u32,
            samenvatting: OudeSamenvatting,
        }

        let map = tempfile::tempdir().unwrap();
        let (g, m) = (gesprek(), Markeringen::default());
        // Zonder pad: het veld staat niet in het bestand, en oud en nieuw lezen hetzelfde.
        bewaar(map.path(), &g, &m, &Naamvelden::default());
        let oud = fs::read_to_string(map.path().join(BESTAND)).unwrap();
        assert!(!oud.contains("opgeslagen"), "{oud}");
        assert!(lees(map.path()).unwrap().opgeslagen().is_none());
        assert!(lees_samenvatting(map.path()).unwrap().opgeslagen.is_none());
        // Met pad: terug in de lijst en bij het openen.
        let pad = map.path().join("Verslag 2026-11-10.docx");
        schrijf(
            map.path(),
            &Bewaren::nieuw(&g, "canary", "Canary", &Naamvelden::default(), &m, None)
                .met_opgeslagen(Some(&pad)),
        )
        .unwrap();
        assert_eq!(lees(map.path()).unwrap().opgeslagen(), Some(pad.as_path()));
        assert_eq!(
            lees_samenvatting(map.path()).unwrap().opgeslagen.as_deref(),
            pad.to_str()
        );
        // De rest van het verslag verandert er niet door.
        assert!(lees(map.path()).unwrap().gesprek == g);
        // Het pad blijft vooraan, bij de samenvatting, zodat de lijst de tekst niet leest.
        let nieuw = fs::read_to_string(map.path().join(BESTAND)).unwrap();
        assert!(
            nieuw.starts_with("{\"versie\":1,\"samenvatting\":{"),
            "{nieuw}"
        );
        // Een oudere app negeert het veld: dezelfde structuur als de oude, plus één sleutel.
        assert!(serde_json::from_str::<OudeKop>(&nieuw).is_ok());
    }

    #[test]
    fn een_met_de_hand_bewerkt_bestand_wordt_bij_het_lezen_hersteld() {
        let map = tempfile::tempdir().unwrap();
        let mut m = Markeringen::default();
        m.markeer("Jan de Vries").unwrap();
        bewaar(map.path(), &gesprek(), &m, &onderwerp(Some("x")));
        let tekst = fs::read_to_string(map.path().join(BESTAND)).unwrap();
        let lang = "a".repeat(150);
        let kapot = tekst
            .replace(
                "\"onderwerp\":\"x\"",
                &format!("\"onderwerp\":\"  Naam \\n  met   {}  \"", "b".repeat(150)),
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
        let naam = hersteld.naamvelden.onderwerp.unwrap();
        assert!(naam.starts_with("Naam met bbb"), "{naam}");
        assert_eq!(naam.chars().count(), 100);
        assert_eq!(
            lees_samenvatting(map.path())
                .unwrap()
                .naamvelden
                .onderwerp
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
        bewaar(map.path(), &g, &markeringen(), &Naamvelden::default());
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
