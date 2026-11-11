//! De klantnamen die Mondig onthoudt als suggestie bij het veld Klant (UI-E).
//!
//! De logica zit in `mondig_core::klanten` (lijst, begrenzing, ontdubbelen, schrijven). Hier staan
//! de twee commando's voor de interface en het onthouden na een geslaagd opslaan. De namen blijven
//! op deze computer en komen nooit in een logboek of een foutmelding.

use std::sync::Mutex;

use mondig_core::klanten;
use mondig_core::taak::{Foutmelding, Vervolg};

use crate::dicteerketen::slot;

/// Eén lezer of schrijver tegelijk, zodat twee opslagacties elkaars naam niet overschrijven.
static BESTAND: Mutex<()> = Mutex::new(());

/// Onthoudt `naam` na een geslaagd opslaan. Een lege naam telt niet. Mislukt het schrijven, dan
/// is dat geen reden om het opslaan te laten falen: de suggestie komt dan niet.
pub(crate) fn onthoud(naam: Option<&str>) {
    let (Some(naam), Some(pad)) = (naam, klanten::standaard_bestand()) else {
        return;
    };
    let _guard = slot(&BESTAND);
    let _ = klanten::voeg_toe(&pad, naam);
}

/// De klantnamen, nieuwste eerst.
#[tauri::command]
pub(crate) fn klanten_lijst() -> Vec<String> {
    let Some(pad) = klanten::standaard_bestand() else {
        return Vec::new();
    };
    let _guard = slot(&BESTAND);
    klanten::lees(&pad)
}

/// Vergeet alle klantnamen. Opgeslagen bestanden en verslagen blijven zoals ze zijn.
#[tauri::command]
pub(crate) fn klanten_verwijder() -> Result<(), Foutmelding> {
    let Some(pad) = klanten::standaard_bestand() else {
        return Ok(());
    };
    let _guard = slot(&BESTAND);
    klanten::wis(&pad).map_err(|_| {
        Foutmelding::nieuw(
            "Mondig kon de klantnamen niet verwijderen. Probeer het opnieuw.",
            Vervolg::OpnieuwProberen,
        )
    })
}
