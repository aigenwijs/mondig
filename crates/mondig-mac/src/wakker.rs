//! De Mac wakker houden zolang Mondig werk heeft, met een activiteit van `NSProcessInfo`.
//!
//! `NSActivityUserInitiated` houdt de computer wakker (het bevat
//! `NSActivityIdleSystemSleepDisabled`) en voorkomt App Nap, maar laat het scherm wel uitgaan;
//! dat is hetzelfde als `ES_SYSTEM_REQUIRED` op Windows. Bron: developer.apple.com,
//! "beginActivity(options:reason:)" en "ProcessInfo.ActivityOptions" (geraadpleegd 9-11-2026).

#![expect(
    unsafe_code,
    reason = "een activiteit beëindigen is in objc2 als unsafe gemarkeerd"
)]

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};

/// Een lopende activiteit, of geen. Hoort bij de thread die hem maakt.
#[derive(Default)]
pub struct Activiteit {
    teken: Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

impl Activiteit {
    /// Nog geen activiteit.
    #[must_use]
    pub fn nieuw() -> Self {
        Self::default()
    }

    /// Houdt de computer wakker (doet niets als dat al zo is).
    pub fn pak(&mut self) {
        if self.teken.is_none() {
            let reden = NSString::from_str("Mondig neemt op of schrijft uit");
            self.teken = Some(
                NSProcessInfo::processInfo()
                    .beginActivityWithOptions_reason(NSActivityOptions::UserInitiated, &reden),
            );
        }
    }

    /// Laat de computer weer slapen als hij wil.
    pub fn laat_los(&mut self) {
        if let Some(teken) = self.teken.take() {
            // SAFETY: het teken komt van `beginActivityWithOptions:reason:` van hetzelfde
            // proces en wordt hier één keer beëindigd (`take`).
            unsafe { NSProcessInfo::processInfo().endActivity(&teken) };
        }
    }
}

impl Drop for Activiteit {
    fn drop(&mut self) {
        self.laat_los();
    }
}
