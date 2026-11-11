//! Rechten op de Mac: toegankelijkheid, het programma vooraan en de knoppen naar
//! Systeeminstellingen.
//!
//! Zonder het recht op toegankelijkheid houdt macOS de ⌘V van Mondig stil tegen. Handy kijkt
//! dat na bij de start en na elke update (Handy #1618, via `tauri-plugin-macos-permissions`);
//! wij doen dat rechtstreeks met `AXIsProcessTrusted`. Een ad-hoc ondertekende app verliest het
//! recht bij elke nieuwe build, omdat het aan de handtekening hangt (ADR 0029).
//!
//! Bronnen: developer.apple.com, `AXIsProcessTrusted`, `AXIsProcessTrustedWithOptions` en
//! `kAXTrustedCheckOptionPrompt` (`HIServices/AXUIElement.h`), geraadpleegd 9-11-2026.

#![expect(
    unsafe_code,
    reason = "het recht op toegankelijkheid is alleen via C-functies van ApplicationServices te lezen"
)]

use std::ffi::c_void;

use dispatch2::run_on_main;
use objc2::rc::Retained;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

use crate::Voorgrond;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    /// `Boolean AXIsProcessTrusted(void)`.
    fn AXIsProcessTrusted() -> u8;
    /// `Boolean AXIsProcessTrustedWithOptions(CFDictionaryRef options)`.
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
    /// De sleutel (een `CFStringRef`) waarmee macOS zelf om het recht vraagt.
    static kAXTrustedCheckOptionPrompt: *const c_void;
}

/// Heeft Mondig het recht op toegankelijkheid?
#[must_use]
pub fn toegankelijk() -> bool {
    // SAFETY: een C-functie zonder argumenten die alleen een vlag teruggeeft.
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Vraagt macOS om het recht op toegankelijkheid. Zonder recht toont macOS dan zijn eigen
/// vraag en zet het Mondig in de lijst bij Toegankelijkheid (uit), zodat de gebruiker hem
/// alleen nog aan hoeft te zetten. Geeft terug of het recht er nu al is.
#[must_use]
pub fn vraag_toegankelijkheid() -> bool {
    // SAFETY: `kAXTrustedCheckOptionPrompt` is een constante `CFStringRef` uit het framework,
    // geldig zolang het programma draait. `CFStringRef` en `NSString` zijn uitwisselbaar
    // (toll-free bridged), dus de aanwijzer mag als `&NSString` gelezen worden.
    let sleutel: &NSString = unsafe { &*kAXTrustedCheckOptionPrompt.cast::<NSString>() };
    let ja = NSNumber::numberWithBool(true);
    let opties: Retained<NSDictionary<NSString, NSNumber>> =
        NSDictionary::from_slices(&[sleutel], &[&*ja]);
    // SAFETY: een `NSDictionary` is uitwisselbaar met de `CFDictionaryRef` die de functie
    // vraagt; `opties` leeft tot na de aanroep.
    unsafe { AXIsProcessTrustedWithOptions(Retained::as_ptr(&opties).cast()) != 0 }
}

/// Opent een adres als `x-apple.systempreferences:...` in het programma dat erbij hoort.
/// Geeft `false` als het adres ongeldig is of macOS het niet opende.
#[must_use]
pub fn open_adres(adres: &str) -> bool {
    let Some(url) = NSURL::URLWithString(&NSString::from_str(adres)) else {
        return false;
    };
    NSWorkspace::sharedWorkspace().openURL(&url)
}

/// Welk programma staat vooraan? Op de hoofdthread gelezen: daar werkt `AppKit`
/// `frontmostApplication` bij.
#[must_use]
pub fn voorgrond() -> Voorgrond {
    run_on_main(|_| {
        let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
            return Voorgrond::Geen;
        };
        let eigen = i64::from(std::process::id());
        if i64::from(app.processIdentifier()) == eigen {
            Voorgrond::Eigen
        } else {
            Voorgrond::Ander
        }
    })
}

/// Het procesnummer van het programma vooraan, of `None`. Ook op de hoofdthread gelezen.
#[must_use]
pub fn voorgrond_id() -> Option<i32> {
    run_on_main(|_| {
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|app| app.processIdentifier())
    })
}
