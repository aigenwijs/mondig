//! Persoonsgegevens markeren en vervangen (stroom V, V-2): wat het verslag en het transcript aan de
//! interface geven van de markeringen van `mondig_core::persoonsgegevens`. De herkenning zelf en
//! de staat (weggehaalde en eigen markeringen, de schakelaar "vervangen") staan in `mondig-core`;
//! de staat leeft in het geheugen bij het verslag of transcript en gaat nooit naar schijf of log.

use mondig_core::bewaren::Document;
use mondig_core::persoonsgegevens::{Markeringen, Soort, Vondst, utf16_plek};
use mondig_core::taak::{Foutmelding, Vervolg};
use serde::Serialize;

/// Eén markering in een regel, met plekken in UTF-16-eenheden zoals de webview telt.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MarkeringInfo {
    pub(crate) soort: Soort,
    pub(crate) van: usize,
    pub(crate) tot: usize,
}

/// De markeringen in `tekst`, in UTF-16-eenheden.
pub(crate) fn markeringen_van(markeringen: &Markeringen, tekst: &str) -> Vec<MarkeringInfo> {
    markeringen_info(tekst, &markeringen.vondsten(tekst))
}

/// De gegeven vondsten in `tekst`, in UTF-16-eenheden.
pub(crate) fn markeringen_info(tekst: &str, vondsten: &[Vondst]) -> Vec<MarkeringInfo> {
    vondsten
        .iter()
        .map(|v| MarkeringInfo {
            soort: v.soort,
            van: utf16_plek(tekst, v.van),
            tot: utf16_plek(tekst, v.tot),
        })
        .collect()
}

/// De staat van de markeringen van één verslag, voor het paneel.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PersoonInfo {
    /// Wordt er bij Kopiëren en opslaan vervangen?
    vervangen: bool,
    /// De stukken die de gebruiker zelf markeerde.
    eigen: Vec<String>,
}

pub(crate) fn persoon_info(markeringen: &Markeringen) -> PersoonInfo {
    PersoonInfo {
        vervangen: markeringen.vervangen(),
        eigen: markeringen.eigen(),
    }
}

/// Zet de persoonsgegevens in de velden van een document die de gebruiker zelf kan hebben getypt
/// (de naam van het bestand, klant en onderwerp, de namen van sprekers) om, als "Persoonsgegevens
/// vervangen" aan staat. De tekst van de regels doet de aanroeper zelf. Een titel die Mondig zelf
/// maakt ("Vergadering 3 november") gaat niet door de vervanging: `eigen_titel` is dan `false`.
pub(crate) fn vervang_velden(document: &mut Document, m: &Markeringen, eigen_titel: bool) {
    if !m.vervangen() {
        return;
    }
    if eigen_titel {
        document.titel = m.voor_naam(&document.titel);
        document.bron = m.voor_naam(&document.bron);
        document.standaard_onderwerp = m.voor_naam(&document.standaard_onderwerp);
    }
    document.klant = document.klant.take().map(|k| m.voor_naam(&k));
    document.onderwerp = document.onderwerp.take().map(|o| m.voor_naam(&o));
    document.datum = document.datum.take().map(|d| m.voor_naam(&d));
    for regel in &mut document.regels {
        regel.spreker = regel.spreker.take().map(|s| m.voor_naam(&s));
    }
}

/// De melding bij een markering die niet kan.
pub(crate) fn markeerfout(melding: &'static str) -> Foutmelding {
    Foutmelding::nieuw(melding, Vervolg::Geen)
}
