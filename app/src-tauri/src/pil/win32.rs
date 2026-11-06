//! De Windows-kant van de pil: stijlen zonder activering, tonen en plaatsen zonder de
//! focus te pakken, en het scherm van het actieve venster vinden.
//!
//! Bronnen (geraadpleegd 1-11-2026):
//! - `WS_EX_NOACTIVATE`: "A top-level window created with this style does not become the
//!   foreground window when the user clicks it"
//!   (learn.microsoft.com/windows/win32/winmsg/extended-window-styles).
//! - `WM_MOUSEACTIVATE` met `MA_NOACTIVATE`: "Does not activate the window, and does not
//!   discard the mouse message" (learn.microsoft.com/windows/win32/inputdev/wm-mouseactivate).
//!   Een kindvenster (de webview) geeft dit bericht door aan zijn ouder.
//! - `GetMonitorInfoW`, `rcWork`: het werkgebied zonder taakbalk
//!   (learn.microsoft.com/windows/win32/api/winuser/ns-winuser-monitorinfo).

#![expect(
    unsafe_code,
    reason = "vensterstijlen, plaatsing en focus kunnen alleen via de Win32-API"
)]

use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
    MonitorFromPoint, MonitorFromWindow,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, HWND_TOPMOST, IsWindow,
    MA_NOACTIVATE, SW_HIDE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SWP_SHOWWINDOW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_ACTIVATE,
    WM_MOUSEACTIVATE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

use super::{MARGE, Plek, Rechthoek, naar_fysiek, nieuw_doel, positie};

/// Het venster waar je werkt (0: onbekend). Bijgewerkt bij tonen, bij elke klik op de pil
/// (`WM_MOUSEACTIVATE`: het voorgrondvenster van dat moment) en als de pil toch actief wordt
/// (`WM_ACTIVATE`: lParam is het venster dat de activering verloor). Zo wint het venster
/// waar je nu bent, ook na Alt+Tab tijdens het opnemen. Gewist bij verbergen.
static DOEL: AtomicIsize = AtomicIsize::new(0);

fn onthoud_doel(kandidaat: HWND, pil: HWND) {
    if let Some(doel) = nieuw_doel(kandidaat.0 as isize, pil.0 as isize) {
        DOEL.store(doel, Ordering::SeqCst);
    }
}

/// Nummer van onze subclass op het venster van de pil.
const SUBCLASS_ID: usize = 0x6d6f_6e64;

fn hwnd(waarde: isize) -> HWND {
    HWND(waarde as *mut core::ffi::c_void)
}

/// Zet de stijlen zonder activering (opnieuw: Tauri kan de stijl bij een wijziging
/// herschrijven).
fn zet_stijl(venster: HWND) {
    // SAFETY: het venster bestaat; we lezen en schrijven alleen de uitgebreide stijl.
    unsafe {
        let stijl = GetWindowLongPtrW(venster, GWL_EXSTYLE);
        #[expect(clippy::cast_possible_wrap, reason = "vlaggen passen in een isize")]
        let nodig = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
        if stijl & nodig != nodig {
            SetWindowLongPtrW(venster, GWL_EXSTYLE, stijl | nodig);
            // Een gewijzigde stijl geldt pas na SWP_FRAMECHANGED. Bovenop blijven gaat via
            // HWND_TOPMOST bij het tonen, niet via de stijl.
            let _ = SetWindowPos(
                venster,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}

/// Beantwoordt een klik zonder activering; al het andere gaat door naar Tauri.
unsafe extern "system" fn subclass(
    venster: HWND,
    bericht: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if bericht == WM_MOUSEACTIVATE {
        // SAFETY: geen voorwaarden.
        onthoud_doel(unsafe { GetForegroundWindow() }, venster);
        #[expect(clippy::cast_possible_wrap, reason = "MA_NOACTIVATE is 3")]
        return LRESULT(MA_NOACTIVATE as isize);
    }
    // De pil wordt toch actief (de laagste 16 bits van wParam zijn niet WA_INACTIVE):
    // lParam is het venster dat de activering verloor, dus waar je werkte.
    if bericht == WM_ACTIVATE && (wparam.0 & 0xFFFF) != 0 {
        onthoud_doel(HWND(lparam.0 as *mut core::ffi::c_void), venster);
    }
    // SAFETY: doorgeven aan de volgende in de keten, met dezelfde argumenten.
    unsafe { DefSubclassProc(venster, bericht, wparam, lparam) }
}

/// Richt het venster in, op de hoofdthread (waar het venster is gemaakt).
pub(super) fn richt_in(waarde: isize) {
    let venster = hwnd(waarde);
    zet_stijl(venster);
    // SAFETY: het venster is van deze thread; de procedure leeft zo lang als het programma.
    unsafe {
        let _ = SetWindowSubclass(venster, Some(subclass), SUBCLASS_ID, 0);
    }
}

/// Het scherm van het actieve venster, of anders dat met de muis.
fn scherm_van_nu(eigen: HWND) -> HMONITOR {
    // SAFETY: geen voorwaarden; een ongeldig venster geeft het dichtstbijzijnde scherm.
    unsafe {
        let voorgrond = GetForegroundWindow();
        if !voorgrond.is_invalid() && voorgrond != eigen {
            return MonitorFromWindow(voorgrond, MONITOR_DEFAULTTONEAREST);
        }
        let mut muis = POINT::default();
        if GetCursorPos(&raw mut muis).is_ok() {
            return MonitorFromPoint(muis, MONITOR_DEFAULTTONEAREST);
        }
        MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY)
    }
}

/// Werkgebied en schaal (1,0 is 96 dpi) van een scherm.
fn gebied_en_schaal(scherm: HMONITOR) -> Option<(Rechthoek, f64)> {
    let mut info = MONITORINFO {
        cbSize: u32::try_from(size_of::<MONITORINFO>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `info` heeft de juiste grootte in `cbSize`.
    if !unsafe { GetMonitorInfoW(scherm, &raw mut info) }.as_bool() {
        return None;
    }
    let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
    // SAFETY: twee geldige uitvoerpunten.
    let _ = unsafe { GetDpiForMonitor(scherm, MDT_EFFECTIVE_DPI, &raw mut dpi_x, &raw mut dpi_y) };
    let werk = info.rcWork;
    Some((
        Rechthoek {
            links: werk.left,
            boven: werk.top,
            rechts: werk.right,
            onder: werk.bottom,
        },
        f64::from(dpi_x.max(1)) / 96.0,
    ))
}

/// Toont de pil zonder activering, met deze maat (CSS-pixels), op het scherm waar je werkt.
pub(super) fn toon(waarde: isize, maat: (f64, f64), plek: Plek) {
    let venster = hwnd(waarde);
    // SAFETY: geen voorwaarden.
    onthoud_doel(unsafe { GetForegroundWindow() }, venster);
    let Some((werkgebied, schaal)) = gebied_en_schaal(scherm_van_nu(venster)) else {
        return;
    };
    zet_stijl(venster);
    let fysiek = (naar_fysiek(maat.0, schaal), naar_fysiek(maat.1, schaal));
    let (x, y) = positie(werkgebied, fysiek, plek, naar_fysiek(MARGE, schaal));
    // Twee keer: gaat de pil naar een scherm met een andere schaal, dan past Windows de
    // maat aan tijdens de eerste verplaatsing (WM_DPICHANGED); de tweede zet hem goed.
    for _ in 0..2 {
        // SAFETY: het venster bestaat; SWP_NOACTIVATE laat de focus waar hij is.
        unsafe {
            let _ = SetWindowPos(
                venster,
                Some(HWND_TOPMOST),
                x,
                y,
                fysiek.0,
                fysiek.1,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
}

/// Verbergt de pil en vergeet het doel.
pub(super) fn verberg(waarde: isize) {
    DOEL.store(0, Ordering::SeqCst);
    // SAFETY: het venster bestaat; verbergen activeert niets.
    unsafe {
        let _ = ShowWindow(hwnd(waarde), SW_HIDE);
    }
}

/// Geeft de voorgrond terug aan het venster waar je werkte, ook als dat een venster van
/// Mondig zelf is. Mag omdat ons proces op dat moment zelf de voorgrond heeft (de pil kreeg
/// hem net). Is er geen doel, dan blijft de pil voorgrond; plakken ziet dan Mondig vooraan,
/// plakt niet en meldt "de tekst staat in Mondig" (`PlakUitkomst::InMondig`).
pub(super) fn geef_focus_terug() {
    let doel = hwnd(DOEL.load(Ordering::SeqCst));
    // SAFETY: we controleren eerst of het venster nog bestaat.
    unsafe {
        if doel.is_invalid() || !IsWindow(Some(doel)).as_bool() {
            return;
        }
        let _ = SetForegroundWindow(doel);
    }
}
