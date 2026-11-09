//! Dunne Tauri-schil van Mondig: venster, systeembalk en levenscyclus.
//! De eigenlijke logica komt in `mondig-core` (ADR 0002).

mod afstellen;
mod bestanden;
mod bewaakte_map;
mod bewaren;
mod computer;
mod dictaat;
mod dictaten;
mod dicteerketen;
mod downloads;
mod geluid;
mod instellingen;
mod logboek;
mod meter;
mod microfoonrecht;
mod modelgebruik;
mod modellen;
mod notulen;
mod opschonen;
mod persoon;
mod pil;
mod plakken;
mod snelheid;
mod sneltoets;
mod systeemvak;
mod taal;
mod venster;
mod vergadering;
mod versie;
mod videokaart;
mod wakker;

use std::path::PathBuf;

use mondig_core::download::{ModelOverzicht, overzicht};
use mondig_core::engine::beheer::Rusttijd;
use mondig_core::engine::supervisor::{Instellingen, Supervisor};
use mondig_core::engine::werkproces_naast;
use mondig_core::model::Gebruik;
use tauri::Emitter;
use tauri::{
    AppHandle, Manager, RunEvent, WindowEvent,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

/// Waar het werkproces met de spraakengine staat. Bepaald bij de start, naast de eigen exe.
/// `None` als het ontbreekt: de app start dan wel, maar dicteren kan niet.
pub(crate) struct EnginePad(pub(crate) Option<PathBuf>);

/// Zoekt het werkproces naast de eigen exe.
fn zoek_engine() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    werkproces_naast(&exe).ok()
}

/// Status van het dicteermodel voor het startscherm: ontbreekt, gedeeltelijk of compleet.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
fn model_status(app: AppHandle) -> Result<ModelOverzicht, String> {
    let (model, map) =
        dicteerketen::model_voor(&app, Gebruik::Dicteren).map_err(|fout| fout.melding)?;
    Ok(overzicht(&model, &map))
}

/// Het argument waarmee Windows Mondig bij het aanmelden start: dan alleen in de
/// systeembalk, zonder venster.
const AUTOSTART_ARG: &str = "--autostart";

/// Staat het dicteermodel helemaal op de computer? Zo niet, dan is dit een eerste start (of
/// een kapotte download) en moet het venster altijd open: de gebruiker moet het downloadscherm zien.
pub(crate) fn dicteermodel_compleet(app: &AppHandle) -> bool {
    dicteerketen::model_voor(app, Gebruik::Dicteren)
        .is_ok_and(|(model, map)| mondig_core::download::is_compleet(&model, &map))
}

/// Blijft het venster bij het starten verborgen? Alleen als de gebruiker dat wil, er een
/// compleet model is en de sneltoets werkt: zonder model of sneltoets hoort het venster altijd
/// te openen, anders staat Mondig onzichtbaar en onbruikbaar in het systeemvak.
fn blijft_verborgen(verborgen_gewenst: bool, model_compleet: bool, sneltoets_actief: bool) -> bool {
    verborgen_gewenst && model_compleet && sneltoets_actief
}

/// Toont het hoofdvenster en zet het op de voorgrond.
pub(crate) fn toon_venster(app: &AppHandle) {
    if let Some(venster) = app.get_webview_window("main") {
        let _ = venster.show();
        let _ = venster.unminimize();
        let _ = venster.set_focus();
    }
}

/// Opent het venster op het scherm Opnemen.
pub(crate) fn toon_opnemen(app: &AppHandle) {
    toon_venster(app);
    let _ = app.emit_to("main", "navigeer", "/vergadering");
}

/// Opent het venster op het scherm Over Mondig.
fn toon_over(app: &AppHandle) {
    toon_venster(app);
    let _ = app.emit_to("main", "navigeer", "/over");
}

/// Opent het venster op het scherm Instellingen. De interface luistert naar het event
/// `navigeer` en gaat naar de route.
fn toon_instellingen(app: &AppHandle) {
    toon_venster(app);
    let _ = app.emit_to("main", "navigeer", "/instellingen");
}

/// Zet "Laatste dictaat kopiëren" aan als er een laatste dictaat is (ADR 0012, "Tekst nooit
/// kwijt").
pub(crate) fn werk_tray_bij(app: &AppHandle) {
    let er_is_een =
        dictaten::tekst_voor_tray(&app.state::<dicteerketen::DicteerStaat>().laatste_dictaat)
            .is_some();
    systeemvak::zet_kopieer(app, er_is_een);
}

/// Kopieert het laatste dictaat naar het klembord, buiten de klembordgeschiedenis.
fn kopieer_laatste_dictaat(app: &AppHandle) {
    let tekst =
        dictaten::tekst_voor_tray(&app.state::<dicteerketen::DicteerStaat>().laatste_dictaat);
    if let Some(tekst) = tekst {
        // Het klembord kan even bezet zijn; daar hoeft de gebeurtenislus niet op te wachten.
        std::thread::spawn(move || {
            let _ = plakken::zet_op_klembord(&tekst);
        });
    }
}

/// Opent de opslagmap in de Verkenner. Lukt dat niet, dan opent Mondig zelf op Instellingen,
/// waar de map te kiezen is: het menu van het systeemvak kan geen melding tonen.
fn open_map(app: &AppHandle) {
    if bewaren::open_map(app).is_err() {
        toon_instellingen(app);
    }
}

/// Sluit de app af. Loopt er een download, dan vraagt een venster eerst of dat de bedoeling
/// is (de download stopt dan en hervat bij een volgende start). De vraag blokkeert, dus hij
/// draait op een eigen thread.
fn sluit_af(app: &AppHandle) {
    if !downloads::iets_actief(app) {
        app.exit(0);
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if modellen::mag_afsluiten(&app) {
            app.exit(0);
        }
    });
}

/// Bouwt het systeembalkpictogram. De regels van het menu staan in `systeemvak`; het menu wordt
/// opnieuw opgebouwd als de opnamestand of het laatste dictaat verandert.
fn maak_tray(app: &AppHandle) -> tauri::Result<()> {
    app.manage(systeemvak::TrayStaat::default());
    let menu = systeemvak::maak_menu(app)?;

    let mut bouwer = TrayIconBuilder::with_id("hoofd")
        .tooltip("Mondig")
        .menu(&menu)
        // Linksklik toont het venster; het menu opent met rechtsklik.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            systeemvak::ID_OPENEN => toon_venster(app),
            systeemvak::ID_OPNAME_START => vergadering::start_vanuit_systeemvak(app),
            systeemvak::ID_OPNAME_STOP => vergadering::stop_vanuit_systeemvak(app),
            systeemvak::ID_KOPIEER => kopieer_laatste_dictaat(app),
            systeemvak::ID_MAP => open_map(app),
            systeemvak::ID_INSTELLINGEN => toon_instellingen(app),
            systeemvak::ID_OVER => toon_over(app),
            systeemvak::ID_AFSLUITEN => sluit_af(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toon_venster(tray.app_handle());
            }
        });
    if let Some(icoon) = app.default_window_icon() {
        bouwer = bouwer.icon(icoon.clone());
    }
    bouwer.build(app)?;
    Ok(())
}

/// Rooktest: met `MONDIG_ROOKTEST=1` start de app het werkproces, wacht tot het zich
/// meldt met de juiste protocolversie, stopt het netjes en sluit zichzelf af.
/// Lukt dat niet, dan een melding en exitcode 1.
#[expect(
    clippy::print_stdout,
    reason = "scripts/rooktest.sh leest de regels \"rooktest: ...\" uit de standaarduitvoer"
)]
fn rooktest(app: &AppHandle) {
    if std::env::var("MONDIG_ROOKTEST").as_deref() != Ok("1") {
        return;
    }
    if app.get_webview_window("main").is_none() {
        eprintln!("rooktest: hoofdvenster ontbreekt");
        app.exit(1);
        return;
    }
    println!("rooktest: gestart {}", app.package_info().version);
    let Some(engine_pad) = app.state::<EnginePad>().0.clone() else {
        eprintln!("rooktest: het werkproces mondig-engine staat niet naast de app");
        app.exit(1);
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        let mut instellingen = Instellingen::standaard(engine_pad);
        instellingen.bouwcontrole = versie::bouwcontrole();
        let mut supervisor = Supervisor::nieuw(instellingen);
        match supervisor.controleer_start() {
            Ok(()) => {
                println!("rooktest: werkproces gemeld en gestopt");
                app.exit(0);
            }
            Err(fout) => {
                eprintln!("rooktest: werkproces mislukt: {fout}");
                app.exit(1);
            }
        }
    });
}

/// Gebeurtenissen van de vensters: de pil en het hoofdvenster.
fn bij_vensterevent(venster: &tauri::Window, event: &WindowEvent) {
    // De pil neemt nooit de focus; krijgt hij hem toch, dan gaat hij meteen terug.
    if venster.label() == pil::LABEL {
        if let WindowEvent::Focused(true) = event {
            pil::kreeg_focus();
        }
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
        }
        return;
    }
    // Bestanden die op het venster worden gesleept, gaan naar de wachtrij (sprint 4-B).
    if let WindowEvent::DragDrop(slepen) = event {
        bestanden::bij_slepen(venster.app_handle(), slepen);
    }
    // Verliest het venster de focus terwijl de interface een nieuwe sneltoets vangt,
    // dan komt de oude toets terug: de toetsen worden toch niet meer gevangen.
    if let WindowEvent::Focused(false) = event {
        sneltoets::herstel_na_vangen(venster.app_handle());
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        // Sluiten of verbergen: de microfoontest en de meters stoppen en een opgeschorte
        // sneltoets komt terug, wat de interface ook nog doet of niet meer doet.
        meter::stop_alle(venster.app_handle(), meter::Reden::Venster);
        sneltoets::herstel_na_vangen(venster.app_handle());
        // Windows en macOS: sluiten verbergt het venster; de app blijft in de systeembalk.
        // Tijdens een vergaderopname minimaliseert het, zodat het stipje op het
        // taakbalkicoon zichtbaar blijft (besluit, 2-11-2026).
        #[cfg(not(target_os = "linux"))]
        {
            api.prevent_close();
            if vergadering::loopt(venster.app_handle()) {
                let _ = venster.minimize();
            } else {
                let _ = venster.hide();
            }
        }
        // Linux: standaard-GNOME toont geen systeembalk. Verbergen zou de app
        // onzichtbaar en onafsluitbaar maken, dus het kruis sluit de app gewoon af.
        #[cfg(target_os = "linux")]
        {
            let _ = (api, venster);
        }
    }
}

/// Bij het starten: de staat van de app, de sneltoets, de systeembalk en de pil.
fn start(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    logboek::start(app);
    app.manage(EnginePad(zoek_engine()));
    app.manage(dicteerketen::DicteerStaat::default());
    // De downloadrij; wat bij de vorige keer onderbroken werd, staat er meteen in.
    downloads::start(app);
    app.manage(bestanden::BestandenStaat::default());
    app.manage(bewaren::BewaarStaat::default());
    app.manage(bewaakte_map::BewaakStaat::default());
    app.manage(vergadering::VergaderingStaat::default());
    app.manage(sneltoets::SneltoetsStand::default());
    app.manage(opschonen::OpschoonStaat::default());
    app.manage(meter::MeterStaat::default());
    app.manage(videokaart::ApparaatStaat::default());
    app.manage(dictaat::Dicteerwerker::start(app.clone()));
    sneltoets::registreer_bij_start(app);
    // Parakeet v3 is vervallen: een bestaande download van dat model gaat eenmalig weg.
    {
        let app = app.clone();
        std::thread::spawn(move || modellen::ruim_afgeschafte_modellen_op(&app));
    }
    // Rusttijd "Altijd": het model laadt al bij het starten van Mondig.
    if instellingen::lees_rusttijd(app) == Rusttijd::Nooit {
        let app = app.clone();
        std::thread::spawn(move || {
            let _ = dicteerketen::beheerder_als_mogelijk(&app);
        });
    }
    maak_tray(app)?;
    pil::maak(app)?;
    // Opnames die nog wachten (afgebroken, of "Later uitschrijven") staan meteen in de stand.
    vergadering::bij_start(app);
    // De dictaatgeschiedenis: het laatste dictaat terug in het geheugen en opruimen.
    dictaten::bij_start(app);
    // De bewaakte map en de slaapstand: twee eigen threads die meelopen zolang Mondig draait,
    // ook vanuit de systeembalk.
    bewaakte_map::start(app);
    wakker::start(app);
    // In het midden van dit scherm (zonder taakbalk), kleiner als de standaardmaat niet past.
    if let Some(hoofdvenster) = app.get_webview_window("main") {
        venster::pas_in_werkgebied(app, &hoofdvenster);
    }
    // Op de Mac: bestanden aannemen die als belofte gesleept worden, uit Dictafoon, Foto's en
    // Mail (ADR 0030).
    #[cfg(target_os = "macos")]
    bestanden::plaats_mac_haak(app);
    // Het venster is bij het aanmaken onzichtbaar (`visible: false` in tauri.conf.json), dus
    // een verborgen start flitst niet: we tonen het alleen als dat nodig is.
    let via_autostart = std::env::args().any(|arg| arg == AUTOSTART_ARG);
    let verborgen = instellingen::verborgen_effectief(
        instellingen::lees_verborgen_keuze(app).as_deref(),
        via_autostart,
    );
    if !blijft_verborgen(
        verborgen,
        dicteermodel_compleet(app),
        sneltoets::is_actief(app),
    ) {
        toon_venster(app);
    }
    rooktest(app);
    Ok(())
}

/// Start de app en blijft draaien tot de gebruiker afsluit.
///
/// # Errors
///
/// Geeft een fout als Tauri de app niet kan bouwen of starten.
#[expect(
    clippy::too_many_lines,
    reason = "de lijst met commando's is lang; opsplitsen maakt hem niet duidelijker"
)]
pub fn run() -> tauri::Result<()> {
    let app = tauri::Builder::default()
        // Tweede start: bestaand venster tonen en focussen.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            toon_venster(app);
        }))
        // De bestandskiezer van de proef opent vanuit Rust; de interface krijgt er geen rechten voor.
        .plugin(tauri_plugin_dialog::init())
        // Sneltoets en instellingen worden alleen vanuit Rust gebruikt. De capability geeft de
        // webview geen rechten voor deze plugins; de interface praat via onze eigen commando's.
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        // Starten bij aanmelden (standaard uit), ook alleen vanuit Rust.
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg(AUTOSTART_ARG)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            model_status,
            modellen::modellen_overzicht,
            modellen::modellen_gezien,
            modellen::model_gebruik_zet,
            modellen::model_verwijder,
            downloads::model_download_start,
            downloads::download_stop,
            downloads::download_lopend,
            downloads::downloads_lijst,
            dicteerketen::proef_start,
            dicteerketen::proef_annuleer,
            dicteerketen::laatste_dictaat,
            dictaten::dictaten_stand,
            dictaten::dictaten_bewaren_zet,
            dictaten::dictaat_verwijder,
            dictaten::dictaten_wis_alles,
            dictaat::dictaat_annuleer,
            sneltoets::sneltoets_status,
            sneltoets::sneltoets_controleer,
            sneltoets::sneltoets_info,
            sneltoets::sneltoets_vang_start,
            sneltoets::sneltoets_vang_stop,
            sneltoets::sneltoets_wijzig,
            instellingen::rusttijd_instelling,
            instellingen::rusttijd_zet,
            instellingen::microfoon_lijst,
            instellingen::microfoon_kies,
            meter::meter_start,
            meter::meter_hartslag,
            meter::meter_stop,
            instellingen::alleen_klembord_instelling,
            instellingen::alleen_klembord_zet,
            instellingen::sprekers_instelling,
            instellingen::sprekers_zet,
            instellingen::stopwoorden_instelling,
            instellingen::stopwoorden_zet,
            instellingen::afspeelsnelheid_instelling,
            instellingen::afspeelsnelheid_zet,
            videokaart::videokaart_info,
            videokaart::videokaart_zet,
            videokaart::apparaat_in_gebruik,
            videokaart::snelheidstest_start,
            computer::computer_check,
            computer::laatste_controle,
            computer::laatste_controle_bewaar,
            afstellen::advies_overnemen,
            afstellen::model_kiezen,
            logboek::logboek_tekst,
            logboek::logboek_map_open,
            logboek::wizard_stap_log,
            instellingen::instellingen_terugzetten,
            instellingen::autostart_instelling,
            instellingen::autostart_zet,
            instellingen::verborgen_starten_instelling,
            instellingen::verborgen_starten_zet,
            dictaat::dictaat_stop,
            geluid::geluid_instelling,
            geluid::geluid_zet,
            geluid::testgeluid_speel,
            microfoonrecht::microfooninstelling_open,
            pil::pil_stand,
            pil::pil_toon,
            pil::pil_verberg,
            pil::pil_open_mondig,
            pil::pil_positie_instelling,
            pil::pil_positie_zet,
            plakken::klembord_zet,
            versie::versie_info,
            versie::werkproces_bouw,
            versie::model_vermeldingen,
            versie::open_vermeldingsadres,
            versie::laatst_geziene_versie,
            versie::laatst_geziene_versie_zet,
            bestanden::bestanden_kies,
            bestanden::bestanden_stand,
            bestanden::bestand_annuleer,
            bestanden::bestand_weghalen,
            bestanden::bestand_transcript,
            bestanden::bestand_zet_tekst,
            bestanden::bestand_persoon_vervangen,
            bestanden::bestand_persoon_weghalen,
            bestanden::bestand_persoon_markeer,
            bestanden::bestand_audio_stuk,
            bewaren::bewaren_instellingen,
            bewaren::bewaren_kies_map,
            bewaren::bewaren_zet_bouwstenen,
            bewaren::bewaren_zet_formaat,
            bewaren::bewaren_zet_tijden,
            bewaren::bewaren_zet_automatisch,
            bewaren::bewaren_zet_audio,
            bewaren::bestand_bewaar,
            bewaren::bestand_bewaar_beslis,
            bewaren::bestand_zet_label,
            bewaren::bestand_open_opgeslagen,
            bewaren::bestand_toon_opgeslagen,
            bewaren::bewaren_open_map,
            bewaakte_map::bewaak_instellingen,
            bewaakte_map::bewaak_kies_map,
            bewaakte_map::bewaak_zet_aan,
            bewaakte_map::bewaak_zet_submappen,
            bewaakte_map::bewaak_zet_overslaan,
            bewaakte_map::bewaak_zet_naast,
            bewaren::bestand_opgeslagen_lijst,
            bewaren::bestand_verwijder,
            vergadering::vergadering_stand,
            instellingen::bewaartermijn_instelling,
            instellingen::bewaartermijn_zet,
            vergadering::vergadering_afspeelapparaten,
            vergadering::vergadering_start,
            vergadering::vergadering_start_gevraagd,
            vergadering::vergadering_stop,
            vergadering::vergadering_later,
            vergadering::vergadering_weggooien,
            vergadering::vergadering_uitschrijven,
            vergadering::vergadering_herstel,
            vergadering::vergadering_naam,
            vergadering::vergadering_zet_tekst,
            vergadering::vergadering_persoon_vervangen,
            vergadering::vergadering_persoon_weghalen,
            vergadering::vergadering_persoon_markeer,
            vergadering::vergadering_audio_stuk,
            vergadering::vergadering_geef_aan_spreker,
            vergadering::vergadering_voeg_sprekers_samen,
            vergadering::vergadering_opslaan,
            vergadering::vergadering_kopieer,
            vergadering::vergadering_audio_bewaren,
            vergadering::vergadering_sluiten,
            vergadering::vergadering_verslag_openen,
            vergadering::vergadering_verslag_wissen,
            vergadering::vergadering_verslag_naam,
            vergadering::vergadering_opnieuw_keuzes,
            vergadering::vergadering_opnieuw_start,
            vergadering::vergadering_opnieuw_stoppen,
            vergadering::vergadering_vorige_terugzetten,
            vergadering::notulen_maken::vergadering_notulen_maken,
            vergadering::notulen_maken::vergadering_notulen_stoppen,
            vergadering::notulen_maken::vergadering_notulen_punt_aanpassen,
            vergadering::notulen_maken::vergadering_notulen_punt_weghalen,
            vergadering::notulen_maken::vergadering_notulen_kopieer,
            notulen::notulen_instellingen,
            notulen::notulen_zet_aanbieder,
            notulen::notulen_zet_model,
            notulen::notulen_zet_training,
            notulen::notulen_sleutel_zet,
            notulen::notulen_sleutel_wis,
            notulen::notulen_sleutel_test,
            notulen::notulen_ollama_stand,
            opschonen::opschonen_instellingen,
            opschonen::opschonen_zet_stand,
            opschonen::dictaat_opschonen_stop
        ])
        .on_window_event(bij_vensterevent)
        .setup(|app| start(app.handle()))
        .build(tauri::generate_context!())?;

    app.run(|app, event: RunEvent| {
        // Bij afsluiten het werkproces netjes stoppen, zodat er niets blijft hangen.
        if let RunEvent::Exit = event {
            // Een lopende vergaderopname eerst netjes afsluiten: dan staat ze de volgende
            // keer klaar om uit te schrijven.
            app.state::<vergadering::VergaderingStaat>().stop();
            app.state::<dictaat::Dicteerwerker>().stop();
            pil::verberg(app);
            downloads::sluit(app);
            app.state::<dicteerketen::DicteerStaat>().stop_alles();
            app.state::<bestanden::BestandenStaat>().stop();
        }
        // macOS: een klik op het Dock-icoon toont het verborgen venster weer.
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen { .. } = event {
            toon_venster(app);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app, event);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::blijft_verborgen;

    #[test]
    fn zonder_compleet_model_opent_het_venster_altijd() {
        assert!(!blijft_verborgen(true, false, true));
    }

    #[test]
    fn zonder_werkende_sneltoets_opent_het_venster_altijd() {
        assert!(!blijft_verborgen(true, true, false));
    }

    #[test]
    fn met_model_beslist_de_instelling() {
        assert!(blijft_verborgen(true, true, true));
        assert!(!blijft_verborgen(false, true, true));
    }
}
