//! De globale sneltoetsen voor dicteren (sprint 3b): registreren bij de start, Esc alleen
//! tijdens een opname, en de gekozen toets in de instellingen (`tauri-plugin-store`).
//!
//! Er zijn er hooguit twee (ADR 0027 punt 1): de gewone dicteertoets, en bij "Dictaten
//! opschonen: Met een eigen sneltoets" een tweede die een opgeschoond dictaat geeft. Elk heeft
//! een eigen [`SneltoetsStatus`] in de [`SneltoetsStand`]; de commando's kiezen er een met
//! [`Soort`] (zonder: de gewone). De twee mogen niet gelijk zijn. De gewone toets gaat altijd
//! voor: de tweede wordt pas na de gewone geregistreerd, heeft geen terugvallijst, en staat uit
//! als hij bezet is of gelijk aan de gewone.
//!
//! De webview krijgt geen rechten voor deze twee plugins; de interface vraagt de stand
//! via het eigen commando [`sneltoets_status`]. De beslissingen (welke toets, houden of
//! tikken) staan in `mondig_core::sneltoets`; dit bestand voert ze alleen uit.
//!
//! Een andere toets kiezen (sprint 3b, stap B2) loopt in drie stappen: [`sneltoets_vang_start`]
//! schort beide actieve toetsen op (anders start Ctrl+Spatie midden in het kiezen een dictaat),
//! de interface vangt de toetsen, en [`sneltoets_wijzig`] registreert de nieuwe toets of
//! zet de oude terug. Het herstellen staat ook in [`herstel_na_vangen`], dat het venster
//! aanroept als het sluit of de focus verliest: dan blijft een toets nooit uit.

use std::sync::Mutex;
use std::time::Instant;

use mondig_core::opschonen::Stand;
use mondig_core::sneltoets::{
    Oordeel, Registratie, STANDAARD_OPSCHOONTOETS, STANDAARD_SNELTOETS, SneltoetsFout, Toets,
    controleer_sneltoets, normaliseer_sneltoets, registreer_opgeslagen, toets_label,
};
use mondig_core::taak::{Foutmelding, Vervolg};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_store::StoreExt;

use crate::dictaat::Dicteerwerker;

/// Het bestand met de instellingen, in de gegevensmap van de app.
pub(crate) const INSTELLINGEN: &str = "instellingen.json";
/// Sleutel: de sneltoets die de gebruiker wil. De terugvaloptie van een conflict komt hier
/// bewust niet in: de volgende start probeert de gewenste toets opnieuw.
pub(crate) const SLEUTEL_SNELTOETS: &str = "sneltoets";
/// Sleutel: de tweede sneltoets, voor een opgeschoond dictaat.
pub(crate) const SLEUTEL_OPSCHOONTOETS: &str = "opschonen_sneltoets";
/// Sleutel: de id van de gekozen microfoon.
pub(crate) const SLEUTEL_MICROFOON: &str = "microfoon";
/// De Esc-toets in de schrijfwijze van de plugin.
const ESC: &str = "Escape";

/// Welke sneltoets een commando bedoelt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Soort {
    /// De gewone dicteertoets.
    #[default]
    Dicteren,
    /// De tweede toets, voor een opgeschoond dictaat.
    Opschonen,
}

impl Soort {
    const ALLE: [Self; 2] = [Self::Dicteren, Self::Opschonen];

    fn ander(self) -> Self {
        match self {
            Self::Dicteren => Self::Opschonen,
            Self::Opschonen => Self::Dicteren,
        }
    }

    fn toets(self) -> Toets {
        match self {
            Self::Dicteren => Toets::Gewoon,
            Self::Opschonen => Toets::Opschonen,
        }
    }

    fn sleutel(self) -> &'static str {
        match self {
            Self::Dicteren => SLEUTEL_SNELTOETS,
            Self::Opschonen => SLEUTEL_OPSCHOONTOETS,
        }
    }
}

/// Wat de interface van een sneltoets weet. Serialiseert met camelCase-sleutels.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SneltoetsStatus {
    /// De actieve toets in gewone taal (`Ctrl+Spatie`), of `None` als geen enkele lukte (of,
    /// bij de tweede toets, als die niet aan hoort te staan).
    actief: Option<String>,
    /// Is de actieve toets een terugvaloptie omdat de gewenste niet vrij was?
    is_terugval: bool,
    /// De gewenste toets in gewone taal.
    gewenst: String,
    /// Alle toetsen die geprobeerd zijn, in gewone taal.
    geprobeerd: Vec<String>,
    /// Alleen bij de tweede toets: staat hij uit omdat hij gelijk is aan de gewone?
    dubbel: bool,
    /// De geregistreerde toets in de schrijfwijze van de plugin, om hem weer af te melden.
    #[serde(skip)]
    ruw: Option<String>,
    /// Is de actieve toets opgeschort omdat de interface een nieuwe toets vangt?
    #[serde(skip)]
    opgeschort: bool,
}

/// Waarom [`sneltoets_wijzig`] weigert. De interface kiest de tekst (Nederlands of Engels)
/// op het soort; de teksten staan dus alleen in de i18n van de interface.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "soort", rename_all = "camelCase")]
pub(crate) enum WijzigFout {
    /// De toets mag niet volgens de regels van Mondig.
    Geweigerd {
        /// Het soort fout.
        fout: SneltoetsFout,
    },
    /// Een ander programma gebruikt de toets al.
    Bezet,
    /// Het is al de andere sneltoets van Mondig.
    Dubbel,
}

/// De stand van beide sneltoetsen.
#[derive(Debug, Default)]
pub(crate) struct Standen {
    dicteren: SneltoetsStatus,
    opschonen: SneltoetsStatus,
    /// Hoort de tweede toets aan te staan? Bijgehouden door [`werk_opschoontoets_bij`], zodat
    /// wijzigen de sleutelbos niet hoeft te lezen.
    opschonen_aan: bool,
}

impl Standen {
    fn van(&mut self, soort: Soort) -> &mut SneltoetsStatus {
        match soort {
            Soort::Dicteren => &mut self.dicteren,
            Soort::Opschonen => &mut self.opschonen,
        }
    }

    fn lees(&self, soort: Soort) -> &SneltoetsStatus {
        match soort {
            Soort::Dicteren => &self.dicteren,
            Soort::Opschonen => &self.opschonen,
        }
    }
}

/// De stand van de sneltoetsen, gemaakt bij de start.
#[derive(Default)]
pub(crate) struct SneltoetsStand(Mutex<Standen>);

/// De stand van een sneltoets voor het scherm (zonder: de gewone).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de State per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn sneltoets_status(
    stand: State<'_, SneltoetsStand>,
    soort: Option<Soort>,
) -> SneltoetsStatus {
    crate::dicteerketen::slot(&stand.0)
        .lees(soort.unwrap_or_default())
        .clone()
}

/// Is er een dicteertoets geregistreerd? Na de start is dat `false` als alle pogingen faalden.
pub(crate) fn is_actief(app: &AppHandle) -> bool {
    app.try_state::<SneltoetsStand>().is_some_and(|stand| {
        crate::dicteerketen::slot(&stand.0)
            .dicteren
            .actief
            .is_some()
    })
}

/// Het oordeel over een sneltoets (schrijfwijze van de plugin), voor de interface. De regels
/// staan op een plek: `mondig_core::sneltoets::controleer_sneltoets`.
#[tauri::command]
pub(crate) fn sneltoets_controleer(toets: &str) -> Oordeel {
    controleer_sneltoets(toets)
}

/// Leest een tekstinstelling. `None` als de instelling ontbreekt of de store niet te lezen is.
pub(crate) fn lees_instelling(app: &AppHandle, sleutel: &str) -> Option<String> {
    let store = app.store(INSTELLINGEN).ok()?;
    store.get(sleutel)?.as_str().map(str::to_string)
}

/// De opgeslagen microfoon, als die is gekozen.
pub(crate) fn lees_microfoon(app: &AppHandle) -> Option<String> {
    lees_instelling(app, SLEUTEL_MICROFOON)
}

/// Registreert één toets, met de handler die houden en tikken doorgeeft met welke toets het is.
fn registreer_toets(app: &AppHandle, toets: &str, soort: Soort) -> Result<(), String> {
    let welke = soort.toets();
    app.global_shortcut()
        .on_shortcut(toets, move |app, _toets, event| {
            let Some(werker) = app.try_state::<Dicteerwerker>() else {
                return;
            };
            match event.state {
                ShortcutState::Pressed => werker.ingedrukt(welke, Instant::now()),
                ShortcutState::Released => werker.losgelaten(welke, Instant::now()),
            }
        })
        .map_err(|fout| fout.to_string())
}

/// Registreert de sneltoets bij de start: de gewenste toets uit de instellingen (of de
/// standaard), met de terugvallijst bij een fout. Meldt het resultaat in het logboek en
/// in de [`SneltoetsStand`] voor het startscherm. Daarna, als die aan hoort te staan, de tweede
/// toets (zie [`werk_opschoontoets_bij`]).
pub(crate) fn registreer_bij_start(app: &AppHandle) {
    let gewenst = lees_instelling(app, SLEUTEL_SNELTOETS).unwrap_or_else(|| {
        // Eerste start: leg de standaard vast, zodat de instellingen (sprint 3d) hem kunnen wijzigen.
        schrijf_instelling(app, SLEUTEL_SNELTOETS, STANDAARD_SNELTOETS);
        STANDAARD_SNELTOETS.to_string()
    });
    // Een opgeslagen toets gaat door dezelfde controle als een nieuwe (zie
    // `registreer_opgeslagen`): een verboden of gereserveerde toets wordt niet geregistreerd.
    let uitkomst = registreer_opgeslagen(&gewenst, |toets| {
        registreer_toets(app, toets, Soort::Dicteren)
    });

    let status = match uitkomst {
        Ok(Registratie { toets, is_terugval }) => {
            eprintln!(
                "sneltoets: {toets} geregistreerd{}",
                if is_terugval {
                    format!(" (terugval, gewenst was {gewenst})")
                } else {
                    String::new()
                }
            );
            SneltoetsStatus {
                actief: Some(toets_label(&toets)),
                is_terugval,
                gewenst: toets_label(&gewenst),
                geprobeerd: vec![toets_label(&toets)],
                dubbel: false,
                ruw: Some(toets),
                opgeschort: false,
            }
        }
        Err(fouten) => {
            for (toets, fout) in &fouten {
                eprintln!("sneltoets: {toets} niet geregistreerd: {fout}");
            }
            SneltoetsStatus {
                actief: None,
                is_terugval: false,
                gewenst: toets_label(&gewenst),
                geprobeerd: fouten.iter().map(|(toets, _)| toets_label(toets)).collect(),
                dubbel: false,
                ruw: None,
                opgeschort: false,
            }
        }
    };
    eprintln!(
        "sneltoets: Esc geregistreerd: {} (hoort buiten een opname nee te zijn)",
        app.global_shortcut().is_registered(ESC)
    );
    crate::dicteerketen::slot(&app.state::<SneltoetsStand>().0).dicteren = status;
    // De tweede toets op een eigen draad: de stand van opschonen vraagt de sleutelbos.
    let app = app.clone();
    std::thread::spawn(move || {
        let stand = crate::opschonen::lees_stand(&app);
        werk_opschoontoets_bij(&app, stand);
    });
}

/// De tweede toets die de gebruiker wil, in de schrijfwijze van de plugin. Een opgeslagen toets
/// die niet (meer) mag, wordt de standaard.
fn gewenste_opschoontoets(app: &AppHandle) -> String {
    lees_instelling(app, SLEUTEL_OPSCHOONTOETS)
        .map(|toets| normaliseer_sneltoets(&toets))
        .filter(|toets| !matches!(controleer_sneltoets(toets), Oordeel::Fout { .. }))
        .unwrap_or_else(|| STANDAARD_OPSCHOONTOETS.to_string())
}

/// Het event waarmee de interface hoort dat een sneltoets veranderde (de gewone of de tweede).
const GEWIJZIGD: &str = "sneltoetsen-gewijzigd";

fn meld_gewijzigd(app: &AppHandle) {
    let _ = app.emit(GEWIJZIGD, ());
}

/// Zet de tweede toets aan of uit, zoals de stand van "Dictaten opschonen" die nu geldt vraagt:
/// alleen bij "Met een eigen sneltoets". De aanroeper bepaalt die stand (daarvoor is de
/// sleutelbos nodig, dus niet op de hoofddraad). De gewone toets blijft altijd zoals hij is. Is
/// de tweede bezet of gelijk aan de gewone, dan staat hij uit en zegt de stand waarom. Meldt de
/// interface dat er iets veranderde. Mag vaker en vanaf elke draad aangeroepen worden.
pub(crate) fn werk_opschoontoets_bij(app: &AppHandle, opschonen: Stand) {
    zet_opschoontoets(app, opschonen.heeft_eigen_sneltoets());
    meld_gewijzigd(app);
}

/// Zie [`werk_opschoontoets_bij`]. Het registreren gebeurt buiten het slot: de plugin werkt op
/// de hoofddraad, en die kan intussen (in een commando) op dit slot wachten.
fn zet_opschoontoets(app: &AppHandle, wil: bool) {
    let gewenst = gewenste_opschoontoets(app);
    let label = toets_label(&gewenst);
    let Some(stand) = app.try_state::<SneltoetsStand>() else {
        return;
    };
    // Onder het slot: beslissen wat er weg moet en wat erbij komt.
    let (oud, nieuw) = {
        let mut standen = crate::dicteerketen::slot(&stand.0);
        standen.opschonen_aan = wil;
        let gewone = standen.dicteren.ruw.clone();
        let status = standen.van(Soort::Opschonen);
        // Al goed: aan met de gewenste toets, of uit.
        if (wil && status.ruw.as_deref() == Some(gewenst.as_str()))
            || (!wil && status.ruw.is_none() && status.actief.is_none())
        {
            status.gewenst = label;
            return;
        }
        let opgeschort = status.opgeschort;
        // Bij het vangen is de oude al afgemeld.
        let oud = status.ruw.take().filter(|_| !opgeschort);
        *status = SneltoetsStatus {
            gewenst: label.clone(),
            opgeschort,
            ..SneltoetsStatus::default()
        };
        let nieuw = if !wil {
            eprintln!("sneltoets: tweede toets uit");
            None
        } else if gewone.as_deref() == Some(gewenst.as_str()) {
            eprintln!("sneltoets: tweede toets {gewenst} is gelijk aan de gewone en staat uit");
            status.dubbel = true;
            status.geprobeerd = vec![label.clone()];
            None
        } else if opgeschort {
            // De interface vangt nu een toets; het herstellen na het vangen registreert hem.
            status.ruw = Some(gewenst.clone());
            status.actief = Some(label.clone());
            None
        } else {
            status.geprobeerd = vec![label.clone()];
            Some(gewenst)
        };
        (oud, nieuw)
    };
    // Zonder slot: afmelden en registreren.
    if let Some(oud) = oud
        && let Err(fout) = app.global_shortcut().unregister(oud.as_str())
    {
        eprintln!("sneltoets: tweede toets {oud} niet af te melden: {fout}");
    }
    let Some(nieuw) = nieuw else {
        return;
    };
    let gelukt = registreer_toets(app, &nieuw, Soort::Opschonen);
    let mut standen = crate::dicteerketen::slot(&stand.0);
    let status = standen.van(Soort::Opschonen);
    match gelukt {
        Ok(()) => {
            eprintln!("sneltoets: tweede toets {nieuw} geregistreerd");
            status.actief = Some(label);
            status.ruw = Some(nieuw);
        }
        Err(fout) => eprintln!("sneltoets: tweede toets {nieuw} niet geregistreerd: {fout}"),
    }
}

/// Schrijft een tekstinstelling. Lukt dat niet, dan werkt Mondig gewoon door met de standaard.
pub(crate) fn schrijf_instelling(app: &AppHandle, sleutel: &str, waarde: &str) {
    if let Ok(store) = app.store(INSTELLINGEN) {
        store.set(sleutel, waarde);
        let _ = store.save();
    }
}

/// Wist een instelling, zodat de standaard weer geldt.
pub(crate) fn wis_instelling(app: &AppHandle, sleutel: &str) {
    if let Ok(store) = app.store(INSTELLINGEN) {
        store.delete(sleutel);
        let _ = store.save();
    }
}

/// Schort beide actieve sneltoetsen op terwijl de interface een nieuwe toets vangt (anders
/// start de andere toets midden in het kiezen een dictaat). Controleert daarna of de toetsen
/// echt vrij zijn; zo niet, dan blijft alles zoals het was en krijgt de interface een fout.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de State per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn sneltoets_vang_start(
    app: AppHandle,
    stand: State<'_, SneltoetsStand>,
) -> Result<(), Foutmelding> {
    let mut standen = crate::dicteerketen::slot(&stand.0);
    let melding = || {
        Foutmelding::nieuw(
            "Mondig kan de sneltoets nu niet wijzigen. Probeer het opnieuw.",
            Vervolg::OpnieuwProberen,
        )
    };
    let mut opgeschort: Vec<(Soort, String)> = Vec::new();
    for soort in Soort::ALLE {
        let status = standen.van(soort);
        if status.opgeschort {
            continue;
        }
        // Er is geen actieve toets (niets lukte, of de tweede staat uit): niets op te schorten.
        let Some(ruw) = status.ruw.clone() else {
            continue;
        };
        let gelukt = match app.global_shortcut().unregister(ruw.as_str()) {
            Err(fout) => {
                eprintln!("sneltoets: {ruw} niet op te schorten: {fout}");
                false
            }
            Ok(()) if app.global_shortcut().is_registered(ruw.as_str()) => {
                eprintln!("sneltoets: {ruw} staat na het opschorten nog geregistreerd");
                false
            }
            Ok(()) => true,
        };
        if !gelukt {
            // Wat al opgeschort was, komt terug: half opgeschort vangen kan niet.
            for (eerder, toets) in opgeschort {
                if let Err(fout) = registreer_toets(&app, &toets, eerder) {
                    eprintln!("sneltoets: {toets} niet terug te zetten: {fout}");
                    let status = standen.van(eerder);
                    status.actief = None;
                    status.ruw = None;
                }
            }
            return Err(melding());
        }
        opgeschort.push((soort, ruw));
    }
    for (soort, _) in opgeschort {
        standen.van(soort).opgeschort = true;
    }
    Ok(())
}

/// Zet de opgeschorte sneltoetsen terug. Doet niets als er niets opgeschort is, dus het mag
/// altijd en vaker aangeroepen worden: door de interface na het vangen, en door het venster
/// zelf bij sluiten of focusverlies. Lukt het terugzetten niet (een ander programma pakte de
/// toets intussen), dan staat de stand op "geen toets" en toont het scherm dat.
pub(crate) fn herstel_na_vangen(app: &AppHandle) {
    let Some(stand) = app.try_state::<SneltoetsStand>() else {
        return;
    };
    let mut standen = crate::dicteerketen::slot(&stand.0);
    for soort in Soort::ALLE {
        let status = standen.van(soort);
        if !status.opgeschort {
            continue;
        }
        status.opgeschort = false;
        let Some(ruw) = status.ruw.clone() else {
            continue;
        };
        match registreer_toets(app, &ruw, soort) {
            Ok(()) => eprintln!("sneltoets: {ruw} hersteld na het vangen"),
            Err(fout) => {
                eprintln!("sneltoets: {ruw} niet te herstellen: {fout}");
                status.geprobeerd = vec![toets_label(&ruw)];
                status.gewenst = toets_label(&ruw);
                status.actief = None;
                status.is_terugval = false;
                status.ruw = None;
            }
        }
    }
}

/// De interface is klaar met vangen (Esc, ander scherm): de oude toetsen terug.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de State per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn sneltoets_vang_stop(
    app: AppHandle,
    stand: State<'_, SneltoetsStand>,
    soort: Option<Soort>,
) -> SneltoetsStatus {
    herstel_na_vangen(&app);
    crate::dicteerketen::slot(&stand.0)
        .lees(soort.unwrap_or_default())
        .clone()
}

/// Registreert de gevangen toets (schrijfwijze van de plugin, bijvoorbeeld `Ctrl+Alt+Space`)
/// voor de gewone of de tweede sneltoets. Lukt dat, dan is de oude toets afgemeld en bewaart de
/// store de nieuwe. Lukt het niet, dan staat de oude toets er weer en krijgt de interface de
/// fout "al in gebruik", of "dubbel" als het de andere sneltoets van Mondig is.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de State per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn sneltoets_wijzig(
    app: AppHandle,
    stand: State<'_, SneltoetsStand>,
    toets: String,
    soort: Option<Soort>,
) -> Result<SneltoetsStatus, WijzigFout> {
    wijzig(&app, &stand, &toets, soort.unwrap_or_default())
}

/// Zie [`sneltoets_wijzig`].
pub(crate) fn wijzig(
    app: &AppHandle,
    stand: &SneltoetsStand,
    toets: &str,
    soort: Soort,
) -> Result<SneltoetsStatus, WijzigFout> {
    // Een waarschuwing is goed: de interface vroeg de gebruiker al om bevestiging. Bewaar en
    // registreer de genormaliseerde vorm, zodat een alias als `Ctrl+Up` niet blijft staan.
    if let Oordeel::Fout { fout } = controleer_sneltoets(toets) {
        return Err(WijzigFout::Geweigerd { fout });
    }
    let toets = normaliseer_sneltoets(toets);
    {
        let standen = crate::dicteerketen::slot(&stand.0);
        let is_ander = standen.lees(soort.ander()).ruw.as_deref() == Some(toets.as_str());
        let is_zelfde = standen.lees(soort).ruw.as_deref() == Some(toets.as_str());
        drop(standen);
        if is_ander {
            herstel_na_vangen(app);
            return Err(WijzigFout::Dubbel);
        }
        // Dezelfde toets als nu: alleen terugzetten wat opgeschort was.
        if is_zelfde {
            herstel_na_vangen(app);
            return Ok(crate::dicteerketen::slot(&stand.0).lees(soort).clone());
        }
    }
    let opschonen_aan = crate::dicteerketen::slot(&stand.0).opschonen_aan;
    // De tweede toets terwijl die niet aan hoort te staan: alleen onthouden.
    if soort == Soort::Opschonen && !opschonen_aan {
        schrijf_instelling(app, SLEUTEL_OPSCHOONTOETS, &toets);
        herstel_na_vangen(app);
        zet_opschoontoets(app, false);
        meld_gewijzigd(app);
        return Ok(crate::dicteerketen::slot(&stand.0).lees(soort).clone());
    }

    let mut standen = crate::dicteerketen::slot(&stand.0);
    match registreer_toets(app, &toets, soort) {
        Ok(()) => {
            let status = standen.van(soort);
            // Was de oude toets al opgeschort, dan is hij al afgemeld.
            if !status.opgeschort
                && let Some(oud) = status.ruw.clone()
                && let Err(fout) = app.global_shortcut().unregister(oud.as_str())
            {
                eprintln!("sneltoets: {oud} niet af te melden: {fout}");
            }
            let label = toets_label(&toets);
            eprintln!("sneltoets: gewijzigd naar {toets}");
            status.actief = Some(label.clone());
            status.is_terugval = false;
            status.gewenst.clone_from(&label);
            status.geprobeerd = vec![label];
            status.dubbel = false;
            status.ruw = Some(toets.clone());
            status.opgeschort = false;
            let nieuw = status.clone();
            drop(standen);
            schrijf_instelling(app, soort.sleutel(), &toets);
            // De andere toets was ook opgeschort tijdens het vangen.
            herstel_na_vangen(app);
            // Stond de tweede toets uit omdat hij gelijk was aan de gewone, dan kan hij nu aan.
            if soort == Soort::Dicteren {
                zet_opschoontoets(app, opschonen_aan);
            }
            meld_gewijzigd(app);
            Ok(nieuw)
        }
        Err(fout) => {
            eprintln!("sneltoets: {toets} niet te registreren: {fout}");
            drop(standen);
            herstel_na_vangen(app);
            Err(WijzigFout::Bezet)
        }
    }
}

/// Zet Esc aan zolang er een opname loopt. Een globale Esc die altijd aan staat zou Esc in
/// alle andere programma's stelen. Lukt het niet (een ander programma heeft Esc), dan
/// werkt annuleren nog via de knop in het venster.
pub(crate) fn esc_aan(app: &AppHandle) {
    let resultaat = app
        .global_shortcut()
        .on_shortcut(ESC, |app, _toets, event| {
            if event.state == ShortcutState::Pressed
                && let Some(werker) = app.try_state::<Dicteerwerker>()
            {
                werker.esc();
            }
        });
    match resultaat {
        Ok(()) => eprintln!("sneltoets: Esc aan, een opname loopt"),
        Err(fout) => eprintln!("sneltoets: Esc niet te registreren: {fout}"),
    }
}

/// Zet Esc weer uit, zodat andere programma's hun Esc terug hebben.
pub(crate) fn esc_uit(app: &AppHandle) {
    if app.global_shortcut().is_registered(ESC) {
        match app.global_shortcut().unregister(ESC) {
            Ok(()) => eprintln!("sneltoets: Esc uit"),
            Err(fout) => eprintln!("sneltoets: Esc niet af te melden: {fout}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

    /// De toetsen die zonder Ctrl of Alt mogen, in de schrijfwijze van de app. F5 en F9
    /// vragen bevestiging, maar de plugin leest ze net zo.
    const LOSSE_TOETSEN: [(&str, Code); 6] = [
        ("Insert", Code::Insert),
        ("ScrollLock", Code::ScrollLock),
        ("F5", Code::F5),
        ("F9", Code::F9),
        ("F13", Code::F13),
        ("F24", Code::F24),
    ];

    #[test]
    fn losse_toetsen_zijn_voor_de_plugin_een_sneltoets_zonder_modifier() {
        for (tekst, code) in LOSSE_TOETSEN {
            assert!(
                !matches!(controleer_sneltoets(tekst), Oordeel::Fout { .. }),
                "{tekst}"
            );
            let uitkomst = tekst.parse::<Shortcut>();
            assert!(uitkomst.is_ok(), "{tekst} niet te lezen: {uitkomst:?}");
            let sneltoets = uitkomst.unwrap();
            assert_eq!(sneltoets.key, code, "{tekst}");
            assert_eq!(sneltoets.mods, Modifiers::empty(), "{tekst}");
        }
    }

    #[test]
    fn pause_is_voor_de_plugin_te_lezen_maar_voor_mondig_verboden() {
        // Dit is de reden voor de eigen controle: de plugin accepteert Pause en Ctrl+Pause
        // wel, maar ze werken niet als houd-sneltoets.
        assert!("Pause".parse::<Shortcut>().is_ok());
        assert!(matches!(
            controleer_sneltoets("Pause"),
            Oordeel::Fout { .. }
        ));
    }

    #[test]
    fn genormaliseerde_toets_is_voor_de_plugin_dezelfde_toets() {
        let alias: Shortcut = "Ctrl+Up".parse().unwrap();
        let canoniek: Shortcut = normaliseer_sneltoets("Ctrl+Up").parse().unwrap();
        assert_eq!(alias.key, canoniek.key);
        assert_eq!(alias.mods, canoniek.mods);
    }

    #[test]
    fn combinaties_met_ctrl_of_alt_blijven_parsen() {
        let sneltoets: Shortcut = "Ctrl+Space".parse().unwrap();
        assert_eq!(sneltoets.mods, Modifiers::CONTROL);
        assert_eq!(sneltoets.key, Code::Space);
    }

    #[test]
    fn de_standaard_tweede_toets_mag_en_is_voor_de_plugin_te_lezen() {
        assert_eq!(controleer_sneltoets(STANDAARD_OPSCHOONTOETS), Oordeel::Goed);
        assert_ne!(STANDAARD_OPSCHOONTOETS, STANDAARD_SNELTOETS);
        let sneltoets: Shortcut = STANDAARD_OPSCHOONTOETS.parse().unwrap();
        assert_eq!(sneltoets.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(toets_label(STANDAARD_OPSCHOONTOETS), "Ctrl+Shift+Spatie");
    }

    #[test]
    fn soort_komt_uit_de_interface_en_wijst_naar_de_juiste_toets() {
        let soort: Soort = serde_json::from_str("\"opschonen\"").unwrap();
        assert_eq!(soort, Soort::Opschonen);
        assert_eq!(soort.toets(), Toets::Opschonen);
        assert_eq!(soort.ander(), Soort::Dicteren);
        assert_eq!(Soort::default().toets(), Toets::Gewoon);
        assert_eq!(Soort::Opschonen.sleutel(), "opschonen_sneltoets");
        let fout = serde_json::to_value(WijzigFout::Dubbel).unwrap();
        assert_eq!(fout["soort"], "dubbel");
    }
}
