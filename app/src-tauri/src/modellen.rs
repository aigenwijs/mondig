//! De pagina Modellen (sprint 5-A, ADR 0014): welke spraakmodellen er zijn, welk model bij
//! welke taak hoort, downloaden, verwijderen. De regels staan in `mondig-core`
//! (`modelkeuze`, `download`); dit bestand bewaart de keuzes in de instellingen, bouwt het
//! overzicht voor de interface en laat de werkprocessen het goede model gebruiken.
//!
//! - event `modellen-gewijzigd` (zonder inhoud): de lijst, de keuzes of een download zijn
//!   veranderd en de interface haalt het overzicht opnieuw op;
//! - keuzes per taak staan in de store onder `model_dicteren` en `model_bestanden`; zonder
//!   keuze gelden de standaarden (zie `modelkeuze`);
//! - `modellen_gezien` onthoudt voor welke modellen de badge "Nieuw" al geweest is;
//! - `download_daarna_<model>` onthoudt voor welke taken een model na "Downloaden en
//!   gebruiken" in gebruik komt, zodat "Verder downloaden" (ook na een herstart) dezelfde
//!   keuze afmaakt en een gewone download de keuze niet stil verandert.
//!
//! De commando's die op schijf kijken of werkprocessen stoppen, draaien op een eigen thread
//! (`spawn_blocking`), zodat het venster niet blijft hangen.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de argumenten per waarde mee; een verwijzing kan niet"
)]

use std::path::PathBuf;

use mondig_core::afgeschaft;
use mondig_core::download::{self, ModelStatus};
use mondig_core::model::{self, Gebruik, Model, Soort};
use mondig_core::modelkeuze::{self, Keuzes};
use mondig_core::taak::{Foutmelding, Vervolg};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::modelgebruik::{GEBRUIK, VRIJ_GRENS};
use crate::sneltoets::{lees_instelling, schrijf_instelling, wis_instelling};

const SLEUTEL_DICTEREN: &str = "model_dicteren";
const SLEUTEL_BESTANDEN: &str = "model_bestanden";
pub(crate) const SLEUTEL_GEZIEN: &str = "modellen_gezien";
pub(crate) const SLEUTEL_DAARNA: &str = "download_daarna_";

fn melding(tekst: impl Into<String>) -> Foutmelding {
    Foutmelding::nieuw(tekst, Vervolg::Geen)
}

fn catalogus_fout(fout: &model::ModelFout) -> Foutmelding {
    melding(fout.to_string())
}

fn instelling_sleutel(gebruik: Gebruik) -> &'static str {
    match gebruik {
        Gebruik::Dicteren => SLEUTEL_DICTEREN,
        Gebruik::Bestanden => SLEUTEL_BESTANDEN,
    }
}

/// De bewaarde keuzes; wat ontbreekt, is `None` en krijgt de standaard.
pub(crate) fn lees_keuzes(app: &AppHandle) -> Keuzes {
    Keuzes {
        dicteren: lees_instelling(app, SLEUTEL_DICTEREN),
        bestanden: lees_instelling(app, SLEUTEL_BESTANDEN),
    }
}

/// Voert blokkerend werk (schijf, werkprocessen stoppen) uit op een eigen thread, zodat een
/// commando de hoofdthread niet vasthoudt.
async fn op_achtergrond<T: Send + 'static>(
    werk: impl FnOnce() -> Result<T, Foutmelding> + Send + 'static,
) -> Result<T, Foutmelding> {
    tauri::async_runtime::spawn_blocking(werk)
        .await
        .map_err(|_| melding("Er ging onverwacht iets mis. Probeer het opnieuw."))?
}

/// Staat dit model helemaal op de computer, met zijn hulpmodel? Kijkt alleen naar de
/// markering en de grootte van de bestanden: dit gebeurt bij elke druk op de sneltoets.
pub(crate) fn is_compleet(catalogus: &[Model], id: &str) -> bool {
    catalogus
        .iter()
        .find(|m| m.id == id)
        .and_then(|m| Some((m, model::modelmap(&m.id).ok()?)))
        .is_some_and(|(m, map)| download::is_compleet(m, &map))
}

/// Het model dat nu geldt voor een taak, met zijn map.
///
/// # Errors
///
/// Geeft een melding als de catalogus niet klopt of de gegevensmap ontbreekt.
pub(crate) fn geldend_voor(
    app: &AppHandle,
    gebruik: Gebruik,
) -> Result<(Model, PathBuf), Foutmelding> {
    let catalogus = model::catalogus().map_err(|f| catalogus_fout(&f))?;
    let id = modelkeuze::geldend_model(gebruik, &lees_keuzes(app), &catalogus, &|id| {
        is_compleet(&catalogus, id)
    });
    let model = catalogus
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| melding("Dit model kent Mondig niet."))?;
    let map = model::modelmap(&model.id).map_err(|f| catalogus_fout(&f))?;
    Ok((model, map))
}

/// Het model voor elke taak.
fn geldend_alle(app: &AppHandle) -> Result<Vec<(Gebruik, String)>, Foutmelding> {
    Gebruik::ALLE
        .into_iter()
        .map(|gebruik| Ok((gebruik, geldend_voor(app, gebruik)?.0.id)))
        .collect()
}

/// Zoekt een spraakmodel met zijn map.
///
/// # Errors
///
/// Geeft een melding als het id geen spraakmodel is.
pub(crate) fn zoek_spraakmodel(id: &str) -> Result<(Model, PathBuf), Foutmelding> {
    let model = model::zoek(id)
        .ok()
        .filter(Model::is_spraakmodel)
        .ok_or_else(|| melding("Dit model kent Mondig niet."))?;
    let map = model::modelmap(&model.id).map_err(|f| catalogus_fout(&f))?;
    Ok((model, map))
}

/// Zoekt een model dat de gebruiker zelf downloadt, met zijn map: een spraakmodel, of het
/// sprekermodel (4b-D). Dat laatste komt niet mee met een spraakmodel (139 MB, alleen voor wie
/// sprekers wil herkennen) en staat naast de spraakmodellen, zoals Silero.
///
/// # Errors
///
/// Geeft een melding als het id geen van beide is.
pub(crate) fn zoek_downloadbaar(id: &str) -> Result<(Model, PathBuf), Foutmelding> {
    let model = model::zoek(id)
        .ok()
        .filter(|m| m.is_spraakmodel() || m.rol == model::Rol::Sprekers)
        .ok_or_else(|| melding("Dit model kent Mondig niet."))?;
    let map = model::modelmap(&model.id).map_err(|f| catalogus_fout(&f))?;
    Ok((model, map))
}

/// Vertaalt de taken waarvoor een model na de download gebruikt moet worden, en controleert
/// dat het model daarvoor te kiezen is.
///
/// # Errors
///
/// Geeft een melding bij een onbekende taak of een model dat daarvoor niet te kiezen is.
pub(crate) fn taken_na_download(
    model: &Model,
    daarna: &[String],
) -> Result<Vec<Gebruik>, Foutmelding> {
    daarna
        .iter()
        .map(|sleutel| {
            let gebruik = Gebruik::uit_sleutel(sleutel)
                .ok_or_else(|| melding("Deze taak kent Mondig niet."))?;
            if model.kiesbaar_voor(gebruik) {
                Ok(gebruik)
            } else {
                Err(melding("Dit model is voor deze taak niet te kiezen."))
            }
        })
        .collect()
}

/// Zet een model in gebruik voor een taak en laat het werkproces overschakelen.
pub(crate) fn zet_gebruik(app: &AppHandle, gebruik: Gebruik, id: &str) {
    schrijf_instelling(app, instelling_sleutel(gebruik), id);
    match gebruik {
        Gebruik::Dicteren => crate::dicteerketen::ververs_beheerder(app),
        Gebruik::Bestanden => crate::bestanden::ververs_wachtrij(app),
    }
}

/// Na een geslaagde download: het model krijgt de taken die de gebruiker erbij koos.
pub(crate) fn gebruik_na_download(app: &AppHandle, id: &str, daarna: &[Gebruik]) {
    for gebruik in daarna {
        zet_gebruik(app, *gebruik, id);
    }
    wis_instelling(app, &format!("{SLEUTEL_DAARNA}{id}"));
}

/// Onthoudt voor welke taken dit model na de download in gebruik komt, zodat verder downloaden
/// dezelfde keuze afmaakt. Een gewone download (zonder taken) wist een oude keuze.
pub(crate) fn onthoud_taken_na_download(app: &AppHandle, id: &str, daarna: &[Gebruik]) {
    let sleutel = format!("{SLEUTEL_DAARNA}{id}");
    if daarna.is_empty() {
        wis_instelling(app, &sleutel);
    } else {
        let tekst: Vec<&str> = daarna.iter().map(|g| g.sleutel()).collect();
        schrijf_instelling(app, &sleutel, &tekst.join(","));
    }
}

/// De taken die bij de eerste download van dit model gekozen werden ("Downloaden en
/// gebruiken"); leeg als het een gewone download was. Een taak waarvoor het model niet (meer)
/// te kiezen is, valt weg.
pub(crate) fn onthouden_taken_na_download(app: &AppHandle, model: &Model) -> Vec<Gebruik> {
    lees_instelling(app, &format!("{SLEUTEL_DAARNA}{}", model.id))
        .map(|tekst| {
            tekst
                .split(',')
                .filter_map(Gebruik::uit_sleutel)
                .filter(|g| model.kiesbaar_voor(*g))
                .collect()
        })
        .unwrap_or_default()
}

/// Zegt de interface dat het overzicht veranderd is.
pub(crate) fn meld_gewijzigd(app: &AppHandle) {
    let _ = app.emit("modellen-gewijzigd", ());
}

// ---------------------------------------------------------------------------
// Afgeschafte modellen opruimen
// ---------------------------------------------------------------------------

/// Voor welke taken staat nog een afgeschaft model in de bewaarde keuzes? Dat model telt al
/// niet meer (`modelkeuze::geldend_model` valt terug op de standaard); de keuze zelf wissen we
/// bij het opruimen, zodat er niets achterblijft.
fn taken_met_afgeschafte_keuze(keuzes: &Keuzes) -> Vec<Gebruik> {
    Gebruik::ALLE
        .into_iter()
        .filter(|g| keuzes.van(*g).is_some_and(afgeschaft::is_afgeschaft))
        .collect()
}

/// Ruimt de mappen van afgeschafte modellen op uit `modellen_map`, voor zover geen motor ze
/// nog gebruikt volgens `register`. Zolang het model dat nu voor dicteren geldt niet compleet
/// op schijf staat (`vervanger_compleet`), blijft alles staan: anders kan de gebruiker zonder
/// internet niet meer dicteren.
fn ruim_op_in(
    modellen_map: &std::path::Path,
    register: &crate::modelgebruik::Modelgebruik,
    vervanger_compleet: bool,
) -> Vec<afgeschaft::Opruiming> {
    if !vervanger_compleet {
        return Vec::new();
    }
    afgeschaft::ruim_op(modellen_map, &|id| {
        !register.wacht_tot_vrij(id, std::time::Duration::ZERO)
    })
}

/// Eenmalig opruimen bij het starten (Parakeet v3 is vervallen op 1 november 2026): haalt de
/// bewaarde keuzes voor een afgeschaft model weg en verwijdert de modelmap, als geen motor
/// hem gebruikt en het huidige dicteermodel compleet op schijf staat (anders probeert de
/// volgende start het opnieuw). Blokkeert op schijf; roep dit aan vanaf een achtergrondthread.
///
/// De regels gaan naar stderr, zoals de andere meldingen bij het starten; Mondig heeft geen
/// logbestand en stuurt niets weg.
pub(crate) fn ruim_afgeschafte_modellen_op(app: &AppHandle) {
    for gebruik in taken_met_afgeschafte_keuze(&lees_keuzes(app)) {
        wis_instelling(app, instelling_sleutel(gebruik));
        eprintln!(
            "modellen: bewaarde keuze voor {} was een afgeschaft model, de standaard geldt weer",
            gebruik.sleutel()
        );
    }
    let Ok(map) = model::modellen_map() else {
        return;
    };
    let mut iets_gedaan = false;
    let vervanger_compleet = crate::dicteermodel_compleet(app);
    if !vervanger_compleet {
        eprintln!(
            "modellen: afgeschafte modellen blijven staan tot het dicteermodel gedownload is"
        );
    }
    for opruiming in ruim_op_in(&map, &GEBRUIK, vervanger_compleet) {
        iets_gedaan = true;
        wis_instelling(app, &format!("{SLEUTEL_DAARNA}{}", opruiming.id));
        match opruiming.uitkomst {
            Ok(bytes) => eprintln!(
                "modellen: afgeschaft model {} opgeruimd, {} MB vrijgemaakt",
                opruiming.id,
                bytes / 1_000_000
            ),
            Err(fout) => eprintln!(
                "modellen: afgeschaft model {} opruimen mislukt ({fout}), volgende start opnieuw",
                opruiming.id
            ),
        }
    }
    if iets_gedaan {
        meld_gewijzigd(app);
    }
}

// ---------------------------------------------------------------------------
// De badge "Nieuw"
// ---------------------------------------------------------------------------

fn lees_gezien(app: &AppHandle) -> Option<Vec<String>> {
    lees_instelling(app, SLEUTEL_GEZIEN).map(|tekst| {
        tekst
            .split(',')
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect()
    })
}

fn schrijf_gezien(app: &AppHandle, ids: &[String]) {
    schrijf_instelling(app, SLEUTEL_GEZIEN, &ids.join(","));
}

/// Alle spraakmodellen van de lijst en de modellen waarvan iets op schijf staat.
fn zichtbaar_en_op_schijf(catalogus: &[Model]) -> (Vec<String>, Vec<String>) {
    let mut zichtbaar = Vec::new();
    let mut op_schijf = Vec::new();
    for m in catalogus.iter().filter(|m| m.is_spraakmodel()) {
        let staat = model::modelmap(&m.id).is_ok_and(|map| download::staat_op_schijf(m, &map));
        if staat {
            op_schijf.push(m.id.clone());
        }
        zichtbaar.push(m.id.clone());
    }
    (zichtbaar, op_schijf)
}

/// Onthoudt dat de gebruiker deze modellen kent. Was er nog niets bewaard, dan begint de lijst
/// bij wat de gebruiker al kende (zie `modelkeuze::gezien_na`), zodat de andere modellen niet
/// ineens "nieuw" worden.
pub(crate) fn markeer_gezien(app: &AppHandle, ids: &[String]) {
    let Ok(catalogus) = model::catalogus() else {
        return;
    };
    let (zichtbaar, op_schijf) = zichtbaar_en_op_schijf(&catalogus);
    if let Some(lijst) =
        modelkeuze::gezien_na(&zichtbaar, lees_gezien(app).as_deref(), &op_schijf, ids)
    {
        schrijf_gezien(app, &lijst);
    }
}

// ---------------------------------------------------------------------------
// Het overzicht voor de interface
// ---------------------------------------------------------------------------

/// Eén model op de pagina.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelRegel {
    id: String,
    /// De naam zoals Mondig die toont, zonder de uitleg tussen haakjes.
    naam: String,
    /// Waar het model voor dient; de pagina deelt hierop in.
    soort: Soort,
    /// De rol (snel, nauwkeurig, extra nauwkeurig) waarmee het model buiten Modellen heet; `None`
    /// voor een model dat niet voor opnames en bestanden te kiezen is (het sprekermodel).
    rol: Option<model::Modelrol>,
    /// Nauwkeurigheid, 1 tot 5 (zie `Model::nauwkeurigheid`).
    nauwkeurigheid: u8,
    /// Snelheid, 1 tot 5 (zie `Model::snelheid`).
    snelheid: u8,
    /// Ondersteunde talen als ISO 639-codes.
    talen: Vec<String>,
    status: ModelStatus,
    /// Wat er te downloaden is (met het hulpmodel).
    grootte_bytes: u64,
    /// Wat er al op schijf staat, ook van een afgebroken download.
    aanwezig_bytes: u64,
    /// Hoeveel ruimte er te weinig is om te downloaden (met de marge); 0 als het past.
    tekort_bytes: u64,
    /// Taken waarvoor het model niet te kiezen is.
    niet_voor: Vec<Gebruik>,
    /// Voor welke taken het model nu in gebruik is.
    in_gebruik_voor: Vec<Gebruik>,
    /// Mag het model nu van de computer? Niet als het in gebruik is, nog binnenkomt of er
    /// niets van staat.
    kan_weg: bool,
    /// Heeft de gebruiker dit model nog niet zien staan?
    nieuw: bool,
    /// Van welke host het model komt.
    bron: String,
    /// Voor welke taken het model in gebruik komt als "Verder downloaden" afmaakt wat
    /// "Downloaden en gebruiken" begon; leeg bij een gewone download.
    daarna_bij_hervatten: Vec<Gebruik>,
}

/// De hele pagina: de modellen, wat er per taak geldt en wat er aan de gang is.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModellenOverzicht {
    modellen: Vec<ModelRegel>,
    /// Het model dat voor dicteren geldt.
    dicteren: String,
    /// Het model dat voor bestanden geldt.
    bestanden: String,
    /// Het model dat nu binnenkomt, als er een download loopt.
    downloadt: Option<String>,
    /// Wat alle modellen samen op de schijf innemen.
    totaal_bytes: u64,
    /// Vrije ruimte op de schijf met de modellen, als die te meten is.
    vrij_bytes: Option<u64>,
}

/// Bouwt het overzicht voor de pagina en de zijbalk.
#[tauri::command]
pub(crate) async fn modellen_overzicht(app: AppHandle) -> Result<ModellenOverzicht, Foutmelding> {
    op_achtergrond(move || overzicht_blokkerend(app)).await
}

fn overzicht_blokkerend(app: AppHandle) -> Result<ModellenOverzicht, Foutmelding> {
    let catalogus = model::catalogus().map_err(|f| catalogus_fout(&f))?;
    let geldend = geldend_alle(&app)?;
    let downloadt = crate::downloads::lopende_download(&app).map(|(id, _)| id);

    let mut regels = Vec::new();
    let mut totaal_bytes = 0;
    let mut op_schijf_ids: Vec<String> = Vec::new();
    let mut vrij_bytes = None;
    for model in &catalogus {
        let map = model::modelmap(&model.id).map_err(|f| catalogus_fout(&f))?;
        totaal_bytes += download::aanwezige_bytes(model, &map);
        // Het sprekermodel staat los op de pagina (4b-D); Silero is nooit zichtbaar.
        let sprekers = model.rol == model::Rol::Sprekers;
        if !model.is_spraakmodel() && !sprekers {
            continue;
        }
        let overzicht = download::overzicht(model, &map);
        vrij_bytes = vrij_bytes.or(overzicht.vrij_bytes);
        let op_schijf = overzicht.status != ModelStatus::Ontbreekt;
        if op_schijf {
            op_schijf_ids.push(model.id.clone());
        }
        let in_gebruik_voor = modelkeuze::gebruikt_voor(&model.id, &geldend);
        // Niet weg zolang het in de downloadrij wacht of binnenkomt.
        let kan_weg = op_schijf
            && in_gebruik_voor.is_empty()
            && !crate::downloads::is_actief(&app, &model.id);
        regels.push(ModelRegel {
            id: model.id.clone(),
            naam: crate::bestanden::korte_naam(&model.naam),
            soort: model.soort,
            rol: model::modelrol(&catalogus, &model.id),
            nauwkeurigheid: model.nauwkeurigheid.unwrap_or(1),
            snelheid: model.snelheid.unwrap_or(1),
            talen: model.talen.clone(),
            status: overzicht.status,
            grootte_bytes: overzicht.grootte_bytes,
            aanwezig_bytes: overzicht.aanwezig_bytes,
            tekort_bytes: overzicht.tekort_bytes,
            // Het sprekermodel schrijft niets uit: voor geen enkele taak te kiezen.
            niet_voor: if sprekers {
                Gebruik::ALLE.to_vec()
            } else {
                model.niet_voor.clone()
            },
            in_gebruik_voor,
            kan_weg,
            nieuw: false,
            bron: overzicht.bron,
            daarna_bij_hervatten: if overzicht.status == ModelStatus::Gedeeltelijk {
                onthouden_taken_na_download(&app, model)
            } else {
                Vec::new()
            },
        });
    }

    // "Nieuw": een eerste installatie kent alles al; zo'n eerste keer leggen we nu vast, anders
    // zou alles "nieuw" worden zodra de eerste download iets op schijf zet.
    // Het sprekermodel hoort daar niet bij: het is een extra, geen nieuw spraakmodel.
    let zichtbaar: Vec<String> = regels
        .iter()
        .filter(|r| r.soort != Soort::Sprekers)
        .map(|r| r.id.clone())
        .collect();
    let mut gezien = lees_gezien(&app);
    if gezien.is_none() && op_schijf_ids.is_empty() {
        schrijf_gezien(&app, &zichtbaar);
        gezien = Some(zichtbaar.clone());
    }
    let nieuw = modelkeuze::nieuwe_modellen(&zichtbaar, gezien.as_deref(), &op_schijf_ids);
    for regel in &mut regels {
        regel.nieuw = nieuw.contains(&regel.id);
    }

    let waarde = |gebruik: Gebruik| {
        geldend
            .iter()
            .find(|(g, _)| *g == gebruik)
            .map(|(_, id)| id.clone())
            .unwrap_or_default()
    };
    Ok(ModellenOverzicht {
        modellen: regels,
        dicteren: waarde(Gebruik::Dicteren),
        bestanden: waarde(Gebruik::Bestanden),
        downloadt,
        totaal_bytes,
        vrij_bytes,
    })
}

/// De gebruiker heeft de pagina geopend: wat nu in de lijst staat, is niet meer nieuw.
#[tauri::command]
pub(crate) async fn modellen_gezien(app: AppHandle) -> Result<(), Foutmelding> {
    op_achtergrond(move || {
        let catalogus = model::catalogus().map_err(|f| catalogus_fout(&f))?;
        let (zichtbaar, _) = zichtbaar_en_op_schijf(&catalogus);
        markeer_gezien(&app, &zichtbaar);
        meld_gewijzigd(&app);
        Ok(())
    })
    .await
}

// ---------------------------------------------------------------------------
// Een model in gebruik nemen en verwijderen
// ---------------------------------------------------------------------------

/// Zet een model in gebruik voor een taak. Het model moet helemaal op de computer staan; een
/// model dat nog binnen moet komen, krijgt zijn taak via `model_download_start`.
#[tauri::command]
pub(crate) async fn model_gebruik_zet(
    app: AppHandle,
    gebruik: String,
    model: String,
) -> Result<(), Foutmelding> {
    op_achtergrond(move || gebruik_zet_blokkerend(&app, &gebruik, &model)).await
}

fn gebruik_zet_blokkerend(app: &AppHandle, gebruik: &str, model: &str) -> Result<(), Foutmelding> {
    let gebruik =
        Gebruik::uit_sleutel(gebruik).ok_or_else(|| melding("Deze taak kent Mondig niet."))?;
    let catalogus = model::catalogus().map_err(|f| catalogus_fout(&f))?;
    modelkeuze::controleer_keuze(&catalogus, gebruik, model)
        .map_err(|fout| melding(fout.to_string()))?;
    if !is_compleet(&catalogus, model) {
        return Err(Foutmelding::nieuw(
            "Dit model staat nog niet helemaal op je computer. Download het eerst.",
            Vervolg::OpnieuwDownloaden,
        ));
    }
    zet_gebruik(app, gebruik, model);
    meld_gewijzigd(app);
    Ok(())
}

/// Haalt een model van de computer. Geeft het aantal vrijgemaakte bytes. Een model dat in
/// gebruik is of nog binnenkomt, gaat niet weg; de interface zet de knop dan ook uit, maar de
/// regel geldt hier.
#[tauri::command]
pub(crate) async fn model_verwijder(app: AppHandle, model: String) -> Result<u64, Foutmelding> {
    op_achtergrond(move || verwijder_blokkerend(&app, &model)).await
}

fn verwijder_blokkerend(app: &AppHandle, model: &str) -> Result<u64, Foutmelding> {
    // Een spraakmodel, of het sprekermodel (4b-D).
    if model == model::SPREKERMODEL {
        // Het sprekermodel draait alleen tijdens het uitschrijven van een bestand of vergadering:
        // dan houdt Windows het bestand vast.
        if crate::bestanden::is_bezig(app) || crate::vergadering::schrijft_iets_uit(app) {
            return Err(Foutmelding::nieuw(
                "Mondig is nog bezig met uitschrijven. Probeer het opnieuw als dat klaar is.",
                Vervolg::OpnieuwProberen,
            ));
        }
    } else {
        zoek_spraakmodel(model)?;
    }
    if crate::downloads::is_actief(app, model) {
        return Err(melding(
            "Dit model wordt nog gedownload. Stop eerst de download.",
        ));
    }
    if modelkeuze::mag_verwijderen(model, &geldend_alle(app)?).is_err() {
        return Err(melding(
            "Je gebruikt dit model nog. Kies eerst een ander model; anders kan Mondig niets uitschrijven.",
        ));
    }
    // De werkprocessen laten het model los, anders houdt Windows het bestand vast. Een motor
    // die net vervangen is, stopt misschien nog op de achtergrond: pas als geen enkele motor
    // het model meer gebruikt, begint het verwijderen.
    crate::dicteerketen::geef_model_vrij(app, model)?;
    crate::bestanden::geef_model_vrij(app, model)?;
    if !GEBRUIK.wacht_tot_vrij(model, VRIJ_GRENS) {
        return Err(Foutmelding::nieuw(
            "Mondig gebruikt dit model nog. Wacht tot het dictaat of het bestand klaar is en probeer het dan opnieuw.",
            Vervolg::OpnieuwProberen,
        ));
    }
    // Silero blijft staan als een model in de downloadrij (wachtend of lopend) het nodig heeft.
    // Pas nu bepalen: tijdens het wachten hierboven kan er een download bijgekomen zijn.
    let binnenkomend = crate::downloads::actieve_modellen(app);
    let vrij = download::verwijder(model, &binnenkomend)
        .map_err(|fout| Foutmelding::nieuw(fout.melding_voor_gebruiker(), fout.vervolg()));
    // Een onderbroken of mislukte download van dit model valt niet meer te hervatten.
    crate::downloads::vergeet(app, model);
    // Ook als het half lukte: de pagina moet de nieuwe stand zien.
    meld_gewijzigd(app);
    vrij
}

/// Houdt het afsluiten tegen om te vragen wat er met een lopende download moet gebeuren.
/// Geeft `true` als de app mag afsluiten.
pub(crate) fn mag_afsluiten(app: &AppHandle) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    let Some((id, procent)) = crate::downloads::eerste_actieve(app) else {
        return true;
    };
    let naam = crate::downloads::naam_van(&id);
    let hoever = procent.map_or_else(String::new, |p| format!(", bij {p} procent"));
    app.dialog()
        .message(format!(
            "{naam} wordt nog gedownload. Als je Mondig nu afsluit, stopt de download. Bij de volgende start kun je verdergaan waar hij bleef{hoever}."
        ))
        .title("Mondig afsluiten?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Toch afsluiten".to_string(),
            "Doorgaan met downloaden".to_string(),
        ))
        .blocking_show()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taken_na_download_weigert_wat_niet_kan() {
        let ultra = model::zoek("parakeet-ultra-q8").unwrap();
        let whisper = model::zoek("whisper-large-v3-turbo-q8").unwrap();
        let taken = |m: &Model, t: &[&str]| {
            taken_na_download(m, &t.iter().map(ToString::to_string).collect::<Vec<_>>())
        };
        assert_eq!(
            taken(&ultra, &["dicteren", "bestanden"]).unwrap(),
            vec![Gebruik::Dicteren, Gebruik::Bestanden]
        );
        assert_eq!(
            taken(&whisper, &["bestanden"]).unwrap(),
            vec![Gebruik::Bestanden]
        );
        // Whisper turbo is niet om mee te dicteren, ook niet na een download.
        assert!(taken(&whisper, &["dicteren"]).is_err());
        assert!(taken(&ultra, &["vergaderen"]).is_err());
        assert!(taken(&ultra, &[]).unwrap().is_empty());
    }

    #[test]
    fn het_sprekermodel_is_te_downloaden_maar_geen_spraakmodel() {
        assert!(zoek_downloadbaar(model::SPREKERMODEL).is_ok());
        assert!(zoek_spraakmodel(model::SPREKERMODEL).is_err());
        assert!(zoek_downloadbaar(model::STANDAARD_MODEL).is_ok());
        // Silero komt altijd mee en is niet los te downloaden.
        assert!(zoek_downloadbaar(model::SILERO_MODEL).is_err());
        // Taken na de download: het sprekermodel is voor geen taak te kiezen.
        let (sprekers, _) = zoek_downloadbaar(model::SPREKERMODEL).unwrap();
        assert!(taken_na_download(&sprekers, &[]).unwrap().is_empty());
        assert!(taken_na_download(&sprekers, &["bestanden".to_string()]).is_err());
    }

    #[test]
    fn alleen_spraakmodellen_zijn_te_zoeken() {
        assert!(zoek_spraakmodel("canary-1b-v2-q8").is_ok());
        assert!(zoek_spraakmodel(model::SILERO_MODEL).is_err());
        assert!(zoek_spraakmodel("bestaat-niet").is_err());
    }

    #[test]
    fn de_instellingssleutels_zijn_per_taak_verschillend() {
        assert_ne!(
            instelling_sleutel(Gebruik::Dicteren),
            instelling_sleutel(Gebruik::Bestanden)
        );
    }

    #[test]
    fn een_bewaarde_keuze_voor_een_afgeschaft_model_wordt_gevonden() {
        let mut keuzes = Keuzes::default();
        assert!(taken_met_afgeschafte_keuze(&keuzes).is_empty());
        keuzes.zet(Gebruik::Dicteren, "parakeet-ultra-q8");
        keuzes.zet(Gebruik::Bestanden, "parakeet-tdt-0.6b-v3-q8");
        assert_eq!(
            taken_met_afgeschafte_keuze(&keuzes),
            vec![Gebruik::Bestanden]
        );
        keuzes.zet(Gebruik::Dicteren, "parakeet-tdt-0.6b-v3-q8");
        assert_eq!(
            taken_met_afgeschafte_keuze(&keuzes),
            vec![Gebruik::Dicteren, Gebruik::Bestanden]
        );
    }

    #[test]
    fn opruimen_wacht_op_een_motor_die_het_afgeschafte_model_nog_vasthoudt() {
        use crate::modelgebruik::Gevolgd;
        use crate::modelgebruik::tests::{NepMotor, register};
        use std::time::Duration;

        const V3: &str = "parakeet-tdt-0.6b-v3-q8";
        let basis = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(basis.path().join(V3)).unwrap();
        std::fs::write(basis.path().join(V3).join("model.gguf"), b"data").unwrap();

        let register = register();
        let motor = Gevolgd::met_register(register, NepMotor::nieuw(Duration::ZERO), V3);
        // Een motor gebruikt het nog: niets weg, ook niet de volgende keer zolang hij draait.
        assert!(ruim_op_in(basis.path(), register, true).is_empty());
        assert!(basis.path().join(V3).exists());
        // Zonder compleet dicteermodel blijft het sowieso staan, ook als de motor vrij is.
        drop(motor);
        assert!(ruim_op_in(basis.path(), register, false).is_empty());
        assert!(basis.path().join(V3).exists());
        // Vrij: nu gaat het weg, één keer.
        assert_eq!(ruim_op_in(basis.path(), register, true).len(), 1);
        assert!(!basis.path().join(V3).exists());
        assert!(ruim_op_in(basis.path(), register, true).is_empty());
    }
}
