//! De bewaakte map (fase 4-D): de logica om nieuwe opnames in een map op te merken, zonder
//! app en zonder tijd van de klok. De app roept [`scan`] aan op een vast ritme, geeft de
//! uitkomst aan [`Stilstand`] en krijgt terug welke bestanden klaar zijn om uit te schrijven.
//!
//! - **Pas als het bestand stilstaat.** `OneDrive` en Teams schrijven een opname in stukken.
//!   [`Stilstand`] geeft een bestand pas af als grootte en wijzigtijd een poos gelijk bleven;
//!   een bestand dat een ander programma nog open houdt (niet te lezen, zie [`is_leesbaar`])
//!   telt niet mee. [`scan`] opent zelf geen bestanden: alleen de mapgegevens, zodat een
//!   netwerkmap met duizenden opnames elke ronde goedkoop blijft.
//! - **Alleen formaten die Bestanden leest** ([`audio::EXTENSIES`]), nooit verborgen of
//!   tijdelijke bestanden, nooit een lege kop, en nooit een uitgesloten map (de map met de
//!   eigen transcripten, de eigen gegevens).
//! - **Onthouden wat gedaan is** in een [`Register`]: pad, grootte en wijzigtijd, zonder
//!   inhoud, zodat een herstart niets dubbel uitschrijft. Wijzigt een bestand, dan is het
//!   een nieuw bestand; hernoemd (zelfde grootte en wijzigtijd) niet. Het register wordt
//!   opgeruimd van bestanden die weg zijn, maar alleen na een volledige lezing van de map.
//! - **Pollen in plaats van `notify`.** Eén map die elke paar seconden wordt gelezen kost
//!   niets, werkt ook op `OneDrive` en netwerkmappen (waar bestandsmeldingen onbetrouwbaar zijn)
//!   en heeft geen extra afhankelijkheid nodig; `notify` is bovendien CC0-1.0, een licentie
//!   die `deny.toml` niet toelaat, en nog een release-candidate.
//!
//! Alles hier raakt de inhoud van een bestand nooit aan: alleen namen, groottes en tijden.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::audio::EXTENSIES;

/// Zo lang moeten grootte en wijzigtijd gelijk blijven voordat een bestand klaar is.
pub const STILSTAND: Duration = Duration::from_secs(8);

/// Hoe vaak de app de map hoort te lezen.
pub const RITME: Duration = Duration::from_secs(2);

/// Zo diep gaat [`scan`] in submappen.
const MAX_DIEPTE: usize = 6;

/// Zoveel bestanden telt [`scan`] hooguit. Wat daarboven staat, wordt niet gezien: de app
/// meldt dat in Instellingen ([`Scan::afgekapt`]). Het [`Register`] heeft geen eigen grens en
/// wordt opgeruimd, dus het register kan nooit eerder volraken dan de scan.
pub const MAX_BESTANDEN: usize = 10_000;

/// De formaten van de transcripten die Mondig zelf maakt.
/// Houd gelijk met `bewaren::Formaat::ALLE` (de test hieronder controleert dat).
pub const TRANSCRIPT_EXTENSIES: &[&str] =
    &["docx", "pdf", "odt", "txt", "md", "html", "srt", "vtt"];

/// Hoe een bestand er op een moment uitziet: genoeg om te zien of het veranderd is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Handtekening {
    /// De grootte in bytes.
    pub grootte: u64,
    /// De wijzigtijd, in milliseconden sinds 1970.
    pub gewijzigd_ms: u64,
}

impl Handtekening {
    /// De handtekening van een bestand, of een fout als het er niet (meer) is.
    ///
    /// # Errors
    ///
    /// Geeft de fout van het besturingssysteem als de gegevens niet te lezen zijn.
    pub fn van(pad: &Path) -> io::Result<Self> {
        Ok(Self::uit_gegevens(&fs::metadata(pad)?))
    }

    /// De handtekening uit gegevens die je al hebt (bijvoorbeeld van een mapregel).
    #[must_use]
    pub fn uit_gegevens(gegevens: &fs::Metadata) -> Self {
        let gewijzigd_ms = gegevens
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        Self {
            grootte: gegevens.len(),
            gewijzigd_ms,
        }
    }
}

/// De eigen gegevensmap van Mondig (modellen, tijdelijke vergaderopnames, dit register). Nooit
/// bewaakt: wat Mondig daar zelf neerzet, is geen nieuwe opname.
#[must_use]
pub fn gegevensmap() -> Option<PathBuf> {
    dirs::data_local_dir().map(|basis| basis.join(crate::model::IDENTIFIER))
}

/// Het bestand van het [`Register`], in de gegevensmap.
#[must_use]
pub fn registerbestand() -> Option<PathBuf> {
    gegevensmap().map(|map| map.join("bewaakte-map.json"))
}

/// Ligt `pad` in `map` (of is het dezelfde map)? Op Windows en macOS zonder onderscheid
/// tussen hoofdletters en kleine letters, want daar zijn `C:\Docs` en `c:\docs` dezelfde map.
#[must_use]
pub fn ligt_in(pad: &Path, map: &Path) -> bool {
    let delen = |p: &Path| -> Vec<String> {
        p.components()
            .map(|c| {
                let tekst = c.as_os_str().to_string_lossy();
                if cfg!(any(windows, target_os = "macos")) {
                    tekst.to_lowercase()
                } else {
                    tekst.into_owned()
                }
            })
            .collect()
    };
    let (pad, map) = (delen(pad), delen(map));
    pad.len() >= map.len() && pad[..map.len()] == map[..]
}

/// Is dit een bestand dat Bestanden leest, en geen verborgen of tijdelijk bestand?
///
/// Een bestandsnaam met een punt vooraan, `~$` vooraan (Office) of de extensie van een
/// formaat dat Bestanden niet leest, valt af. Een transcript dat Mondig zelf maakte heeft
/// altijd een tekstformaat en valt dus ook af.
#[must_use]
pub fn is_kandidaat(pad: &Path) -> bool {
    let Some(naam) = pad.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if naam.starts_with('.') || naam.starts_with("~$") {
        return false;
    }
    pad.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIES.contains(&e.to_lowercase().as_str()))
}

/// Kan het bestand gelezen worden? Een opname die Teams of `OneDrive` nog open houdt, is dat
/// op Windows vaak niet. De app roept dit alleen aan voor bestanden die het register nog niet
/// kent.
#[must_use]
pub fn is_leesbaar(pad: &Path) -> bool {
    fs::File::open(pad).is_ok()
}

/// Wat [`scan`] vond.
#[derive(Debug, Default)]
pub struct Scan {
    /// De bestanden die een opname kunnen zijn (formaat, niet leeg), op volgorde van pad.
    pub bestanden: Vec<(PathBuf, Handtekening)>,
    /// Is de hele map gelezen? Niet als een map of een regel niet te lezen was (netwerk weg,
    /// geen rechten) of de lezing werd afgekapt. Alleen een volledige lezing mag het register
    /// opruimen.
    pub volledig: bool,
    /// Stonden er meer bestanden dan [`MAX_BESTANDEN`]? Het teveel is niet gezien.
    pub afgekapt: bool,
}

/// Leest de map (en op verzoek de submappen) en geeft elk bestand dat een opname kan zijn: een
/// formaat dat Bestanden leest, niet leeg, en niet in een uitgesloten map. Verborgen mappen
/// (punt vooraan) en snelkoppelingen naar mappen worden niet ingegaan. Opent geen bestanden.
#[must_use]
pub fn scan(map: &Path, submappen: bool, uitgesloten: &[PathBuf]) -> Scan {
    scan_met_grens(map, submappen, uitgesloten, MAX_BESTANDEN)
}

/// [`scan`] met een eigen grens voor het aantal bestanden.
#[must_use]
pub fn scan_met_grens(map: &Path, submappen: bool, uitgesloten: &[PathBuf], grens: usize) -> Scan {
    let mut uitkomst = Scan {
        volledig: true,
        ..Scan::default()
    };
    let mut te_doen = vec![(map.to_path_buf(), 0_usize)];
    'mappen: while let Some((huidig, diepte)) = te_doen.pop() {
        if uitgesloten.iter().any(|u| ligt_in(&huidig, u)) {
            continue;
        }
        let Ok(inhoud) = fs::read_dir(&huidig) else {
            uitkomst.volledig = false;
            continue;
        };
        for item in inhoud {
            let Ok(item) = item else {
                uitkomst.volledig = false;
                continue;
            };
            let Ok(soort) = item.file_type() else {
                uitkomst.volledig = false;
                continue;
            };
            let pad = item.path();
            if soort.is_dir() {
                let verborgen = item.file_name().to_string_lossy().starts_with('.');
                if submappen && diepte < MAX_DIEPTE && !verborgen {
                    te_doen.push((pad, diepte + 1));
                }
            } else if soort.is_file() && is_kandidaat(&pad) {
                let Ok(gegevens) = item.metadata() else {
                    uitkomst.volledig = false;
                    continue;
                };
                let handtekening = Handtekening::uit_gegevens(&gegevens);
                if handtekening.grootte == 0 {
                    continue;
                }
                if uitkomst.bestanden.len() >= grens {
                    uitkomst.afgekapt = true;
                    uitkomst.volledig = false;
                    break 'mappen;
                }
                uitkomst.bestanden.push((pad, handtekening));
            }
        }
    }
    uitkomst.bestanden.sort_by(|a, b| a.0.cmp(&b.0));
    uitkomst
}

/// Wat [`Stilstand`] van één bestand weet.
struct Waarneming {
    handtekening: Handtekening,
    /// Sinds wanneer (op de klok van de app) het bestand er zo uitziet.
    sinds: Duration,
    afgegeven: bool,
}

/// Houdt bij hoe lang elk bestand er hetzelfde uitziet en geeft het af zodra het stilstaat.
/// Zonder klok: de app geeft bij elke aanroep de tijd sinds een vast beginpunt mee.
pub struct Stilstand {
    stil_na: Duration,
    gezien: HashMap<PathBuf, Waarneming>,
}

impl Stilstand {
    /// Een nieuwe bewaker die een bestand afgeeft als het `stil_na` niet veranderde.
    #[must_use]
    pub fn nieuw(stil_na: Duration) -> Self {
        Self {
            stil_na,
            gezien: HashMap::new(),
        }
    }

    /// Verwerkt een nieuwe lezing van de map op tijdstip `nu` en geeft de bestanden die nu
    /// voor het eerst stilstaan. Een bestand dat verandert (groeit, of wordt vervangen)
    /// begint opnieuw te tellen, ook als het al was afgegeven; een bestand dat uit de map
    /// verdwijnt, wordt vergeten.
    pub fn kijk(
        &mut self,
        nu: Duration,
        gevonden: &[(PathBuf, Handtekening)],
    ) -> Vec<(PathBuf, Handtekening)> {
        self.gezien
            .retain(|pad, _| gevonden.iter().any(|(p, _)| p == pad));
        let mut klaar = Vec::new();
        for (pad, handtekening) in gevonden {
            match self.gezien.get_mut(pad) {
                Some(w) if w.handtekening == *handtekening => {
                    if !w.afgegeven && nu.saturating_sub(w.sinds) >= self.stil_na {
                        w.afgegeven = true;
                        klaar.push((pad.clone(), *handtekening));
                    }
                }
                _ => {
                    self.gezien.insert(
                        pad.clone(),
                        Waarneming {
                            handtekening: *handtekening,
                            sinds: nu,
                            afgegeven: false,
                        },
                    );
                }
            }
        }
        klaar
    }

    /// Vergeet één bestand, zodat het opnieuw gaat tellen (bijvoorbeeld omdat toevoegen aan de
    /// wachtrij niet lukte en het later opnieuw moet).
    pub fn vergeet(&mut self, pad: &Path) {
        self.gezien.remove(pad);
    }

    /// Vergeet alles, bijvoorbeeld omdat de bewaking uitstaat of de map is gewijzigd.
    pub fn reset(&mut self) {
        self.gezien.clear();
    }

    /// Meldt hoe lang de bewaker tussen twee lezingen sliep. Na de slaapstand van de computer
    /// kan de klok die sprong hebben meegeteld, en dan zou een bestand dat tijdens die slaap
    /// half gekopieerd werd meteen "al acht seconden stil" lijken. Duurde de pauze meer dan
    /// twee keer [`RITME`], dan begint alles opnieuw te tellen.
    pub fn pauze(&mut self, gemeten: Duration) {
        if gemeten > RITME * 2 {
            self.reset();
        }
    }
}

/// De sleutel van een pad in het register: op Windows en macOS verschilt hoofdlettergebruik
/// niet.
fn sleutel(pad: &Path) -> String {
    let tekst = pad.to_string_lossy();
    if cfg!(any(windows, target_os = "macos")) {
        tekst.to_lowercase()
    } else {
        tekst.into_owned()
    }
}

/// Het register van bestanden die al zijn uitgeschreven (of overgeslagen op verzoek van de
/// gebruiker), bewaard als klein JSON-bestand zonder inhoud van de opnames.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Register {
    /// Voor welke instelling het register geldt (map en submappen). Wijzigt die, dan geldt
    /// het register als nieuw: wat er dan al staat, telt als bestaand en wordt niet
    /// uitgeschreven.
    #[serde(default)]
    instelling: Option<String>,
    /// Per pad hoe het bestand er toen uitzag.
    #[serde(default)]
    gedaan: HashMap<String, Handtekening>,
    /// Hoe vaak elke handtekening voorkomt, om een hernoemd bestand te herkennen.
    #[serde(skip)]
    tekens: HashMap<Handtekening, usize>,
}

impl Register {
    /// Leest het register. Ontbreekt het of is het onleesbaar, dan een leeg register: de
    /// instelling staat dan ook niet meer vast, dus de eerstvolgende lezing van de map telt
    /// als beginstand (zie [`Register::zet_instelling`]).
    #[must_use]
    pub fn laad(pad: &Path) -> Self {
        let mut register: Self = fs::read(pad)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        register.herbouw_tekens();
        register
    }

    fn herbouw_tekens(&mut self) {
        self.tekens.clear();
        for handtekening in self.gedaan.values() {
            *self.tekens.entry(*handtekening).or_default() += 1;
        }
    }

    /// Bewaart het register: eerst naast het bestand, dan hernoemd, zodat een crash nooit
    /// een half bestand laat staan.
    ///
    /// # Errors
    ///
    /// Geeft de fout van het besturingssysteem als schrijven of hernoemen niet lukt.
    pub fn bewaar(&self, pad: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec(self).map_err(io::Error::other)?;
        let tijdelijk = pad.with_extension("json.tmp");
        fs::write(&tijdelijk, bytes)?;
        fs::rename(&tijdelijk, pad)
    }

    /// Is dit bestand, zoals het er nu uitziet, al gedaan? Ook als het onder een andere naam
    /// of in een andere map staat (zelfde grootte en wijzigtijd): hernoemen maakt een bestand
    /// niet nieuw.
    #[must_use]
    pub fn bevat(&self, pad: &Path, handtekening: Handtekening) -> bool {
        self.gedaan.get(&sleutel(pad)) == Some(&handtekening)
            || self.tekens.contains_key(&handtekening)
    }

    /// Onthoudt een bestand als gedaan. Een eerdere versie van hetzelfde pad vervalt.
    pub fn noteer(&mut self, pad: &Path, handtekening: Handtekening) {
        if let Some(oud) = self.gedaan.insert(sleutel(pad), handtekening) {
            self.verlaag(oud);
        }
        *self.tekens.entry(handtekening).or_default() += 1;
    }

    fn verlaag(&mut self, handtekening: Handtekening) {
        if let Some(aantal) = self.tekens.get_mut(&handtekening) {
            *aantal -= 1;
            if *aantal == 0 {
                self.tekens.remove(&handtekening);
            }
        }
    }

    /// Verwerkt een lezing van de map: geeft de bestanden die nog niet gedaan zijn, neemt een
    /// hernoemd bestand over onder zijn nieuwe pad, en ruimt bij een `volledige` lezing de
    /// vermeldingen op van bestanden die er niet meer staan. Bij een onvolledige lezing (map
    /// niet bereikbaar, afgekapt) blijft alles staan: een tijdelijk weggevallen netwerkschijf
    /// mag de beginstand niet wissen. Geeft ook terug of het register veranderde (dan bewaren).
    pub fn verwerk(
        &mut self,
        bestanden: &[(PathBuf, Handtekening)],
        volledig: bool,
    ) -> (Vec<(PathBuf, Handtekening)>, bool) {
        let mut nieuw = Vec::new();
        let mut veranderd = false;
        let mut aanwezig = HashSet::with_capacity(bestanden.len());
        for (pad, handtekening) in bestanden {
            let sleutel = sleutel(pad);
            match self.gedaan.get(&sleutel) {
                Some(bekend) if bekend == handtekening => {}
                None if self.tekens.contains_key(handtekening) => {
                    self.noteer(pad, *handtekening);
                    veranderd = true;
                }
                Some(_) | None => nieuw.push((pad.clone(), *handtekening)),
            }
            aanwezig.insert(sleutel);
        }
        if volledig {
            let voor = self.gedaan.len();
            self.gedaan.retain(|pad, _| aanwezig.contains(pad));
            if self.gedaan.len() != voor {
                self.herbouw_tekens();
                veranderd = true;
            }
        }
        (nieuw, veranderd)
    }

    /// Hoeveel bestanden staan erin?
    #[must_use]
    pub fn aantal(&self) -> usize {
        self.gedaan.len()
    }

    /// Staat er een instelling vast (is de bewaking dus in gebruik)?
    #[must_use]
    pub fn heeft_instelling(&self) -> bool {
        self.instelling.is_some()
    }

    /// Geldt dit register voor deze instelling? Zo niet, dan is het een beginstand nodig.
    #[must_use]
    pub fn is_voor(&self, instelling: &str) -> bool {
        self.instelling.as_deref() == Some(instelling)
    }

    /// Legt vast voor welke instelling het register geldt, en begint leeg als dat een andere
    /// is dan voorheen.
    pub fn zet_instelling(&mut self, instelling: Option<String>) {
        if self.instelling != instelling {
            self.gedaan.clear();
            self.tekens.clear();
            self.instelling = instelling;
        }
    }
}

/// De sleutel van een instelling voor het register: de map en of de submappen meedoen.
#[must_use]
pub fn instelling_sleutel(map: &Path, submappen: bool) -> String {
    format!(
        "{}|{}",
        sleutel(map),
        if submappen { "mmap" } else { "map" }
    )
}

/// Een transcript (tekstformaat) in een map, voor de controle of een opname al een transcript heeft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcriptbestand {
    /// De bestandsnaam met extensie.
    pub naam: String,
    /// De wijzigtijd, in milliseconden sinds 1970.
    pub gewijzigd_ms: u64,
}

/// De bestanden in een tekstformaat ([`TRANSCRIPT_EXTENSIES`]) in een map, zonder submappen.
/// Of zo'n bestand echt het transcript van een opname is, beslist de app aan de hand van de
/// naamregel en de wijzigtijd: een naam die toevallig lijkt, is geen transcript.
#[must_use]
pub fn transcripten_in(map: &Path) -> Vec<Transcriptbestand> {
    let Ok(inhoud) = fs::read_dir(map) else {
        return Vec::new();
    };
    inhoud
        .filter_map(Result::ok)
        .filter_map(|item| {
            let pad = item.path();
            let is_tekst = pad
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| TRANSCRIPT_EXTENSIES.contains(&e.to_lowercase().as_str()));
            let gegevens = item.metadata().ok()?;
            (is_tekst && gegevens.is_file()).then(|| Transcriptbestand {
                naam: item.file_name().to_string_lossy().into_owned(),
                gewijzigd_ms: Handtekening::uit_gegevens(&gegevens).gewijzigd_ms,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(grootte: u64, gewijzigd_ms: u64) -> Handtekening {
        Handtekening {
            grootte,
            gewijzigd_ms,
        }
    }

    fn s(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    /// Elk formaat waarin Mondig opslaat, telt als transcript; anders ziet de bewaakte map een
    /// opgeslagen Markdown- of HTML-bestand niet en schrijft hij de opname opnieuw uit.
    #[cfg(feature = "bewaren")]
    #[test]
    fn elk_bewaarformaat_telt_als_transcript() {
        for formaat in crate::bewaren::Formaat::ALLE {
            assert!(
                TRANSCRIPT_EXTENSIES.contains(&formaat.extensie()),
                "{} ontbreekt",
                formaat.extensie()
            );
        }
        assert_eq!(
            TRANSCRIPT_EXTENSIES.len(),
            crate::bewaren::Formaat::ALLE.len()
        );
    }

    fn maak(map: &Path, naam: &str, inhoud: &[u8]) -> PathBuf {
        let pad = map.join(naam);
        if let Some(ouder) = pad.parent() {
            fs::create_dir_all(ouder).unwrap();
        }
        fs::write(&pad, inhoud).unwrap();
        pad
    }

    #[test]
    fn alleen_formaten_die_bestanden_leest_zijn_kandidaat() {
        for goed in ["a.mp3", "b.M4A", "c.wav", "d.mp4", "e.webm", "f.opus"] {
            assert!(is_kandidaat(Path::new(goed)), "{goed}");
        }
        for fout in [
            "a.docx",
            "b.txt",
            "c.pdf",
            "d.srt",
            "e.vtt",
            "f.odt",
            "g.xyz",
            "h",
            "i.mp3.crdownload",
            "i.mp3.tmp",
            ".verborgen.mp3",
            "~$opname.mp3",
        ] {
            assert!(!is_kandidaat(Path::new(fout)), "{fout}");
        }
    }

    #[test]
    fn eigen_transcripten_en_onbekende_formaten_worden_niet_gevonden() {
        let dir = tempfile::tempdir().unwrap();
        maak(dir.path(), "opname.mp3", b"geluid");
        maak(dir.path(), "2026-11-02 opname.docx", b"x");
        maak(dir.path(), "2026-11-02 opname.pdf", b"x");
        maak(dir.path(), "2026-11-02 opname.srt", b"x");
        maak(dir.path(), "notities.xyz", b"x");
        maak(dir.path(), ".verborgen.mp3", b"x");
        maak(dir.path(), "leeg.mp3", b"");
        let namen: Vec<_> = scan(dir.path(), false, &[])
            .bestanden
            .into_iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(namen, vec!["opname.mp3"]);
    }

    #[test]
    fn submappen_alleen_op_verzoek_en_uitgesloten_mappen_nooit() {
        let dir = tempfile::tempdir().unwrap();
        maak(dir.path(), "boven.mp3", b"x");
        maak(dir.path(), "onder/diep.mp3", b"x");
        maak(dir.path(), "Mondig/eigen.wav", b"x");
        maak(dir.path(), ".verstopt/geheim.mp3", b"x");
        let zonder = scan(dir.path(), false, &[]);
        assert_eq!(zonder.bestanden.len(), 1);
        assert!(zonder.volledig);
        let uitgesloten = vec![dir.path().join("Mondig")];
        let met: Vec<_> = scan(dir.path(), true, &uitgesloten)
            .bestanden
            .into_iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(met, vec!["boven.mp3", "diep.mp3"]);
    }

    #[test]
    fn een_bestand_wordt_pas_afgegeven_als_het_stilstaat() {
        let mut bewaker = Stilstand::nieuw(s(8));
        let pad = PathBuf::from("opname.mp3");
        // Groeit elke 2 seconden: nooit afgegeven.
        for (tijd, grootte) in [(0, 10), (2, 20), (4, 30), (6, 40), (8, 50), (10, 60)] {
            assert!(
                bewaker
                    .kijk(s(tijd), &[(pad.clone(), h(grootte, tijd))])
                    .is_empty(),
                "t={tijd}"
            );
        }
        // Daarna stil: pas 8 seconden na de laatste verandering.
        assert!(bewaker.kijk(s(12), &[(pad.clone(), h(60, 10))]).is_empty());
        assert!(bewaker.kijk(s(16), &[(pad.clone(), h(60, 10))]).is_empty());
        let klaar = bewaker.kijk(s(18), &[(pad.clone(), h(60, 10))]);
        assert_eq!(klaar, vec![(pad.clone(), h(60, 10))]);
        // Maar één keer.
        assert!(bewaker.kijk(s(20), &[(pad, h(60, 10))]).is_empty());
    }

    #[test]
    fn een_bestand_dat_verdwijnt_en_terugkomt_telt_opnieuw() {
        let mut bewaker = Stilstand::nieuw(s(4));
        let pad = PathBuf::from("a.mp3");
        bewaker.kijk(s(0), &[(pad.clone(), h(1, 1))]);
        // Even niet leesbaar (open in een ander programma): de teller begint opnieuw.
        bewaker.kijk(s(3), &[]);
        assert!(bewaker.kijk(s(4), &[(pad.clone(), h(1, 1))]).is_empty());
        assert!(bewaker.kijk(s(7), &[(pad.clone(), h(1, 1))]).is_empty());
        assert_eq!(bewaker.kijk(s(8), &[(pad, h(1, 1))]).len(), 1);
    }

    #[test]
    fn een_veranderd_bestand_wordt_opnieuw_afgegeven() {
        let mut bewaker = Stilstand::nieuw(s(2));
        let pad = PathBuf::from("a.mp3");
        bewaker.kijk(s(0), &[(pad.clone(), h(1, 1))]);
        assert_eq!(bewaker.kijk(s(2), &[(pad.clone(), h(1, 1))]).len(), 1);
        // Vervangen door een andere versie.
        assert!(bewaker.kijk(s(3), &[(pad.clone(), h(5, 9))]).is_empty());
        assert_eq!(bewaker.kijk(s(5), &[(pad, h(5, 9))]).len(), 1);
    }

    #[test]
    fn het_register_onthoudt_na_een_herstart_en_ziet_een_wijziging() {
        let dir = tempfile::tempdir().unwrap();
        let bestand = dir.path().join("bewaakte-map.json");
        let opname = Path::new("C:/Teams/vergadering.mp4");
        let mut register = Register::laad(&bestand);
        assert!(!register.bevat(opname, h(100, 5)));
        register.zet_instelling(Some("map".to_string()));
        register.noteer(opname, h(100, 5));
        register.bewaar(&bestand).unwrap();

        // "Herstart": opnieuw laden.
        let register = Register::laad(&bestand);
        assert!(register.is_voor("map"));
        assert!(register.bevat(opname, h(100, 5)));
        // Een andere versie van hetzelfde pad is nieuw.
        assert!(!register.bevat(opname, h(101, 5)));
        assert!(!register.bevat(opname, h(100, 6)));
        // Het bestand bevat alleen pad, grootte en tijd.
        let tekst = fs::read_to_string(&bestand).unwrap();
        assert!(tekst.to_lowercase().contains("vergadering.mp4"));
        assert!(!tekst.contains("geluid"));
    }

    #[test]
    fn een_nieuwe_instelling_begint_met_een_leeg_register() {
        let mut register = Register::default();
        register.zet_instelling(Some("a".to_string()));
        register.noteer(Path::new("x.mp3"), h(1, 1));
        register.zet_instelling(Some("a".to_string()));
        assert_eq!(register.aantal(), 1);
        register.zet_instelling(Some("b".to_string()));
        assert_eq!(register.aantal(), 0);
        assert!(!register.is_voor("a"));
    }

    #[test]
    fn een_onleesbaar_register_geeft_een_leeg_register() {
        let dir = tempfile::tempdir().unwrap();
        let pad = maak(dir.path(), "r.json", b"{ kapot");
        assert_eq!(Register::laad(&pad).aantal(), 0);
        assert_eq!(
            Register::laad(&dir.path().join("bestaat-niet.json")).aantal(),
            0
        );
    }

    fn namen(lijst: &[(PathBuf, Handtekening)]) -> Vec<String> {
        lijst
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn een_brede_map_boven_de_oude_grens_verliest_geen_beginstand() {
        // Zes duizend bestanden, meer dan de oude grens van het register (5.000).
        let bestanden: Vec<_> = (0..6_000_u64)
            .map(|n| (PathBuf::from(format!("/map/{n}.mp3")), h(n + 1, 7)))
            .collect();
        let mut register = Register::default();
        register.zet_instelling(Some("x".to_string()));
        for (pad, handtekening) in &bestanden {
            register.noteer(pad, *handtekening);
        }
        let (nieuw, _) = register.verwerk(&bestanden, true);
        assert!(
            nieuw.is_empty(),
            "{} bestaande bestanden gelden als nieuw",
            nieuw.len()
        );
        assert_eq!(register.aantal(), 6_000);
    }

    #[test]
    fn het_register_ruimt_weggehaalde_bestanden_op_maar_alleen_na_een_volledige_lezing() {
        let a = (PathBuf::from("/map/a.mp3"), h(1, 1));
        let b = (PathBuf::from("/map/b.mp3"), h(2, 2));
        let mut register = Register::default();
        register.noteer(&a.0, a.1);
        register.noteer(&b.0, b.1);

        // Een onvolledige lezing (netwerkschijf even weg): niets opruimen.
        let (nieuw, veranderd) = register.verwerk(&[], false);
        assert!(nieuw.is_empty() && !veranderd);
        assert_eq!(register.aantal(), 2);

        // Volledig en b is weg: b wordt opgeruimd, a blijft.
        let (_, veranderd) = register.verwerk(std::slice::from_ref(&a), true);
        assert!(veranderd);
        assert_eq!(register.aantal(), 1);
        assert!(register.bevat(&a.0, a.1));
        assert!(!register.bevat(&b.0, b.1));
    }

    #[test]
    fn een_hernoemd_bestand_telt_niet_opnieuw() {
        let oud = (PathBuf::from("/map/VN0001.mp3"), h(500, 9));
        let nieuwe_naam = (PathBuf::from("/map/Overleg wethouder.mp3"), h(500, 9));
        let mut register = Register::default();
        register.noteer(&oud.0, oud.1);
        let (nieuw, veranderd) = register.verwerk(std::slice::from_ref(&nieuwe_naam), true);
        assert!(nieuw.is_empty(), "een hernoemd bestand is niet nieuw");
        assert!(veranderd);
        // Het register kent het nu onder zijn nieuwe naam, en de oude naam is opgeruimd.
        assert_eq!(register.aantal(), 1);
        assert!(register.bevat(&nieuwe_naam.0, nieuwe_naam.1));
        // Een bestand met andere grootte of tijd is wel nieuw.
        let ander = (PathBuf::from("/map/Overleg wethouder.mp3"), h(501, 9));
        assert_eq!(register.verwerk(&[ander], true).0.len(), 1);
    }

    #[test]
    fn een_scan_boven_de_grens_meldt_dat_hij_is_afgekapt() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..5 {
            maak(dir.path(), &format!("{n}.mp3"), b"x");
        }
        let uitkomst = scan_met_grens(dir.path(), false, &[], 3);
        assert!(uitkomst.afgekapt && !uitkomst.volledig);
        assert_eq!(uitkomst.bestanden.len(), 3);
        let ruim = scan_met_grens(dir.path(), false, &[], 5);
        assert!(!ruim.afgekapt && ruim.volledig);
    }

    #[test]
    fn een_map_die_niet_te_lezen_is_geeft_een_onvolledige_scan() {
        let dir = tempfile::tempdir().unwrap();
        let uitkomst = scan(&dir.path().join("bestaat-niet"), false, &[]);
        assert!(!uitkomst.volledig);
        assert!(uitkomst.bestanden.is_empty());
    }

    #[test]
    fn scan_opent_geen_bestanden_en_het_register_ziet_wat_nog_gelezen_moet_worden() {
        let dir = tempfile::tempdir().unwrap();
        let pad = maak(dir.path(), "a.mp3", b"geluid");
        let uitkomst = scan(dir.path(), false, &[]);
        assert_eq!(namen(&uitkomst.bestanden), vec!["a.mp3"]);
        assert!(is_leesbaar(&pad));
        assert!(!is_leesbaar(&dir.path().join("weg.mp3")));
    }

    #[test]
    fn na_een_lange_pauze_begint_het_tellen_opnieuw() {
        let mut bewaker = Stilstand::nieuw(s(8));
        let pad = PathBuf::from("half.mp4");
        bewaker.kijk(s(0), &[(pad.clone(), h(10, 1))]);
        // De computer sliep een uur; de klok telde dat mee. Zonder reset zou het bestand nu
        // meteen "stil" zijn.
        bewaker.pauze(s(3_600));
        assert!(
            bewaker
                .kijk(s(3_600), &[(pad.clone(), h(10, 1))])
                .is_empty()
        );
        assert!(
            bewaker
                .kijk(s(3_604), &[(pad.clone(), h(10, 1))])
                .is_empty()
        );
        assert_eq!(bewaker.kijk(s(3_608), &[(pad, h(10, 1))]).len(), 1);
    }

    #[test]
    fn een_gewone_pauze_laat_de_tellers_staan() {
        let mut bewaker = Stilstand::nieuw(s(8));
        let pad = PathBuf::from("a.mp4");
        bewaker.kijk(s(0), &[(pad.clone(), h(10, 1))]);
        bewaker.pauze(RITME + Duration::from_millis(500));
        assert_eq!(bewaker.kijk(s(8), &[(pad, h(10, 1))]).len(), 1);
    }

    #[test]
    fn transcripten_met_hun_wijzigtijd_worden_gevonden() {
        let dir = tempfile::tempdir().unwrap();
        maak(dir.path(), "2026-11-02 Teams-opname.docx", b"x");
        maak(dir.path(), "Teams-opname.mp3", b"x");
        maak(dir.path(), "notitie.xyz", b"x");
        let mut gevonden = transcripten_in(dir.path());
        gevonden.sort_by(|a, b| a.naam.cmp(&b.naam));
        assert_eq!(gevonden.len(), 1);
        assert_eq!(gevonden[0].naam, "2026-11-02 Teams-opname.docx");
        assert!(gevonden[0].gewijzigd_ms > 0);
        assert!(transcripten_in(&dir.path().join("weg")).is_empty());
    }

    #[test]
    fn een_pad_in_een_map_negeert_hoofdletters_op_windows() {
        let map = Path::new("/data/Mondig");
        assert!(ligt_in(Path::new("/data/Mondig"), map));
        assert!(ligt_in(Path::new("/data/Mondig/diep/a.wav"), map));
        assert!(!ligt_in(Path::new("/data/Mondig2/a.wav"), map));
        assert!(!ligt_in(Path::new("/data"), map));
        if cfg!(any(windows, target_os = "macos")) {
            assert!(ligt_in(Path::new("/DATA/mondig/a.wav"), map));
        }
    }

    /// Een opname die in stukken wordt gekopieerd, wordt pas na afloop afgegeven. Echte
    /// bestanden en de echte klok, met een korte stilstand; `MONDIG_BEWAAK_MB` bepaalt de
    /// grootte (standaard 64 MB; de handmatige proef gebruikt enkele honderden).
    #[test]
    fn een_kopie_in_stukken_wordt_pas_na_afloop_opgepakt() {
        use std::io::Write;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Instant;

        let mb: usize = std::env::var("MONDIG_BEWAAK_MB")
            .ok()
            .and_then(|t| t.parse().ok())
            .unwrap_or(64);
        let dir = tempfile::tempdir().unwrap();
        let pad = dir.path().join("grote-opname.mp4");
        let klaar_met_kopieren = Arc::new(AtomicBool::new(false));

        let schrijver = {
            let pad = pad.clone();
            let klaar = Arc::clone(&klaar_met_kopieren);
            std::thread::spawn(move || {
                let stuk = vec![7_u8; 8 * 1024 * 1024];
                let mut bestand = fs::File::create(&pad).unwrap();
                for _ in 0..mb.div_ceil(8) {
                    bestand.write_all(&stuk).unwrap();
                    bestand.flush().unwrap();
                    // Een pauze tussen de stukken, korter dan de stilstand.
                    std::thread::sleep(Duration::from_millis(150));
                }
                drop(bestand);
                klaar.store(true, Ordering::SeqCst);
            })
        };

        let begin = Instant::now();
        let mut bewaker = Stilstand::nieuw(Duration::from_millis(600));
        let mut opgepakt_na_afloop = None;
        while begin.elapsed() < Duration::from_secs(120) {
            let schrijven_klaar = klaar_met_kopieren.load(Ordering::SeqCst);
            let gevonden = scan(dir.path(), false, &[]).bestanden;
            let klaar = bewaker.kijk(begin.elapsed(), &gevonden);
            if !klaar.is_empty() {
                opgepakt_na_afloop = Some(schrijven_klaar);
                assert_eq!(
                    klaar[0].1.grootte,
                    (mb.div_ceil(8) * 8 * 1024 * 1024) as u64
                );
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        schrijver.join().unwrap();
        assert_eq!(
            opgepakt_na_afloop,
            Some(true),
            "het bestand is opgepakt terwijl het nog groeide (of helemaal niet)"
        );
    }
}
