//! Toetsen op de Mac: de stand van de modifiers, beveiligde invoer, ⌘V als toetsaanslag en
//! tekst typen als terugval.
//!
//! ⌘V gaat via `CGEventPost` naar de event tap van het HID-systeem, zoals `enigo` dat voor
//! Handy doet. macOS laat dat alleen toe als Mondig het recht op toegankelijkheid heeft; zonder
//! dat recht verdwijnt de aanslag stil, daarom kijkt de app dat vooraf na
//! ([`crate::rechten::toegankelijk`]).
//!
//! Typen ([`typ_tekst`]) gaat langs dezelfde weg, maar met de tekens zelf in de gebeurtenis
//! (`CGEventKeyboardSetUnicodeString`) in plaats van een toetscode per letter. Zo maakt de
//! toetsenbordindeling (AZERTY, QWERTZ, Dvorak) niet uit.
//!
//! Bronnen: developer.apple.com, `CGEventPost`, `CGEventSourceFlagsState`,
//! `CGEventKeyboardSetUnicodeString` en `IsSecureEventInputEnabled` (Carbon,
//! `HIToolbox/CarbonEvents.h`), geraadpleegd 9-11-2026.

#![expect(
    unsafe_code,
    reason = "beveiligde invoer en de toetsenbordindeling zijn alleen via C-functies van Carbon te lezen, en tekens gaan via een aanwijzer in een toetsgebeurtenis"
)]

use std::ffi::{c_ulong, c_void};
use std::time::{Duration, Instant};

use dispatch2::run_on_main;
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation, CGKeyCode,
};

use crate::{TYPBLOK, modifier_in, toets_voor_v};

/// De toetscode van ⌘ links (`kVK_Command`, `HIToolbox/Events.h`).
const TOETS_COMMAND: CGKeyCode = 0x37;
/// De toetscode van V op een ANSI-toetsenbord (`kVK_ANSI_V`): de terugval als de huidige
/// indeling niet te lezen is (zie [`toets_v`]).
const TOETS_ANSI_V: CGKeyCode = 0x09;
/// `kUCKeyActionDisplay`: welk teken een toets toont.
const UC_TOON: u16 = 3;
/// `kUCKeyTranslateNoDeadKeysBit` als masker.
const UC_GEEN_DODE_TOETSEN: u32 = 1;
/// ⌘ zoals `UCKeyTranslate` het wil: de Carbon-modifiers acht bits naar rechts (`cmdKey` is
/// bit 8). Met ⌘ erbij, omdat indelingen als "Dvorak - QWERTY ⌘" sneltoetsen anders leggen.
const UC_COMMAND: u32 = 1;
/// Zo lang blijft ⌘ ingedrukt na V. Handy kwam op 100 ms omdat sommige systemen een te snel
/// losgelaten combinatie misten (Handy #165, `CHORD_HOLD_MS`); dezelfde waarde als op Windows.
const COMMAND_VASTHOUDEN: Duration = Duration::from_millis(100);

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    /// `Boolean IsSecureEventInputEnabled(void)`; `Boolean` is een `unsigned char`.
    fn IsSecureEventInputEnabled() -> u8;
    /// `TISInputSourceRef TISCopyCurrentKeyboardLayoutInputSource(void)` (Create Rule).
    fn TISCopyCurrentKeyboardLayoutInputSource() -> *const c_void;
    /// `void *TISGetInputSourceProperty(TISInputSourceRef, CFStringRef)` (Get Rule).
    fn TISGetInputSourceProperty(bron: *const c_void, sleutel: *const c_void) -> *const c_void;
    /// De sleutel voor de indeling als `CFDataRef` met een `UCKeyboardLayout`.
    static kTISPropertyUnicodeKeyLayoutData: *const c_void;
    /// `OSStatus UCKeyTranslate(...)`.
    fn UCKeyTranslate(
        indeling: *const u8,
        toets: u16,
        actie: u16,
        modifiers: u32,
        toetsenbordsoort: u32,
        opties: u32,
        dode_toets: *mut u32,
        max_lengte: usize,
        lengte: *mut usize,
        tekens: *mut u16,
    ) -> i32;
    /// `UInt8 LMGetKbdType(void)`.
    fn LMGetKbdType() -> u8;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
    fn CFRelease(waarde: *const c_void);
}

/// Laat de bron los als hij uit beeld gaat.
struct Bron(*const c_void);

impl Drop for Bron {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: de bron kwam van een Copy-functie (Create Rule); dit is de enige release.
            unsafe { CFRelease(self.0) };
        }
    }
}

/// De toetscode die in de huidige indeling met ⌘ een `v` geeft, of `None` als de indeling niet
/// te lezen is. Werkwijze van Handy (`src-tauri/src/input.rs`, MIT). Alleen op de hoofdthread:
/// de TIS-functies vragen dat.
fn toets_v_hier() -> Option<CGKeyCode> {
    // SAFETY: op de hoofdthread aangeroepen; de bron valt onder de Create Rule en `Bron` geeft
    // hem vrij.
    let bron = Bron(unsafe { TISCopyCurrentKeyboardLayoutInputSource() });
    if bron.0.is_null() {
        return None;
    }
    // SAFETY: de bron leeft tot het einde van deze functie; de sleutel is een constante van
    // het framework. Het resultaat is van de bron (Get Rule), dus niet vrijgeven.
    let data = unsafe { TISGetInputSourceProperty(bron.0, kTISPropertyUnicodeKeyLayoutData) };
    if data.is_null() {
        return None;
    }
    // SAFETY: `data` is een geldige CFData zolang de bron leeft.
    let indeling = unsafe { CFDataGetBytePtr(data) };
    if indeling.is_null() {
        return None;
    }
    // SAFETY: geen argumenten.
    let soort = u32::from(unsafe { LMGetKbdType() });
    toets_voor_v(|toets| {
        let mut dode_toets = 0u32;
        let mut tekens = [0u16; 4];
        let mut lengte = 0usize;
        // SAFETY: `indeling` wijst naar een UCKeyboardLayout zolang de bron leeft; alle
        // uitvoer wijst naar eigen, geïnitialiseerde variabelen van de opgegeven maat.
        let status = unsafe {
            UCKeyTranslate(
                indeling,
                toets,
                UC_TOON,
                UC_COMMAND,
                soort,
                UC_GEEN_DODE_TOETSEN,
                &raw mut dode_toets,
                tekens.len(),
                &raw mut lengte,
                tekens.as_mut_ptr(),
            )
        };
        (status == 0 && lengte == 1)
            .then(|| char::from_u32(u32::from(tekens[0])))
            .flatten()
    })
}

/// De toetscode voor V in de huidige indeling, met `kVK_ANSI_V` als terugval.
fn toets_v() -> CGKeyCode {
    run_on_main(|_| toets_v_hier()).unwrap_or(TOETS_ANSI_V)
}

/// Houdt de gebruiker nu ⌘, ⌥, ⌃ of ⇧ ingedrukt? Over alle toetsenborden van deze sessie.
#[must_use]
pub fn modifier_ingedrukt() -> bool {
    let vlaggen = CGEventSource::flags_state(CGEventSourceStateID::CombinedSessionState);
    modifier_in(vlaggen.bits())
}

/// Staat beveiligde invoer aan? Dat doet macOS bij een wachtwoordveld, en Terminal bij
/// "Beveiligde toetsenbordinvoer". Een gestuurde toetsaanslag komt dan niet aan.
#[must_use]
pub fn beveiligde_invoer() -> bool {
    // SAFETY: een C-functie zonder argumenten die alleen een vlag teruggeeft.
    unsafe { IsSecureEventInputEnabled() != 0 }
}

/// Stuurt ⌘V naar het programma vooraan: ⌘ neer, V neer en los, en na
/// [`COMMAND_VASTHOUDEN`] ⌘ los. Blokkeert die tijd; nooit op de hoofdthread aanroepen, want
/// daar beantwoordt `AppKit` intussen het leesverzoek. Alle vier de aanslagen worden eerst
/// gemaakt: lukt er een niet, dan gaat er niets weg (anders bleef ⌘ hangen). Geeft het moment
/// vlak voor V neer: lezingen van daarvoor komen niet van ⌘V.
#[must_use]
pub fn stuur_cmd_v() -> Option<Instant> {
    let v = toets_v();
    let bron = CGEventSource::new(CGEventSourceStateID::HIDSystemState);
    let stappen = [
        (TOETS_COMMAND, true, CGEventFlags::MaskCommand),
        (v, true, CGEventFlags::MaskCommand),
        (v, false, CGEventFlags::MaskCommand),
        (TOETS_COMMAND, false, CGEventFlags::empty()),
    ];
    let mut aanslagen = Vec::with_capacity(stappen.len());
    for (toets, neer, vlaggen) in stappen {
        let aanslag = CGEvent::new_keyboard_event(bron.as_deref(), toets, neer)?;
        // De vlaggen expliciet: anders telt een ingedrukte toets van de gebruiker mee.
        CGEvent::set_flags(Some(&aanslag), vlaggen);
        aanslagen.push(aanslag);
    }
    let mut verstuurd = None;
    for (index, aanslag) in aanslagen.iter().enumerate() {
        if index == 1 {
            verstuurd = Some(Instant::now());
        }
        if index == aanslagen.len() - 1 {
            std::thread::sleep(COMMAND_VASTHOUDEN);
        }
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(aanslag));
    }
    verstuurd
}

/// Een toets neer en een toets los, zonder modifiers (anders telt een ingedrukte toets van
/// de gebruiker mee). Beide gebeurtenissen dragen `tekens`; een echte
/// toetsaanslag heeft ze ook bij het loslaten. `None` als een gebeurtenis niet te maken was.
fn neer_en_los(toets: CGKeyCode, tekens: &[u16]) -> Option<[CFRetained<CGEvent>; 2]> {
    let lengte = c_ulong::try_from(tekens.len()).ok()?;
    let bron = CGEventSource::new(CGEventSourceStateID::HIDSystemState);
    let maak = |neer| {
        let aanslag = CGEvent::new_keyboard_event(bron.as_deref(), toets, neer)?;
        CGEvent::set_flags(Some(&aanslag), CGEventFlags::empty());
        if !tekens.is_empty() {
            // SAFETY: de aanwijzer wijst naar `lengte` geldige UTF-16-eenheden in `tekens`, dat
            // langer leeft dan deze aanroep; de functie kopieert ze.
            unsafe {
                CGEvent::keyboard_set_unicode_string(Some(&aanslag), lengte, tekens.as_ptr());
            }
        }
        Some(aanslag)
    };
    Some([maak(true)?, maak(false)?])
}

/// Stuurt de gebeurtenissen naar de event tap van het HID-systeem, zoals ⌘V.
fn stuur(aanslagen: &[CFRetained<CGEvent>]) {
    for aanslag in aanslagen {
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(aanslag));
    }
}

/// Typt tekst als Unicode-tekens in het programma vooraan, hooguit [`TYPBLOK`] UTF-16-eenheden
/// (de aanroeper deelt op, zonder een surrogaatpaar te splitsen). De toetscode is 0; de
/// programma's lezen de tekens uit de gebeurtenis, niet uit de indeling. `false` als de tekst te
/// lang is of een gebeurtenis niet te maken was; er gaat dan niets weg.
#[must_use]
pub fn typ_tekst(tekst: &str) -> bool {
    let tekens: Vec<u16> = tekst.encode_utf16().collect();
    if tekens.is_empty() {
        return true;
    }
    if tekens.len() > TYPBLOK {
        return false;
    }
    let Some(aanslagen) = neer_en_los(0, &tekens) else {
        return false;
    };
    stuur(&aanslagen);
    true
}
