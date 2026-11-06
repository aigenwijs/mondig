//! De privacy-schakelaars lezen uit het register en een instellingenpagina openen.

#![expect(
    unsafe_code,
    reason = "register lezen en een adres openen kan alleen via de Win32-API"
)]

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

use super::Schakelaars;

const CONSENT: &str =
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

fn lees_tekst(sleutel: HKEY, pad: &str) -> Option<String> {
    let pad = HSTRING::from(pad);
    let mut buffer = [0u16; 32];
    let mut grootte = u32::try_from(size_of_val(&buffer)).unwrap_or(0);
    // SAFETY: buffer en grootte horen bij elkaar; RegGetValueW schrijft er hooguit zoveel in.
    let uitslag = unsafe {
        RegGetValueW(
            sleutel,
            &pad,
            w!("Value"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&raw mut grootte),
        )
    };
    if uitslag != ERROR_SUCCESS {
        return None;
    }
    let tekens = (grootte as usize / 2).min(buffer.len());
    let tekst = String::from_utf16_lossy(&buffer[..tekens]);
    Some(tekst.trim_end_matches('\0').to_string())
}

/// De stand van de schakelaars van nu.
pub(super) fn lees_schakelaars() -> Schakelaars {
    Schakelaars {
        apparaat: lees_tekst(HKEY_LOCAL_MACHINE, CONSENT),
        apps: lees_tekst(HKEY_CURRENT_USER, CONSENT),
        bureaublad_apps: lees_tekst(HKEY_CURRENT_USER, &format!(r"{CONSENT}\NonPackaged")),
    }
}

/// Opent een vast adres (een pagina van de Windows-instellingen).
pub(super) fn open_adres(adres: &str) {
    let adres = HSTRING::from(adres);
    // SAFETY: alle teksten zijn geldig en eindigen op nul; geen venster als eigenaar.
    unsafe {
        let _ = ShellExecuteW(
            None,
            w!("open"),
            &adres,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}
