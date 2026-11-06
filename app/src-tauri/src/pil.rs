//! De zwevende pil (sprint 3d): een tweede venster dat
//! tijdens een dictaat onder (of boven) in beeld staat, op het scherm waar je werkt.
//!
//! Het venster neemt nooit de focus over, anders plakt de tekst in de pil in plaats van in
//! het programma waar je typt. Daarvoor drie lagen:
//!
//! 1. `focusable(false)` en `focused(false)`: Tauri zet dan `WS_EX_NOACTIVATE`, zodat Windows
//!    het venster bij een klik niet naar de voorgrond haalt. Daarnaast `WS_EX_TOOLWINDOW`:
//!    nooit in de taakbalk of in Alt+Tab.
//! 2. Tonen en verplaatsen via Win32 met `SWP_NOACTIVATE`, en `WM_MOUSEACTIVATE` beantwoorden
//!    met `MA_NOACTIVATE`: ook een klik in de webview activeert het venster niet.
//! 3. Krijgt de pil toch de focus (een webview die zichzelf focust), dan gaat die meteen
//!    terug naar het venster waar je werkte (bijgehouden bij elke klik en activering).
//!
//! Alleen op Windows. Op macOS en Linux zou `show()` het venster activeren; daar toont
//! Mondig de pil pas in sprint 3e en blijft de tekst in het venster.
//!
//! Wat de pil toont, beslist de interface (`src/routes/pil`) uit dezelfde `taak`-events als
//! het hoofdvenster. Zij meet zichzelf en vraagt dan `pil_toon` met haar maat in CSS-pixels;
//! Rust kiest het scherm en de plek. Rust meldt met `pil-taak` welk dictaat bij de sneltoets
//! hoort (de proef in het venster krijgt geen pil) en stuurt tijdens het opnemen het niveau
//! als `pil-niveau`. Er wordt niets bewaard.

// De plaatsing via Win32 heeft nog geen tegenhanger op macOS en Linux (sprint 3e).
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicU64, Ordering};

use mondig_core::taak::TaakId;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::sneltoets::{lees_instelling, schrijf_instelling, wis_instelling};

#[cfg(windows)]
mod win32;

/// Het label van het venster.
pub(crate) const LABEL: &str = "pil";

/// Sleutel in de store: staat er "boven", dan staat de pil bovenaan; anders onderaan.
const SLEUTEL_POSITIE: &str = "pil_positie";

/// Het laatste dictaat van de sneltoets (0: nog geen).
static LAATSTE_TAAK: AtomicU64 = AtomicU64::new(0);
/// Het dictaat waarvan de opname nu loopt (0: geen).
static OPNAME: AtomicU64 = AtomicU64::new(0);

/// Afstand tot de rand van het werkgebied, in logische pixels. De pil zelf heeft 8 px
/// doorzichtige rand voor de schaduw, dus de zichtbare afstand is ongeveer 22 px.
const MARGE: f64 = 14.0;

/// Boven of onder in beeld.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Plek {
    #[default]
    Onder,
    Boven,
}

/// Een rechthoek in fysieke pixels (het werkgebied van een scherm: zonder taakbalk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rechthoek {
    pub(crate) links: i32,
    pub(crate) boven: i32,
    pub(crate) rechts: i32,
    pub(crate) onder: i32,
}

/// Linksboven van de pil in fysieke pixels: horizontaal in het midden van het werkgebied,
/// `marge` boven de onderrand (of onder de bovenrand). Past de pil niet, dan blijft hij
/// binnen het werkgebied, met de linker- en bovenkant zichtbaar.
pub(crate) fn positie(
    werkgebied: Rechthoek,
    maat: (i32, i32),
    plek: Plek,
    marge: i32,
) -> (i32, i32) {
    let (breedte, hoogte) = maat;
    let werkbreedte = werkgebied.rechts - werkgebied.links;
    let x = werkgebied.links + (werkbreedte - breedte).max(0) / 2;
    let y = match plek {
        Plek::Onder => (werkgebied.onder - hoogte - marge).max(werkgebied.boven),
        Plek::Boven => werkgebied.boven + marge,
    };
    (x, y)
}

/// Wordt dit venster het nieuwe doel van de focus? Niet als het leeg is of de pil zelf;
/// een venster van Mondig zelf mag wel (dan krijgt dat de focus terug).
pub(crate) fn nieuw_doel(kandidaat: isize, pil: isize) -> Option<isize> {
    (kandidaat != 0 && kandidaat != pil).then_some(kandidaat)
}

/// Een maat in CSS-pixels naar fysieke pixels, naar boven afgerond.
#[expect(
    clippy::cast_possible_truncation,
    reason = "de waarde is begrensd tot een schermmaat"
)]
pub(crate) fn naar_fysiek(css: f64, schaal: f64) -> i32 {
    (css.max(0.0) * schaal).ceil().min(f64::from(i16::MAX)) as i32
}

/// De bewaarde plek.
pub(crate) fn lees_plek(app: &AppHandle) -> Plek {
    if lees_instelling(app, SLEUTEL_POSITIE).as_deref() == Some("boven") {
        Plek::Boven
    } else {
        Plek::Onder
    }
}

/// Maakt het (verborgen) venster bij de start van de app.
pub(crate) fn maak(app: &AppHandle) -> tauri::Result<()> {
    let venster = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("pil".into()))
        .title("Mondig")
        .inner_size(360.0, 60.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()?;
    #[cfg(windows)]
    if let Ok(hwnd) = venster.hwnd() {
        win32::richt_in(hwnd.0 as isize);
    }
    #[cfg(not(windows))]
    let _ = venster;
    Ok(())
}

/// De interface heeft zich gemeten: toon de pil met deze maat (CSS-pixels) op het scherm
/// waar je werkt. Mag vaker komen; elke keer opnieuw gemeten en geplaatst.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn pil_toon(app: AppHandle, breedte: f64, hoogte: f64) {
    let plek = lees_plek(&app);
    let Some(venster) = app.get_webview_window(LABEL) else {
        return;
    };
    #[cfg(windows)]
    if let Ok(hwnd) = venster.hwnd() {
        win32::toon(hwnd.0 as isize, (breedte, hoogte), plek);
    }
    // Buiten Windows nog niet tonen (sprint 3e).
    #[cfg(not(windows))]
    let _ = (venster, breedte, hoogte, plek);
}

/// Verbergt de pil.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn pil_verberg(app: AppHandle) {
    verberg(&app);
}

/// Verbergt de pil (ook bij afsluiten).
pub(crate) fn verberg(app: &AppHandle) {
    let Some(venster) = app.get_webview_window(LABEL) else {
        return;
    };
    #[cfg(windows)]
    if let Ok(hwnd) = venster.hwnd() {
        win32::verberg(hwnd.0 as isize);
    }
    #[cfg(not(windows))]
    let _ = venster.hide();
}

/// De pil kreeg toch de focus: geef die meteen terug aan het venster van daarvoor.
pub(crate) fn kreeg_focus() {
    #[cfg(windows)]
    win32::geef_focus_terug();
}

/// Meldt de pil dat dit dictaat van de sneltoets is.
pub(crate) fn meld_dictaat(app: &AppHandle, taak: TaakId) {
    LAATSTE_TAAK.store(taak.0, Ordering::SeqCst);
    let _ = app.emit_to(LABEL, "pil-taak", taak);
}

/// Houdt bij welke opname loopt, voor een pil die later laadt (`pil_stand`).
pub(crate) fn opname_loopt(taak: Option<TaakId>) {
    OPNAME.store(taak.map_or(0, |t| t.0), Ordering::SeqCst);
}

/// Wat de pil bij het laden opvraagt: events van vóór het laden van de webview zijn weg.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct PilStand {
    /// Het laatste dictaat van de sneltoets.
    taak: Option<TaakId>,
    /// Loopt de opname van dat dictaat nog?
    luistert: bool,
}

/// De stand voor een pil die net geladen is.
pub(crate) fn stand_van(laatste: u64, opname: u64) -> PilStand {
    PilStand {
        taak: (laatste != 0).then_some(TaakId(laatste)),
        luistert: laatste != 0 && opname == laatste,
    }
}

/// Het laatste dictaat en of de opname nog loopt.
#[tauri::command]
pub(crate) fn pil_stand() -> PilStand {
    stand_van(
        LAATSTE_TAAK.load(Ordering::SeqCst),
        OPNAME.load(Ordering::SeqCst),
    )
}

/// Het niveau van de microfoon tijdens het opnemen, voor de meter (0 tot 1).
pub(crate) fn meld_niveau(app: &AppHandle, niveau: f32) {
    let _ = app.emit_to(LABEL, "pil-niveau", niveau);
}

/// Opent het hoofdvenster op een vaste route, voor de vervolgactie "Mondig openen".
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn pil_open_mondig(app: AppHandle) {
    crate::toon_venster(&app);
    let _ = app.emit_to("main", "navigeer", "/modellen");
}

/// De bewaarde plek, voor het keuzemenu.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn pil_positie_instelling(app: AppHandle) -> Plek {
    lees_plek(&app)
}

/// Bewaart de plek; de volgende keer dat de pil verschijnt, staat hij daar.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn pil_positie_zet(app: AppHandle, plek: Plek) {
    match plek {
        Plek::Boven => schrijf_instelling(&app, SLEUTEL_POSITIE, "boven"),
        Plek::Onder => wis_instelling(&app, SLEUTEL_POSITIE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHERM: Rechthoek = Rechthoek {
        links: 0,
        boven: 0,
        rechts: 1920,
        onder: 1032,
    };

    #[test]
    fn onder_in_het_midden_boven_de_taakbalk() {
        // 1920 x 1080 met een taakbalk van 48 px: het werkgebied eindigt op 1032.
        assert_eq!(positie(SCHERM, (400, 60), Plek::Onder, 14), (760, 958));
    }

    #[test]
    fn boven_in_het_midden() {
        assert_eq!(positie(SCHERM, (400, 60), Plek::Boven, 14), (760, 14));
    }

    #[test]
    fn tweede_scherm_links_van_het_eerste_met_taakbalk_bovenaan() {
        // Een scherm links van het hoofdscherm heeft negatieve coördinaten; de taakbalk
        // staat hier bovenaan, dus het werkgebied begint op 40.
        let links = Rechthoek {
            links: -2560,
            boven: 40,
            rechts: 0,
            onder: 1440,
        };
        assert_eq!(positie(links, (500, 90), Plek::Onder, 21), (-1530, 1329));
        assert_eq!(positie(links, (500, 90), Plek::Boven, 21), (-1530, 61));
    }

    #[test]
    fn te_brede_pil_blijft_binnen_het_werkgebied() {
        let smal = Rechthoek {
            links: 100,
            boven: 0,
            rechts: 400,
            onder: 50,
        };
        assert_eq!(positie(smal, (500, 80), Plek::Onder, 14), (100, 0));
    }

    #[test]
    fn doel_is_nooit_leeg_of_de_pil_zelf() {
        assert_eq!(nieuw_doel(0, 5), None);
        assert_eq!(nieuw_doel(5, 5), None);
        assert_eq!(nieuw_doel(9, 5), Some(9));
    }

    #[test]
    fn stand_voor_een_pil_die_later_laadt() {
        assert_eq!(
            stand_van(0, 0),
            PilStand {
                taak: None,
                luistert: false
            }
        );
        assert_eq!(
            stand_van(7, 7),
            PilStand {
                taak: Some(TaakId(7)),
                luistert: true
            }
        );
        assert_eq!(
            stand_van(7, 0),
            PilStand {
                taak: Some(TaakId(7)),
                luistert: false
            }
        );
    }

    #[test]
    fn css_pixels_naar_fysiek_met_schaal() {
        assert_eq!(naar_fysiek(400.0, 1.0), 400);
        assert_eq!(naar_fysiek(400.4, 1.5), 601);
        assert_eq!(naar_fysiek(14.0, 1.25), 18);
        assert_eq!(naar_fysiek(-3.0, 2.0), 0);
    }
}
