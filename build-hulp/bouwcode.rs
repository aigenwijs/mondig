//! Gedeeld stuk van de build.rs van de app en het werkproces: de korte commit als
//! bouwcode (ADR 0012, aanvulling 2-11-2026). Beide binaries krijgen zo dezelfde `MONDIG_BOUW_COMMIT`, en het
//! werkproces meldt `versie+commit` aan de app, die het vergelijkt met haar eigen code.
//!
//! Wordt met `#[path]` als module ingeladen; een eigen crate zou hier te veel voor zijn.

use std::process::Command;

/// Zet `MONDIG_BOUW_COMMIT` voor `env!` en vraagt Cargo de build.rs opnieuw te draaien
/// als de commit wisselt. Is de werkmap gewijzigd, dan staat er `-dirty` achter de commit.
/// Zonder git (bijvoorbeeld in een tarball) wordt de commit `onbekend`; de build faalt daar
/// nooit op. `bronmappen` zijn de mappen waarvan een wijziging `-dirty` kan veranderen.
pub(crate) fn zet_commit(bronmappen: &[&str]) {
    let commit = match git(&["rev-parse", "--short=7", "HEAD"]) {
        Some(c) if git(&["status", "--porcelain"]).is_some() => format!("{c}-dirty"),
        Some(c) => c,
        None => "onbekend".into(),
    };
    println!("cargo:rustc-env=MONDIG_BOUW_COMMIT={commit}");

    // Opnieuw uitvoeren als de commit wisselt (HEAD zelf wijzigt niet bij een nieuwe commit op
    // dezelfde branch; logs/HEAD wel), als er iets wordt klaargezet (index) of als bronbestanden
    // wijzigen, voor `-dirty`. Het pad komt van git zelf, zodat het ook in een worktree klopt.
    for bestand in ["HEAD", "logs/HEAD", "index"] {
        if let Some(pad) = git(&["rev-parse", "--git-path", bestand])
            && std::path::Path::new(&pad).exists()
        {
            println!("cargo:rerun-if-changed={pad}");
        }
    }
    for map in bronmappen {
        println!("cargo:rerun-if-changed={map}");
    }
}

/// Draait git met de gegeven argumenten; `None` als git ontbreekt, faalt of niets teruggeeft.
pub(crate) fn git(args: &[&str]) -> Option<String> {
    let uitvoer = Command::new("git").args(args).output().ok()?;
    if !uitvoer.status.success() {
        return None;
    }
    let tekst = String::from_utf8(uitvoer.stdout).ok()?;
    let tekst = tekst.trim();
    (!tekst.is_empty()).then(|| tekst.to_string())
}
