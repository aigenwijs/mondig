//! De bestandsnaam uit bouwstenen (ontwerp 4-C): de gebruiker klikt aan wat erin moet, in
//! welke volgorde, en Mondig maakt er een naam van die Windows, macOS en Linux accepteren.
//!
//! Regels, voor alle platforms gelijk (een map kan later naar een ander systeem gaan):
//!
//! - tekens die Windows niet toestaat (`< > : " / \ | ? *` en stuurtekens) worden `-`;
//! - onzichtbare tekens (nul breedte, richtingtekens, U+FEFF) vallen weg, zodat een naam niet
//!   anders leest dan hij is;
//! - witruimte wordt één spatie, en een naam eindigt nooit op een spatie of punt;
//! - een gereserveerde naam van Windows (CON, PRN, AUX, NUL, COM1 tot COM9, LPT1 tot LPT9)
//!   krijgt er een `_` achter, ook als er nog een punt volgt ("CON.2026" wordt "CON_.2026");
//! - een te lange naam wordt ingekort tot [`MAX_STAM`] tekens, zodat er ruimte blijft voor
//!   " (2)" en de extensie;
//! - een lege naam bestaat niet: dan de standaard van het document (de naam van de opname, of
//!   "Vergadering"), en anders "Uitschrift".
//!
//! De bouwstenen zijn Datum, Klant, Onderwerp, Tijd en Model. Een lege bouwsteen valt weg. Oudere
//! instellingen kennen "label" en "naam"; die worden bij het lezen Klant en Onderwerp
//! ([`lees_bouwstenen`]).

use serde::{Deserialize, Serialize};

use super::{Document, Formaat, Zoned, is_onzichtbaar};
use crate::gesprek::schone_naamveld;

/// Zoveel tekens mag de naam zonder extensie hooguit hebben. Windows staat 255 tekens per
/// naam toe, maar het hele pad mag in veel programma's niet langer zijn dan 260 tekens.
pub const MAX_STAM: usize = 120;

/// De naam als er niets bruikbaars overblijft.
const TERUGVAL: &str = "Uitschrift";

/// Een onderdeel van de bestandsnaam.
///
/// Oudere instellingen en bestanden noemen Klant "label" en Onderwerp "naam"; de aliassen lezen
/// die nog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bouwsteen {
    /// De datum als 2026-11-01, zodat de namen goed sorteren. De gebruiker mag hem aanpassen.
    Datum,
    /// Voor wie het is, zoals een gemeente of een zaaknummer. Valt weg als het leeg is.
    #[serde(alias = "label")]
    Klant,
    /// Waar het over gaat. Staat vooraf ingevuld met de naam van de opname (of "Vergadering").
    #[serde(alias = "naam")]
    Onderwerp,
    /// De tijd als 14.32 (een dubbele punt mag niet in een naam).
    Tijd,
    /// Het spraakmodel, kort ("Parakeet Ultra").
    Model,
}

/// De standaard: "2026-11-01 Interview wethouder" (de Klant is dan leeg en valt weg).
pub const STANDAARD_BOUWSTENEN: [Bouwsteen; 3] =
    [Bouwsteen::Datum, Bouwsteen::Klant, Bouwsteen::Onderwerp];

impl Bouwsteen {
    /// Alle bouwstenen, in de volgorde van het ontwerp.
    pub const ALLE: [Self; 5] = [
        Self::Datum,
        Self::Klant,
        Self::Onderwerp,
        Self::Tijd,
        Self::Model,
    ];

    /// De sleutel in de instellingen.
    #[must_use]
    pub const fn sleutel(self) -> &'static str {
        match self {
            Self::Datum => "datum",
            Self::Klant => "klant",
            Self::Onderwerp => "onderwerp",
            Self::Tijd => "tijd",
            Self::Model => "model",
        }
    }

    /// De bouwsteen bij een sleutel; `None` als die onbekend is. De oude sleutels "label" en
    /// "naam" geven Klant en Onderwerp.
    #[must_use]
    pub fn uit_sleutel(sleutel: &str) -> Option<Self> {
        let sleutel = sleutel.trim();
        match sleutel {
            "label" => Some(Self::Klant),
            "naam" => Some(Self::Onderwerp),
            _ => Self::ALLE.into_iter().find(|b| b.sleutel() == sleutel),
        }
    }
}

/// Leest de bewaarde bouwstenen ("datum,klant,onderwerp"). Onbekende en dubbele vallen weg.
///
/// Migratie: "label" wordt Klant en "naam" wordt Onderwerp. Staat er een oude sleutel in maar na
/// de omzetting geen Klant, dan komt Klant direct vóór Onderwerp (of achteraan zonder
/// Onderwerp). Anders zou de gebruiker een klant invullen en de naam niet veranderen.
#[must_use]
pub fn lees_bouwstenen(tekst: &str) -> Vec<Bouwsteen> {
    let mut lijst = Vec::new();
    let mut oud = false;
    for deel in tekst.split(',') {
        oud |= matches!(deel.trim(), "label" | "naam");
        if let Some(steen) = Bouwsteen::uit_sleutel(deel)
            && !lijst.contains(&steen)
        {
            lijst.push(steen);
        }
    }
    if oud && !lijst.contains(&Bouwsteen::Klant) {
        let plek = lijst
            .iter()
            .position(|s| *s == Bouwsteen::Onderwerp)
            .unwrap_or(lijst.len());
        lijst.insert(plek, Bouwsteen::Klant);
    }
    lijst
}

/// De bouwstenen als tekst voor de instellingen ("datum,klant,onderwerp").
#[must_use]
pub fn schrijf_bouwstenen(bouwstenen: &[Bouwsteen]) -> String {
    bouwstenen
        .iter()
        .map(|b| b.sleutel())
        .collect::<Vec<_>>()
        .join(",")
}

/// Wat er in de naam kan komen.
pub struct NaamGegevens<'a> {
    /// Wanneer het transcript klaar was.
    pub moment: &'a Zoned,
    /// De datum die de gebruiker typte. `None`: de datum van `moment`. Leeg: valt weg.
    pub datum: Option<&'a str>,
    /// De klant; leeg valt weg.
    pub klant: &'a str,
    /// Het onderwerp, zoals het nu geldt (de standaard als de gebruiker niets typte); leeg valt weg.
    pub onderwerp: &'a str,
    /// Wat de naam wordt als er niets overblijft: de naam van de opname, of het soortwoord
    /// ("Vergadering", "Notitie") bij een verslag.
    pub terugval: &'a str,
    /// Het spraakmodel, kort.
    pub model: &'a str,
}

impl Document {
    /// De gegevens voor de bestandsnaam van dit document. Zonder eigen onderwerp geldt de
    /// standaard (de naam van de opname of het soortwoord).
    #[must_use]
    pub fn naamgegevens(&self) -> NaamGegevens<'_> {
        NaamGegevens {
            moment: &self.moment,
            datum: self.datum.as_deref(),
            klant: self.klant.as_deref().unwrap_or_default(),
            onderwerp: self
                .onderwerp
                .as_deref()
                .unwrap_or(&self.standaard_onderwerp),
            terugval: &self.standaard_onderwerp,
            model: &self.model,
        }
    }
}

/// De datum zoals de naam hem standaard heeft: 2026-11-01.
#[must_use]
pub fn standaard_datum(moment: &Zoned) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        moment.year(),
        moment.month(),
        moment.day()
    )
}

/// Wat de gebruiker in een veld typte, tegenover de standaard van dat veld: schoongemaakt en
/// begrensd, en `None` als het gelijk is aan de standaard. Zo blijft een veld dat de gebruiker
/// niet echt veranderde "nooit aangeraakt" en volgt het de standaard. Leeg getypt is `Some("")`:
/// het veld valt dan weg uit de naam.
#[must_use]
pub fn eigen_waarde(getypt: &str, standaard: &str) -> Option<String> {
    let schoon = schone_naamveld(getypt);
    (schoon != schone_naamveld(standaard)).then_some(schoon)
}

/// De bestandsnaam met extensie, uit de bouwstenen in hun volgorde, met spaties ertussen.
#[must_use]
pub fn bestandsnaam(
    bouwstenen: &[Bouwsteen],
    gegevens: &NaamGegevens<'_>,
    formaat: Formaat,
) -> String {
    let m = gegevens.moment;
    let delen: Vec<String> = bouwstenen
        .iter()
        .map(|steen| match steen {
            Bouwsteen::Datum => gegevens
                .datum
                .map_or_else(|| standaard_datum(m), str::to_string),
            Bouwsteen::Klant => gegevens.klant.to_string(),
            Bouwsteen::Onderwerp => gegevens.onderwerp.to_string(),
            Bouwsteen::Tijd => format!("{:02}.{:02}", m.hour(), m.minute()),
            Bouwsteen::Model => gegevens.model.to_string(),
        })
        .map(|deel| schoon_deel(&deel))
        .filter(|deel| !deel.is_empty())
        .collect();
    let mut stam = veilige_stam(&delen.join(" "));
    if stam.is_empty() {
        stam = veilige_stam(&schoon_deel(gegevens.terugval));
    }
    if stam.is_empty() {
        stam = TERUGVAL.to_string();
    }
    format!("{stam}.{}", formaat.extensie())
}

/// De naam met een volgnummer voor de extensie: "Interview.docx" wordt "Interview (2).docx".
#[must_use]
pub fn met_volgnummer(naam: &str, nummer: u32) -> String {
    match naam.rfind('.') {
        Some(punt) if punt > 0 => format!("{} ({nummer}){}", &naam[..punt], &naam[punt..]),
        _ => format!("{naam} ({nummer})"),
    }
}

/// Het volgnummer na het hoogste bestaande: staan "Naam.docx" en "Naam (7).docx" in de map, dan
/// wordt het 8. Hoofdletters tellen niet mee, zoals in Windows. Minstens 2.
#[must_use]
pub fn volgend_nummer<'a>(naam: &str, bestaande: impl IntoIterator<Item = &'a str>) -> u32 {
    let (stam, extensie) = match naam.rfind('.') {
        Some(punt) if punt > 0 => (&naam[..punt], &naam[punt..]),
        _ => (naam, ""),
    };
    let begin = format!("{stam} (").to_lowercase();
    let eind = format!("){extensie}").to_lowercase();
    bestaande
        .into_iter()
        .filter_map(|bestand| {
            let bestand = bestand.to_lowercase();
            bestand
                .strip_prefix(&begin)?
                .strip_suffix(&eind)?
                .parse::<u32>()
                .ok()
        })
        .max()
        .map_or(2, |hoogste| hoogste.saturating_add(1).max(2))
}

/// Is dit een teken dat niet in een bestandsnaam mag?
fn is_verboden(teken: char) -> bool {
    teken.is_control() || matches!(teken, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
}

/// Eén onderdeel: verboden tekens worden `-`, witruimte één spatie, zonder spaties eromheen.
fn schoon_deel(deel: &str) -> String {
    let vervangen: String = deel
        .chars()
        .filter(|c| !is_onzichtbaar(*c))
        .map(|c| {
            if c.is_whitespace() {
                ' '
            } else if is_verboden(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    vervangen.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Gereserveerde namen van Windows, zonder hoofdlettergevoeligheid.
fn is_gereserveerd(stam: &str) -> bool {
    const VAST: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];
    let groot = stam.trim_end().to_uppercase();
    if VAST.contains(&groot.as_str()) {
        return true;
    }
    let mut tekens = groot.chars();
    let begin: String = tekens.by_ref().take(3).collect();
    let rest: Vec<char> = tekens.collect();
    (begin == "COM" || begin == "LPT")
        && rest.len() == 1
        && matches!(rest[0], '0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}')
}

/// De naam zonder extensie, veilig voor elk systeem (zie de module-uitleg).
fn veilige_stam(stam: &str) -> String {
    let mut stam: String = stam.chars().take(MAX_STAM).collect();
    let einde = stam.trim_end_matches(['.', ' ']).len();
    stam.truncate(einde);
    let stam = stam.trim_start().to_string();
    // Windows kijkt naar het deel voor de eerste punt: "CON.2026.docx" is ook gereserveerd.
    let (eerste, rest) = stam
        .split_once('.')
        .map_or((stam.as_str(), ""), |(e, r)| (e, r));
    if is_gereserveerd(eerste) {
        if rest.is_empty() {
            format!("{eerste}_")
        } else {
            format!("{eerste}_.{rest}")
        }
    } else {
        stam
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::moment;
    use super::*;

    /// Gegevens met een onderwerp en een klant; `terugval` is hetzelfde onderwerp.
    fn gegevens<'a>(moment: &'a Zoned, onderwerp: &'a str, klant: &'a str) -> NaamGegevens<'a> {
        NaamGegevens {
            moment,
            datum: None,
            klant,
            onderwerp,
            terugval: onderwerp,
            model: "Parakeet Ultra",
        }
    }

    #[test]
    fn de_standaard_is_datum_klant_en_onderwerp() {
        let m = moment();
        let naam = bestandsnaam(
            &STANDAARD_BOUWSTENEN,
            &gegevens(&m, "Interview wethouder", "Gemeente Vlaardingen"),
            Formaat::Docx,
        );
        assert_eq!(
            naam,
            "2026-11-01 Gemeente Vlaardingen Interview wethouder.docx"
        );
        // Zonder klant verandert er niets ten opzichte van voorheen.
        let zonder = bestandsnaam(
            &STANDAARD_BOUWSTENEN,
            &gegevens(&m, "Interview wethouder", ""),
            Formaat::Docx,
        );
        assert_eq!(zonder, "2026-11-01 Interview wethouder.docx");
    }

    #[test]
    fn de_datum_kan_achter_het_onderwerp_staan() {
        let m = moment();
        let g = gegevens(&m, "Interview wethouder", "");
        let omgedraaid = lees_bouwstenen("onderwerp,datum");
        assert_eq!(
            bestandsnaam(&omgedraaid, &g, Formaat::Docx),
            "Interview wethouder 2026-11-01.docx"
        );
        assert_eq!(
            schrijf_bouwstenen(&STANDAARD_BOUWSTENEN),
            "datum,klant,onderwerp"
        );
    }

    #[test]
    fn alle_bouwstenen_in_de_gekozen_volgorde() {
        let m = moment();
        let g = gegevens(&m, "Interview wethouder", "Z-2026-1043");
        let alle = [
            Bouwsteen::Klant,
            Bouwsteen::Datum,
            Bouwsteen::Tijd,
            Bouwsteen::Onderwerp,
            Bouwsteen::Model,
        ];
        assert_eq!(
            bestandsnaam(&alle, &g, Formaat::Pdf),
            "Z-2026-1043 2026-11-01 14.32 Interview wethouder Parakeet Ultra.pdf"
        );
    }

    #[test]
    fn lege_delen_vallen_weg() {
        let m = moment();
        let mut g = gegevens(&m, "  ", "Gemeente");
        // Alleen de datum en de klant blijven over.
        assert_eq!(
            bestandsnaam(&STANDAARD_BOUWSTENEN, &g, Formaat::Txt),
            "2026-11-01 Gemeente.txt"
        );
        // Een lege datum (de gebruiker wiste hem) valt ook weg.
        g.datum = Some("");
        assert_eq!(
            bestandsnaam(&STANDAARD_BOUWSTENEN, &g, Formaat::Txt),
            "Gemeente.txt"
        );
        // Een eigen datum komt zoals getypt in de naam.
        g.datum = Some("30 okt 2026");
        g.onderwerp = "Intake";
        assert_eq!(
            bestandsnaam(&STANDAARD_BOUWSTENEN, &g, Formaat::Txt),
            "30 okt 2026 Gemeente Intake.txt"
        );
    }

    #[test]
    fn ongeldige_tekens_worden_een_streepje() {
        let m = moment();
        let g = gegevens(&m, "Overleg: a/b \\ c|d?*\"<x>\u{7}", "Z/2026/1043");
        assert_eq!(
            bestandsnaam(&[Bouwsteen::Onderwerp, Bouwsteen::Klant], &g, Formaat::Txt),
            "Overleg- a-b - c-d----x-- Z-2026-1043.txt"
        );
    }

    #[test]
    fn witruimte_en_punten_aan_het_eind() {
        let m = moment();
        let g = gegevens(&m, "  Vergadering\t\n  dinsdag ... ", "");
        assert_eq!(
            bestandsnaam(&[Bouwsteen::Onderwerp], &g, Formaat::Srt),
            "Vergadering dinsdag.srt"
        );
    }

    #[test]
    fn gereserveerde_namen_krijgen_een_liggend_streepje() {
        let m = moment();
        for (naam, verwacht) in [
            ("CON", "CON_.docx"),
            ("con", "con_.docx"),
            ("Nul", "Nul_.docx"),
            ("COM1", "COM1_.docx"),
            ("lpt9", "lpt9_.docx"),
            ("COM\u{b9}", "COM\u{b9}_.docx"),
            ("CON.2026", "CON_.2026.docx"),
            ("AUX ", "AUX_.docx"),
            ("CONSOLE", "CONSOLE.docx"),
            ("COM10", "COM10.docx"),
        ] {
            let g = gegevens(&m, naam, "");
            assert_eq!(
                bestandsnaam(&[Bouwsteen::Onderwerp], &g, Formaat::Docx),
                verwacht,
                "{naam}"
            );
        }
    }

    #[test]
    fn te_lange_namen_worden_ingekort() {
        let m = moment();
        let lang = "é".repeat(400);
        let g = gegevens(&m, &lang, "");
        let naam = bestandsnaam(&[Bouwsteen::Onderwerp], &g, Formaat::Docx);
        assert_eq!(naam.chars().count(), MAX_STAM + ".docx".len());
        // Inkorten mag niet op een spatie of punt eindigen.
        let met_spatie = format!("{} b", "a".repeat(MAX_STAM - 1));
        let g = gegevens(&m, &met_spatie, "");
        assert_eq!(
            bestandsnaam(&[Bouwsteen::Onderwerp], &g, Formaat::Txt),
            format!("{}.txt", "a".repeat(MAX_STAM - 1))
        );
    }

    #[test]
    fn nooit_een_lege_naam() {
        let m = moment();
        // Alleen een lege klant: dan de terugval (de naam van de opname).
        let g = gegevens(&m, "Interview", "  ");
        assert_eq!(
            bestandsnaam(&[Bouwsteen::Klant], &g, Formaat::Txt),
            "Interview.txt"
        );
        // Geen bouwstenen: ook de terugval.
        assert_eq!(bestandsnaam(&[], &g, Formaat::Txt), "Interview.txt");
        // Bij een verslag is de terugval het soortwoord, ook met een leeg onderwerp.
        let mut g = gegevens(&m, "", "");
        g.terugval = "Vergadering";
        assert_eq!(
            bestandsnaam(&[Bouwsteen::Klant, Bouwsteen::Onderwerp], &g, Formaat::Docx),
            "Vergadering.docx"
        );
        // Niets bruikbaars, ook niet in de terugval: "Uitschrift".
        let g = gegevens(&m, " ... ", "");
        assert_eq!(
            bestandsnaam(&[Bouwsteen::Onderwerp], &g, Formaat::Vtt),
            "Uitschrift.vtt"
        );
    }

    #[test]
    fn een_veld_gelijk_aan_de_standaard_blijft_ongeraakt() {
        assert_eq!(eigen_waarde("Vergadering", "Vergadering"), None);
        assert_eq!(eigen_waarde("  Vergadering ", "Vergadering"), None);
        assert_eq!(
            eigen_waarde("Intake", "Vergadering"),
            Some("Intake".to_string())
        );
        // Leeg getypt is een eigen keuze: het veld valt weg.
        assert_eq!(eigen_waarde("  ", "Vergadering"), Some(String::new()));
        assert_eq!(standaard_datum(&moment()), "2026-11-01");
        assert_eq!(
            eigen_waarde("2026-11-01", &standaard_datum(&moment())),
            None
        );
    }

    #[test]
    fn volgnummer_voor_de_extensie() {
        assert_eq!(met_volgnummer("Interview.docx", 2), "Interview (2).docx");
        assert_eq!(
            met_volgnummer("2026-11-01 Interview v1.2.pdf", 3),
            "2026-11-01 Interview v1.2 (3).pdf"
        );
        assert_eq!(met_volgnummer("zonder", 2), "zonder (2)");
    }

    #[test]
    fn onzichtbare_tekens_vallen_weg_uit_de_naam() {
        let m = moment();
        let g = gegevens(
            &m,
            "Verslag\u{202e}xcod.exe\u{200b}\u{feff}",
            "Z\u{2066}1\u{2069}",
        );
        assert_eq!(
            bestandsnaam(&[Bouwsteen::Onderwerp, Bouwsteen::Klant], &g, Formaat::Docx),
            "Verslagxcod.exe Z1.docx"
        );
    }

    #[test]
    fn volgend_nummer_na_het_hoogste_bestaande() {
        assert_eq!(volgend_nummer("Naam.docx", []), 2);
        assert_eq!(volgend_nummer("Naam.docx", ["Naam.docx"]), 2);
        assert_eq!(
            volgend_nummer(
                "Naam.docx",
                [
                    "Naam.docx",
                    "naam (3).DOCX",
                    "Naam (7).docx",
                    "Naam (x).docx",
                    "Naam (9).pdf"
                ]
            ),
            8
        );
        assert_eq!(
            volgend_nummer("v1.2 verslag.pdf", ["v1.2 verslag (4).pdf"]),
            5
        );
        assert_eq!(
            volgend_nummer("Naam.docx", ["Naam (4294967295).docx"]),
            u32::MAX
        );
    }

    #[test]
    fn bouwstenen_bewaren_en_lezen() {
        let lijst = [Bouwsteen::Klant, Bouwsteen::Datum];
        assert_eq!(schrijf_bouwstenen(&lijst), "klant,datum");
        assert_eq!(lees_bouwstenen("klant,datum"), lijst);
        assert_eq!(
            lees_bouwstenen("datum, onderwerp,onbekend,datum"),
            [Bouwsteen::Datum, Bouwsteen::Onderwerp]
        );
        assert!(lees_bouwstenen("").is_empty());
    }

    #[test]
    fn de_oude_sleutels_label_en_naam_worden_klant_en_onderwerp() {
        // De oude standaard "datum,naam" wordt datum, klant, onderwerp: anders zou een ingevulde
        // klant de naam niet veranderen.
        assert_eq!(
            lees_bouwstenen("datum,naam"),
            [Bouwsteen::Datum, Bouwsteen::Klant, Bouwsteen::Onderwerp]
        );
        // Een label dat er al was, wordt Klant op zijn eigen plek.
        assert_eq!(
            lees_bouwstenen("label,datum,naam"),
            [Bouwsteen::Klant, Bouwsteen::Datum, Bouwsteen::Onderwerp]
        );
        assert_eq!(
            lees_bouwstenen("naam,label"),
            [Bouwsteen::Onderwerp, Bouwsteen::Klant]
        );
        assert_eq!(
            lees_bouwstenen("datum,label"),
            [Bouwsteen::Datum, Bouwsteen::Klant]
        );
        // Alleen "naam": Klant komt ervoor.
        assert_eq!(
            lees_bouwstenen("naam,tijd"),
            [Bouwsteen::Klant, Bouwsteen::Onderwerp, Bouwsteen::Tijd]
        );
        // Wie bewust zonder Klant opsloeg (nieuwe sleutels), houdt dat.
        assert_eq!(
            lees_bouwstenen("datum,onderwerp"),
            [Bouwsteen::Datum, Bouwsteen::Onderwerp]
        );
        // Nogmaals lezen van het resultaat verandert niets.
        let eens = lees_bouwstenen("datum,naam");
        assert_eq!(lees_bouwstenen(&schrijf_bouwstenen(&eens)), eens);
    }

    #[test]
    fn de_oude_namen_werken_ook_als_serde_alias() {
        let oud: Vec<Bouwsteen> = serde_json::from_str(r#"["datum","label","naam"]"#).unwrap();
        assert_eq!(
            oud,
            [Bouwsteen::Datum, Bouwsteen::Klant, Bouwsteen::Onderwerp]
        );
        let nieuw = serde_json::to_string(&oud).unwrap();
        assert_eq!(nieuw, r#"["datum","klant","onderwerp"]"#);
    }

    #[test]
    fn een_document_geeft_zijn_naamgegevens() {
        let mut doc = super::super::tests::document();
        doc.klant = Some("Gemeente Vlaardingen".to_string());
        doc.onderwerp = None;
        doc.datum = None;
        doc.standaard_onderwerp = "Interview wethouder".to_string();
        assert_eq!(
            bestandsnaam(&STANDAARD_BOUWSTENEN, &doc.naamgegevens(), Formaat::Docx),
            "2026-11-01 Gemeente Vlaardingen Interview wethouder.docx"
        );
        // Een gewist onderwerp valt weg en de datum wordt wat de gebruiker typte.
        doc.onderwerp = Some(String::new());
        doc.datum = Some("2026-10-30".to_string());
        assert_eq!(
            bestandsnaam(&STANDAARD_BOUWSTENEN, &doc.naamgegevens(), Formaat::Docx),
            "2026-10-30 Gemeente Vlaardingen.docx"
        );
    }
}
