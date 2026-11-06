//! Maat en plek van het hoofdvenster bij de start. Het venster is standaard 900 bij 780
//! (tauri.conf.json) en staat altijd in het midden van het werkgebied van de monitor (het scherm
//! zonder taakbalk). Past de breedte of hoogte niet, dan wordt het venster zo groot als past,
//! nooit kleiner dan `minWidth` en `minHeight` uit tauri.conf.json.

use tauri::{AppHandle, PhysicalPosition, PhysicalSize, WebviewWindow};

/// Ruimte die rond het venster vrij blijft, in logische pixels (beide kanten samen).
const MARGE: f64 = 32.0;

/// Een breedte en hoogte in logische pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Maat {
    pub b: f64,
    pub h: f64,
}

/// Waar het venster komt, in logische pixels: `x` en `y` van de buitenrand, gemeten vanaf de
/// linkerbovenhoek van het werkgebied, en `b` en `h` van de binnenkant (zonder titelbalk).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rechthoek {
    pub x: f64,
    pub y: f64,
    pub b: f64,
    pub h: f64,
}

/// Eén kant: de gewenste maat als die past, anders zo groot als past, nooit onder het minimum.
fn passend(gewenst: f64, minimum: f64, rand: f64, werkgebied: f64) -> f64 {
    let beschikbaar = werkgebied - rand - MARGE;
    if gewenst <= beschikbaar {
        gewenst
    } else {
        beschikbaar.max(minimum)
    }
}

/// Plaatst het venster in het werkgebied: zo groot als gewenst als dat past, anders kleiner
/// (niet onder het minimum), en altijd in het midden. `rand` is de titelbalk en de randen
/// (buitenmaat min binnenmaat).
pub(crate) fn plaats(gewenst: Maat, minimum: Maat, rand: Maat, werkgebied: Maat) -> Rechthoek {
    let b = passend(gewenst.b, minimum.b, rand.b, werkgebied.b);
    let h = passend(gewenst.h, minimum.h, rand.h, werkgebied.h);
    Rechthoek {
        x: ((werkgebied.b - b - rand.b) / 2.0).max(0.0),
        y: ((werkgebied.h - h - rand.h) / 2.0).max(0.0),
        b,
        h,
    }
}

/// De maten van het hoofdvenster uit tauri.conf.json: gewenst en minimum.
fn uit_config(app: &AppHandle) -> Option<(Maat, Maat)> {
    let config = app.config();
    let venster = config.app.windows.iter().find(|v| v.label == "main")?;
    let gewenst = Maat {
        b: venster.width,
        h: venster.height,
    };
    let minimum = Maat {
        b: venster.min_width.unwrap_or(0.0),
        h: venster.min_height.unwrap_or(0.0),
    };
    Some((gewenst, minimum))
}

/// Zet het venster in het midden van het werkgebied, zo groot als past. Mislukt iets (geen
/// monitor bekend), dan blijft het venster zoals het is.
pub(crate) fn pas_in_werkgebied(app: &AppHandle, venster: &WebviewWindow) {
    let Some((gewenst, minimum)) = uit_config(app) else {
        return;
    };
    let Ok(Some(monitor)) = venster.current_monitor() else {
        return;
    };
    let (Ok(binnen), Ok(buiten)) = (venster.inner_size(), venster.outer_size()) else {
        return;
    };
    let schaal = monitor.scale_factor();
    let werkgebied = monitor.work_area();
    let logisch = |fysiek: u32| f64::from(fysiek) / schaal;
    let rand = Maat {
        b: logisch(buiten.width.saturating_sub(binnen.width)),
        h: logisch(buiten.height.saturating_sub(binnen.height)),
    };
    let plek = plaats(
        gewenst,
        minimum,
        rand,
        Maat {
            b: logisch(werkgebied.size.width),
            h: logisch(werkgebied.size.height),
        },
    );
    let _ = venster.set_size(PhysicalSize::new(
        naar_fysiek(plek.b, schaal),
        naar_fysiek(plek.h, schaal),
    ));
    let x = werkgebied.position.x + i32::try_from(naar_fysiek(plek.x, schaal)).unwrap_or(0);
    let y = werkgebied.position.y + i32::try_from(naar_fysiek(plek.y, schaal)).unwrap_or(0);
    let _ = venster.set_position(PhysicalPosition::new(x, y));
}

/// Logische pixels naar fysieke, afgerond; nooit negatief.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "een vensterhoogte is positief en ver onder u32::MAX"
)]
fn naar_fysiek(logisch: f64, schaal: f64) -> u32 {
    (logisch * schaal).round().max(0.0) as u32
}

#[cfg(test)]
mod tests {
    use super::{Maat, Rechthoek, naar_fysiek, plaats};

    const GEWENST: Maat = Maat { b: 900.0, h: 780.0 };
    const MINIMUM: Maat = Maat { b: 480.0, h: 400.0 };
    /// Titelbalk en randen van Windows 11, in logische pixels.
    const RAND: Maat = Maat { b: 16.0, h: 39.0 };

    /// Het werkgebied in logische pixels: het scherm gedeeld door de schaal, min de taakbalk
    /// (48 logische pixels).
    fn werkgebied(b: f64, h: f64, schaal: f64) -> Maat {
        Maat {
            b: b / schaal,
            h: h / schaal - 48.0,
        }
    }

    fn ongeveer(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn full_hd_op_100_procent_houdt_de_maat_en_staat_in_het_midden() {
        let plek = plaats(GEWENST, MINIMUM, RAND, werkgebied(1920.0, 1080.0, 1.0));
        assert_eq!(
            plek,
            Rechthoek {
                x: 502.0,
                y: 106.5,
                b: 900.0,
                h: 780.0
            }
        );
    }

    #[test]
    fn full_hd_op_150_procent_wordt_lager_en_blijft_900_breed() {
        // 1280 bij 672 logisch: hoogte 672 - 39 - 32 = 601.
        let plek = plaats(GEWENST, MINIMUM, RAND, werkgebied(1920.0, 1080.0, 1.5));
        assert_eq!(
            plek,
            Rechthoek {
                x: 182.0,
                y: 16.0,
                b: 900.0,
                h: 601.0
            }
        );
    }

    #[test]
    fn klein_scherm_op_150_procent_wordt_smaller_en_lager() {
        // 1280x800 op 150 procent: 853,3 bij 485,3 logisch.
        let plek = plaats(GEWENST, MINIMUM, RAND, werkgebied(1280.0, 800.0, 1.5));
        assert!(ongeveer(plek.b, 1280.0 / 1.5 - 16.0 - 32.0));
        assert!(ongeveer(plek.h, 800.0 / 1.5 - 48.0 - 39.0 - 32.0));
        assert!(ongeveer(plek.x, 16.0));
        assert!(ongeveer(plek.y, 16.0));
    }

    #[test]
    fn scherm_1366x768_wordt_lager_maar_niet_smaller() {
        // 1366 bij 720 logisch: hoogte 720 - 39 - 32 = 649.
        let plek = plaats(GEWENST, MINIMUM, RAND, werkgebied(1366.0, 768.0, 1.0));
        assert_eq!(
            plek,
            Rechthoek {
                x: 225.0,
                y: 16.0,
                b: 900.0,
                h: 649.0
            }
        );
    }

    #[test]
    fn nooit_kleiner_dan_het_minimum_en_nooit_buiten_beeld() {
        let plek = plaats(GEWENST, MINIMUM, RAND, Maat { b: 400.0, h: 300.0 });
        assert_eq!((plek.b, plek.h), (480.0, 400.0));
        assert_eq!((plek.x, plek.y), (0.0, 0.0));
    }

    #[test]
    fn naar_fysiek_rondt_af() {
        assert_eq!(naar_fysiek(601.0, 1.5), 902);
        assert_eq!(naar_fysiek(-3.0, 1.0), 0);
    }
}
