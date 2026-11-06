//! Tekst in een verslag of transcript verbeteren (stroom V, V-1): een regel krijgt een nieuwe
//! tekst, en daarna gebruiken het scherm, Kopiëren en alle exports die nieuwe tekst.
//!
//! De regels zelf blijven zoals het model ze gaf; de bewerking staat ernaast, per plek in de
//! lijst. Zo kan een bewerking terug naar het origineel (de tekst weer gelijk maken), en
//! verandert er niets aan de tijden of sprekers. Bij een vergadering ligt dat anders: daar
//! zijn beurten al samengesteld en vervangt [`crate::gesprek::Gesprek::zet_tekst`] de tekst
//! van de beurt zelf.
//!
//! Bewust zonder `Debug`: een bewerking is tekst van de gebruiker en komt nooit in een log.

use std::collections::BTreeMap;

/// Zoveel tekens mag één regel hebben na bewerken. Ruim boven een lange beurt (een monoloog
/// van tien minuten is ongeveer 10 000 tekens), maar begrensd tegen een plak-ongelukje.
pub const MAX_TEKENS: usize = 50_000;

/// De tekst zoals de gebruiker hem typte, schoongemaakt: regeleinden worden `\n`, andere
/// stuurtekens vallen weg (een tab wordt een spatie), elke regel verliest witruimte aan de
/// randen, lege regels aan het begin en het eind vallen weg en meerdere lege regels achter
/// elkaar worden één. Hooguit [`MAX_TEKENS`] tekens. Leeg geeft `None`: een lege regel is
/// verloren tekst, geen bewerking.
#[must_use]
pub fn schone_bewerking(tekst: &str) -> Option<String> {
    let genormaliseerd = tekst.replace("\r\n", "\n").replace('\r', "\n");
    let zichtbaar: String = genormaliseerd
        .chars()
        .map(|c| if c == '\t' { ' ' } else { c })
        .filter(|c| *c == '\n' || !c.is_control())
        .collect();
    let mut regels: Vec<&str> = Vec::new();
    for regel in zichtbaar.split('\n').map(str::trim) {
        // Meerdere lege regels achter elkaar worden één.
        if regel.is_empty() && regels.last().is_none_or(|vorige| vorige.is_empty()) {
            continue;
        }
        regels.push(regel);
    }
    while regels.last().is_some_and(|r| r.is_empty()) {
        regels.pop();
    }
    let schoon: String = regels.join("\n").chars().take(MAX_TEKENS).collect();
    let schoon = schoon.trim_end();
    (!schoon.is_empty()).then(|| schoon.to_string())
}

/// De bewerkte teksten van een lijst regels, per plek in die lijst.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Bewerkingen {
    per_plek: BTreeMap<usize, String>,
}

impl Bewerkingen {
    /// Zet de tekst van regel `plek`, waarvan het origineel `oorspronkelijk` is. Een lege
    /// tekst verandert niets (de oude tekst blijft). Is de nieuwe tekst gelijk aan het
    /// origineel, dan vervalt de bewerking. Geeft `true` als er iets veranderde.
    pub fn zet(&mut self, plek: usize, oorspronkelijk: &str, nieuw: &str) -> bool {
        let Some(schoon) = schone_bewerking(nieuw) else {
            return false;
        };
        if schone_bewerking(oorspronkelijk).as_deref() == Some(schoon.as_str()) {
            return self.per_plek.remove(&plek).is_some();
        }
        self.per_plek.insert(plek, schoon.clone()) != Some(schoon)
    }

    /// De tekst van regel `plek`: de bewerkte, anders het origineel.
    #[must_use]
    pub fn tekst<'a>(&'a self, plek: usize, oorspronkelijk: &'a str) -> &'a str {
        self.per_plek
            .get(&plek)
            .map_or(oorspronkelijk, String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schoonmaken_houdt_bewuste_regeleinden_en_haalt_de_rest_weg() {
        assert_eq!(
            schone_bewerking("  Goedemiddag.\r\n\r\n\r\n\r\n  Tot zo.\t \u{7}"),
            Some("Goedemiddag.\n\nTot zo.".to_string())
        );
        assert_eq!(schone_bewerking("\n\n  \n"), None);
        assert_eq!(schone_bewerking(""), None);
        assert_eq!(schone_bewerking("a\tb"), Some("a b".to_string()));
    }

    #[test]
    fn schoonmaken_kapt_op_het_maximum_af() {
        let lang = "x".repeat(MAX_TEKENS + 10);
        assert_eq!(schone_bewerking(&lang).unwrap().chars().count(), MAX_TEKENS);
    }

    #[test]
    fn een_bewerking_vervangt_de_tekst_en_een_lege_houdt_de_oude() {
        let mut b = Bewerkingen::default();
        assert!(b.zet(2, "Goedemidag.", "Goedemiddag."));
        assert_eq!(b.tekst(2, "Goedemidag."), "Goedemiddag.");
        assert_eq!(b.tekst(3, "Anders."), "Anders.");
        // Leeg maken houdt de oude tekst.
        assert!(!b.zet(2, "Goedemidag.", "  \n "));
        assert_eq!(b.tekst(2, "Goedemidag."), "Goedemiddag.");
        // Dezelfde tekst nog eens is geen verandering.
        assert!(!b.zet(2, "Goedemidag.", "Goedemiddag."));
    }

    #[test]
    fn terugzetten_naar_het_origineel_laat_de_bewerking_vervallen() {
        let mut b = Bewerkingen::default();
        b.zet(0, "Hallo.", "Hallo daar.");
        assert!(b.zet(0, "Hallo.", "  Hallo. "));
        assert_eq!(b.tekst(0, "Hallo."), "Hallo.");
    }
}
