//! De gemeten snelheid van elk spraakmodel op deze computer (ADR 0023, punt 6).
//!
//! Na elk geslaagd opnieuw uitschrijven bewaart Mondig hoe lang het uitschrijven zelf duurde
//! (vanaf het eerste percentage, dus zonder het laden van het model) ten opzichte van het
//! verwerkte geluid: de realtime-factor, per model-id. Gewoon uitschrijven telt niet mee, want
//! daar zit de sprekerstap in; zo komt de schatting alleen uit hetzelfde soort werk. De dialoog "Opnieuw
//! uitschrijven" rekent daarmee de duur voor een opname uit; zonder meting zegt hij dat eerlijk
//! in plaats van een getal te verzinnen. In de store staan alleen getallen: geen tekst, geen
//! namen en geen paden.

use std::collections::BTreeMap;
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_store::{JsonValue, StoreExt};

use crate::sneltoets::INSTELLINGEN;

/// Sleutel in de store: per model-id de realtime-factor.
const SLEUTEL: &str = "snelheid_opnieuw_per_model";

/// Een factor boven deze waarde is geen meting maar een uitschieter (bijvoorbeeld een computer
/// die sliep tijdens het uitschrijven).
const MAX_FACTOR: f64 = 20.0;

/// De gemeten realtime-factoren per model-id; leeg als er nog niets gemeten is.
pub(crate) fn lees(app: &AppHandle) -> BTreeMap<String, f64> {
    let Ok(store) = app.store(INSTELLINGEN) else {
        return BTreeMap::new();
    };
    let Some(waarde) = store.get(SLEUTEL) else {
        return BTreeMap::new();
    };
    waarde
        .as_object()
        .map(|lijst| {
            lijst
                .iter()
                .filter_map(|(id, factor)| Some((id.clone(), factor.as_f64()?)))
                .filter(|(_, factor)| bruikbaar(*factor))
                .collect()
        })
        .unwrap_or_default()
}

/// Bewaart een nieuwe meting voor `model_id`, gemengd met de vorige ([`meng`]).
pub(crate) fn bewaar(app: &AppHandle, model_id: &str, factor: f64) {
    let Ok(store) = app.store(INSTELLINGEN) else {
        return;
    };
    let mut lijst = lees(app);
    let Some(nieuw) = meng(lijst.get(model_id).copied(), factor) else {
        return;
    };
    lijst.insert(model_id.to_string(), nieuw);
    store.set(SLEUTEL, lijst.into_iter().collect::<JsonValue>());
    let _ = store.save();
}

/// Is dit een bruikbare factor: een eindig getal boven nul en niet absurd groot?
fn bruikbaar(factor: f64) -> bool {
    factor.is_finite() && factor > 0.0 && factor <= MAX_FACTOR
}

/// De factor na een nieuwe meting: het gemiddelde van de vorige en de nieuwe, zodat één trage
/// keer (een model dat nog moest laden) de schatting niet meteen omgooit. Een onbruikbare
/// meting verandert niets (`None`).
fn meng(vorige: Option<f64>, nieuw: f64) -> Option<f64> {
    if !bruikbaar(nieuw) {
        return None;
    }
    Some(match vorige.filter(|v| bruikbaar(*v)) {
        Some(vorige) => f64::midpoint(vorige, nieuw),
        None => nieuw,
    })
}

/// De realtime-factor van een uitschrijving: hoe lang het duurde gedeeld door hoeveel geluid er
/// verwerkt is. `None` zonder geluid.
pub(crate) fn factor(duurde: Duration, geluid: Duration) -> Option<f64> {
    if geluid.is_zero() {
        return None;
    }
    Some(duurde.as_secs_f64() / geluid.as_secs_f64())
}

/// De verwachte duur voor zoveel geluid bij deze factor.
pub(crate) fn schat(factor: f64, geluid: Duration) -> Option<Duration> {
    bruikbaar(factor).then(|| geluid.mul_f64(factor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn een_meting_mengt_met_de_vorige_en_onzin_telt_niet() {
        assert_eq!(meng(None, 0.2), Some(0.2));
        assert!(meng(Some(0.2), 0.4).is_some_and(|f| (f - 0.3).abs() < 1e-9));
        assert_eq!(meng(Some(0.2), f64::NAN), None);
        assert_eq!(meng(Some(0.2), 0.0), None);
        assert_eq!(meng(Some(0.2), 100.0), None);
        // Een onbruikbare vorige waarde wordt vervangen.
        assert_eq!(meng(Some(-1.0), 0.5), Some(0.5));
    }

    #[test]
    fn factor_en_schatting() {
        let uur = Duration::from_secs(3_600);
        assert_eq!(factor(Duration::from_secs(360), uur), Some(0.1));
        assert_eq!(factor(Duration::from_secs(1), Duration::ZERO), None);
        assert_eq!(schat(0.1, uur), Some(Duration::from_secs(360)));
        assert_eq!(schat(f64::INFINITY, uur), None);
    }
}
