//! Microfoon geblokkeerd door Windows (sprint 3d): herkennen, uitleggen in gewone taal, en
//! een knop naar de juiste instelling.
//!
//! Herkennen gaat in twee stappen. De opnamelaag krijgt van Windows "toegang geweigerd"
//! (`E_ACCESSDENIED`) als de privacy-instelling de microfoon blokkeert; `handy-recorder`
//! maakt daar `PermissionDenied` van (`backend/cpal/error.rs`, versie 0.1.0) en wij
//! `FoutSoort::ToegangGeweigerd`. Daarnaast lezen we na zo'n fout, of na een opname zonder
//! geluid, de privacy-schakelaars in het register.
//!
//! Let op: de betekenis van die registerwaarden is niet officieel gedocumenteerd door
//! Microsoft. Bekend uit de praktijk (elevenforum.com, "Enable or Disable Apps Access to
//! Microphone in Windows 11", geraadpleegd 1-11-2026): onder
//! `CapabilityAccessManager\ConsentStore\microphone` staat `Value` = `Allow` of `Deny`.
//! In HKLM is dat de schakelaar voor het hele apparaat, in HKCU de hoofdschakelaar voor
//! deze gebruiker, en `NonPackaged\Value` in HKCU die voor bureaublad-apps zoals Mondig.
//! Alleen die drie blokkeren een bureaublad-app; de schakelaars per Store-app tellen niet.
//! Daarom gebruiken we ze alleen om een toegangsfout of stilte beter uit te leggen, nooit
//! om een dictaat vooraf tegen te houden, en nooit bij bijvoorbeeld "geen microfoon".
//!
//! De knop opent `ms-settings:privacy-microphone` als de microfoon geblokkeerd is, en anders
//! `ms-settings:sound` (geen microfoon, gedempt, geen geluid): daar kies of test je een
//! microfoon. Openen via `ShellExecuteW` en niet via de opener-plugin: het adres staat vast
//! in Rust, er komt geen afhankelijkheid bij en de webview krijgt geen recht om zelf
//! adressen of programma's te openen.

// Alleen Windows heeft deze privacy-schakelaars; macOS en Linux volgen in sprint 3e.
#![cfg_attr(not(windows), allow(dead_code))]

use mondig_core::opname::{FoutSoort, OpnameFout};
use mondig_core::taak::{Foutmelding, Vervolg};

#[cfg(windows)]
mod win32;

/// De stand van de drie schakelaars die een bureaublad-app blokkeren (`None`: niet gevonden).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Schakelaars {
    /// "Toegang tot de microfoon" voor het hele apparaat (HKLM).
    pub(crate) apparaat: Option<String>,
    /// De hoofdschakelaar "Apps toegang geven tot de microfoon" voor deze gebruiker (HKCU).
    pub(crate) apps: Option<String>,
    /// "Bureaublad-apps toegang geven tot de microfoon" (HKCU, `NonPackaged`).
    pub(crate) bureaublad_apps: Option<String>,
}

/// Blokkeert Windows de microfoon voor Mondig volgens deze schakelaars?
pub(crate) fn geblokkeerd(schakelaars: &Schakelaars) -> bool {
    let weigert = |waarde: &Option<String>| {
        waarde
            .as_deref()
            .is_some_and(|w| w.eq_ignore_ascii_case("deny"))
    };
    weigert(&schakelaars.apparaat)
        || weigert(&schakelaars.apps)
        || weigert(&schakelaars.bureaublad_apps)
}

/// Mag deze fout uitgelegd worden als een blokkade van Windows? Alleen een toegangsfout,
/// een microfoon die geen geluid doorgeeft, of een opname met alleen stilte (`stilte`).
/// Niet bij "geen microfoon gevonden", "bezet", "losgekoppeld" en dergelijke.
pub(crate) fn mag_verfijnen(fout: &Foutmelding, stilte: bool) -> bool {
    stilte
        || *fout == Foutmelding::from(OpnameFout::nieuw(FoutSoort::ToegangGeweigerd))
        || *fout == Foutmelding::from(OpnameFout::nieuw(FoutSoort::GeenGeluid))
}

/// De melding als Windows de microfoon blokkeert.
pub(crate) fn melding_geblokkeerd() -> Foutmelding {
    Foutmelding::nieuw(
        "Windows laat Mondig de microfoon niet gebruiken. Zet in de Windows-instellingen bij Privacy en beveiliging, Microfoon, de toegang voor apps en voor bureaublad-apps aan.",
        Vervolg::MicrofooninstellingOpenen,
    )
}

/// Blokkeert Windows nu de microfoon?
pub(crate) fn is_geblokkeerd() -> bool {
    #[cfg(windows)]
    {
        geblokkeerd(&win32::lees_schakelaars())
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Een toegangsfout of stilte, beter uitgelegd als Windows de microfoon blokkeert. Elke
/// andere fout blijft zoals hij is. `stilte`: de opname bevatte alleen stilte.
pub(crate) fn verfijn(fout: Foutmelding, stilte: bool) -> Foutmelding {
    if mag_verfijnen(&fout, stilte) && is_geblokkeerd() {
        melding_geblokkeerd()
    } else {
        fout
    }
}

/// Welke instelling de knop opent.
pub(crate) fn instelling_adres(geblokkeerd: bool) -> &'static str {
    if geblokkeerd {
        "ms-settings:privacy-microphone"
    } else {
        "ms-settings:sound"
    }
}

/// De vervolgactie "Microfooninstelling openen". Op de Mac altijd Privacy en beveiliging,
/// Microfoon: daar staat of Mondig de microfoon mag gebruiken (mijlpaal M1).
#[tauri::command]
pub(crate) fn microfooninstelling_open() {
    let adres = instelling_adres(is_geblokkeerd());
    #[cfg(windows)]
    win32::open_adres(adres);
    #[cfg(target_os = "macos")]
    {
        let _ = adres;
        let _ = mondig_mac::rechten::open_adres(mondig_mac::ADRES_MICROFOON);
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = adres;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn met(apparaat: &str, apps: &str, bureaublad: &str) -> Schakelaars {
        let w = |s: &str| (!s.is_empty()).then(|| s.to_string());
        Schakelaars {
            apparaat: w(apparaat),
            apps: w(apps),
            bureaublad_apps: w(bureaublad),
        }
    }

    #[test]
    fn alles_aan_is_niet_geblokkeerd() {
        assert!(!geblokkeerd(&met("Allow", "Allow", "Allow")));
        assert!(!geblokkeerd(&Schakelaars::default()));
    }

    #[test]
    fn elke_schakelaar_voor_bureaublad_apps_blokkeert() {
        assert!(geblokkeerd(&met("Deny", "Allow", "Allow")));
        assert!(geblokkeerd(&met("Allow", "Deny", "Allow")));
        assert!(geblokkeerd(&met("", "", "deny")));
    }

    #[test]
    fn alleen_toegangsfout_of_stilte_wordt_verfijnd() {
        let toegang = Foutmelding::from(OpnameFout::nieuw(FoutSoort::ToegangGeweigerd));
        let geen_geluid = Foutmelding::from(OpnameFout::nieuw(FoutSoort::GeenGeluid));
        let geen_microfoon = Foutmelding::from(OpnameFout::nieuw(FoutSoort::GeenMicrofoon));
        let bezet = Foutmelding::from(OpnameFout::nieuw(FoutSoort::Bezet));
        assert!(mag_verfijnen(&toegang, false));
        assert!(mag_verfijnen(&geen_geluid, false));
        assert!(!mag_verfijnen(&geen_microfoon, false));
        assert!(!mag_verfijnen(&bezet, false));
        // Een opname met alleen stilte: dat kan een blokkade zijn.
        assert!(mag_verfijnen(&geen_microfoon, true));
    }

    #[test]
    fn knop_kiest_de_juiste_instelling() {
        assert_eq!(instelling_adres(true), "ms-settings:privacy-microphone");
        assert_eq!(instelling_adres(false), "ms-settings:sound");
    }

    #[test]
    fn melding_heeft_de_knop_als_vervolg() {
        assert_eq!(
            melding_geblokkeerd().vervolg,
            Vervolg::MicrofooninstellingOpenen
        );
    }
}
