//! Dictaten opschonen in de app (ADR 0027): de keuzelijst "Dictaten opschonen" onder Instellingen,
//! Dicteren, de vraag "Waar gaat de tekst heen?" de eerste keer per aanbieder, en het opschonen
//! zelf in de dicteerketen, tussen uitschrijven en plakken.
//!
//! De regels staan in `mondig_core::opschonen` (opdracht, controle, wachttijd). Hier staat wat de
//! app eromheen doet:
//!
//! - **Instelling.** De stand (`uit`, `sneltoets`, `elk`) in de store. Aanzetten kan alleen met
//!   een aanbieder onder Notulen die klaar is (zelfde regels als notulen maken,
//!   [`crate::notulen::bepaal_start`]) en, per aanbieder één keer, na de vraag waar de tekst heen
//!   gaat. Dat dwingt Rust af: [`opschonen_zet_stand`] weigert zonder die bevestiging, en de
//!   dicteerketen schoont niet op voor een aanbieder die niet bevestigd is (de gebruiker kan
//!   onder Notulen intussen een andere gekozen hebben).
//! - **Verbinding.** Eén eigen, langlevende [`Verbinding`] voor opschonen, los van notulen: na
//!   de wachttijd breekt `schoon_op` de aanvraag af, en dat zou anders ook lopende notulen
//!   afbreken. De verbinding bouwt na een time-out zelf een nieuwe agent op. Bij een andere
//!   aanbieder of sleutel maakt [`vergeet_verbinding`] haar opnieuw.
//! - **Pil.** Tijdens het opschonen meldt het event `pil-opschonen` het label van de aanbieder;
//!   daarna of het opgeschoond of letterlijk is. Het kruisje in de pil stopt het opschonen
//!   ([`dictaat_opschonen_stop`]); dan komt de letterlijke tekst.
//! - **Logboek.** Alleen de uitkomst, de reden en de duur (`Uitkomst::logboek`), nooit tekst.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use mondig_core::dictaten::OpgeschoondDoor;
use mondig_core::logboek;
use mondig_core::model::Gebruik;
use mondig_core::notulen::Taal;
use mondig_core::opschonen::{self, Instellingen, Reden, Stand};
use mondig_core::sleutel;
use mondig_core::sneltoets::Toets;
use mondig_core::soevereiniteit::Soevereiniteit;
use mondig_core::taak::{Foutmelding, TaakId, Vervolg};
use mondig_core::taalmodel::{Verbinding, Voorinstelling};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::dicteerketen::slot;
use crate::sneltoets::{lees_instelling, schrijf_instelling, wis_instelling};
use crate::vergadering::blokkerend;

/// Sleutel in de store: `uit`, `sneltoets` of `elk`. Ontbreekt hij, dan staat opschonen uit.
pub(crate) const SLEUTEL_STAND: &str = "opschonen";
/// Voorvoegsel van de sleutel die zegt dat de gebruiker voor deze aanbieder bevestigde waar de
/// tekst heen gaat (`opschonen_bevestigd_mistral`).
const VOORVOEGSEL_BEVESTIGD: &str = "opschonen_bevestigd_";
const JA: &str = "ja";

/// Wat de app bijhoudt voor opschonen.
#[derive(Default)]
pub(crate) struct OpschoonStaat {
    /// De eigen verbinding voor opschonen, voor de aanbieder van nu.
    verbinding: Mutex<Option<Verbinding>>,
    /// Per dictaat de vlag waarmee het kruisje (of Annuleren) het opschonen stopt. Een stop wordt
    /// onthouden, ook als hij komt voordat het opschonen begint; de dicteerketen ruimt de vlag op
    /// als het dictaat afgelopen is ([`vergeet`]).
    stoppen: Mutex<HashMap<TaakId, Arc<AtomicBool>>>,
}

impl OpschoonStaat {
    /// De stopvlag van dit dictaat (een nieuwe als die er nog niet is).
    fn vlag(&self, taak: TaakId) -> Arc<AtomicBool> {
        Arc::clone(slot(&self.stoppen).entry(taak).or_default())
    }

    fn stop(&self, taak: TaakId) {
        self.vlag(taak).store(true, Ordering::Relaxed);
    }

    fn vergeet(&self, taak: TaakId) {
        slot(&self.stoppen).remove(&taak);
    }
}

// ---------------------------------------------------------------------------
// De regels, zonder Tauri
// ---------------------------------------------------------------------------

/// De stand zoals de gebruiker hem ziet en zoals hij geldt: uit als er geen aanbieder klaar is,
/// of als de gebruiker voor die aanbieder nog niet bevestigde waar de tekst heen gaat.
fn geldende_stand(gekozen: Stand, klaar: Option<Voorinstelling>, bevestigd: bool) -> Stand {
    if klaar.is_some() && bevestigd {
        gekozen
    } else {
        Stand::Uit
    }
}

/// Mag de stand zo gezet worden? Geeft `true` als de bevestiging voor deze aanbieder nu
/// vastgelegd moet worden. `bevestiging` is de aanbieder (`mistral`, `ollama`) waarvoor de
/// gebruiker de vraag net beantwoordde.
fn controleer_zet(
    stand: Stand,
    klaar: Option<Voorinstelling>,
    al_bevestigd: bool,
    bevestiging: Option<&str>,
) -> Result<bool, Foutmelding> {
    if stand == Stand::Uit {
        return Ok(false);
    }
    let Some(aanbieder) = klaar else {
        return Err(Foutmelding::nieuw(
            "Kies eerst een taalmodel onder Notulen.",
            Vervolg::Geen,
        ));
    };
    if al_bevestigd {
        return Ok(false);
    }
    if bevestiging == Some(aanbieder.code()) {
        return Ok(true);
    }
    Err(Foutmelding::nieuw(
        "Bevestig eerst waar de tekst heen gaat.",
        Vervolg::OpnieuwProberen,
    ))
}

/// De taal van een dictaat: de gesproken taal (zie `crate::taal`), dezelfde als bij het
/// stopwoordenfilter.
fn taal_van(code: &str) -> Taal {
    if code == "en" { Taal::En } else { Taal::Nl }
}

/// Alles wat bepaalt of en hoe Mondig opschoont, één keer gelezen ([`lees`]). De stand die
/// geldt, de info voor de interface en het plan voor een dictaat volgen hier allemaal uit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Huidig {
    /// De stand die de gebruiker koos (ook als hij nu niet geldt).
    gekozen: Stand,
    /// De aanbieder onder Notulen als die klaar is voor gebruik, met het model voor notulen.
    klaar: Option<(Voorinstelling, String)>,
    /// Bevestigde de gebruiker voor die aanbieder waar de tekst heen gaat?
    bevestigd: bool,
}

impl Huidig {
    fn aanbieder(&self) -> Option<Voorinstelling> {
        self.klaar.as_ref().map(|(aanbieder, _)| *aanbieder)
    }

    /// De stand die nu geldt.
    pub(crate) fn geldend(&self) -> Stand {
        geldende_stand(self.gekozen, self.aanbieder(), self.bevestigd)
    }

    fn info(&self) -> OpschonenInfo {
        let aanbieder = self.aanbieder();
        OpschonenInfo {
            stand: self.geldend(),
            aanbieder: aanbieder.map(Voorinstelling::code),
            naam: aanbieder.map(Voorinstelling::naam),
            label: aanbieder.map(Voorinstelling::soevereiniteit),
            model: self.klaar.as_ref().map(|(aanbieder, notulenmodel)| {
                Instellingen::voor(*aanbieder, notulenmodel, Taal::Nl).model
            }),
            bevestigd: self.bevestigd,
        }
    }

    /// Moet een dictaat dat met `toets` begon opgeschoond worden, en hoe?
    fn plan(&self, toets: Toets, taal: Taal) -> Option<Plan> {
        if !self.geldend().schoont_op(toets) {
            return None;
        }
        let (aanbieder, notulenmodel) = self.klaar.as_ref()?;
        Some(Plan {
            aanbieder: *aanbieder,
            instellingen: Instellingen::voor(*aanbieder, notulenmodel, taal),
        })
    }
}

// ---------------------------------------------------------------------------
// Met de app
// ---------------------------------------------------------------------------

fn is_bevestigd(app: &AppHandle, aanbieder: Voorinstelling) -> bool {
    lees_instelling(app, &format!("{VOORVOEGSEL_BEVESTIGD}{}", aanbieder.code())).as_deref()
        == Some(JA)
}

/// Leest de stand, de aanbieder onder Notulen (met de sleutelbos bij Mistral) en de
/// bevestiging. De sleutelbos kan even duren: niet op de hoofddraad aanroepen.
pub(crate) fn lees(app: &AppHandle) -> Huidig {
    let gekozen = lees_instelling(app, SLEUTEL_STAND)
        .and_then(|waarde| Stand::uit_sleutel(&waarde))
        .unwrap_or_default();
    let notulen = crate::notulen::lees(app);
    let sleutel_staat = notulen.aanbieder == Some(Voorinstelling::Mistral)
        && sleutel::bestaat(Voorinstelling::Mistral).unwrap_or(false);
    let klaar = crate::notulen::bepaal_start(&notulen, sleutel_staat).ok();
    let bevestigd = klaar
        .as_ref()
        .is_some_and(|(aanbieder, _)| is_bevestigd(app, *aanbieder));
    Huidig {
        gekozen,
        klaar,
        bevestigd,
    }
}

/// De stand die nu geldt, voor de tweede sneltoets. Leest de sleutelbos, dus niet op de
/// hoofddraad.
pub(crate) fn lees_stand(app: &AppHandle) -> Stand {
    lees(app).geldend()
}

/// Wat de interface van opschonen weet. Het label en de naam komen uit Rust, zodat de interface
/// ze nooit zelf afleidt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpschonenInfo {
    /// De stand die geldt (zie [`geldende_stand`]).
    stand: Stand,
    /// `mistral` of `ollama` als er onder Notulen een aanbieder klaar is; anders niets.
    aanbieder: Option<&'static str>,
    /// De naam van die aanbieder (`Mistral`, `Ollama`).
    naam: Option<&'static str>,
    /// Het label van die aanbieder (`europees`, `lokaal`).
    label: Option<Soevereiniteit>,
    /// Het model dat opschoont: bij Mistral altijd Mistral Small, bij Ollama dat van notulen.
    model: Option<String>,
    /// Bevestigde de gebruiker al voor deze aanbieder waar de tekst heen gaat?
    bevestigd: bool,
}

/// De instellingen van opschonen.
#[tauri::command]
pub(crate) async fn opschonen_instellingen(app: AppHandle) -> Result<OpschonenInfo, Foutmelding> {
    blokkerend(move || Ok(lees(&app).info())).await
}

/// Kiest wanneer Mondig opschoont. Aanzetten weigert zonder aanbieder onder Notulen, en de eerste
/// keer per aanbieder zonder `bevestigd` (de code van de aanbieder waarvoor de gebruiker de vraag
/// "Waar gaat de tekst heen?" beantwoordde). De tweede sneltoets gaat mee aan of uit.
#[tauri::command]
pub(crate) async fn opschonen_zet_stand(
    app: AppHandle,
    stand: Stand,
    bevestigd: Option<String>,
) -> Result<OpschonenInfo, Foutmelding> {
    blokkerend(move || {
        let mut huidig = lees(&app);
        let aanbieder = huidig.aanbieder();
        if controleer_zet(stand, aanbieder, huidig.bevestigd, bevestigd.as_deref())?
            && let Some(aanbieder) = aanbieder
        {
            schrijf_instelling(
                &app,
                &format!("{VOORVOEGSEL_BEVESTIGD}{}", aanbieder.code()),
                JA,
            );
            huidig.bevestigd = true;
        }
        match stand {
            Stand::Uit => wis_instelling(&app, SLEUTEL_STAND),
            _ => schrijf_instelling(&app, SLEUTEL_STAND, stand.sleutel()),
        }
        huidig.gekozen = stand;
        crate::sneltoets::werk_opschoontoets_bij(&app, huidig.geldend());
        Ok(huidig.info())
    })
    .await
}

/// De aanbieder of de sleutel onder Notulen veranderde: de volgende keer een nieuwe verbinding,
/// en de tweede sneltoets volgt de stand die nu geldt. Leest de sleutelbos: niet op de
/// hoofddraad.
pub(crate) fn na_wijziging_notulen(app: &AppHandle) {
    vergeet_verbinding(app);
    crate::sneltoets::werk_opschoontoets_bij(app, lees_stand(app));
}

/// Gooit de verbinding voor opschonen weg, zodat de volgende een nieuwe maakt.
fn vergeet_verbinding(app: &AppHandle) {
    if let Some(staat) = app.try_state::<OpschoonStaat>() {
        *slot(&staat.verbinding) = None;
    }
}

/// Wat de dicteerketen nodig heeft om een dictaat op te schonen.
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    aanbieder: Voorinstelling,
    instellingen: Instellingen,
}

/// Moet dit dictaat (begonnen met `toets`) opgeschoond worden, en hoe? `None` als opschonen uit
/// staat, als het met de gewone toets begon bij "Met een eigen sneltoets", of als de aanbieder
/// niet klaar of niet bevestigd is.
pub(crate) fn plan_voor(app: &AppHandle, toets: Toets) -> Option<Plan> {
    lees(app).plan(
        toets,
        taal_van(crate::taal::gesproken_taal(app, Gebruik::Dicteren)),
    )
}

/// Wat de pil van het opschonen hoort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum PilFase {
    /// Het opschonen loopt, bij een aanbieder met dit label.
    Bezig { label: Soevereiniteit },
    /// Opgeschoond: de opgeschoonde tekst wordt geplakt.
    Opgeschoond,
    /// Niet opgeschoond: de letterlijke tekst wordt geplakt.
    Letterlijk,
}

#[derive(Clone, Serialize)]
struct PilOpschonen {
    taak: TaakId,
    fase: PilFase,
}

fn meld_pil(app: &AppHandle, taak: TaakId, fase: PilFase) {
    let _ = app.emit_to(
        crate::pil::LABEL,
        "pil-opschonen",
        PilOpschonen { taak, fase },
    );
}

/// Het dictaat na het opschonen.
pub(crate) struct Afloop {
    /// De tekst om te plakken.
    pub(crate) tekst: String,
    /// Wie het opschoonde; `None`: het is de letterlijke tekst.
    pub(crate) door: Option<OpgeschoondDoor>,
    /// Het label van de aanbieder als de tekst naar de aanbieder ging (ook als het daarna
    /// terugviel); `None` als hij de computer niet verliet.
    pub(crate) label: Option<Soevereiniteit>,
}

/// Is de tekst naar de aanbieder gegaan? Niet als er geen verbinding was, als er geen tekst was,
/// of als het opschonen al gestopt was voordat het begon.
fn is_verstuurd(verbonden: bool, al_gestopt: bool, letterlijk: Option<Reden>) -> bool {
    verbonden && !al_gestopt && letterlijk != Some(Reden::GeenTekst)
}

/// De verbinding voor deze aanbieder: de bestaande, of een nieuwe.
fn verbinding_voor(
    staat: &OpschoonStaat,
    aanbieder: Voorinstelling,
) -> Result<Verbinding, mondig_core::taalmodel::TaalmodelFout> {
    let mut houder = slot(&staat.verbinding);
    if let Some(verbinding) = houder.as_ref()
        && verbinding.voorinstelling() == aanbieder
    {
        return Ok(verbinding.clone());
    }
    let verbinding = Verbinding::voor(aanbieder)?;
    *houder = Some(verbinding.clone());
    Ok(verbinding)
}

/// Schoont het dictaat op (hooguit de wachttijd van de kern) en meldt de pil wat er gebeurt.
/// Geeft altijd een tekst om te plakken.
pub(crate) fn schoon_dictaat_op(app: &AppHandle, taak: TaakId, tekst: &str, plan: &Plan) -> Afloop {
    let staat = app.state::<OpschoonStaat>();
    // Eerst de stopvlag, dan pas het kruisje in de pil: een snelle klik gaat zo nooit verloren.
    let annuleer = staat.vlag(taak);
    let al_gestopt = annuleer.load(Ordering::Relaxed);
    let label = plan.aanbieder.soevereiniteit();
    meld_pil(app, taak, PilFase::Bezig { label });
    let (uitkomst, verbonden) = match verbinding_voor(&staat, plan.aanbieder) {
        Ok(verbinding) => (
            opschonen::schoon_op(&verbinding, tekst, &plan.instellingen, &annuleer),
            true,
        ),
        Err(fout) => (
            opschonen::Uitkomst {
                tekst: tekst.to_string(),
                letterlijk: Some(Reden::Aanbieder(fout)),
                voorstel: None,
                duur: std::time::Duration::ZERO,
                gebruik: None,
            },
            false,
        ),
    };
    logboek::log(&uitkomst.logboek());
    let opgeschoond = uitkomst.is_opgeschoond();
    meld_pil(
        app,
        taak,
        if opgeschoond {
            PilFase::Opgeschoond
        } else {
            PilFase::Letterlijk
        },
    );
    Afloop {
        door: opgeschoond.then(|| OpgeschoondDoor {
            model: plan.instellingen.model.clone(),
            aanbieder: plan.aanbieder.code().to_string(),
            label: Some(label),
        }),
        label: is_verstuurd(verbonden, al_gestopt, uitkomst.letterlijk).then_some(label),
        tekst: uitkomst.tekst,
    }
}

/// Het kruisje in de pil tijdens het opschonen: stoppen, de letterlijke tekst wordt geplakt.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de State per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn dictaat_opschonen_stop(taak: TaakId, staat: State<'_, OpschoonStaat>) {
    staat.stop(taak);
}

/// Stopt het opschonen van dit dictaat, ook als het nog moet beginnen (Annuleren in het venster).
pub(crate) fn stop_voor(app: &AppHandle, taak: TaakId) {
    if let Some(staat) = app.try_state::<OpschoonStaat>() {
        staat.stop(taak);
    }
}

/// Het dictaat is afgelopen: zijn stopvlag mag weg.
pub(crate) fn vergeet(app: &AppHandle, taak: TaakId) {
    if let Some(staat) = app.try_state::<OpschoonStaat>() {
        staat.vergeet(taak);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn huidig(gekozen: Stand, klaar: Option<(Voorinstelling, &str)>, bevestigd: bool) -> Huidig {
        Huidig {
            gekozen,
            klaar: klaar.map(|(aanbieder, model)| (aanbieder, model.to_string())),
            bevestigd,
        }
    }

    #[test]
    fn een_stop_voor_het_begin_gaat_niet_verloren() {
        let staat = OpschoonStaat::default();
        // Het kruisje (of Annuleren) komt voordat het opschonen zijn vlag ophaalt.
        staat.stop(TaakId(7));
        assert!(staat.vlag(TaakId(7)).load(Ordering::Relaxed));
        // Een ander dictaat heeft een eigen vlag.
        assert!(!staat.vlag(TaakId(8)).load(Ordering::Relaxed));
        staat.vergeet(TaakId(7));
        assert!(!staat.vlag(TaakId(7)).load(Ordering::Relaxed));
    }

    #[test]
    fn een_stop_tijdens_het_opschonen_bereikt_de_vlag_van_dat_dictaat() {
        let staat = OpschoonStaat::default();
        let vlag = staat.vlag(TaakId(3));
        staat.stop(TaakId(4));
        assert!(!vlag.load(Ordering::Relaxed));
        staat.stop(TaakId(3));
        assert!(vlag.load(Ordering::Relaxed));
    }

    #[test]
    fn de_tekst_verliet_de_computer_alleen_als_hij_echt_verstuurd_is() {
        assert!(is_verstuurd(true, false, None));
        assert!(is_verstuurd(true, false, Some(Reden::TeLaat)));
        assert!(!is_verstuurd(false, false, None));
        assert!(!is_verstuurd(true, true, Some(Reden::Gestopt)));
        assert!(!is_verstuurd(true, false, Some(Reden::GeenTekst)));
    }

    #[test]
    fn de_stand_geldt_alleen_met_een_aanbieder_die_bevestigd_is() {
        let mistral = Some((Voorinstelling::Mistral, "mistral-large-latest"));
        assert_eq!(
            huidig(Stand::BijElkDictaat, mistral, true).geldend(),
            Stand::BijElkDictaat
        );
        assert_eq!(
            huidig(Stand::BijElkDictaat, mistral, false).geldend(),
            Stand::Uit
        );
        assert_eq!(
            huidig(Stand::EigenSneltoets, None, true).geldend(),
            Stand::Uit
        );
    }

    #[test]
    fn het_plan_volgt_uit_dezelfde_regel_als_de_stand() {
        let mistral = Some((Voorinstelling::Mistral, "mistral-large-latest"));
        let eigen = huidig(Stand::EigenSneltoets, mistral, true);
        assert!(eigen.plan(Toets::Gewoon, Taal::Nl).is_none());
        let plan = eigen.plan(Toets::Opschonen, Taal::En).unwrap();
        assert_eq!(plan.instellingen.model, opschonen::MISTRAL_MODEL);
        assert_eq!(plan.instellingen.taal, Taal::En);
        // Niet bevestigd: geen plan, ook bij elk dictaat.
        let niet = huidig(Stand::BijElkDictaat, mistral, false);
        assert!(niet.plan(Toets::Gewoon, Taal::Nl).is_none());
        // Ollama: het model van notulen.
        let ollama = huidig(
            Stand::BijElkDictaat,
            Some((Voorinstelling::OllamaLokaal, "qwen3:8b")),
            true,
        );
        assert_eq!(
            ollama
                .plan(Toets::Gewoon, Taal::Nl)
                .unwrap()
                .instellingen
                .model,
            "qwen3:8b"
        );
    }

    #[test]
    fn aanzetten_weigert_zonder_aanbieder() {
        let fout = controleer_zet(Stand::BijElkDictaat, None, false, Some("mistral")).unwrap_err();
        assert!(fout.melding.contains("Notulen"), "{}", fout.melding);
        // Uitzetten kan altijd.
        assert!(!controleer_zet(Stand::Uit, None, false, None).unwrap());
    }

    #[test]
    fn de_eerste_keer_per_aanbieder_moet_de_gebruiker_bevestigen() {
        let mistral = Some(Voorinstelling::Mistral);
        // Zonder bevestiging: weigeren, de interface stelt de vraag.
        let fout = controleer_zet(Stand::EigenSneltoets, mistral, false, None).unwrap_err();
        assert_eq!(fout.vervolg, Vervolg::OpnieuwProberen);
        // Een bevestiging voor een andere aanbieder telt niet.
        assert!(controleer_zet(Stand::EigenSneltoets, mistral, false, Some("ollama")).is_err());
        // Met bevestiging voor deze aanbieder: vastleggen.
        assert!(controleer_zet(Stand::EigenSneltoets, mistral, false, Some("mistral")).unwrap());
        // Al bevestigd: niets nieuws vast te leggen.
        assert!(!controleer_zet(Stand::BijElkDictaat, mistral, true, None).unwrap());
    }

    #[test]
    fn de_taal_is_die_van_het_dicteermodel() {
        assert_eq!(taal_van("nl"), Taal::Nl);
        assert_eq!(taal_van("en"), Taal::En);
        assert_eq!(taal_van(crate::taal::MODELTAAL), Taal::Nl);
    }

    #[test]
    fn de_pil_hoort_het_label_en_de_afloop() {
        let bezig = serde_json::to_value(PilOpschonen {
            taak: TaakId(7),
            fase: PilFase::Bezig {
                label: Soevereiniteit::Europees,
            },
        })
        .unwrap();
        assert_eq!(
            bezig,
            serde_json::json!({"taak": 7, "fase": {"type": "bezig", "label": "europees"}})
        );
        let klaar = serde_json::to_value(PilFase::Letterlijk).unwrap();
        assert_eq!(klaar, serde_json::json!({"type": "letterlijk"}));
    }

    #[test]
    fn de_info_voor_de_interface_draagt_de_naam_en_het_label_van_rust() {
        let info = huidig(
            Stand::EigenSneltoets,
            Some((Voorinstelling::Mistral, "mistral-large-latest")),
            true,
        )
        .info();
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "stand": "eigenSneltoets",
                "aanbieder": "mistral",
                "naam": "Mistral",
                "label": "europees",
                "model": "mistral-small-latest",
                "bevestigd": true
            })
        );
        let lokaal = huidig(
            Stand::Uit,
            Some((Voorinstelling::OllamaLokaal, "qwen3:8b")),
            false,
        )
        .info();
        assert_eq!(lokaal.label, Some(Soevereiniteit::Lokaal));
        assert_eq!(lokaal.naam, Some("Ollama"));
        let zonder = huidig(Stand::BijElkDictaat, None, false).info();
        assert_eq!(zonder.stand, Stand::Uit);
        assert_eq!(zonder.label, None);
    }
}
