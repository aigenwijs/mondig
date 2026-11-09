//! Commando's van het instellingenscherm (sprint 3b, stap B2): de microfoon kiezen en
//! hoe lang het spraakmodel in het geheugen blijft. Sprint 3c voegt de
//! terugval "Alleen op het klembord zetten" toe.
//!
//! De webview krijgt geen rechten voor de store; alles loopt via deze commando's. De
//! keuzes worden direct bewaard en toegepast, er is geen "Opslaan".
//!
//! Sprint 4b-D voegt "Sprekers herkennen" toe (standaard aan), met de stand van het
//! sprekermodel erbij.

use std::sync::Arc;

use mondig_core::download::{self, ModelOverzicht};
use mondig_core::engine::beheer::{Motorbeheerder, Rusttijd};
use mondig_core::filter::{FilterInstellingen, FilterTaal};
use mondig_core::model::{self, Gebruik};
use mondig_core::opname::handy::HandyApparatuur;
use mondig_core::opname::vergadering::{BEWAAR_KEUZES_DAGEN, BEWAAR_STANDAARD_DAGEN};
use mondig_core::opname::{Apparatuur, opgeslagen_microfoon};
use mondig_core::taak::{Foutmelding, Vervolg};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_store::StoreExt;

use crate::dicteerketen::{DicteerStaat, slot};
use crate::sneltoets::{
    INSTELLINGEN, SLEUTEL_MICROFOON, lees_instelling, lees_microfoon, schrijf_instelling,
    wis_instelling,
};

/// Sleutel: hoe lang het spraakmodel in het geheugen blijft (zie `RUSTTIJD_KEUZES`).
const SLEUTEL_RUSTTIJD: &str = "rusttijd";
/// Sleutel: de terugval "Alleen op het klembord zetten". Staat er "aan", dan plakt Mondig
/// niet; ontbreekt hij, dan plakt Mondig (de standaard).
const SLEUTEL_ALLEEN_KLEMBORD: &str = "alleen_klembord";
const AAN: &str = "aan";
const UIT: &str = "uit";
/// Sleutel: start Mondig zonder hoofdvenster, alleen in de systeembalk? "aan" of "uit";
/// ontbreekt hij, dan heeft de gebruiker het nog nooit zelf ingesteld.
const SLEUTEL_VERBORGEN_STARTEN: &str = "verborgen_starten";
/// Sleutel: "Sprekers herkennen" bij vergaderingen (4b-D). Staat er "uit", dan niet;
/// ontbreekt hij, dan wel (de standaard).
const SLEUTEL_SPREKERS: &str = "sprekers_herkennen";
/// Sleutel: "Stopwoorden weghalen" (ADR 0028). Staat er "uit", dan blijven klanken als "ehm"
/// staan; ontbreekt hij, dan gaan ze weg (de standaard).
const SLEUTEL_STOPWOORDEN: &str = "stopwoorden_weghalen";
/// Sleutel: de snelheid waarmee het verslag afspeelt (V-1), in tienden: "10", "15" of "20".
const SLEUTEL_AFSPEELSNELHEID: &str = "afspeelsnelheid";
/// De keuzes voor de afspeelsnelheid, in tienden: 1, 1,5 en 2 keer. De interface kent dezelfde.
const AFSPEELSNELHEDEN: [u8; 3] = [10, 15, 20];
/// Sleutel: na hoeveel dagen Mondig vergaderopnames in de tijdelijke map wist (1, 7 of 30).
const SLEUTEL_BEWAAR_DAGEN: &str = "vergadering_bewaar_dagen";

// ---------------------------------------------------------------------------
// Opnames automatisch wissen
// ---------------------------------------------------------------------------

/// Wat de store over de bewaartermijn zegt.
#[derive(Debug, PartialEq, Eq)]
enum Gelezen {
    /// Nooit gekozen: er staat niets in de store.
    Ontbreekt,
    Tekst(String),
    /// De store is niet te lezen of de waarde is geen tekst.
    Onleesbaar,
}

/// De bewaartermijn in dagen. Nooit gekozen is de standaard (7). Een waarde die niet te lezen
/// of ongeldig is, geeft `None`: bij twijfel wist Mondig niets.
fn bewaar_dagen_uit(gelezen: &Gelezen) -> Option<u32> {
    match gelezen {
        Gelezen::Ontbreekt => Some(BEWAAR_STANDAARD_DAGEN),
        Gelezen::Tekst(tekst) => tekst
            .parse::<u32>()
            .ok()
            .filter(|dagen| BEWAAR_KEUZES_DAGEN.contains(dagen)),
        Gelezen::Onleesbaar => None,
    }
}

/// De bewaartermijn van vergaderopnames in dagen, of `None` als de instelling niet te lezen
/// of ongeldig is (zie [`bewaar_dagen_uit`]).
pub(crate) fn lees_bewaar_dagen(app: &AppHandle) -> Option<u32> {
    let Ok(store) = app.store(INSTELLINGEN) else {
        return None;
    };
    let gelezen = match store.get(SLEUTEL_BEWAAR_DAGEN) {
        None => Gelezen::Ontbreekt,
        Some(waarde) => waarde
            .as_str()
            .map_or(Gelezen::Onleesbaar, |t| Gelezen::Tekst(t.to_string())),
    };
    bewaar_dagen_uit(&gelezen)
}

fn bewaar_schrijffout() -> Foutmelding {
    Foutmelding::nieuw(
        "Mondig kon deze keuze niet bewaren. Controleer of er ruimte vrij is op je schijf en probeer het opnieuw.",
        Vervolg::OpnieuwProberen,
    )
}

/// De bewaarde bewaartermijn, voor het keuzemenu.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn bewaartermijn_instelling(app: AppHandle) -> Result<u32, Foutmelding> {
    lees_bewaar_dagen(&app).ok_or_else(|| {
        Foutmelding::nieuw(
            "Mondig kan deze instelling niet lezen. Kies een bewaartijd om hem te herstellen.",
            Vervolg::Geen,
        )
    })
}

/// Bewaart een andere bewaartermijn (1, 7 of 30 dagen). De lijst met opnames rekent er meteen
/// mee; het wissen volgt bij de volgende ronde.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de argumenten per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn bewaartermijn_zet(app: AppHandle, dagen: u32) -> Result<(), Foutmelding> {
    if !BEWAAR_KEUZES_DAGEN.contains(&dagen) {
        return Err(Foutmelding::nieuw(
            "Deze bewaartijd bestaat niet. Kies 1, 7 of 30 dagen.",
            Vervolg::Geen,
        ));
    }
    // Hier wordt een schrijffout wel doorgegeven: wie "Opgeslagen" ziet, moet erop kunnen
    // rekenen dat de keuze er ook na een herstart nog is.
    let store = app.store(INSTELLINGEN).map_err(|_| bewaar_schrijffout())?;
    let oud = store.get(SLEUTEL_BEWAAR_DAGEN);
    store.set(SLEUTEL_BEWAAR_DAGEN, dagen.to_string());
    if store.save().is_err() {
        match oud {
            Some(waarde) => store.set(SLEUTEL_BEWAAR_DAGEN, waarde),
            None => {
                store.delete(SLEUTEL_BEWAAR_DAGEN);
            }
        }
        return Err(bewaar_schrijffout());
    }
    crate::vergadering::bewaartermijn_gewijzigd(&app, dagen);
    Ok(())
}

// ---------------------------------------------------------------------------
// Alleen op het klembord zetten
// ---------------------------------------------------------------------------

/// Staat de terugval aan?
pub(crate) fn lees_alleen_klembord(app: &AppHandle) -> bool {
    lees_instelling(app, SLEUTEL_ALLEEN_KLEMBORD).as_deref() == Some(AAN)
}

/// De bewaarde stand van de terugval, voor de schakelaar.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn alleen_klembord_instelling(app: AppHandle) -> bool {
    lees_alleen_klembord(&app)
}

/// Bewaart de terugval; geldt vanaf het volgende dictaat.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn alleen_klembord_zet(app: AppHandle, aan: bool) {
    if aan {
        schrijf_instelling(&app, SLEUTEL_ALLEEN_KLEMBORD, AAN);
    } else {
        wis_instelling(&app, SLEUTEL_ALLEEN_KLEMBORD);
    }
}

// ---------------------------------------------------------------------------
// Sprekers herkennen (4b-D)
// ---------------------------------------------------------------------------

/// Herkent Mondig de sprekers in een vergadering? Standaard aan; zonder sprekermodel gebeurt
/// het toch niet (dan blijft alles zoals het was).
pub(crate) fn lees_sprekers_herkennen(app: &AppHandle) -> bool {
    lees_instelling(app, SLEUTEL_SPREKERS).as_deref() != Some(UIT)
}

/// De stand van "Sprekers herkennen" voor het scherm.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SprekersInstelling {
    /// Staat de schakelaar aan?
    aan: bool,
    /// Het sprekermodel: of het er is, hoe groot het is en wat er al staat. Downloaden gaat
    /// met `model_download_start` en het id hierin.
    model: ModelOverzicht,
}

/// De schakelaar "Sprekers herkennen" met de stand van het sprekermodel.
///
/// # Errors
///
/// Geeft een melding als de catalogus niet klopt of de gegevensmap ontbreekt.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn sprekers_instelling(app: AppHandle) -> Result<SprekersInstelling, Foutmelding> {
    let model = download::overzicht_van_id(model::SPREKERMODEL)
        .map_err(|f| Foutmelding::nieuw(f.to_string(), Vervolg::Geen))?;
    Ok(SprekersInstelling {
        aan: lees_sprekers_herkennen(&app),
        model,
    })
}

/// Bewaart "Sprekers herkennen"; geldt vanaf de volgende vergadering die uitgeschreven wordt.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn sprekers_zet(app: AppHandle, aan: bool) {
    if aan {
        wis_instelling(&app, SLEUTEL_SPREKERS);
    } else {
        schrijf_instelling(&app, SLEUTEL_SPREKERS, UIT);
    }
}

// ---------------------------------------------------------------------------
// Stopwoorden weghalen (ADR 0028)
// ---------------------------------------------------------------------------

/// Is "Stopwoorden weghalen" bewaard als uit? Standaard aan.
fn stopwoorden_aan(bewaard: Option<&str>) -> bool {
    bewaard != Some(UIT)
}

/// Staat "Stopwoorden weghalen" aan?
pub(crate) fn lees_stopwoorden(app: &AppHandle) -> bool {
    stopwoorden_aan(lees_instelling(app, SLEUTEL_STOPWOORDEN).as_deref())
}

/// Het filter met de schakelaar en de taal erin. De taal komt van het model dat uitschrijft.
fn filter_met(basis: FilterInstellingen, stopwoorden: bool, taalcode: &str) -> FilterInstellingen {
    basis
        .met_stopwoorden(stopwoorden)
        .met_taal(Some(FilterTaal::uit_code(taalcode)))
}

/// Het filter voor dictaten: de gesproken taal (`crate::taal`) en de stand van de schakelaar.
pub(crate) fn dicteer_filter(app: &AppHandle) -> FilterInstellingen {
    filter_met(
        FilterInstellingen::standaard(),
        lees_stopwoorden(app),
        crate::taal::gesproken_taal(app, Gebruik::Dicteren),
    )
}

/// Het filter voor bestanden en vergaderopnames, met dezelfde schakelaar.
pub(crate) fn bestand_filter(app: &AppHandle) -> FilterInstellingen {
    filter_met(
        FilterInstellingen::bestand(),
        lees_stopwoorden(app),
        crate::taal::gesproken_taal(app, Gebruik::Bestanden),
    )
}

/// De stand van de schakelaar "Stopwoorden weghalen", voor het scherm.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn stopwoorden_instelling(app: AppHandle) -> bool {
    lees_stopwoorden(&app)
}

/// Bewaart "Stopwoorden weghalen". Geldt vanaf het volgende dictaat en het volgende bestand;
/// de wachtrij van bestanden krijgt het nieuwe filter meteen, ook als ze al draait.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn stopwoorden_zet(app: AppHandle, aan: bool) {
    if aan {
        wis_instelling(&app, SLEUTEL_STOPWOORDEN);
    } else {
        schrijf_instelling(&app, SLEUTEL_STOPWOORDEN, UIT);
    }
    crate::bestanden::werk_bestandfilter_bij(&app);
}

// ---------------------------------------------------------------------------
// Afspeelsnelheid van het verslag (V-1)
// ---------------------------------------------------------------------------

/// De snelheid uit de store: een van [`AFSPEELSNELHEDEN`], anders de gewone snelheid (10).
fn afspeelsnelheid_uit(waarde: Option<&str>) -> u8 {
    waarde
        .and_then(|w| w.parse::<u8>().ok())
        .filter(|s| AFSPEELSNELHEDEN.contains(s))
        .unwrap_or(AFSPEELSNELHEDEN[0])
}

/// De bewaarde afspeelsnelheid in tienden (10, 15 of 20), voor het verslag bij Opnemen en Bestanden.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn afspeelsnelheid_instelling(app: AppHandle) -> u8 {
    afspeelsnelheid_uit(lees_instelling(&app, SLEUTEL_AFSPEELSNELHEID).as_deref())
}

/// Bewaart de afspeelsnelheid in tienden. Een andere waarde dan 10, 15 of 20 wordt genegeerd.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn afspeelsnelheid_zet(app: AppHandle, snelheid: u8) {
    if AFSPEELSNELHEDEN.contains(&snelheid) {
        schrijf_instelling(&app, SLEUTEL_AFSPEELSNELHEID, &snelheid.to_string());
    }
}

// ---------------------------------------------------------------------------
// Starten bij aanmelden
// ---------------------------------------------------------------------------

/// Start Mondig bij het aanmelden? De stand staat in Windows zelf (een regel onder `Run` in
/// het register van de gebruiker), niet in de store: zo kan hij nooit afwijken van wat
/// Windows echt doet, ook niet als iemand hem in Taakbeheer uitzet.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn autostart_instelling(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Zet starten bij aanmelden aan of uit; geldt meteen. Zet de gebruiker het aan en heeft hij
/// "Verborgen starten" nog nooit zelf ingesteld, dan gaat die mee aan: wie Mondig bij het
/// aanmelden laat starten, wil meestal geen venster midden in zijn werk.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn autostart_zet(app: AppHandle, aan: bool) -> Result<(), Foutmelding> {
    let starter = app.autolaunch();
    let uitslag = if aan {
        starter.enable()
    } else {
        starter.disable()
    };
    uitslag.map_err(|_| {
        Foutmelding::nieuw(
            "Mondig kon dit niet in Windows instellen. Probeer het opnieuw.",
            Vervolg::OpnieuwProberen,
        )
    })?;
    if aan
        && let Some(keuze) =
            verborgen_bij_autostart_aan(lees_instelling(&app, SLEUTEL_VERBORGEN_STARTEN).as_deref())
    {
        schrijf_instelling(&app, SLEUTEL_VERBORGEN_STARTEN, keuze);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Verborgen starten
// ---------------------------------------------------------------------------

/// Wat "Verborgen starten" wordt als de gebruiker starten bij aanmelden aanzet: `Some("aan")`
/// als hij het nog nooit zelf instelde, anders `None` (zijn eigen keuze blijft).
fn verborgen_bij_autostart_aan(bewaard: Option<&str>) -> Option<&'static str> {
    match bewaard {
        None => Some(AAN),
        Some(_) => None,
    }
}

/// De keuze voor "Verborgen starten": de bewaarde waarde, anders `standaard`.
pub(crate) fn verborgen_keuze(bewaard: Option<&str>, standaard: bool) -> bool {
    match bewaard {
        Some(waarde) => waarde == AAN,
        None => standaard,
    }
}

/// Start Mondig zonder hoofdvenster? Alleen als Windows Mondig bij het aanmelden start en de
/// keuze aan staat. Start de gebruiker Mondig zelf, dan opent het venster altijd: anders lijkt
/// er niets te gebeuren. Zonder bewaarde keuze geldt het oude gedrag van starten bij
/// aanmelden (verborgen).
pub(crate) fn verborgen_effectief(bewaard: Option<&str>, via_autostart: bool) -> bool {
    via_autostart && verborgen_keuze(bewaard, true)
}

/// De bewaarde keuze voor "Verborgen starten", voor `start` in lib.rs.
pub(crate) fn lees_verborgen_keuze(app: &AppHandle) -> Option<String> {
    lees_instelling(app, SLEUTEL_VERBORGEN_STARTEN)
}

/// De stand van de schakelaar "Verborgen starten".
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn verborgen_starten_instelling(app: AppHandle) -> bool {
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    verborgen_keuze(
        lees_instelling(&app, SLEUTEL_VERBORGEN_STARTEN).as_deref(),
        autostart,
    )
}

/// Bewaart de keuze. Ook "uit" wordt bewaard (niet gewist): zo weet Mondig dat de gebruiker
/// het zelf koos en zet starten bij aanmelden het later niet alsnog aan.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn verborgen_starten_zet(app: AppHandle, aan: bool) {
    schrijf_instelling(&app, SLEUTEL_VERBORGEN_STARTEN, if aan { AAN } else { UIT });
}

// ---------------------------------------------------------------------------
// Spraakmodel in het geheugen houden
// ---------------------------------------------------------------------------

/// De keuzes die de gebruiker ziet: 5 minuten (standaard), 15 minuten, 1 uur en altijd.
/// Een oudere bewaarde waarde (meteen, 2 of 10 minuten) valt op de dichtstbijzijnde keuze
/// die minstens zo lang houdt, zodat het model nooit korter blijft dan de gebruiker koos.
/// `None` voor een onbekende sleutel.
fn rusttijd_keuze(bewaard: &str) -> Option<&'static str> {
    match bewaard {
        "meteen" | "2m" | "5m" => Some("5m"),
        "10m" | "15m" => Some("15m"),
        "1u" => Some("1u"),
        "nooit" => Some("nooit"),
        _ => None,
    }
}

/// De rusttijd bij een bewaarde sleutel; de standaard (5 minuten) als hij onbekend is.
fn rusttijd_uit_bewaard(bewaard: Option<&str>) -> Rusttijd {
    bewaard
        .and_then(rusttijd_keuze)
        .and_then(Rusttijd::uit_sleutel)
        .unwrap_or_default()
}

/// De bewaarde rusttijd; de standaard als er niets bewaard is of de waarde onbekend is.
pub(crate) fn lees_rusttijd(app: &AppHandle) -> Rusttijd {
    rusttijd_uit_bewaard(lees_instelling(app, SLEUTEL_RUSTTIJD).as_deref())
}

/// De sleutel van de bewaarde rusttijd, voor het keuzemenu.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn rusttijd_instelling(app: AppHandle) -> String {
    lees_instelling(&app, SLEUTEL_RUSTTIJD)
        .as_deref()
        .and_then(rusttijd_keuze)
        .unwrap_or("5m")
        .to_string()
}

/// Bewaart een andere rusttijd en past hem meteen toe op een draaiende beheerder, zonder
/// herstart. Het geldt voor het dicteermodel; het bestandsmodel laadt alleen tijdens het
/// uitschrijven (ADR 0012). "Altijd" laadt het model nu; als er nog geen beheerder is, maakt dit er
/// een (alleen als het model compleet is).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de argumenten per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn rusttijd_zet(
    app: AppHandle,
    staat: State<'_, DicteerStaat>,
    sleutel: String,
) -> Result<(), Foutmelding> {
    let Some(keuze) = rusttijd_keuze(&sleutel) else {
        return Err(Foutmelding::nieuw(
            "Deze keuze bestaat niet.",
            Vervolg::Geen,
        ));
    };
    let rusttijd = rusttijd_uit_bewaard(Some(keuze));
    schrijf_instelling(&app, SLEUTEL_RUSTTIJD, keuze);
    let bestaande: Option<Arc<Motorbeheerder>> = slot(&staat.beheerder).clone();
    match bestaande {
        Some(beheerder) => {
            let _ = beheerder.zet_rusttijd(rusttijd);
        }
        None if rusttijd == Rusttijd::Nooit => {
            // De beheerder leest de zojuist bewaarde rusttijd en laadt meteen.
            std::thread::spawn(move || {
                let _ = crate::dicteerketen::beheerder_als_mogelijk(&app);
            });
        }
        None => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Microfoon kiezen
// ---------------------------------------------------------------------------

/// Een microfoon in de keuzelijst.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MicrofoonInfo {
    id: String,
    naam: String,
}

/// De microfoons van nu, met de bewaarde keuze.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MicrofoonLijst {
    microfoons: Vec<MicrofoonInfo>,
    /// De naam van de microfoon die het systeem nu als standaard kiest.
    standaard_naam: Option<String>,
    /// De bewaarde keuze (id), als die er is.
    opgeslagen: Option<String>,
    /// Is de bewaarde microfoon nu aangesloten? Zo niet, dan geldt de standaard.
    opgeslagen_aanwezig: bool,
}

/// De lijst van nu, vers bij elke aanroep (een headset kan intussen aangesloten zijn).
#[tauri::command]
pub(crate) async fn microfoon_lijst(app: AppHandle) -> Result<MicrofoonLijst, Foutmelding> {
    tauri::async_runtime::spawn_blocking(move || {
        let lijst = HandyApparatuur
            .microfoons()
            .map_err(|fout| Foutmelding::from(&fout))?;
        let opgeslagen = lees_microfoon(&app);
        let opgeslagen_aanwezig = opgeslagen_microfoon(opgeslagen.as_deref(), &lijst).is_some();
        Ok(MicrofoonLijst {
            standaard_naam: lijst.iter().find(|m| m.standaard).map(|m| m.naam.clone()),
            microfoons: lijst
                .into_iter()
                .map(|m| MicrofoonInfo {
                    id: m.id,
                    naam: m.naam,
                })
                .collect(),
            opgeslagen,
            opgeslagen_aanwezig,
        })
    })
    .await
    .map_err(|_| {
        Foutmelding::nieuw(
            "Het zoeken naar microfoons stopte onverwacht.",
            Vervolg::OpnieuwProberen,
        )
    })?
}

/// Bewaart de gekozen microfoon, of wist de keuze (`None`: de standaard van het systeem).
/// Geldt vanaf het volgende dictaat. Een bewaarde microfoon die er later niet meer is,
/// wordt bij een dictaat gewoon overgeslagen ten faveure van de standaard.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de argumenten per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn microfoon_kies(app: AppHandle, id: Option<String>) {
    match id {
        Some(id) => schrijf_instelling(&app, SLEUTEL_MICROFOON, &id),
        None => wis_instelling(&app, SLEUTEL_MICROFOON),
    }
}

// ---------------------------------------------------------------------------
// Mondig terugzetten
// ---------------------------------------------------------------------------

/// Sleutels die geen instelling zijn maar wat Mondig onthoudt, en dus blijven staan: welke
/// modellen de gebruiker al zag (anders krijgen ze opnieuw de badge "Nieuw"), voor welke taken
/// een lopende download straks in gebruik komt, en voor welke versie "Wat is er nieuw" al
/// getoond is.
const BLIJFT_STAAN: [&str; 4] = [
    crate::modellen::SLEUTEL_GEZIEN,
    // De downloads die na een herstart als "onderbroken" klaarstaan: dat is geen instelling,
    // en de rij in het geheugen loopt gewoon door.
    crate::downloads::SLEUTEL_ONDERBROKEN,
    crate::versie::SLEUTEL_LAATST_GEZIEN,
    // De bewaartermijn van opnames: terugzetten naar 7 dagen zou opnames van 8 tot 30 dagen oud
    // een dag later laten wissen. Wat de gebruiker bewust langer bewaarde, blijft zo staan.
    SLEUTEL_BEWAAR_DAGEN,
];

/// Welke sleutels van de store worden gewist bij "Mondig terugzetten": alles behalve wat
/// [`BLIJFT_STAAN`] noemt. De modellen zelf staan op schijf, niet in de store.
fn sleutels_om_te_wissen(alle: Vec<String>) -> Vec<String> {
    alle.into_iter()
        .filter(|sleutel| {
            !BLIJFT_STAAN.contains(&sleutel.as_str())
                && !sleutel.starts_with(crate::modellen::SLEUTEL_DAARNA)
        })
        .collect()
}

/// Wat de interface na het terugzetten wil weten.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TerugzetUitkomst {
    /// Was de standaardsneltoets bezet? Dan werkt de toets van voor het terugzetten nog.
    sneltoets_bezet: bool,
}

/// Zet alle instellingen terug naar de standaard, zonder modellen te wissen: de store wordt
/// leeggemaakt (behalve [`BLIJFT_STAAN`]), de sneltoets, starten bij aanmelden, de videokaart
/// en de rusttijd gaan meteen naar de standaard. Hoe de gebruiker erover denkt, vraagt de
/// interface eerst met een bevestiging.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn instellingen_terugzetten(app: AppHandle) -> Result<TerugzetUitkomst, Foutmelding> {
    let store = app.store(INSTELLINGEN).map_err(|_| bewaar_schrijffout())?;
    // Eerst een kopie van wat weg gaat: lukt het opslaan niet, dan staat alles weer zoals het was
    // en draait de app niet half teruggezet door tot een latere schrijfactie het vastlegt.
    let oude_toets = lees_instelling(&app, crate::sneltoets::SLEUTEL_SNELTOETS);
    let weg = sleutels_om_te_wissen(store.keys());
    let kopie: Vec<_> = weg
        .iter()
        .filter_map(|sleutel| store.get(sleutel).map(|waarde| (sleutel.clone(), waarde)))
        .collect();
    for sleutel in &weg {
        store.delete(sleutel);
    }
    if store.save().is_err() {
        for (sleutel, waarde) in kopie {
            store.set(sleutel, waarde);
        }
        return Err(bewaar_schrijffout());
    }
    drop(store);

    // Opschonen staat weer uit (de keuze is gewist): eerst de tweede sneltoets weg, anders weigert
    // de gewone standaard als die gelijk is aan de tweede.
    crate::sneltoets::werk_opschoontoets_bij(&app, mondig_core::opschonen::Stand::Uit);
    // De sneltoets staat in Windows, niet alleen in de store: zet de standaard weer vast.
    let sneltoets_bezet = crate::sneltoets::wijzig(
        &app,
        &app.state::<crate::sneltoets::SneltoetsStand>(),
        mondig_core::sneltoets::standaard_sneltoets(mondig_core::sneltoets::Platform::huidig()),
        crate::sneltoets::Soort::Dicteren,
    )
    .is_err();
    if sneltoets_bezet && let Some(oud) = oude_toets {
        // De standaardtoets is bezet: de oude blijft actief, dus ook in de instellingen staan.
        schrijf_instelling(&app, crate::sneltoets::SLEUTEL_SNELTOETS, &oud);
    }
    // Starten bij aanmelden is uit; lukt dat niet, dan blijft het zoals het was.
    let _ = app.autolaunch().disable();
    // De videokaart uit en de motoren opnieuw laten beginnen.
    crate::videokaart::videokaart_zet(app.clone(), false);
    // Het model in het geheugen: terug naar de standaardtijd.
    let bestaande: Option<Arc<Motorbeheerder>> =
        slot(&app.state::<DicteerStaat>().beheerder).clone();
    if let Some(beheerder) = bestaande {
        let _ = beheerder.zet_rusttijd(rusttijd_uit_bewaard(None));
    }
    // Stopwoorden weghalen staat weer aan (de standaard); een draaiende wachtrij volgt meteen.
    crate::bestanden::werk_bestandfilter_bij(&app);
    // De modellen staan nog op de computer; alleen de keuze per taak is terug naar de standaard.
    crate::modellen::meld_gewijzigd(&app);
    mondig_core::logboek::log(&mondig_core::logboek::Gebeurtenis::InstellingenTerugGezet);
    Ok(TerugzetUitkomst { sneltoets_bezet })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn stopwoorden_staan_standaard_aan_en_alleen_uit_is_uit() {
        assert!(stopwoorden_aan(None));
        assert!(stopwoorden_aan(Some("aan")));
        assert!(stopwoorden_aan(Some("onzin")));
        assert!(!stopwoorden_aan(Some("uit")));
    }

    #[test]
    fn het_filter_krijgt_de_schakelaar_en_de_taal_mee() {
        let aan = filter_met(FilterInstellingen::standaard(), true, "nl");
        assert!(aan.stopwoorden);
        assert_eq!(aan.taal, Some(FilterTaal::Nl));
        let uit = filter_met(FilterInstellingen::bestand(), false, "en");
        assert!(!uit.stopwoorden);
        assert_eq!(uit.taal, Some(FilterTaal::En));
        // De drempels van het bestandsfilter blijven.
        assert!((uit.spraak_rms - FilterInstellingen::bestand().spraak_rms).abs() < f32::EPSILON);
    }

    #[test]
    fn de_afspeelsnelheid_is_een_van_de_drie_keuzes_en_anders_gewoon() {
        assert_eq!(afspeelsnelheid_uit(None), 10);
        assert_eq!(afspeelsnelheid_uit(Some("15")), 15);
        assert_eq!(afspeelsnelheid_uit(Some("20")), 20);
        for onbekend in ["", "x", "12", "30", "255", "-10", "1.5"] {
            assert_eq!(afspeelsnelheid_uit(Some(onbekend)), 10, "{onbekend}");
        }
    }

    #[test]
    fn autostart_aanzetten_zet_verborgen_starten_mee_aan_als_het_nog_nooit_is_ingesteld() {
        assert_eq!(verborgen_bij_autostart_aan(None), Some("aan"));
    }

    #[test]
    fn autostart_aanzetten_laat_een_eigen_keuze_met_rust() {
        assert_eq!(verborgen_bij_autostart_aan(Some("uit")), None);
        assert_eq!(verborgen_bij_autostart_aan(Some("aan")), None);
    }

    #[test]
    fn een_bewaarde_keuze_voor_verborgen_starten_wint_bij_aanmelden() {
        assert!(verborgen_effectief(Some("aan"), true));
        assert!(!verborgen_effectief(Some("uit"), true));
    }

    #[test]
    fn zelf_starten_opent_altijd_het_venster() {
        assert!(!verborgen_effectief(Some("aan"), false));
        assert!(!verborgen_effectief(None, false));
    }

    #[test]
    fn zonder_keuze_blijft_het_oude_gedrag_van_starten_bij_aanmelden() {
        assert!(verborgen_effectief(None, true));
    }

    #[test]
    fn de_schakelaar_toont_de_keuze_of_de_standaard() {
        assert!(verborgen_keuze(Some("aan"), false));
        assert!(!verborgen_keuze(Some("uit"), true));
        assert!(verborgen_keuze(None, true));
        assert!(!verborgen_keuze(None, false));
    }

    #[test]
    fn de_standaard_rusttijd_is_vijf_minuten() {
        assert_eq!(
            rusttijd_uit_bewaard(None),
            Rusttijd::Duur(Duration::from_mins(5))
        );
        assert_eq!(
            rusttijd_uit_bewaard(Some("onzin")),
            Rusttijd::Duur(Duration::from_mins(5))
        );
    }

    #[test]
    fn de_vier_keuzes_geven_de_rusttijd_die_de_dicteerketen_gebruikt() {
        assert_eq!(
            rusttijd_uit_bewaard(Some("5m")),
            Rusttijd::Duur(Duration::from_mins(5))
        );
        assert_eq!(
            rusttijd_uit_bewaard(Some("15m")),
            Rusttijd::Duur(Duration::from_mins(15))
        );
        assert_eq!(
            rusttijd_uit_bewaard(Some("1u")),
            Rusttijd::Duur(Duration::from_hours(1))
        );
        assert_eq!(rusttijd_uit_bewaard(Some("nooit")), Rusttijd::Nooit);
    }

    #[test]
    fn altijd_klaar_aan_wordt_altijd() {
        // De oude keuze "Altijd klaar" bewaarde "nooit"; dat is nu de keuze "Altijd".
        assert_eq!(rusttijd_keuze("nooit"), Some("nooit"));
        assert_eq!(rusttijd_uit_bewaard(Some("nooit")), Rusttijd::Nooit);
    }

    #[test]
    fn oudere_rusttijden_vallen_op_een_keuze_die_minstens_zo_lang_houdt() {
        assert_eq!(rusttijd_keuze("meteen"), Some("5m"));
        assert_eq!(rusttijd_keuze("2m"), Some("5m"));
        assert_eq!(rusttijd_keuze("10m"), Some("15m"));
        assert_eq!(rusttijd_keuze("onzin"), None);
    }

    #[test]
    fn nooit_gekozen_is_zeven_dagen() {
        assert_eq!(bewaar_dagen_uit(&Gelezen::Ontbreekt), Some(7));
    }

    #[test]
    fn een_geldige_keuze_wordt_gelezen() {
        for dagen in [1, 7, 30] {
            assert_eq!(
                bewaar_dagen_uit(&Gelezen::Tekst(dagen.to_string())),
                Some(dagen)
            );
        }
    }

    #[test]
    fn onleesbaar_of_ongeldig_betekent_niet_wissen() {
        assert_eq!(bewaar_dagen_uit(&Gelezen::Onleesbaar), None);
        for tekst in ["", "0", "14", "-7", "zeven", "7 dagen"] {
            assert_eq!(
                bewaar_dagen_uit(&Gelezen::Tekst(tekst.to_string())),
                None,
                "{tekst}"
            );
        }
    }

    #[test]
    fn terugzetten_wist_instellingen_maar_niet_wat_mondig_onthoudt() {
        let alle: Vec<String> = [
            "sneltoets",
            "microfoon",
            "videokaart",
            "model_dicteren",
            "model_bestanden",
            "bewaar_map",
            "modellen_gezien",
            "download_daarna_parakeet-ultra-q8",
            "laatst_geziene_versie",
            "vergadering_bewaar_dagen",
            "downloads_onderbroken",
        ]
        .map(String::from)
        .to_vec();
        let weg = sleutels_om_te_wissen(alle);
        assert_eq!(
            weg,
            [
                "sneltoets",
                "microfoon",
                "videokaart",
                "model_dicteren",
                "model_bestanden",
                "bewaar_map"
            ]
        );
    }
}
