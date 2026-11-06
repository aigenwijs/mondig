//! Modellen die uit de catalogus zijn gehaald, en het eenmalig opruimen van hun bestanden.
//!
//! Parakeet v3 is op 1 november 2026 vervallen: Parakeet Ultra is beter en even snel. Wie v3
//! nog op de computer had staan, houdt anders 740 MB over waar Mondig niets meer mee doet.
//! De lijst hieronder is bewust expliciet: Mondig ruimt alleen mappen op die hier met naam
//! staan, nooit een onbekende map in de modelmap.
//!
//! Deze module kent alleen schijf, geen app en geen instellingen. De app roept [`ruim_op`]
//! bij het starten aan, op een achtergrondthread, en zegt zelf of een model nog in gebruik is.

use std::fs;
use std::io;
use std::path::Path;

/// Modellen die niet meer in de catalogus staan, maar waarvan een bestaande installatie nog
/// bestanden kan hebben. Alleen id's van spraakmodellen, nooit van een hulpmodel: Silero
/// blijft staan, want Parakeet Ultra gebruikt het ook.
pub const AFGESCHAFTE_MODELLEN: [&str; 1] = ["parakeet-tdt-0.6b-v3-q8"];

/// Is dit een model dat Mondig heeft afgeschaft?
#[must_use]
pub fn is_afgeschaft(id: &str) -> bool {
    AFGESCHAFTE_MODELLEN.contains(&id)
}

/// Eén model dat is opgeruimd, met wat het vrijmaakte.
#[derive(Debug)]
pub struct Opruiming {
    /// Het id van het afgeschafte model.
    pub id: &'static str,
    /// Het aantal bytes dat de map innam, of de fout waarom verwijderen niet lukte.
    pub uitkomst: io::Result<u64>,
}

/// Verwijdert de mappen van afgeschafte modellen uit `modellen_map` (de map waarin elk model
/// een eigen submap heeft, zie `model::modelmap`).
///
/// - Een model waarvoor `in_gebruik` ja zegt, blijft staan; de volgende start probeert het
///   opnieuw.
/// - Een map die er niet is, valt niet op: dan is er niets te doen. Zo gebeurt het opruimen
///   precies één keer.
/// - Alleen de mappen uit [`AFGESCHAFTE_MODELLEN`] gaan weg. Een gewone map die toevallig
///   hetzelfde heet als een bestand, of een koppeling (symlink), laat deze functie staan.
#[must_use]
pub fn ruim_op(modellen_map: &Path, in_gebruik: &dyn Fn(&str) -> bool) -> Vec<Opruiming> {
    let mut gedaan = Vec::new();
    for id in AFGESCHAFTE_MODELLEN {
        let map = modellen_map.join(id);
        let is_echte_map = fs::symlink_metadata(&map).is_ok_and(|m| m.is_dir());
        if !is_echte_map || in_gebruik(id) {
            continue;
        }
        let bytes = map_grootte(&map);
        let uitkomst = fs::remove_dir_all(&map).map(|()| bytes);
        gedaan.push(Opruiming { id, uitkomst });
    }
    gedaan
}

/// Totale grootte van de bestanden in een map, onderliggende mappen meegeteld.
fn map_grootte(map: &Path) -> u64 {
    let Ok(inhoud) = fs::read_dir(map) else {
        return 0;
    };
    inhoud
        .flatten()
        .map(|item| match item.metadata() {
            Ok(m) if m.is_dir() => map_grootte(&item.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{self, SILERO_MODEL};

    const V3: &str = "parakeet-tdt-0.6b-v3-q8";

    fn zet_model(basis: &Path, id: &str, bytes: usize) {
        let map = basis.join(id);
        fs::create_dir_all(&map).unwrap();
        fs::write(map.join("model.gguf"), vec![0u8; bytes]).unwrap();
    }

    #[test]
    fn de_lijst_noemt_geen_model_uit_de_catalogus_en_geen_pad() {
        let catalogus = model::catalogus().unwrap();
        for id in AFGESCHAFTE_MODELLEN {
            assert!(
                catalogus.iter().all(|m| m.id != id),
                "{id} staat nog in de catalogus"
            );
            assert!(!id.contains(['/', '\\']) && !id.contains(".."), "{id}");
            assert!(is_afgeschaft(id));
        }
        assert!(!is_afgeschaft(model::STANDAARD_MODEL));
        assert!(!is_afgeschaft(SILERO_MODEL));
    }

    #[test]
    fn verwijdert_de_map_van_v3_en_meldt_de_grootte() {
        let basis = tempfile::tempdir().unwrap();
        zet_model(basis.path(), V3, 1234);
        let gedaan = ruim_op(basis.path(), &|_| false);
        assert_eq!(gedaan.len(), 1);
        assert_eq!(gedaan[0].id, V3);
        assert_eq!(*gedaan[0].uitkomst.as_ref().unwrap(), 1234);
        assert!(!basis.path().join(V3).exists());
    }

    #[test]
    fn silero_en_ultra_blijven_staan() {
        let basis = tempfile::tempdir().unwrap();
        zet_model(basis.path(), V3, 10);
        zet_model(basis.path(), SILERO_MODEL, 20);
        zet_model(basis.path(), model::STANDAARD_MODEL, 30);
        let _ = ruim_op(basis.path(), &|_| false);
        assert!(!basis.path().join(V3).exists());
        assert!(basis.path().join(SILERO_MODEL).join("model.gguf").exists());
        assert!(
            basis
                .path()
                .join(model::STANDAARD_MODEL)
                .join("model.gguf")
                .exists()
        );
    }

    #[test]
    fn een_onbekende_map_blijft_staan() {
        let basis = tempfile::tempdir().unwrap();
        zet_model(basis.path(), V3, 10);
        zet_model(basis.path(), "iets-van-de-gebruiker", 10);
        let _ = ruim_op(basis.path(), &|_| false);
        assert!(basis.path().join("iets-van-de-gebruiker").exists());
    }

    #[test]
    fn een_model_in_gebruik_blijft_staan_tot_de_volgende_start() {
        let basis = tempfile::tempdir().unwrap();
        zet_model(basis.path(), V3, 10);
        let gedaan = ruim_op(basis.path(), &|id| id == V3);
        assert!(gedaan.is_empty());
        assert!(basis.path().join(V3).exists());
        // Is het weer vrij, dan gaat het alsnog weg.
        assert_eq!(ruim_op(basis.path(), &|_| false).len(), 1);
        assert!(!basis.path().join(V3).exists());
    }

    #[test]
    fn zonder_map_of_zonder_modelmap_gebeurt_er_niets() {
        let basis = tempfile::tempdir().unwrap();
        assert!(ruim_op(basis.path(), &|_| false).is_empty());
        assert!(ruim_op(&basis.path().join("bestaat-niet"), &|_| false).is_empty());
    }

    #[test]
    fn opruimen_gebeurt_maar_een_keer() {
        let basis = tempfile::tempdir().unwrap();
        zet_model(basis.path(), V3, 10);
        assert_eq!(ruim_op(basis.path(), &|_| false).len(), 1);
        assert!(ruim_op(basis.path(), &|_| false).is_empty());
    }

    #[test]
    fn een_gewoon_bestand_met_de_naam_van_v3_blijft_staan() {
        let basis = tempfile::tempdir().unwrap();
        fs::write(basis.path().join(V3), b"geen map").unwrap();
        assert!(ruim_op(basis.path(), &|_| false).is_empty());
        assert!(basis.path().join(V3).exists());
    }
}
