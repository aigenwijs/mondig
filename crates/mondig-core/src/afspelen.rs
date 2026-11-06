//! Een stuk geluid om te beluisteren (stroom V, V-1): de interface vraagt om het stuk bij een
//! regel van het verslag en krijgt een kleine WAV terug, 16 kHz mono 16 bit. Zo hoeft de
//! webview geen bestandsrecht te hebben en staat er nooit een heel bestand in het geheugen.
//!
//! De tijden zijn die van het verslag: bij een bestand de positie in het bestand, bij een
//! vergadering de tijdlijn van de opname (met de ankers van de stukken en de vertraging van de
//! microfoon, zoals [`crate::spoor::mengen`] ze toepast).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::audio::{AudioFout, DOEL_SAMPLERATE, Lezer};
use crate::spoor::mengen::{MengFout, meng_deel};

/// Zoveel geluid geven we hooguit in één keer: tien minuten (ongeveer 19 MB). Een langere
/// beurt wordt afgekapt; de interface speelt dan het eerste deel.
pub const MAX_FRAGMENT_MS: u64 = 600_000;

/// De WAV-bytes (kop en samples) van 16-bit mono op 16 kHz.
#[must_use]
pub fn wav_van_samples(samples: &[i16]) -> Vec<u8> {
    let data = u32::try_from(samples.len().saturating_mul(2)).unwrap_or(u32::MAX - 36);
    let mut bytes = crate::spoor::stukken::kop(data).to_vec();
    bytes.reserve(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// Het eind van een stuk, begrensd op [`MAX_FRAGMENT_MS`] na het begin.
fn begrensd_eind(begin_ms: u64, eind_ms: u64) -> u64 {
    eind_ms.min(begin_ms.saturating_add(MAX_FRAGMENT_MS))
}

/// Zet een sample van ongeveer `-1.0..=1.0` om naar 16 bit, afgekapt waar het erboven uitkomt.
#[expect(
    clippy::cast_possible_truncation,
    reason = "na begrenzen op -1..=1 en afronden past de waarde altijd in een i16"
)]
fn naar_i16(sample: f32) -> i16 {
    // De lezer deelt 16 bit door 32768, dus dat doen we hier terug.
    (sample * 32768.0).round().clamp(-32768.0, 32767.0) as i16
}

/// Zoveel milliseconden begint het lezen voor het stuk na een sprong, om de decoder op gang te
/// laten komen (bij AAC en Opus zit er aan het begin een opstartstuk).
const AANLOOP_MS: u64 = 2_000;

/// Het stuk van `begin_ms` tot `eind_ms` uit een audio- of videobestand, als WAV. Het bestand
/// springt eerst naar vlak voor het stuk (zonder alles ervoor te decoderen); kan het formaat dat
/// niet, dan wordt het vanaf het begin gelezen. Er blijft maar het stuk zelf in het geheugen. Een
/// stuk voorbij het einde van het bestand wordt afgekapt.
///
/// Zodra `annuleer` waar wordt (de gebruiker koos intussen een andere regel), stopt het lezen en
/// komt er `None`.
///
/// # Errors
///
/// Geeft een [`AudioFout`] in gewone taal als het bestand weg of onleesbaar is.
pub fn fragment_uit_bestand(
    pad: &Path,
    begin_ms: u64,
    eind_ms: u64,
    annuleer: &AtomicBool,
) -> Result<Option<Vec<u8>>, AudioFout> {
    let per_ms = u64::from(DOEL_SAMPLERATE / 1000);
    let eind_ms = begrensd_eind(begin_ms, eind_ms);
    let van = usize::try_from(begin_ms.saturating_mul(per_ms)).unwrap_or(usize::MAX);
    let tot = usize::try_from(eind_ms.saturating_mul(per_ms)).unwrap_or(usize::MAX);
    let mut lezer = Lezer::open(pad)?;
    let mut positie = 0usize;
    if begin_ms > AANLOOP_MS {
        match lezer.spring_naar(begin_ms - AANLOOP_MS) {
            Some(plek) => positie = usize::try_from(plek).unwrap_or(0),
            // Springen lukte niet: opnieuw beginnen, want de lezer kan half verschoven zijn.
            None => lezer = Lezer::open(pad)?,
        }
    }
    let mut uit: Vec<i16> = Vec::new();
    while positie < tot {
        if annuleer.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let Some(blok) = lezer.lees_blok()? else {
            break;
        };
        let blok_eind = positie.saturating_add(blok.len());
        if blok_eind > van {
            let a = van.saturating_sub(positie);
            let b = (tot - positie).min(blok.len());
            if a < b {
                uit.extend(blok[a..b].iter().map(|s| naar_i16(*s)));
            }
        }
        positie = blok_eind;
    }
    Ok(Some(wav_van_samples(&uit)))
}

/// Het stuk van `begin_ms` tot `eind_ms` van de tijdlijn van een afgesloten vergaderopname,
/// beide sporen gemengd, als WAV.
///
/// # Errors
///
/// Zie [`meng_deel`]: de opname is niet afgesloten of niet te lezen.
pub fn fragment_uit_opname(
    opnamemap: &Path,
    begin_ms: u64,
    eind_ms: u64,
) -> Result<Vec<u8>, MengFout> {
    let mut bytes = Vec::new();
    meng_deel(
        opnamemap,
        begin_ms,
        begrensd_eind(begin_ms, eind_ms),
        &mut bytes,
    )?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Schrijft een wav van `seconden` seconden waarvan elke milliseconde (16 samples) de
    /// waarde van die milliseconde heeft, zodat te zien is welk stuk eruit komt.
    fn schrijf_wav(map: &Path, seconden: u32) -> std::path::PathBuf {
        let pad = map.join("trap.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut schrijver = hound::WavWriter::create(&pad, spec).unwrap();
        for n in 0..(16_000 * seconden) {
            schrijver
                .write_sample(i16::try_from(n / 16).unwrap())
                .unwrap();
        }
        schrijver.finalize().unwrap();
        pad
    }

    fn geen() -> AtomicBool {
        AtomicBool::new(false)
    }

    fn samples(wav: &[u8]) -> Vec<i16> {
        assert_eq!(&wav[0..4], b"RIFF");
        let data = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
        assert_eq!(data, wav.len() - 44);
        wav[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| i16::from_le_bytes(*p))
            .collect()
    }

    #[test]
    fn een_stuk_uit_een_bestand_begint_en_eindigt_op_de_juiste_milliseconde() {
        let map = tempfile::tempdir().unwrap();
        let pad = schrijf_wav(map.path(), 3);
        let wav = fragment_uit_bestand(&pad, 1_000, 1_500, &AtomicBool::new(false))
            .unwrap()
            .unwrap();
        let s = samples(&wav);
        assert_eq!(s.len(), 8_000, "een halve seconde");
        assert_eq!(s[0], 1_000, "de eerste milliseconde van het stuk");
        assert_eq!(s[s.len() - 1], 1_499);
    }

    #[test]
    fn een_stuk_voorbij_het_einde_van_het_bestand_wordt_afgekapt() {
        let map = tempfile::tempdir().unwrap();
        let pad = schrijf_wav(map.path(), 2);
        let s = samples(
            &fragment_uit_bestand(&pad, 1_500, 9_000, &geen())
                .unwrap()
                .unwrap(),
        );
        assert_eq!(s.len(), 8_000);
        let leeg = samples(
            &fragment_uit_bestand(&pad, 5_000, 6_000, &geen())
                .unwrap()
                .unwrap(),
        );
        assert!(leeg.is_empty());
    }

    #[test]
    fn een_stuk_diep_in_een_bestand_komt_na_een_sprong_op_de_juiste_milliseconde() {
        let map = tempfile::tempdir().unwrap();
        let pad = schrijf_wav(map.path(), 30);
        // Een lezer die kan springen komt dichtbij zonder alles ervoor te lezen.
        let mut lezer = Lezer::open(&pad).unwrap();
        let plek = lezer.spring_naar(20_000).expect("wav kan springen");
        assert!(
            plek <= 20_000 * 16 && plek + 16_000 >= 20_000 * 16,
            "{plek}"
        );
        let s = samples(
            &fragment_uit_bestand(&pad, 22_000, 22_400, &geen())
                .unwrap()
                .unwrap(),
        );
        assert_eq!(s.len(), 6_400);
        assert_eq!(s[0], 22_000);
        assert_eq!(s[s.len() - 1], 22_399);
    }

    #[test]
    fn een_geannuleerd_stuk_geeft_niets_terug() {
        let map = tempfile::tempdir().unwrap();
        let pad = schrijf_wav(map.path(), 3);
        let vlag = AtomicBool::new(true);
        assert!(
            fragment_uit_bestand(&pad, 0, 2_000, &vlag)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn een_ontbrekend_bestand_geeft_een_fout_in_gewone_taal() {
        let map = tempfile::tempdir().unwrap();
        let fout =
            fragment_uit_bestand(&map.path().join("weg.wav"), 0, 1_000, &geen()).unwrap_err();
        assert!(matches!(fout, AudioFout::NietGevonden(_)));
    }

    #[test]
    fn een_stuk_is_hooguit_tien_minuten() {
        assert_eq!(begrensd_eind(1_000, 5_000), 5_000);
        assert_eq!(begrensd_eind(1_000, 9_999_999), 601_000);
    }

    #[test]
    fn de_wav_kop_klopt_met_de_samples() {
        let wav = wav_van_samples(&[1, -2, 3]);
        assert_eq!(wav.len(), 44 + 6);
        assert_eq!(samples(&wav), vec![1, -2, 3]);
        assert_eq!(
            u32::from_le_bytes(wav[24..28].try_into().unwrap()),
            DOEL_SAMPLERATE
        );
    }
}
