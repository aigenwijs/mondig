//! Soevereiniteitslabel van een aanbieder: waar wordt de audio verwerkt?

use serde::{Deserialize, Serialize};

/// Waar een aanbieder de audio verwerkt. Serialiseert als `lokaal`, `europees` of
/// `buiten_europa`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Soevereiniteit {
    /// Op de computer van de gebruiker.
    Lokaal,
    /// Bij een aanbieder in Europa.
    Europees,
    /// Bij een aanbieder buiten Europa.
    BuitenEuropa,
}

impl Soevereiniteit {
    /// Moet de gebruiker gewaarschuwd worden voordat audio naar deze aanbieder gaat?
    /// Alleen bij verwerking buiten Europa.
    #[must_use]
    pub fn vraagt_waarschuwing(self) -> bool {
        matches!(self, Self::BuitenEuropa)
    }

    /// Korte Nederlandse naam voor in de interface.
    #[must_use]
    pub fn label_nl(self) -> &'static str {
        match self {
            Self::Lokaal => "Op je computer",
            Self::Europees => "Europees",
            Self::BuitenEuropa => "Buiten Europa",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alleen_buiten_europa_vraagt_waarschuwing() {
        assert!(!Soevereiniteit::Lokaal.vraagt_waarschuwing());
        assert!(!Soevereiniteit::Europees.vraagt_waarschuwing());
        assert!(Soevereiniteit::BuitenEuropa.vraagt_waarschuwing());
    }

    #[test]
    fn serialiseert_als_snake_case() {
        let json = |s| serde_json::to_string(&s).unwrap();
        assert_eq!(json(Soevereiniteit::Lokaal), r#""lokaal""#);
        assert_eq!(json(Soevereiniteit::BuitenEuropa), r#""buiten_europa""#);
    }

    #[test]
    fn labels_zijn_nederlands() {
        assert_eq!(Soevereiniteit::Lokaal.label_nl(), "Op je computer");
        assert_eq!(Soevereiniteit::Europees.label_nl(), "Europees");
        assert_eq!(Soevereiniteit::BuitenEuropa.label_nl(), "Buiten Europa");
    }
}
