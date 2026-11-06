//! De slaapstand tegenhouden met `SetThreadExecutionState`. De stand geldt voor de thread die
//! hem zet, tot dezelfde thread hem met alleen `ES_CONTINUOUS` weer wist (of eindigt); daarom
//! roept alleen de eigen thread van `wakker.rs` dit aan.
//!
//! Bron: learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setthreadexecutionstate,
//! geraadpleegd 2-11-2026. `ES_CONTINUOUS | ES_SYSTEM_REQUIRED` houdt de computer wakker zonder
//! het scherm aan te houden (dat is `ES_DISPLAY_REQUIRED`, dat we bewust niet zetten).

#![expect(
    unsafe_code,
    reason = "de slaapstand tegenhouden kan alleen via de Win32-API"
)]

use std::io;

use mondig_core::wakker::Slot;
use windows::Win32::System::Power::{ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState};

/// Het slot van Windows.
pub(super) struct Windows;

impl Slot for Windows {
    fn pak(&mut self) -> io::Result<()> {
        // SAFETY: de aanroep krijgt alleen vlaggen en geeft de vorige stand terug; er worden
        // geen aanwijzers of geheugen gedeeld.
        let vorige = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
        // 0 betekent: mislukt.
        if vorige.0 == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn laat_los(&mut self) {
        // SAFETY: zie `pak`. Alleen ES_CONTINUOUS wist wat deze thread eerder vroeg.
        let _ = unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
    }
}
