//! Dictaatgeschiedenis in de app (ADR 0021): de instelling "Dictaten bewaren", het bewaren
//! van elk nieuw dictaat, het wissen na de termijn en de commando's voor de lijst op Dicteren.
//!
//! De bestanden en de regels staan in `mondig_core::dictaten`. Hier staat wat daar omheen
//! hoort: de store, het geheugen met het laatste dictaat (ook bij Niet bewaren gevuld, zodat
//! "Laatste dictaat kopiëren" in het systeemvak altijd werkt), een opruimdraad en het event
//! `dictaten-gewijzigd` waarmee de interface haar lijst ververst. Elk commando dat iets
//! verandert stuurt dat event; de interface ververst nergens anders.
//!
//! Alles wat de bestanden raakt (bewaren, opruimen, verwijderen, alles wissen, de termijn
//! zetten) loopt onder één slot, [`SCHIJF`]. Zo kan Niet bewaren nooit een bestand achterlaten
//! dat net werd geschreven, en wist een ronde nooit een tijdelijk bestand dat nog geschreven
//! wordt. Het geheugen heeft een eigen, kort slot en wordt altijd na [`SCHIJF`] genomen.
//!
//! Dictaattekst en bestandsnamen komen nooit in het logboek. Er is ook geen logregel voor een
//! mislukte wis: de kern heeft daar geen gebeurtenis voor en een volgende ronde probeert het
//! een uur later opnieuw; bij Verwijderen en Alles wissen krijgt de gebruiker een melding.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use mondig_core::dictaten::{self, Dictaat, Gewist, OpgeschoondDoor, Termijn, VerwijderFout};
use mondig_core::taak::{Foutmelding, Vervolg};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::dicteerketen::{DicteerStaat, slot};
use crate::sneltoets::{INSTELLINGEN, lees_instelling};

/// Het event waarmee de interface hoort dat de lijst of de termijn veranderde.
const GEWIJZIGD: &str = "dictaten-gewijzigd";

/// Sleutel in de store: `niet`, `24u` of `7d`. Ontbreekt hij, dan geldt 24 uur.
const SLEUTEL: &str = "dictaten_bewaren";

/// Zo vaak ruimt Mondig verlopen dictaten op, naast het opruimen bij het starten.
const OPRUIMEN_ELK: Duration = Duration::from_hours(1);

/// Het ene slot om alles wat de dictaatbestanden raakt.
static SCHIJF: Mutex<()> = Mutex::new(());

/// Telt de dictaten van deze sessie, om een schrijfactie aan het juiste geheugen te koppelen.
static REEKS: AtomicU64 = AtomicU64::new(0);

fn schijf_slot() -> MutexGuard<'static, ()> {
    SCHIJF.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Het laatste dictaat in het geheugen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Laatste {
    pub(crate) tekst: String,
    /// Het id van het bestand op schijf; `None` bij Niet bewaren, of zolang het schrijven nog
    /// loopt of mislukte.
    pub(crate) id: Option<String>,
    reeks: u64,
}

/// Het geheugen met het laatste dictaat.
pub(crate) type Geheugen = Mutex<Option<Laatste>>;

// ---------------------------------------------------------------------------
// De instelling
// ---------------------------------------------------------------------------

/// De termijn bij een waarde uit de store. Anders dan bij opnames (waar een onleesbare
/// instelling betekent: niets wissen) geldt hier bij twijfel de standaard van 24 uur: een
/// opname kwijtraken is erg, een dictaat van gisteren niet, en te lang bewaren is hier het
/// grotere risico (ADR 0021 punt 5).
fn termijn_uit(bewaard: Option<&str>) -> Termijn {
    bewaard
        .and_then(Termijn::uit_sleutel)
        .unwrap_or(Termijn::STANDAARD)
}

/// De termijn die nu geldt.
pub(crate) fn lees_termijn(app: &AppHandle) -> Termijn {
    termijn_uit(lees_instelling(app, SLEUTEL).as_deref())
}

fn schrijffout() -> Foutmelding {
    Foutmelding::nieuw(
        "Mondig kon deze keuze niet bewaren. Controleer of er ruimte vrij is op je schijf en probeer het opnieuw.",
        Vervolg::OpnieuwProberen,
    )
}

fn wisfout() -> Foutmelding {
    Foutmelding::nieuw(
        "Mondig kon niet alles wissen. Probeer het opnieuw.",
        Vervolg::OpnieuwProberen,
    )
}

// ---------------------------------------------------------------------------
// De logica zonder Tauri, zodat ze te testen is
// ---------------------------------------------------------------------------

fn nieuwe_reeks() -> u64 {
    REEKS.fetch_add(1, Ordering::Relaxed) + 1
}

/// Zet een nieuw dictaat in het geheugen, nog zonder id. Geeft de reeks voor
/// [`bewaar_op_schijf`].
fn onthoud(geheugen: &Geheugen, tekst: &str) -> u64 {
    let reeks = nieuwe_reeks();
    *slot(geheugen) = Some(Laatste {
        tekst: tekst.to_string(),
        id: None,
        reeks,
    });
    reeks
}

/// Schrijft het dictaat naar schijf (behalve bij Niet bewaren) en koppelt het id aan het
/// geheugen, als daar nog hetzelfde dictaat staat. Bij een opgeschoond dictaat gaan de
/// letterlijke tekst en wie het opschoonde mee (ADR 0027 punt 4). Lukt het schrijven niet, dan
/// blijft de tekst in het geheugen, zonder id. De aanroeper houdt [`SCHIJF`] vast.
fn bewaar_op_schijf(
    geheugen: &Geheugen,
    basis: Option<&Path>,
    termijn: Termijn,
    tekst: &str,
    opgeschoond: Option<(&str, &OpgeschoondDoor)>,
    nu_ms: u64,
    reeks: u64,
) -> std::io::Result<()> {
    if termijn == Termijn::Niet {
        return Ok(());
    }
    let basis = basis.ok_or_else(|| std::io::Error::other("geen gegevensmap"))?;
    let id = match opgeschoond {
        Some((letterlijk, door)) => {
            dictaten::bewaar_opgeschoond(basis, tekst, letterlijk, door, nu_ms)?
        }
        None => dictaten::bewaar(basis, tekst, nu_ms)?,
    };
    if let Some(laatste) = slot(geheugen).as_mut()
        && laatste.reeks == reeks
    {
        laatste.id = Some(id);
    }
    Ok(())
}

/// Zet het nieuwste dictaat dat nog op schijf staat in het geheugen (met 24 uur of 7 dagen).
fn vul_aan(geheugen: &Geheugen, basis: &Path, termijn: Termijn, nu_ms: u64) {
    if termijn == Termijn::Niet {
        return;
    }
    let nieuwste = dictaten::lijst(basis, termijn, nu_ms).into_iter().next();
    *slot(geheugen) = nieuwste.map(|d| Laatste {
        tekst: d.tekst,
        id: Some(d.id),
        reeks: nieuwe_reeks(),
    });
}

/// Bij het starten: met 24 uur of 7 dagen is het nieuwste bewaarde dictaat het laatste.
fn herstel_geheugen(geheugen: &Geheugen, basis: Option<&Path>, termijn: Termijn, nu_ms: u64) {
    let Some(basis) = basis else {
        return;
    };
    if slot(geheugen).is_none() {
        vul_aan(geheugen, basis, termijn, nu_ms);
    }
}

/// Haalt het laatste dictaat uit het geheugen als precies dat bestand gewist is, en vult het
/// aan met het nieuwste dat nog staat. Een dictaat met dezelfde tekst maar een ander id telt
/// niet mee.
fn vergeet_gewiste(
    geheugen: &Geheugen,
    gewist: &[String],
    basis: &Path,
    termijn: Termijn,
    nu_ms: u64,
) {
    let weg = slot(geheugen)
        .as_ref()
        .and_then(|l| l.id.as_ref())
        .is_some_and(|id| gewist.contains(id));
    if weg {
        *slot(geheugen) = None;
        vul_aan(geheugen, basis, termijn, nu_ms);
    }
}

/// Een wisronde: wist wat verlopen is. Bij Niet bewaren is alles verlopen en gaat dus alles
/// weg; het geheugen blijft dan zoals het was (ADR 0021 punt 6 en 7). De aanroeper houdt
/// [`SCHIJF`] vast.
fn ruim_op_in(geheugen: &Geheugen, basis: &Path, termijn: Termijn, nu_ms: u64) -> Gewist {
    let gewist = dictaten::wis_verlopen(basis, termijn, nu_ms);
    if termijn != Termijn::Niet {
        vergeet_gewiste(geheugen, &gewist.ids, basis, termijn, nu_ms);
    }
    gewist
}

/// Verwijdert één dictaat; is het ook het laatste in het geheugen, dan komt daar het nieuwste
/// dat nog staat voor in de plaats.
fn verwijder_in(
    geheugen: &Geheugen,
    basis: &Path,
    termijn: Termijn,
    nu_ms: u64,
    id: &str,
) -> Result<(), VerwijderFout> {
    dictaten::verwijder(basis, id)?;
    if termijn != Termijn::Niet {
        vergeet_gewiste(geheugen, &[id.to_string()], basis, termijn, nu_ms);
    }
    Ok(())
}

/// Alles wissen: alle bestanden, en met 24 uur of 7 dagen ook het geheugen (wat nog staat
/// omdat wissen mislukte, wordt dan het laatste dictaat).
fn wis_alles_in(geheugen: &Geheugen, basis: &Path, termijn: Termijn, nu_ms: u64) -> Gewist {
    let gewist = dictaten::wis_alles(basis);
    if termijn != Termijn::Niet {
        *slot(geheugen) = None;
        vul_aan(geheugen, basis, termijn, nu_ms);
    }
    gewist
}

/// De tekst die "Laatste dictaat kopiëren" in het systeemvak kopieert, of `None` als er geen is.
pub(crate) fn tekst_voor_tray(geheugen: &Geheugen) -> Option<String> {
    slot(geheugen).as_ref().map(|l| l.tekst.clone())
}

// ---------------------------------------------------------------------------
// Met de app
// ---------------------------------------------------------------------------

fn map() -> Option<PathBuf> {
    dictaten::standaard_map()
}

fn meld_gewijzigd(app: &AppHandle) {
    let _ = app.emit(GEWIJZIGD, ());
}

/// Een nieuw dictaat is klaar (sneltoets of proefveld). Het geheugen en het systeemvak weten
/// het meteen; het schrijven naar schijf loopt op een eigen draad, zodat plakken niet op de
/// schijf wacht. Daarna meldt het event de lijst.
pub(crate) fn na_dictaat(app: &AppHandle, tekst: &str) {
    na_dictaat_met(app, tekst, None);
}

/// Een opgeschoond dictaat is klaar: `tekst` is wat geplakt wordt (en wat "Laatste dictaat
/// kopiëren" geeft); de letterlijke tekst en wie het opschoonde gaan mee naar schijf.
pub(crate) fn na_opgeschoond(
    app: &AppHandle,
    tekst: &str,
    letterlijk: &str,
    door: OpgeschoondDoor,
) {
    na_dictaat_met(app, tekst, Some((letterlijk.to_string(), door)));
}

fn na_dictaat_met(app: &AppHandle, tekst: &str, opgeschoond: Option<(String, OpgeschoondDoor)>) {
    let reeks = onthoud(&app.state::<DicteerStaat>().laatste_dictaat, tekst);
    crate::werk_tray_bij(app);
    let app = app.clone();
    let tekst = tekst.to_string();
    thread::spawn(move || {
        {
            let _slot = schijf_slot();
            // De termijn wordt onder het slot gelezen: een keuze voor Niet bewaren die net
            // gezet is, wint dan altijd van dit schrijven. Een schrijffout hoort het dictaat
            // niet in de weg te zitten: de tekst staat in het geheugen en de interface toont
            // hem dan als laatste dictaat.
            let _ = bewaar_op_schijf(
                &app.state::<DicteerStaat>().laatste_dictaat,
                map().as_deref(),
                lees_termijn(&app),
                &tekst,
                opgeschoond
                    .as_ref()
                    .map(|(letterlijk, door)| (letterlijk.as_str(), door)),
                dictaten::nu_ms(),
                reeks,
            );
        }
        meld_gewijzigd(&app);
    });
}

/// Een ronde opruimen, en de interface bijwerken als er iets verdween.
fn ruim_op(app: &AppHandle) {
    let Some(basis) = map() else {
        return;
    };
    let gewist = {
        let _slot = schijf_slot();
        ruim_op_in(
            &app.state::<DicteerStaat>().laatste_dictaat,
            &basis,
            lees_termijn(app),
            dictaten::nu_ms(),
        )
    };
    if gewist.aantal > 0 {
        crate::werk_tray_bij(app);
        meld_gewijzigd(app);
    }
}

/// Bij het starten van de app: het laatste dictaat terug in het geheugen, en daarna meteen en
/// elk uur de verlopen dictaten opruimen, op een eigen draad.
pub(crate) fn bij_start(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        // Eerst opruimen: het nieuwste dictaat dat we onthouden is er dan een dat nog geldt.
        ruim_op(&app);
        {
            let _slot = schijf_slot();
            herstel_geheugen(
                &app.state::<DicteerStaat>().laatste_dictaat,
                map().as_deref(),
                lees_termijn(&app),
                dictaten::nu_ms(),
            );
        }
        crate::werk_tray_bij(&app);
        meld_gewijzigd(&app);
        loop {
            thread::sleep(OPRUIMEN_ELK);
            ruim_op(&app);
        }
    });
}

// ---------------------------------------------------------------------------
// Commando's
// ---------------------------------------------------------------------------

/// Het laatste dictaat zoals de interface het ziet.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaatsteStand {
    tekst: String,
    /// Het id van het bestand; `None` als het niet op schijf staat.
    id: Option<String>,
}

/// Wat de interface van de geschiedenis nodig heeft.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DictatenStand {
    /// `niet`, `24u` of `7d`.
    termijn: &'static str,
    /// Nieuwste eerst; wat verlopen is staat er niet in. Leeg bij Niet bewaren.
    dictaten: Vec<Dictaat>,
    /// Het aantal dictaatbestanden dat nu op schijf staat, ook wat nog moet verlopen: zoveel
    /// gaat weg bij Niet bewaren of Alles wissen.
    bewaard: usize,
    /// Het laatste dictaat in het geheugen.
    laatste: Option<LaatsteStand>,
}

/// De termijn en de lijst, voor Dicteren en voor de bevestigvraag in Instellingen.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn dictaten_stand(app: AppHandle) -> DictatenStand {
    let termijn = lees_termijn(&app);
    let basis = map();
    let dictaten = match (termijn, basis.as_deref()) {
        (Termijn::Niet, _) | (_, None) => Vec::new(),
        (termijn, Some(basis)) => dictaten::lijst(basis, termijn, dictaten::nu_ms()),
    };
    let laatste = slot(&app.state::<DicteerStaat>().laatste_dictaat)
        .as_ref()
        .map(|l| LaatsteStand {
            tekst: l.tekst.clone(),
            id: l.id.clone(),
        });
    DictatenStand {
        termijn: termijn.sleutel(),
        dictaten,
        bewaard: basis.as_deref().map_or(0, dictaten::aantal),
        laatste,
    }
}

/// Bewaart een andere termijn en past hem meteen toe: korter wist wat ouder is, Niet bewaren
/// wist alles (de interface heeft daar eerst een bevestigvraag voor gesteld).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de argumenten per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn dictaten_bewaren_zet(app: AppHandle, termijn: String) -> Result<(), Foutmelding> {
    let Some(nieuw) = Termijn::uit_sleutel(&termijn) else {
        return Err(Foutmelding::nieuw(
            "Deze keuze bestaat niet. Kies Niet bewaren, 24 uur of 7 dagen.",
            Vervolg::Geen,
        ));
    };
    let mislukt = {
        let _slot = schijf_slot();
        // Een schrijffout wordt doorgegeven: wie de keuze ziet staan, moet erop kunnen rekenen
        // dat ze na een herstart nog geldt.
        let store = app.store(INSTELLINGEN).map_err(|_| schrijffout())?;
        let oud = store.get(SLEUTEL);
        store.set(SLEUTEL, nieuw.sleutel());
        if store.save().is_err() {
            match oud {
                Some(waarde) => store.set(SLEUTEL, waarde),
                None => {
                    store.delete(SLEUTEL);
                }
            }
            return Err(schrijffout());
        }
        map().is_some_and(|basis| {
            ruim_op_in(
                &app.state::<DicteerStaat>().laatste_dictaat,
                &basis,
                nieuw,
                dictaten::nu_ms(),
            )
            .mislukt
                > 0
        })
    };
    crate::werk_tray_bij(&app);
    meld_gewijzigd(&app);
    if mislukt {
        return Err(wisfout());
    }
    Ok(())
}

/// Verwijdert één dictaat (direct, geen prullenbak van Windows).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de argumenten per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn dictaat_verwijder(app: AppHandle, id: String) -> Result<(), Foutmelding> {
    let basis = map().ok_or_else(wisfout)?;
    let uitkomst = {
        let _slot = schijf_slot();
        verwijder_in(
            &app.state::<DicteerStaat>().laatste_dictaat,
            &basis,
            lees_termijn(&app),
            dictaten::nu_ms(),
            &id,
        )
    };
    crate::werk_tray_bij(&app);
    meld_gewijzigd(&app);
    uitkomst.map_err(|fout| Foutmelding::nieuw(fout.to_string(), Vervolg::Geen))
}

/// Wist alle bewaarde dictaten, en met 24 uur of 7 dagen het laatste dictaat in het geheugen.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn dictaten_wis_alles(app: AppHandle) -> Result<(), Foutmelding> {
    let basis = map().ok_or_else(wisfout)?;
    let gewist = {
        let _slot = schijf_slot();
        wis_alles_in(
            &app.state::<DicteerStaat>().laatste_dictaat,
            &basis,
            lees_termijn(&app),
            dictaten::nu_ms(),
        )
    };
    crate::werk_tray_bij(&app);
    meld_gewijzigd(&app);
    if gewist.mislukt > 0 {
        return Err(wisfout());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    /// Een tijdelijke map die bij het einde van de test verdwijnt. Eigen hulpje om geen
    /// dev-afhankelijkheid toe te voegen.
    struct Map(PathBuf);

    impl Map {
        fn nieuw() -> Self {
            static TELLER: AtomicU32 = AtomicU32::new(0);
            let pad = std::env::temp_dir().join(format!(
                "mondig-dictaten-test-{}-{}",
                std::process::id(),
                TELLER.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&pad).unwrap();
            Self(pad)
        }
    }

    impl Drop for Map {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const DAG: u64 = 86_400_000;

    fn geheugen() -> Geheugen {
        Mutex::new(None)
    }

    /// Een dictaat zoals `na_dictaat` het vastlegt, maar op dezelfde draad.
    fn leg_vast(
        mem: &Geheugen,
        basis: Option<&Path>,
        termijn: Termijn,
        tekst: &str,
        nu: u64,
    ) -> std::io::Result<()> {
        let reeks = onthoud(mem, tekst);
        bewaar_op_schijf(mem, basis, termijn, tekst, None, nu, reeks)
    }

    fn laatste_id(mem: &Geheugen) -> Option<String> {
        slot(mem).as_ref().and_then(|l| l.id.clone())
    }

    #[test]
    fn een_ontbrekende_of_ongeldige_instelling_geeft_24_uur() {
        assert_eq!(termijn_uit(None), Termijn::Dag);
        assert_eq!(termijn_uit(Some("")), Termijn::Dag);
        assert_eq!(termijn_uit(Some("30d")), Termijn::Dag);
        assert_eq!(termijn_uit(Some("niet")), Termijn::Niet);
        assert_eq!(termijn_uit(Some("24u")), Termijn::Dag);
        assert_eq!(termijn_uit(Some("7d")), Termijn::Week);
    }

    #[test]
    fn het_systeemvak_heeft_tekst_in_alle_drie_de_standen() {
        for termijn in [Termijn::Niet, Termijn::Dag, Termijn::Week] {
            let map = Map::nieuw();
            let mem = geheugen();
            assert_eq!(tekst_voor_tray(&mem), None, "{termijn:?} begint leeg");
            leg_vast(&mem, Some(&map.0), termijn, "Een dictaat.", 5 * DAG).unwrap();
            assert_eq!(
                tekst_voor_tray(&mem).as_deref(),
                Some("Een dictaat."),
                "{termijn:?}"
            );
        }
    }

    #[test]
    fn het_geheugen_krijgt_het_id_van_het_bestand_behalve_bij_niet_bewaren() {
        let map = Map::nieuw();
        let mem = geheugen();
        leg_vast(&mem, Some(&map.0), Termijn::Dag, "Bewaard.", 5 * DAG).unwrap();
        assert_eq!(
            laatste_id(&mem),
            dictaten::lijst(&map.0, Termijn::Dag, 5 * DAG)
                .first()
                .map(|d| d.id.clone())
        );
        leg_vast(&mem, Some(&map.0), Termijn::Niet, "Niet bewaard.", 5 * DAG).unwrap();
        assert_eq!(laatste_id(&mem), None);
    }

    #[test]
    fn niet_bewaren_schrijft_niets_naar_schijf() {
        let map = Map::nieuw();
        let mem = geheugen();
        leg_vast(&mem, Some(&map.0), Termijn::Niet, "Geheim.", 5 * DAG).unwrap();
        assert!(dictaten::lijst(&map.0, Termijn::Week, 5 * DAG).is_empty());
        assert_eq!(dictaten::aantal(&map.0), 0);
    }

    #[test]
    fn een_schrijffout_laat_het_geheugen_gevuld_zonder_id() {
        let mem = geheugen();
        assert!(leg_vast(&mem, None, Termijn::Dag, "Toch bewaard.", 5 * DAG).is_err());
        assert_eq!(tekst_voor_tray(&mem).as_deref(), Some("Toch bewaard."));
        assert_eq!(laatste_id(&mem), None);
    }

    #[test]
    fn een_late_schrijfactie_van_een_ouder_dictaat_overschrijft_het_id_niet() {
        let map = Map::nieuw();
        let mem = geheugen();
        let oud = onthoud(&mem, "Oud.");
        // Een nieuwer dictaat staat al in het geheugen voor het oude naar schijf is.
        let nieuw = onthoud(&mem, "Nieuw.");
        bewaar_op_schijf(&mem, Some(&map.0), Termijn::Dag, "Oud.", None, 5 * DAG, oud).unwrap();
        assert_eq!(laatste_id(&mem), None);
        bewaar_op_schijf(
            &mem,
            Some(&map.0),
            Termijn::Dag,
            "Nieuw.",
            None,
            5 * DAG + 1,
            nieuw,
        )
        .unwrap();
        assert!(laatste_id(&mem).is_some());
    }

    #[test]
    fn een_opgeschoond_dictaat_gaat_met_de_letterlijke_tekst_naar_schijf() {
        let map = Map::nieuw();
        let mem = geheugen();
        let reeks = onthoud(&mem, "We zien elkaar woensdag.");
        let door = OpgeschoondDoor {
            model: "mistral-small-latest".into(),
            aanbieder: "mistral".into(),
            label: Some(mondig_core::soevereiniteit::Soevereiniteit::Europees),
        };
        bewaar_op_schijf(
            &mem,
            Some(&map.0),
            Termijn::Dag,
            "We zien elkaar woensdag.",
            Some(("ehm we zien elkaar dinsdag nee woensdag", &door)),
            5 * DAG,
            reeks,
        )
        .unwrap();
        // Het systeemvak geeft de geplakte versie.
        assert_eq!(
            tekst_voor_tray(&mem).as_deref(),
            Some("We zien elkaar woensdag.")
        );
        let lijst = dictaten::lijst(&map.0, Termijn::Dag, 5 * DAG);
        assert_eq!(lijst.len(), 1);
        assert_eq!(
            lijst[0].letterlijk.as_deref(),
            Some("ehm we zien elkaar dinsdag nee woensdag")
        );
        assert_eq!(lijst[0].opgeschoond_door.as_ref(), Some(&door));
    }

    #[test]
    fn na_een_herstart_met_24_uur_of_7_dagen_staat_het_nieuwste_in_het_geheugen() {
        for termijn in [Termijn::Dag, Termijn::Week] {
            let map = Map::nieuw();
            let eerste = geheugen();
            leg_vast(&eerste, Some(&map.0), termijn, "Oudste.", 5 * DAG).unwrap();
            leg_vast(&eerste, Some(&map.0), termijn, "Nieuwste.", 5 * DAG + 1000).unwrap();
            let na_herstart = geheugen();
            herstel_geheugen(&na_herstart, Some(&map.0), termijn, 5 * DAG + 2000);
            assert_eq!(
                tekst_voor_tray(&na_herstart).as_deref(),
                Some("Nieuwste."),
                "{termijn:?}"
            );
            assert!(laatste_id(&na_herstart).is_some());
        }
    }

    #[test]
    fn na_een_herstart_met_niet_bewaren_is_er_niets() {
        let map = Map::nieuw();
        let eerste = geheugen();
        leg_vast(&eerste, Some(&map.0), Termijn::Niet, "Weg.", 5 * DAG).unwrap();
        let na_herstart = geheugen();
        herstel_geheugen(&na_herstart, Some(&map.0), Termijn::Niet, 5 * DAG + 1);
        assert_eq!(tekst_voor_tray(&na_herstart), None);
    }

    #[test]
    fn een_verlopen_dictaat_wordt_niet_teruggezet_bij_het_starten() {
        let map = Map::nieuw();
        let mem = geheugen();
        leg_vast(&mem, Some(&map.0), Termijn::Dag, "Gisteren.", 5 * DAG).unwrap();
        let na_herstart = geheugen();
        herstel_geheugen(&na_herstart, Some(&map.0), Termijn::Dag, 6 * DAG + 1);
        assert_eq!(tekst_voor_tray(&na_herstart), None);
    }

    #[test]
    fn na_alles_wissen_kopieert_het_systeemvak_niets_meer() {
        for termijn in [Termijn::Dag, Termijn::Week] {
            let map = Map::nieuw();
            let mem = geheugen();
            leg_vast(&mem, Some(&map.0), termijn, "Een.", 5 * DAG).unwrap();
            leg_vast(&mem, Some(&map.0), termijn, "Twee.", 5 * DAG + 1).unwrap();
            let gewist = wis_alles_in(&mem, &map.0, termijn, 5 * DAG + 2);
            assert_eq!(gewist.aantal, 2);
            assert_eq!(tekst_voor_tray(&mem), None, "{termijn:?}");
            let na_herstart = geheugen();
            herstel_geheugen(&na_herstart, Some(&map.0), termijn, 5 * DAG + 2);
            assert_eq!(tekst_voor_tray(&na_herstart), None);
        }
    }

    #[test]
    fn verwijderen_van_het_laatste_dictaat_vult_het_geheugen_met_het_nieuwste_dat_nog_staat() {
        let map = Map::nieuw();
        let mem = geheugen();
        leg_vast(&mem, Some(&map.0), Termijn::Dag, "Eerder.", 5 * DAG).unwrap();
        leg_vast(&mem, Some(&map.0), Termijn::Dag, "Laatste.", 5 * DAG + 1).unwrap();
        let lijst = dictaten::lijst(&map.0, Termijn::Dag, 5 * DAG + 2);
        // Een ouder dictaat verwijderen laat het geheugen met rust.
        verwijder_in(&mem, &map.0, Termijn::Dag, 5 * DAG + 2, &lijst[1].id).unwrap();
        assert_eq!(tekst_voor_tray(&mem).as_deref(), Some("Laatste."));
        // Het laatste verwijderen: er blijft niets over, dus ook niets in het geheugen.
        verwijder_in(&mem, &map.0, Termijn::Dag, 5 * DAG + 2, &lijst[0].id).unwrap();
        assert_eq!(tekst_voor_tray(&mem), None);
    }

    #[test]
    fn verwijderen_van_het_laatste_laat_een_ouder_dictaat_het_laatste_worden() {
        let map = Map::nieuw();
        let mem = geheugen();
        leg_vast(&mem, Some(&map.0), Termijn::Week, "Ouder.", 5 * DAG).unwrap();
        leg_vast(&mem, Some(&map.0), Termijn::Week, "Laatste.", 5 * DAG + 1).unwrap();
        let id = laatste_id(&mem).unwrap();
        verwijder_in(&mem, &map.0, Termijn::Week, 5 * DAG + 2, &id).unwrap();
        assert_eq!(tekst_voor_tray(&mem).as_deref(), Some("Ouder."));
        assert!(laatste_id(&mem).is_some());
    }

    #[test]
    fn twee_dictaten_met_dezelfde_tekst_worden_op_id_onderscheiden() {
        let map = Map::nieuw();
        let mem = geheugen();
        leg_vast(&mem, Some(&map.0), Termijn::Week, "Ja.", 5 * DAG).unwrap();
        leg_vast(&mem, Some(&map.0), Termijn::Week, "Ja.", 5 * DAG + 1000).unwrap();
        let lijst = dictaten::lijst(&map.0, Termijn::Week, 5 * DAG + 2000);
        assert_eq!(lijst.len(), 2);
        // Het oudere verwijderen raakt het geheugen niet, ook al is de tekst gelijk.
        verwijder_in(&mem, &map.0, Termijn::Week, 5 * DAG + 2000, &lijst[1].id).unwrap();
        assert_eq!(laatste_id(&mem).as_deref(), Some(lijst[0].id.as_str()));
        // Automatisch verlopen van het oudste laat het nieuwere in het geheugen staan.
        let map2 = Map::nieuw();
        let mem2 = geheugen();
        leg_vast(&mem2, Some(&map2.0), Termijn::Dag, "Ja.", 5 * DAG).unwrap();
        leg_vast(
            &mem2,
            Some(&map2.0),
            Termijn::Dag,
            "Ja.",
            5 * DAG + 3_600_000,
        )
        .unwrap();
        ruim_op_in(&mem2, &map2.0, Termijn::Dag, 6 * DAG);
        assert_eq!(dictaten::aantal(&map2.0), 1);
        assert_eq!(tekst_voor_tray(&mem2).as_deref(), Some("Ja."));
        assert!(laatste_id(&mem2).is_some());
    }

    #[test]
    fn een_kwaadaardig_id_wordt_geweigerd_en_laat_het_geheugen_staan() {
        let map = Map::nieuw();
        let mem = geheugen();
        leg_vast(&mem, Some(&map.0), Termijn::Dag, "Blijft.", 5 * DAG).unwrap();
        assert_eq!(
            verwijder_in(&mem, &map.0, Termijn::Dag, 5 * DAG, "../x.json"),
            Err(VerwijderFout::OngeldigId)
        );
        assert_eq!(tekst_voor_tray(&mem).as_deref(), Some("Blijft."));
    }

    #[test]
    fn automatisch_verlopen_haalt_het_dictaat_ook_uit_het_geheugen() {
        let map = Map::nieuw();
        let mem = geheugen();
        leg_vast(&mem, Some(&map.0), Termijn::Dag, "Verloopt.", 5 * DAG).unwrap();
        assert_eq!(
            ruim_op_in(&mem, &map.0, Termijn::Dag, 6 * DAG - 1).aantal,
            0
        );
        assert_eq!(tekst_voor_tray(&mem).as_deref(), Some("Verloopt."));
        assert_eq!(ruim_op_in(&mem, &map.0, Termijn::Dag, 6 * DAG).aantal, 1);
        assert_eq!(tekst_voor_tray(&mem), None);
    }

    #[test]
    fn een_dictaat_zonder_id_blijft_in_het_geheugen_bij_een_wisronde() {
        let map = Map::nieuw();
        let mem = geheugen();
        onthoud(&mem, "Schrijven mislukt.");
        ruim_op_in(&mem, &map.0, Termijn::Dag, 9 * DAG);
        assert_eq!(tekst_voor_tray(&mem).as_deref(), Some("Schrijven mislukt."));
    }

    #[test]
    fn overschakelen_naar_niet_bewaren_wist_de_bestanden_maar_houdt_het_laatste() {
        let map = Map::nieuw();
        let mem = geheugen();
        leg_vast(&mem, Some(&map.0), Termijn::Week, "Een.", 5 * DAG).unwrap();
        leg_vast(&mem, Some(&map.0), Termijn::Week, "Twee.", 5 * DAG + 1).unwrap();
        let gewist = ruim_op_in(&mem, &map.0, Termijn::Niet, 5 * DAG + 2);
        assert_eq!(gewist.aantal, 2);
        assert_eq!(dictaten::aantal(&map.0), 0);
        assert_eq!(tekst_voor_tray(&mem).as_deref(), Some("Twee."));
    }

    #[test]
    fn korter_zetten_wist_wat_ouder_is_en_houdt_wat_nog_geldt() {
        let map = Map::nieuw();
        let mem = geheugen();
        leg_vast(
            &mem,
            Some(&map.0),
            Termijn::Week,
            "Drie dagen oud.",
            3 * DAG,
        )
        .unwrap();
        leg_vast(&mem, Some(&map.0), Termijn::Week, "Van vandaag.", 6 * DAG).unwrap();
        let gewist = ruim_op_in(&mem, &map.0, Termijn::Dag, 6 * DAG + 1);
        assert_eq!(gewist.aantal, 1);
        assert_eq!(tekst_voor_tray(&mem).as_deref(), Some("Van vandaag."));
        assert_eq!(dictaten::lijst(&map.0, Termijn::Dag, 6 * DAG + 1).len(), 1);
    }
}
