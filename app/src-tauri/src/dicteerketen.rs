//! Commando's en gebeurtenissen voor de proef op het dicteerscherm en de dicteermotor
//! (sprint 3a, ADR 0012). De logica zit in `mondig-core`; dit bestand vertaalt opdrachten
//! van de interface en stuurt gebeurtenissen terug (de downloads staan in `downloads.rs`):
//!
//! - event `taak`: elke [`Gebeurtenis`] van een dictaat;
//! - event `proef-resultaat`: de uitslag van een proef.
//!
//! Transcripttekst gaat nooit naar een log; hij wordt alleen als event verstuurd en in het
//! geheugen bewaard als laatste dictaat.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de State per waarde mee; een verwijzing kan niet"
)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::time::{Duration, Instant};

use mondig_core::aanbieder::LokaleEngine;
use mondig_core::download::{self, ModelStatus};
use mondig_core::engine::beheer::{BeheerFout, Motorbeheerder, Rusttijd, Uitslag};
use mondig_core::engine::protocol::Apparaat;
use mondig_core::engine::supervisor::Instellingen;
use mondig_core::model::{Gebruik, Model};
use mondig_core::proef::{self, ProefResultaat};
use mondig_core::soevereiniteit::Soevereiniteit;
use mondig_core::taak::{Foutmelding, Gebeurtenis, TaakId, Vervolg};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::EnginePad;
use crate::modelgebruik::Gevolgd;
use crate::modellen;

/// Waarmee een beheerder draait: model en apparaat. Verandert een van de twee (een ander
/// dicteermodel, of de schakelaar "Videokaart gebruiken"), dan maakt het volgende dictaat een
/// nieuwe beheerder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MotorSleutel {
    model: String,
    apparaat: Apparaat,
}

impl MotorSleutel {
    pub(crate) fn nieuw(model: &str, apparaat: Apparaat) -> Self {
        Self {
            model: model.to_string(),
            apparaat,
        }
    }
}

/// Alles wat de app onthoudt tussen opdrachten.
#[derive(Default)]
pub(crate) struct DicteerStaat {
    /// Wordt pas gemaakt bij het eerste gebruik: het model kan na de start gedownload zijn.
    pub(crate) beheerder: Mutex<Option<Arc<Motorbeheerder>>>,
    /// Met welk model en apparaat de beheerder draait. Verandert de gebruiker het
    /// dicteermodel of de videokaart, dan maakt het volgende dictaat een nieuwe beheerder.
    /// Altijd na `beheerder` vergrendelen.
    beheerder_model: Mutex<Option<MotorSleutel>>,
    /// De beheerder waarop de lopende proef draait, zodat annuleren die proef vindt, ook als
    /// de beheerder intussen voor een ander model is vervangen.
    proef_beheerder: Mutex<Option<Arc<Motorbeheerder>>>,
    /// Het laatste dictaat. Alleen in het geheugen, tot het volgende dictaat of afsluiten.
    pub(crate) laatste_dictaat: crate::dictaten::Geheugen,
    /// Het laatst gekozen opnamebestand, voor "opnieuw proberen".
    laatste_opname: Mutex<Option<PathBuf>>,
    /// Loopt er een proef (van bestandskiezer tot uitslag)? Eén tegelijk, anders
    /// overschrijft een stille tweede proef het laatste dictaat.
    pub(crate) proef_bezig: AtomicBool,
    /// Loopt er een dictaat met de sneltoets? Een proef en een dictaat delen de beheerder en
    /// het scherm, dus ze lopen niet door elkaar.
    pub(crate) dictaat_bezig: AtomicBool,
}

/// Houdt de proef bezet zolang hij leeft; geeft haar vrij bij elke uitgang, ook bij een fout.
struct ProefBezet(AppHandle);

impl ProefBezet {
    /// Bezet de proef, of `None` als er al een loopt.
    fn neem(app: &AppHandle) -> Option<Self> {
        app.state::<DicteerStaat>()
            .proef_bezig
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| Self(app.clone()))
    }
}

impl Drop for ProefBezet {
    fn drop(&mut self) {
        let staat = self.0.state::<DicteerStaat>();
        if let Some(beheerder) = slot(&staat.proef_beheerder).take() {
            laat_los(beheerder);
        }
        staat.proef_bezig.store(false, Ordering::SeqCst);
    }
}

/// Een slot lezen, ook als een andere thread erin is vastgelopen: de gegevens zijn hier
/// eenvoudig genoeg om er altijd mee verder te kunnen.
pub(crate) fn slot<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl DicteerStaat {
    /// Stopt alles netjes bij het afsluiten van de app: het werkproces met de spraakengine
    /// eindigt, zodat er geen proces blijft hangen. (De downloads stopt `downloads::sluit`.)
    pub(crate) fn stop_alles(&self) {
        // Een plakactie die nog loopt: eerst het oude klembord terug (of anders het dictaat
        // erop), anders is het klembord van de gebruiker weg na afsluiten.
        crate::plakken::afsluiten();
        let beheerder = slot(&self.beheerder).take();
        *slot(&self.beheerder_model) = None;
        if let Some(beheerder) = beheerder {
            stop_en_wacht(beheerder);
        }
    }
}

/// Laat een beheerder los zonder te wachten. Is dit de laatste verwijzing, dan stopt hij op
/// een eigen thread (stoppen wacht op het werkproces); anders stopt hij pas als de laatste
/// houder (een lopend dictaat) hem loslaat.
pub(crate) fn laat_los(beheerder: Arc<Motorbeheerder>) {
    if Arc::strong_count(&beheerder) > 1 {
        drop(beheerder);
    } else {
        std::thread::spawn(move || stop_en_wacht(beheerder));
    }
}

/// Stopt een beheerder en wacht hooguit twee seconden. Netjes stoppen duurt normaal
/// milliseconden, maar wacht op een lopende uitschrijving. Daar wachten we niet op: zonder
/// beheerder sluit de invoer van het werkproces en stopt het zelf na de lopende opdracht.
fn stop_en_wacht(beheerder: Arc<Motorbeheerder>) {
    let (klaar, wacht) = mpsc::channel();
    std::thread::spawn(move || {
        // Zijn er nog andere houders (een lopend commando), dan stopt de laatste bij Drop.
        if let Ok(beheerder) = Arc::try_unwrap(beheerder) {
            beheerder.stop();
        }
        let _ = klaar.send(());
    });
    let _ = wacht.recv_timeout(Duration::from_secs(2));
}

/// Het model dat nu geldt voor een taak, met zijn map (sprint 5-A: het dicteermodel en het
/// bestandsmodel zijn los van elkaar te kiezen).
pub(crate) fn model_voor(
    app: &AppHandle,
    gebruik: Gebruik,
) -> Result<(Model, PathBuf), Foutmelding> {
    modellen::geldend_voor(app, gebruik)
}

/// Loopt er een dictaat of een proef op de beheerder?
fn dictaat_loopt(staat: &DicteerStaat) -> bool {
    staat.dictaat_bezig.load(Ordering::SeqCst) || staat.proef_bezig.load(Ordering::SeqCst)
}

/// Laat de dicteermotor los, wat er ook geladen is, zodat zijn geheugen vrijkomt (voor de
/// snelheidstest van de videokaart). Het volgende dictaat maakt een nieuwe. De aanroeper
/// zorgt dat er geen dictaat start of loopt.
pub(crate) fn laat_dicteermotor_los(app: &AppHandle) {
    let staat = app.state::<DicteerStaat>();
    let oude = {
        let mut houder = slot(&staat.beheerder);
        let mut huidig = slot(&staat.beheerder_model);
        *huidig = None;
        houder.take()
    };
    if let Some(oude) = oude {
        laat_los(oude);
    }
}

/// Het dicteermodel of de videokaart kan veranderd zijn: laat een beheerder met een ander
/// model of apparaat los, zodat het volgende dictaat ermee begint. Een dictaat dat nu loopt,
/// houdt zijn eigen beheerder vast en maakt het met de oude af. Bij "Altijd" laadt de nieuwe
/// meteen.
pub(crate) fn ververs_beheerder(app: &AppHandle) {
    let staat = app.state::<DicteerStaat>();
    let Ok((model, _)) = model_voor(app, Gebruik::Dicteren) else {
        return;
    };
    let gewenst = MotorSleutel::nieuw(&model.id, crate::videokaart::gewenst_apparaat(app));
    let oude = {
        let mut houder = slot(&staat.beheerder);
        let mut huidig = slot(&staat.beheerder_model);
        let verouderd = huidig.as_ref().is_some_and(|sleutel| *sleutel != gewenst);
        if verouderd {
            *huidig = None;
            houder.take()
        } else {
            None
        }
    };
    if let Some(oude) = oude {
        laat_los(oude);
    }
    if crate::instellingen::lees_rusttijd(app) == Rusttijd::Nooit {
        let app = app.clone();
        std::thread::spawn(move || {
            let _ = beheerder_als_mogelijk(&app);
        });
    }
}

/// Laat de beheerder met dit model los, zodat het van schijf kan. Wacht niet: stoppen gebeurt
/// op de achtergrond, en `model_verwijder` wacht via het register in `modelgebruik` tot het
/// werkproces het model echt heeft losgelaten.
///
/// # Errors
///
/// Geeft een melding als er op dit moment een dictaat loopt.
pub(crate) fn geef_model_vrij(app: &AppHandle, id: &str) -> Result<(), Foutmelding> {
    let staat = app.state::<DicteerStaat>();
    let oude = {
        let mut houder = slot(&staat.beheerder);
        let mut huidig = slot(&staat.beheerder_model);
        if huidig.as_ref().map(|sleutel| sleutel.model.as_str()) != Some(id) {
            return Ok(());
        }
        if dictaat_loopt(&staat) {
            return Err(Foutmelding::nieuw(
                "Er loopt een dictaat. Probeer het opnieuw als het klaar is.",
                Vervolg::OpnieuwProberen,
            ));
        }
        *huidig = None;
        houder.take()
    };
    if let Some(oude) = oude {
        laat_los(oude);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Proef op het dicteerscherm
// ---------------------------------------------------------------------------

/// Kiest een opname (of hergebruikt de vorige), laat het werkproces opwarmen en schrijft
/// de opname uit. Geeft `None` als de gebruiker de bestandskiezer sloot. Alles wat daarna
/// gebeurt komt als `taak`-event en uiteindelijk als `proef-resultaat`.
#[tauri::command]
pub(crate) async fn proef_start(
    app: AppHandle,
    opnieuw: bool,
) -> Result<Option<TaakId>, Foutmelding> {
    tauri::async_runtime::spawn_blocking(move || proef_start_blokkerend(&app, opnieuw))
        .await
        .map_err(|_| Foutmelding::nieuw("De proef is onverwacht gestopt.", Vervolg::Geen))?
}

fn proef_start_blokkerend(app: &AppHandle, opnieuw: bool) -> Result<Option<TaakId>, Foutmelding> {
    let staat = app.state::<DicteerStaat>();
    let (model, map) = model_voor(app, Gebruik::Dicteren)?;
    let engine_pad = app.state::<EnginePad>().0.clone();
    let model_compleet = download::status(&model, &map) == ModelStatus::Compleet;
    if let Some(melding) = proef::ontbrekend(engine_pad.is_some(), model_compleet) {
        return Err(melding);
    }
    let Some(engine_pad) = engine_pad else {
        // Onbereikbaar: `ontbrekend` meldde dit al.
        return Ok(None);
    };
    // Loopt er al een proef (dubbelklik, of de kiezer stond nog open), dan gebeurt er
    // niets: het scherm volgt die lopende proef al.
    let Some(bezet) = ProefBezet::neem(app) else {
        return Ok(None);
    };
    // Eerst de proef bezetten, dan kijken naar het dictaat: het dictaat doet het omgekeerde,
    // zodat minstens één van de twee de ander ziet. Bij een dictaat stopt de proef met een
    // melding (de bezetting gaat bij het verlaten van deze functie vanzelf weg).
    if staat.dictaat_bezig.load(Ordering::SeqCst) {
        return Err(Foutmelding::nieuw(
            "Er loopt al een dictaat. Wacht tot het klaar is en probeer het dan opnieuw.",
            Vervolg::Geen,
        ));
    }

    // Bestand kiezen. Sluit de gebruiker de kiezer, dan gebeurt er niets.
    let hergebruik = if opnieuw {
        slot(&staat.laatste_opname).clone()
    } else {
        None
    };
    let pad = if let Some(pad) = hergebruik {
        pad
    } else {
        let mut kiezer = app.dialog().file();
        // Aan het venster gekoppeld, zodat de kiezer modaal is en het venster niet klikbaar.
        if let Some(venster) = app.get_webview_window("main") {
            kiezer = kiezer.set_parent(&venster);
        }
        let gekozen = kiezer
            .add_filter(
                "Opnamen",
                &["wav", "mp3", "m4a", "flac", "ogg", "mp4", "mov"],
            )
            .blocking_pick_file();
        let Some(pad) = gekozen.and_then(|p| p.into_path().ok()) else {
            return Ok(None);
        };
        *slot(&staat.laatste_opname) = Some(pad.clone());
        pad
    };

    let beheerder = haal_beheerder(app, &staat, engine_pad, &model, map);
    *slot(&staat.proef_beheerder) = Some(Arc::clone(&beheerder));
    // Eerst het dictaat beginnen, zodat het werkproces opwarmt terwijl het bestand wordt gelezen.
    let id = beheerder
        .nieuw_dictaat()
        .map_err(|fout| Foutmelding::nieuw(fout.to_string(), Vervolg::Geen))?;

    let samples = match proef::lees_opname(&pad) {
        Ok(samples) => samples,
        Err(fout) => {
            // De fout gaat als gebeurtenis naar de interface, zoals elke andere taakfout.
            let _ = beheerder.mislukt(id, Foutmelding::from(fout));
            return Ok(Some(id));
        }
    };

    let begin = Instant::now();
    let uitslag =
        match beheerder.uitschrijven(id, samples, crate::instellingen::dicteer_filter(app)) {
            Ok(uitslag) => uitslag,
            // Geannuleerd terwijl de opname werd gelezen: die gebeurtenis ging al naar de
            // interface, dus dit is geen fout.
            Err(BeheerFout::VerkeerdeToestand(_)) => return Ok(Some(id)),
            Err(fout) => return Err(Foutmelding::nieuw(fout.to_string(), Vervolg::Geen)),
        };
    let app = app.clone();
    std::thread::spawn(move || {
        // De proef blijft bezet tot de uitslag er is, wat die ook is.
        let _bezet = bezet;
        // Fout en annuleren zijn al als gebeurtenis gemeld door de beheerder.
        if let Ok(Uitslag::Klaar(gefilterd)) = uitslag.recv() {
            let resultaat =
                ProefResultaat::nieuw(id, &gefilterd, begin.elapsed(), Soevereiniteit::Lokaal);
            if let Some(tekst) = &resultaat.tekst {
                crate::dictaten::na_dictaat(&app, tekst);
            }
            let _ = app.emit("proef-resultaat", &resultaat);
        }
    });
    Ok(Some(id))
}

/// De motorbeheerder, gemaakt bij het eerste gebruik. De gebeurtenissen gaan via een
/// doorstuurthread als `taak`-event naar de interface.
pub(crate) fn haal_beheerder(
    app: &AppHandle,
    staat: &DicteerStaat,
    engine_pad: PathBuf,
    model: &Model,
    modelmap: PathBuf,
) -> Arc<Motorbeheerder> {
    // De schakelaar "Videokaart gebruiken" (ADR 0011, aanvulling 2-11-2026); standaard de
    // processor. Bij een fout op de videokaart valt de motor zelf terug (ADR 0009).
    let apparaat = crate::videokaart::gewenst_apparaat(app);
    let mut houder = slot(&staat.beheerder);
    let mut huidig = slot(&staat.beheerder_model);
    beheerder_voor(
        &mut houder,
        &mut huidig,
        MotorSleutel::nieuw(&model.id, apparaat),
        || {
            let mut instellingen = Instellingen::standaard(engine_pad);
            instellingen.bouwcontrole = crate::versie::bouwcontrole();
            let engine = LokaleEngine::nieuw(
                instellingen,
                model,
                modelmap,
                apparaat,
                Some(crate::taal::MODELTAAL.to_string()),
            )
            .met_melder(crate::videokaart::melder(
                app,
                crate::videokaart::Route::Dicteren,
            ));
            let (zender, ontvanger) = mpsc::channel::<Gebeurtenis>();
            let rusttijd = crate::instellingen::lees_rusttijd(app);
            let app = app.clone();
            std::thread::spawn(move || {
                for gebeurtenis in ontvanger {
                    crate::logboek::taak(&gebeurtenis);
                    let _ = app.emit("taak", &gebeurtenis);
                }
            });
            Motorbeheerder::start(Gevolgd::nieuw(engine, &model.id), rusttijd, zender)
        },
    )
}

/// De beheerder voor `sleutel` (model en apparaat): de bestaande als die daarmee draait,
/// anders een nieuwe uit `maak`. Een beheerder met een ander model of apparaat gaat los
/// (`laat_los`); een dictaat dat er nog op loopt, houdt zelf een verwijzing vast en maakt het
/// daarop af. Een wissel geldt dus vanaf het volgende dictaat.
fn beheerder_voor(
    houder: &mut Option<Arc<Motorbeheerder>>,
    huidig: &mut Option<MotorSleutel>,
    sleutel: MotorSleutel,
    maak: impl FnOnce() -> Motorbeheerder,
) -> Arc<Motorbeheerder> {
    if let Some(beheerder) = houder.as_ref()
        && huidig.as_ref() == Some(&sleutel)
    {
        return Arc::clone(beheerder);
    }
    if let Some(oude) = houder.take() {
        laat_los(oude);
    }
    let beheerder = Arc::new(maak());
    *houder = Some(Arc::clone(&beheerder));
    *huidig = Some(sleutel);
    beheerder
}

/// De motorbeheerder als hij gemaakt kan worden zonder dat de gebruiker iets doet: alleen
/// als het werkproces en het complete model er zijn. Voor "Altijd", dat het model al
/// laadt bij het starten van Mondig. Geeft `None` als er iets ontbreekt.
pub(crate) fn beheerder_als_mogelijk(app: &AppHandle) -> Option<Arc<Motorbeheerder>> {
    let (model, map) = model_voor(app, Gebruik::Dicteren).ok()?;
    let engine_pad = app.state::<EnginePad>().0.clone()?;
    if download::status(&model, &map) != ModelStatus::Compleet {
        return None;
    }
    let staat = app.state::<DicteerStaat>();
    Some(haal_beheerder(app, &staat, engine_pad, &model, map))
}

/// Annuleert een dictaat van de proef.
#[tauri::command]
pub(crate) fn proef_annuleer(
    taak: TaakId,
    staat: State<'_, DicteerStaat>,
) -> Result<(), Foutmelding> {
    let proef = slot(&staat.proef_beheerder).clone();
    let beheerder = proef.or_else(|| slot(&staat.beheerder).clone());
    let Some(beheerder) = beheerder else {
        return Ok(());
    };
    beheerder
        .annuleer(taak)
        .map_err(|fout| Foutmelding::nieuw(fout.to_string(), Vervolg::Geen))
}

/// Het laatste dictaat, als er een is. Alleen in het geheugen bewaard.
#[tauri::command]
pub(crate) fn laatste_dictaat(staat: State<'_, DicteerStaat>) -> Option<String> {
    crate::dictaten::tekst_voor_tray(&staat.laatste_dictaat)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mondig_core::engine::beheer::Rusttijd;
    use mondig_core::filter::FilterInstellingen;

    use super::*;
    use crate::modelgebruik::tests::{NepMotor, register};

    fn nep_beheerder(
        register: &'static crate::modelgebruik::Modelgebruik,
        model: &str,
    ) -> Motorbeheerder {
        let (zender, _) = mpsc::channel();
        Motorbeheerder::start(
            Gevolgd::met_register(register, NepMotor::nieuw(Duration::from_millis(100)), model),
            Rusttijd::Duur(Duration::from_secs(60)),
            zender,
        )
    }

    #[test]
    fn een_lopend_dictaat_maakt_het_af_op_zijn_eigen_beheerder_na_een_wissel() {
        let register = register();
        let mut houder = None;
        let mut huidig = None;
        let x = || MotorSleutel::nieuw("x", Apparaat::Cpu);
        let y = || MotorSleutel::nieuw("y", Apparaat::Cpu);
        let oud = beheerder_voor(&mut houder, &mut huidig, x(), || {
            nep_beheerder(register, "x")
        });
        // Hetzelfde model: dezelfde beheerder.
        let nog_eens = beheerder_voor(&mut houder, &mut huidig, x(), || unreachable!());
        assert!(Arc::ptr_eq(&oud, &nog_eens));
        drop(nog_eens);

        // Een dictaat begint (het houdt zijn beheerder vast, zoals `Lopend` in dictaat.rs) en
        // terwijl het opneemt, kiest de gebruiker een ander model.
        let id = oud.nieuw_dictaat().unwrap();
        let nieuw = beheerder_voor(&mut houder, &mut huidig, y(), || {
            nep_beheerder(register, "y")
        });
        assert!(!Arc::ptr_eq(&oud, &nieuw));
        assert_eq!(huidig, Some(y()));
        assert!(
            register.in_gebruik("x"),
            "het oude model blijft tot het dictaat klaar is"
        );

        // Bij het stoppen schrijft de oude beheerder het dictaat gewoon uit.
        let uitslag = oud
            .uitschrijven(id, vec![0.1; 16_000], FilterInstellingen::standaard())
            .unwrap();
        assert!(matches!(
            uitslag.recv_timeout(Duration::from_secs(5)),
            Ok(Uitslag::Klaar(_))
        ));
        assert_eq!(oud.toestand(id), Some(mondig_core::taak::Toestand::Klaar));

        // Klaar: het dictaat laat los, de oude beheerder stopt en het oude model is vrij.
        laat_los(oud);
        assert!(register.wacht_tot_vrij("x", Duration::from_secs(5)));
        assert!(register.in_gebruik("y"));
        // Het volgende dictaat gaat naar de nieuwe beheerder.
        let volgende = beheerder_voor(&mut houder, &mut huidig, y(), || unreachable!());
        assert!(Arc::ptr_eq(&volgende, &nieuw));
    }

    #[test]
    fn de_videokaart_omzetten_geeft_een_nieuwe_beheerder_met_hetzelfde_model() {
        let register = register();
        let mut houder = None;
        let mut huidig = None;
        let cpu = MotorSleutel::nieuw("x", Apparaat::Cpu);
        let gpu = MotorSleutel::nieuw("x", Apparaat::Gpu);
        let processor = beheerder_voor(&mut houder, &mut huidig, cpu.clone(), || {
            nep_beheerder(register, "x")
        });
        // Een dictaat loopt nog op de processor terwijl de schakelaar omgaat.
        let id = processor.nieuw_dictaat().unwrap();
        let videokaart = beheerder_voor(&mut houder, &mut huidig, gpu.clone(), || {
            nep_beheerder(register, "x")
        });
        assert!(!Arc::ptr_eq(&processor, &videokaart));
        assert_eq!(huidig, Some(gpu.clone()));
        // Het lopende dictaat maakt het af op de oude beheerder.
        let uitslag = processor
            .uitschrijven(id, vec![0.1; 16_000], FilterInstellingen::standaard())
            .unwrap();
        assert!(matches!(
            uitslag.recv_timeout(Duration::from_secs(5)),
            Ok(Uitslag::Klaar(_))
        ));
        laat_los(processor);
        // Weer terug naar de processor: weer een nieuwe.
        let terug = beheerder_voor(&mut houder, &mut huidig, cpu.clone(), || {
            nep_beheerder(register, "x")
        });
        assert!(!Arc::ptr_eq(&terug, &videokaart));
        assert_eq!(huidig, Some(cpu));
    }
}
