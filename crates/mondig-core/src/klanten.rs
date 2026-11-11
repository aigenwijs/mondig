//! De klantnamen die Mondig onthoudt als suggestie bij het veld Klant (UI-E).
//!
//! Ze staan als `klanten.json` in de eigen gegevensmap van Mondig, alleen op deze computer,
//! naast de andere bestanden die de app zelf bewaart. Nieuwste eerst, hooguit [`MAX`] namen.
//! Twee namen die alleen in hoofdletters verschillen, zijn dezelfde klant: de nieuwste
//! schrijfwijze blijft. Een naam komt erbij na een geslaagd opslaan, niet bij het typen.
//!
//! Lezen is mild: een bestand dat ontbreekt of kapot is, geeft een lege lijst. De namen horen
//! bij de gebruiker en komen nooit in een logboek of foutmelding; de fouten hier zeggen niets
//! over de inhoud.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bewaak::gegevensmap;
use crate::gesprek::schone_naamveld;
use crate::spoor::stukken::schrijf_atomair;

/// De naam van het bestand in de gegevensmap.
pub const BESTAND: &str = "klanten.json";

/// Zoveel namen onthoudt Mondig hooguit.
pub const MAX: usize = 50;

/// De versie van het bestand dat deze code schrijft.
const VERSIE: u32 = 1;

/// Het bestand zoals het op schijf staat. Bewust zonder `Debug`: er staan namen in.
#[derive(Serialize, Deserialize)]
struct Bestand {
    #[serde(default)]
    versie: u32,
    #[serde(default)]
    klanten: Vec<String>,
}

/// Het bestand met de klantnamen, of `None` als het systeem geen gegevensmap kent.
#[must_use]
pub fn standaard_bestand() -> Option<PathBuf> {
    gegevensmap().map(|map| map.join(BESTAND))
}

/// Zet `naam` vooraan in de lijst. De naam wordt schoongemaakt; een lege naam verandert niets. Een
/// naam die al in de lijst staat (zonder verschil in hoofdletters) schuift naar voren met de
/// nieuwe schrijfwijze. De lijst wordt tot [`MAX`] namen ingekort.
pub fn voeg_toe_aan(lijst: &mut Vec<String>, naam: &str) {
    let schoon = schone_naamveld(naam);
    if schoon.is_empty() {
        return;
    }
    let sleutel = schoon.to_lowercase();
    lijst.retain(|bestaand| bestaand.to_lowercase() != sleutel);
    lijst.insert(0, schoon);
    lijst.truncate(MAX);
}

/// Maakt van wat er op schijf stond een nette lijst: schoon, zonder lege en dubbele namen, hooguit
/// [`MAX`]. De volgorde (nieuwste eerst) blijft; bij een dubbele naam telt de eerste.
fn opgeschoond(namen: Vec<String>) -> Vec<String> {
    let mut lijst: Vec<String> = Vec::new();
    for naam in namen {
        let schoon = schone_naamveld(&naam);
        let sleutel = schoon.to_lowercase();
        if !schoon.is_empty() && !lijst.iter().any(|b| b.to_lowercase() == sleutel) {
            lijst.push(schoon);
        }
        if lijst.len() == MAX {
            break;
        }
    }
    lijst
}

/// Leest de lijst uit `pad`. Een ontbrekend of kapot bestand geeft een lege lijst.
#[must_use]
pub fn lees(pad: &Path) -> Vec<String> {
    let Ok(tekst) = fs::read_to_string(pad) else {
        return Vec::new();
    };
    serde_json::from_str::<Bestand>(&tekst).map_or_else(|_| Vec::new(), |b| opgeschoond(b.klanten))
}

/// Schrijft de lijst atomair naar `pad`; de map wordt zo nodig aangemaakt.
fn schrijf(pad: &Path, lijst: &[String]) -> io::Result<()> {
    if let Some(map) = pad.parent() {
        fs::create_dir_all(map)?;
    }
    let bestand = Bestand {
        versie: VERSIE,
        klanten: lijst.to_vec(),
    };
    let bytes = serde_json::to_vec(&bestand).map_err(io::Error::other)?;
    schrijf_atomair(pad, &bytes)
}

/// Onthoudt `naam` in het bestand en geeft de nieuwe lijst. Een lege naam laat het bestand zoals
/// het was.
///
/// # Errors
///
/// Geeft een I/O-fout als het bestand niet te schrijven is.
pub fn voeg_toe(pad: &Path, naam: &str) -> io::Result<Vec<String>> {
    let mut lijst = lees(pad);
    if schone_naamveld(naam).is_empty() {
        return Ok(lijst);
    }
    voeg_toe_aan(&mut lijst, naam);
    schrijf(pad, &lijst)?;
    Ok(lijst)
}

/// Vergeet alle klantnamen. Een bestand dat er niet is, telt als gelukt.
///
/// # Errors
///
/// Geeft een I/O-fout als het bestand er is maar niet weg kan.
pub fn wis(pad: &Path) -> io::Result<()> {
    match fs::remove_file(pad) {
        Err(fout) if fout.kind() != io::ErrorKind::NotFound => Err(fout),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lijst(namen: &[&str]) -> Vec<String> {
        namen.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn de_nieuwste_staat_vooraan() {
        let mut l = lijst(&["Gemeente Meerwijk"]);
        voeg_toe_aan(&mut l, "Gemeente Vlaardingen");
        assert_eq!(l, lijst(&["Gemeente Vlaardingen", "Gemeente Meerwijk"]));
    }

    #[test]
    fn dubbele_namen_zonder_verschil_in_hoofdletters_vallen_samen() {
        let mut l = lijst(&["Gemeente Vlaardingen", "Gemeente Meerwijk"]);
        voeg_toe_aan(&mut l, "gemeente meerwijk");
        // De nieuwste schrijfwijze blijft, en staat nu vooraan.
        assert_eq!(l, lijst(&["gemeente meerwijk", "Gemeente Vlaardingen"]));
    }

    #[test]
    fn een_naam_wordt_schoongemaakt_en_een_lege_naam_telt_niet() {
        let mut l = lijst(&["A"]);
        voeg_toe_aan(&mut l, "  Gemeente \t Vlaardingen \n");
        voeg_toe_aan(&mut l, "   ");
        voeg_toe_aan(&mut l, "");
        assert_eq!(l, lijst(&["Gemeente Vlaardingen", "A"]));
    }

    #[test]
    fn hooguit_vijftig_namen_en_de_oudste_valt_eraf() {
        let mut l = Vec::new();
        for n in 0..60 {
            voeg_toe_aan(&mut l, &format!("Klant {n}"));
        }
        assert_eq!(l.len(), MAX);
        assert_eq!(l[0], "Klant 59");
        assert_eq!(l[MAX - 1], "Klant 10");
    }

    #[test]
    fn opslaan_en_teruglezen_in_een_map_die_er_nog_niet_is() {
        let map = tempfile::tempdir().unwrap();
        let pad = map.path().join("gegevens").join(BESTAND);
        assert_eq!(voeg_toe(&pad, "Gemeente Meerwijk").unwrap().len(), 1);
        let na = voeg_toe(&pad, "Gemeente Vlaardingen").unwrap();
        assert_eq!(na, lijst(&["Gemeente Vlaardingen", "Gemeente Meerwijk"]));
        assert_eq!(lees(&pad), na);
        // Een lege naam raakt het bestand niet aan.
        assert_eq!(voeg_toe(&pad, "  ").unwrap(), na);
        // Geen tijdelijk bestand achtergebleven.
        let namen: Vec<_> = fs::read_dir(pad.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(namen, vec![BESTAND.to_string()]);
    }

    #[test]
    fn een_ontbrekend_of_kapot_bestand_is_een_lege_lijst() {
        let map = tempfile::tempdir().unwrap();
        let pad = map.path().join(BESTAND);
        assert!(lees(&pad).is_empty());
        for rommel in ["", "{", "geen json", "[1,2,3]", "{\"klanten\":\"x\"}"] {
            fs::write(&pad, rommel).unwrap();
            assert!(lees(&pad).is_empty(), "{rommel:?}");
        }
        // Na een kapot bestand begint de lijst gewoon opnieuw.
        assert_eq!(voeg_toe(&pad, "Gemeente").unwrap(), lijst(&["Gemeente"]));
    }

    #[test]
    fn wat_er_op_schijf_staat_wordt_bij_het_lezen_hersteld() {
        let map = tempfile::tempdir().unwrap();
        let pad = map.path().join(BESTAND);
        let veel: Vec<String> = (0..80).map(|n| format!("\"Klant {n}\"")).collect();
        fs::write(
            &pad,
            format!(
                "{{\"klanten\":[\"  A  \",\"a\",\"\",\"  \",\"B\",{}]}}",
                veel.join(",")
            ),
        )
        .unwrap();
        let l = lees(&pad);
        assert_eq!(l.len(), MAX);
        assert_eq!(&l[..3], ["A", "B", "Klant 0"]);
    }

    #[test]
    fn wissen_haalt_het_bestand_weg_en_mag_vaker() {
        let map = tempfile::tempdir().unwrap();
        let pad = map.path().join(BESTAND);
        voeg_toe(&pad, "Gemeente").unwrap();
        assert!(pad.is_file());
        wis(&pad).unwrap();
        assert!(!pad.exists());
        assert!(lees(&pad).is_empty());
        // Er was al niets.
        wis(&pad).unwrap();
    }
}
