//! De wachtrij met modeldownloads voor de app. De rij zelf staat in
//! `mondig_core::downloadrij` (na elkaar, nooit twee keer hetzelfde model); de download zelf in
//! `mondig_core::download` (hervatten, nieuwe pogingen, controle). Dit bestand verbindt ze met de
//! interface:
//!
//! - commando `model_download_start` (`model`, `daarna`): zet een model in de rij en geeft het
//!   nummer van het item. Staat het model er al in, dan komt er niets bij en is het antwoord
//!   het bestaande nummer (een dubbele klik start niets dubbel);
//! - commando `download_stop` (`model`, optioneel): stopt dat model, of zonder model het model
//!   dat nu binnenkomt. Wat binnen is, blijft staan;
//! - commando `downloads_lijst`: de hele rij met zijn volgnummer (`{ versie, items }`);
//! - commando `download_lopend`: de download die nu binnenkomt (voor oudere schermen);
//! - event `downloads-gewijzigd`: de hele rij met een oplopend volgnummer bij elke
//!   toestandswijziging (de interface negeert een oudere stand), met daarna
//!   `modellen-gewijzigd`;
//! - event `download-voortgang`: `{ id, model, binnen, totaal }`, hooguit vier keer per seconde;
//! - event `taak`: elke [`Gebeurtenis`] van een download, zoals bij de andere taken;
//! - store `downloads_onderbroken`: de modellen die wachtten of liepen. Na een herstart staan
//!   ze als "onderbroken" in de rij, met hun deelbestand, zodat de gebruiker ze kan hervatten.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de argumenten per waarde mee; een verwijzing kan niet"
)]

use mondig_core::download::{self, ModelStatus, Opties, Voortgang};
use mondig_core::downloadrij::{Downloadrij, ItemId, Opdracht, RijStand, RijToestand, Toevoeging};
use mondig_core::logboek::{self, Gebeurtenis as LogGebeurtenis, ModelNaam};
use mondig_core::model;
use mondig_core::probleem::Probleem;
use mondig_core::taak::{Foutmelding, Gebeurtenis, Soort, TaakId, Toestand, Vervolg};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::modellen;
use crate::sneltoets::{lees_instelling, schrijf_instelling, wis_instelling};

pub(crate) const SLEUTEL_ONDERBROKEN: &str = "downloads_onderbroken";

/// De rij van de app, als Tauri-staat.
pub(crate) struct Downloads(Downloadrij);

/// Wat `download-voortgang` meestuurt.
#[derive(Clone, Serialize)]
struct VoortgangEvent<'a> {
    id: ItemId,
    model: &'a str,
    binnen: u64,
    totaal: u64,
}

/// Maakt de rij en zet de downloads die bij de vorige keer onderbroken werden erin (op een
/// eigen thread: dat kijkt op schijf).
pub(crate) fn start(app: &AppHandle) {
    let werker_app = app.clone();
    let wijziging_app = app.clone();
    let voortgang_app = app.clone();
    let rij = Downloadrij::start(
        move |opdracht| voer_uit(&werker_app, opdracht),
        move |stand| bij_wijziging(&wijziging_app, stand),
        move |item| {
            if let Some(stand) = item.voortgang {
                let _ = voortgang_app.emit(
                    "download-voortgang",
                    VoortgangEvent {
                        id: item.id,
                        model: &item.model,
                        binnen: stand.binnen,
                        totaal: stand.totaal,
                    },
                );
            }
        },
    );
    app.manage(Downloads(rij));
    let app = app.clone();
    std::thread::spawn(move || {
        let onderbroken = onderbroken_van_vorige_keer(&app);
        app.state::<Downloads>().0.zet_onderbroken(&onderbroken);
    });
}

/// De modellen die bij de vorige keer wachtten of liepen, er nog niet helemaal staan en er wel
/// iets van op schijf hebben: een verwijderd model valt niets meer te hervatten.
fn onderbroken_van_vorige_keer(app: &AppHandle) -> Vec<String> {
    te_hervatten(
        &lees_instelling(app, SLEUTEL_ONDERBROKEN).unwrap_or_default(),
        &|id| {
            modellen::zoek_downloadbaar(id).is_ok_and(|(m, map)| {
                !download::is_compleet(&m, &map) && download::staat_op_schijf(&m, &map)
            })
        },
    )
}

/// De bewaarde lijst (`a,b`), zonder lege stukken en zonder wat niet meer te hervatten is.
fn te_hervatten(bewaard: &str, nog_te_hervatten: &dyn Fn(&str) -> bool) -> Vec<String> {
    bewaard
        .split(',')
        .filter(|id| !id.is_empty() && nog_te_hervatten(id))
        .map(str::to_string)
        .collect()
}

/// Elke wijziging van de rij: naar de interface, en onthouden wat er na een herstart als
/// "onderbroken" moet staan. Draait terwijl de rij vergrendeld is (zie `downloadrij`).
fn bij_wijziging(app: &AppHandle, stand: &RijStand) {
    let te_onthouden: Vec<&str> = stand
        .items
        .iter()
        .filter(|i| i.toestand.is_actief() || i.toestand == RijToestand::Onderbroken)
        .map(|i| i.model.as_str())
        .collect();
    if te_onthouden.is_empty() {
        wis_instelling(app, SLEUTEL_ONDERBROKEN);
    } else {
        schrijf_instelling(app, SLEUTEL_ONDERBROKEN, &te_onthouden.join(","));
    }
    let _ = app.emit("downloads-gewijzigd", stand);
    modellen::meld_gewijzigd(app);
}

/// Voert één download uit de rij uit. Is het model er intussen al, dan is het meteen klaar;
/// is er te weinig ruimte (een eerder model in de rij nam die in), dan een fout.
fn voer_uit(app: &AppHandle, opdracht: &Opdracht<'_>) -> Toestand {
    let Ok((model, map)) = modellen::zoek_downloadbaar(opdracht.model()) else {
        return Toestand::fout("Dit model kent Mondig niet.", Vervolg::Geen);
    };
    let overzicht = download::overzicht(&model, &map);
    if overzicht.status != ModelStatus::Compleet && overzicht.tekort_bytes > 0 {
        // Geen taakgebeurtenis: zelf in het logboek, met de vaste soort en zonder tekst.
        crate::logboek::mislukt(Soort::Download, Some(Probleem::SchijfVol));
        return Toestand::fout(
            "Er is te weinig ruimte op de schijf voor dit model. Maak ruimte vrij en probeer het opnieuw.",
            Vervolg::OpnieuwProberen,
        );
    }
    // Het logboek krijgt alleen de modelnaam uit de catalogus (`ModelNaam`), nooit vrije tekst.
    logboek::log(&LogGebeurtenis::DownloadGestart {
        model: ModelNaam::nieuw(&model.id),
    });
    download::voer_download_uit(
        &model,
        &map,
        TaakId(opdracht.id()),
        opdracht.annuleer(),
        &Opties::default(),
        |gebeurtenis: Gebeurtenis| {
            // Gelukt: eerst de keuzes zetten, dan pas melden, zodat de interface bij "klaar"
            // al het nieuwe model in gebruik ziet.
            if matches!(gebeurtenis.toestand, Toestand::Klaar) {
                let daarna = modellen::onthouden_taken_na_download(app, &model);
                modellen::gebruik_na_download(app, &model.id, &daarna);
                logboek::log(&LogGebeurtenis::DownloadKlaar {
                    model: ModelNaam::nieuw(&model.id),
                });
            }
            // Een mislukte download: soort taak en soort probleem in het logboek.
            crate::logboek::taak(&gebeurtenis);
            let _ = app.emit("taak", &gebeurtenis);
        },
        |melding| opdracht.meld(melding),
    )
}

fn rij(app: &AppHandle) -> Option<State<'_, Downloads>> {
    app.try_state::<Downloads>()
}

/// Het model dat nu binnenkomt, met hoe ver het is (in procenten).
pub(crate) fn lopende_download(app: &AppHandle) -> Option<(String, Option<u8>)> {
    let item = rij(app)?.0.bezig()?;
    Some((item.model, item.voortgang.map(|v| v.procent())))
}

/// Wacht dit model in de rij of komt het nu binnen?
pub(crate) fn is_actief(app: &AppHandle, model: &str) -> bool {
    rij(app).is_some_and(|r| r.0.is_actief(model))
}

/// De modellen die in de rij wachten of binnenkomen.
pub(crate) fn actieve_modellen(app: &AppHandle) -> Vec<String> {
    rij(app).map_or_else(Vec::new, |r| {
        r.0.lijst()
            .into_iter()
            .filter(|i| i.toestand.is_actief())
            .map(|i| i.model)
            .collect()
    })
}

/// Het model is van de computer gehaald: een afgelopen of onderbroken item ervan gaat uit de
/// rij (en daarmee uit de store), want er valt niets meer te hervatten.
pub(crate) fn vergeet(app: &AppHandle, model: &str) {
    if let Some(rij) = rij(app) {
        let _ = rij.0.vergeet(model);
    }
}

/// Wacht er een download of loopt er een? Voor de vraag bij het afsluiten.
pub(crate) fn iets_actief(app: &AppHandle) -> bool {
    rij(app).is_some_and(|r| r.0.lijst().iter().any(|i| i.toestand.is_actief()))
}

/// Het eerste model dat wacht of binnenkomt, met hoe ver het is: voor de vraag bij het afsluiten.
pub(crate) fn eerste_actieve(app: &AppHandle) -> Option<(String, Option<u8>)> {
    lopende_download(app).or_else(|| {
        rij(app)?
            .0
            .lijst()
            .into_iter()
            .find(|i| i.toestand.is_actief())
            .map(|i| (i.model, None))
    })
}

/// Mondig sluit af: de lopende download stopt, en wat liep of wachtte staat de volgende keer
/// als "onderbroken" klaar.
pub(crate) fn sluit(app: &AppHandle) {
    if let Some(rij) = rij(app) {
        rij.0.sluit();
    }
}

/// Zet een model in de rij. Is de download gelukt, dan krijgt het model de taken in `daarna`
/// ("Downloaden en gebruiken"); het oude model blijft tot dan in gebruik. Zonder `daarna`
/// ("Verder downloaden", "Hervatten") gelden de taken die bij de eerste start gekozen werden,
/// zodat hervatten de keuze niet stil verandert. Dat kan een spraakmodel zijn of het
/// sprekermodel (4b-D, zonder taken).
#[tauri::command]
pub(crate) fn model_download_start(
    app: AppHandle,
    staat: State<'_, Downloads>,
    model: String,
    daarna: Option<Vec<String>>,
) -> Result<ItemId, Foutmelding> {
    zet_in_rij(&app, &staat, &model, daarna)
}

/// De kern van `model_download_start`, ook voor wie een download start zonder interface
/// (Mondig opnieuw afstellen).
pub(crate) fn zet_in_rij(
    app: &AppHandle,
    staat: &Downloads,
    model: &str,
    daarna: Option<Vec<String>>,
) -> Result<ItemId, Foutmelding> {
    let (model, map) = modellen::zoek_downloadbaar(model)?;
    let nieuw_gekozen = daarna
        .map(|lijst| modellen::taken_na_download(&model, &lijst))
        .transpose()?;
    // Staat het al in de rij, dan verandert er niets, ook niet aan de gekozen taken.
    if let Some(item) = staat
        .0
        .lijst()
        .into_iter()
        .find(|i| i.model == model.id && i.toestand.is_actief())
    {
        return Ok(item.id);
    }
    let overzicht = download::overzicht(&model, &map);
    if overzicht.status == ModelStatus::Compleet {
        return Err(Foutmelding::nieuw(
            "Dit model staat al op je computer.",
            Vervolg::Geen,
        ));
    }
    if overzicht.tekort_bytes > 0 {
        return Err(Foutmelding::nieuw(
            "Er is te weinig ruimte op de schijf om dit model te downloaden.",
            Vervolg::OpnieuwProberen,
        ));
    }
    // Wie een model downloadt, kent het: de badge "Nieuw" mag weg.
    modellen::markeer_gezien(app, std::slice::from_ref(&model.id));
    if let Some(taken) = nieuw_gekozen {
        modellen::onthoud_taken_na_download(app, &model.id, &taken);
    }
    Ok(match staat.0.voeg_toe(&model.id) {
        Toevoeging::Nieuw(id) | Toevoeging::AlInDeRij(id) => id,
    })
}

/// Stopt de download van een model, of zonder model de download die nu binnenkomt. Het
/// deelbestand blijft staan, zodat "Verder downloaden" kan. Bij een onderbroken download is
/// dit "Niet hervatten": na een herstart staat hij er niet meer.
#[tauri::command]
pub(crate) fn download_stop(staat: State<'_, Downloads>, model: Option<String>) {
    let model = model.or_else(|| staat.0.bezig().map(|i| i.model));
    if let Some(model) = model {
        let _ = staat.0.stop_model(&model);
    }
}

/// De hele rij met zijn volgnummer, voor wie een scherm opent.
#[tauri::command]
pub(crate) fn downloads_lijst(staat: State<'_, Downloads>) -> RijStand {
    staat.0.stand()
}

/// Wat `download_lopend` teruggeeft.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LopendeDownloadInfo {
    taak: TaakId,
    /// Het id van het model dat binnenkomt.
    model: String,
    toestand: Toestand,
    voortgang: Option<Voortgang>,
}

/// De download die nu binnenkomt, als er een is.
#[tauri::command]
pub(crate) fn download_lopend(staat: State<'_, Downloads>) -> Option<LopendeDownloadInfo> {
    let item = staat.0.bezig()?;
    Some(LopendeDownloadInfo {
        taak: TaakId(item.id),
        model: item.model,
        toestand: Toestand::verwerkt(item.voortgang.map(|v| v.procent())),
        voortgang: item.voortgang,
    })
}

/// Het model van een id, voor de vraag bij het afsluiten.
pub(crate) fn naam_van(id: &str) -> String {
    model::zoek(id).map_or_else(
        |_| id.to_string(),
        |m| crate::bestanden::korte_naam(&m.naam),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alleen_wat_nog_op_schijf_staat_is_te_hervatten() {
        // b is intussen verwijderd (of helemaal binnen): die staat na de herstart niet meer
        // als onderbroken in de rij.
        let op_schijf = |id: &str| id != "b";
        assert_eq!(te_hervatten("a,b,,c", &op_schijf), vec!["a", "c"]);
        assert!(te_hervatten("", &op_schijf).is_empty());
    }
}
