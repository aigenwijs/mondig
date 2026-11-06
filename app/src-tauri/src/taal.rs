//! De gesproken taal, op één plek (ADR 0028 punt 3).
//!
//! Mondig is Nederlands eerst: modellen die een taal nodig hebben (Whisper, Canary) krijgen
//! [`MODELTAAL`]. Parakeet negeert de taal; voor zo'n model is de taal van de interface de beste
//! schatting. Het stopwoordenfilter en het opschonen met een taalmodel gebruiken dezelfde
//! uitkomst, zodat ze het nooit oneens zijn.

use mondig_core::model::{Engine, Gebruik};
use tauri::AppHandle;

/// De taal die elk spraakmodel meekrijgt bij het laden.
pub(crate) const MODELTAAL: &str = "nl";

/// De taal van de interface. Er is nog geen keuze voor in de app, dus Nederlands.
const INTERFACETAAL: &str = "nl";

/// De gesproken taal bij een soort model: die van het model als het een taal gebruikt,
/// anders die van de interface.
pub(crate) fn gesproken_taal_van(engine: Engine) -> &'static str {
    match engine {
        Engine::Whisper | Engine::Canary => MODELTAAL,
        Engine::Parakeet | Engine::Silero | Engine::Sortformer => INTERFACETAAL,
    }
}

/// De gesproken taal van het model dat nu geldt voor `gebruik`. Is het model niet te bepalen,
/// dan geldt de taal van de interface.
pub(crate) fn gesproken_taal(app: &AppHandle, gebruik: Gebruik) -> &'static str {
    crate::modellen::geldend_voor(app, gebruik)
        .map_or(INTERFACETAAL, |(model, _)| gesproken_taal_van(model.engine))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_en_canary_krijgen_de_modeltaal_en_parakeet_die_van_de_interface() {
        assert_eq!(gesproken_taal_van(Engine::Whisper), MODELTAAL);
        assert_eq!(gesproken_taal_van(Engine::Canary), MODELTAAL);
        assert_eq!(gesproken_taal_van(Engine::Parakeet), INTERFACETAAL);
    }
}
