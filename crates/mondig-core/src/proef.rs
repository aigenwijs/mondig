//! De proef op het dicteerscherm (sprint 3a): een opnamebestand kiezen en uitschrijven,
//! zodat de hele keten (decoderen, werkproces, model, filter) te zien is voordat de
//! sneltoets er is.
//!
//! De Tauri-schil kiest het bestand en stuurt de gebeurtenissen door; wat er met het
//! bestand en de uitslag gebeurt staat hier, zodat het zonder app te testen is.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use crate::aanbieder::AanbiederFout;
use crate::audio;
use crate::engine::supervisor::controleer_dictaatlengte;
use crate::filter::{Gefilterd, Uitkomst};
use crate::soevereiniteit::Soevereiniteit;
use crate::taak::{Foutmelding, TaakId, Vervolg};

/// Leest een opnamebestand en zet het om naar 16 kHz mono, met de lengtegrens van een
/// dictaat (tien minuten, ADR 0012).
///
/// # Errors
///
/// Geeft een [`AanbiederFout`] in gewone taal als het bestand niet te lezen is of te lang duurt.
pub fn lees_opname(pad: &Path) -> Result<Vec<f32>, AanbiederFout> {
    let geluid = audio::laad(pad)?;
    controleer_dictaatlengte(&geluid.samples)?;
    Ok(geluid.samples)
}

/// Kan de proef beginnen? Zo niet, dan de melding met de vervolgactie: een ontbrekend
/// werkproces is een kapotte installatie (geen vervolg), een ontbrekend model
/// verwijst naar het downloadscherm.
#[must_use]
pub fn ontbrekend(werkproces_aanwezig: bool, model_compleet: bool) -> Option<Foutmelding> {
    if !werkproces_aanwezig {
        return Some(Foutmelding::nieuw(
            "Het spraakprogramma ontbreekt bij Mondig. Installeer Mondig opnieuw.",
            Vervolg::Geen,
        ));
    }
    if !model_compleet {
        return Some(Foutmelding::nieuw(
            "Het spraakmodel is er nog niet. Download het eerst.",
            Vervolg::OpnieuwDownloaden,
        ));
    }
    None
}

/// De uitslag van een proef voor de interface. Serialiseert met camelCase-sleutels
/// (`duurMs`). Bewust zonder `Debug`, zodat de tekst nooit per ongeluk in een log komt.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProefResultaat {
    /// Welk dictaat.
    pub taak: TaakId,
    /// Wat het filter ervan vindt: bij `tekst` staat de tekst erbij, anders is er een
    /// melding ("geen geluid", "geen tekst", "verdacht") nodig in plaats van tekst.
    pub uitkomst: Uitkomst,
    /// De tekst, alleen bij [`Uitkomst::Tekst`].
    pub tekst: Option<String>,
    /// Tijd van het begin van het uitschrijven tot de uitslag, in milliseconden.
    pub duur_ms: u64,
    /// Waar de audio verwerkt is.
    pub soevereiniteit: Soevereiniteit,
}

impl ProefResultaat {
    /// Maakt het resultaat uit de gefilterde uitslag.
    #[must_use]
    pub fn nieuw(
        taak: TaakId,
        gefilterd: &Gefilterd,
        duur: Duration,
        soevereiniteit: Soevereiniteit,
    ) -> Self {
        let tekst = (gefilterd.uitkomst == Uitkomst::Tekst).then(|| gefilterd.transcript.tekst());
        Self {
            taak,
            uitkomst: gefilterd.uitkomst,
            tekst,
            duur_ms: u64::try_from(duur.as_millis()).unwrap_or(u64::MAX),
            soevereiniteit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aanbieder::Transcript;
    use crate::engine::protocol::Segment;

    fn gefilterd(uitkomst: Uitkomst, tekst: &str) -> Gefilterd {
        Gefilterd {
            transcript: Transcript {
                segmenten: vec![Segment::nieuw(0, 1000, tekst)],
                audio_duur: Duration::from_secs(1),
            },
            uitkomst,
            aandeel_spraak: 0.9,
            rms: 0.1,
            ingrepen: Vec::new(),
        }
    }

    fn schrijf_wav(pad: &Path, seconden: usize) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut schrijver = hound::WavWriter::create(pad, spec).unwrap();
        for i in 0..seconden * 16_000 {
            schrijver
                .write_sample(i16::from(u8::try_from(i % 100).unwrap()))
                .unwrap();
        }
        schrijver.finalize().unwrap();
    }

    #[test]
    fn lees_opname_geeft_16_khz_samples() {
        let map = tempfile::tempdir().unwrap();
        let pad = map.path().join("kort.wav");
        schrijf_wav(&pad, 2);
        assert_eq!(lees_opname(&pad).unwrap().len(), 32_000);
    }

    #[test]
    fn lees_opname_weigert_meer_dan_tien_minuten() {
        let map = tempfile::tempdir().unwrap();
        let pad = map.path().join("lang.wav");
        schrijf_wav(&pad, 601);
        let fout = lees_opname(&pad).unwrap_err();
        let melding = Foutmelding::from(fout);
        assert_eq!(melding.vervolg, Vervolg::Geen);
        assert!(
            melding.melding.contains("10 minuten"),
            "{}",
            melding.melding
        );
    }

    #[test]
    fn lees_opname_meldt_een_onleesbaar_bestand_in_gewone_taal() {
        let map = tempfile::tempdir().unwrap();
        let pad = map.path().join("kapot.wav");
        std::fs::write(&pad, b"dit is geen geluid").unwrap();
        let melding = Foutmelding::from(lees_opname(&pad).unwrap_err());
        assert_eq!(melding.vervolg, Vervolg::Geen);
        assert!(!melding.melding.is_empty());
        let weg = Foutmelding::from(lees_opname(&map.path().join("weg.wav")).unwrap_err());
        assert!(weg.melding.contains("niet gevonden"), "{}", weg.melding);
    }

    #[test]
    fn ontbrekend_werkproces_of_model_geeft_een_melding_met_vervolg() {
        assert_eq!(ontbrekend(true, true), None);
        assert_eq!(
            ontbrekend(false, true).map(|m| m.vervolg),
            Some(Vervolg::Geen)
        );
        assert_eq!(
            ontbrekend(true, false).map(|m| m.vervolg),
            Some(Vervolg::OpnieuwDownloaden)
        );
        // Zonder werkproces helpt downloaden niet: dat gaat voor.
        assert_eq!(
            ontbrekend(false, false).map(|m| m.vervolg),
            Some(Vervolg::Geen)
        );
    }

    #[test]
    fn resultaat_heeft_alleen_bij_gewone_tekst_een_tekst() {
        let duur = Duration::from_millis(1400);
        let resultaat = ProefResultaat::nieuw(
            TaakId(4),
            &gefilterd(Uitkomst::Tekst, " Hallo wereld. "),
            duur,
            Soevereiniteit::Lokaal,
        );
        assert_eq!(resultaat.tekst.as_deref(), Some("Hallo wereld."));
        assert_eq!(resultaat.duur_ms, 1400);
        let json = serde_json::to_string(&resultaat).unwrap();
        assert_eq!(
            json,
            r#"{"taak":4,"uitkomst":"tekst","tekst":"Hallo wereld.","duurMs":1400,"soevereiniteit":"lokaal"}"#
        );
        for uitkomst in [
            Uitkomst::GeenGeluid,
            Uitkomst::GeenTekst,
            Uitkomst::Verdacht,
        ] {
            let resultaat = ProefResultaat::nieuw(
                TaakId(5),
                &gefilterd(uitkomst, "Ondertiteling ingediend door"),
                duur,
                Soevereiniteit::Lokaal,
            );
            assert!(resultaat.tekst.is_none(), "{uitkomst:?}");
        }
        let json = serde_json::to_string(&ProefResultaat::nieuw(
            TaakId(5),
            &gefilterd(Uitkomst::GeenGeluid, ""),
            duur,
            Soevereiniteit::Lokaal,
        ))
        .unwrap();
        assert!(json.contains(r#""uitkomst":"geen_geluid""#) && json.contains(r#""tekst":null"#));
    }
}
