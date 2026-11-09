//! Het recht op toegankelijkheid op de Mac (mijlpaal M1). Zonder dat recht houdt macOS de ⌘V
//! van Mondig stil tegen; dan zet Mondig de tekst op het klembord (`plakken/macos.rs`).
//!
//! De interface vraagt bij de start (en als het venster weer de focus krijgt) de stand op en
//! toont zonder recht een uitleg met een knop. Die knop laat macOS Mondig eerst in de lijst bij
//! Toegankelijkheid zetten (`AXIsProcessTrustedWithOptions` met de vraag aan) en opent dan
//! Systeeminstellingen, Privacy en beveiliging, Toegankelijkheid. Op Windows is er geen recht
//! nodig: de stand is dan "niet nodig" en de interface toont niets.

use serde::Serialize;

/// Hoe het met het recht staat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToegangStand {
    /// Dit systeem kent het recht niet (Windows, Linux).
    NietNodig,
    /// Mondig mag toetsaanslagen sturen.
    Toegestaan,
    /// Mondig mag het (nog) niet: plakken valt terug op het klembord.
    Geweigerd,
}

/// De stand op een systeem dat het recht wel (`mac`) of niet kent.
pub(crate) fn stand_van(mac: bool, toegestaan: bool) -> ToegangStand {
    match (mac, toegestaan) {
        (false, _) => ToegangStand::NietNodig,
        (true, true) => ToegangStand::Toegestaan,
        (true, false) => ToegangStand::Geweigerd,
    }
}

/// De stand van het recht nu.
#[tauri::command]
pub(crate) fn toegankelijkheid_stand() -> ToegangStand {
    #[cfg(target_os = "macos")]
    {
        stand_van(true, mondig_mac::rechten::toegankelijk())
    }
    #[cfg(not(target_os = "macos"))]
    {
        stand_van(false, false)
    }
}

/// De knop "Toegankelijkheid openen".
#[tauri::command]
pub(crate) fn toegankelijkheid_open() {
    #[cfg(target_os = "macos")]
    if !mondig_mac::rechten::vraag_toegankelijkheid() {
        let _ = mondig_mac::rechten::open_adres(mondig_mac::ADRES_TOEGANKELIJKHEID);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alleen_de_mac_kent_het_recht() {
        assert_eq!(stand_van(false, false), ToegangStand::NietNodig);
        assert_eq!(stand_van(false, true), ToegangStand::NietNodig);
        assert_eq!(stand_van(true, true), ToegangStand::Toegestaan);
        assert_eq!(stand_van(true, false), ToegangStand::Geweigerd);
    }
}
