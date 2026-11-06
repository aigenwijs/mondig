//! De dicteerketen met sneltoets (sprint 3b, ADR 0012 en 0013): sneltoets indrukken,
//! opnemen, stilte wegknippen en uitschrijven. De tekst komt in het venster van Mondig en
//! wordt geplakt in het venster met de focus (sprint 3c, zie `plakken.rs`).
//!
//! Alles gebeurt op één eigen thread, de dicteerwerker. De sneltoets-handlers sturen alleen
//! een bericht en wachten nergens op, want een handler draait op de gebeurtenislus van het
//! venster en het openen van een microfoon (zeker een Bluetooth-headset) kan seconden duren.
//! De werker houdt de [`Toetsbesturing`] bij die houden van tikken scheidt, opent de
//! microfoon en meldt elke toestandswijziging via de beheerder als `taak`-event, dezelfde
//! stroom die venster en schermlezer al lezen.
//!
//! Twee dictaten tegelijk kan niet: tijdens "verwerkt" wordt een nieuwe druk genegeerd. Het
//! werkproces schrijft één dictaat tegelijk uit (ADR 0012); een tweede opname erdoorheen zou
//! het scherm laten springen, en de gebruiker ziet in het venster dat Mondig nog bezig is.
//! Annuleren via het venster maakt de sneltoets direct weer vrij.
//!
//! Events: `taak` (toestand), `dictaat-resultaat` (de uitslag, zelfde vorm als
//! `proef-resultaat`) en `plak-uitslag` (hoe het plakken afliep). De tekst gaat nooit naar
//! een log.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use mondig_core::download;
use mondig_core::engine::beheer::{Motorbeheerder, Uitslag};
use mondig_core::model::Gebruik;
use mondig_core::opname::handy::HandyApparatuur;
use mondig_core::opname::{Apparatuur, Melder, Opname, OpnameGebeurtenis, opgeslagen_microfoon};
use mondig_core::proef::{self, ProefResultaat};
use mondig_core::sneltoets::{Actie, NALOOPTIJD, Toets, Toetsbesturing};
use mondig_core::soevereiniteit::Soevereiniteit;
use mondig_core::stilte::controleer_nulsignaal;
use mondig_core::taak::{Foutmelding, Soort, Taak, TaakId, Toestand, Vervolg};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::EnginePad;
use crate::dicteerketen::{DicteerStaat, haal_beheerder, laat_los, model_voor, slot};
use crate::geluid::{self, Toon};
use crate::microfoonrecht::verfijn;
use crate::pil;
use crate::sneltoets::{esc_aan, esc_uit, lees_microfoon};

/// Taken die mislukken voordat er een dictaat is (geen model, geen werkproces) krijgen een
/// id ver boven de ids van de beheerder, zodat ze nooit botsen.
const LOS_ID_BASIS: u64 = 1 << 40;

/// Wat de dicteerwerker moet doen.
enum Opdracht {
    /// Een sneltoets ging in (de gewone of de tweede, voor opschonen).
    Ingedrukt(Toets, Instant),
    /// Een sneltoets ging los.
    Losgelaten(Toets, Instant),
    /// Esc, of de knop "Annuleren" tijdens het opnemen.
    Esc,
    /// De knop "Annuleren" in het venster, voor dit dictaat.
    AnnuleerTaak(TaakId),
    /// De knop "Stoppen" in de pil: dit dictaat stoppen en uitschrijven.
    StopTaak(TaakId),
    /// De opname zit aan de grens van tien minuten.
    OpnameVol(TaakId),
    /// De microfoon viel weg tijdens het opnemen.
    OpnameUitgevallen(TaakId, Foutmelding),
    /// Het uitschrijven van dit dictaat is afgelopen, hoe dan ook.
    Afgelopen(TaakId),
    /// De app sluit af.
    Stop,
}

/// Het handvat van de dicteerwerker: sturen kan overal, zonder te wachten.
pub(crate) struct Dicteerwerker {
    zender: Sender<Opdracht>,
}

impl Dicteerwerker {
    /// Start de werker op een eigen thread.
    pub(crate) fn start(app: AppHandle) -> Self {
        let (zender, ontvanger) = mpsc::channel();
        let eigen = zender.clone();
        thread::spawn(move || Werker::nieuw(app, eigen).draai(&ontvanger));
        Self { zender }
    }

    /// Een sneltoets ging in (het tijdstip is dat van de gebeurtenis, niet van de verwerking).
    pub(crate) fn ingedrukt(&self, toets: Toets, nu: Instant) {
        let _ = self.zender.send(Opdracht::Ingedrukt(toets, nu));
    }

    /// Een sneltoets ging los.
    pub(crate) fn losgelaten(&self, toets: Toets, nu: Instant) {
        let _ = self.zender.send(Opdracht::Losgelaten(toets, nu));
    }

    /// Esc is ingedrukt terwijl er een opname loopt.
    pub(crate) fn esc(&self) {
        let _ = self.zender.send(Opdracht::Esc);
    }

    /// Stopt de werker bij het afsluiten van de app.
    pub(crate) fn stop(&self) {
        let _ = self.zender.send(Opdracht::Stop);
    }
}

/// Annuleert een dictaat van de sneltoets: tijdens het opnemen wordt de opname weggegooid,
/// tijdens het uitschrijven wordt het resultaat genegeerd (ADR 0012).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de State per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn dictaat_annuleer(taak: TaakId, werker: State<'_, Dicteerwerker>) {
    let _ = werker.zender.send(Opdracht::AnnuleerTaak(taak));
}

/// Stopt een dictaat van de sneltoets en schrijft het uit, zoals loslaten of opnieuw
/// tikken (de knop "Stoppen" in de pil).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de State per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn dictaat_stop(taak: TaakId, werker: State<'_, Dicteerwerker>) {
    let _ = werker.zender.send(Opdracht::StopTaak(taak));
}

/// Een opname die nu loopt, met de beheerder waarop het dictaat begon. Het dictaat houdt die
/// beheerder vast tot het klaar is: kiest de gebruiker intussen een ander model, dan maakt dit
/// dictaat het met het oude model af en geldt de wissel vanaf het volgende.
struct Lopend {
    id: TaakId,
    opname: Opname,
    beheerder: Arc<Motorbeheerder>,
}

/// Alles wat de werker bijhoudt.
struct Werker {
    app: AppHandle,
    zender: Sender<Opdracht>,
    besturing: Toetsbesturing,
    lopend: Option<Lopend>,
    /// Het dictaat dat nu uitgeschreven wordt, met zijn beheerder en de stand van zijn
    /// uitslagdraad.
    verwerkt: Option<(TaakId, Arc<Motorbeheerder>, Arc<Mutex<Keten>>)>,
    los_teller: u64,
}

impl Werker {
    fn nieuw(app: AppHandle, zender: Sender<Opdracht>) -> Self {
        Self {
            app,
            zender,
            besturing: Toetsbesturing::nieuw(),
            lopend: None,
            verwerkt: None,
            los_teller: 0,
        }
    }

    fn draai(mut self, ontvanger: &Receiver<Opdracht>) {
        for opdracht in ontvanger {
            match opdracht {
                Opdracht::Ingedrukt(toets, nu) => match self.besturing.ingedrukt_met(toets, nu) {
                    Actie::Start => self.begin(),
                    Actie::Stop => self.stop(true),
                    Actie::Geen => {}
                },
                Opdracht::Losgelaten(toets, nu) => {
                    if self.besturing.losgelaten_met(toets, nu) == Actie::Stop {
                        self.stop(true);
                    }
                }
                Opdracht::Esc => self.annuleer_opname(),
                Opdracht::AnnuleerTaak(id) => self.annuleer_taak(id),
                Opdracht::StopTaak(id) => {
                    if self.is_lopend(id) && self.besturing.stop_van_buitenaf() {
                        self.stop(true);
                    }
                }
                Opdracht::OpnameVol(id) => {
                    if self.is_lopend(id) && self.besturing.stop_van_buitenaf() {
                        // Vol: ADR 0012. Er is niets meer om na te lopen.
                        self.stop(false);
                    }
                }
                Opdracht::OpnameUitgevallen(id, melding) => {
                    if self.is_lopend(id) {
                        self.breek_af(id, melding);
                    }
                }
                Opdracht::Afgelopen(id) => {
                    crate::opschonen::vergeet(&self.app, id);
                    if self.is_verwerkt(id) {
                        self.laat_verwerkt_los();
                        self.rust();
                    }
                }
                Opdracht::Stop => {
                    esc_uit(&self.app);
                    self.lopend = None;
                    break;
                }
            }
        }
    }

    fn is_lopend(&self, id: TaakId) -> bool {
        self.lopend.as_ref().is_some_and(|lopend| lopend.id == id)
    }

    fn is_verwerkt(&self, id: TaakId) -> bool {
        self.verwerkt.as_ref().is_some_and(|(v, _, _)| *v == id)
    }

    /// Het uitschrijven is voorbij: de beheerder van dit dictaat gaat los. Was hij intussen
    /// vervangen, dan stopt hij nu, op een eigen thread, zodat de sneltoets niet wacht.
    fn laat_verwerkt_los(&mut self) {
        if let Some((_, beheerder, _)) = self.verwerkt.take() {
            laat_los(beheerder);
        }
    }

    /// Terug naar rust: de sneltoets begint weer een nieuw dictaat, en de proef mag weer.
    fn rust(&mut self) {
        self.besturing.terug_naar_rust();
        self.app
            .state::<DicteerStaat>()
            .dictaat_bezig
            .store(false, Ordering::SeqCst);
    }

    /// Meldt een fout die optreedt voordat er een dictaat is: de interface krijgt een taak
    /// met alleen de toestand "fout", zodat ook dit een melding in gewone taal wordt.
    fn meld_fout_zonder_taak(&mut self, fout: Foutmelding) {
        self.los_teller += 1;
        let id = TaakId(LOS_ID_BASIS + self.los_teller);
        // Ook de pil toont deze fout, met de vervolgactie: anders merkt wie in een ander
        // programma dicteert niets.
        pil::meld_dictaat(&self.app, id);
        self.meld_fout(id, fout);
    }

    /// Meldt een fout voor een dictaat buiten de beheerder om, met het id dat de interface
    /// al volgt, zodat die uit "luistert" komt.
    fn meld_fout(&self, id: TaakId, fout: Foutmelding) {
        let mut taak = Taak::nieuw(id, Soort::Dictaat);
        if let Ok(gebeurtenis) = taak.ga_naar(Toestand::fout(fout.melding, fout.vervolg)) {
            crate::logboek::taak(&gebeurtenis);
            let _ = self.app.emit("taak", &gebeurtenis);
        }
    }

    /// De beheerder en het model, of een melding in gewone taal als dicteren nog niet kan.
    fn beheerder(&self) -> Result<Arc<Motorbeheerder>, Foutmelding> {
        let (model, map) = model_voor(&self.app, Gebruik::Dicteren)?;
        let engine_pad = self.app.state::<EnginePad>().0.clone();
        // Bij elke druk op de sneltoets: alleen de markering en de grootte, geen vrije ruimte.
        // Dicteren vraagt het hele model, ook Silero: zonder dat is het model niet "klaar".
        let compleet = download::is_compleet(&model, &map);
        if let Some(melding) = proef::ontbrekend(engine_pad.is_some(), compleet) {
            if engine_pad.is_some() && download::staat_op_schijf(&model, &map) {
                return Err(Foutmelding::nieuw(
                    "Het spraakmodel is nog niet compleet. Download eerst wat er nog mist.",
                    Vervolg::OpnieuwDownloaden,
                ));
            }
            return Err(melding);
        }
        let Some(engine_pad) = engine_pad else {
            // Onbereikbaar: `ontbrekend` meldde dit al.
            return Err(Foutmelding::nieuw(
                "Het dictaat kan niet starten.",
                Vervolg::Geen,
            ));
        };
        let staat = self.app.state::<DicteerStaat>();
        Ok(haal_beheerder(&self.app, &staat, engine_pad, &model, map))
    }

    /// De sneltoets ging in en er loopt niets: een nieuw dictaat. De gebruiker praat al,
    /// dus alles wat kan wachten, wacht: de interface krijgt "luistert" voordat de
    /// microfoon open is, en het werkproces laadt het model terwijl de gebruiker praat.
    fn begin(&mut self) {
        let staat = self.app.state::<DicteerStaat>();
        // Eerst bezetten, dan kijken of de proef loopt: de proef doet het omgekeerde, zodat
        // minstens één van de twee de ander ziet.
        staat.dictaat_bezig.store(true, Ordering::SeqCst);
        let proef = staat.proef_bezig.load(Ordering::SeqCst);
        match blokkade_door_proef(proef, proef && crate::videokaart::test_loopt(&self.app)) {
            Blokkade::Geen => {}
            Blokkade::Proef => {
                self.rust();
                return;
            }
            Blokkade::Snelheidstest => {
                // De snelheidstest zet de proefvlag ook, maar zij zet eerst haar eigen vlag.
                // De pil zegt waarom er niets gebeurt (geen stille fout).
                geluid::speel(&self.app, Toon::Stop);
                self.meld_fout_zonder_taak(Foutmelding::nieuw(
                    crate::videokaart::MELDING_DICTEREN_TIJDENS_TEST,
                    Vervolg::Geen,
                ));
                self.rust();
                return;
            }
        }
        // Tijdens een vergaderopname geen dictaat (besluit, 2-11-2026): de melding komt
        // via de gewone foutweergave van een dictaat, ook in de pil, met een kort foutsignaal.
        // De vergadering zet eerst haar vlag en kijkt dan naar `dictaat_bezig`, dus minstens
        // een van de twee ziet de ander.
        if crate::vergadering::loopt(&self.app) {
            geluid::speel(&self.app, Toon::Stop);
            self.meld_fout_zonder_taak(Foutmelding::nieuw(
                crate::vergadering::MELDING_DICTEREN_TIJDENS_OPNAME,
                Vervolg::Geen,
            ));
            self.rust();
            return;
        }
        // Een dictaat gaat voor: een lopende microfoontest en de meters stoppen voordat we
        // zelf openen.
        crate::meter::stop_alle(&self.app, crate::meter::Reden::Voorrang);
        let beheerder = match self.beheerder() {
            Ok(beheerder) => beheerder,
            Err(fout) => {
                self.meld_fout_zonder_taak(fout);
                self.rust();
                return;
            }
        };
        let Ok(id) = beheerder.nieuw_dictaat() else {
            self.meld_fout_zonder_taak(Foutmelding::nieuw(
                "De spraakengine is gestopt. Start Mondig opnieuw.",
                Vervolg::Geen,
            ));
            self.rust();
            return;
        };

        // De pil weet nu dat dit dictaat van de sneltoets is. Het startgeluid gaat af voordat
        // de microfoon opent (zie geluid.rs); afspelen wacht nergens op.
        pil::meld_dictaat(&self.app, id);
        geluid::speel(&self.app, Toon::Start);
        let microfoon = self.kies_microfoon();
        let melder = self.melder(id);
        let voor_meter = self.app.clone();
        let opname =
            Opname::open(&HandyApparatuur, microfoon.as_deref(), melder).and_then(|mut opname| {
                // Alleen het niveau, voor de meter in de pil; er wordt niets bewaard.
                opname.bij_niveau(move |niveau| pil::meld_niveau(&voor_meter, niveau));
                opname.start()?;
                Ok(opname)
            });
        match opname {
            Ok(opname) => {
                esc_aan(&self.app);
                pil::opname_loopt(Some(id));
                self.lopend = Some(Lopend {
                    id,
                    opname,
                    beheerder,
                });
            }
            Err(fout) => {
                geluid::speel(&self.app, Toon::Stop);
                let _ = beheerder.mislukt(id, verfijn(fout, false));
                self.rust();
            }
        }
    }

    /// De opgeslagen microfoon als die er nog is, anders de standaard van het systeem.
    fn kies_microfoon(&self) -> Option<String> {
        let opgeslagen = lees_microfoon(&self.app)?;
        let lijst = HandyApparatuur.microfoons().ok()?;
        opgeslagen_microfoon(Some(&opgeslagen), &lijst)
    }

    /// Wat de opname meldt gaat als bericht naar de werker, nooit rechtstreeks: de melder
    /// draait op een thread van de bron, en daar mag de opname niet gestopt worden.
    fn melder(&self, id: TaakId) -> Melder {
        let zender = self.zender.clone();
        Arc::new(move |gebeurtenis| match gebeurtenis {
            OpnameGebeurtenis::Vol => {
                let _ = zender.send(Opdracht::OpnameVol(id));
            }
            OpnameGebeurtenis::Uitgevallen(melding) => {
                let _ = zender.send(Opdracht::OpnameUitgevallen(id, melding));
            }
            // De waarschuwing na negen minuten toont de interface zelf (ze kent het begin
            // van de opname); de rest is voor de meting.
            OpnameGebeurtenis::BijnaVol | OpnameGebeurtenis::EersteBlok(_) => {}
        })
    }

    /// Stopt de opname en schrijft uit. Met `naloop` loopt de opname nog even door, zodat
    /// het laatste woord niet wegvalt.
    fn stop(&mut self, naloop: bool) {
        let loslaten = Instant::now();
        esc_uit(&self.app);
        let Some(Lopend {
            id,
            opname,
            beheerder,
        }) = self.lopend.take()
        else {
            self.rust();
            return;
        };
        pil::opname_loopt(None);
        if naloop {
            thread::sleep(NALOOPTIJD);
        }
        let gestopt = opname.stop();
        // De microfoon is dicht (ook bij een fout): het stopgeluid komt niet in de opname.
        geluid::speel(&self.app, Toon::Stop);
        let opgenomen = match gestopt {
            Ok(opgenomen) => opgenomen,
            Err(fout) => {
                self.mislukt(beheerder, id, verfijn(fout, false));
                return;
            }
        };
        // Is de microfoon weggevallen, dan zeggen we dat, in plaats van een half dictaat
        // te laten doorgaan alsof er niets gebeurde.
        if let Some(melding) = opgenomen.onderbroken.clone() {
            self.mislukt(beheerder, id, verfijn(melding, false));
            return;
        }
        let samples = opgenomen.samples().to_vec();
        drop(opgenomen);

        let filter = crate::instellingen::dicteer_filter(&self.app);
        if let Err(melding) = controleer_nulsignaal(&samples, &filter) {
            // Een geblokkeerde microfoon kan ook stilte geven in plaats van een fout.
            self.mislukt(beheerder, id, verfijn(melding, true));
            return;
        }
        // Mislukt dit, dan is het dictaat geannuleerd tijdens de naloop of de beheerder gestopt:
        // de interface weet het al.
        let Ok(uitslag) = beheerder.uitschrijven_met(id, samples, filter, true) else {
            laat_los(beheerder);
            self.rust();
            return;
        };
        let keten = Arc::new(Mutex::new(Keten::default()));
        self.verwerkt = Some((id, beheerder, Arc::clone(&keten)));
        // Opschonen (ADR 0027): bij "Bij elk dictaat", of met de tweede sneltoets bij "Met een
        // eigen sneltoets". Met de toets die dit dictaat begon.
        let toets = self.besturing.toets();
        let app = self.app.clone();
        let zender = self.zender.clone();
        thread::spawn(move || {
            // Fout en annuleren zijn al als `taak`-event gemeld door de beheerder.
            // Is het dictaat intussen geannuleerd, dan niets meer: geen opschonen, geen plakken.
            if let Ok(Uitslag::Klaar(gefilterd)) = uitslag.recv()
                && slot(&keten).begin()
            {
                // Op deze draad: de sleutelbos lezen mag de werker niet ophouden.
                let plan = crate::opschonen::plan_voor(&app, toets);
                let mut resultaat = ProefResultaat::nieuw(
                    id,
                    &gefilterd,
                    loslaten.elapsed(),
                    Soevereiniteit::Lokaal,
                );
                // Na het uitschrijven en voor het plakken: opschonen, hooguit de wachttijd van
                // de kern. Wat er ook misgaat, er komt een tekst uit (de letterlijke).
                if let (Some(letterlijk), Some(plan)) = (resultaat.tekst.clone(), plan.as_ref()) {
                    let afloop = crate::opschonen::schoon_dictaat_op(&app, id, &letterlijk, plan);
                    match afloop.door {
                        Some(door) => {
                            crate::dictaten::na_opgeschoond(&app, &afloop.tekst, &letterlijk, door);
                        }
                        None => crate::dictaten::na_dictaat(&app, &letterlijk),
                    }
                    // Ging de tekst naar de aanbieder (ook als het daarna terugviel), dan zegt het
                    // resultaat dat, en niet meer "Lokaal".
                    if let Some(label) = afloop.label {
                        resultaat.soevereiniteit = label;
                    }
                    resultaat.tekst = Some(afloop.tekst);
                    resultaat.duur_ms =
                        u64::try_from(loslaten.elapsed().as_millis()).unwrap_or(u64::MAX);
                } else if let Some(tekst) = &resultaat.tekst {
                    crate::dictaten::na_dictaat(&app, tekst);
                }
                let _ = app.emit("dictaat-resultaat", &resultaat);
                // Eerst het venster bijwerken, dan plakken: ook als plakken mislukt, staat
                // de tekst al in het geheugen (het schrijven naar schijf loopt op een eigen
                // draad en houdt het plakken niet op). Het dictaat telt als afgelopen zodra
                // Ctrl+V verstuurd is; een volgend dictaat rondt het terugzetten af.
                if let Some(tekst) = &resultaat.tekst {
                    crate::plakken::na_dictaat(&app, id, tekst);
                }
            }
            // Alleen deze draad meldt het einde: pas dan is de sneltoets weer vrij.
            let _ = zender.send(Opdracht::Afgelopen(id));
        });
    }

    /// Het dictaat mislukt voordat het uitgeschreven wordt: de interface krijgt de melding,
    /// de beheerder van dit dictaat gaat los en de sneltoets is weer vrij.
    fn mislukt(&mut self, beheerder: Arc<Motorbeheerder>, id: TaakId, melding: Foutmelding) {
        let _ = beheerder.mislukt(id, melding);
        laat_los(beheerder);
        self.rust();
    }

    /// Esc of de knop tijdens het opnemen: de opname wordt weggegooid.
    fn annuleer_opname(&mut self) {
        if !self.besturing.annuleer() {
            return;
        }
        esc_uit(&self.app);
        // Weggooien stopt de bron en wist het geheugen (`Opname::drop`).
        if let Some(Lopend {
            id,
            opname,
            beheerder,
        }) = self.lopend.take()
        {
            pil::opname_loopt(None);
            drop(opname);
            geluid::speel(&self.app, Toon::Stop);
            let _ = beheerder.annuleer(id);
            laat_los(beheerder);
        }
        self.rust();
    }

    /// De microfoon viel weg: de opname gaat weg en de gebruiker krijgt de melding.
    fn breek_af(&mut self, id: TaakId, melding: Foutmelding) {
        if !self.besturing.annuleer() {
            return;
        }
        esc_uit(&self.app);
        if let Some(lopend) = self.lopend.take() {
            pil::opname_loopt(None);
            geluid::speel(&self.app, Toon::Stop);
            let _ = lopend.beheerder.mislukt(id, verfijn(melding, false));
            drop(lopend.opname);
            laat_los(lopend.beheerder);
        }
        self.rust();
    }

    /// De knop "Annuleren" in het venster. Tijdens het uitschrijven maakt dit de sneltoets
    /// meteen vrij; het werkproces maakt zijn lopende werk af en negeert het resultaat.
    fn annuleer_taak(&mut self, id: TaakId) {
        if self.is_lopend(id) {
            self.annuleer_opname();
        } else if let Some((verwerkt, beheerder, keten)) = &self.verwerkt
            && *verwerkt == id
        {
            if slot(keten).annuleer() {
                // De uitslagdraad schoont op of plakt al: het opschonen stopt meteen (de
                // letterlijke tekst wordt geplakt), en de sneltoets komt vrij als die draad
                // Afgelopen meldt. Zo loopt er nooit een tweede dictaat doorheen.
                crate::opschonen::stop_voor(&self.app, id);
                return;
            }
            let _ = beheerder.annuleer(id);
            self.laat_verwerkt_los();
            self.rust();
        }
    }
}

/// Waar de uitslagdraad van een dictaat is, gedeeld met de werker, zodat Annuleren en die draad
/// elkaar nooit missen.
#[derive(Debug, Default)]
struct Keten {
    /// Het dictaat is geannuleerd voordat de draad de tekst had: die doet niets meer.
    geannuleerd: bool,
    /// De draad heeft de tekst en schoont op of plakt.
    bezig: bool,
}

impl Keten {
    /// De draad heeft de tekst. Mag hij door (opschonen, plakken)? Niet na Annuleren.
    fn begin(&mut self) -> bool {
        if self.geannuleerd {
            return false;
        }
        self.bezig = true;
        true
    }

    /// Annuleren in het venster. `true`: de draad is al bezig, de werker wacht op Afgelopen.
    /// `false`: de draad slaat alles over, de werker is meteen vrij.
    fn annuleer(&mut self) -> bool {
        if self.bezig {
            return true;
        }
        self.geannuleerd = true;
        false
    }
}

/// Waarom een dictaat bij de sneltoets niet start door een lopende proef of test.
#[derive(Debug, PartialEq, Eq)]
enum Blokkade {
    Geen,
    /// De gewone proef loopt: de toets doet niets (de proef toont zelf de voortgang).
    Proef,
    /// De snelheidstest van de videokaart loopt: toon en melding, geen stille toets.
    Snelheidstest,
}

/// De snelheidstest gaat voor de proef: zij zet haar eigen vlag en daarna de proefvlag.
fn blokkade_door_proef(proef_bezig: bool, test_bezig: bool) -> Blokkade {
    match (proef_bezig, test_bezig) {
        (_, true) => Blokkade::Snelheidstest,
        (true, false) => Blokkade::Proef,
        (false, false) => Blokkade::Geen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annuleren_tijdens_opschonen_wacht_op_de_draad() {
        // De draad heeft de tekst en schoont op: annuleren maakt de sneltoets niet vrij.
        let mut keten = Keten::default();
        assert!(keten.begin());
        assert!(keten.annuleer());
    }

    #[test]
    fn annuleren_voor_de_uitslag_laat_de_draad_niets_meer_doen() {
        let mut keten = Keten::default();
        assert!(!keten.annuleer());
        // De uitslag komt daarna toch: niet opschonen, niet plakken.
        assert!(!keten.begin());
    }

    #[test]
    fn de_snelheidstest_krijgt_een_melding_en_de_proef_niet() {
        assert_eq!(blokkade_door_proef(false, false), Blokkade::Geen);
        assert_eq!(blokkade_door_proef(true, false), Blokkade::Proef);
        assert_eq!(blokkade_door_proef(true, true), Blokkade::Snelheidstest);
    }
}
