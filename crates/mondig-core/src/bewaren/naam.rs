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
//! - een lege naam bestaat niet: dan de naam van de opname, en anders "Transcript".

use serde::{Deserialize, Serialize};

use super::{Formaat, Zoned, is_onzichtbaar};

/// Zoveel tekens mag de naam zonder extensie hooguit hebben. Windows staat 255 tekens per
/// naam toe, maar het hele pad mag in veel programma's niet langer zijn dan 260 tekens.
pub const MAX_STAM: usize = 120;

/// De naam als er niets bruikbaars overblijft.
const TERUGVAL: &str = "Transcript";

/// Een onderdeel van de bestandsnaam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bouwsteen {
    /// De datum als 2026-11-01, zodat de namen goed sorteren.
    Datum,
    /// De naam van de opname, zonder extensie.
    Naam,
    /// Het label, zoals een zaaknummer. Valt weg als het leeg is.
    Label,
    /// De tijd als 14.32 (een dubbele punt mag niet in een naam).
    Tijd,
    /// Het spraakmodel, kort ("Parakeet Ultra").
    Model,
}

/// De standaard: "2026-11-01 Interview wethouder".
pub const STANDAARD_BOUWSTENEN: [Bouwsteen; 2] = [Bouwsteen::Datum, Bouwsteen::Naam];

impl Bouwsteen {
    /// Alle bouwstenen, in de volgorde van het ontwerp.
    pub const ALLE: [Self; 5] = [
        Self::Datum,
        Self::Naam,
        Self::Label,
        Self::Tijd,
        Self::Model,
    ];

    /// De sleutel in de instellingen.
    #[must_use]
    pub const fn sleutel(self) -> &'static str {
        match self {
            Self::Datum => "datum",
            Self::Naam => "naam",
            Self::Label => "label",
            Self::Tijd => "tijd",
            Self::Model => "model",
        }
    }

    /// De bouwsteen bij een sleutel; `None` als die onbekend is.
    #[must_use]
    pub fn uit_sleutel(sleutel: &str) -> Option<Self> {
        Self::ALLE
            .into_iter()
            .find(|b| b.sleutel() == sleutel.trim())
    }
}

/// Leest de bewaarde bouwstenen ("datum,naam"). Onbekende en dubbele vallen weg.
#[must_use]
pub fn lees_bouwstenen(tekst: &str) -> Vec<Bouwsteen> {
    let mut lijst = Vec::new();
    for steen in tekst.split(',').filter_map(Bouwsteen::uit_sleutel) {
        if !lijst.contains(&steen) {
            lijst.push(steen);
        }
    }
    lijst
}

/// De bouwstenen als tekst voor de instellingen ("datum,naam").
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
    /// De naam van de opname zonder extensie.
    pub naam: &'a str,
    /// Het label, als het er is.
    pub label: Option<&'a str>,
    /// Het spraakmodel, kort.
    pub model: &'a str,
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
            Bouwsteen::Datum => format!("{:04}-{:02}-{:02}", m.year(), m.month(), m.day()),
            Bouwsteen::Naam => gegevens.naam.to_string(),
            Bouwsteen::Label => gegevens.label.unwrap_or_default().to_string(),
            Bouwsteen::Tijd => format!("{:02}.{:02}", m.hour(), m.minute()),
            Bouwsteen::Model => gegevens.model.to_string(),
        })
        .map(|deel| schoon_deel(&deel))
        .filter(|deel| !deel.is_empty())
        .collect();
    let mut stam = veilige_stam(&delen.join(" "));
    if stam.is_empty() {
        stam = veilige_stam(&schoon_deel(gegevens.naam));
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

    fn gegevens<'a>(moment: &'a Zoned, naam: &'a str, label: Option<&'a str>) -> NaamGegevens<'a> {
        NaamGegevens {
            moment,
            naam,
            label,
            model: "Parakeet Ultra",
        }
    }

    #[test]
    fn de_standaard_is_datum_en_naam() {
        let m = moment();
        let naam = bestandsnaam(
            &STANDAARD_BOUWSTENEN,
            &gegevens(&m, "Interview wethouder", Some("Z-2026-1043")),
            Formaat::Docx,
        );
        assert_eq!(naam, "2026-11-01 Interview wethouder.docx");
    }

    #[test]
    fn de_datum_kan_achter_de_naam_staan() {
        let m = moment();
        let g = gegevens(&m, "Interview wethouder", None);
        let omgedraaid = lees_bouwstenen("naam,datum");
        assert_eq!(
            bestandsnaam(&omgedraaid, &g, Formaat::Docx),
            "Interview wethouder 2026-11-01.docx"
        );
        // De standaard blijft hetzelfde als voorheen, dus bestaande instellingen veranderen niet.
        assert_eq!(schrijf_bouwstenen(&STANDAARD_BOUWSTENEN), "datum,naam");
    }

    #[test]
    fn alle_bouwstenen_in_de_gekozen_volgorde() {
        let m = moment();
        let g = gegevens(&m, "Interview wethouder", Some("Z-2026-1043"));
        let alle = [
            Bouwsteen::Label,
            Bouwsteen::Datum,
            Bouwsteen::Tijd,
            Bouwsteen::Naam,
            Bouwsteen::Model,
        ];
        assert_eq!(
            bestandsnaam(&alle, &g, Formaat::Pdf),
            "Z-2026-1043 2026-11-01 14.32 Interview wethouder Parakeet Ultra.pdf"
        );
    }

    #[test]
    fn ongeldige_tekens_worden_een_streepje() {
        let m = moment();
        let g = gegevens(&m, "Overleg: a/b \\ c|d?*\"<x>\u{7}", Some("Z/2026/1043"));
        assert_eq!(
            bestandsnaam(&[Bouwsteen::Naam, Bouwsteen::Label], &g, Formaat::Txt),
            "Overleg- a-b - c-d----x-- Z-2026-1043.txt"
        );
    }

    #[test]
    fn witruimte_en_punten_aan_het_eind() {
        let m = moment();
        let g = gegevens(&m, "  Vergadering\t\n  dinsdag ... ", None);
        assert_eq!(
            bestandsnaam(&[Bouwsteen::Naam], &g, Formaat::Srt),
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
            let g = gegevens(&m, naam, None);
            assert_eq!(
                bestandsnaam(&[Bouwsteen::Naam], &g, Formaat::Docx),
                verwacht,
                "{naam}"
            );
        }
    }

    #[test]
    fn te_lange_namen_worden_ingekort() {
        let m = moment();
        let lang = "é".repeat(400);
        let g = gegevens(&m, &lang, None);
        let naam = bestandsnaam(&[Bouwsteen::Naam], &g, Formaat::Docx);
        assert_eq!(naam.chars().count(), MAX_STAM + ".docx".len());
        // Inkorten mag niet op een spatie of punt eindigen.
        let met_spatie = format!("{} b", "a".repeat(MAX_STAM - 1));
        let g = gegevens(&m, &met_spatie, None);
        assert_eq!(
            bestandsnaam(&[Bouwsteen::Naam], &g, Formaat::Txt),
            format!("{}.txt", "a".repeat(MAX_STAM - 1))
        );
    }

    #[test]
    fn nooit_een_lege_naam() {
        let m = moment();
        // Alleen een leeg label: dan de naam van de opname.
        let g = gegevens(&m, "Interview", Some("  "));
        assert_eq!(
            bestandsnaam(&[Bouwsteen::Label], &g, Formaat::Txt),
            "Interview.txt"
        );
        // Geen bouwstenen: ook de naam van de opname.
        assert_eq!(bestandsnaam(&[], &g, Formaat::Txt), "Interview.txt");
        // Niets bruikbaars: "Transcript".
        let g = gegevens(&m, " ... ", None);
        assert_eq!(
            bestandsnaam(&[Bouwsteen::Naam], &g, Formaat::Vtt),
            "Transcript.vtt"
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
            Some("Z\u{2066}1\u{2069}"),
        );
        assert_eq!(
            bestandsnaam(&[Bouwsteen::Naam, Bouwsteen::Label], &g, Formaat::Docx),
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
        let lijst = [Bouwsteen::Label, Bouwsteen::Datum];
        assert_eq!(schrijf_bouwstenen(&lijst), "label,datum");
        assert_eq!(lees_bouwstenen("label,datum"), lijst);
        assert_eq!(
            lees_bouwstenen("datum, naam,onbekend,datum"),
            [Bouwsteen::Datum, Bouwsteen::Naam]
        );
        assert!(lees_bouwstenen("").is_empty());
    }
}
