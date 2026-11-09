//! De pil op de Mac: tonen zonder dat Mondig actief wordt.
//!
//! Neemt Mondig de focus, dan plakt ⌘V in Mondig in plaats van in het programma waar je typt.
//! Daarom:
//!
//! 1. De app maakt het venster met `focusable(false)`. Tao (de vensterlaag onder Tauri) laat
//!    `canBecomeKeyWindow` en `canBecomeMainWindow` dan `NO` geven (`TaoWindow`,
//!    `platform_impl/macos/window.rs` in tao 0.37), dus het venster wordt nooit key of main.
//! 2. Tonen met `orderFrontRegardless` in plaats van `show()` van Tauri: dat doet
//!    `makeKeyAndOrderFront`. `orderFrontRegardless` zet het venster vooraan, ook als Mondig
//!    niet actief is, zonder Mondig te activeren.
//! 3. Vensterniveau "status" en `collectionBehavior` met alle Spaces en
//!    `FullScreenAuxiliary`, zodat de pil ook naast een programma op volledig scherm staat.
//!
//! Handy maakt van zijn pil een `NSPanel` met `nonactivatingPanel` (via de git-afhankelijkheid
//! `tauri-nspanel`, `src-tauri/src/overlay.rs`). Dat kan hier niet zonder ADR, en het venster van
//! Tao heeft eigen ivars, dus de klasse omzetten naar een `NSPanel` is niet veilig. Of een gewoon
//! venster ook boven een volledig-schermprogramma verschijnt, test de tester (zie het rapport
//! bij M1).
//!
//! Een klik op een gewoon venster activeert de app toch, ook als het venster nooit key wordt.
//! Daarom onthoudt [`toon`] het programma dat vooraan stond, en zet [`herstel_voorgrond`] dat
//! terug, zoals `geef_focus_terug` op Windows.
//!
//! De plek: net als op Windows het scherm met het venster dat de focus heeft
//! (`NSScreen.mainScreen`, het scherm met het key-venster), en anders het scherm onder de muis;
//! in het midden onderaan het werkgebied (`visibleFrame`, zonder menubalk en Dock); zie
//! [`crate::kies_scherm`] en [`crate::pil_oorsprong`].

#![expect(
    unsafe_code,
    reason = "het venster komt als ruwe NSWindow-aanwijzer van Tauri"
)]

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use dispatch2::run_on_main;
use objc2::MainThreadOnly;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSEvent, NSRunningApplication, NSScreen,
    NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior, NSWorkspace,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::{Kader, kies_scherm, pil_oorsprong};

/// Het programma dat vooraan stond toen de pil verscheen (proces-id, 0: geen).
static VORIGE: AtomicI32 = AtomicI32::new(0);
/// Het adres van het venster van de pil (0: nog niet ingericht).
static PIL: AtomicUsize = AtomicUsize::new(0);

/// Het proces-id van Mondig.
fn eigen_pid() -> i32 {
    i32::try_from(std::process::id()).unwrap_or(-1)
}

/// Het programma vooraan, als dat niet Mondig is. Op de hoofdthread.
fn ander_vooraan() -> Option<Retained<NSRunningApplication>> {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .filter(|app| app.processIdentifier() != eigen_pid())
}

/// Staat Mondig vooraan door een klik op de pil, dan het programma van daarvoor weer vooraan.
/// Met `altijd` (de pil meldt zelf een klik) zonder verdere controle; anders alleen als geen
/// ander venster van Mondig de focus heeft (dan werkt de gebruiker in Mondig zelf). Geeft
/// `true` als macOS het programma weer vooraan zette.
#[must_use]
pub fn herstel_voorgrond(altijd: bool) -> bool {
    run_on_main(|mtm| {
        let pid = VORIGE.load(Ordering::SeqCst);
        if pid == 0 || ander_vooraan().is_some() {
            return false;
        }
        if !altijd {
            let pil = PIL.load(Ordering::SeqCst);
            let sleutel = NSApplication::sharedApplication(mtm).keyWindow();
            if sleutel.is_some_and(|venster| Retained::as_ptr(&venster) as usize != pil) {
                return false;
            }
        }
        NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
            .is_some_and(|app| app.activateWithOptions(NSApplicationActivationOptions::empty()))
    })
}

fn kader(rechthoek: NSRect) -> Kader {
    Kader {
        x: rechthoek.origin.x,
        y: rechthoek.origin.y,
        breedte: rechthoek.size.width,
        hoogte: rechthoek.size.height,
    }
}

/// Doet `werk` op de hoofdthread met het venster.
///
/// # Safety
///
/// `venster` moet een geldige `NSWindow` zijn die blijft bestaan tot `werk` klaar is.
unsafe fn met_venster(venster: NonNull<c_void>, werk: impl FnOnce(&NSWindow) + Send) {
    // Een aanwijzer mag niet tussen threads; het adres wel. Het venster zelf gebruiken we
    // alleen op de hoofdthread.
    let adres = venster.as_ptr() as usize;
    run_on_main(move |_| {
        // SAFETY: zie de voorwaarde van deze functie; we zijn nu op de hoofdthread, waar een
        // NSWindow gebruikt mag worden.
        let venster = unsafe { &*(adres as *const NSWindow) };
        werk(venster);
    });
}

/// Richt het venster van de pil in: boven andere vensters, op alle Spaces en naast een
/// programma op volledig scherm, en niet weg als Mondig niet actief is.
///
/// # Safety
///
/// `venster` moet de `NSWindow` van de pil zijn (`WebviewWindow::ns_window`), die zo lang
/// bestaat als de app.
pub unsafe fn richt_in(venster: NonNull<c_void>) {
    PIL.store(venster.as_ptr() as usize, Ordering::SeqCst);
    // SAFETY: doorgegeven aan de aanroeper.
    unsafe {
        met_venster(venster, |venster| {
            venster.setLevel(NSStatusWindowLevel);
            venster.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::IgnoresCycle,
            );
            venster.setHidesOnDeactivate(false);
        });
    }
}

/// Toont de pil met deze maat (in punten, gelijk aan CSS-pixels) op het scherm onder de muis,
/// onderaan of (`boven`) bovenaan, zonder Mondig te activeren.
///
/// # Safety
///
/// Zie [`richt_in`].
pub unsafe fn toon(venster: NonNull<c_void>, maat: (f64, f64), boven: bool, marge: f64) {
    // SAFETY: doorgegeven aan de aanroeper.
    unsafe {
        met_venster(venster, move |venster| {
            let mtm = venster.mtm();
            // Het programma waar je typt, om na een klik op de pil terug te zetten.
            if let Some(app) = ander_vooraan() {
                VORIGE.store(app.processIdentifier(), Ordering::SeqCst);
            }
            let schermen = NSScreen::screens(mtm);
            let kaders: Vec<Kader> = schermen
                .iter()
                .map(|scherm| kader(scherm.frame()))
                .collect();
            // Het scherm met het key-venster: opgezocht aan zijn kader.
            let voorkeur = NSScreen::mainScreen(mtm)
                .map(|scherm| kader(scherm.frame()))
                .and_then(|hoofd| kaders.iter().position(|k| *k == hoofd));
            let muis = NSEvent::mouseLocation();
            let Some(scherm) = kies_scherm(voorkeur, (muis.x, muis.y), &kaders)
                .and_then(|index| schermen.iter().nth(index))
            else {
                return;
            };
            let (x, y) = pil_oorsprong(kader(scherm.visibleFrame()), maat, boven, marge);
            venster.setFrame_display(
                NSRect::new(NSPoint::new(x, y), NSSize::new(maat.0, maat.1)),
                true,
            );
            venster.orderFrontRegardless();
        });
    }
}

/// Verbergt de pil.
///
/// # Safety
///
/// Zie [`richt_in`].
pub unsafe fn verberg(venster: NonNull<c_void>) {
    // SAFETY: doorgegeven aan de aanroeper.
    unsafe {
        met_venster(venster, |venster| venster.orderOut(None));
    }
}
