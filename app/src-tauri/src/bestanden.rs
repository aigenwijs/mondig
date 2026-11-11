//! Het scherm Bestanden (sprint 4-B): bestanden kiezen of slepen, de wachtrij met voortgang
//! en annuleren, en het transcript met tijdcodes. Het werk zelf doet de wachtrij uit
//! `mondig-core` op het achtergrondwerkproces (ADR 0012); dit bestand houdt de lijst bij
//! die de interface toont en vertaalt opdrachten en gebeurtenissen:
//!
//! - event `bestanden-gewijzigd`: de hele [`BestandenStand`] na elke wijziging, met een
//!   oplopend volgnummer, zodat de interface een oudere stand kan negeren;
//! - event `taak`: elke [`Gebeurtenis`] van een bestand (voor de resterende tijd en de
//!   schermlezer);
//! - event `bestanden-slepen`: `true` zolang er bestanden boven het venster hangen.
//!
//! Levert een drop van een andere app niets op (geen pad en geen bestandsbelofte, alleen te
//! zien met de Mac-haak), dan staat dat in de stand (`nietGepakt`) en zegt het scherm in gewone
//! taal wat wel kan. Een lege drop van binnen Mondig blijft stil. Op de Mac haalt een haak in
//! de webview bestandsbeloftes op, zoals van Dictafoon, Foto's en Mail (ADR 0030).
//!
//! Een fout bij toevoegen (geen model, gestopte wachtrij) staat in de stand, zodat een
//! scherm dat pas na het slepen opent hem nog ziet ([`bestanden_stand`]).
//!
//! Gesleepte bestanden komen via het venster-event van Tauri binnen, niet via de webview:
//! de interface geeft nooit zelf een pad door. De app leest van een bestand alleen de kop
//! van de container (de duur); decoderen gebeurt in het werkproces (ADR 0009 en 0015).
//! Transcripttekst blijft in het geheugen en gaat nooit naar een log. Bewaren en exporteren
//! (4-C) staan in `bewaren.rs`; dit bestand onthoudt per transcript het label en de bestanden
//! die Mondig ervan opsloeg.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de State per waarde mee; een verwijzing kan niet"
)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mondig_core::aanbieder::{LokaleEngine, Uitgeschreven};
use mondig_core::afspelen;
use mondig_core::audio::{self, EXTENSIES};
use mondig_core::bestanden::{Bestandsuitslag, Wachtrij};
use mondig_core::bewaren::naam::{eigen_waarde, standaard_datum};
use mondig_core::bewaren::{self, Document, Regel};
use mondig_core::bewerken::Bewerkingen;
use mondig_core::download::{self, ModelStatus};
use mondig_core::engine::protocol::Apparaat;
use mondig_core::engine::supervisor::Instellingen;
use mondig_core::gesprek::{Sprekerlabels, schone_naamveld};
use mondig_core::logboek::{self, Gebeurtenis as LogGebeurtenis, SleepbordType};
use mondig_core::model::{Gebruik, Model};
use mondig_core::persoonsgegevens::Markeringen;
use mondig_core::proef;
use mondig_core::soevereiniteit::Soevereiniteit;
use mondig_core::taak::{Foutmelding, Gebeurtenis, Soort, TaakId, Toestand, Vervolg};
use serde::Serialize;
use tauri::{AppHandle, DragDropEvent, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::EnginePad;
use crate::bewaakte_map::{self, Bewaakt};
use crate::bewaren::Opgeslagen;
use crate::dicteerketen::{model_voor, slot};
use crate::modelgebruik::Gevolgd;
use crate::persoon::{
    MarkeringInfo, PersoonInfo, markeerfout, markeringen_info, persoon_info, vervang_velden,
};

/// Een transcript dat klaarstaat. Bewust zonder `Debug`: de tekst mag nooit in een log.
struct Klaar {
    uitgeschreven: Uitgeschreven,
    /// Wanneer het klaar was, in milliseconden sinds 1970.
    op_ms: u64,
    /// Het model waarmee het is uitgeschreven, kort ("Parakeet Ultra").
    model: String,
    /// De regels die de gebruiker verbeterde (V-1), per plek in [`Uitgeschreven::uitingen`].
    /// Scherm, Kopiëren en alle exports lezen hieruit, nooit direct de tekst van het model.
    bewerkt: Bewerkingen,
    /// De markeringen van persoonsgegevens (V-2), alleen in het geheugen.
    persoon: Markeringen,
}

/// Eén bestand in de lijst.
struct Item {
    taak: TaakId,
    naam: String,
    /// Waar het bronbestand staat, om een regel af te spelen (V-1). Nooit naar de interface.
    pad: PathBuf,
    /// Het model waarmee dit bestand wordt uitgeschreven: het model van de wachtrij op het
    /// moment dat het erin kwam. Leeg voor een bestand dat niet in de wachtrij kwam.
    model_id: String,
    /// De korte naam van dat model ("Parakeet Ultra"), voor het transcript.
    model_naam: String,
    video: bool,
    duur_ms: Option<u64>,
    toestand: Toestand,
    /// Wanneer het uitschrijven begon, voor "klaar in 1 min 48 s".
    begin: Option<Instant>,
    verwerking_ms: Option<u64>,
    transcript: Option<Klaar>,
    /// De klant die de gebruiker invulde, voor de naam en de gegevens van het document.
    klant: String,
    /// Het onderwerp voor de bestandsnaam. `None`: nooit aangeraakt, dan geldt de naam van de
    /// opname (zonder extensie).
    onderwerp: Option<String>,
    /// De datum voor de bestandsnaam. `None`: de datum waarop het transcript klaar was.
    datum: Option<String>,
    /// De bestanden die Mondig van dit transcript opsloeg, het laatste achteraan.
    opgeslagen: Vec<Opgeslagen>,
    /// Is het transcript na het opslaan nog verbeterd of anders gemarkeerd? Dan staat er in het
    /// opgeslagen bestand nog de oude tekst.
    verouderd: bool,
    /// Lukte automatisch opslaan niet, dan staat hier waarom.
    bewaar_fout: Option<String>,
    /// Komt dit bestand uit de bewaakte map? Dan slaat Mondig het transcript vanzelf op en
    /// onthoudt het bestand als gedaan.
    bewaakt: Option<Bewaakt>,
    /// Is dit bestand uit de bewaakte map overgeslagen omdat er al een transcript van is?
    overgeslagen: bool,
}

impl Item {
    fn nieuw(
        taak: TaakId,
        naam: String,
        pad: PathBuf,
        video: bool,
        duur_ms: Option<u64>,
        toestand: Toestand,
    ) -> Self {
        Self {
            taak,
            naam,
            pad,
            model_id: String::new(),
            model_naam: String::new(),
            video,
            duur_ms,
            toestand,
            begin: None,
            verwerking_ms: None,
            transcript: None,
            klant: String::new(),
            onderwerp: None,
            datum: None,
            opgeslagen: Vec::new(),
            verouderd: false,
            bewaar_fout: None,
            bewaakt: None,
            overgeslagen: false,
        }
    }
}

/// Een bestand zoals de interface het ziet.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "de vorm die de interface krijgt: elk veld is een eigen feit over het bestand"
)]
pub(crate) struct BestandInfo {
    taak: TaakId,
    naam: String,
    video: bool,
    duur_ms: Option<u64>,
    toestand: Toestand,
    verwerking_ms: Option<u64>,
    heeft_transcript: bool,
    /// De naam van het laatst opgeslagen bestand, als Mondig er een opsloeg.
    opgeslagen: Option<String>,
    /// Is het transcript na het opslaan nog verbeterd? Dan klopt het opgeslagen bestand niet meer.
    opgeslagen_verouderd: bool,
    /// Waarom automatisch opslaan niet lukte, als dat zo is.
    bewaar_fout: Option<String>,
    /// Kwam dit bestand vanzelf uit de bewaakte map?
    uit_map: bool,
    /// Overgeslagen omdat er al een transcript van is (bewaakte map).
    overgeslagen: bool,
}

impl From<&Item> for BestandInfo {
    fn from(item: &Item) -> Self {
        Self {
            taak: item.taak,
            naam: item.naam.clone(),
            video: item.video,
            duur_ms: item.duur_ms,
            toestand: item.toestand.clone(),
            verwerking_ms: item.verwerking_ms,
            heeft_transcript: item.transcript.is_some(),
            opgeslagen: item.opgeslagen.last().map(Opgeslagen::naam),
            opgeslagen_verouderd: item.verouderd && !item.opgeslagen.is_empty(),
            bewaar_fout: item.bewaar_fout.clone(),
            uit_map: item.bewaakt.is_some(),
            overgeslagen: item.overgeslagen,
        }
    }
}

/// De hele stand van het scherm: de lijst, een eventuele fout bij toevoegen, en een
/// volgnummer dat bij elke wijziging oploopt.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BestandenStand {
    versie: u64,
    lijst: Vec<BestandInfo>,
    fout: Option<Foutmelding>,
    /// De laatste drop leverde geen bestand op; het scherm zegt wat wel kan (ADR 0030).
    niet_gepakt: bool,
}

/// Eén stuk tekst met tijdcodes, voor de interface.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TranscriptRegel {
    /// De plek van de regel in het transcript, voor `bestand_zet_tekst`.
    index: usize,
    start_ms: u64,
    eind_ms: u64,
    tekst: String,
    /// "Spreker 1", als de sprekers herkend zijn (4b-D); anders niet aanwezig.
    spreker: Option<String>,
    /// De persoonsgegevens in de tekst (V-2), in UTF-16-eenheden.
    markeringen: Vec<MarkeringInfo>,
    /// De tekst voor Kopiëren: vervangen als "Persoonsgegevens vervangen" aan staat.
    uitvoer: String,
}

/// Het transcript van één bestand, voor het scherm met tijdcodes.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BestandTranscript {
    taak: TaakId,
    naam: String,
    duur_ms: u64,
    klaar_op_ms: u64,
    model: String,
    soevereiniteit: Soevereiniteit,
    /// Een melding van het filter in plaats van (of naast) tekst, zoals "Geen geluid gehoord."
    melding: Option<&'static str>,
    /// Wat bij het lezen opviel, zoals een afgebroken bestand, in gewone taal.
    waarschuwing: Option<String>,
    regels: Vec<TranscriptRegel>,
    /// De klant zoals de gebruiker hem invulde; leeg zonder klant.
    klant: String,
    /// Het onderwerp voor de bestandsnaam: wat de gebruiker invulde, anders de naam van de
    /// opname, als echte tekst.
    onderwerp: String,
    /// De datum voor de bestandsnaam (2026-11-02): wat de gebruiker invulde, anders de datum
    /// waarop het transcript klaar was.
    datum: String,
    /// Staat het bronbestand er nog? Zo niet, dan kan een regel niet worden afgespeeld (V-1).
    audio_aanwezig: bool,
    /// De stand van "Persoonsgegevens markeren en vervangen" (V-2).
    persoon: PersoonInfo,
}

/// De lijst met het volgnummer en de laatste fout bij toevoegen.
#[derive(Default)]
struct Lijst {
    versie: u64,
    items: Vec<Item>,
    fout: Option<Foutmelding>,
    /// Zie [`BestandenStand`]; gaat weg bij het volgende toevoegen.
    niet_gepakt: bool,
}

impl Lijst {
    fn stand(&self) -> BestandenStand {
        BestandenStand {
            versie: self.versie,
            lijst: self.items.iter().map(BestandInfo::from).collect(),
            fout: self.fout.clone(),
            niet_gepakt: self.niet_gepakt,
        }
    }

    fn item(&mut self, taak: TaakId) -> Option<&mut Item> {
        self.items.iter_mut().find(|i| i.taak == taak)
    }
}

/// Waar de uitslag van elk bestand heen gaat: één verzamelthread voor alle bestanden.
type Verzamelaar = mpsc::Sender<(TaakId, Bestandsuitslag)>;

/// De wachtrij met het model dat hij nu gebruikt.
struct Rij {
    wachtrij: Arc<Wachtrij>,
    verzamelaar: Verzamelaar,
    /// Het model voor elk bestand dat er nu bij komt. Een wissel (`Wachtrij::wissel`) staat in
    /// de rij als een bestand, dus wat al in de rij stond, gaat door het vorige model.
    model_id: String,
    model_naam: String,
    /// Het apparaat van de motor voor elk bestand dat er nu bij komt (de schakelaar
    /// "Videokaart gebruiken"). Een wissel ervan werkt als een wissel van model.
    apparaat: Apparaat,
}

impl Rij {
    /// Draait de rij al met dit model en dit apparaat?
    fn past_bij(&self, model: &str, apparaat: Apparaat) -> bool {
        self.model_id == model && self.apparaat == apparaat
    }
}

/// Wat de app over bestanden onthoudt.
#[derive(Default)]
pub(crate) struct BestandenStaat {
    /// Wordt pas gemaakt bij het eerste bestand: het model kan na de start gedownload zijn.
    /// Altijd voor `lijst` vergrendelen.
    rij: Mutex<Option<Rij>>,
    lijst: Arc<Mutex<Lijst>>,
    /// Bestanden die Mondig opsloeg van een taak die niet in de lijst staat, zoals het verslag
    /// van een vergadering. Zo werken Openen en Map tonen voor beide.
    losse: Mutex<HashMap<TaakId, Vec<Opgeslagen>>>,
    /// Telt elke gebeurtenis van de wachtrij (bestanden en vergaderingen). Staat de teller lang
    /// stil terwijl er werk wacht, dan is het werkproces vastgelopen en houdt Mondig de
    /// computer niet langer wakker (`wakker.rs`).
    activiteit: AtomicU64,
    /// De stopvlag van het stuk geluid dat nu wordt klaargezet (V-1): een nieuwe klik op een
    /// regel stopt het vorige, zodat er nooit twee lange leesbeurten tegelijk lopen.
    afspelen: Mutex<Option<Arc<AtomicBool>>>,
    /// De standaardnamen van de sprekers in de taal van de app, voor scherm, Kopiëren en Opslaan.
    /// De interface geeft ze door ([`crate::vergadering::vergadering_zet_labels`]); tot dan zijn ze
    /// Nederlands.
    labels: Mutex<Sprekerlabels>,
}

impl BestandenStaat {
    /// Onthoudt de standaardnamen van de sprekers zoals de interface ze doorgaf.
    pub(crate) fn zet_labels(&self, labels: Sprekerlabels) {
        *slot(&self.labels) = labels;
    }

    fn sprekerlabels(&self) -> Sprekerlabels {
        slot(&self.labels).clone()
    }

    /// Is er een bestand aan het wachten of aan het werk?
    pub(crate) fn is_bezig(&self) -> bool {
        slot(&self.lijst)
            .items
            .iter()
            .any(|item| !item.toestand.is_afgelopen())
    }

    /// Wacht of loopt er een bestand met dit model?
    fn is_bezig_met(&self, model: &str) -> bool {
        slot(&self.lijst)
            .items
            .iter()
            .any(|item| item.model_id == model && !item.toestand.is_afgelopen())
    }
}

/// De teller van gebeurtenissen van de wachtrij.
pub(crate) fn activiteit(app: &AppHandle) -> u64 {
    app.state::<BestandenStaat>()
        .activiteit
        .load(Ordering::Relaxed)
}

/// Is er een bestand aan het wachten of aan het werk?
pub(crate) fn is_bezig(app: &AppHandle) -> bool {
    app.state::<BestandenStaat>().is_bezig()
}

/// Het bestandsmodel of de videokaart kan veranderd zijn: de wachtrij krijgt dan een motor
/// met het nieuwe model of apparaat. Wat al in de rij staat, maakt de oude motor af; elk
/// bestand dat daarna komt, gaat door de nieuwe. Is er nog geen wachtrij, dan begint de
/// eerste met de juiste motor.
pub(crate) fn ververs_wachtrij(app: &AppHandle) {
    let staat = app.state::<BestandenStaat>();
    let Ok((model, map)) = model_voor(app, Gebruik::Bestanden) else {
        return;
    };
    let apparaat = crate::videokaart::gewenst_apparaat(app);
    let mut rij = slot(&staat.rij);
    if let Some(rij) = rij.as_mut()
        && !rij.past_bij(&model.id, apparaat)
    {
        // Lukt het niet (het model staat er niet helemaal), dan probeert het volgende
        // bestand het opnieuw via `haal_wachtrij`.
        let _ = wissel(app, rij, &model, map);
    }
}

/// Geeft de wachtrij een motor met dit model.
fn wissel(app: &AppHandle, rij: &mut Rij, model: &Model, map: PathBuf) -> Result<(), Foutmelding> {
    let apparaat = crate::videokaart::gewenst_apparaat(app);
    let motor = maak_motor(app, model, map, apparaat)?;
    rij.wachtrij.wissel(motor).map_err(|_| {
        Foutmelding::nieuw(
            "Het uitschrijven is gestopt. Start Mondig opnieuw.",
            Vervolg::Geen,
        )
    })?;
    rij.model_id.clone_from(&model.id);
    rij.model_naam = korte_naam(&model.naam);
    rij.apparaat = apparaat;
    Ok(())
}

/// Laat de wachtrij dit model los, zodat het van schijf kan. Wacht niet: stoppen gebeurt op
/// de achtergrond, en `model_verwijder` wacht via het register in `modelgebruik` tot het
/// werkproces het model echt heeft losgelaten.
///
/// # Errors
///
/// Geeft een melding als er bestanden of een vergadering met dit model wachten of worden
/// uitgeschreven.
pub(crate) fn geef_model_vrij(app: &AppHandle, id: &str) -> Result<(), Foutmelding> {
    let staat = app.state::<BestandenStaat>();
    let mut rij = slot(&staat.rij);
    if staat.is_bezig_met(id) {
        return Err(Foutmelding::nieuw(
            "Er worden bestanden uitgeschreven met dit model. Probeer het opnieuw als ze klaar zijn.",
            Vervolg::OpnieuwProberen,
        ));
    }
    // Een vergadering gaat door dezelfde wachtrij, maar staat niet in de lijst van Bestanden.
    // Hij houdt zijn eigen model bij: na een wissel staat in de rij al het nieuwe model.
    if crate::vergadering::schrijft_uit(app, id) {
        return Err(Foutmelding::nieuw(
            "Er wordt een vergadering uitgeschreven met dit model. Probeer het opnieuw als die klaar is.",
            Vervolg::OpnieuwProberen,
        ));
    }
    let met_dit_model = rij.as_ref().is_some_and(|r| r.model_id == id);
    if met_dit_model
        && !staat.is_bezig()
        && let Some(oude) = rij.take()
    {
        drop(rij);
        std::thread::spawn(move || oude.wachtrij.stop());
    }
    Ok(())
}

impl BestandenStaat {
    /// Stopt de wachtrij bij het afsluiten: annuleert wat loopt en wacht, en wacht hooguit
    /// twee seconden. Daarna sluit de invoer van het werkproces en stopt het zelf.
    pub(crate) fn stop(&self) {
        let Some(rij) = slot(&self.rij).take() else {
            return;
        };
        let (klaar, wacht) = mpsc::channel();
        std::thread::spawn(move || {
            rij.wachtrij.stop();
            let _ = klaar.send(());
        });
        let _ = wacht.recv_timeout(Duration::from_secs(2));
    }

    pub(crate) fn wachtrij(&self) -> Option<Arc<Wachtrij>> {
        slot(&self.rij).as_ref().map(|r| Arc::clone(&r.wachtrij))
    }
}

/// Geeft een draaiende wachtrij het filter zoals de instellingen het nu zeggen (de schakelaar
/// "Stopwoorden weghalen"). Zonder wachtrij is er niets te doen: de eerstvolgende krijgt het
/// goede filter bij het starten.
pub(crate) fn werk_bestandfilter_bij(app: &AppHandle) {
    if let Some(wachtrij) = app.state::<BestandenStaat>().wachtrij() {
        wachtrij.zet_filter(Some(crate::instellingen::bestand_filter(app)));
    }
}

/// Werkt de lijst bij, verhoogt het volgnummer en stuurt de nieuwe stand naar de interface.
fn wijzig(
    app: &AppHandle,
    lijst: &Mutex<Lijst>,
    wijziging: impl FnOnce(&mut Lijst),
) -> BestandenStand {
    let stand = {
        let mut lijst = slot(lijst);
        wijziging(&mut lijst);
        lijst.versie += 1;
        lijst.stand()
    };
    let _ = app.emit("bestanden-gewijzigd", stand.clone());
    stand
}

fn nu_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn ms(duur: Duration) -> u64 {
    u64::try_from(duur.as_millis()).unwrap_or(u64::MAX)
}

/// De naam zonder de uitleg tussen haakjes: "Parakeet Ultra (standaard, snel)" wordt "Parakeet Ultra".
pub(crate) fn korte_naam(naam: &str) -> String {
    naam.split(" (").next().unwrap_or(naam).trim().to_string()
}

/// Geeft de wachtrij met het id en de korte naam van het model waarmee hij nu uitschrijft aan
/// `werk`, onder het slot van de rij. Zo kan een vergadering in de wachtrij komen en zijn
/// model noteren zonder dat een modelwissel ertussen valt (net als `voeg_toe` voor bestanden).
/// Vergaderingen (sprint 4b-C) gaan door dezelfde rij als bestanden, dus met het model voor
/// bestanden; hun gebeurtenissen gaan naar `vergadering.rs`. Slotvolgorde: eerst de rij, dan
/// de toestand van de vergadering.
pub(crate) fn met_wachtrij<T>(
    app: &AppHandle,
    werk: impl FnOnce(&Wachtrij, &str, &str) -> T,
) -> Result<T, Foutmelding> {
    haal_wachtrij(app)?;
    let staat = app.state::<BestandenStaat>();
    let rij = slot(&staat.rij);
    rij.as_ref()
        .map(|rij| werk(&rij.wachtrij, &rij.model_id, &rij.model_naam))
        .ok_or_else(|| {
            Foutmelding::nieuw(
                "Het uitschrijven is gestopt. Start Mondig opnieuw.",
                Vervolg::Geen,
            )
        })
}

/// Een motor met dit model voor opnieuw uitschrijven (ADR 0023), op het apparaat dat nu geldt.
/// De wachtrij stopt eerst zijn eigen werkproces voordat deze begint, en laat hem na afloop weer
/// los; zolang hij bestaat, staat het model als in gebruik geregistreerd.
pub(crate) fn motor_voor(
    app: &AppHandle,
    model: &Model,
    map: PathBuf,
) -> Result<Gevolgd<LokaleEngine>, Foutmelding> {
    maak_motor(app, model, map, crate::videokaart::gewenst_apparaat(app))
}

/// Een motor voor de wachtrij met dit model, gevolgd in het register van `modelgebruik`.
fn maak_motor(
    app: &AppHandle,
    model: &Model,
    map: PathBuf,
    apparaat: Apparaat,
) -> Result<Gevolgd<LokaleEngine>, Foutmelding> {
    let engine_pad = app.state::<EnginePad>().0.clone();
    let compleet = download::status(model, &map) == ModelStatus::Compleet;
    if let Some(melding) = proef::ontbrekend(engine_pad.is_some(), compleet) {
        return Err(melding);
    }
    let Some(engine_pad) = engine_pad else {
        return Err(Foutmelding::nieuw(
            "Het spraakprogramma ontbreekt bij Mondig. Installeer Mondig opnieuw.",
            Vervolg::Geen,
        ));
    };
    // Het achtergrondwerkproces op een lagere prioriteit: een dictaat gaat voor (ADR 0012).
    // Het apparaat volgt de schakelaar "Videokaart gebruiken" (ADR 0011, aanvulling 2-11-2026).
    let mut instellingen = Instellingen::achtergrond(engine_pad);
    // Net als bij dicteren: een werkproces uit een andere uitgave valt meteen op (ADR 0016).
    instellingen.bouwcontrole = crate::versie::bouwcontrole();
    let engine = LokaleEngine::nieuw(
        instellingen,
        model,
        map,
        apparaat,
        Some(crate::taal::MODELTAAL.to_string()),
    )
    .met_melder(crate::videokaart::melder(
        app,
        crate::videokaart::Route::Bestanden,
    ));
    Ok(Gevolgd::nieuw(engine, &model.id))
}

/// Zorgt dat de wachtrij er is, met het model dat nu geldt: gemaakt bij het eerste gebruik,
/// of met een wissel als het model intussen veranderde. Twee threads eromheen: een die de
/// gebeurtenissen in de lijst verwerkt en doorstuurt, en één die alle uitslagen verzamelt.
fn haal_wachtrij(app: &AppHandle) -> Result<(), Foutmelding> {
    crate::videokaart::weiger_tijdens_test(app)?;
    let staat = app.state::<BestandenStaat>();
    let (model, map) = model_voor(app, Gebruik::Bestanden)?;
    let apparaat = crate::videokaart::gewenst_apparaat(app);
    let mut houder = slot(&staat.rij);
    if let Some(rij) = houder.as_mut() {
        if !rij.past_bij(&model.id, apparaat) {
            wissel(app, rij, &model, map)?;
            // Een ander model kan een andere gesproken taal hebben.
            rij.wachtrij
                .zet_filter(Some(crate::instellingen::bestand_filter(app)));
        }
        return Ok(());
    }
    let motor = maak_motor(app, &model, map, apparaat)?;

    let (zender, gebeurtenissen) = mpsc::channel::<Gebeurtenis>();
    let lijst = Arc::clone(&staat.lijst);
    let doorsturen = app.clone();
    std::thread::spawn(move || {
        for gebeurtenis in gebeurtenissen {
            doorsturen
                .state::<BestandenStaat>()
                .activiteit
                .fetch_add(1, Ordering::Relaxed);
            if gebeurtenis.soort == Soort::Vergadering {
                crate::vergadering::bij_gebeurtenis(&doorsturen, &gebeurtenis);
            } else {
                wijzig(&doorsturen, &lijst, |lijst| werk_bij(lijst, &gebeurtenis));
            }
            crate::logboek::taak(&gebeurtenis);
            let _ = doorsturen.emit("taak", &gebeurtenis);
        }
    });

    let (verzamelaar, uitslagen) = mpsc::channel::<(TaakId, Bestandsuitslag)>();
    let lijst = Arc::clone(&staat.lijst);
    let verzamelen = app.clone();
    std::thread::spawn(move || {
        for (taak, uitslag) in uitslagen {
            // Fout en annuleren zijn al als gebeurtenis gemeld door de wachtrij.
            match uitslag {
                Bestandsuitslag::Klaar(uitgeschreven) => {
                    // Wie het bestand bewaakte, nu vastleggen: haalt de gebruiker het intussen
                    // uit de lijst, dan blijft het opslaan en het noteren als gedaan staan.
                    let bewaakt = verzamelen.state::<BestandenStaat>().bewaakt(taak);
                    wijzig(&verzamelen, &lijst, |lijst| {
                        if let Some(item) = lijst.item(taak) {
                            item.transcript = Some(Klaar {
                                uitgeschreven,
                                op_ms: nu_ms(),
                                model: item.model_naam.clone(),
                                bewerkt: Bewerkingen::default(),
                                persoon: Markeringen::default(),
                            });
                        }
                    });
                    // Na het uitschrijven meteen opslaan, als de gebruiker dat aanzette of het
                    // bestand uit de bewaakte map komt (dan onthoudt `automatisch` ook dat het
                    // gedaan is).
                    crate::bewaren::automatisch(&verzamelen, taak, bewaakt);
                }
                Bestandsuitslag::Geannuleerd => {
                    // De gebruiker besloot dit bestand niet uit te schrijven: de bewaakte map
                    // pakt het niet opnieuw op.
                    let bewaakt = verzamelen.state::<BestandenStaat>().bewaakt(taak);
                    if let Some(bewaakt) = bewaakt {
                        bewaakte_map::markeer_gedaan(&verzamelen, &bewaakt);
                    }
                }
                Bestandsuitslag::Fout(_) => {}
            }
        }
    });

    let wachtrij = Arc::new(Wachtrij::start(
        motor,
        Some(crate::instellingen::bestand_filter(app)),
        zender,
    ));
    *houder = Some(Rij {
        wachtrij,
        verzamelaar,
        model_id: model.id.clone(),
        model_naam: korte_naam(&model.naam),
        apparaat,
    });
    Ok(())
}

/// Werkt de lijst bij met een gebeurtenis van de wachtrij: toestand, begin en verwerkingstijd.
fn werk_bij(lijst: &mut Lijst, gebeurtenis: &Gebeurtenis) {
    // Een bestand dat al uit de lijst is gehaald, hoeft niet meer bijgewerkt.
    let Some(item) = lijst.item(gebeurtenis.taak) else {
        return;
    };
    item.toestand = gebeurtenis.toestand.clone();
    match gebeurtenis.toestand {
        Toestand::Verwerkt { .. } if item.begin.is_none() => item.begin = Some(Instant::now()),
        Toestand::Klaar => item.verwerking_ms = item.begin.map(|b| ms(b.elapsed())),
        _ => {}
    }
}

/// Zet bestanden achteraan in de wachtrij en geeft de nieuwe stand. Een bestand dat Mondig
/// niet kan openen, komt in de lijst met de melding wat je kunt doen, zonder dat het
/// werkproces ervoor start. Lukt toevoegen niet (geen model, gestopte wachtrij), dan staat
/// die fout in de stand, naast wat wel lukte.
fn voeg_toe(app: &AppHandle, paden: Vec<PathBuf>) -> BestandenStand {
    voeg_toe_met(app, paden, None).0
}

/// De uitkomst van [`voeg_toe_bewaakt`].
pub(crate) enum Toevoeging {
    /// Het bestand staat in de wachtrij, of als bestand dat Mondig niet kan lezen in de lijst.
    Behandeld,
    /// Toevoegen lukte niet (geen model, gestopte wachtrij). De fout komt niet in de foutbalk
    /// van Bestanden: de bewaakte map meldt hem een keer in Instellingen en probeert het later
    /// opnieuw.
    Mislukt(Foutmelding),
}

/// Zoals [`voeg_toe`], voor een bestand uit de bewaakte map.
pub(crate) fn voeg_toe_bewaakt(app: &AppHandle, bewaakt: &Bewaakt) -> Toevoeging {
    let (_, fout, behandeld) = voeg_toe_met(app, vec![bewaakt.pad.clone()], Some(bewaakt));
    match fout {
        Some(fout) if !behandeld => Toevoeging::Mislukt(fout),
        _ => Toevoeging::Behandeld,
    }
}

/// Zet een bestand uit de bewaakte map in de lijst als overgeslagen, omdat er al een transcript
/// van is, en onthoudt het als gedaan. Zo ziet de gebruiker dat Mondig het bestand zag.
pub(crate) fn voeg_overgeslagen_toe(app: &AppHandle, bewaakt: &Bewaakt) {
    let staat = app.state::<BestandenStaat>();
    let naam = bewaakt.pad.file_name().map_or_else(
        || bewaakt.pad.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let mut item = Item::nieuw(
        TaakId::nieuw(),
        naam,
        bewaakt.pad.clone(),
        audio::is_video(&bewaakt.pad),
        None,
        Toestand::Geannuleerd,
    );
    item.bewaakt = Some(bewaakt.clone());
    item.overgeslagen = true;
    wijzig(app, &staat.lijst, |lijst| lijst.items.push(item));
    bewaakte_map::markeer_gedaan(app, bewaakt);
}

/// Het werk van [`voeg_toe`] en [`voeg_toe_bewaakt`]; `bewaakt` hoort bij een lijst met
/// precies dat ene pad.
fn voeg_toe_met(
    app: &AppHandle,
    paden: Vec<PathBuf>,
    bewaakt: Option<&Bewaakt>,
) -> (BestandenStand, Option<Foutmelding>, bool) {
    let staat = app.state::<BestandenStaat>();
    if let Err(fout) = haal_wachtrij(app) {
        // Een bewaakt bestand meldt de fout niet in de foutbalk van Bestanden: die zou elke
        // paar seconden terugkomen zolang het model ontbreekt.
        let stand = if bewaakt.is_some() {
            slot(&staat.lijst).stand()
        } else {
            wijzig(app, &staat.lijst, |lijst| {
                lijst.fout = Some(fout.clone());
                lijst.niet_gepakt = false;
            })
        };
        return (stand, Some(fout), false);
    }
    let mut behandeld = false;
    let mut fout = None;
    // De instelling "Sprekers herkennen" geldt voor elk bestand dat nu binnenkomt; zonder
    // sprekermodel gebeurt het toch niet.
    let sprekers = crate::instellingen::lees_sprekers_herkennen(app);
    for pad in paden {
        let naam = pad.file_name().map_or_else(
            || pad.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let video = audio::is_video(&pad);
        // Alleen de kop van de container: kan Mondig het bestand niet openen, dan weten we
        // dat meteen. De duur mag onbekend zijn; die telt het werkproces dan zelf.
        match audio::duur(&pad) {
            Ok(duur) => {
                // Binnen het slot van de rij, zodat een wissel van model niet tussen het
                // toevoegen en het noteren van het model valt; en binnen het slot van de lijst,
                // zodat de eerste gebeurtenis van de wachtrij het item vindt.
                let rij = slot(&staat.rij);
                let mut lijst = slot(&staat.lijst);
                // Opnieuw kijken onder de sloten: een test die na de eerste controle begon,
                // ziet dit bestand dan in de lijst, en anders weigeren wij hier.
                if let Err(melding) = crate::videokaart::weiger_tijdens_test(app) {
                    fout = Some(melding);
                    break;
                }
                let toegevoegd = rij.as_ref().map(|rij| {
                    let verzamelaar = rij.verzamelaar.clone();
                    let taak = rij.wachtrij.voeg_toe_met_sprekers(
                        pad.clone(),
                        sprekers,
                        move |taak, uitslag| {
                            let _ = verzamelaar.send((taak, uitslag));
                        },
                    );
                    (taak, rij.model_id.clone(), rij.model_naam.clone())
                });
                let Some((Ok(taak), model_id, model_naam)) = toegevoegd else {
                    fout = Some(Foutmelding::nieuw(
                        "Het uitschrijven is gestopt. Start Mondig opnieuw.",
                        Vervolg::Geen,
                    ));
                    break;
                };
                let mut item = Item::nieuw(taak, naam, pad, video, duur.map(ms), Toestand::Wacht);
                item.model_id = model_id;
                item.model_naam = model_naam;
                item.bewaakt = bewaakt.cloned();
                lijst.items.push(item);
                behandeld = true;
            }
            Err(leesfout) => {
                let taak = TaakId::nieuw();
                let toestand = Toestand::fout(leesfout.to_string(), Vervolg::Geen);
                let mut item = Item::nieuw(taak, naam, pad, video, None, toestand.clone());
                item.bewaakt = bewaakt.cloned();
                slot(&staat.lijst).items.push(item);
                behandeld = true;
                // Een bestand dat Mondig niet kan lezen, blijft een fout in de lijst; de
                // bewaakte map probeert het niet bij elke start opnieuw.
                if let Some(bewaakt) = bewaakt {
                    bewaakte_map::markeer_gedaan(app, bewaakt);
                }
                let _ = app.emit(
                    "taak",
                    &Gebeurtenis {
                        taak,
                        soort: Soort::Bestand,
                        toestand,
                    },
                );
            }
        }
    }
    let stand = if bewaakt.is_some() {
        slot(&staat.lijst).stand()
    } else {
        wijzig(app, &staat.lijst, |lijst| {
            lijst.fout.clone_from(&fout);
            lijst.niet_gepakt = false;
        })
    };
    (stand, fout, behandeld)
}

/// Laat de gebruiker bestanden kiezen en zet ze in de wachtrij. Geeft de nieuwe stand;
/// sluit de gebruiker de kiezer, dan blijft de lijst zoals hij was.
#[tauri::command]
pub(crate) async fn bestanden_kies(app: AppHandle) -> Result<BestandenStand, Foutmelding> {
    tauri::async_runtime::spawn_blocking(move || {
        let staat = app.state::<BestandenStaat>();
        // Eerst kijken of uitschrijven kan, zodat de gebruiker niet voor niets kiest.
        if let Err(fout) = haal_wachtrij(&app) {
            return wijzig(&app, &staat.lijst, |lijst| lijst.fout = Some(fout));
        }
        let mut kiezer = app.dialog().file();
        if let Some(venster) = app.get_webview_window("main") {
            kiezer = kiezer.set_parent(&venster);
        }
        let gekozen = kiezer
            .add_filter("Opnames en video's", EXTENSIES)
            .blocking_pick_files();
        let paden: Vec<PathBuf> = gekozen
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.into_path().ok())
            .collect();
        if paden.is_empty() {
            return slot(&staat.lijst).stand();
        }
        voeg_toe(&app, paden)
    })
    .await
    .map_err(|_| Foutmelding::nieuw("Kiezen is onverwacht gestopt.", Vervolg::Geen))
}

/// De stand zoals hij nu is, met een fout van het laatste toevoegen (bijvoorbeeld bij
/// slepen vanaf een ander scherm, toen dit scherm nog niet luisterde).
#[tauri::command]
pub(crate) fn bestanden_stand(staat: State<'_, BestandenStaat>) -> BestandenStand {
    slot(&staat.lijst).stand()
}

/// Annuleert een bestand dat wacht of loopt.
#[tauri::command]
pub(crate) fn bestand_annuleer(
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
) -> Result<(), Foutmelding> {
    match staat.wachtrij() {
        Some(wachtrij) => wachtrij
            .annuleer(taak)
            .map_err(|f| Foutmelding::nieuw(f.to_string(), Vervolg::Geen)),
        None => Ok(()),
    }
}

/// Haalt een bestand uit de lijst. Loopt het nog, dan wordt het eerst geannuleerd. Het
/// transcript verdwijnt mee uit het geheugen.
#[tauri::command]
pub(crate) fn bestand_weghalen(
    app: AppHandle,
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
) -> BestandenStand {
    let actief = slot(&staat.lijst)
        .items
        .iter()
        .any(|i| i.taak == taak && i.toestand.is_actief());
    if actief && let Some(wachtrij) = staat.wachtrij() {
        let _ = wachtrij.annuleer(taak);
    }
    // Weghalen is een keuze van de gebruiker: de bewaakte map pakt het bestand niet opnieuw
    // op. Hier noteren, niet bij het bericht "geannuleerd" dat pas later komt, als het item
    // al uit de lijst is.
    if let Some(bewaakt) = staat.bewaakt(taak) {
        bewaakte_map::markeer_gedaan(&app, &bewaakt);
    }
    wijzig(&app, &staat.lijst, |lijst| {
        lijst.items.retain(|i| i.taak != taak);
        lijst.fout = None;
    })
}

/// De naam van de opname zonder extensie: het onderwerp zolang de gebruiker dat niet aanraakte, en
/// de titel van het document.
fn standaard_onderwerp(item: &Item) -> String {
    PathBuf::from(&item.naam)
        .file_stem()
        .map_or_else(|| item.naam.clone(), |s| s.to_string_lossy().into_owned())
}

/// Het transcript van een item, als het er is.
fn maak_transcript(item: &Item, labels: &Sprekerlabels) -> Option<BestandTranscript> {
    let klaar = item.transcript.as_ref()?;
    let gefilterd = &klaar.uitgeschreven.gefilterd;
    let transcript = &gefilterd.transcript;
    Some(BestandTranscript {
        taak: item.taak,
        naam: item.naam.clone(),
        duur_ms: ms(transcript.audio_duur),
        klaar_op_ms: klaar.op_ms,
        model: klaar.model.clone(),
        soevereiniteit: Soevereiniteit::Lokaal,
        melding: gefilterd.uitkomst.melding(),
        waarschuwing: waarschuwing_tekst(&klaar.uitgeschreven),
        regels: scherm_regels(klaar, labels),
        klant: item.klant.clone(),
        onderwerp: item
            .onderwerp
            .clone()
            .unwrap_or_else(|| standaard_onderwerp(item)),
        datum: item
            .datum
            .clone()
            .unwrap_or_else(|| standaard_datum(&bewaren::lokaal_moment(klaar.op_ms))),
        audio_aanwezig: item.pad.is_file(),
        persoon: persoon_info(&klaar.persoon),
    })
}

/// Het transcript van een bestand, of `None` als het er (nog) niet is.
#[tauri::command]
pub(crate) fn bestand_transcript(
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
) -> Option<BestandTranscript> {
    let labels = staat.sprekerlabels();
    let lijst = slot(&staat.lijst);
    maak_transcript(lijst.items.iter().find(|i| i.taak == taak)?, &labels)
}

/// Past de markeringen van persoonsgegevens van een transcript aan (V-2) en geeft het transcript
/// zoals het nu is.
fn wijzig_persoon(
    app: &AppHandle,
    staat: &BestandenStaat,
    taak: TaakId,
    wijziging: impl FnOnce(&mut Markeringen) -> Result<(), Foutmelding>,
) -> Result<BestandTranscript, Foutmelding> {
    let labels = staat.sprekerlabels();
    let (uitslag, veranderd) = {
        let mut lijst = slot(&staat.lijst);
        let item = lijst.item(taak).ok_or_else(geen_transcript)?;
        let klaar = item.transcript.as_mut().ok_or_else(geen_transcript)?;
        let voor = klaar.persoon.clone();
        wijziging(&mut klaar.persoon)?;
        let veranderd = klaar.persoon != voor;
        if veranderd && !item.opgeslagen.is_empty() {
            item.verouderd = true;
        }
        (maak_transcript(item, &labels), veranderd && item.verouderd)
    };
    if veranderd {
        // De lijst toont dan dat het opgeslagen bestand niet meer klopt.
        wijzig(app, &staat.lijst, |_| {});
    }
    uitslag.ok_or_else(geen_transcript)
}

/// Zet "Persoonsgegevens vervangen" aan of uit voor een transcript (V-2): Kopiëren en alle exports
/// gebruiken dan `[BSN]`, `[IBAN]` enzovoort. Er wordt geen sleutel bewaard.
#[tauri::command]
pub(crate) fn bestand_persoon_vervangen(
    app: AppHandle,
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
    aan: bool,
) -> Result<BestandTranscript, Foutmelding> {
    wijzig_persoon(&app, &staat, taak, |persoon| {
        persoon.zet_vervangen(aan);
        Ok(())
    })
}

/// Haalt een markering weg (V-2): een gevonden gegeven wordt in dit transcript overal genegeerd,
/// een zelf gemarkeerd stuk verdwijnt.
#[tauri::command]
pub(crate) fn bestand_persoon_weghalen(
    app: AppHandle,
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
    waarde: String,
) -> Result<BestandTranscript, Foutmelding> {
    wijzig_persoon(&app, &staat, taak, |persoon| {
        persoon.haal_weg(&waarde);
        Ok(())
    })
}

/// Markeert zelf een stuk tekst (V-2), overal in het transcript waar het als los stuk staat.
#[tauri::command]
pub(crate) fn bestand_persoon_markeer(
    app: AppHandle,
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
    waarde: String,
) -> Result<BestandTranscript, Foutmelding> {
    wijzig_persoon(&app, &staat, taak, |persoon| {
        persoon.markeer(&waarde).map_err(markeerfout)
    })
}

/// Verbetert de tekst van één regel van een transcript (V-1) en geeft de regels zoals ze nu zijn.
/// Een lege tekst laat de oude staan. Kopiëren en alle exports gebruiken daarna de nieuwe tekst.
/// De plek is die uit [`TranscriptRegel::index`].
#[tauri::command]
pub(crate) fn bestand_zet_tekst(
    app: AppHandle,
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
    index: usize,
    tekst: String,
) -> Result<Vec<TranscriptRegel>, Foutmelding> {
    let labels = staat.sprekerlabels();
    let (regels, verouderd) = {
        let mut lijst = slot(&staat.lijst);
        let item = lijst.item(taak).ok_or_else(geen_transcript)?;
        let klaar = item.transcript.as_mut().ok_or_else(geen_transcript)?;
        let origineel = klaar
            .uitgeschreven
            .uitingen()
            .get(index)
            .map(|(segment, _)| segment.tekst.trim().to_string())
            .ok_or_else(geen_regel)?;
        let veranderd = klaar.bewerkt.zet(index, &origineel, &tekst);
        let regels = scherm_regels(klaar, &labels);
        if veranderd && !item.opgeslagen.is_empty() {
            item.verouderd = true;
        }
        (regels, veranderd && item.verouderd)
    };
    if verouderd {
        // De lijst toont dan dat het opgeslagen bestand niet meer klopt.
        wijzig(&app, &staat.lijst, |_| {});
    }
    Ok(regels)
}

/// Het stuk geluid van `begin_ms` tot `eind_ms` van het bronbestand, als WAV-bytes (V-1). De
/// interface maakt er een blob van; ze krijgt nooit een pad of bestandsrecht. De tijden zijn
/// die van het transcript. Is het bronbestand weg, dan een melding in gewone taal.
#[tauri::command]
pub(crate) async fn bestand_audio_stuk(
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
    begin_ms: u64,
    eind_ms: u64,
) -> Result<tauri::ipc::Response, Foutmelding> {
    let pad = {
        let lijst = slot(&staat.lijst);
        let item = lijst
            .items
            .iter()
            .find(|i| i.taak == taak && i.transcript.is_some())
            .ok_or_else(geen_transcript)?;
        item.pad.clone()
    };
    // Een nieuw stuk stopt het vorige.
    let vlag = Arc::new(AtomicBool::new(false));
    if let Some(vorige) = slot(&staat.afspelen).replace(Arc::clone(&vlag)) {
        vorige.store(true, Ordering::Relaxed);
    }
    tauri::async_runtime::spawn_blocking(move || {
        match afspelen::fragment_uit_bestand(&pad, begin_ms, eind_ms, &vlag) {
            Ok(Some(bytes)) => Ok(tauri::ipc::Response::new(bytes)),
            Ok(None) => Err(Foutmelding::nieuw(
                "Het afspelen van deze regel is gestopt.",
                Vervolg::Geen,
            )),
            Err(fout) => Err(Foutmelding::nieuw(fout.to_string(), Vervolg::Geen)),
        }
    })
    .await
    .map_err(|_| Foutmelding::nieuw("Dit stopte onverwacht.", Vervolg::OpnieuwProberen))?
}

fn geen_transcript() -> Foutmelding {
    Foutmelding::nieuw("Dit uitschrift staat er niet meer.", Vervolg::Geen)
}

fn geen_regel() -> Foutmelding {
    Foutmelding::nieuw(
        "Deze regel staat niet (meer) in het uitschrift.",
        Vervolg::Geen,
    )
}

// ---------------------------------------------------------------------------
// Voor bewaren.rs: het transcript als document, het label en de opgeslagen bestanden
// ---------------------------------------------------------------------------

impl BestandenStaat {
    /// Het transcript als [`Document`] om te bewaren, met de velden die nu gelden. `None` als
    /// er (nog) geen transcript is.
    pub(crate) fn document(&self, taak: TaakId) -> Option<Document> {
        self.document_met(taak, None)
    }

    /// Een document zonder tekst met de velden zoals de gebruiker ze nu typt (nog niet bewaard),
    /// alleen om de bestandsnaam uit te rekenen. Dezelfde stappen als [`Self::document`].
    pub(crate) fn naam_document(
        &self,
        taak: TaakId,
        klant: &str,
        onderwerp: &str,
        datum: &str,
    ) -> Option<Document> {
        self.document_met(taak, Some((klant, onderwerp, datum)))
    }

    /// Het document van `taak`. Met `getypt` (klant, onderwerp, datum) gelden die velden in plaats
    /// van de bewaarde, en blijft de tekst weg.
    fn document_met(&self, taak: TaakId, getypt: Option<(&str, &str, &str)>) -> Option<Document> {
        let labels = self.sprekerlabels();
        let lijst = slot(&self.lijst);
        let item = lijst.items.iter().find(|i| i.taak == taak)?;
        let klaar = item.transcript.as_ref()?;
        let gefilterd = &klaar.uitgeschreven.gefilterd;
        let titel = standaard_onderwerp(item);
        let moment = bewaren::lokaal_moment(klaar.op_ms);
        let (klant, onderwerp, datum) = match getypt {
            Some((klant, onderwerp, datum)) => (
                schone_naamveld(klant),
                eigen_waarde(onderwerp, &titel),
                eigen_waarde(datum, &standaard_datum(&moment)),
            ),
            None => (
                item.klant.clone(),
                item.onderwerp.clone(),
                item.datum.clone(),
            ),
        };
        let mut document = Document {
            standaard_onderwerp: titel.clone(),
            titel,
            bron: item.naam.clone(),
            model: klaar.model.clone(),
            klant: Some(klant).filter(|k| !k.trim().is_empty()),
            onderwerp,
            datum,
            moment,
            duur_ms: ms(gefilterd.transcript.audio_duur),
            melding: gefilterd.uitkomst.melding().map(str::to_string),
            regels: if getypt.is_some() {
                Vec::new()
            } else {
                export_regels(klaar, &labels)
            },
        };
        // De naam van het bestand, de klant, het onderwerp en de titel kunnen namen of nummers
        // bevatten.
        vervang_velden(&mut document, &klaar.persoon, true);
        Some(document)
    }

    /// De klant van `taak`, als die is ingevuld.
    pub(crate) fn klant(&self, taak: TaakId) -> Option<String> {
        slot(&self.lijst)
            .items
            .iter()
            .find(|i| i.taak == taak)
            .map(|i| i.klant.clone())
            .filter(|k| !k.trim().is_empty())
    }

    /// Komt dit bestand uit de bewaakte map? Dan met de gegevens om het als gedaan te noteren.
    pub(crate) fn bewaakt(&self, taak: TaakId) -> Option<Bewaakt> {
        slot(&self.lijst)
            .items
            .iter()
            .find(|i| i.taak == taak)
            .and_then(|i| i.bewaakt.clone())
    }

    /// Bewaart klant, onderwerp en datum van een transcript. Een veld gelijk aan de standaard (de
    /// naam van de opname, de datum van het transcript) blijft "nooit aangeraakt". `false` als het
    /// transcript er niet meer is.
    pub(crate) fn zet_naamvelden(
        &self,
        taak: TaakId,
        klant: &str,
        onderwerp: &str,
        datum: &str,
    ) -> bool {
        let mut lijst = slot(&self.lijst);
        let Some(item) = lijst.item(taak) else {
            return false;
        };
        // Zonder transcript is er nog geen naam om te bewaren.
        let Some(klaar) = &item.transcript else {
            return false;
        };
        let standaard = standaard_datum(&bewaren::lokaal_moment(klaar.op_ms));
        item.datum = eigen_waarde(datum, &standaard);
        item.klant = schone_naamveld(klant);
        item.onderwerp = eigen_waarde(onderwerp, &standaard_onderwerp(item));
        true
    }

    /// Onthoudt een bestand dat Mondig opsloeg (of de fout bij automatisch opslaan) en
    /// stuurt de nieuwe stand.
    pub(crate) fn noteer(
        &self,
        app: &AppHandle,
        taak: TaakId,
        uitkomst: Result<Opgeslagen, String>,
    ) {
        if !slot(&self.lijst).items.iter().any(|i| i.taak == taak) {
            // Geen bestand in de lijst (een vergadering): alleen het opgeslagen bestand telt.
            if let Ok(opgeslagen) = uitkomst {
                self.noteer_los(taak, opgeslagen);
            }
            return;
        }
        wijzig(app, &self.lijst, |lijst| {
            if let Some(item) = lijst.item(taak) {
                match uitkomst {
                    Ok(opgeslagen) => {
                        // Hetzelfde pad opnieuw (Vervangen) telt een keer.
                        item.opgeslagen.retain(|o| o.pad != opgeslagen.pad);
                        item.opgeslagen.push(opgeslagen);
                        item.verouderd = false;
                        item.bewaar_fout = None;
                    }
                    Err(fout) => item.bewaar_fout = Some(fout),
                }
            }
        });
    }

    /// De bestanden die Mondig van dit transcript (of van het verslag van een vergadering)
    /// opsloeg.
    pub(crate) fn opgeslagen(&self, taak: TaakId) -> Vec<Opgeslagen> {
        if let Some(item) = slot(&self.lijst).items.iter().find(|i| i.taak == taak) {
            return item.opgeslagen.clone();
        }
        slot(&self.losse).get(&taak).cloned().unwrap_or_default()
    }

    /// Onthoudt een opgeslagen bestand van een taak buiten de lijst; hetzelfde pad telt een keer.
    fn noteer_los(&self, taak: TaakId, opgeslagen: Opgeslagen) {
        let mut losse = slot(&self.losse);
        let lijst = losse.entry(taak).or_default();
        lijst.retain(|o| o.pad != opgeslagen.pad);
        lijst.push(opgeslagen);
    }

    /// Vergeet wat er van een taak buiten de lijst is opgeslagen (het verslag is gesloten).
    pub(crate) fn vergeet_los(&self, taak: TaakId) {
        slot(&self.losse).remove(&taak);
    }

    /// Vergeet bestanden die weg zijn of die Mondig niet meer verwijdert (na een Verwijderen
    /// dat het transcript liet staan) en stuurt de nieuwe stand.
    pub(crate) fn vergeet(
        &self,
        app: &AppHandle,
        taak: TaakId,
        paden: &[PathBuf],
    ) -> BestandenStand {
        wijzig(app, &self.lijst, |lijst| {
            if let Some(item) = lijst.item(taak) {
                item.opgeslagen.retain(|o| !paden.contains(&o.pad));
            }
        })
    }

    /// Haalt een transcript uit de lijst (na Verwijderen) en stuurt de nieuwe stand.
    pub(crate) fn haal_weg(&self, app: &AppHandle, taak: TaakId) -> BestandenStand {
        wijzig(app, &self.lijst, |lijst| {
            lijst.items.retain(|i| i.taak != taak);
            lijst.fout = None;
        })
    }
}

/// Slepen boven het hoofdvenster. Bij loslaten gaan de bestanden in de wachtrij en opent het
/// scherm Bestanden, ook als de gebruiker op een ander scherm was. Een fout bij toevoegen, of
/// een drop zonder bestand, staat in de stand, die het scherm bij het openen ophaalt.
pub(crate) fn bij_slepen(app: &AppHandle, event: &DragDropEvent) {
    match event {
        DragDropEvent::Enter { paths, .. } if !paths.is_empty() => {
            let _ = app.emit("bestanden-slepen", true);
        }
        DragDropEvent::Leave => {
            let _ = app.emit("bestanden-slepen", false);
        }
        DragDropEvent::Drop { paths, .. } => {
            let _ = app.emit("bestanden-slepen", false);
            // Wat de Mac-haak over deze drop meldde; die meldt het eerder dan Tauri.
            let VolgendeDrop {
                typen,
                beloftes,
                van_buiten,
            } = slot(&VOLGENDE_DROP).take().unwrap_or_default();
            if paths.is_empty() {
                // Alleen typenamen, voor de tester: wat stuurt een app als Dictafoon mee?
                logboek::log(&LogGebeurtenis::SlepenZonderPaden {
                    beloftes: u64::try_from(beloftes).unwrap_or(u64::MAX),
                    typen: SleepbordType::lijst(&typen),
                });
            }
            match na_drop(paths, beloftes, van_buiten) {
                NaDrop::Toevoegen => voeg_toe_gesleept(app, paths.clone()),
                // De bestanden komen via de haak binnen (`bij_mac_melding`); of het is een
                // lege drop van binnen Mondig of zonder haak, die stil blijft.
                NaDrop::Overgenomen | NaDrop::Negeren => {}
                NaDrop::NietGepakt => niet_gepakt(app),
            }
        }
        _ => {}
    }
}

/// Wat de Mac-haak meldde over de drop die Tauri nog moet doorgeven. Op andere platforms (en
/// op de Mac zonder haak) blijft dit leeg.
static VOLGENDE_DROP: Mutex<Option<VolgendeDrop>> = Mutex::new(None);

/// Zie [`VOLGENDE_DROP`].
#[derive(Default)]
struct VolgendeDrop {
    /// De typenamen op het sleepbord.
    typen: Vec<String>,
    /// Hoeveel beloftes de haak ophaalt.
    beloftes: usize,
    /// Kwam de drop van een andere app?
    van_buiten: bool,
}

/// Wat er gebeurt als Tauri een drop doorgeeft.
#[derive(Debug, PartialEq, Eq)]
enum NaDrop {
    /// De paden gaan in de wachtrij.
    Toevoegen,
    /// De Mac-haak haalt beloftes op en geeft ook de paden door; die van Tauri zouden dubbel zijn.
    Overgenomen,
    /// Iets van een andere app, maar niets te pakken: de melding in gewone taal.
    NietGepakt,
    /// Leeg, en niet aantoonbaar van buiten (tekst die binnen Mondig versleept wordt, of geen
    /// haak): stil, zoals vroeger.
    Negeren,
}

/// `van_buiten` komt alleen van de Mac-haak; zonder haak is het `false`.
fn na_drop(paden: &[PathBuf], beloftes: usize, van_buiten: bool) -> NaDrop {
    if beloftes > 0 {
        NaDrop::Overgenomen
    } else if !paden.is_empty() {
        NaDrop::Toevoegen
    } else if van_buiten {
        NaDrop::NietGepakt
    } else {
        NaDrop::Negeren
    }
}

/// Zet gesleepte bestanden in de wachtrij en opent het scherm Bestanden. Niet op de thread
/// van het venster: de koppen lezen kan even duren.
fn voeg_toe_gesleept(app: &AppHandle, paden: Vec<PathBuf>) {
    let app = app.clone();
    std::thread::spawn(move || {
        voeg_toe(&app, paden);
        let _ = app.emit("navigeer", "/bestanden");
    });
}

/// Een drop die niets opleverde: de stand zegt het en het scherm Bestanden gaat open.
fn niet_gepakt(app: &AppHandle) {
    let staat = app.state::<BestandenStaat>();
    wijzig(app, &staat.lijst, |lijst| lijst.niet_gepakt = true);
    let _ = app.emit("navigeer", "/bestanden");
}

/// Een melding van de Mac-haak (ADR 0030).
#[cfg(target_os = "macos")]
fn bij_mac_melding(app: &AppHandle, melding: mondig_mac_slepen::Melding) {
    use mondig_mac_slepen::Melding;
    match melding {
        Melding::BelofteBoven => {
            let _ = app.emit("bestanden-slepen", true);
        }
        Melding::Losgelaten {
            typen,
            beloftes,
            van_buiten,
        } => {
            *slot(&VOLGENDE_DROP) = Some(VolgendeDrop {
                typen,
                beloftes,
                van_buiten,
            });
        }
        Melding::Bestand(pad) => voeg_toe_gesleept(app, vec![pad]),
        Melding::Mislukt => niet_gepakt(app),
    }
}

/// Plaatst op de Mac de haak voor bestandsbeloftes op de webview van het hoofdvenster
/// (ADR 0030). Lukt dat niet, dan staat de reden in het logboek en werkt slepen zoals
/// zonder haak: paden uit de Finder wel, beloftes niet.
#[cfg(target_os = "macos")]
#[expect(
    unsafe_code,
    reason = "de haak krijgt de WKWebView als ruwe pointer van Tauri"
)]
pub(crate) fn plaats_mac_haak(app: &AppHandle) {
    let niet = |reden: &'static str| logboek::log(&LogGebeurtenis::SleephaakNiet { reden });
    let Some(venster) = app.get_webview_window("main") else {
        niet("geen_venster");
        return;
    };
    // De tijdelijke map voor beloftes, in de cache van de app. Bij het starten leeg: een
    // opname blijft niet langer bewaard dan nodig (ADR 0008).
    let Ok(map) = app.path().app_cache_dir().map(|map| map.join("gesleept")) else {
        niet("geen_map");
        return;
    };
    let _ = std::fs::remove_dir_all(&map);
    let app = app.clone();
    let gelukt = venster.with_webview(move |webview| {
        let ontvanger = Box::new(move |melding| bij_mac_melding(&app, melding));
        // SAFETY: `inner()` is de WKWebView van het hoofdvenster, die bestaat zolang de app
        // draait (sluiten verbergt het venster). `with_webview` draait op de hoofdthread.
        if let Err(fout) = unsafe { mondig_mac_slepen::installeer(webview.inner(), map, ontvanger) }
        {
            logboek::log(&LogGebeurtenis::SleephaakNiet { reden: fout.code() });
        }
    });
    if gelukt.is_err() {
        niet("geen_webview");
    }
}

/// De waarschuwing boven het transcript: wat bij het lezen opviel en, als het herkennen van de
/// sprekers mislukte, ook dat. Allebei als ze er zijn.
fn waarschuwing_tekst(uitgeschreven: &Uitgeschreven) -> Option<String> {
    let delen: Vec<String> = uitgeschreven
        .waarschuwing
        .map(audio::Leeswaarschuwing::melding)
        .into_iter()
        .chain(uitgeschreven.sprekerfout.clone())
        .collect();
    (!delen.is_empty()).then(|| delen.join(" "))
}

/// De tekst van elke uiting zoals de gebruiker hem zag na verbeteren (V-1), zonder witruimte aan de
/// randen.
fn uiting_teksten(klaar: &Klaar) -> Vec<String> {
    klaar
        .uitgeschreven
        .uitingen()
        .into_iter()
        .enumerate()
        .map(|(index, (s, _))| klaar.bewerkt.tekst(index, s.tekst.trim()).to_string())
        .collect()
}

/// De markeringen en de tekst voor Kopiëren en exports van elke uiting (V-2). Er wordt gezocht over
/// de uitingen van één spreker die achter elkaar staan, zodat een nummer dat over de grens van
/// twee uitingen loopt, toch gevonden wordt.
fn persoon_per_uiting(klaar: &Klaar) -> Vec<(Vec<MarkeringInfo>, String)> {
    let teksten = uiting_teksten(klaar);
    let sprekers: Vec<Option<u8>> = klaar
        .uitgeschreven
        .uitingen()
        .into_iter()
        .map(|(_, spreker)| spreker)
        .collect();
    let mut uit: Vec<(Vec<MarkeringInfo>, String)> =
        vec![(Vec::new(), String::new()); teksten.len()];
    // Uitingen zonder tekst tellen niet mee; de rest wordt gegroepeerd per spreker.
    let aanwezig: Vec<usize> = (0..teksten.len())
        .filter(|i| !teksten[*i].is_empty())
        .collect();
    let mut begin = 0;
    while begin < aanwezig.len() {
        let mut eind = begin + 1;
        while eind < aanwezig.len() && sprekers[aanwezig[eind]] == sprekers[aanwezig[begin]] {
            eind += 1;
        }
        let indexen = &aanwezig[begin..eind];
        let groep: Vec<&str> = indexen.iter().map(|i| teksten[*i].as_str()).collect();
        let vondsten = klaar.persoon.vondsten_over(&groep);
        let uitvoer = klaar.persoon.uitvoer_over(&groep);
        for (nr, (v, tekst)) in vondsten.iter().zip(uitvoer).enumerate() {
            uit[indexen[nr]] = (markeringen_info(groep[nr], v), tekst);
        }
        begin = eind;
    }
    uit
}

/// De regels van het transcript op het scherm, met de spreker ("Spreker 1") als de sprekers
/// herkend zijn (4b-D) en de tekst zoals de gebruiker hem verbeterde (V-1). Regels zonder tekst
/// vallen weg; `index` blijft de plek in het hele transcript.
fn scherm_regels(klaar: &Klaar, labels: &Sprekerlabels) -> Vec<TranscriptRegel> {
    let teksten = uiting_teksten(klaar);
    let mut persoon = persoon_per_uiting(klaar).into_iter();
    klaar
        .uitgeschreven
        .uitingen()
        .into_iter()
        .enumerate()
        .zip(teksten)
        .filter_map(|((index, (s, spreker)), tekst)| {
            let (markeringen, uitvoer) = persoon.next().unwrap_or_default();
            (!tekst.is_empty()).then(|| TranscriptRegel {
                index,
                start_ms: s.start_ms,
                eind_ms: s.eind_ms,
                tekst,
                spreker: spreker.map(|n| labels.spreker(n)),
                markeringen,
                uitvoer,
            })
        })
        .collect()
}

/// De regels voor een export (tekst, Word, pdf, ondertitels), met dezelfde sprekers en
/// dezelfde verbeterde tekst als het scherm. Zonder sprekers is `spreker` overal `None`, dus de
/// exports zijn dan ongewijzigd.
fn export_regels(klaar: &Klaar, labels: &Sprekerlabels) -> Vec<Regel> {
    klaar
        .uitgeschreven
        .uitingen()
        .into_iter()
        .zip(persoon_per_uiting(klaar))
        .map(|((s, spreker), (_, uitvoer))| Regel {
            start_ms: s.start_ms,
            eind_ms: s.eind_ms,
            tekst: uitvoer,
            spreker: spreker.map(|n| labels.spreker(n)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn een_lege_drop_van_buiten_geeft_de_melding() {
        // Van een andere app, zonder pad en zonder belofte: melden.
        assert_eq!(na_drop(&[], 0, true), NaDrop::NietGepakt);
        // Leeg van binnen Mondig (versleepte tekst) of zonder haak (Windows, Linux): stil.
        assert_eq!(na_drop(&[], 0, false), NaDrop::Negeren);
        // Paden uit de Finder of de Verkenner: gewoon toevoegen.
        let pad = [PathBuf::from("opname.m4a")];
        assert_eq!(na_drop(&pad, 0, true), NaDrop::Toevoegen);
        assert_eq!(na_drop(&pad, 0, false), NaDrop::Toevoegen);
        // Beloftes: de haak geeft alles door, ook de paden die Tauri meestuurt.
        assert_eq!(na_drop(&[], 2, true), NaDrop::Overgenomen);
        assert_eq!(na_drop(&pad, 1, true), NaDrop::Overgenomen);
    }

    #[test]
    fn een_lege_drop_staat_in_de_stand() {
        let lijst = Lijst {
            niet_gepakt: true,
            ..Lijst::default()
        };
        assert!(lijst.stand().niet_gepakt);
        assert!(!Lijst::default().stand().niet_gepakt);
    }
    use mondig_core::aanbieder::Transcript;
    use mondig_core::engine::protocol::Segment;
    use mondig_core::filter::{self, Meting};
    use mondig_core::gesprek::sprekers::Toegewezen;

    fn uitgeschreven(sprekers: Option<Vec<Option<u8>>>) -> Uitgeschreven {
        let segmenten = vec![
            Segment::nieuw(0, 5_000, "Goedemiddag."),
            Segment::nieuw(5_000, 9_000, "  "),
            Segment::nieuw(9_000, 12_000, "Dank u wel."),
        ];
        let transcript = Transcript {
            segmenten: segmenten.clone(),
            audio_duur: Duration::from_secs(12),
        };
        let meting = Meting::van_samples(&vec![0.1; 16_000]);
        Uitgeschreven {
            gefilterd: filter::zonder_filter(transcript, &meting),
            waarschuwing: None,
            sprekers: sprekers.map(|nummers| {
                segmenten
                    .into_iter()
                    .zip(nummers)
                    .map(|(segment, spreker)| Toegewezen { segment, spreker })
                    .collect()
            }),
            sprekerfout: None,
        }
    }

    fn klaar(uitgeschreven: Uitgeschreven) -> Klaar {
        Klaar {
            uitgeschreven,
            op_ms: 0,
            model: "Parakeet Ultra".to_string(),
            bewerkt: Bewerkingen::default(),
            persoon: Markeringen::default(),
        }
    }

    #[test]
    fn leeswaarschuwing_en_sprekerfout_staan_allebei_boven_het_transcript() {
        use mondig_core::audio::Leeswaarschuwing;
        let mut uit = uitgeschreven(None);
        assert_eq!(waarschuwing_tekst(&uit), None);
        uit.sprekerfout = Some("Sprekers mislukt.".to_string());
        assert_eq!(
            waarschuwing_tekst(&uit).as_deref(),
            Some("Sprekers mislukt.")
        );
        uit.waarschuwing = Some(Leeswaarschuwing::Afgebroken { tot_ms: 26_000 });
        let tekst = waarschuwing_tekst(&uit).unwrap();
        assert!(
            tekst.contains("afgebroken") && tekst.ends_with("Sprekers mislukt."),
            "{tekst}"
        );
    }

    #[test]
    fn zonder_sprekers_blijven_scherm_en_export_zoals_ze_waren() {
        let uit = klaar(uitgeschreven(None));
        let scherm = scherm_regels(&uit, &Sprekerlabels::default());
        assert_eq!(scherm.len(), 2, "een regel zonder tekst valt weg");
        assert!(scherm.iter().all(|r| r.spreker.is_none()));
        let export = export_regels(&uit, &Sprekerlabels::default());
        assert_eq!(export.len(), 3);
        assert!(export.iter().all(|r| r.spreker.is_none()));
    }

    #[test]
    fn scherm_en_export_gebruiken_de_sprekerlabels_van_de_app() {
        let uit = klaar(uitgeschreven(Some(vec![Some(1), Some(1), Some(2)])));
        let en = Sprekerlabels {
            jij: "You".into(),
            anderen: "Others".into(),
            sprekers: (1..=8).map(|n| format!("Speaker {n}")).collect(),
        };
        let scherm: Vec<_> = scherm_regels(&uit, &en)
            .into_iter()
            .map(|r| r.spreker)
            .collect();
        assert_eq!(
            scherm,
            vec![Some("Speaker 1".to_string()), Some("Speaker 2".to_string())]
        );
        let export: Vec<_> = export_regels(&uit, &en)
            .into_iter()
            .map(|r| r.spreker)
            .collect();
        assert_eq!(
            export,
            vec![
                Some("Speaker 1".to_string()),
                Some("Speaker 1".to_string()),
                Some("Speaker 2".to_string())
            ]
        );
    }

    #[test]
    fn met_sprekers_krijgt_elke_regel_zijn_spreker_op_scherm_en_in_de_export() {
        let uit = klaar(uitgeschreven(Some(vec![Some(1), Some(1), Some(2)])));
        let scherm: Vec<_> = scherm_regels(&uit, &Sprekerlabels::default())
            .into_iter()
            .map(|r| (r.tekst, r.spreker))
            .collect();
        assert_eq!(
            scherm,
            vec![
                ("Goedemiddag.".to_string(), Some("Spreker 1".to_string())),
                ("Dank u wel.".to_string(), Some("Spreker 2".to_string())),
            ]
        );
        let export: Vec<_> = export_regels(&uit, &Sprekerlabels::default())
            .into_iter()
            .map(|r| r.spreker)
            .collect();
        assert_eq!(
            export,
            vec![
                Some("Spreker 1".to_string()),
                Some("Spreker 1".to_string()),
                Some("Spreker 2".to_string())
            ]
        );
    }

    #[test]
    fn de_exports_zetten_de_spreker_voor_de_tekst() {
        use mondig_core::bewaren::{Document, Formaat, maak};
        let uit = klaar(uitgeschreven(Some(vec![Some(1), Some(1), Some(2)])));
        let document = Document {
            titel: "Interview".to_string(),
            bron: "interview.m4a".to_string(),
            model: "Parakeet Ultra".to_string(),
            klant: None,
            onderwerp: None,
            datum: None,
            standaard_onderwerp: "Interview".to_string(),
            moment: bewaren::lokaal_moment(0),
            duur_ms: 12_000,
            melding: None,
            regels: export_regels(&uit, &Sprekerlabels::default()),
        };
        let tekst = |formaat| String::from_utf8(maak(&document, formaat, true).unwrap()).unwrap();
        // Tekst, ondertitels (SRT met het label, VTT met een stem) en de rest.
        assert!(tekst(Formaat::Txt).contains("Spreker 1: Goedemiddag."));
        assert!(tekst(Formaat::Txt).contains("Spreker 2: Dank u wel."));
        assert!(tekst(Formaat::Srt).contains("Spreker 2: Dank u wel."));
        assert!(tekst(Formaat::Vtt).contains("<v Spreker 2>Dank u wel."));
        for formaat in [Formaat::Docx, Formaat::Odt, Formaat::Pdf] {
            assert!(!maak(&document, formaat, true).unwrap().is_empty());
        }
    }

    /// Een transcript waarin de tweede regel (zonder tekst) niet op het scherm staat, met de
    /// bewerking op de derde.
    fn bewerkt_transcript() -> Klaar {
        let mut klaar = klaar(uitgeschreven(Some(vec![Some(1), Some(1), Some(2)])));
        let origineel = klaar.uitgeschreven.uitingen()[2].0.tekst.trim().to_string();
        assert!(
            klaar
                .bewerkt
                .zet(2, &origineel, "  Dank u wel, voorzitter.\r\nTot zo.  ")
        );
        klaar
    }

    #[test]
    fn een_bewerkte_regel_staat_op_het_scherm_met_zijn_plek_in_het_hele_transcript() {
        let klaar = bewerkt_transcript();
        let scherm = scherm_regels(&klaar, &Sprekerlabels::default());
        // De lege regel (plek 1) staat niet op het scherm, maar plek 2 blijft plek 2.
        assert_eq!(
            scherm
                .iter()
                .map(|r| (r.index, r.tekst.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, "Goedemiddag."), (2, "Dank u wel, voorzitter.\nTot zo."),]
        );
        assert_eq!(scherm[1].spreker.as_deref(), Some("Spreker 2"));
        assert_eq!((scherm[1].start_ms, scherm[1].eind_ms), (9_000, 12_000));
    }

    #[test]
    fn een_bewerkte_regel_gaat_mee_in_elke_export() {
        use mondig_core::bewaren::{Document, Formaat, maak};
        let klaar = bewerkt_transcript();
        let document = document_met(&klaar);
        let tekst = |formaat| String::from_utf8(maak(&document, formaat, true).unwrap()).unwrap();
        // Tekst en ondertitels: nieuwe tekst erin, de oude eruit. Een regeleinde uit de
        // bewerking wordt in de exports een spatie, zodat een ondertitel heel blijft.
        for formaat in [
            Formaat::Txt,
            Formaat::Md,
            Formaat::Html,
            Formaat::Srt,
            Formaat::Vtt,
        ] {
            let uit = tekst(formaat);
            assert!(uit.contains("Dank u wel, voorzitter. Tot zo."), "{uit}");
            assert!(!uit.contains("Dank u wel."), "{uit}");
        }
        // Word, OpenDocument en pdf zijn binair en gecomprimeerd: de tekst is er niet in te
        // zoeken, maar zonder de bewerking zijn ze anders.
        let zonder = Document {
            regels: export_regels(&klaar_zonder_bewerking(), &Sprekerlabels::default()),
            ..document
        };
        for formaat in [Formaat::Docx, Formaat::Odt, Formaat::Pdf] {
            let met = maak(&document_met(&klaar), formaat, true).unwrap();
            assert!(!met.is_empty());
            assert_ne!(met, maak(&zonder, formaat, true).unwrap());
        }
    }

    fn klaar_zonder_bewerking() -> Klaar {
        klaar(uitgeschreven(Some(vec![Some(1), Some(1), Some(2)])))
    }

    fn document_met(klaar: &Klaar) -> mondig_core::bewaren::Document {
        mondig_core::bewaren::Document {
            titel: "Interview".to_string(),
            bron: "interview.m4a".to_string(),
            model: "Parakeet Ultra".to_string(),
            klant: None,
            onderwerp: None,
            datum: None,
            standaard_onderwerp: "Interview".to_string(),
            moment: bewaren::lokaal_moment(0),
            duur_ms: 12_000,
            melding: None,
            regels: export_regels(klaar, &Sprekerlabels::default()),
        }
    }

    #[test]
    fn leeg_maken_houdt_de_oude_tekst_en_terugzetten_geeft_het_origineel() {
        let mut klaar = bewerkt_transcript();
        let origineel = klaar.uitgeschreven.uitingen()[2].0.tekst.trim().to_string();
        assert!(!klaar.bewerkt.zet(2, &origineel, "   \n"));
        assert!(
            scherm_regels(&klaar, &Sprekerlabels::default())[1]
                .tekst
                .starts_with("Dank u wel, voorzitter.")
        );
        assert!(klaar.bewerkt.zet(2, &origineel, &origineel));
        assert_eq!(
            scherm_regels(&klaar, &Sprekerlabels::default())[1].tekst,
            "Dank u wel."
        );
    }

    #[test]
    fn persoonsgegevens_worden_gemarkeerd_en_bij_vervangen_overal_ingevuld() {
        use mondig_core::bewaren::{Formaat, maak};
        let mut klaar = klaar(uitgeschreven(None));
        // Na een bewerking wordt die regel opnieuw doorzocht.
        assert!(
            klaar
                .bewerkt
                .zet(2, "Dank u wel.", "Bel 06 12345678 of mail jan@example.com.")
        );
        let scherm = scherm_regels(&klaar, &Sprekerlabels::default());
        let regel = &scherm[1];
        assert_eq!(regel.markeringen.len(), 2);
        assert_eq!(
            (regel.markeringen[0].van, regel.markeringen[0].tot),
            (4, 15)
        );
        // Uit: het scherm en de export tonen de tekst zelf.
        assert_eq!(regel.uitvoer, regel.tekst);
        let tekst = |klaar: &Klaar, formaat| {
            let document = document_met(klaar);
            String::from_utf8(maak(&document, formaat, true).unwrap()).unwrap()
        };
        assert!(tekst(&klaar, Formaat::Txt).contains("06 12345678"));
        // Aan: Kopiëren en elke export krijgen de plaatsvervangers, het scherm houdt de tekst.
        klaar.persoon.zet_vervangen(true);
        let scherm = scherm_regels(&klaar, &Sprekerlabels::default());
        assert_eq!(scherm[1].uitvoer, "Bel [TELEFOON] of mail [E-MAIL].");
        assert!(scherm[1].tekst.contains("06 12345678"));
        for formaat in [
            Formaat::Txt,
            Formaat::Md,
            Formaat::Html,
            Formaat::Srt,
            Formaat::Vtt,
        ] {
            let uit = tekst(&klaar, formaat);
            assert!(uit.contains("Bel [TELEFOON] of mail [E-MAIL]."), "{uit}");
            assert!(
                !uit.contains("12345678") && !uit.contains("example.com"),
                "{uit}"
            );
        }
        // Een weggehaalde markering blijft staan in de tekst.
        klaar.persoon.haal_weg("jan@example.com");
        assert_eq!(
            scherm_regels(&klaar, &Sprekerlabels::default())[1].uitvoer,
            "Bel [TELEFOON] of mail jan@example.com."
        );
    }

    #[test]
    fn een_nummer_over_twee_uitingen_wordt_gevonden_en_in_de_export_vervangen() {
        let mut klaar = klaar(uitgeschreven(None));
        let o0 = klaar.uitgeschreven.uitingen()[0].0.tekst.trim().to_string();
        let o2 = klaar.uitgeschreven.uitingen()[2].0.tekst.trim().to_string();
        klaar.bewerkt.zet(0, &o0, "Mijn nummer is 06 1234");
        klaar.bewerkt.zet(2, &o2, "5678 en dat was het.");
        let scherm = scherm_regels(&klaar, &Sprekerlabels::default());
        assert_eq!(scherm[0].markeringen.len(), 1);
        assert_eq!(scherm[1].markeringen.len(), 1);
        klaar.persoon.zet_vervangen(true);
        let regels = export_regels(&klaar, &Sprekerlabels::default());
        assert_eq!(regels[0].tekst, "Mijn nummer is [TELEFOON]");
        assert!(!regels[2].tekst.contains("5678"), "{}", regels[2].tekst);
    }

    #[test]
    fn naam_titel_klant_en_onderwerp_gaan_door_de_vervanging_maar_een_datum_in_een_naam_niet() {
        let mut klaar = klaar(uitgeschreven(None));
        klaar.persoon.markeer("Jan de Vries").unwrap();
        let mut document = document_met(&klaar);
        document.titel = "2026-11-03 Interview Jan de Vries".to_string();
        document.bron = "2026-11-03 Interview Jan de Vries.m4a".to_string();
        document.klant = Some("Zaak Jan de Vries".to_string());
        document.onderwerp = Some("Intake Jan de Vries".to_string());
        document.datum = Some("Jan de Vries 3 november".to_string());
        // Uit: ongewijzigd.
        crate::persoon::vervang_velden(&mut document, &klaar.persoon, true);
        assert_eq!(document.titel, "2026-11-03 Interview Jan de Vries");
        klaar.persoon.zet_vervangen(true);
        crate::persoon::vervang_velden(&mut document, &klaar.persoon, true);
        assert_eq!(document.titel, "2026-11-03 Interview [GEGEVEN]");
        assert_eq!(document.bron, "2026-11-03 Interview [GEGEVEN].m4a");
        assert_eq!(document.klant.as_deref(), Some("Zaak [GEGEVEN]"));
        assert_eq!(document.onderwerp.as_deref(), Some("Intake [GEGEVEN]"));
        assert_eq!(document.datum.as_deref(), Some("[GEGEVEN] 3 november"));
        // De bestandsnaam zelf bevat dan ook geen naam meer.
        let naam = crate::bewaren::standaardnaam(&document, mondig_core::bewaren::Formaat::Docx);
        assert!(!naam.contains("Jan"), "{naam}");
        assert!(naam.contains("Zaak [GEGEVEN] Intake [GEGEVEN]"), "{naam}");
    }

    #[test]
    fn korte_modelnaam_zonder_uitleg() {
        assert_eq!(
            korte_naam("Parakeet Ultra (standaard, snel, 25 Europese talen)"),
            "Parakeet Ultra"
        );
        assert_eq!(korte_naam("Canary"), "Canary");
    }

    #[test]
    fn een_opgeslagen_verslag_is_terug_te_vinden_zonder_bestand_in_de_lijst() {
        let map = std::env::temp_dir().join(format!("mondig-losse-{}", std::process::id()));
        std::fs::create_dir_all(&map).unwrap();
        let pad = map.join("Vergadering.docx");
        std::fs::write(&pad, b"inhoud").unwrap();
        let staat = BestandenStaat::default();
        assert!(staat.opgeslagen(TaakId(42)).is_empty());
        staat.noteer_los(TaakId(42), Opgeslagen::van(pad.clone()).unwrap());
        // Hetzelfde pad nog eens (Vervangen) telt een keer.
        staat.noteer_los(TaakId(42), Opgeslagen::van(pad.clone()).unwrap());
        let lijst = staat.opgeslagen(TaakId(42));
        assert_eq!(lijst.len(), 1);
        assert_eq!(lijst[0].pad, pad);
        assert!(staat.opgeslagen(TaakId(43)).is_empty());
        staat.vergeet_los(TaakId(42));
        assert!(staat.opgeslagen(TaakId(42)).is_empty());
        std::fs::remove_dir_all(&map).unwrap();
    }

    #[test]
    fn de_stand_telt_door_en_draagt_de_fout() {
        let mut lijst = Lijst::default();
        assert_eq!(lijst.stand().versie, 0);
        lijst.fout = Some(Foutmelding::nieuw("geen model", Vervolg::OpnieuwDownloaden));
        lijst.versie += 1;
        let stand = lijst.stand();
        assert_eq!(stand.versie, 1);
        assert_eq!(
            stand.fout.map(|f| f.vervolg),
            Some(Vervolg::OpnieuwDownloaden)
        );
    }
}
