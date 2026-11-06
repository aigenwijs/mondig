//! Het totale werkgeheugen via `GlobalMemoryStatusEx`
//! (learn.microsoft.com/windows/win32/api/sysinfoapi/nf-sysinfoapi-globalmemorystatusex,
//! `ullTotalPhys`, geraadpleegd 2-11-2026).

#![expect(
    unsafe_code,
    reason = "het werkgeheugen lezen kan alleen via de Win32-API"
)]

use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

/// Het fysieke werkgeheugen in bytes, of `None` als Windows het niet geeft.
pub(super) fn totaal_geheugen() -> Option<u64> {
    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).ok()?,
        ..Default::default()
    };
    // SAFETY: `status` is een geldige, uitgelijnde MEMORYSTATUSEX met `dwLength` ingevuld,
    // zoals de aanroep vraagt; de aanroep schrijft alleen daarin.
    unsafe { GlobalMemoryStatusEx(&raw mut status) }.ok()?;
    Some(status.ullTotalPhys)
}
