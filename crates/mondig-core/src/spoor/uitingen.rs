//! Een spoor in uitingen verdelen (sprint 4b-B).
//!
//! Parakeet geeft één segment per run en Canary geeft geen tijden, dus de tijdcodes van het
//! model zijn niet te gebruiken om twee sporen in elkaar te vlechten. Daarom schrijft het
//! werkproces een spoor uit per uiting: Silero zoekt de spraakgebieden, en elk gebied gaat
//! apart naar het model. Begin en eind van het segment komen dan uit Silero.
//!
//! Silero draait in vensters van [`VENSTER`], zodat een spoor van twee uur nooit in zijn
//! geheel in het geheugen staat. Deze module bevat de rekenregel per venster, los van het
//! model, zodat ze zonder werkproces te testen is: welke gebieden klaar zijn, en vanaf waar
//! het volgende venster begint. Een gebied dat tot aan de rand van het venster loopt, is
//! misschien nog niet af en gaat mee naar het volgende venster. Een gebied van meer dan
//! [`UITING_MAX`] wordt op het stilste moment geknipt, met dezelfde regel als bij bestanden
//! ([`audio::deel_op_stilte`]).

use std::ops::Range;
use std::time::Duration;

use crate::audio::{self, DOEL_SAMPLERATE};

/// Zoveel geluid krijgt Silero per keer.
pub const VENSTER: Duration = Duration::from_mins(1);

/// Een gebied dat binnen dit stuk van de rand van een venster ophoudt, kan nog doorlopen:
/// Silero sluit een gebied pas na een stilte van [`STILTE_MS`], plus de marge.
pub const RAND: Duration = Duration::from_secs(1);

/// Een uiting is hooguit zo lang; langer wordt geknipt (dezelfde grens als een stuk van een
/// bestand, [`audio::STUK_DUUR`]).
pub const UITING_MAX: Duration = audio::STUK_DUUR;

/// Zoveel stilte (ms) sluit voor Silero een uiting af. De standaard van Silero (100 ms)
/// knipt tussen woorden; een halve seconde houdt een zin bij elkaar. Nog niet gemeten.
pub const STILTE_MS: i32 = 500;

/// Zoveel marge (ms) zet Silero aan beide kanten van een uiting, zodat een zacht begin of
/// einde niet wegvalt (gelijk aan [`crate::stilte::KNIP_MARGE_MS`]).
pub const MARGE_MS: i32 = 200;

/// Wat er met een venster gebeurt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdeling {
    /// De uitingen die nu uitgeschreven kunnen worden, als bereiken in het venster, op
    /// volgorde en elk hooguit [`UITING_MAX`].
    pub klaar: Vec<Range<usize>>,
    /// Vanaf hier gaat het venster mee naar het volgende; alles ervoor is af.
    pub verder_vanaf: usize,
}

/// Het aantal samples (16 kHz) in een duur.
#[must_use]
pub fn samples_in(duur: Duration) -> usize {
    usize::try_from(duur.as_millis() * u128::from(DOEL_SAMPLERATE) / 1000).unwrap_or(usize::MAX)
}

/// Verdeelt een venster. `spraak` zijn de gebieden van Silero (`(begin, eind)` in samples
/// van het venster). Met `einde` is dit het laatste venster van het spoor en is alles af.
///
/// Gebieden die elkaar raken of overlappen (door de marge) worden één. Zonder `einde` gaat
/// alles vanaf [`RAND`] voor het eind van het venster mee naar het volgende, en ook een
/// gebied dat daar nog loopt, vanaf zijn begin. Is dat lopende gebied al langer dan
/// [`UITING_MAX`], dan zijn de geknipte delen voor het laatste wel klaar; zo schiet het
/// altijd op, ook als iemand een minuut lang zonder pauze praat.
#[must_use]
pub fn verdeel(samples: &[f32], spraak: &[(i64, i64)], einde: bool) -> Verdeling {
    let lengte = samples.len();
    let gebieden = samengevoegd(spraak, lengte);
    let rand = if einde {
        lengte
    } else {
        lengte.saturating_sub(samples_in(RAND))
    };
    let mut klaar = Vec::new();
    let mut verder_vanaf = rand;
    for gebied in gebieden {
        if gebied.end < rand || einde {
            klaar.extend(knip(samples, gebied));
            continue;
        }
        // Dit gebied loopt misschien nog door; latere gebieden liggen er helemaal achter.
        let mut delen = knip(samples, gebied.start..lengte);
        let laatste = delen.pop().map_or(gebied.start, |d| d.start);
        verder_vanaf = if delen.is_empty() {
            // Niets geknipt: ook de stilte voor een gebied dat pas na de rand begint, gaat mee.
            laatste.min(rand)
        } else {
            laatste
        };
        klaar.extend(delen);
        break;
    }
    Verdeling {
        klaar,
        verder_vanaf,
    }
}

/// De gebieden binnen het venster, op volgorde, met overlappende en aansluitende samen.
fn samengevoegd(spraak: &[(i64, i64)], lengte: usize) -> Vec<Range<usize>> {
    let grens = |w: i64| usize::try_from(w.max(0)).unwrap_or(usize::MAX).min(lengte);
    let mut gebieden: Vec<Range<usize>> = spraak
        .iter()
        .map(|&(begin, eind)| grens(begin)..grens(eind))
        .filter(|g| !g.is_empty())
        .collect();
    gebieden.sort_by_key(|g| g.start);
    let mut uit: Vec<Range<usize>> = Vec::with_capacity(gebieden.len());
    for gebied in gebieden {
        match uit.last_mut() {
            Some(vorig) if gebied.start <= vorig.end => vorig.end = vorig.end.max(gebied.end),
            _ => uit.push(gebied),
        }
    }
    uit
}

/// Knipt een gebied in delen van hooguit [`UITING_MAX`] op het stilste moment.
fn knip(samples: &[f32], gebied: Range<usize>) -> Vec<Range<usize>> {
    let begin = gebied.start;
    audio::deel_op_stilte(&samples[gebied], UITING_MAX)
        .into_iter()
        .map(|d| begin + d.start..begin + d.end)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: usize = DOEL_SAMPLERATE as usize;

    fn s(seconden: f64) -> i64 {
        #[allow(clippy::cast_possible_truncation, reason = "testwaarden")]
        let w = (seconden * 16_000.0) as i64;
        w
    }

    #[test]
    fn afgeronde_gebieden_zijn_klaar_en_de_rand_gaat_mee() {
        let venster = vec![0.1_f32; 60 * S];
        let v = verdeel(&venster, &[(s(1.0), s(3.0)), (s(10.0), s(12.5))], false);
        assert_eq!(v.klaar, vec![S..3 * S, 10 * S..12 * S + S / 2]);
        // Geen lopend gebied: alleen de laatste seconde gaat mee.
        assert_eq!(v.verder_vanaf, 59 * S);
    }

    #[test]
    fn een_gebied_over_de_rand_gaat_mee_naar_het_volgende_venster() {
        let venster = vec![0.1_f32; 60 * S];
        let v = verdeel(&venster, &[(s(2.0), s(4.0)), (s(55.0), s(60.0))], false);
        assert_eq!(v.klaar, vec![2 * S..4 * S]);
        assert_eq!(v.verder_vanaf, 55 * S);
        // Ook een gebied dat net voor de rand ophoudt, kan nog doorlopen.
        let v = verdeel(&venster, &[(s(50.0), s(59.5))], false);
        assert!(v.klaar.is_empty());
        assert_eq!(v.verder_vanaf, 50 * S);
    }

    #[test]
    fn in_het_laatste_venster_is_alles_klaar() {
        let venster = vec![0.1_f32; 20 * S];
        let v = verdeel(&venster, &[(s(15.0), s(20.0))], true);
        assert_eq!(v.klaar, vec![15 * S..20 * S]);
        assert_eq!(v.verder_vanaf, 20 * S);
        let v = verdeel(&venster, &[], true);
        assert!(v.klaar.is_empty());
        assert_eq!(v.verder_vanaf, 20 * S);
    }

    #[test]
    fn overlappende_gebieden_worden_een_en_grenzen_worden_begrensd() {
        let venster = vec![0.1_f32; 10 * S];
        let v = verdeel(
            &venster,
            &[
                (s(3.0), s(5.0)),
                (-100, s(1.0)),
                (s(4.5), s(6.0)),
                (s(9.0), s(99.0)),
            ],
            true,
        );
        assert_eq!(v.klaar, vec![0..S, 3 * S..6 * S, 9 * S..10 * S]);
    }

    #[test]
    fn een_lang_gebied_wordt_op_het_stilste_moment_geknipt() {
        // 45 s spraak met een dip op 25 s, in het laatste venster.
        let mut venster = vec![0.3_f32; 50 * S];
        venster[25 * S..25 * S + S / 4].fill(0.0);
        let v = verdeel(&venster, &[(0, s(45.0))], true);
        assert_eq!(v.klaar.len(), 2, "{:?}", v.klaar);
        assert!((25 * S..25 * S + S / 4).contains(&v.klaar[0].end));
        assert_eq!(v.klaar[1], v.klaar[0].end..45 * S);
    }

    #[test]
    fn wie_een_minuut_doorpraat_houdt_het_venster_niet_vast() {
        // Spraak over het hele venster: de geknipte delen voor het laatste zijn klaar.
        let venster = vec![0.3_f32; 60 * S];
        let v = verdeel(&venster, &[(0, s(60.0))], false);
        assert!(!v.klaar.is_empty());
        assert!(v.klaar.iter().all(|d| d.len() <= 30 * S));
        assert_eq!(v.klaar.first().map(|d| d.start), Some(0));
        assert_eq!(v.klaar.last().map(|d| d.end), Some(v.verder_vanaf));
        assert!(v.verder_vanaf > 0 && v.verder_vanaf < 60 * S);
    }
}
