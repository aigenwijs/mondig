//! Een kort, zacht geluidje bij het starten en stoppen van een dictaat (sprint 3d). Eén set,
//! zonder geluidsbestand van derden: twee sinustonen van 80 ms met een zachte in- en
//! uitloop, hier in code gemaakt als WAV in het geheugen.
//!
//! Volgorde rond de microfoon: het startgeluid gaat af voordat de microfoon opent, en het
//! stopgeluid pas als de microfoon dicht is. Afspelen loopt asynchroon (`SND_ASYNC`), dus
//! het wacht nergens op en vertraagt de opname niet. Het startgeluid kan heel even
//! overlappen met de eerste milliseconden van de opname als de microfoon sneller opent dan
//! 80 ms; dat is een zachte toon zonder spraak, en de stilte aan het begin knipt Silero weg.
//! Wachten tot de toon klaar is, zou het begin van je zin kosten (ADR 0012: meteen reageren).
//!
//! Het volume zit in de samples zelf, zodat het mengpaneel van Windows het niet hoeft te
//! kennen. Een volume van 0 tot 100 loopt kwadratisch op (zo klinkt de schuif gelijkmatig)
//! tot hooguit 35 procent van de volle uitsturing: zacht genoeg om niet te schrikken.
//!
//! Daarnaast het testgeluid (fase UX-C, commando `testgeluid_speel`): drie korte tonen van
//! samen 0,9 seconde op het gekozen afspeelapparaat, niet op de standaarduitgang. Het speelt
//! via cpal in de opnamelaag (`SysteemApparatuur::speel_af`), met dezelfde toongenerator
//! als de geluidjes hierboven. Het volgt de geluidsinstelling niet: wie de geluidjes uit
//! heeft staan, wil het juiste apparaat nog steeds kunnen horen.

use std::collections::HashMap;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use mondig_core::opname::systeem::SysteemApparatuur;
use mondig_core::taak::{Foutmelding, Vervolg};
use tauri::AppHandle;

use crate::sneltoets::{lees_instelling, schrijf_instelling};

#[cfg(windows)]
mod win32;

/// Bemonsteringsfrequentie van de tonen.
pub(crate) const SAMPLERATE: u32 = 44_100;
/// Lengte van een toon.
pub(crate) const DUUR_MS: u32 = 80;
/// Lengte van de in- en uitloop.
pub(crate) const LOOP_MS: u32 = 15;
/// Grootste uitsturing bij volume 100.
pub(crate) const MAX_UITSTURING: f32 = 0.35;
/// Standaardvolume.
pub(crate) const STANDAARD_VOLUME: u8 = 50;

/// Sleutel: staat er "uit", dan klinkt er niets.
const SLEUTEL_GELUID: &str = "geluid";
/// Sleutel: het volume, 0 tot 100.
const SLEUTEL_VOLUME: &str = "geluid_volume";

/// Welke toon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Toon {
    /// Hoger: de opname loopt.
    Start,
    /// Lager: de opname is klaar.
    Stop,
}

impl Toon {
    fn frequentie(self) -> f32 {
        match self {
            Self::Start => 880.0,
            Self::Stop => 587.0,
        }
    }
}

/// De uitsturing (0 tot [`MAX_UITSTURING`]) bij een volume van 0 tot 100.
pub(crate) fn uitsturing(volume: u8) -> f32 {
    let deel = f32::from(volume.min(100)) / 100.0;
    MAX_UITSTURING * deel * deel
}

/// Een sinustoon van `duur_ms` met een in- en uitloop van `loop_ms`, van `-hoogte` tot
/// `hoogte`, op `samplerate`.
#[expect(clippy::cast_precision_loss, reason = "kleine aantallen samples")]
pub(crate) fn sinustoon(
    frequentie: f32,
    samplerate: u32,
    duur_ms: u32,
    loop_ms: u32,
    hoogte: f32,
) -> impl Iterator<Item = f32> {
    let per_ms =
        |ms: u32| usize::try_from(u64::from(samplerate) * u64::from(ms) / 1000).unwrap_or(0);
    let aantal = per_ms(duur_ms);
    let lus = per_ms(loop_ms);
    (0..aantal).map(move |i| {
        let t = i as f32 / samplerate as f32;
        // Een halve cosinus aan begin en eind: geen klik.
        let rand = i.min(aantal - 1 - i);
        let omhulling = if rand < lus {
            0.5 - 0.5 * (PI * rand as f32 / lus as f32).cos()
        } else {
            1.0
        };
        hoogte * omhulling * (2.0 * PI * frequentie * t).sin()
    })
}

/// De samples van een toon, 16-bit mono.
#[expect(
    clippy::cast_possible_truncation,
    reason = "de waarde is begrensd tot de 16-bit-schaal"
)]
pub(crate) fn samples(toon: Toon, volume: u8) -> Vec<i16> {
    let hoogte = uitsturing(volume) * f32::from(i16::MAX);
    sinustoon(toon.frequentie(), SAMPLERATE, DUUR_MS, LOOP_MS, hoogte)
        .map(|s| s as i16)
        .collect()
}

/// De tonen van het testgeluid: een oplopend drieklankje (C, E, G), elk 300 ms.
const TESTGELUID_TONEN: [f32; 3] = [523.25, 659.25, 783.99];
/// Lengte van elke toon van het testgeluid.
const TESTGELUID_TOON_MS: u32 = 300;
/// In- en uitloop van elke toon van het testgeluid.
const TESTGELUID_LOOP_MS: u32 = 30;

/// Het testgeluid (mono, -1 tot 1) op de samplerate van het apparaat: zacht, op de
/// uitsturing van het standaardvolume van de geluidjes (ongeveer -21 dBFS). Op de meter
/// van het systeemgeluid is dat ongeveer zes van de tien streepjes.
pub(crate) fn testgeluid(samplerate: u32) -> Vec<f32> {
    let hoogte = uitsturing(STANDAARD_VOLUME);
    TESTGELUID_TONEN
        .iter()
        .flat_map(|&f| {
            sinustoon(
                f,
                samplerate,
                TESTGELUID_TOON_MS,
                TESTGELUID_LOOP_MS,
                hoogte,
            )
        })
        .collect()
}

/// Speelt er nu een testgeluid? Een tweede klik speelt dan niet nog eens over het eerste heen.
static TESTGELUID_SPEELT: AtomicBool = AtomicBool::new(false);

/// Speelt het testgeluid op het gekozen afspeelapparaat (`None`: de standaard van het
/// systeem; een id uit `vergadering_afspeelapparaten`). Antwoordt als het geluid klaar is,
/// na ongeveer een seconde. Speelt er al een, dan antwoordt het meteen zonder opnieuw te
/// spelen.
#[tauri::command]
pub(crate) async fn testgeluid_speel(apparaat: Option<String>) -> Result<(), Foutmelding> {
    if TESTGELUID_SPEELT.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let uitslag = tauri::async_runtime::spawn_blocking(move || {
        SysteemApparatuur
            .speel_af(apparaat.as_deref(), testgeluid)
            .map_err(Foutmelding::from)
    })
    .await;
    TESTGELUID_SPEELT.store(false, Ordering::SeqCst);
    uitslag.map_err(|_| {
        Foutmelding::nieuw(
            "Het testgeluid stopte onverwacht.",
            Vervolg::OpnieuwProberen,
        )
    })?
}

/// Een WAV-bestand (PCM, 16-bit mono) met deze samples.
pub(crate) fn wav(samples: &[i16]) -> Vec<u8> {
    let data_lengte = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX);
    let mut uit = Vec::with_capacity(44 + samples.len() * 2);
    uit.extend_from_slice(b"RIFF");
    uit.extend_from_slice(&(36 + data_lengte).to_le_bytes());
    uit.extend_from_slice(b"WAVEfmt ");
    uit.extend_from_slice(&16u32.to_le_bytes());
    uit.extend_from_slice(&1u16.to_le_bytes()); // PCM
    uit.extend_from_slice(&1u16.to_le_bytes()); // mono
    uit.extend_from_slice(&SAMPLERATE.to_le_bytes());
    uit.extend_from_slice(&(SAMPLERATE * 2).to_le_bytes()); // bytes per seconde
    uit.extend_from_slice(&2u16.to_le_bytes()); // bytes per frame
    uit.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    uit.extend_from_slice(b"data");
    uit.extend_from_slice(&data_lengte.to_le_bytes());
    for sample in samples {
        uit.extend_from_slice(&sample.to_le_bytes());
    }
    uit
}

/// De WAV van een toon bij een volume, één keer gemaakt en daarna bewaard. Asynchroon
/// afspelen vraagt geheugen dat blijft bestaan tot de toon klaar is; daarom leeft elke
/// versie zo lang als het programma (hooguit 2 x 101 stuks van 7 KB).
fn wav_van(toon: Toon, volume: u8) -> &'static [u8] {
    type Gemaakt = Mutex<HashMap<(Toon, u8), &'static [u8]>>;
    static GEMAAKT: OnceLock<Gemaakt> = OnceLock::new();
    let mut gemaakt = GEMAAKT
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    gemaakt
        .entry((toon, volume.min(100)))
        .or_insert_with(|| Box::leak(wav(&samples(toon, volume)).into_boxed_slice()))
}

/// Het bewaarde volume, of `None` als het geluid uit staat.
pub(crate) fn lees(app: &AppHandle) -> Option<u8> {
    if lees_instelling(app, SLEUTEL_GELUID).as_deref() == Some("uit") {
        return None;
    }
    Some(lees_volume(app))
}

fn lees_volume(app: &AppHandle) -> u8 {
    lees_instelling(app, SLEUTEL_VOLUME)
        .and_then(|v| v.parse::<u8>().ok())
        .map_or(STANDAARD_VOLUME, |v| v.min(100))
}

/// Speelt een toon af volgens de instellingen, zonder te wachten.
pub(crate) fn speel(app: &AppHandle, toon: Toon) {
    let Some(volume) = lees(app) else {
        return;
    };
    if volume == 0 {
        return;
    }
    let wav = wav_van(toon, volume);
    #[cfg(windows)]
    win32::speel_af(wav);
    // Op de Mac met `NSSound`, ook asynchroon (crate `mondig-mac`).
    #[cfg(target_os = "macos")]
    mondig_mac::geluid::speel(wav);
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = wav;
}

/// De instelling voor het scherm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct GeluidInstelling {
    aan: bool,
    volume: u8,
}

/// De bewaarde instelling.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn geluid_instelling(app: AppHandle) -> GeluidInstelling {
    GeluidInstelling {
        aan: lees(&app).is_some(),
        volume: lees_volume(&app),
    }
}

/// Bewaart de instelling. Met `proef` klinkt meteen het startgeluid, zodat je hoort hoe
/// hard het is.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de argumenten per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn geluid_zet(app: AppHandle, instelling: GeluidInstelling, proef: bool) {
    schrijf_instelling(
        &app,
        SLEUTEL_GELUID,
        if instelling.aan { "aan" } else { "uit" },
    );
    schrijf_instelling(
        &app,
        SLEUTEL_VOLUME,
        &instelling.volume.min(100).to_string(),
    );
    if proef {
        speel(&app, Toon::Start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_loopt_kwadratisch_tot_de_grens() {
        assert!(uitsturing(0).abs() < f32::EPSILON);
        assert!((uitsturing(50) - MAX_UITSTURING / 4.0).abs() < 1e-6);
        assert!((uitsturing(100) - MAX_UITSTURING).abs() < 1e-6);
        // Boven 100 telt als 100.
        assert!((uitsturing(255) - MAX_UITSTURING).abs() < 1e-6);
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "een positieve waarde onder i16::MAX"
    )]
    fn toon_is_80_ms_en_begint_en_eindigt_stil() {
        let s = samples(Toon::Start, 100);
        assert_eq!(s.len(), 3528);
        assert_eq!(s[0], 0);
        assert!(s[s.len() - 1].unsigned_abs() < 200);
        let piek = s.iter().map(|x| x.unsigned_abs()).max().unwrap();
        let grens = (MAX_UITSTURING * f32::from(i16::MAX)) as u16;
        assert!(piek <= grens && piek > grens - 200, "piek {piek}");
    }

    #[test]
    fn volume_schaalt_de_samples() {
        let hard = samples(Toon::Stop, 100);
        let zacht = samples(Toon::Stop, 50);
        let piek = |s: &[i16]| s.iter().map(|x| x.unsigned_abs()).max().unwrap();
        let verhouding = f32::from(piek(&zacht)) / f32::from(piek(&hard));
        assert!((verhouding - 0.25).abs() < 0.01, "{verhouding}");
        assert!(samples(Toon::Stop, 0).iter().all(|x| *x == 0));
    }

    #[test]
    #[expect(clippy::cast_precision_loss, reason = "kleine aantallen samples")]
    fn testgeluid_duurt_0_9_seconde_is_zacht_en_begint_en_eindigt_stil() {
        for samplerate in [44_100, 48_000] {
            let s = testgeluid(samplerate);
            let duur = s.len() as f32 / samplerate as f32;
            assert!((duur - 0.9).abs() < 0.001, "{samplerate}: {duur}");
            assert!(s[0].abs() < 1e-6 && s[s.len() - 1].abs() < 0.01);
            let piek = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            let grens = uitsturing(STANDAARD_VOLUME);
            assert!(piek <= grens && piek > grens * 0.95, "piek {piek}");
        }
    }

    #[test]
    fn wav_kop_klopt() {
        let w = wav(&[0, 1, -1]);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()), 36 + 6);
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(
            u32::from_le_bytes(w[24..28].try_into().unwrap()),
            SAMPLERATE
        );
        assert_eq!(&w[36..40], b"data");
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 6);
        assert_eq!(w.len(), 44 + 6);
    }

    #[test]
    fn dezelfde_toon_wordt_hergebruikt() {
        let a = wav_van(Toon::Start, 40);
        let b = wav_van(Toon::Start, 40);
        assert_eq!(a.as_ptr(), b.as_ptr());
        assert_ne!(wav_van(Toon::Stop, 40).as_ptr(), a.as_ptr());
    }
}
