//! De schakelaar "Videokaart gebruiken" (2-11-2026, ADR 0011 aanvulling): een tussenstap tot
//! de hardwarecheck van fase 5 het zelf beslist.
//!
//! - De keuze staat in de store (sleutel `videokaart`); ontbreekt hij, dan rekent Mondig op
//!   de processor (de standaard, ook voor oude instellingenbestanden).
//! - Dicteren en Bestanden vragen het apparaat hier op ([`gewenst_apparaat`]). Omzetten laat
//!   een motor die niets doet los of zet hem achter in de wachtrij; een lopend dictaat of
//!   bestand maakt het af op de oude motor.
//! - Elke motor meldt waarop hij werkelijk rekent ([`melder`]); het event
//!   `apparaat-gewijzigd` stuurt de samenvatting naar de interface, `apparaat_in_gebruik`
//!   vraagt haar op. Na een terugval blijft een motor op de processor tot de schakelaar
//!   opnieuw omgaat of Mondig herstart.
//! - Of er een videokaart is, vraagt Mondig aan een eigen, kortlevend werkproces
//!   ([`Supervisor::apparaten`]): een crash in de driver raakt de app niet (ADR 0009).
//! - "Snelheid testen" meet het bestandsmodel op de processor en op de videokaart
//!   (`mondig_core::snelheid`).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use mondig_core::aanbieder::{LokaleEngine, Rekenstand};
use mondig_core::download::{self, ModelStatus};
use mondig_core::engine::protocol::{Apparaat, Videokaart};
use mondig_core::engine::supervisor::{Instellingen, Supervisor};
use mondig_core::logboek::{self, Gebeurtenis};
use mondig_core::model::Gebruik;
use mondig_core::snelheid::{self, Vergelijking};
use mondig_core::taak::{Foutmelding, Vervolg};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::EnginePad;
use crate::dicteerketen::{DicteerStaat, slot};
use crate::sneltoets::{lees_instelling, schrijf_instelling, wis_instelling};

/// Sleutel: "aan" als de gebruiker de videokaart wil gebruiken; ontbreekt hij, dan niet.
const SLEUTEL_VIDEOKAART: &str = "videokaart";
const AAN: &str = "aan";

/// Het event met de samenvatting van [`ApparaatStaat`].
const EVENT: &str = "apparaat-gewijzigd";

// ---------------------------------------------------------------------------
// De keuze
// ---------------------------------------------------------------------------

/// Staat de schakelaar aan, bij deze bewaarde waarde? Niets bewaard is uit.
fn aan_uit_bewaard(bewaard: Option<&str>) -> bool {
    bewaard == Some(AAN)
}

/// Wat er bewaard wordt: "aan", of niets (wissen), zodat uit gelijk is aan de standaard.
fn bewaarwaarde(aan: bool) -> Option<&'static str> {
    aan.then_some(AAN)
}

/// Het apparaat dat de motoren krijgen bij deze stand van de schakelaar.
fn apparaat_voor(aan: bool) -> Apparaat {
    if aan { Apparaat::Gpu } else { Apparaat::Cpu }
}

/// Staat de schakelaar aan?
pub(crate) fn lees_aan(app: &AppHandle) -> bool {
    aan_uit_bewaard(lees_instelling(app, SLEUTEL_VIDEOKAART).as_deref())
}

/// Het apparaat voor een nieuwe motor (dicteren en bestanden).
pub(crate) fn gewenst_apparaat(app: &AppHandle) -> Apparaat {
    apparaat_voor(lees_aan(app))
}

// ---------------------------------------------------------------------------
// Wat er werkelijk gebruikt wordt
// ---------------------------------------------------------------------------

/// Welke motor iets meldt.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Route {
    Dicteren,
    Bestanden,
}

/// Of Mondig al naar videokaarten zocht, en wat eruit kwam.
#[derive(Default)]
enum Zoektocht {
    #[default]
    NogNiet,
    /// De gevonden kaarten (leeg als er geen is), of `None` als zoeken niet lukte.
    Klaar(Option<Vec<Videokaart>>),
}

/// Wat de app over de videokaart onthoudt.
#[derive(Default)]
pub(crate) struct ApparaatStaat {
    /// Gaat omhoog bij elke stand van de schakelaar: een oude motor die nog een dictaat
    /// afmaakt, meldt dan niets meer.
    generatie: AtomicU64,
    dicteren: Mutex<Option<Rekenstand>>,
    bestanden: Mutex<Option<Rekenstand>>,
    /// De videokaarten van deze sessie: één keer gezocht.
    kaarten: Mutex<Zoektocht>,
    /// Loopt de snelheidstest?
    test_bezig: AtomicBool,
}

impl ApparaatStaat {
    fn samen(&self) -> Option<Rekenstand> {
        samenvatting(*slot(&self.dicteren), *slot(&self.bestanden))
    }
}

/// Wat de regel "In gebruik" zegt bij de stand van beide motoren: een terugval gaat voor (dat
/// moet de gebruiker weten), dan de videokaart, dan de processor; niets als geen van beide
/// ooit laadde.
fn samenvatting(dicteren: Option<Rekenstand>, bestanden: Option<Rekenstand>) -> Option<Rekenstand> {
    let rang = |stand: &Rekenstand| match stand {
        Rekenstand::Terugval => 2,
        Rekenstand::Videokaart => 1,
        Rekenstand::Processor => 0,
    };
    [dicteren, bestanden].into_iter().flatten().max_by_key(rang)
}

/// De melder voor een nieuwe motor: zet zijn stand in [`ApparaatStaat`] en stuurt de
/// samenvatting naar de interface, zolang de schakelaar niet intussen omging.
pub(crate) fn melder(app: &AppHandle, route: Route) -> impl Fn(Rekenstand) + Send + 'static {
    let generatie = app
        .state::<ApparaatStaat>()
        .generatie
        .load(Ordering::SeqCst);
    let app = app.clone();
    move |stand| {
        let staat = app.state::<ApparaatStaat>();
        if staat.generatie.load(Ordering::SeqCst) != generatie {
            return;
        }
        let veld = match route {
            Route::Dicteren => &staat.dicteren,
            Route::Bestanden => &staat.bestanden,
        };
        *slot(veld) = Some(stand);
        if stand == Rekenstand::Terugval {
            logboek::log(&Gebeurtenis::VideokaartTerugval);
        }
        let _ = app.emit(EVENT, staat.samen());
    }
}

/// Waarop Mondig nu rekent, voor de regel onder de schakelaar en "Versiegegevens kopiëren".
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn apparaat_in_gebruik(app: AppHandle) -> Option<Rekenstand> {
    app.state::<ApparaatStaat>().samen()
}

// ---------------------------------------------------------------------------
// Schakelaar en zoeken
// ---------------------------------------------------------------------------

/// Wat de schakelaar nodig heeft.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideokaartInfo {
    aan: bool,
    /// De gevonden videokaarten, leeg als er geen is; `None` als zoeken niet lukte.
    kaarten: Option<Vec<Videokaart>>,
}

/// Zoekt de videokaarten in een eigen werkproces, één keer per sessie. Ook een mislukte
/// zoektocht wordt onthouden: crasht de driver, dan niet bij elk openen van Instellingen.
pub(crate) fn kaarten(app: &AppHandle) -> Option<Vec<Videokaart>> {
    let staat = app.state::<ApparaatStaat>();
    // Het slot blijft vast tijdens het zoeken: wie tegelijk vraagt, wacht op dezelfde uitkomst.
    let mut onthouden = slot(&staat.kaarten);
    if let Zoektocht::Klaar(kaarten) = &*onthouden {
        return kaarten.clone();
    }
    let gevonden = app.state::<EnginePad>().0.clone().and_then(|pad| {
        let mut instellingen = Instellingen::achtergrond(pad);
        instellingen.bouwcontrole = crate::versie::bouwcontrole();
        Supervisor::nieuw(instellingen)
            .apparaten()
            .inspect_err(|fout| eprintln!("[mondig] videokaart zoeken mislukt: {fout}"))
            .ok()
    });
    *onthouden = Zoektocht::Klaar(gevonden.clone());
    gevonden
}

/// De stand van de schakelaar en de gevonden videokaarten.
#[tauri::command]
pub(crate) async fn videokaart_info(app: AppHandle) -> Result<VideokaartInfo, Foutmelding> {
    tauri::async_runtime::spawn_blocking(move || VideokaartInfo {
        aan: lees_aan(&app),
        kaarten: kaarten(&app),
    })
    .await
    .map_err(|_| {
        Foutmelding::nieuw(
            "Het zoeken naar de videokaart stopte onverwacht.",
            Vervolg::OpnieuwProberen,
        )
    })
}

/// Bewaart de keuze en laat hem meteen gelden, zonder herstart: een dicteermotor die niets
/// doet gaat los (bij "Altijd" laadt de nieuwe meteen), de wachtrij krijgt een nieuwe motor
/// achteraan. Wat nu loopt, maakt het af op de oude motor. De regel "In gebruik" begint
/// opnieuw, ook na een terugval.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn videokaart_zet(app: AppHandle, aan: bool) {
    logboek::log(&Gebeurtenis::VideokaartKeuze { aan });
    match bewaarwaarde(aan) {
        Some(waarde) => schrijf_instelling(&app, SLEUTEL_VIDEOKAART, waarde),
        None => wis_instelling(&app, SLEUTEL_VIDEOKAART),
    }
    let staat = app.state::<ApparaatStaat>();
    staat.generatie.fetch_add(1, Ordering::SeqCst);
    *slot(&staat.dicteren) = None;
    *slot(&staat.bestanden) = None;
    let _ = app.emit(EVENT, None::<Rekenstand>);
    // De schakelaar in Instellingen volgt: ook als Mondig opnieuw afstellen de kaart aanzet.
    let _ = app.emit("videokaart-gewijzigd", aan);
    crate::dicteerketen::ververs_beheerder(&app);
    crate::bestanden::ververs_wachtrij(&app);
}

// ---------------------------------------------------------------------------
// Snelheid testen
// ---------------------------------------------------------------------------

/// De melding als er iets wil uitschrijven terwijl de snelheidstest loopt.
const MELDING_TEST_LOOPT: &str =
    "Mondig test nu de snelheid van de videokaart. Probeer het over een halve minuut opnieuw.";

/// De melding als de dicteertoets ingaat terwijl de snelheidstest loopt.
pub(crate) const MELDING_DICTEREN_TIJDENS_TEST: &str =
    "Mondig test de snelheid van de videokaart. Dicteer zo weer.";

/// Loopt de snelheidstest? Eigen vlag van de test, los van de proef.
pub(crate) fn test_loopt(app: &AppHandle) -> bool {
    app.try_state::<ApparaatStaat>()
        .is_some_and(|staat| staat.test_bezig.load(Ordering::SeqCst))
}

/// Weigert nieuw werk voor de wachtrij (bestanden, vergaderingen) zolang de test loopt.
///
/// # Errors
///
/// Geeft een melding als de snelheidstest loopt.
pub(crate) fn weiger_tijdens_test(app: &AppHandle) -> Result<(), Foutmelding> {
    if test_loopt(app) {
        Err(Foutmelding::nieuw(
            MELDING_TEST_LOOPT,
            Vervolg::OpnieuwProberen,
        ))
    } else {
        Ok(())
    }
}

/// Houdt de test bezet, en het dictaat tegen via de vlag van de proef (een dictaat start niet
/// zolang die staat); geeft beide vrij bij elke uitgang.
struct TestBezet {
    app: AppHandle,
    proef: bool,
}

impl Drop for TestBezet {
    fn drop(&mut self) {
        if self.proef {
            self.app
                .state::<DicteerStaat>()
                .proef_bezig
                .store(false, Ordering::SeqCst);
        }
        self.app
            .state::<ApparaatStaat>()
            .test_bezig
            .store(false, Ordering::SeqCst);
    }
}

fn testfout(melding: &str) -> Foutmelding {
    Foutmelding::nieuw(melding, Vervolg::Geen)
}

/// Bezet de test, of zegt waarom het nu niet kan.
fn bezet_test(app: &AppHandle) -> Result<TestBezet, Foutmelding> {
    let staat = app.state::<ApparaatStaat>();
    if staat
        .test_bezig
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(testfout("De snelheidstest loopt al."));
    }
    let mut bezet = TestBezet {
        app: app.clone(),
        proef: false,
    };
    // Eerst de proefvlag zetten, dan naar het dictaat kijken: het dictaat doet het omgekeerde
    // (zie `ProefBezet` in dicteerketen.rs).
    let dicteer = app.state::<DicteerStaat>();
    if dicteer
        .proef_bezig
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(testfout(
            "Er loopt een proef. Test de snelheid als die klaar is.",
        ));
    }
    bezet.proef = true;
    if dicteer.dictaat_bezig.load(Ordering::SeqCst) {
        return Err(testfout(
            "Er loopt een dictaat. Test de snelheid als het klaar is.",
        ));
    }
    Ok(bezet)
}

/// Meet het bestandsmodel op de processor en op de videokaart en zegt of aanzetten loont.
#[tauri::command]
pub(crate) async fn snelheidstest_start(app: AppHandle) -> Result<Vergelijking, Foutmelding> {
    tauri::async_runtime::spawn_blocking(move || snelheidstest(&app))
        .await
        .map_err(|_| testfout("De snelheidstest stopte onverwacht."))?
}

fn snelheidstest(app: &AppHandle) -> Result<Vergelijking, Foutmelding> {
    let _bezet = bezet_test(app)?;
    let (model, map) = crate::dicteerketen::model_voor(app, Gebruik::Bestanden)?;
    if app.state::<crate::bestanden::BestandenStaat>().is_bezig()
        || crate::vergadering::schrijft_uit(app, &model.id)
    {
        return Err(testfout(
            "Mondig schrijft nu iets uit. Test de snelheid als dat klaar is.",
        ));
    }
    let Some(engine_pad) = app.state::<EnginePad>().0.clone() else {
        return Err(testfout(
            "Het spraakprogramma ontbreekt bij Mondig. Installeer Mondig opnieuw.",
        ));
    };
    if download::status(&model, &map) != ModelStatus::Compleet {
        return Err(testfout(
            "Download eerst een spraakmodel voor bestanden; daarmee test Mondig de snelheid.",
        ));
    }
    if kaarten(app).is_none_or(|kaarten| kaarten.is_empty()) {
        return Err(testfout("Geen geschikte videokaart gevonden."));
    }
    // Het dicteermodel gaat uit het videogeheugen: de test mag niet falen omdat dat vol zit.
    // Dicteren laadt het model weer bij het volgende dictaat; dat kan nu niet starten, want
    // de test houdt de proefvlag vast.
    crate::dicteerketen::laat_dicteermotor_los(app);
    let geluid = snelheid::testgeluid(snelheid::TESTDUUR);
    let meet = |apparaat: Apparaat| {
        // Zoals elke motor: het model mag niet van schijf zolang de test het gebruikt.
        let _aanmelding = crate::modelgebruik::GEBRUIK.meld_aan(&model.id);
        let mut instellingen = Instellingen::achtergrond(engine_pad.clone());
        instellingen.bouwcontrole = crate::versie::bouwcontrole();
        let mut engine = LokaleEngine::nieuw(
            instellingen,
            &model,
            map.clone(),
            apparaat,
            Some("nl".into()),
        );
        let meting = snelheid::meet(&mut engine, &geluid);
        engine.stop();
        meting
    };
    let processor = meet(Apparaat::Cpu).map_err(|fout| {
        Foutmelding::nieuw(
            format!("De snelheidstest lukte niet op de processor: {fout}"),
            Vervolg::OpnieuwProberen,
        )
    })?;
    let videokaart = meet(Apparaat::Gpu)
        .inspect_err(|fout| eprintln!("[mondig] snelheidstest op de videokaart: {fout}"))
        .ok();
    let uitkomst = snelheid::vergelijk(
        processor.realtime_factor(),
        videokaart.as_ref().and_then(snelheid::op_videokaart),
    );
    logboek::log(&Gebeurtenis::Snelheidstest {
        oordeel: uitkomst.oordeel.code(),
        keer_sneller_tienden: uitkomst.keer_sneller_tienden(),
    });
    Ok(uitkomst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standaard_en_oude_instellingen_rekenen_op_de_processor() {
        assert!(!aan_uit_bewaard(None));
        assert!(!aan_uit_bewaard(Some("uit")));
        assert!(!aan_uit_bewaard(Some("onzin")));
        assert_eq!(apparaat_voor(aan_uit_bewaard(None)), Apparaat::Cpu);
    }

    #[test]
    fn de_keuze_gaat_heen_en_terug_door_de_store() {
        for aan in [true, false] {
            assert_eq!(aan_uit_bewaard(bewaarwaarde(aan)), aan);
        }
        // Uit wordt gewist, niet als "uit" bewaard: dan is uit gelijk aan de standaard.
        assert_eq!(bewaarwaarde(false), None);
    }

    #[test]
    fn de_schakelaar_bepaalt_het_apparaat_van_de_motor() {
        assert_eq!(apparaat_voor(true), Apparaat::Gpu);
        assert_eq!(apparaat_voor(false), Apparaat::Cpu);
    }

    #[test]
    fn de_regel_in_gebruik_zet_een_terugval_voorop() {
        use Rekenstand::{Processor, Terugval, Videokaart};
        assert_eq!(samenvatting(None, None), None);
        assert_eq!(samenvatting(Some(Processor), None), Some(Processor));
        assert_eq!(samenvatting(None, Some(Videokaart)), Some(Videokaart));
        assert_eq!(
            samenvatting(Some(Videokaart), Some(Processor)),
            Some(Videokaart)
        );
        assert_eq!(
            samenvatting(Some(Videokaart), Some(Terugval)),
            Some(Terugval)
        );
        assert_eq!(
            samenvatting(Some(Terugval), Some(Processor)),
            Some(Terugval)
        );
    }
}
