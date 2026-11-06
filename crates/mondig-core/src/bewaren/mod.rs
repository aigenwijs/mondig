//! Transcripten bewaren en exporteren (sprint 4-C, ADR 0017): Word (.docx), PDF voor archief
//! (PDF/A-2b), LibreOffice (.odt), platte tekst (.txt) en ondertitels (.srt en .vtt).
//!
//! Alles hier is puur: een [`Document`] gaat erin, de bytes van het bestand komen eruit. Waar
//! het bestand terechtkomt en hoe het heet, bepaalt de app met [`naam::bestandsnaam`] en de
//! map uit de instelling of de mapkiezer; de webview geeft nooit zelf een pad.
//!
//! De gegevens (datum, bron, duur, model, label en "Lokaal verwerkt") staan in elk document
//! zichtbaar bovenaan en, waar het formaat dat kent, ook in de documenteigenschappen, zodat
//! een archief ze kan lezen. Alleen SRT kent geen plek voor gegevens.
//!
//! Een [`Document`] heeft bewust geen `Debug`: de tekst mag nooit in een log.

mod kantoor;
pub mod naam;
mod pdf;
mod tekst;

pub use jiff::Zoned;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};

use crate::gesprek::{Gesprek, Vergadersoort};

/// Hoe de verwerking in de gegevens heet (het soevereiniteitslabel van de lokale engine).
pub const LOKAAL_VERWERKT: &str = "Lokaal verwerkt";

/// De taal van een document: die van de notulen (en dus van de interface, ADR 0026 punt 9) als
/// die erbij staan, anders Nederlands.
#[derive(Clone, Copy)]
pub(crate) struct Taalcode {
    /// Met land, voor Word en PDF ("nl-NL").
    pub(crate) volledig: &'static str,
    /// Alleen de taal, voor HTML en LibreOffice ("nl").
    pub(crate) taal: &'static str,
    /// Het land, voor LibreOffice ("NL").
    pub(crate) land: &'static str,
}

impl Taalcode {
    /// De taal van een document met (of zonder) notulen.
    pub(crate) fn van(notulen: Option<&Notulenblok>) -> Self {
        match notulen.map(|n| n.taal) {
            Some(crate::notulen::Taal::En) => Self {
                volledig: "en-GB",
                taal: "en",
                land: "GB",
            },
            _ => Self {
                volledig: "nl-NL",
                taal: "nl",
                land: "NL",
            },
        }
    }
}

/// De versie van Mondig, voor het veld "gemaakt met" in de documenten.
const VERSIE: &str = env!("CARGO_PKG_VERSION");

/// Een bestandsformaat waarin Mondig een transcript bewaart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Formaat {
    /// Word (.docx).
    #[default]
    Docx,
    /// PDF voor archief (PDF/A-2b).
    Pdf,
    /// LibreOffice (.odt).
    Odt,
    /// Platte tekst (.txt).
    Txt,
    /// Markdown (.md).
    Md,
    /// Een webpagina (.html), zelfstandig: zonder bestanden of verbindingen van buiten.
    Html,
    /// Ondertitels (.srt).
    Srt,
    /// Ondertitels voor het web (.vtt).
    Vtt,
}

impl Formaat {
    /// Alle formaten, in de volgorde van het menu.
    pub const ALLE: [Self; 8] = [
        Self::Docx,
        Self::Pdf,
        Self::Odt,
        Self::Txt,
        Self::Md,
        Self::Html,
        Self::Srt,
        Self::Vtt,
    ];

    /// De extensie zonder punt; ook de sleutel in de instellingen.
    #[must_use]
    pub const fn extensie(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Pdf => "pdf",
            Self::Odt => "odt",
            Self::Txt => "txt",
            Self::Md => "md",
            Self::Html => "html",
            Self::Srt => "srt",
            Self::Vtt => "vtt",
        }
    }

    /// Het formaat bij een extensie (de sleutel in de instellingen); `None` als die onbekend is.
    #[must_use]
    pub fn uit_sleutel(sleutel: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|f| f.extensie() == sleutel)
    }

    /// Ondertitels hebben altijd tijden, wat de instelling ook zegt.
    #[must_use]
    pub const fn altijd_tijden(self) -> bool {
        matches!(self, Self::Srt | Self::Vtt)
    }
}

/// Eén stuk tekst met tijdcodes, zoals het model het gaf.
pub struct Regel {
    /// Begin in milliseconden vanaf het begin van de opname.
    pub start_ms: u64,
    /// Einde in milliseconden.
    pub eind_ms: u64,
    /// De tekst.
    pub tekst: String,
    /// Wie er spreekt, bij een gesprek ("Jij", "Anderen" of een naam die de gebruiker gaf).
    /// Bewust een tekst en geen vaste keuze, zodat de gebruiker een naam kan geven. `None`
    /// bij een gewoon transcript: dan staat er geen label.
    pub spreker: Option<String>,
}

/// Een transcript met zijn gegevens, klaar om te bewaren.
pub struct Document {
    /// De naam van de opname zonder extensie, ook de titel van het document.
    pub titel: String,
    /// De oorspronkelijke bestandsnaam van de opname.
    pub bron: String,
    /// Het spraakmodel, kort ("Parakeet Ultra").
    pub model: String,
    /// Een label van de gebruiker, zoals een zaaknummer.
    pub label: Option<String>,
    /// Wanneer het transcript klaar was, in de tijdzone van de computer.
    pub moment: Zoned,
    /// De duur van de opname.
    pub duur_ms: u64,
    /// Een melding van het filter als er geen tekst is, zoals "Geen geluid gehoord."
    pub melding: Option<String>,
    /// De tekst.
    pub regels: Vec<Regel>,
}

/// Wat er mis kan gaan bij het maken van een bestand.
#[derive(Debug, thiserror::Error)]
pub enum BewaarFout {
    /// De PDF kon niet worden gemaakt of voldoet niet aan PDF/A.
    #[error("Het PDF-bestand kon niet worden gemaakt.")]
    Pdf(String),
    /// Het Word- of LibreOffice-bestand kon niet worden ingepakt.
    #[error("Het bestand kon niet worden gemaakt.")]
    Inpakken(String),
    /// Ondertitels zonder tekst: daar maakt Mondig geen bestand van.
    #[error("Er is geen tekst herkend, dus er zijn geen ondertitels om op te slaan.")]
    GeenTekst,
}

/// Notulen die vóór het verslag in een document komen (ADR 0026 punt 8). Al opgemaakt: in de
/// taal van de notulen, met de tijd van de bron bij elk punt en met de persoonsgegevens vervangen
/// zoals in het verslag (`notulen::BewaardeNotulen::als_blok`). Bewust zonder `Debug`: er staat
/// tekst in.
pub struct Notulenblok {
    /// De regels van de notulen, in volgorde.
    pub regels: Vec<Notulenregel>,
    /// De kop boven het verslag dat erna komt ("Verslag").
    pub verslagkop: String,
    /// De taal van de notulen; die bepaalt ook de taal van het document.
    pub taal: crate::notulen::Taal,
}

/// Eén regel van een [`Notulenblok`].
pub enum Notulenregel {
    /// De kop van de notulen ("Notulen").
    Kop(String),
    /// De kop van een deel ("Besluiten").
    Tussenkop(String),
    /// Een gewone alinea (de samenvatting of een opmerking).
    Alinea(String),
    /// Een besluit of actiepunt, met de tijd van de bron als die is teruggevonden.
    Punt {
        /// De tijd van de bron in milliseconden, of `None` zonder teruggevonden bron.
        tijd_ms: Option<u64>,
        /// De tekst van het punt.
        tekst: String,
    },
}

/// Het teken vooraan een punt zonder tijd.
pub(crate) const OPSOMTEKEN: &str = "\u{2022}";

/// Maakt de bytes van het bestand. Met `tijden` staat voor elke regel de tijd; ondertitels
/// hebben altijd tijden. Een document zonder tekst krijgt de melding van het filter; ondertitels
/// zonder tekst worden niet gemaakt.
///
/// # Errors
///
/// Geeft een [`BewaarFout`] als het PDF- of zip-bestand niet te maken is, of
/// [`BewaarFout::GeenTekst`] voor ondertitels zonder tekst.
pub fn maak(document: &Document, formaat: Formaat, tijden: bool) -> Result<Vec<u8>, BewaarFout> {
    maak_met_notulen(document, None, formaat, tijden)
}

/// Als [`maak`], met de notulen vóór het verslag (ADR 0026 punt 8). Ondertitels (SRT en VTT)
/// krijgen geen notulen: daar is geen plek voor.
///
/// # Errors
///
/// Zie [`maak`].
pub fn maak_met_notulen(
    document: &Document,
    notulen: Option<&Notulenblok>,
    formaat: Formaat,
    tijden: bool,
) -> Result<Vec<u8>, BewaarFout> {
    if formaat.altijd_tijden() && document.tekstregels().is_empty() {
        return Err(BewaarFout::GeenTekst);
    }
    match formaat {
        Formaat::Docx => kantoor::docx(document, notulen, tijden),
        Formaat::Odt => kantoor::odt(document, notulen, tijden),
        Formaat::Pdf => pdf::pdf(document, notulen, tijden),
        Formaat::Txt => Ok(tekst::txt(document, notulen, tijden)),
        Formaat::Md => Ok(tekst::md(document, notulen, tijden)),
        Formaat::Html => Ok(tekst::html(document, notulen, tijden)),
        Formaat::Srt => Ok(tekst::srt(document)),
        Formaat::Vtt => Ok(tekst::vtt(document)),
    }
}

/// De standaardmap: `Documenten\Mondig` (op macOS en Linux de map Documenten van de
/// gebruiker). `None` als het systeem geen map Documenten kent.
#[must_use]
pub fn standaard_map() -> Option<std::path::PathBuf> {
    dirs::document_dir().map(|map| map.join("Mondig"))
}

/// Het moment bij milliseconden sinds 1970, in de tijdzone van de computer. Valt dat buiten
/// wat jiff kent, dan het begin van 1970 (in de praktijk komt dat niet voor).
#[must_use]
pub fn lokaal_moment(ms: u64) -> Zoned {
    let ms = i64::try_from(ms).unwrap_or(i64::MAX);
    jiff::Timestamp::from_millisecond(ms)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .to_zoned(TimeZone::system())
}

const MAANDEN: [&str; 12] = [
    "januari",
    "februari",
    "maart",
    "april",
    "mei",
    "juni",
    "juli",
    "augustus",
    "september",
    "oktober",
    "november",
    "december",
];

/// "1 november 2026, 14:32".
pub(crate) fn datum_voluit(moment: &Zoned) -> String {
    let maand = usize::try_from(moment.month() - 1)
        .ok()
        .and_then(|i| MAANDEN.get(i))
        .copied()
        .unwrap_or_default();
    format!(
        "{} {maand} {}, {:02}:{:02}",
        moment.day(),
        moment.year(),
        moment.hour(),
        moment.minute()
    )
}

/// Het moment in UTC volgens W3CDTF ("2026-11-01T13:32:05Z"), voor Word.
pub(crate) fn iso_utc(moment: &Zoned) -> String {
    let utc = moment.with_time_zone(TimeZone::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        utc.year(),
        utc.month(),
        utc.day(),
        utc.hour(),
        utc.minute(),
        utc.second()
    )
}

/// Het moment in lokale tijd zonder zone ("2026-11-01T14:32:05"), zoals LibreOffice het schrijft.
pub(crate) fn iso_lokaal(moment: &Zoned) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        moment.year(),
        moment.month(),
        moment.day(),
        moment.hour(),
        moment.minute(),
        moment.second()
    )
}

/// Een tijd in het document: "00:07", "12:40", boven het uur "1:02:40" (zoals het scherm).
pub(crate) use crate::notulen::tijdcode;

/// De duur van de opname: "2:05", "23:14", "1:02:40".
pub(crate) fn duur(ms: u64) -> String {
    let totaal = (ms + 500) / 1000;
    let (uren, minuten, seconden) = (totaal / 3600, (totaal % 3600) / 60, totaal % 60);
    if uren > 0 {
        format!("{uren}:{minuten:02}:{seconden:02}")
    } else {
        format!("{minuten}:{seconden:02}")
    }
}

/// Tekens die niets tonen maar tekst kunnen omdraaien of verbergen: tekens met nul breedte en
/// de richtingtekens LRM en RLM (U+200B tot U+200F), de richtingtekens voor stukken tekst
/// (U+202A tot U+202E en U+2066 tot U+2069) en U+FEFF. Ze vallen weg uit namen en documenten.
pub(crate) fn is_onzichtbaar(teken: char) -> bool {
    matches!(
        teken,
        '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}'
    )
}

/// Witruimte (ook regeleinden) wordt één spatie; tekens die geen tekst zijn en onzichtbare
/// tekens vallen weg.
pub(crate) fn schone_tekst(tekst: &str) -> String {
    let zichtbaar: String = tekst.chars().filter(|c| !is_onzichtbaar(*c)).collect();
    zichtbaar
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|deel| !deel.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Een regel zoals de formaten hem schrijven: de tekst en de spreker zijn schoongemaakt.
pub(crate) struct Tekstregel {
    pub(crate) start_ms: u64,
    pub(crate) eind_ms: u64,
    pub(crate) tekst: String,
    /// De spreker; `None` als die er niet is of na het schoonmaken leeg is.
    pub(crate) spreker: Option<String>,
}

impl Document {
    /// De regels met tekst, schoongemaakt; lege regels vallen weg.
    pub(crate) fn tekstregels(&self) -> Vec<Tekstregel> {
        self.regels
            .iter()
            .map(|r| Tekstregel {
                start_ms: r.start_ms,
                eind_ms: r.eind_ms,
                tekst: schone_tekst(&r.tekst),
                spreker: r
                    .spreker
                    .as_deref()
                    .map(schone_tekst)
                    .filter(|s| !s.is_empty()),
            })
            .filter(|r| !r.tekst.is_empty())
            .collect()
    }

    /// Maakt van een uitgeschreven vergadering een document: per beurt een regel met de
    /// spreker (de naam die de gebruiker gaf, anders "Spreker 1" of de kant). Bij een
    /// vergadering in de ruimte zonder herkende sprekers staan er geen sprekers in, dus elk
    /// formaat is dan gelijk aan een gewoon transcript. De titel is "Vergadering" met datum en tijd, de bron
    /// "Vergaderopname"; bij Alleen ik "Notitie" en "Ingesproken notitie". De meldingen (zoals "het geluid van de computer is niet opgenomen")
    /// gaan samengevoegd in `melding`.
    #[must_use]
    pub fn van_gesprek(gesprek: &Gesprek, model: &str, moment: Zoned) -> Self {
        let melding = (!gesprek.meldingen.is_empty()).then(|| gesprek.meldingen.join(" "));
        let met_sprekers = gesprek.met_sprekers();
        Self {
            titel: format!("{} {}", gesprek.soort.titelwoord(), datum_voluit(&moment)),
            bron: match gesprek.soort {
                Vergadersoort::Alleen => "Ingesproken notitie",
                Vergadersoort::Online | Vergadersoort::Ruimte => "Vergaderopname",
            }
            .to_string(),
            model: model.to_string(),
            label: None,
            moment,
            duur_ms: u64::try_from(gesprek.duur.as_millis()).unwrap_or(u64::MAX),
            melding,
            regels: gesprek
                .beurten
                .iter()
                .map(|b| Regel {
                    start_ms: b.start_ms,
                    eind_ms: b.eind_ms,
                    tekst: b.tekst.clone(),
                    spreker: met_sprekers.then(|| b.spreker()),
                })
                .collect(),
        }
    }

    /// De titel, schoongemaakt; "Transcript" als hij leeg is.
    pub(crate) fn titel(&self) -> String {
        let titel = schone_tekst(&self.titel);
        if titel.is_empty() {
            "Transcript".to_string()
        } else {
            titel
        }
    }

    /// Het label, schoongemaakt, als het er is.
    pub(crate) fn label(&self) -> Option<String> {
        self.label
            .as_deref()
            .map(schone_tekst)
            .filter(|l| !l.is_empty())
    }

    /// De gegevens bovenaan het document en in de eigenschappen, in deze volgorde.
    pub(crate) fn gegevens(&self) -> Vec<(&'static str, String)> {
        let mut gegevens = vec![
            ("Datum", datum_voluit(&self.moment)),
            ("Bron", schone_tekst(&self.bron)),
            ("Duur", duur(self.duur_ms)),
            ("Model", schone_tekst(&self.model)),
        ];
        if let Some(label) = self.label() {
            gegevens.push(("Label", label));
        }
        gegevens.push(("Verwerking", LOKAAL_VERWERKT.to_string()));
        gegevens
    }

    /// Eén zin met de gegevens voor het veld "beschrijving" van een document.
    pub(crate) fn beschrijving(&self) -> String {
        self.gegevens()
            .iter()
            .filter(|(sleutel, _)| *sleutel != "Datum")
            .map(|(sleutel, waarde)| format!("{sleutel}: {waarde}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// De tekst als er geen regels zijn: de melding van het filter of een vaste zin.
    pub(crate) fn zonder_tekst(&self) -> String {
        self.melding
            .as_deref()
            .map(schone_tekst)
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "Er is geen tekst herkend.".to_string())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use jiff::civil::date;

    /// Een vast moment: 1 november 2026, 14:32:05 in Nederland (UTC+1).
    pub(crate) fn moment() -> Zoned {
        date(2026, 11, 1)
            .at(14, 32, 5, 0)
            .to_zoned(TimeZone::fixed(jiff::tz::offset(1)))
            .expect("geldig moment")
    }

    pub(crate) fn document() -> Document {
        Document {
            titel: "Interview wethouder".to_string(),
            bron: "Interview wethouder.m4a".to_string(),
            model: "Parakeet Ultra".to_string(),
            label: Some("Z-2026-1043".to_string()),
            moment: moment(),
            duur_ms: 1_394_000,
            melding: None,
            regels: vec![
                Regel {
                    start_ms: 0,
                    eind_ms: 7_000,
                    tekst: "Goedemiddag, fijn dat u tijd heeft.".to_string(),
                    spreker: None,
                },
                Regel {
                    start_ms: 7_000,
                    eind_ms: 15_000,
                    tekst: " Ja, dank u. Het belangrijkste is de markt & minder <auto's>.\n"
                        .to_string(),
                    spreker: None,
                },
                Regel {
                    start_ms: 15_000,
                    eind_ms: 15_500,
                    tekst: "   ".to_string(),
                    spreker: None,
                },
            ],
        }
    }

    /// Een gesprek: twee kanten, een spreker met tekens die ontsnapt moeten worden en een
    /// spreker die na het schoonmaken leeg is.
    pub(crate) fn gesprek_document() -> Document {
        Document {
            regels: vec![
                Regel {
                    start_ms: 62_000,
                    eind_ms: 70_000,
                    tekst: "Goedemiddag, welkom.".to_string(),
                    spreker: Some("Jij".to_string()),
                },
                Regel {
                    start_ms: 71_000,
                    eind_ms: 80_000,
                    tekst: "Dank u wel & fijn dat u er bent.".to_string(),
                    spreker: Some("Wethouder <De Vries> &".to_string()),
                },
                Regel {
                    start_ms: 81_000,
                    eind_ms: 85_000,
                    tekst: "Zonder naam.".to_string(),
                    spreker: Some(" \u{202e} ".to_string()),
                },
            ],
            ..document()
        }
    }

    #[test]
    fn een_spreker_wordt_schoongemaakt_zoals_de_tekst() {
        let regels = gesprek_document().tekstregels();
        assert_eq!(regels[0].spreker.as_deref(), Some("Jij"));
        assert_eq!(regels[1].spreker.as_deref(), Some("Wethouder <De Vries> &"));
        // Alleen witruimte en onzichtbare tekens: geen spreker.
        assert_eq!(regels[2].spreker, None);
        let rommel = Document {
            regels: vec![Regel {
                start_ms: 0,
                eind_ms: 1_000,
                tekst: "Hallo.".to_string(),
                spreker: Some("  Jij\n\u{200b}  ".to_string()),
            }],
            ..document()
        };
        assert_eq!(rommel.tekstregels()[0].spreker.as_deref(), Some("Jij"));
    }

    #[test]
    fn een_gesprek_wordt_een_document_met_een_spreker_per_beurt() {
        use crate::gesprek::{Beurt, Gesprek, Kant, Spoorstand, Vergadersoort};
        use std::time::Duration;

        let gesprek = Gesprek {
            beurten: vec![
                Beurt {
                    kant: Kant::Anderen,
                    sprekernummer: None,
                    start_ms: 1_000,
                    eind_ms: 4_000,
                    tekst: "Welkom allemaal.".to_string(),
                    naam: Some("Wethouder De Vries".to_string()),
                    verbeterd: false,
                },
                Beurt {
                    kant: Kant::Jij,
                    sprekernummer: None,
                    start_ms: 4_500,
                    eind_ms: 6_000,
                    tekst: "Dank je.".to_string(),
                    naam: None,
                    verbeterd: false,
                },
            ],
            duur: Duration::from_millis(61_400),
            meldingen: vec!["Eerste melding.".to_string(), "Tweede melding.".to_string()],
            jij: Spoorstand::Opgenomen,
            anderen: Spoorstand::Opgenomen,
            weggelaten: 0,
            soort: Vergadersoort::Online,
        };
        let doc = Document::van_gesprek(&gesprek, "Parakeet v3", moment());
        assert_eq!(doc.titel, "Vergadering 1 november 2026, 14:32");
        assert_eq!(doc.bron, "Vergaderopname");
        assert_eq!(doc.model, "Parakeet v3");
        assert_eq!(doc.duur_ms, 61_400);
        assert_eq!(
            doc.melding.as_deref(),
            Some("Eerste melding. Tweede melding.")
        );
        assert_eq!(doc.label, None);
        let kort: Vec<_> = doc
            .regels
            .iter()
            .map(|r| {
                (
                    r.start_ms,
                    r.eind_ms,
                    r.tekst.as_str(),
                    r.spreker.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            kort,
            vec![
                (1_000, 4_000, "Welkom allemaal.", Some("Wethouder De Vries")),
                (4_500, 6_000, "Dank je.", Some("Jij")),
            ]
        );
        // In de ruimte staan er geen sprekers in.
        let ruimte = Gesprek {
            soort: Vergadersoort::Ruimte,
            ..gesprek.clone()
        };
        let zonder = Document::van_gesprek(&ruimte, "m", moment());
        assert!(zonder.regels.iter().all(|r| r.spreker.is_none()));
        assert_eq!(zonder.titel, "Vergadering 1 november 2026, 14:32");
        // Alleen ik: geen sprekers, en het heet een notitie (ook in de bestandsnaam).
        let alleen = Gesprek {
            soort: Vergadersoort::Alleen,
            ..gesprek.clone()
        };
        let notitie = Document::van_gesprek(&alleen, "m", moment());
        assert!(notitie.regels.iter().all(|r| r.spreker.is_none()));
        assert_eq!(notitie.titel, "Notitie 1 november 2026, 14:32");
        // In de ruimte met herkende sprekers staan ze er wel in (4b-D).
        let mut met_sprekers = ruimte.clone();
        for (b, nummer) in met_sprekers.beurten.iter_mut().zip([1, 2]) {
            b.sprekernummer = Some(nummer);
            b.naam = None;
        }
        let regels = Document::van_gesprek(&met_sprekers, "m", moment()).regels;
        assert_eq!(
            regels
                .iter()
                .map(|r| r.spreker.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("Spreker 1"), Some("Spreker 2")]
        );
        assert_eq!(notitie.bron, "Ingesproken notitie");
        // Zonder meldingen is er ook geen melding.
        let stil = Gesprek {
            meldingen: Vec::new(),
            ..gesprek
        };
        assert_eq!(Document::van_gesprek(&stil, "m", moment()).melding, None);
        // En het document is in elk formaat te maken.
        for formaat in Formaat::ALLE {
            assert!(maak(&doc, formaat, true).is_ok());
        }
    }

    #[test]
    fn formaten_heen_en_terug() {
        for formaat in Formaat::ALLE {
            assert_eq!(Formaat::uit_sleutel(formaat.extensie()), Some(formaat));
        }
        assert_eq!(Formaat::uit_sleutel("exe"), None);
        assert_eq!(Formaat::default(), Formaat::Docx);
        assert!(Formaat::Srt.altijd_tijden() && Formaat::Vtt.altijd_tijden());
        assert!(!Formaat::Docx.altijd_tijden());
    }

    #[test]
    fn datums_en_tijden() {
        let m = moment();
        assert_eq!(datum_voluit(&m), "1 november 2026, 14:32");
        assert_eq!(iso_utc(&m), "2026-11-01T13:32:05Z");
        assert_eq!(iso_lokaal(&m), "2026-11-01T14:32:05");
        assert_eq!(tijdcode(7_900), "00:07");
        assert_eq!(tijdcode(3_760_000), "1:02:40");
        assert_eq!(duur(1_394_000), "23:14");
        assert_eq!(duur(125_000), "2:05");
        assert_eq!(duur(3_760_000), "1:02:40");
    }

    #[test]
    fn lokaal_moment_klopt_met_de_klok() {
        let m = lokaal_moment(1_793_539_925_000);
        assert_eq!(m.timestamp().as_millisecond(), 1_793_539_925_000);
    }

    #[test]
    fn gegevens_in_vaste_volgorde_met_label() {
        let doc = document();
        let sleutels: Vec<_> = doc.gegevens().into_iter().map(|(s, _)| s).collect();
        assert_eq!(
            sleutels,
            ["Datum", "Bron", "Duur", "Model", "Label", "Verwerking"]
        );
        assert_eq!(
            doc.beschrijving(),
            "Bron: Interview wethouder.m4a; Duur: 23:14; Model: Parakeet Ultra; Label: Z-2026-1043; Verwerking: Lokaal verwerkt"
        );
        let zonder = Document {
            label: Some("  ".to_string()),
            ..document()
        };
        assert!(zonder.gegevens().iter().all(|(s, _)| *s != "Label"));
    }

    #[test]
    fn tekst_wordt_schoon_en_lege_regels_vallen_weg() {
        let regels = document().tekstregels();
        assert_eq!(regels.len(), 2);
        assert_eq!(
            regels[1].tekst,
            "Ja, dank u. Het belangrijkste is de markt & minder <auto's>."
        );
        assert_eq!(schone_tekst("a\u{0}b\r\nc\td"), "a b c d");
    }

    #[test]
    fn onzichtbare_tekens_vallen_weg() {
        assert_eq!(
            schone_tekst("fac\u{202e}cod.exe\u{202c} \u{200b}x\u{2066}y\u{2069}\u{feff}z\u{200d}"),
            "faccod.exe xyz"
        );
    }

    #[test]
    fn ondertitels_zonder_tekst_worden_niet_gemaakt() {
        let leeg = Document {
            regels: Vec::new(),
            melding: Some("Geen geluid gehoord.".to_string()),
            ..document()
        };
        for formaat in [Formaat::Srt, Formaat::Vtt] {
            assert!(matches!(
                maak(&leeg, formaat, true),
                Err(BewaarFout::GeenTekst)
            ));
        }
        // De andere formaten wel, met de melding als tekst.
        assert!(maak(&leeg, Formaat::Txt, true).is_ok());
        assert!(maak(&leeg, Formaat::Docx, false).is_ok());
    }
}

/// Schrijft van elk formaat een voorbeeld naar de map in `MONDIG_VOORBEELDEN`, om de bestanden
/// met andere programma's na te lopen (Word, LibreOffice, een PDF/A-validator):
/// `MONDIG_VOORBEELDEN=pad cargo test -p mondig-core --features bewaren -- --ignored voorbeelden`.
#[cfg(test)]
mod voorbeelden {
    use super::tests::document;
    use super::*;

    #[test]
    #[ignore = "schrijft bestanden; alleen met de hand"]
    fn voorbeelden() {
        let map = std::path::PathBuf::from(
            std::env::var("MONDIG_VOORBEELDEN").expect("MONDIG_VOORBEELDEN"),
        );
        std::fs::create_dir_all(&map).expect("map");
        let mut doc = document();
        // Een uur aan tekst, zoals een lange opname.
        doc.regels = (0..720)
            .map(|i| Regel {
                start_ms: i * 5_000,
                eind_ms: i * 5_000 + 4_800,
                tekst: format!(
                    "Zin {i}: de gemeente wil het centrum autoluw maken, maar ondernemers vrezen \u{e9}\u{e9}n ding: minder klanten & omzet."
                ),
                spreker: None,
            })
            .collect();
        for formaat in Formaat::ALLE {
            for tijden in [true, false] {
                let start = std::time::Instant::now();
                let bytes = maak(&doc, formaat, tijden).expect("maken");
                let naam = format!(
                    "voorbeeld-{}.{}",
                    if tijden { "met" } else { "zonder" },
                    formaat.extensie()
                );
                std::fs::write(map.join(&naam), bytes).expect("schrijven");
                eprintln!("{naam}: {:?}", start.elapsed());
            }
        }
    }
}
