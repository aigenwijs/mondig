//! De instellingen voor notulen (T3, ADR 0024 en 0025): waar Mondig de tekst van een verslag
//! naartoe stuurt om notulen te maken, welk model, en de sleutel van Mistral.
//!
//! De keuzes staan in de store (`aanbieder`, `model`, het vinkje over training). De sleutel staat
//! alleen in de sleutelbos van het besturingssysteem (`mondig_core::sleutel`): hij gaat van de
//! interface naar Rust en nooit terug. Wat de interface terugkrijgt, is alleen of er een sleutel
//! staat. Geen enkel commando schrijft tekst, een prompt, een antwoord of een sleutel in het
//! logboek.
//!
//! Het maken van notulen zelf staat in `vergadering/notulen_maken.rs`, want het hoort bij het
//! verslag dat open is.

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use mondig_core::sleutel::{self, Sleutel};
use mondig_core::taak::{Foutmelding, Vervolg};
use mondig_core::taalmodel::{Opties, TaalmodelFout, Verbinding, Voorinstelling};
use serde::Serialize;
use tauri::AppHandle;

use crate::sneltoets::{lees_instelling, schrijf_instelling, wis_instelling};
use crate::vergadering::blokkerend;

const SLEUTEL_AANBIEDER: &str = "notulen_aanbieder";
const SLEUTEL_MODEL_MISTRAL: &str = "notulen_model_mistral";
const SLEUTEL_MODEL_OLLAMA: &str = "notulen_model_ollama";
const SLEUTEL_TRAINING: &str = "notulen_training_uit";
const JA: &str = "ja";

/// Het standaardmodel bij Mistral: "Nauwkeurig".
pub(crate) const MISTRAL_NAUWKEURIG: &str = "mistral-large-latest";
/// De tweede keuze bij Mistral: "Sneller".
pub(crate) const MISTRAL_SNEL: &str = "mistral-small-latest";

/// Zo lang wacht Mondig op Ollama bij het kijken of het draait: een lokale aanvraag is meteen
/// beantwoord, dus een korte tijd houdt de rij in Instellingen snel.
const OLLAMA_VERBINDEN: Duration = Duration::from_millis(1500);
const OLLAMA_TOTAAL: Duration = Duration::from_secs(4);
/// Een sleutel testen mag langer duren: het gaat over internet.
const TEST_TOTAAL: Duration = Duration::from_secs(15);

/// Wat de gebruiker in Instellingen koos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Gekozen {
    /// De aanbieder; `None` is "Niet ingesteld": dan maakt Mondig geen notulen.
    pub(crate) aanbieder: Option<Voorinstelling>,
    /// Het model bij Mistral (een van de twee vaste keuzes).
    pub(crate) mistral_model: String,
    /// Het model bij Ollama, als de gebruiker er een koos.
    pub(crate) ollama_model: Option<String>,
    /// Heeft de gebruiker bevestigd dat training uit staat voor zijn sleutel?
    pub(crate) training_uit: bool,
}

impl Gekozen {
    fn uit_store(app: &AppHandle) -> Self {
        let aanbieder = match lees_instelling(app, SLEUTEL_AANBIEDER).as_deref() {
            Some("mistral") => Some(Voorinstelling::Mistral),
            Some("ollama") => Some(Voorinstelling::OllamaLokaal),
            _ => None,
        };
        let mistral_model = lees_instelling(app, SLEUTEL_MODEL_MISTRAL)
            .filter(|m| m == MISTRAL_SNEL)
            .unwrap_or_else(|| MISTRAL_NAUWKEURIG.to_string());
        Self {
            aanbieder,
            mistral_model,
            ollama_model: lees_instelling(app, SLEUTEL_MODEL_OLLAMA)
                .filter(|m| !m.trim().is_empty()),
            training_uit: lees_instelling(app, SLEUTEL_TRAINING).as_deref() == Some(JA),
        }
    }
}

/// De keuzes uit de store.
pub(crate) fn lees(app: &AppHandle) -> Gekozen {
    Gekozen::uit_store(app)
}

/// Mag Mondig notulen maken met deze keuzes, en met welke aanbieder en welk model? Zo niet, dan
/// een melding in gewone taal. `sleutel_staat` is alleen van belang bij Mistral.
pub(crate) fn bepaal_start(
    gekozen: &Gekozen,
    sleutel_staat: bool,
) -> Result<(Voorinstelling, String), Foutmelding> {
    let geen = |melding: &str| Err(Foutmelding::nieuw(melding, Vervolg::Geen));
    match gekozen.aanbieder {
        None => geen(
            "Je hebt nog niet gekozen waar Mondig notulen maakt. Dat kies je bij Instellingen, onder Notulen.",
        ),
        Some(Voorinstelling::Mistral) if !gekozen.training_uit => geen(
            "Zet bij Instellingen, onder Notulen, het vinkje dat training uit staat voor je sleutel. Zonder dat maakt Mondig geen notulen via Mistral.",
        ),
        Some(Voorinstelling::Mistral) if !sleutel_staat => geen(
            "Er is nog geen sleutel voor Mistral ingevuld. Vul hem in bij Instellingen, onder Notulen.",
        ),
        Some(Voorinstelling::Mistral) => {
            Ok((Voorinstelling::Mistral, gekozen.mistral_model.clone()))
        }
        Some(Voorinstelling::OllamaLokaal) => match &gekozen.ollama_model {
            Some(model) => Ok((Voorinstelling::OllamaLokaal, model.clone())),
            None => {
                geen("Kies bij Instellingen, onder Notulen, welk model van Ollama Mondig gebruikt.")
            }
        },
    }
}

/// De instellingen zoals de interface ze toont. Bevat bewust nooit een sleutel, alleen of er een
/// staat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstellingenInfo {
    /// `mistral`, `ollama` of niets.
    aanbieder: Option<&'static str>,
    /// Het model bij Mistral.
    mistral_model: String,
    /// Het model bij Ollama, als er een gekozen is.
    ollama_model: Option<String>,
    /// Het vinkje dat training uit staat.
    training_uit: bool,
    /// Staat er een sleutel voor Mistral in de sleutelbos?
    sleutel_staat: bool,
    /// Wat er met de sleutelbos mis is, in gewone taal.
    sleutel_fout: Option<String>,
}

impl InstellingenInfo {
    fn van(gekozen: &Gekozen, sleutel: Result<bool, String>) -> Self {
        let (sleutel_staat, sleutel_fout) = match sleutel {
            Ok(staat) => (staat, None),
            Err(melding) => (false, Some(melding)),
        };
        Self {
            aanbieder: gekozen.aanbieder.map(Voorinstelling::code),
            mistral_model: gekozen.mistral_model.clone(),
            ollama_model: gekozen.ollama_model.clone(),
            training_uit: gekozen.training_uit,
            sleutel_staat,
            sleutel_fout,
        }
    }
}

fn instellingen_nu(app: &AppHandle) -> InstellingenInfo {
    let sleutel = sleutel::bestaat(Voorinstelling::Mistral).map_err(|f| f.to_string());
    InstellingenInfo::van(&Gekozen::uit_store(app), sleutel)
}

/// De instellingen van Notulen.
#[tauri::command]
pub(crate) async fn notulen_instellingen(app: AppHandle) -> Result<InstellingenInfo, Foutmelding> {
    blokkerend(move || Ok(instellingen_nu(&app))).await
}

/// Kiest waar notulen gemaakt worden: `mistral`, `ollama` of niets (`null`).
#[tauri::command]
pub(crate) async fn notulen_zet_aanbieder(
    app: AppHandle,
    aanbieder: Option<String>,
) -> Result<InstellingenInfo, Foutmelding> {
    blokkerend(move || {
        match aanbieder.as_deref() {
            Some("mistral") => schrijf_instelling(&app, SLEUTEL_AANBIEDER, "mistral"),
            Some("ollama") => schrijf_instelling(&app, SLEUTEL_AANBIEDER, "ollama"),
            _ => wis_instelling(&app, SLEUTEL_AANBIEDER),
        }
        crate::opschonen::na_wijziging_notulen(&app);
        Ok(instellingen_nu(&app))
    })
    .await
}

/// Kiest het model: bij Mistral een van de twee vaste keuzes, bij Ollama een naam uit de lijst
/// van Ollama.
#[tauri::command]
pub(crate) async fn notulen_zet_model(
    app: AppHandle,
    aanbieder: String,
    model: String,
) -> Result<InstellingenInfo, Foutmelding> {
    blokkerend(move || {
        let model = model.trim();
        match aanbieder.as_str() {
            "mistral" if model == MISTRAL_NAUWKEURIG || model == MISTRAL_SNEL => {
                schrijf_instelling(&app, SLEUTEL_MODEL_MISTRAL, model);
            }
            "ollama" if !model.is_empty() && model.len() <= 200 => {
                schrijf_instelling(&app, SLEUTEL_MODEL_OLLAMA, model);
            }
            _ => {
                return Err(Foutmelding::nieuw(
                    "Dit model kent Mondig niet. Kies een model uit de lijst.",
                    Vervolg::Geen,
                ));
            }
        }
        crate::opschonen::na_wijziging_notulen(&app);
        Ok(instellingen_nu(&app))
    })
    .await
}

/// Zet het vinkje "Training staat uit voor deze sleutel."
#[tauri::command]
pub(crate) async fn notulen_zet_training(
    app: AppHandle,
    uit: bool,
) -> Result<InstellingenInfo, Foutmelding> {
    blokkerend(move || {
        if uit {
            schrijf_instelling(&app, SLEUTEL_TRAINING, JA);
        } else {
            wis_instelling(&app, SLEUTEL_TRAINING);
        }
        crate::opschonen::na_wijziging_notulen(&app);
        Ok(instellingen_nu(&app))
    })
    .await
}

/// Overschrijft de tekst die de interface stuurde, voor zover dat zonder `unsafe` kan: de bytes
/// gaan eruit en worden op nul gezet voor ze vrijkomen.
fn wis_tekst(tekst: String) {
    let mut bytes = tekst.into_bytes();
    bytes.fill(0);
    drop(bytes);
}

/// Bewaart de sleutel van Mistral in de sleutelbos. De waarde komt alleen hier binnen en gaat
/// nooit terug; een nieuwe sleutel haalt het vinkje over training weg, want dat gold voor de oude.
#[tauri::command]
pub(crate) async fn notulen_sleutel_zet(
    app: AppHandle,
    waarde: String,
) -> Result<InstellingenInfo, Foutmelding> {
    blokkerend(move || {
        let sleutel = Sleutel::nieuw(&waarde);
        wis_tekst(waarde);
        let sleutel = sleutel.map_err(sleutel::SleutelFout::foutmelding)?;
        sleutel::zet(Voorinstelling::Mistral, &sleutel)
            .map_err(sleutel::SleutelFout::foutmelding)?;
        wis_instelling(&app, SLEUTEL_TRAINING);
        crate::opschonen::na_wijziging_notulen(&app);
        Ok(instellingen_nu(&app))
    })
    .await
}

/// Wist de sleutel van Mistral uit de sleutelbos, met het vinkje over training.
#[tauri::command]
pub(crate) async fn notulen_sleutel_wis(app: AppHandle) -> Result<InstellingenInfo, Foutmelding> {
    blokkerend(move || {
        sleutel::wis(Voorinstelling::Mistral).map_err(sleutel::SleutelFout::foutmelding)?;
        wis_instelling(&app, SLEUTEL_TRAINING);
        crate::opschonen::na_wijziging_notulen(&app);
        Ok(instellingen_nu(&app))
    })
    .await
}

/// Test de sleutel van Mistral met een lichte aanvraag (de lijst met modellen). Geen tekst van een
/// verslag gaat mee.
#[tauri::command]
pub(crate) async fn notulen_sleutel_test() -> Result<(), Foutmelding> {
    blokkerend(|| {
        let verbinding = Verbinding::voor(Voorinstelling::Mistral)
            .map_err(Foutmelding::from)?
            .met_opties(Opties {
                verbinden: Duration::from_secs(10),
                totaal: TEST_TOTAAL,
            });
        verbinding
            .modellen(&AtomicBool::new(false))
            .map(|_| ())
            .map_err(Foutmelding::from)
    })
    .await
}

/// Draait Ollama op deze computer, en welke modellen heeft het?
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OllamaStand {
    draait: bool,
    modellen: Vec<String>,
    /// Een melding als het niet lukte om een andere reden dan "Ollama draait niet".
    melding: Option<String>,
}

fn ollama_stand_van(uitslag: Result<Vec<String>, TaalmodelFout>) -> OllamaStand {
    match uitslag {
        Ok(modellen) => OllamaStand {
            draait: true,
            modellen,
            melding: None,
        },
        Err(TaalmodelFout::OllamaDraaitNiet) => OllamaStand {
            draait: false,
            modellen: Vec::new(),
            melding: None,
        },
        Err(fout) => OllamaStand {
            draait: false,
            modellen: Vec::new(),
            melding: Some(fout.to_string()),
        },
    }
}

/// Kijkt of Ollama draait en welke modellen het heeft (`GET /models`, met een korte tijdslimiet).
#[tauri::command]
pub(crate) async fn notulen_ollama_stand() -> Result<OllamaStand, Foutmelding> {
    blokkerend(|| {
        let uitslag = Verbinding::voor(Voorinstelling::OllamaLokaal).and_then(|verbinding| {
            verbinding
                .met_opties(Opties {
                    verbinden: OLLAMA_VERBINDEN,
                    totaal: OLLAMA_TOTAAL,
                })
                .modellen(&AtomicBool::new(false))
        });
        Ok(ollama_stand_van(uitslag))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gekozen(aanbieder: Option<Voorinstelling>, training: bool) -> Gekozen {
        Gekozen {
            aanbieder,
            mistral_model: MISTRAL_NAUWKEURIG.to_string(),
            ollama_model: None,
            training_uit: training,
        }
    }

    #[test]
    fn notulen_maken_weigert_zonder_instelling() {
        let fout = bepaal_start(&gekozen(None, true), true).unwrap_err();
        assert!(
            fout.melding.contains("nog niet gekozen"),
            "{}",
            fout.melding
        );
    }

    #[test]
    fn notulen_maken_via_mistral_weigert_zonder_vinkje() {
        let fout = bepaal_start(&gekozen(Some(Voorinstelling::Mistral), false), true).unwrap_err();
        assert!(
            fout.melding.contains("training uit staat"),
            "{}",
            fout.melding
        );
    }

    #[test]
    fn notulen_maken_via_mistral_weigert_zonder_sleutel() {
        let fout = bepaal_start(&gekozen(Some(Voorinstelling::Mistral), true), false).unwrap_err();
        assert!(fout.melding.contains("geen sleutel"), "{}", fout.melding);
    }

    #[test]
    fn mistral_met_vinkje_en_sleutel_gebruikt_het_gekozen_model() {
        let (aanbieder, model) =
            bepaal_start(&gekozen(Some(Voorinstelling::Mistral), true), true).unwrap();
        assert_eq!(aanbieder, Voorinstelling::Mistral);
        assert_eq!(model, "mistral-large-latest");
    }

    #[test]
    fn ollama_heeft_geen_sleutel_of_vinkje_nodig_maar_wel_een_model() {
        let fout =
            bepaal_start(&gekozen(Some(Voorinstelling::OllamaLokaal), false), false).unwrap_err();
        assert!(fout.melding.contains("Ollama"), "{}", fout.melding);
        let mut met_model = gekozen(Some(Voorinstelling::OllamaLokaal), false);
        met_model.ollama_model = Some("qwen3:8b".into());
        let (_, model) = bepaal_start(&met_model, false).unwrap();
        assert_eq!(model, "qwen3:8b");
    }

    #[test]
    fn de_instellingen_voor_de_interface_hebben_nooit_een_sleutel() {
        let info = InstellingenInfo::van(&gekozen(Some(Voorinstelling::Mistral), true), Ok(true));
        let json = serde_json::to_value(&info).unwrap();
        // Alleen deze velden gaan naar de interface: of er een sleutel staat, nooit de waarde.
        let mut velden: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        velden.sort_unstable();
        assert_eq!(
            velden,
            [
                "aanbieder",
                "mistralModel",
                "ollamaModel",
                "sleutelFout",
                "sleutelStaat",
                "trainingUit"
            ]
        );
        assert_eq!(json["sleutelStaat"], true);
    }

    #[test]
    fn een_sleutelbos_die_faalt_staat_als_geen_sleutel_met_een_melding() {
        let info = InstellingenInfo::van(
            &gekozen(Some(Voorinstelling::Mistral), true),
            Err("De sleutelbos gaf een fout.".into()),
        );
        assert!(!info.sleutel_staat);
        assert!(info.sleutel_fout.is_some());
    }

    #[test]
    fn ollama_dat_niet_draait_is_geen_foutmelding() {
        let stand = ollama_stand_van(Err(TaalmodelFout::OllamaDraaitNiet));
        assert!(!stand.draait);
        assert!(stand.melding.is_none());
        let stand = ollama_stand_van(Ok(vec!["qwen3:8b".into()]));
        assert!(stand.draait);
        assert_eq!(stand.modellen, ["qwen3:8b"]);
    }
}
