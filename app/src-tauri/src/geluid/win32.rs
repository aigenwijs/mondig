//! Afspelen via `PlaySoundW` met `SND_MEMORY | SND_ASYNC`: de WAV staat in het geheugen
//! en de aanroep wacht niet tot hij klaar is. Het geheugen moet blijven bestaan zolang de
//! toon klinkt; daarom alleen `'static` (learn.microsoft.com/previous-versions/dd743680,
//! "`PlaySound` function", geraadpleegd 1-11-2026). `SND_NODEFAULT`: lukt het niet, dan geen
//! systeemgeluid in de plaats.

#![expect(unsafe_code, reason = "geluid afspelen kan alleen via de Win32-API")]

use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
use windows::core::PCWSTR;

/// Speelt de WAV af zonder te wachten. Een nieuwe toon onderbreekt een vorige.
pub(super) fn speel_af(wav: &'static [u8]) {
    // SAFETY: met SND_MEMORY wijst de "naam" naar de WAV in het geheugen; die blijft
    // bestaan zolang het programma draait.
    unsafe {
        let _ = PlaySoundW(
            PCWSTR(wav.as_ptr().cast()),
            None,
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}
