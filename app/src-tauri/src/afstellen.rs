//! "Mondig opnieuw afstellen", het overnemen van het advies (N-C5, ontwerp ux-naar-9 sectie 5).
//!
//! De interface laat het advies zien als lijst met vinkjes; er begint niets tot de gebruiker op
//! "Advies overnemen" klikt. Dan stuurt de interface de aangevinkte punten naar
//! `advies_overnemen`. De beslissing wat er per punt gebeurt, staat in [`plan`] (puur, zonder
//! app, dus te testen):
//!
//! - de videokaart gaat meteen aan;
//! - een model dat al op de computer staat, wordt meteen in gebruik genomen;
//! - een model dat nog binnen moet komen, gaat in de downloadrij met de taken erbij
//!   (`taken na download`): het huidige model blijft in gebruik tot de download klaar is. Mislukt
//!   of stopt de download, dan verandert er niets.
//!
//! Hoort één model bij beide taken, dan wordt het maar één keer gedownload.

use std::collections::BTreeMap;

use mondig_core::model::{self, Gebruik};
use mondig_core::modelkeuze;
use mondig_core::taak::{Foutmelding, Vervolg};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::downloads::{self, Downloads};
use crate::modellen;

/// Eén aangevinkt punt uit het advies, zoals de interface het stuurt.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "soort", rename_all = "snake_case")]
pub(crate) enum AdviesPunt {
    /// De videokaart aanzetten.
    Videokaart,
    /// Een model voor een taak in gebruik nemen (`gebruik` is `dicteren` of `bestanden`).
    Model { gebruik: String, model: String },
}

/// Wat er voor één punt gebeurt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Actie {
    /// De videokaart aanzetten.
    VideokaartAan,
    /// Het model staat er: meteen in gebruik nemen voor deze taak.
    Wisselen { gebruik: Gebruik, model: String },
    /// Het model moet nog binnen: downloaden, en daarna gebruiken voor deze taken.
    Downloaden { model: String, taken: Vec<Gebruik> },
}

/// Wat er gebeurd is, voor de zin "Overgenomen" in de interface.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Uitkomst {
    /// De videokaart staat nu aan.
    pub videokaart_aan: bool,
    /// Modellen die meteen in gebruik zijn genomen.
    pub gewisseld: Vec<ModelTaak>,
    /// Modellen die binnenkomen en daarna in gebruik komen.
    pub downloadt: Vec<ModelTaken>,
}

/// Een model dat voor één taak in gebruik is genomen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ModelTaak {
    pub gebruik: String,
    pub model: String,
}

/// Een model dat na de download voor deze taken in gebruik komt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ModelTaken {
    pub model: String,
    pub taken: Vec<String>,
}

/// Een punt waarvan de taak is gecontroleerd.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Punt {
    Videokaart,
    Model { gebruik: Gebruik, model: String },
}

/// Bepaalt wat er per punt gebeurt. `compleet` zegt of een model helemaal op de computer staat.
/// De volgorde is: eerst de downloads, dan het wisselen, dan de videokaart, zodat een download die
/// niet kan starten (geen ruimte) niets anders al heeft veranderd. Dubbele punten tellen één keer;
/// zonder punten is er niets te doen.
pub(crate) fn plan(punten: &[Punt], compleet: &dyn Fn(&str) -> bool) -> Vec<Actie> {
    let mut downloads: BTreeMap<String, Vec<Gebruik>> = BTreeMap::new();
    let mut wissels: Vec<(Gebruik, String)> = Vec::new();
    let mut kaart = false;
    for punt in punten {
        match punt {
            Punt::Videokaart => kaart = true,
            Punt::Model { gebruik, model } if compleet(model) => {
                if !wissels.iter().any(|(g, m)| g == gebruik && m == model) {
                    wissels.push((*gebruik, model.clone()));
                }
            }
            Punt::Model { gebruik, model } => {
                let taken = downloads.entry(model.clone()).or_default();
                if !taken.contains(gebruik) {
                    taken.push(*gebruik);
                }
            }
        }
    }
    let mut acties: Vec<Actie> = downloads
        .into_iter()
        .map(|(model, taken)| Actie::Downloaden { model, taken })
        .collect();
    acties.extend(
        wissels
            .into_iter()
            .map(|(gebruik, model)| Actie::Wisselen { gebruik, model }),
    );
    if kaart {
        acties.push(Actie::VideokaartAan);
    }
    acties
}

fn melding(tekst: &str) -> Foutmelding {
    Foutmelding::nieuw(tekst, Vervolg::Geen)
}

/// Controleert de punten van de interface: een bekende taak en een model dat daarvoor te kiezen is.
fn controleer(punten: &[AdviesPunt]) -> Result<Vec<Punt>, Foutmelding> {
    let catalogus = model::catalogus().map_err(|f| melding(&f.to_string()))?;
    punten
        .iter()
        .map(|punt| match punt {
            AdviesPunt::Videokaart => Ok(Punt::Videokaart),
            AdviesPunt::Model { gebruik, model } => {
                let gebruik = Gebruik::uit_sleutel(gebruik)
                    .ok_or_else(|| melding("Deze taak kent Mondig niet."))?;
                modelkeuze::controleer_keuze(&catalogus, gebruik, model)
                    .map_err(|fout| melding(&fout.to_string()))?;
                Ok(Punt::Model {
                    gebruik,
                    model: model.clone(),
                })
            }
        })
        .collect()
}

/// Neemt de aangevinkte punten van het advies over. Een download die niet kan starten (te weinig
/// ruimte) geeft een melding; wat daarvoor al startte, loopt door en krijgt zijn taken, maar het
/// wisselen en de videokaart zijn dan nog niet gedaan. Draait op een eigen thread: het kijkt op
/// schijf.
#[tauri::command]
pub(crate) async fn advies_overnemen(
    app: AppHandle,
    punten: Vec<AdviesPunt>,
) -> Result<Uitkomst, Foutmelding> {
    tauri::async_runtime::spawn_blocking(move || {
        let punten = controleer(&punten)?;
        let catalogus = model::catalogus().map_err(|f| melding(&f.to_string()))?;
        let acties = plan(&punten, &|id| modellen::is_compleet(&catalogus, id));
        controleer_ruimte(&app, &acties)?;
        voer_uit(&app, &acties)
    })
    .await
    .map_err(|_| melding("Er ging onverwacht iets mis. Probeer het opnieuw."))?
}

/// Kiest op Modellen (Wijzigen in een functiekaart) een model voor één taak. Staat het er, dan
/// meteen in gebruik; komt het al binnen, dan komt de taak bij de taken na die download; anders
/// gaat het in de rij met deze taak erbij. Een andere download die deze taak na afloop zou
/// overnemen, verliest hem: de nieuwste keuze wint.
#[tauri::command]
pub(crate) async fn model_kiezen(
    app: AppHandle,
    gebruik: String,
    model: String,
) -> Result<Uitkomst, Foutmelding> {
    tauri::async_runtime::spawn_blocking(move || {
        let punten = controleer(&[AdviesPunt::Model {
            gebruik,
            model: model.clone(),
        }])?;
        let Some(Punt::Model { gebruik, .. }) = punten.first() else {
            return Err(melding("Deze taak kent Mondig niet."));
        };
        let gebruik = *gebruik;
        trek_taak_in_bij_anderen(&app, gebruik, &model);
        let mut uitkomst = Uitkomst::default();
        download(&app, &model, &[gebruik], &mut uitkomst)?;
        modellen::meld_gewijzigd(&app);
        Ok(uitkomst)
    })
    .await
    .map_err(|_| melding("Er ging onverwacht iets mis. Probeer het opnieuw."))?
}

/// De taken na een download zonder `gebruik`.
fn zonder_taak(taken: &[Gebruik], gebruik: Gebruik) -> Vec<Gebruik> {
    taken.iter().copied().filter(|g| *g != gebruik).collect()
}

/// Haalt `gebruik` weg uit de taken na download van elk ander model dan `gekozen`.
fn trek_taak_in_bij_anderen(app: &AppHandle, gebruik: Gebruik, gekozen: &str) {
    let Ok(alle) = model::spraakmodellen() else {
        return;
    };
    for ander in alle.iter().filter(|m| m.id != gekozen) {
        let taken = modellen::onthouden_taken_na_download(app, ander);
        if taken.contains(&gebruik) {
            modellen::onthoud_taken_na_download(app, &ander.id, &zonder_taak(&taken, gebruik));
        }
    }
}

/// Hoeveel schijfruimte er te weinig is voor deze downloads samen, met dezelfde marge als de
/// downloads zelf. `vrij` is `None` als de vrije ruimte niet te meten is (dan weigert niets).
fn ruimte_tekort(vrij: Option<u64>, nodig: &[u64]) -> u64 {
    mondig_core::download::ruimtetekort(vrij, nodig.iter().sum())
}

/// Past alles wat gedownload moet worden samen op de schijf? Telt ook mee wat al in de rij staat
/// en nog binnen moet komen: elke download afzonderlijk past misschien, samen niet.
fn controleer_ruimte(app: &AppHandle, acties: &[Actie]) -> Result<(), Foutmelding> {
    let mut modellen_te_halen: Vec<String> = acties
        .iter()
        .filter_map(|a| match a {
            Actie::Downloaden { model, .. } => Some(model.clone()),
            _ => None,
        })
        .collect();
    if modellen_te_halen.is_empty() {
        return Ok(());
    }
    for id in downloads::actieve_modellen(app) {
        if !modellen_te_halen.contains(&id) {
            modellen_te_halen.push(id);
        }
    }
    let nodig: Vec<u64> = modellen_te_halen
        .iter()
        .map(|id| {
            mondig_core::download::overzicht_van_id(id)
                .map_or(0, |o| o.grootte_bytes.saturating_sub(o.aanwezig_bytes))
        })
        .collect();
    let vrij = model::modellen_map()
        .ok()
        .and_then(|map| mondig_core::download::vrije_ruimte(&map));
    if ruimte_tekort(vrij, &nodig) > 0 {
        let tekst = if modellen_te_halen.len() > 1 {
            "Er is niet genoeg ruimte voor beide modellen. Maak ruimte vrij of kies er één."
        } else {
            "Er is niet genoeg ruimte voor dit model. Maak ruimte vrij."
        };
        return Err(Foutmelding::nieuw(tekst, Vervolg::OpnieuwProberen));
    }
    Ok(())
}

/// Voert de acties uit, in de volgorde van [`plan`].
fn voer_uit(app: &AppHandle, acties: &[Actie]) -> Result<Uitkomst, Foutmelding> {
    let mut uitkomst = Uitkomst::default();
    for actie in acties {
        match actie {
            Actie::Downloaden { model, taken } => {
                download(app, model, taken, &mut uitkomst)?;
            }
            Actie::Wisselen { gebruik, model } => {
                modellen::zet_gebruik(app, *gebruik, model);
                uitkomst.gewisseld.push(ModelTaak {
                    gebruik: gebruik.sleutel().to_string(),
                    model: model.clone(),
                });
            }
            Actie::VideokaartAan => {
                // Zet ook de schakelaar in Instellingen om: Rust meldt `videokaart-gewijzigd`.
                crate::videokaart::videokaart_zet(app.clone(), true);
                uitkomst.videokaart_aan = true;
            }
        }
    }
    modellen::meld_gewijzigd(app);
    Ok(uitkomst)
}

/// De taken bij een model dat al in de rij staat, samen met de nieuwe: niets valt weg, niets staat
/// dubbel.
fn voeg_taken_samen(bestaand: &[Gebruik], nieuw: &[Gebruik]) -> Vec<Gebruik> {
    let mut alle = bestaand.to_vec();
    for gebruik in nieuw {
        if !alle.contains(gebruik) {
            alle.push(*gebruik);
        }
    }
    alle
}

/// Wat er gebeurt na een poging om een download te starten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NaStart {
    /// De download staat in de rij.
    Gestart,
    /// Starten kon niet omdat het model er intussen helemaal staat: meteen gebruiken.
    Wisselen,
    /// Starten mislukte om een andere reden: de melding van Rust gaat naar de gebruiker.
    Fout,
}

/// Een model dat tussen het plan en het starten compleet werd (een parallelle download, of de
/// gebruiker was zelf al klaar) is geen fout: dan wisselen we meteen.
fn na_start(gelukt: bool, compleet_nu: bool) -> NaStart {
    match (gelukt, compleet_nu) {
        (true, _) => NaStart::Gestart,
        (false, true) => NaStart::Wisselen,
        (false, false) => NaStart::Fout,
    }
}

/// Neemt een model dat er al staat meteen in gebruik voor deze taken en ruimt de bewaarde taken
/// na een download op (die hoeven niet meer).
fn wissel_nu(app: &AppHandle, id: &str, taken: &[Gebruik], uitkomst: &mut Uitkomst) {
    for gebruik in taken {
        modellen::zet_gebruik(app, *gebruik, id);
        uitkomst.gewisseld.push(ModelTaak {
            gebruik: gebruik.sleutel().to_string(),
            model: id.to_string(),
        });
    }
    modellen::onthoud_taken_na_download(app, id, &[]);
}

/// Zet een model in de downloadrij met de taken erbij. Staat het al in de rij, dan komen de
/// taken bij die al gekozen waren. Is het model er intussen helemaal, dan wordt het meteen
/// gebruikt.
fn download(
    app: &AppHandle,
    id: &str,
    taken: &[Gebruik],
    uitkomst: &mut Uitkomst,
) -> Result<(), Foutmelding> {
    let (model, map) = modellen::zoek_downloadbaar(id)?;
    if mondig_core::download::is_compleet(&model, &map) {
        wissel_nu(app, id, taken, uitkomst);
        return Ok(());
    }
    let (alle, taken_tekst) = if downloads::is_actief(app, id) {
        let alle = voeg_taken_samen(&modellen::onthouden_taken_na_download(app, &model), taken);
        modellen::onthoud_taken_na_download(app, id, &alle);
        (alle, None)
    } else {
        let tekst: Vec<String> = taken.iter().map(|g| g.sleutel().to_string()).collect();
        (taken.to_vec(), Some(tekst))
    };
    let uitslag = downloads::zet_in_rij(app, &app.state::<Downloads>(), id, taken_tekst);
    match na_start(
        uitslag.is_ok(),
        mondig_core::download::is_compleet(&model, &map),
    ) {
        NaStart::Gestart => {
            uitkomst.downloadt.push(ModelTaken {
                model: id.to_string(),
                taken: alle.iter().map(|g| g.sleutel().to_string()).collect(),
            });
            Ok(())
        }
        NaStart::Wisselen => {
            wissel_nu(app, id, &alle, uitkomst);
            Ok(())
        }
        NaStart::Fout => uitslag.map(|_| ()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ULTRA: &str = "parakeet-ultra-q8";
    const CANARY: &str = "canary-1b-v2-q8";

    fn model(gebruik: Gebruik, id: &str) -> Punt {
        Punt::Model {
            gebruik,
            model: id.to_string(),
        }
    }

    #[test]
    fn niets_aangevinkt_doet_niets() {
        assert!(plan(&[], &|_| true).is_empty());
    }

    #[test]
    fn de_videokaart_gaat_meteen_aan() {
        assert_eq!(
            plan(&[Punt::Videokaart], &|_| false),
            vec![Actie::VideokaartAan]
        );
    }

    #[test]
    fn een_model_dat_er_staat_wordt_meteen_gebruikt() {
        let acties = plan(&[model(Gebruik::Bestanden, CANARY)], &|id| id == CANARY);
        assert_eq!(
            acties,
            vec![Actie::Wisselen {
                gebruik: Gebruik::Bestanden,
                model: CANARY.to_string()
            }]
        );
    }

    #[test]
    fn een_model_dat_nog_binnen_moet_komen_wordt_gedownload_met_zijn_taak() {
        let acties = plan(&[model(Gebruik::Bestanden, CANARY)], &|_| false);
        assert_eq!(
            acties,
            vec![Actie::Downloaden {
                model: CANARY.to_string(),
                taken: vec![Gebruik::Bestanden]
            }]
        );
    }

    #[test]
    fn een_model_voor_beide_taken_wordt_een_keer_gedownload() {
        let acties = plan(
            &[
                model(Gebruik::Dicteren, ULTRA),
                model(Gebruik::Bestanden, ULTRA),
                model(Gebruik::Bestanden, ULTRA),
            ],
            &|_| false,
        );
        assert_eq!(
            acties,
            vec![Actie::Downloaden {
                model: ULTRA.to_string(),
                taken: vec![Gebruik::Dicteren, Gebruik::Bestanden]
            }]
        );
    }

    #[test]
    fn eerst_de_download_dan_het_wisselen_dan_de_videokaart() {
        let acties = plan(
            &[
                Punt::Videokaart,
                model(Gebruik::Dicteren, ULTRA),
                model(Gebruik::Bestanden, CANARY),
            ],
            &|id| id == ULTRA,
        );
        assert_eq!(
            acties,
            vec![
                Actie::Downloaden {
                    model: CANARY.to_string(),
                    taken: vec![Gebruik::Bestanden]
                },
                Actie::Wisselen {
                    gebruik: Gebruik::Dicteren,
                    model: ULTRA.to_string()
                },
                Actie::VideokaartAan,
            ]
        );
    }

    #[test]
    fn de_interface_stuurt_punten_als_json() {
        let punten: Vec<AdviesPunt> = serde_json::from_str(
            r#"[{"soort":"videokaart"},{"soort":"model","gebruik":"bestanden","model":"canary-1b-v2-q8"}]"#,
        )
        .expect("geldige punten");
        assert_eq!(
            punten,
            vec![
                AdviesPunt::Videokaart,
                AdviesPunt::Model {
                    gebruik: "bestanden".into(),
                    model: CANARY.into()
                }
            ]
        );
    }

    #[test]
    fn onbekende_taken_en_modellen_worden_geweigerd() {
        let fout = controleer(&[AdviesPunt::Model {
            gebruik: "zingen".into(),
            model: ULTRA.into(),
        }]);
        assert!(fout.is_err());
        let fout = controleer(&[AdviesPunt::Model {
            gebruik: "dicteren".into(),
            model: "bestaat-niet".into(),
        }]);
        assert!(fout.is_err());
        let goed = controleer(&[AdviesPunt::Model {
            gebruik: "dicteren".into(),
            model: ULTRA.into(),
        }]);
        assert!(goed.is_ok());
    }

    #[test]
    fn taken_van_een_lopende_download_en_nieuwe_komen_samen_zonder_dubbelen() {
        assert_eq!(
            voeg_taken_samen(
                &[Gebruik::Dicteren],
                &[Gebruik::Bestanden, Gebruik::Dicteren]
            ),
            vec![Gebruik::Dicteren, Gebruik::Bestanden]
        );
        assert_eq!(
            voeg_taken_samen(&[], &[Gebruik::Bestanden]),
            vec![Gebruik::Bestanden]
        );
    }

    #[test]
    fn een_model_dat_intussen_compleet_is_wordt_gewisseld_in_plaats_van_een_fout() {
        assert_eq!(na_start(true, false), NaStart::Gestart);
        assert_eq!(na_start(true, true), NaStart::Gestart);
        assert_eq!(na_start(false, true), NaStart::Wisselen);
        assert_eq!(na_start(false, false), NaStart::Fout);
    }

    #[test]
    fn downloads_die_samen_niet_passen_geven_een_tekort() {
        const GB: u64 = 1_000_000_000;
        // Vrij 1,5 GB: Canary (1,1 GB) past alleen, met Ultra erbij (0,7 GB) niet.
        assert_eq!(ruimte_tekort(Some(3 * GB / 2), &[1_100_000_000]), 0);
        assert!(ruimte_tekort(Some(3 * GB / 2), &[1_100_000_000, 700_000_000]) > 0);
        // Niet te meten: Mondig weigert niets.
        assert_eq!(ruimte_tekort(None, &[1_100_000_000, 700_000_000]), 0);
    }

    #[test]
    fn een_nieuwe_keuze_trekt_de_taak_in_bij_een_andere_download() {
        let beide = [Gebruik::Dicteren, Gebruik::Bestanden];
        assert_eq!(
            zonder_taak(&beide, Gebruik::Bestanden),
            vec![Gebruik::Dicteren]
        );
        assert_eq!(
            zonder_taak(&[Gebruik::Bestanden], Gebruik::Bestanden),
            Vec::<Gebruik>::new()
        );
        // Een taak die er niet bij stond, verandert niets.
        assert_eq!(
            zonder_taak(&[Gebruik::Dicteren], Gebruik::Bestanden),
            vec![Gebruik::Dicteren]
        );
    }
}
