//! De spraakengine in een eigen werkproces (ADR 0009).
//!
//! De engine zelf (transcribe.cpp met ggml) zit in de crate
//! `mondig-engine`. Deze module bevat alleen wat de app en de CLI nodig hebben om
//! dat werkproces te besturen, zodat de kern licht blijft:
//!
//! - [`beheer`]: het laadbeleid (opwarmen, rusttijd, annuleren) op een eigen thread;
//! - [`protocol`]: de berichten die over stdin en stdout gaan;
//! - [`supervisor`]: start en bewaakt het werkproces en vertaalt crashes en
//!   time-outs naar een melding in gewone taal.

pub mod beheer;
pub mod protocol;
pub mod supervisor;

use std::path::{Path, PathBuf};

/// Bestandsnaam (zonder extensie) van het werkproces met de spraakengine.
pub const WERKPROCES_NAAM: &str = "mondig-engine";

/// Zoekt het werkproces naast het opgegeven programma (meestal de eigen exe).
///
/// Alleen daar: nooit in de werkmap of op PATH, zodat een bestand met die naam in een
/// willekeurige map niet kan worden uitgevoerd. In ontwikkeling is dat `target/debug`,
/// na installatie de programmamap.
///
/// # Errors
///
/// Geeft [`supervisor::EngineFout::Starten`] als het werkproces daar niet staat.
pub fn werkproces_naast(exe: &Path) -> Result<PathBuf, supervisor::EngineFout> {
    let map = exe
        .parent()
        .ok_or_else(|| supervisor::EngineFout::Starten {
            reden: "de map van het programma is niet te bepalen".into(),
        })?;
    let pad = map.join(format!("{WERKPROCES_NAAM}{}", std::env::consts::EXE_SUFFIX));
    if pad.is_file() {
        Ok(pad)
    } else {
        Err(supervisor::EngineFout::Starten {
            reden: format!("{WERKPROCES_NAAM} staat niet in {}", map.display()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vindt_het_werkproces_naast_de_exe() {
        let map = tempfile::tempdir().unwrap();
        let exe = map.path().join("mondig");
        let engine = map
            .path()
            .join(format!("{WERKPROCES_NAAM}{}", std::env::consts::EXE_SUFFIX));
        assert!(werkproces_naast(&exe).is_err());
        std::fs::write(&engine, b"").unwrap();
        assert_eq!(werkproces_naast(&exe).unwrap(), engine);
    }

    #[test]
    fn zoekt_niet_in_een_andere_map() {
        let map = tempfile::tempdir().unwrap();
        let andere = tempfile::tempdir().unwrap();
        std::fs::write(
            andere
                .path()
                .join(format!("{WERKPROCES_NAAM}{}", std::env::consts::EXE_SUFFIX)),
            b"",
        )
        .unwrap();
        assert!(werkproces_naast(&map.path().join("mondig")).is_err());
    }
}
