//! De computer uitlezen voor de hardwarecheck (fase 5, hoofdstuk 9): werkgeheugen,
//! processorkernen, vrije schijfruimte en videokaarten. Het advies zelf staat in
//! `mondig_core::hardware`; hier staat alleen wat het besturingssysteem moet vertellen.
//!
//! De videokaarten komen van een eigen, kortlevend werkproces (zie `videokaart.rs`), zodat
//! een driver die crasht de app niet raakt (ADR 0009).

use mondig_core::download::{self, ModelOverzicht};
use mondig_core::hardware::{self, Advies, Computer, Nodig, VideokaartStand};
use mondig_core::logboek::{self, Gebeurtenis};
use mondig_core::model::{self, Gebruik};
use mondig_core::snelheid::Vergelijking;
use mondig_core::taak::{Foutmelding, Vervolg};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::{JsonValue, StoreExt};

use crate::sneltoets::INSTELLINGEN;

#[cfg(windows)]
mod win32;

/// Het fysieke werkgeheugen in bytes, of `None` als het niet te meten is.
fn totaal_geheugen() -> Option<u64> {
    #[cfg(windows)]
    {
        win32::totaal_geheugen()
    }
    #[cfg(target_os = "linux")]
    {
        // `MemTotal:    16384000 kB` in /proc/meminfo.
        let tekst = std::fs::read_to_string("/proc/meminfo").ok()?;
        let regel = tekst.lines().find(|r| r.starts_with("MemTotal:"))?;
        let kb: u64 = regel.split_whitespace().nth(1)?.parse().ok()?;
        Some(kb * 1024)
    }
    #[cfg(target_os = "macos")]
    {
        let uitvoer = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        String::from_utf8(uitvoer.stdout).ok()?.trim().parse().ok()
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// Wat Mondig van deze computer weet. Meet vers: de vrije schijfruimte verandert.
pub(crate) fn lees_computer(app: &AppHandle) -> Computer {
    let vrij_bytes = model::modellen_map()
        .ok()
        .and_then(|map| download::vrije_ruimte(&map));
    Computer {
        geheugen_bytes: totaal_geheugen(),
        kernen: std::thread::available_parallelism()
            .ok()
            .and_then(|n| u32::try_from(n.get()).ok()),
        vrij_bytes,
        videokaarten: crate::videokaart::kaarten(app),
    }
}

/// Hoeveel er nog te downloaden is voor een model; 0 als het er helemaal staat of onbekend is.
fn nog_nodig(id: &str) -> u64 {
    download::overzicht_van_id(id).map_or(0, |o: ModelOverzicht| {
        o.grootte_bytes.saturating_sub(o.aanwezig_bytes)
    })
}

/// Het advies voor deze computer, met de gelezen gegevens erbij.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ComputerInfo {
    computer: Computer,
    advies: Advies,
    /// Welk model Mondig nu voor elke taak gebruikt ("In gebruik"), zodat de interface kan
    /// zeggen "dit staat er al".
    dicteren_in_gebruik: String,
    bestanden_in_gebruik: String,
}

/// Leest de computer uit en geeft het advies. Met `proef` (de uitkomst van de snelheidstest)
/// zegt het advies ook of de videokaart loont. Het advies wordt in het logboek gezet zonder
/// iets van de gebruiker: alleen getallen.
#[tauri::command]
pub(crate) async fn computer_check(
    app: AppHandle,
    proef: Option<Vergelijking>,
) -> Result<ComputerInfo, Foutmelding> {
    tauri::async_runtime::spawn_blocking(move || {
        let computer = lees_computer(&app);
        let nodig = Nodig {
            dicteren_bytes: nog_nodig(model::STANDAARD_MODEL),
            bestanden_bytes: nog_nodig(mondig_core::modelkeuze::STANDAARD_BESTANDEN),
        };
        let advies = hardware::adviseer(&computer, &nodig, proef.as_ref());
        logboek::log(&Gebeurtenis::ComputerGelezen {
            geheugen_mb: computer.geheugen_bytes.map(|b| b / 1_000_000),
            kernen: computer.kernen,
            vrij_mb: computer.vrij_bytes.map(|b| b / 1_000_000),
            videokaarten: computer
                .videokaarten
                .as_ref()
                .and_then(|k| u32::try_from(k.len()).ok()),
        });
        let in_gebruik = |gebruik| {
            crate::dicteerketen::model_voor(&app, gebruik)
                .map(|(m, _)| m.id)
                .unwrap_or_default()
        };
        ComputerInfo {
            computer,
            advies,
            dicteren_in_gebruik: in_gebruik(Gebruik::Dicteren),
            bestanden_in_gebruik: in_gebruik(Gebruik::Bestanden),
        }
    })
    .await
    .map_err(|_| {
        Foutmelding::nieuw(
            "Het lezen van deze computer stopte onverwacht.",
            Vervolg::OpnieuwProberen,
        )
    })
}

/// De sleutel in de instellingen voor de laatste controle van "Mondig opnieuw afstellen".
const SLEUTEL_LAATSTE_CONTROLE: &str = "laatste_controle";

/// De uitkomst van de laatste controle, voor "Past bij deze computer" op Modellen: alleen de
/// modelkeuzes, de stand van de videokaart en de datum. Geen namen of maten van de hardware.
/// Staat als JSON-waarde in de instellingen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaatsteControle {
    /// Het model dat het advies voor dicteren aanraadt.
    dicteren: String,
    /// Het model dat het advies voor opnames en bestanden aanraadt.
    bestanden: String,
    /// Wat het advies over de videokaart zei.
    videokaart: VideokaartStand,
    /// Wanneer, in milliseconden sinds 1970.
    datum_ms: u64,
}

impl LaatsteControle {
    /// Leest de bewaarde waarde terug. Alles wat niet klopt (een oude tekstwaarde, een onleesbare
    /// waarde, een onbekend model) is "nooit gecontroleerd", zodat er geen label komt dat Mondig
    /// niet kan onderbouwen.
    fn uit_waarde(waarde: JsonValue, bekend: &dyn Fn(&str) -> bool) -> Option<Self> {
        let controle = Self::deserialize(waarde).ok()?;
        (bekend(&controle.dicteren) && bekend(&controle.bestanden)).then_some(controle)
    }
}

/// Is dit een spraakmodel uit de catalogus?
fn is_spraakmodel(id: &str) -> bool {
    model::zoek(id).is_ok_and(|m| m.is_spraakmodel())
}

/// De laatste controle, of `None` als Mondig deze computer nog niet bekeek.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn laatste_controle(app: AppHandle) -> Option<LaatsteControle> {
    let waarde = app
        .store(INSTELLINGEN)
        .ok()?
        .get(SLEUTEL_LAATSTE_CONTROLE)?;
    LaatsteControle::uit_waarde(waarde, &is_spraakmodel)
}

/// Bewaart de uitkomst van een afgeronde controle, met de datum van nu.
///
/// # Errors
///
/// Geeft een melding als een model niet in de catalogus staat of de instellingen niet te
/// schrijven zijn.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn laatste_controle_bewaar(
    app: AppHandle,
    dicteren: String,
    bestanden: String,
    videokaart: VideokaartStand,
) -> Result<LaatsteControle, Foutmelding> {
    if !is_spraakmodel(&dicteren) || !is_spraakmodel(&bestanden) {
        return Err(Foutmelding::nieuw(
            "Dit model kent Mondig niet.",
            Vervolg::Geen,
        ));
    }
    let datum_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    let controle = LaatsteControle {
        dicteren,
        bestanden,
        videokaart,
        datum_ms,
    };
    let fout = || Foutmelding::nieuw("Mondig kon de controle niet bewaren.", Vervolg::Geen);
    let waarde = serde_json::to_value(&controle).map_err(|_| fout())?;
    let store = app.store(INSTELLINGEN).map_err(|_| fout())?;
    store.set(SLEUTEL_LAATSTE_CONTROLE, waarde);
    store.save().map_err(|_| fout())?;
    Ok(controle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn het_werkgeheugen_is_te_meten_en_redelijk() {
        let bytes = totaal_geheugen().expect("op deze testmachine is het geheugen te meten");
        // Tussen 512 MB en 16 TB: een gelezen eenheid die klopt, niet kB of pagina's.
        assert!(bytes > 512 * 1024 * 1024, "{bytes}");
        assert!(bytes < 16 * 1024 * 1024 * 1024 * 1024, "{bytes}");
    }

    fn controle() -> LaatsteControle {
        LaatsteControle {
            dicteren: "parakeet-ultra-q8".into(),
            bestanden: "canary-1b-v2-q8".into(),
            videokaart: VideokaartStand::Aanzetten,
            datum_ms: 1_793_820_806_215,
        }
    }

    #[test]
    fn de_laatste_controle_komt_terug_zoals_hij_bewaard_is() {
        let waarde = serde_json::to_value(controle()).unwrap();
        assert_eq!(
            waarde,
            serde_json::json!({
                "dicteren": "parakeet-ultra-q8",
                "bestanden": "canary-1b-v2-q8",
                "videokaart": "aanzetten",
                "datumMs": 1_793_820_806_215_u64
            })
        );
        assert_eq!(
            LaatsteControle::uit_waarde(waarde, &|_| true),
            Some(controle())
        );
    }

    #[test]
    fn een_oude_of_onleesbare_waarde_telt_als_nooit_gecontroleerd() {
        let bekend = |_: &str| true;
        // Het tekstformaat van de eerste versie van deze ronde.
        let oud = serde_json::json!("parakeet-ultra-q8|canary-1b-v2-q8|aanzetten|1");
        assert_eq!(LaatsteControle::uit_waarde(oud, &bekend), None);
        assert_eq!(
            LaatsteControle::uit_waarde(serde_json::json!(null), &bekend),
            None
        );
        let vliegt = serde_json::json!({
            "dicteren": "a", "bestanden": "b", "videokaart": "vliegt", "datumMs": 1
        });
        assert_eq!(LaatsteControle::uit_waarde(vliegt, &bekend), None);
        let zonder_datum = serde_json::json!({
            "dicteren": "a", "bestanden": "b", "videokaart": "geen"
        });
        assert_eq!(LaatsteControle::uit_waarde(zonder_datum, &bekend), None);
    }

    #[test]
    fn een_controle_met_een_onbekend_model_telt_niet() {
        let waarde = serde_json::to_value(controle()).unwrap();
        assert_eq!(
            LaatsteControle::uit_waarde(waarde, &|id| id != "canary-1b-v2-q8"),
            None
        );
    }

    #[test]
    fn de_modellen_uit_het_advies_zijn_spraakmodellen() {
        assert!(is_spraakmodel(model::STANDAARD_MODEL));
        assert!(is_spraakmodel(mondig_core::modelkeuze::STANDAARD_BESTANDEN));
        assert!(!is_spraakmodel("bestaat-niet"));
    }
}
