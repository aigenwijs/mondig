//! De snelheidstest: hoe snel schrijft een motor uit op de processor en op de videokaart?
//!
//! Voor de knop "Snelheid testen" bij "Videokaart gebruiken" (2-11-2026), en straks voor de
//! hardwarecheck van fase 5, die het zonder knop beslist (ADR 0011). [`meet`] geeft per
//! motor (dus per apparaat) een realtime-factor; [`vergelijk`] maakt daar een oordeel van.
//!
//! Het testgeluid is kunstmatig ([`testgeluid`]): Mondig levert geen opname van een mens mee,
//! en die komt ook niet in git. De tekst die eruit komt doet er niet toe; alleen de tijd telt.
//! Omdat het model op ruis weinig tekst maakt, valt de winst van de videokaart hier hoger uit
//! dan bij echte spraak (het uitrekenen van tekst gaat op beide apparaten ongeveer even snel);
//! het oordeel "loont" of "loont niet" klopt wel.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::aanbieder::{Aanbieder, AanbiederFout, Invoer, LokaleEngine, Rekenstand};
use crate::audio::DOEL_SAMPLERATE;

/// Hoe lang het testgeluid duurt. Lang genoeg dat de rekentijd boven de ruis van het starten
/// uitkomt, kort genoeg dat de test op de processor binnen een halve minuut klaar is.
pub const TESTDUUR: Duration = Duration::from_secs(30);

/// Het stuk waarmee de motor eerst opwarmt, zodat de eerste berekening (shaders compileren,
/// geheugen klaarzetten) niet in de meting valt.
const OPWARMEN: Duration = Duration::from_secs(2);

/// Vanaf hoeveel keer sneller de videokaart "loont". Daaronder is het verschil te klein om
/// een extra onderdeel (de videodriver) in te zetten.
pub const LOONT_VANAF: f64 = 1.5;

/// Kunstmatig testgeluid, 16 kHz mono: ruis in vlagen van een kwart seconde (zoals
/// lettergrepen), met een korte pauze om de 2,5 seconde. Altijd hetzelfde, zodat twee metingen
/// precies hetzelfde geluid krijgen.
///
/// Bewust zonder toonhoogte: een zoemende "stem" liet Canary herhalingen verzinnen tot de
/// motor stopte met "output repetition" (gemeten 2-11-2026). Op ruis schrijft het model weinig
/// of niets uit, maar het rekent het hele stuk door; dat is wat de test meet.
#[must_use]
pub fn testgeluid(duur: Duration) -> Vec<f32> {
    let rate = f64::from(DOEL_SAMPLERATE);
    let mut ruis: u32 = 0x1234_5678;
    (0..sample_aantal(duur))
        .map(|n| {
            let t = n_als_f64(n) / rate;
            let lettergreep = (std::f64::consts::PI * 4.0 * t).sin().abs();
            let pauze = if (t % 2.5) > 2.2 { 0.0 } else { 1.0 };
            ruis = ruis.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let sis = f64::from(ruis >> 8) / f64::from(1_u32 << 24) - 0.5;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "een sample tussen -1 en 1 past ruim in een f32"
            )]
            let sample = (0.1 * sis * lettergreep * pauze) as f32;
            sample
        })
        .collect()
}

fn sample_aantal(duur: Duration) -> usize {
    let ms = usize::try_from(duur.as_millis()).unwrap_or(usize::MAX);
    ms.saturating_mul(DOEL_SAMPLERATE as usize) / 1000
}

#[expect(
    clippy::cast_precision_loss,
    reason = "een samplenummer van een testgeluid blijft ver onder 2^52"
)]
fn n_als_f64(n: usize) -> f64 {
    n as f64
}

/// Eén meting op één motor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meting {
    /// Waarop de motor werkelijk rekende. Een [`Rekenstand::Terugval`] betekent: de
    /// videokaart gaf een fout en de meting ging over de processor.
    pub stand: Rekenstand,
    /// Hoe lang het laden van het model duurde (apart gehouden, telt niet mee).
    pub laadtijd: Duration,
    /// Hoe lang het uitschrijven van het testgeluid duurde, na het opwarmen.
    pub rekentijd: Duration,
    /// Hoe lang het testgeluid duurt.
    pub audio: Duration,
}

impl Meting {
    /// Rekentijd gedeeld door de duur van het geluid: 0,1 is tien keer sneller dan realtime.
    #[must_use]
    pub fn realtime_factor(&self) -> f64 {
        let audio = self.audio.as_secs_f64();
        if audio <= 0.0 {
            return f64::INFINITY;
        }
        self.rekentijd.as_secs_f64() / audio
    }
}

/// Meet hoe snel `engine` het geluid uitschrijft: eerst laden (apart getimed), dan opwarmen
/// met de eerste twee seconden, dan het hele geluid. De motor blijft daarna geladen; stop hem
/// zelf. Het apparaat staat in de motor (`LokaleEngine::nieuw`).
///
/// # Errors
///
/// Geeft de [`AanbiederFout`] van het laden of uitschrijven.
pub fn meet(engine: &mut LokaleEngine, samples: &[f32]) -> Result<Meting, AanbiederFout> {
    let begin = Instant::now();
    engine.laad()?;
    let laadtijd = begin.elapsed();
    // Viel de motor terug op de processor, dan is meten op de videokaart zinloos: stop met
    // de uitslag "videokaart faalde" in plaats van nog ruim een halve minuut te rekenen.
    if is_terugval(engine.rekenstand()) {
        return Ok(Meting {
            stand: Rekenstand::Terugval,
            laadtijd,
            rekentijd: Duration::ZERO,
            audio: crate::audio::duur_van_samples(samples.len()),
        });
    }
    let warm = samples.len().min(sample_aantal(OPWARMEN));
    engine.transcribeer(Invoer::Geluid(&samples[..warm]))?;
    let begin = Instant::now();
    engine.transcribeer(Invoer::Geluid(samples))?;
    let rekentijd = begin.elapsed();
    Ok(Meting {
        stand: engine.rekenstand().unwrap_or(Rekenstand::Processor),
        laadtijd,
        rekentijd,
        audio: crate::audio::duur_van_samples(samples.len()),
    })
}

/// Viel de motor bij het laden terug op de processor?
fn is_terugval(stand: Option<Rekenstand>) -> bool {
    stand == Some(Rekenstand::Terugval)
}

/// Wat de test zegt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Oordeel {
    /// De videokaart is minstens [`LOONT_VANAF`] keer zo snel: aanzetten loont.
    Loont,
    /// Even snel of langzamer: laat het uit.
    LoontNiet,
    /// De videokaart gaf een fout (of de motor viel terug op de processor).
    VideokaartFaalde,
}

impl Oordeel {
    /// Een vaste code zonder tekst van de gebruiker, voor het logboek.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Loont => "loont",
            Self::LoontNiet => "loont_niet",
            Self::VideokaartFaalde => "videokaart_faalde",
        }
    }
}

/// De uitkomst van de vergelijking.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vergelijking {
    /// Loont het, loont het niet, of faalde de videokaart?
    pub oordeel: Oordeel,
    /// Hoeveel keer sneller de videokaart is; `None` als hij faalde.
    pub keer_sneller: Option<f64>,
    /// Realtime-factor op de processor.
    pub processor: f64,
    /// Realtime-factor op de videokaart; `None` als hij faalde.
    pub videokaart: Option<f64>,
}

impl Vergelijking {
    /// De winst in tienden (95 is 9,5 keer sneller), afgerond, voor het logboek.
    #[must_use]
    pub fn keer_sneller_tienden(&self) -> Option<u64> {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "een winst is positief en veel kleiner dan 2^52 tienden"
        )]
        self.keer_sneller
            .filter(|k| k.is_finite() && *k >= 0.0)
            .map(|k| (k * 10.0).round() as u64)
    }
}

/// Het oordeel bij de realtime-factor van de processor en die van de videokaart (`None` als
/// de videokaart faalde of terugviel).
#[must_use]
pub fn vergelijk(processor: f64, videokaart: Option<f64>) -> Vergelijking {
    let Some(videokaart) = videokaart.filter(|v| v.is_finite()) else {
        return Vergelijking {
            oordeel: Oordeel::VideokaartFaalde,
            keer_sneller: None,
            processor,
            videokaart: None,
        };
    };
    // Een meting van nul seconden kan niet, maar deel er nooit door.
    let keer = processor / videokaart.max(1e-6);
    Vergelijking {
        oordeel: if keer >= LOONT_VANAF {
            Oordeel::Loont
        } else {
            Oordeel::LoontNiet
        },
        keer_sneller: Some(keer),
        processor,
        videokaart: Some(videokaart),
    }
}

/// De realtime-factor van een meting op de videokaart, of `None` als die meting niet op de
/// videokaart liep (een terugval telt als falen).
#[must_use]
pub fn op_videokaart(meting: &Meting) -> Option<f64> {
    (meting.stand == Rekenstand::Videokaart).then(|| meting.realtime_factor())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negen_keer_sneller_loont() {
        let v = vergelijk(0.09, Some(0.01));
        assert_eq!(v.oordeel, Oordeel::Loont);
        assert!((v.keer_sneller.unwrap() - 9.0).abs() < 1e-9);
    }

    #[test]
    fn de_winst_in_tienden_voor_het_logboek() {
        assert_eq!(vergelijk(0.95, Some(0.1)).keer_sneller_tienden(), Some(95));
        assert_eq!(vergelijk(0.1, None).keer_sneller_tienden(), None);
        assert_eq!(Oordeel::Loont.code(), "loont");
        assert_eq!(Oordeel::VideokaartFaalde.code(), "videokaart_faalde");
    }

    #[test]
    fn de_grens_is_anderhalf_keer() {
        assert_eq!(vergelijk(0.75, Some(0.5)).oordeel, Oordeel::Loont);
        assert_eq!(vergelijk(0.74, Some(0.5)).oordeel, Oordeel::LoontNiet);
    }

    #[test]
    fn even_snel_of_trager_loont_niet() {
        assert_eq!(vergelijk(0.1, Some(0.1)).oordeel, Oordeel::LoontNiet);
        assert_eq!(vergelijk(0.1, Some(0.3)).oordeel, Oordeel::LoontNiet);
    }

    #[test]
    fn zonder_meting_op_de_videokaart_faalde_hij() {
        let v = vergelijk(0.1, None);
        assert_eq!(v.oordeel, Oordeel::VideokaartFaalde);
        assert_eq!(v.keer_sneller, None);
        assert_eq!(
            vergelijk(0.1, Some(f64::NAN)).oordeel,
            Oordeel::VideokaartFaalde
        );
    }

    #[test]
    fn een_terugval_telt_niet_als_meting_op_de_videokaart() {
        let meting = |stand| Meting {
            stand,
            laadtijd: Duration::from_secs(1),
            rekentijd: Duration::from_secs(3),
            audio: Duration::from_secs(30),
        };
        assert!(op_videokaart(&meting(Rekenstand::Terugval)).is_none());
        assert!(op_videokaart(&meting(Rekenstand::Processor)).is_none());
        let rtf = op_videokaart(&meting(Rekenstand::Videokaart)).unwrap();
        assert!((rtf - 0.1).abs() < 1e-9);
    }

    #[test]
    fn een_terugval_bij_het_laden_stopt_de_meting() {
        assert!(is_terugval(Some(Rekenstand::Terugval)));
        assert!(!is_terugval(Some(Rekenstand::Videokaart)));
        assert!(!is_terugval(Some(Rekenstand::Processor)));
        assert!(!is_terugval(None));
    }

    #[test]
    fn het_testgeluid_is_vast_hoorbaar_en_binnen_bereik() {
        let geluid = testgeluid(Duration::from_secs(3));
        assert_eq!(geluid.len(), 48_000);
        assert_eq!(geluid, testgeluid(Duration::from_secs(3)));
        assert!(geluid.iter().all(|s| s.abs() <= 1.0));
        let rms = (geluid.iter().map(|s| s * s).sum::<f32>() / 48_000.0).sqrt();
        assert!(rms > 0.01, "het geluid moet hoorbaar zijn: {rms}");
    }
}
