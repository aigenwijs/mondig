//! Het element met de focus, via de toegankelijkheid van de Mac (`AXUIElement`). Mondig typt
//! alleen als dat een bewerkbaar tekstveld is ([`crate::is_tekstveld`]); het recht op
//! toegankelijkheid heeft Mondig voor ⌘V al.
//!
//! Elk antwoord komt van het programma zelf. Hangt dat, dan wacht een vraag hooguit
//! [`TIJDSLIMIET`] (`AXUIElementSetMessagingTimeout`) en zegt Mondig "weet niet": dan typt het
//! niet.
//!
//! Bronnen: developer.apple.com, `AXUIElementCreateSystemWide`, `AXUIElementCopyAttributeValue`,
//! `AXUIElementSetMessagingTimeout`, `kAXFocusedUIElementAttribute`, `kAXRoleAttribute` en
//! `kAXSelectedTextRangeAttribute` (`HIServices/AXUIElement.h`, `AXAttributeConstants.h`),
//! geraadpleegd 9-11-2026.

#![expect(
    unsafe_code,
    reason = "de toegankelijkheid van de Mac is alleen via C-functies van ApplicationServices te lezen"
)]

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2_foundation::NSString;

use crate::Focusveld;

/// Zo lang wacht een vraag aan een ander programma hooguit, in seconden.
const TIJDSLIMIET: f32 = 0.3;
/// `kAXErrorSuccess`.
const AX_GELUKT: i32 = 0;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    /// `AXUIElementRef AXUIElementCreateSystemWide(void)` (Create Rule).
    fn AXUIElementCreateSystemWide() -> *const c_void;
    /// `AXError AXUIElementCopyAttributeValue(AXUIElementRef, CFStringRef, CFTypeRef *)`
    /// (Copy Rule voor de waarde).
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribuut: *const c_void,
        waarde: *mut *const c_void,
    ) -> i32;
    /// `AXError AXUIElementSetMessagingTimeout(AXUIElementRef, float)`.
    fn AXUIElementSetMessagingTimeout(element: *const c_void, seconden: f32) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(waarde: *const c_void);
    fn CFGetTypeID(waarde: *const c_void) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(waarde: *const c_void) -> u8;
}

/// Een CF-waarde die we zelf vrijgeven (Create of Copy Rule).
struct Eigen(*const c_void);

impl Drop for Eigen {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: de waarde kwam van een Create- of Copy-functie; dit is de enige release.
            unsafe { CFRelease(self.0) };
        }
    }
}

/// Een attribuut van een element, of `None` als het er niet is of de vraag mislukte.
fn attribuut(element: &Eigen, naam: &str) -> Option<Eigen> {
    let naam: Retained<NSString> = NSString::from_str(naam);
    let mut waarde: *const c_void = std::ptr::null();
    // SAFETY: `element` is een geldig AXUIElement; een `NSString` is uitwisselbaar met de
    // `CFStringRef` die gevraagd wordt en leeft tot na de aanroep; `waarde` is een eigen
    // variabele.
    let fout = unsafe {
        AXUIElementCopyAttributeValue(element.0, Retained::as_ptr(&naam).cast(), &raw mut waarde)
    };
    let waarde = Eigen(waarde);
    (fout == AX_GELUKT && !waarde.0.is_null()).then_some(waarde)
}

/// De waarde als tekst, als het een `CFString` is.
fn als_tekst(waarde: &Eigen) -> Option<String> {
    // SAFETY: `waarde` is een geldige CF-waarde.
    let is_tekst = unsafe { CFGetTypeID(waarde.0) == CFStringGetTypeID() };
    // SAFETY: een `CFString` is uitwisselbaar met `NSString`; de waarde leeft zolang `waarde`.
    is_tekst.then(|| unsafe { &*waarde.0.cast::<NSString>() }.to_string())
}

/// De waarde als ja of nee, als het een `CFBoolean` is.
fn als_vlag(waarde: &Eigen) -> Option<bool> {
    // SAFETY: `waarde` is een geldige CF-waarde; alleen een `CFBoolean` wordt gelezen.
    unsafe {
        (CFGetTypeID(waarde.0) == CFBooleanGetTypeID()).then(|| CFBooleanGetValue(waarde.0) != 0)
    }
}

/// Wat de toegankelijkheid over het element met de focus zegt, of `None` als er geen is of
/// het programma niet op tijd antwoordde. Mag op elke thread.
#[must_use]
pub fn focusveld() -> Option<Focusveld> {
    // SAFETY: geen argumenten; het resultaat valt onder de Create Rule en `Eigen` geeft het vrij.
    let systeem = Eigen(unsafe { AXUIElementCreateSystemWide() });
    if systeem.0.is_null() {
        return None;
    }
    // SAFETY: een geldig element.
    unsafe { AXUIElementSetMessagingTimeout(systeem.0, TIJDSLIMIET) };
    let focus = attribuut(&systeem, "AXFocusedUIElement")?;
    // SAFETY: het focus-element is een geldig AXUIElement (de waarde van dit attribuut).
    unsafe { AXUIElementSetMessagingTimeout(focus.0, TIJDSLIMIET) };
    let rol = attribuut(&focus, "AXRole").and_then(|w| als_tekst(&w));
    // Zonder rol antwoordde het programma niet (op tijd): twijfel.
    rol.as_ref()?;
    Some(Focusveld {
        rol,
        bewerkbaar: attribuut(&focus, "AXEditable").and_then(|w| als_vlag(&w)),
        selectie: attribuut(&focus, "AXSelectedTextRange").is_some(),
    })
}
