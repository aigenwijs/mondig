//! Versiegegevens en naamsvermelding voor het scherm Over Mondig (ADR 0016), plus de
//! laatst geziene versie voor de kaart "Wat is er nieuw".
//!
//! De versie komt uit de workspace (`CARGO_PKG_VERSION`); de korte commit en de bouwdatum
//! zet `build.rs` bij het compileren. Niets hiervan gaat het netwerk op.

use std::sync::LazyLock;

use mondig_core::engine::supervisor::{self, Bouwcontrole};
use mondig_core::model::{self, Gebruik, Model};
use serde::Serialize;
use tauri::AppHandle;

use crate::sneltoets::{lees_instelling, schrijf_instelling};

/// Sleutel: de versie waarvoor de gebruiker "Wat is er nieuw" al zag.
pub(crate) const SLEUTEL_LAATST_GEZIEN: &str = "laatst_geziene_versie";

/// De bouwcode van de app: versie plus commit. Het werkproces meldt dezelfde vorm
/// (`mondig_engine::BOUWCODE`); wijken ze af, dan is de installatie niet compleet.
const BOUWCODE: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("MONDIG_BOUW_COMMIT"));

/// De controle of het werkproces bij deze app past. Een debugbuild en een bouw met
/// `-dirty` waarschuwen alleen in het log, zodat lokaal werken kan; een uitgave weigert
/// een werkproces van een andere bouw.
pub(crate) fn bouwcontrole() -> Bouwcontrole {
    Bouwcontrole::voor_app(BOUWCODE, cfg!(debug_assertions))
}

/// Het actieve model, voor in de te kopiëren versiegegevens.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActiefModel {
    id: String,
    naam: String,
}

/// Wat de interface over de uitgave weet. Geen persoonsgegevens.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VersieInfo {
    /// Semver uit de workspace, bijvoorbeeld `0.3.0-dev`.
    versie: String,
    /// Korte commit, of `onbekend` zonder git.
    bouw: String,
    /// Bouwdatum als `jjjj-mm-dd`.
    datum: String,
    /// In gewone taal: `Windows 11 (build 26200)`, `macOS` of `Linux`.
    systeem: String,
    /// Bijvoorbeeld `x86-64`.
    processor: String,
    model: ActiefModel,
}

/// De naamsvermelding van één model, uit `modellen.json`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelVermelding {
    id: String,
    naam: String,
    hulpmodel: bool,
    maker: String,
    titel: String,
    copyright: Option<String>,
    bron_url: String,
    gguf_url: String,
    licentie: String,
    licentie_url: String,
    disclaimer_url: String,
    wijziging: String,
}

/// Maakt een leesbare naam van een processorarchitectuur zoals Rust die noemt.
pub(crate) fn processor_naam(arch: &str) -> String {
    match arch {
        "x86_64" => "x86-64".to_string(),
        "aarch64" => "ARM64".to_string(),
        "x86" => "x86 (32 bit)".to_string(),
        andere => andere.to_string(),
    }
}

/// Haalt uit de uitvoer van `ver` het versienummer (`10.0.26200.1234`) en maakt er
/// `Windows 11 (build 26200)` van. Build 22000 of hoger is Windows 11. Werkt voor elke
/// taal van Windows, want we zoeken het getal en niet het woord ervoor.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_uit_ver(uitvoer: &str) -> Option<String> {
    let nummer = uitvoer
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .find(|deel| deel.split('.').count() == 4 && deel.split('.').all(|d| !d.is_empty()))?;
    let build: u32 = nummer.split('.').nth(2)?.parse().ok()?;
    let naam = if build >= 22_000 { 11 } else { 10 };
    Some(format!("Windows {naam} (build {build})"))
}

/// De Windows-versie. Via `ver`, zonder venster; lukt dat niet, dan alleen "Windows".
#[cfg(windows)]
pub(crate) fn systeem_naam() -> String {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW: geen console die even oplicht.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("cmd")
        .args(["/C", "ver"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
        .and_then(|uitvoer| windows_uit_ver(&String::from_utf8_lossy(&uitvoer.stdout)))
        .unwrap_or_else(|| "Windows".to_string())
}

#[cfg(not(windows))]
pub(crate) fn systeem_naam() -> String {
    match std::env::consts::OS {
        "macos" => "macOS".to_string(),
        "linux" => "Linux".to_string(),
        ander => ander.to_string(),
    }
}

/// Versie, bouw, datum, systeem en actief model voor het scherm Over Mondig en de knop
/// "Versiegegevens kopiëren".
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn versie_info(app: AppHandle) -> Result<VersieInfo, String> {
    let (actief, _) =
        crate::dicteerketen::model_voor(&app, Gebruik::Dicteren).map_err(|fout| fout.melding)?;
    Ok(versie_met(actief))
}

/// De versiegegevens met dit model als het actieve.
fn versie_met(actief: Model) -> VersieInfo {
    VersieInfo {
        versie: env!("CARGO_PKG_VERSION").to_string(),
        bouw: env!("MONDIG_BOUW_COMMIT").to_string(),
        datum: env!("MONDIG_BOUW_DATUM").to_string(),
        systeem: systeem_naam(),
        processor: processor_naam(std::env::consts::ARCH),
        model: ActiefModel {
            id: actief.id,
            naam: actief.naam,
        },
    }
}

/// Wat het laatst gestarte werkproces over zichzelf meldde (bouwcode en protocolversie), voor
/// "Versiegegevens kopiëren". `None` als er sinds de start van Mondig nog geen werkproces
/// gestart is. Apart van [`versie_info`], want de interface bewaart die één keer en het
/// werkproces start pas als het nodig is (ADR 0012, aanvulling 2-11-2026).
#[tauri::command]
pub(crate) fn werkproces_bouw() -> Option<String> {
    supervisor::laatst_gemelde_bouw()
}

/// De naamsvermelding van alle modellen, één keer samengesteld: de catalogus verandert niet
/// zolang de app draait, dus hij hoeft niet bij elke aanroep opnieuw gelezen te worden.
static VERMELDINGEN: LazyLock<Result<Vec<ModelVermelding>, String>> = LazyLock::new(|| {
    let catalogus = model::catalogus().map_err(|fout| fout.to_string())?;
    let vermeldingen = model::vermeldingen().map_err(|fout| fout.to_string())?;
    catalogus
        .into_iter()
        .map(|m| {
            let v = vermeldingen
                .iter()
                .find(|v| v.id == m.id)
                .ok_or_else(|| format!("{}: geen naamsvermelding", m.id))?;
            Ok(ModelVermelding {
                hulpmodel: m.rol != model::Rol::Spraak,
                id: m.id,
                naam: m.naam,
                licentie: m.licentie,
                maker: v.maker.clone(),
                titel: v.titel.clone(),
                copyright: v.copyright.clone(),
                bron_url: v.bron_url.clone(),
                gguf_url: v.gguf_url.clone(),
                licentie_url: v.licentie_url.clone(),
                disclaimer_url: v.disclaimer_url.clone(),
                wijziging: v.wijziging.clone(),
            })
        })
        .collect()
});

/// De naamsvermelding van alle modellen, uit dezelfde catalogus als de downloads.
#[tauri::command]
pub(crate) fn model_vermeldingen() -> Result<Vec<ModelVermelding>, String> {
    VERMELDINGEN.clone()
}

/// Mag dit adres geopend worden? Alleen de adressen uit de naamsvermelding van de modellen:
/// de interface kan Mondig zo niet vragen een willekeurige pagina te openen.
fn is_bekend_adres(adres: &str) -> bool {
    VERMELDINGEN.as_ref().is_ok_and(|lijst| {
        lijst.iter().any(|v| {
            [&v.bron_url, &v.gguf_url, &v.licentie_url, &v.disclaimer_url]
                .iter()
                .any(|a| a.as_str() == adres)
        })
    })
}

/// Opent een adres uit de naamsvermelding in de standaardbrowser. De webview zelf gaat niet
/// weg: een link in de interface zou anders het venster van Mondig vervangen.
#[tauri::command]
pub(crate) fn open_vermeldingsadres(adres: &str) -> Result<(), String> {
    if !is_bekend_adres(adres) {
        return Err("onbekend adres".to_string());
    }
    #[cfg(windows)]
    let opener = "explorer";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let opener = "xdg-open";
    std::process::Command::new(opener)
        .arg(adres)
        .spawn()
        .map(|_| ())
        .map_err(|fout| fout.to_string())
}

/// De versie waarvoor "Wat is er nieuw" al getoond is; `None` bij een eerste start.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn laatst_geziene_versie(app: AppHandle) -> Option<String> {
    lees_instelling(&app, SLEUTEL_LAATST_GEZIEN)
}

/// Bewaart de versie waarvoor "Wat is er nieuw" is gesloten.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn laatst_geziene_versie_zet(app: AppHandle, versie: String) {
    schrijf_instelling(&app, SLEUTEL_LAATST_GEZIEN, &versie);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_11_uit_nederlandse_uitvoer() {
        let uitvoer = "\r\nMicrosoft Windows [Versie 10.0.26200.6584]\r\n";
        assert_eq!(
            windows_uit_ver(uitvoer).as_deref(),
            Some("Windows 11 (build 26200)")
        );
    }

    #[test]
    fn windows_10_uit_engelse_uitvoer() {
        let uitvoer = "Microsoft Windows [Version 10.0.19045.5555]";
        assert_eq!(
            windows_uit_ver(uitvoer).as_deref(),
            Some("Windows 10 (build 19045)")
        );
    }

    #[test]
    fn onleesbare_uitvoer_geeft_niets() {
        assert_eq!(windows_uit_ver(""), None);
        assert_eq!(windows_uit_ver("geen versie hier"), None);
    }

    #[test]
    fn processor_krijgt_een_leesbare_naam() {
        assert_eq!(processor_naam("x86_64"), "x86-64");
        assert_eq!(processor_naam("aarch64"), "ARM64");
        assert_eq!(processor_naam("riscv64"), "riscv64");
    }

    #[test]
    fn versiegegevens_hebben_een_bouw_en_datum() {
        let info = versie_met(model::zoek(model::STANDAARD_MODEL).unwrap());
        assert!(!info.versie.is_empty());
        assert!(!info.bouw.is_empty());
        // jjjj-mm-dd
        assert_eq!(info.datum.len(), 10);
        assert_eq!(info.model.id, model::STANDAARD_MODEL);
    }

    #[test]
    fn alleen_adressen_uit_de_catalogus_zijn_toegestaan() {
        let eerste = model_vermeldingen().unwrap().remove(0);
        assert!(is_bekend_adres(&eerste.bron_url));
        assert!(is_bekend_adres(&eerste.disclaimer_url));
        assert!(!is_bekend_adres("https://example.invalid"));
        assert!(!is_bekend_adres("file:///C:/Windows/System32/calc.exe"));
    }

    #[test]
    fn elk_model_komt_in_de_vermeldingen() {
        let lijst = model_vermeldingen().unwrap();
        assert_eq!(lijst.len(), model::catalogus().unwrap().len());
        assert!(lijst.iter().any(|v| v.hulpmodel));
    }
}
