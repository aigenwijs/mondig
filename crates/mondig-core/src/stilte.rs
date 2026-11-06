//! Stilte in een dictaat: het stuk met spraak bepalen en een nulsignaal herkennen.
//!
//! Silero (in het werkproces, ADR 0012) geeft de spraaksegmenten van een opname. Deze
//! module bevat de rekenregels daaromheen, los van het model, zodat ze zonder
//! werkproces te testen zijn. Ook het nulsignaal (een gedempte microfoon) zit hier: dat
//! is een melding voor de gebruiker en geen lege tekst, en het model komt er niet bij.

use std::ops::Range;

use crate::audio::DOEL_SAMPLERATE;
use crate::filter::{FilterInstellingen, is_stil};
use crate::taak::{Foutmelding, Vervolg};

/// Zoveel stilte (in milliseconden) blijft aan begin en eind van de spraak staan, zodat een
/// zacht begin of einde van een woord niet wegvalt.
pub const KNIP_MARGE_MS: u64 = 200;

/// De melding bij een nulsignaal.
pub const NULSIGNAAL_MELDING: &str = "Mondig hoort niets. Controleer of je microfoon aan staat.";

/// Het deel van de opname dat overblijft na het wegknippen van stilte aan begin en eind:
/// van het begin van het eerste tot het eind van het laatste spraaksegment, plus
/// [`KNIP_MARGE_MS`] aan beide kanten (begrensd op de opname). Segmenten zijn
/// `(begin, eind)` in samples, in tijdsvolgorde. Geeft `None` als er geen spraak is.
#[must_use]
pub fn spraakbereik(segmenten: &[(i64, i64)], aantal_samples: usize) -> Option<Range<usize>> {
    let begin = segmenten.iter().map(|&(begin, _)| begin).min()?;
    let eind = segmenten.iter().map(|&(_, eind)| eind).max()?;
    let marge = i64::try_from(KNIP_MARGE_MS * u64::from(DOEL_SAMPLERATE) / 1000).ok()?;
    let totaal = i64::try_from(aantal_samples).ok()?;
    let begin = begin.saturating_sub(marge).clamp(0, totaal);
    let eind = eind.saturating_add(marge).clamp(0, totaal);
    if eind <= begin {
        return None;
    }
    Some(usize::try_from(begin).ok()?..usize::try_from(eind).ok()?)
}

/// Is de opname vrijwel stil (een gedempte of dode microfoon)? Dan geeft dit een melding in
/// gewone taal met het vervolg "microfooninstelling openen"; de aanroeper schrijft dan niet
/// uit. Gebruikt dezelfde drempel als het filter tegen verzonnen tekst
/// ([`FilterInstellingen::stilte_rms`]).
///
/// # Errors
///
/// Geeft een [`Foutmelding`] bij een nulsignaal.
pub fn controleer_nulsignaal(
    samples: &[f32],
    instellingen: &FilterInstellingen,
) -> Result<(), Foutmelding> {
    if is_stil(samples, instellingen) {
        return Err(Foutmelding::nieuw(
            NULSIGNAAL_MELDING,
            Vervolg::MicrofooninstellingOpenen,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bereik_loopt_van_eerste_begin_tot_laatste_eind_met_marge() {
        let bereik = spraakbereik(&[(16_000, 32_000), (48_000, 64_000)], 160_000).unwrap();
        assert_eq!(bereik, (16_000 - 3200)..(64_000 + 3200));
    }

    #[test]
    fn marge_wordt_begrensd_op_de_opname() {
        let bereik = spraakbereik(&[(1000, 158_000)], 160_000).unwrap();
        assert_eq!(bereik, 0..160_000);
    }

    #[test]
    fn geen_segmenten_is_geen_spraak() {
        assert_eq!(spraakbereik(&[], 16_000), None);
    }

    #[test]
    fn segment_buiten_de_opname_is_geen_spraak() {
        assert_eq!(spraakbereik(&[(-9000, -5000)], 16_000), None);
        assert_eq!(spraakbereik(&[(100_000, 120_000)], 16_000), None);
    }

    #[test]
    fn nulsignaal_geeft_een_melding_met_vervolg_en_geen_lege_tekst() {
        let instellingen = FilterInstellingen::standaard();
        let fout = controleer_nulsignaal(&vec![0.0; 16_000], &instellingen).unwrap_err();
        assert_eq!(
            fout.melding,
            "Mondig hoort niets. Controleer of je microfoon aan staat."
        );
        assert_eq!(fout.vervolg, Vervolg::MicrofooninstellingOpenen);
        // Een lege opname hoort ook niets.
        assert!(controleer_nulsignaal(&[], &instellingen).is_err());
        // Dithering rond -90 dBFS is nog steeds "niets".
        let ruis: Vec<f32> = (0..16_000)
            .map(|i| if i % 2 == 0 { 0.000_03 } else { -0.000_03 })
            .collect();
        assert!(controleer_nulsignaal(&ruis, &instellingen).is_err());
    }

    #[test]
    fn zachte_spraak_is_geen_nulsignaal() {
        let zacht: Vec<f32> = (0..16_000)
            .map(|i| if i % 2 == 0 { 0.01 } else { -0.01 })
            .collect();
        assert!(controleer_nulsignaal(&zacht, &FilterInstellingen::standaard()).is_ok());
    }
}
