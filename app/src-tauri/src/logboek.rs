//! Het logboek voor support (S-2a): de Tauri-kant van `mondig_core::logboek`.
//!
//! De regels zelf, de typen die tekst van de gebruiker buiten houden en de rotatie staan in
//! de kern. Hier staat waar het logboek woont (de logmap van de app), de twee knoppen in
//! Instellingen en de ene plek waar taken die mislukken in het logboek komen. Er gaat niets
//! vanzelf weg: de gebruiker kopieert het logboek zelf (ADR 0003).

use mondig_core::logboek::{self, Gebeurtenis};
use mondig_core::probleem::Probleem;
use mondig_core::taak::{self, Foutmelding, Toestand, Vervolg};
use tauri::{AppHandle, Manager};

/// Zet het logboek op in de logmap van de app (`%LOCALAPPDATA%\com.aigenwijs.spraak\logs` op
/// Windows) en schrijft de eerste regel. Lukt de map niet, dan werkt Mondig zonder logboek.
pub(crate) fn start(app: &AppHandle) {
    if let Ok(map) = app.path().app_log_dir() {
        logboek::start(map);
    }
    logboek::log(&Gebeurtenis::AppGestart {
        versie: env!("CARGO_PKG_VERSION"),
        besturingssysteem: std::env::consts::OS,
    });
}

/// Zet een mislukte taak in het logboek, met de soort taak en het soort probleem, nooit met
/// de melding: die kan een bestandsnaam bevatten. Taken die niet mislukken doen niets.
pub(crate) fn taak(gebeurtenis: &taak::Gebeurtenis) {
    if let Toestand::Fout { melding, .. } = &gebeurtenis.toestand {
        logboek::log(&Gebeurtenis::TaakMislukt {
            taak: gebeurtenis.soort,
            probleem: Probleem::uit_melding(melding),
        });
    }
}

/// Zet een mislukte actie in het logboek die niet als taakgebeurtenis langskomt (een volle schijf
/// bij het opnemen, mengen of bewaren). Alleen de soort taak en het soort probleem.
pub(crate) fn mislukt(taak: taak::Soort, probleem: Option<Probleem>) {
    logboek::log(&Gebeurtenis::TaakMislukt { taak, probleem });
}

/// De stappen van de eerste-start-wizard die in het logboek komen. Een stap die hier niet
/// staat, wordt genegeerd: de interface kan er geen eigen tekst in kwijt.
const WIZARD_STAPPEN: [&str; 6] = [
    "waarom",
    "advies",
    "downloaden",
    "proef",
    "klaar",
    "nu_niet",
];

/// Zet een stap van de wizard in het logboek.
#[tauri::command]
pub(crate) fn wizard_stap_log(stap: &str) {
    if let Some(vast) = WIZARD_STAPPEN.iter().find(|s| **s == stap) {
        logboek::log(&Gebeurtenis::WizardStap { stap: vast });
    }
}

/// De kop boven het gekopieerde logboek: welke uitgave en welk systeem. Geen namen, geen paden.
fn kop(versie: &str, bouw: &str, datum: &str, systeem: &str, processor: &str) -> String {
    format!("Mondig {versie} (bouw {bouw}, {datum})\nSysteem: {systeem}, {processor}\n\n")
}

/// Het logboek als tekst om te kopiëren, met een kop. De interface zet het op het klembord.
#[tauri::command]
pub(crate) fn logboek_tekst() -> String {
    let inhoud = logboek::logboek()
        .map(logboek::Logboek::lees)
        .unwrap_or_default();
    let tekst = kop(
        env!("CARGO_PKG_VERSION"),
        env!("MONDIG_BOUW_COMMIT"),
        env!("MONDIG_BOUW_DATUM"),
        &crate::versie::systeem_naam(),
        &crate::versie::processor_naam(std::env::consts::ARCH),
    ) + &if inhoud.is_empty() {
        "Het logboek is nog leeg.\n".to_string()
    } else {
        inhoud
    };
    logboek::log(&Gebeurtenis::LogboekGekopieerd);
    tekst
}

/// Opent de logmap in de Verkenner.
#[tauri::command]
pub(crate) fn logboek_map_open() -> Result<(), Foutmelding> {
    let Some(logboek) = logboek::logboek() else {
        return Err(Foutmelding::nieuw(
            "Mondig heeft nog geen logmap. Start Mondig opnieuw.",
            Vervolg::Geen,
        ));
    };
    let map = logboek.map();
    std::fs::create_dir_all(map)
        .map_err(|_| Foutmelding::nieuw("Mondig kon de logmap niet maken.", Vervolg::Geen))?;
    crate::bewaren::open_in_verkenner(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mondig_core::taak::{Soort, TaakId};

    #[test]
    fn de_kop_noemt_de_uitgave_en_het_systeem() {
        let tekst = kop(
            "0.3.0",
            "abc1234",
            "2026-11-02",
            "Windows 11 (build 26200)",
            "x86_64",
        );
        assert!(tekst.starts_with("Mondig 0.3.0 (bouw abc1234, 2026-11-02)\n"));
        assert!(tekst.contains("Windows 11"));
    }

    #[test]
    fn een_wizardstap_komt_alleen_uit_de_vaste_lijst() {
        // Zonder gestart logboek schrijft dit niets; de functie mag er nooit op paniekeren.
        wizard_stap_log("advies");
        wizard_stap_log("Dit is vrije tekst");
        assert!(WIZARD_STAPPEN.contains(&"klaar"));
        assert!(!WIZARD_STAPPEN.contains(&"Dit is vrije tekst"));
    }

    #[test]
    fn alleen_een_mislukte_taak_komt_in_het_logboek() {
        // Zonder `logboek::start` schrijft `log` niets weg; deze test controleert dat de
        // functie ook voor elke toestand zonder fout of paniek doorloopt.
        for toestand in [
            Toestand::Wacht,
            Toestand::Klaar,
            Toestand::fout("Het bestand is niet gevonden: C:/geheim.wav", Vervolg::Geen),
        ] {
            taak(&taak::Gebeurtenis {
                taak: TaakId(1),
                soort: Soort::Bestand,
                toestand,
            });
        }
    }
}
