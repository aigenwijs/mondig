//! Bewaren en exporteren (sprint 4-C, ADR 0017).
//!
//! - **Instellingen, groep Bestanden:** de map (standaard `Documenten\Mondig`, te wijzigen via
//!   de mapkiezer), de bestandsnaam uit bouwstenen met een voorbeeld, het standaardformaat
//!   (Word), tijden in het document (aan) en automatisch opslaan (uit). Alles in de store.
//! - **Opslaan:** het transcript als bestand in de map uit de instelling, of in een map uit de
//!   mapkiezer ("Opslaan op een andere plek…"), met de naam uit de bouwstenen.
//! - **Nooit stil overschrijven:** bestaat de naam al, dan onthoudt Rust de vraag en kiest de
//!   gebruiker "Opslaan als (2)", Vervangen of Annuleren.
//! - **Openen, Map tonen en Verwijderen** werken alleen op bestanden die Mondig zelf opsloeg,
//!   en Verwijderen alleen als het bestand sindsdien niet is gewijzigd.
//!
//! De webview krijgt geen schrijfrechten en geeft nooit een pad: de map komt uit de instelling
//! of de mapkiezer (hier in Rust geopend), de naam uit de bouwstenen.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de argumenten per waarde mee; een verwijzing kan niet"
)]

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::SystemTime;

use mondig_core::bewaren::naam::{
    self, Bouwsteen, NaamGegevens, STANDAARD_BOUWSTENEN, lees_bouwstenen, schrijf_bouwstenen,
};
use mondig_core::bewaren::{self, BewaarFout, Document, Formaat, Notulenblok};
use mondig_core::model::Gebruik;
use mondig_core::probleem::Probleem;
use mondig_core::taak::{Foutmelding, Soort, TaakId, Vervolg};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::bestanden::{BestandenStaat, BestandenStand, korte_naam};
use crate::bewaakte_map::{self, Bewaakt};
use crate::dicteerketen::{model_voor, slot};
use crate::sneltoets::{lees_instelling, schrijf_instelling};

const SLEUTEL_MAP: &str = "bewaar_map";
const SLEUTEL_NAAM: &str = "bewaar_naam";
const SLEUTEL_FORMAAT: &str = "bewaar_formaat";
const SLEUTEL_TIJDEN: &str = "bewaar_tijden";
const SLEUTEL_AUTOMATISCH: &str = "bewaar_automatisch";
/// De opname ook als WAV naast een vergaderverslag (aanvulling op ADR 0008). Standaard uit.
const SLEUTEL_AUDIO: &str = "bewaar_audio";
const AAN: &str = "aan";
const UIT: &str = "uit";

// ---------------------------------------------------------------------------
// Instellingen
// ---------------------------------------------------------------------------

/// Een map zoals de interface hem toont.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MapInfo {
    /// Het volledige pad.
    pad: String,
    /// Ligt de map in Documenten, dan het deel daarna ("Mondig"); de interface toont dan
    /// "Documenten\Mondig".
    in_documenten: Option<String>,
}

impl MapInfo {
    pub(crate) fn van(map: &Path) -> Self {
        let in_documenten = dirs_documenten().and_then(|documenten| {
            map.strip_prefix(&documenten)
                .ok()
                .map(|rest| rest.display().to_string())
        });
        Self {
            pad: map.display().to_string(),
            in_documenten,
        }
    }
}

/// De map Documenten (de ouder van de standaardmap).
fn dirs_documenten() -> Option<PathBuf> {
    bewaren::standaard_map().and_then(|m| m.parent().map(Path::to_path_buf))
}

/// De keuzes in de groep Bestanden, met een voorbeeld van de naam.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BewaarInstellingen {
    map: Option<MapInfo>,
    bouwstenen: Vec<Bouwsteen>,
    formaat: Formaat,
    tijden: bool,
    automatisch: bool,
    /// De opname ook als WAV naast een vergaderverslag.
    audio: bool,
    voorbeeld: String,
}

/// Wat er bewaard is, met de standaard waar niets bewaard is.
struct Keuzes {
    map: Option<PathBuf>,
    bouwstenen: Vec<Bouwsteen>,
    formaat: Formaat,
    tijden: bool,
    automatisch: bool,
    audio: bool,
}

/// Een schakelaar zoals de store hem bewaart: "aan" of "uit".
fn aan_uit(aan: bool) -> &'static str {
    if aan { AAN } else { UIT }
}

/// Een schakelaar die standaard uit staat: alleen "aan" is aan.
fn standaard_uit(waarde: Option<&str>) -> bool {
    waarde == Some(AAN)
}

fn lees_keuzes(app: &AppHandle) -> Keuzes {
    Keuzes {
        map: lees_instelling(app, SLEUTEL_MAP)
            .map(PathBuf::from)
            .or_else(bewaren::standaard_map),
        bouwstenen: lees_instelling(app, SLEUTEL_NAAM)
            .map_or_else(|| STANDAARD_BOUWSTENEN.to_vec(), |t| lees_bouwstenen(&t)),
        formaat: lees_instelling(app, SLEUTEL_FORMAAT)
            .and_then(|s| Formaat::uit_sleutel(&s))
            .unwrap_or_default(),
        tijden: lees_instelling(app, SLEUTEL_TIJDEN).as_deref() != Some(UIT),
        automatisch: standaard_uit(lees_instelling(app, SLEUTEL_AUTOMATISCH).as_deref()),
        audio: standaard_uit(lees_instelling(app, SLEUTEL_AUDIO).as_deref()),
    }
}

/// Staat "Opname ook bewaren als geluidsbestand" aan?
pub(crate) fn audio_naast_aan(app: &AppHandle) -> bool {
    standaard_uit(lees_instelling(app, SLEUTEL_AUDIO).as_deref())
}

/// De map voor transcripten uit de instelling, zonder hem aan te maken. Voor de bewaakte map,
/// die er niet in mag kijken of schrijven zonder dat de gebruiker erom vroeg.
pub(crate) fn transcriptmap(app: &AppHandle) -> Option<PathBuf> {
    lees_keuzes(app).map
}

/// Het voorbeeld onder de bouwstenen: de gemeente en het onderwerp uit het ontwerp, op het
/// moment van nu.
fn voorbeeld(app: &AppHandle, keuzes: &Keuzes) -> String {
    let nu = bewaren::lokaal_moment(
        u64::try_from(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_default(),
        )
        .unwrap_or_default(),
    );
    let model = model_voor(app, Gebruik::Bestanden)
        .map(|(m, _)| korte_naam(&m.naam))
        .unwrap_or_default();
    naam::bestandsnaam(
        &keuzes.bouwstenen,
        &NaamGegevens {
            moment: &nu,
            datum: None,
            klant: "Gemeente Vlaardingen",
            onderwerp: "Intake",
            terugval: "Intake",
            model: &model,
        },
        keuzes.formaat,
    )
}

fn instellingen(app: &AppHandle) -> BewaarInstellingen {
    let keuzes = lees_keuzes(app);
    BewaarInstellingen {
        map: keuzes.map.as_deref().map(MapInfo::van),
        voorbeeld: voorbeeld(app, &keuzes),
        bouwstenen: keuzes.bouwstenen,
        formaat: keuzes.formaat,
        tijden: keuzes.tijden,
        automatisch: keuzes.automatisch,
        audio: keuzes.audio,
    }
}

/// De keuzes van de groep Bestanden.
#[tauri::command]
pub(crate) fn bewaren_instellingen(app: AppHandle) -> BewaarInstellingen {
    instellingen(&app)
}

/// Opent de mapkiezer, in Rust, en bewaart de gekozen map. Sluit de gebruiker de kiezer, dan
/// blijft de map zoals hij was.
pub(crate) fn kies_map(app: &AppHandle, begin: Option<&Path>) -> Option<PathBuf> {
    let mut kiezer = app.dialog().file();
    if let Some(venster) = app.get_webview_window("main") {
        kiezer = kiezer.set_parent(&venster);
    }
    if let Some(begin) = begin.filter(|b| b.is_dir()) {
        kiezer = kiezer.set_directory(begin);
    }
    kiezer.blocking_pick_folder()?.into_path().ok()
}

/// Kiest een andere standaardmap.
#[tauri::command]
pub(crate) async fn bewaren_kies_map(app: AppHandle) -> Result<BewaarInstellingen, Foutmelding> {
    tauri::async_runtime::spawn_blocking(move || {
        let huidig = lees_keuzes(&app).map;
        if let Some(map) = kies_map(&app, huidig.as_deref()) {
            schrijf_instelling(&app, SLEUTEL_MAP, &map.to_string_lossy());
        }
        instellingen(&app)
    })
    .await
    .map_err(|_| Foutmelding::nieuw("Kiezen is onverwacht gestopt.", Vervolg::Geen))
}

/// Bewaart de bouwstenen van de naam, in deze volgorde.
#[tauri::command]
pub(crate) fn bewaren_zet_bouwstenen(
    app: AppHandle,
    bouwstenen: Vec<Bouwsteen>,
) -> BewaarInstellingen {
    let tekst = schrijf_bouwstenen(&bouwstenen);
    // Via lezen, zodat dubbele er nooit in komen.
    schrijf_instelling(
        &app,
        SLEUTEL_NAAM,
        &schrijf_bouwstenen(&lees_bouwstenen(&tekst)),
    );
    instellingen(&app)
}

/// Bewaart het standaardformaat.
#[tauri::command]
pub(crate) fn bewaren_zet_formaat(app: AppHandle, formaat: Formaat) -> BewaarInstellingen {
    schrijf_instelling(&app, SLEUTEL_FORMAAT, formaat.extensie());
    instellingen(&app)
}

/// Zet de tijden in het document aan of uit.
#[tauri::command]
pub(crate) fn bewaren_zet_tijden(app: AppHandle, aan: bool) -> BewaarInstellingen {
    schrijf_instelling(&app, SLEUTEL_TIJDEN, aan_uit(aan));
    instellingen(&app)
}

/// Zet automatisch opslaan na het uitschrijven aan of uit.
#[tauri::command]
pub(crate) fn bewaren_zet_automatisch(app: AppHandle, aan: bool) -> BewaarInstellingen {
    schrijf_instelling(&app, SLEUTEL_AUTOMATISCH, aan_uit(aan));
    instellingen(&app)
}

/// Zet "Opname ook bewaren als geluidsbestand" aan of uit.
#[tauri::command]
pub(crate) fn bewaren_zet_audio(app: AppHandle, aan: bool) -> BewaarInstellingen {
    schrijf_instelling(&app, SLEUTEL_AUDIO, aan_uit(aan));
    instellingen(&app)
}

// ---------------------------------------------------------------------------
// Opslaan
// ---------------------------------------------------------------------------

/// Zoveel volgnummers probeert Mondig hooguit voordat het een fout geeft.
const MAX_POGINGEN: u32 = 100;

const IN_GEBRUIK: &str =
    "Het bestand is open in een ander programma. Sluit het en probeer het opnieuw.";

/// Een bestand dat Mondig opsloeg: waar, en hoe het er toen uitzag. Alleen dan mag Mondig het
/// later verwijderen.
#[derive(Clone)]
pub(crate) struct Opgeslagen {
    pub(crate) pad: PathBuf,
    grootte: u64,
    gewijzigd: Option<SystemTime>,
}

impl Opgeslagen {
    pub(crate) fn van(pad: PathBuf) -> io::Result<Self> {
        let gegevens = fs::metadata(&pad)?;
        Ok(Self {
            grootte: gegevens.len(),
            gewijzigd: gegevens.modified().ok(),
            pad,
        })
    }

    /// De bestandsnaam zonder map.
    pub(crate) fn naam(&self) -> String {
        self.pad
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Staat het bestand er nog precies zoals Mondig het schreef?
    fn ongewijzigd(&self) -> bool {
        fs::metadata(&self.pad).is_ok_and(|g| {
            g.is_file() && g.len() == self.grootte && g.modified().ok() == self.gewijzigd
        })
    }
}

/// Een vraag die op het antwoord van de gebruiker wacht: de naam bestaat al. De bytes zijn al
/// gemaakt, zodat het document niet nog eens gegenereerd hoeft te worden.
struct Vraag {
    id: u64,
    taak: TaakId,
    map: PathBuf,
    naam: String,
    inhoud: Vec<u8>,
}

/// Een transcript dat automatisch moet worden opgeslagen, en of het uit de bewaakte map komt.
type AutomatischWerk = (TaakId, Option<Bewaakt>);

/// Wat Rust over opslaan onthoudt.
#[derive(Default)]
pub(crate) struct BewaarStaat {
    vraag: Mutex<Option<Vraag>>,
    volgende: AtomicU64,
    /// De werkrij voor automatisch opslaan: een eigen thread, zodat het maken van een PDF de
    /// uitslagen van de wachtrij niet ophoudt. Gestart bij het eerste gebruik.
    werkrij: Mutex<Option<mpsc::Sender<AutomatischWerk>>>,
}

/// Hoe opslaan afliep.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum BewaarUitslag {
    /// Opgeslagen, met de naam en de map. Bij een vergaderverslag met de instelling
    /// "Opname ook bewaren als geluidsbestand" ook de naam van de WAV, of waarom die niet
    /// lukte (het tekstbestand staat er dan wel).
    Opgeslagen {
        naam: String,
        map: MapInfo,
        #[serde(skip_serializing_if = "Option::is_none")]
        audio: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        audio_fout: Option<String>,
    },
    /// De naam bestaat al: de gebruiker kiest. `voorstel` is de naam met volgnummer.
    BestaatAl {
        vraag: u64,
        naam: String,
        voorstel: String,
    },
    /// De gebruiker sloot de mapkiezer of koos Annuleren.
    Geannuleerd,
}

/// Wat de gebruiker kiest als de naam al bestaat.
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Keuze {
    /// Opslaan met het eerste vrije volgnummer.
    Nummer,
    /// Het bestaande bestand vervangen.
    Vervangen,
    /// Niets doen.
    Annuleren,
}

/// Is dit de fout "het bestand is in gebruik" (Windows-fout 32 of 33)?
fn in_gebruik(fout: &io::Error) -> bool {
    cfg!(windows) && matches!(fout.raw_os_error(), Some(32 | 33))
}

/// Een foutmelding in gewone taal bij een schrijffout. Zegt nooit iets over de tekst. `doel` is
/// het bestand dat vervangen werd: bestaat dat en weigert Windows (fout 5, 32 of 33), dan houdt
/// een ander programma het open, want het tijdelijke bestand in dezelfde map lukte wel.
pub(crate) fn schrijffout(fout: &io::Error, map: &Path, doel: Option<&Path>) -> Foutmelding {
    crate::logboek::mislukt(Soort::Bestand, Probleem::uit_io(fout.kind()));
    let geweigerd = cfg!(windows) && matches!(fout.raw_os_error(), Some(5 | 32 | 33));
    if let Some(doel) = doel.filter(|d| geweigerd && d.is_file()) {
        if fs::metadata(doel).is_ok_and(|g| g.permissions().readonly()) {
            return Foutmelding::nieuw(
                "Het bestaande bestand is alleen-lezen. Kies \"Opslaan als (2)\" of een andere map.",
                Vervolg::Geen,
            );
        }
        return Foutmelding::nieuw(IN_GEBRUIK, Vervolg::OpnieuwProberen);
    }
    if in_gebruik(fout) {
        return Foutmelding::nieuw(IN_GEBRUIK, Vervolg::OpnieuwProberen);
    }
    let melding = match fout.kind() {
        io::ErrorKind::NotFound => format!(
            "De map {} bestaat niet (meer). Kies een andere map in Instellingen.",
            map.display()
        ),
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
            "Mondig mag niet schrijven in deze map. Kies een andere map.".to_string()
        }
        io::ErrorKind::StorageFull => {
            "De schijf is vol. Maak ruimte vrij of kies een andere map.".to_string()
        }
        _ => "Opslaan is niet gelukt. Probeer het opnieuw of kies een andere map.".to_string(),
    };
    Foutmelding::nieuw(melding, Vervolg::OpnieuwProberen)
}

/// Schrijft een bestand atomair: eerst een tijdelijk bestand in dezelfde map (nieuw gemaakt,
/// met een willekeurige naam, dus geen bestaand bestand of snelkoppeling), naar schijf
/// geschreven, en dan hernoemd. Zonder `vervangen` mag het doel niet bestaan
/// (`AlreadyExists`); met `vervangen` vervangt de hernoeming het in één stap. Bij een fout
/// ruimt `tempfile` het tijdelijke bestand op.
fn schrijf_atomair(pad: &Path, bytes: &[u8], vervangen: bool) -> io::Result<()> {
    let map = pad
        .parent()
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
    let mut tijdelijk = tempfile::Builder::new()
        .prefix(".mondig-")
        .suffix(".tijdelijk")
        .tempfile_in(map)?;
    tijdelijk.write_all(bytes)?;
    tijdelijk.as_file().sync_all()?;
    let uitslag = if vervangen {
        tijdelijk.persist(pad)
    } else {
        tijdelijk.persist_noclobber(pad)
    };
    // De fout bevat het tijdelijke bestand; dat valt hier weg en wordt dan verwijderd.
    uitslag.map(|_| ()).map_err(|fout| fout.error)
}

/// De naam van een transcript volgens de bouwstenen.
fn naam_voor(document: &Document, keuzes: &Keuzes, formaat: Formaat) -> String {
    naam::bestandsnaam(&keuzes.bouwstenen, &document.naamgegevens(), formaat)
}

/// De naam die `document` krijgt in het standaardformaat, met de bouwstenen uit de instellingen:
/// het voorbeeld onder de velden Datum, Klant en Onderwerp. Het echte bestand kan er nog een
/// volgnummer bij krijgen als de naam al bestaat.
pub(crate) fn voorbeeldnaam(app: &AppHandle, document: &Document) -> String {
    let keuzes = lees_keuzes(app);
    naam_voor(document, &keuzes, keuzes.formaat)
}

/// De bestandsnaam voor `document` met de standaardbouwstenen, zonder instellingen: voor tests in
/// andere modules die willen zien wat er in de naam van het bestand komt.
#[cfg(test)]
pub(crate) fn standaardnaam(document: &Document, formaat: Formaat) -> String {
    naam::bestandsnaam(&STANDAARD_BOUWSTENEN, &document.naamgegevens(), formaat)
}

/// Het eerste volgnummer boven alle bestaande "Naam (n).ext" in de map. De map wordt één keer
/// gelezen, ook op een netwerkmap.
pub(crate) fn eerste_nummer(map: &Path, naam: &str) -> u32 {
    let bestaande: Vec<String> = fs::read_dir(map)
        .map(|lijst| {
            lijst
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    naam::volgend_nummer(naam, bestaande.iter().map(String::as_str))
}

/// Het document van een transcript, of van het verslag van een vergadering (sprint 4b-C):
/// beide hebben een taak-id uit dezelfde reeks.
fn document_van(app: &AppHandle, taak: TaakId) -> Result<Document, Foutmelding> {
    app.state::<BestandenStaat>()
        .document(taak)
        .or_else(|| {
            app.try_state::<crate::vergadering::VergaderingStaat>()
                .and_then(|staat| staat.document(taak))
        })
        .ok_or_else(|| {
            Foutmelding::nieuw("Dit uitschrift of verslag is er niet meer.", Vervolg::Geen)
        })
}

/// De klant die de gebruiker bij dit transcript of verslag invulde, als die er is.
fn klant_van(app: &AppHandle, taak: TaakId) -> Option<String> {
    app.state::<BestandenStaat>().klant(taak).or_else(|| {
        app.try_state::<crate::vergadering::VergaderingStaat>()
            .and_then(|staat| staat.klant(taak))
    })
}

/// De notulen die vóór het verslag komen (ADR 0026 punt 8); alleen een vergaderverslag met
/// notulen heeft ze.
fn notulen_van(app: &AppHandle, taak: TaakId) -> Option<Notulenblok> {
    app.try_state::<crate::vergadering::VergaderingStaat>()
        .and_then(|staat| staat.notulenblok(Some(taak)))
}

fn maak(
    document: &Document,
    notulen: Option<&Notulenblok>,
    formaat: Formaat,
    tijden: bool,
) -> Result<Vec<u8>, Foutmelding> {
    bewaren::maak_met_notulen(document, notulen, formaat, tijden).map_err(|fout| {
        let vervolg = match fout {
            BewaarFout::GeenTekst => Vervolg::Geen,
            _ => Vervolg::OpnieuwProberen,
        };
        Foutmelding::nieuw(fout.to_string(), vervolg)
    })
}

/// Schrijft het bestand en onthoudt het bij het transcript. Is het een vergaderverslag en staat
/// "Opname ook bewaren als geluidsbestand" aan, dan komt de opname als WAV ernaast; mislukt
/// dat, dan staat het tekstbestand er toch en zegt de uitslag waarom.
fn schrijf(
    app: &AppHandle,
    taak: TaakId,
    map: &Path,
    naam: &str,
    bytes: &[u8],
    vervangen: bool,
) -> io::Result<BewaarUitslag> {
    let pad = map.join(naam);
    schrijf_atomair(&pad, bytes, vervangen)?;
    let opgeslagen = Opgeslagen::van(pad.clone())?;
    app.state::<BestandenStaat>()
        .noteer(app, taak, Ok(opgeslagen));
    // De klant komt in de suggesties, nu het opslaan is gelukt (nooit bij typen).
    crate::klanten::onthoud(klant_van(app, taak).as_deref());
    // Een verslag onthoudt waar het staat, ook na een herstart (zie `Verslag::opgeslagen`).
    crate::vergadering::noteer_opgeslagen(app, taak, &pad);
    let (audio, audio_fout) =
        match crate::vergadering::audio_naast_verslag(app, taak, &pad, vervangen) {
            None => (None, None),
            Some(Ok(wav)) => (Some(wav), None),
            Some(Err(fout)) => (None, Some(fout.melding)),
        };
    Ok(BewaarUitslag::Opgeslagen {
        naam: naam.to_string(),
        map: MapInfo::van(map),
        audio,
        audio_fout,
    })
}

/// De naam van een geluidsbestand met dezelfde basisnaam: "Vergadering 14.00.docx" wordt
/// "Vergadering 14.00.wav". Alleen de laatste extensie gaat eraf.
pub(crate) fn als_wav(naam: &str) -> String {
    match naam.rfind('.') {
        Some(punt) if punt > 0 => format!("{}.wav", &naam[..punt]),
        _ => format!("{naam}.wav"),
    }
}

/// De naam die het verslag van `taak` in het standaardformaat zou krijgen, als WAV: de
/// voorgestelde naam bij "Opname bewaren". `None` als er geen verslag is.
pub(crate) fn wav_naam_voor(app: &AppHandle, taak: TaakId) -> Option<String> {
    let keuzes = lees_keuzes(app);
    let document = document_van(app, taak).ok()?;
    Some(als_wav(&naam_voor(&document, &keuzes, keuzes.formaat)))
}

/// De opslagmap uit de instelling, om de bewaarkiezer van "Opname bewaren" in te laten
/// beginnen. De standaardmap wordt aangemaakt als hij er nog niet is.
pub(crate) fn ingestelde_map(app: &AppHandle) -> Option<PathBuf> {
    let map = lees_keuzes(app).map?;
    let _ = maak_standaardmap(&map);
    map.is_dir().then_some(map)
}

/// Schrijft met het eerste vrije volgnummer. Staat die naam er intussen toch, dan het volgende,
/// hooguit [`MAX_POGINGEN`] keer.
fn schrijf_met_nummer(
    app: &AppHandle,
    taak: TaakId,
    map: &Path,
    naam: &str,
    bytes: &[u8],
) -> Result<BewaarUitslag, Foutmelding> {
    let begin = eerste_nummer(map, naam);
    for nummer in begin..begin.saturating_add(MAX_POGINGEN) {
        let kandidaat = naam::met_volgnummer(naam, nummer);
        match schrijf(app, taak, map, &kandidaat, bytes, false) {
            Err(fout) if fout.kind() == io::ErrorKind::AlreadyExists => {}
            uitslag => return uitslag.map_err(|f| schrijffout(&f, map, None)),
        }
    }
    Err(Foutmelding::nieuw(
        "Mondig vond geen vrije naam voor dit bestand. Kies een andere map of pas de naam aan.",
        Vervolg::Geen,
    ))
}

/// Maakt de standaardmap als die er nog niet is; een gekozen map bestaat al.
fn maak_standaardmap(map: &Path) -> Result<(), Foutmelding> {
    if bewaren::standaard_map().as_deref() == Some(map) {
        fs::create_dir_all(map).map_err(|f| schrijffout(&f, map, None))?;
    }
    Ok(())
}

/// Slaat op in `map`; bestaat de naam al, dan een vraag in plaats van overschrijven. Het
/// document wordt één keer gemaakt.
fn bewaar_in(
    app: &AppHandle,
    taak: TaakId,
    formaat: Formaat,
    map: PathBuf,
    keuzes: &Keuzes,
) -> Result<BewaarUitslag, Foutmelding> {
    let document = document_van(app, taak)?;
    let naam = naam_voor(&document, keuzes, formaat);
    maak_standaardmap(&map)?;
    let notulen = notulen_van(app, taak);
    let inhoud = maak(&document, notulen.as_ref(), formaat, keuzes.tijden)?;
    match schrijf(app, taak, &map, &naam, &inhoud, false) {
        Ok(uitslag) => Ok(uitslag),
        Err(fout) if fout.kind() == io::ErrorKind::AlreadyExists => {
            let staat = app.state::<BewaarStaat>();
            let id = staat.volgende.fetch_add(1, Ordering::Relaxed) + 1;
            let voorstel = naam::met_volgnummer(&naam, eerste_nummer(&map, &naam));
            *slot(&staat.vraag) = Some(Vraag {
                id,
                taak,
                map,
                naam: naam.clone(),
                inhoud,
            });
            Ok(BewaarUitslag::BestaatAl {
                vraag: id,
                naam,
                voorstel,
            })
        }
        Err(fout) => Err(schrijffout(&fout, &map, None)),
    }
}

/// Slaat een transcript op. Zonder `formaat` het standaardformaat; met `andere_plek` opent
/// eerst de mapkiezer, anders gaat het naar de map uit de instelling.
#[tauri::command]
pub(crate) async fn bestand_bewaar(
    app: AppHandle,
    taak: TaakId,
    formaat: Option<Formaat>,
    andere_plek: bool,
) -> Result<BewaarUitslag, Foutmelding> {
    tauri::async_runtime::spawn_blocking(move || bewaar(&app, taak, formaat, andere_plek))
        .await
        .map_err(|_| {
            Foutmelding::nieuw("Opslaan is onverwacht gestopt.", Vervolg::OpnieuwProberen)
        })?
}

/// Slaat een transcript of het verslag van een vergadering op (zie [`bestand_bewaar`]).
/// Blokkeert: de mapkiezer en het schrijven wachten.
pub(crate) fn bewaar(
    app: &AppHandle,
    taak: TaakId,
    formaat: Option<Formaat>,
    andere_plek: bool,
) -> Result<BewaarUitslag, Foutmelding> {
    let keuzes = lees_keuzes(app);
    let formaat = formaat.unwrap_or(keuzes.formaat);
    let map = if andere_plek {
        match kies_map(app, keuzes.map.as_deref()) {
            Some(map) => map,
            None => return Ok(BewaarUitslag::Geannuleerd),
        }
    } else {
        keuzes.map.clone().ok_or_else(|| {
            Foutmelding::nieuw(
                "Er is nog geen map gekozen. Kies er een in Instellingen.",
                Vervolg::Geen,
            )
        })?
    };
    bewaar_in(app, taak, formaat, map, &keuzes)
}

/// Haalt de openstaande vraag op, maar alleen als het id klopt; een verkeerd id laat de vraag
/// staan.
fn neem_vraag(houder: &Mutex<Option<Vraag>>, id: u64) -> Option<Vraag> {
    let mut houder = slot(houder);
    if houder.as_ref().is_some_and(|v| v.id == id) {
        houder.take()
    } else {
        None
    }
}

/// Het antwoord op "Er staat al een bestand met deze naam".
#[tauri::command]
pub(crate) async fn bestand_bewaar_beslis(
    app: AppHandle,
    vraag: u64,
    keuze: Keuze,
) -> Result<BewaarUitslag, Foutmelding> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(open) = neem_vraag(&app.state::<BewaarStaat>().vraag, vraag) else {
            return Err(Foutmelding::nieuw(
                "Deze vraag is verlopen. Klik nog eens op Opslaan.",
                Vervolg::Geen,
            ));
        };
        match keuze {
            Keuze::Annuleren => Ok(BewaarUitslag::Geannuleerd),
            Keuze::Vervangen => {
                let doel = open.map.join(&open.naam);
                schrijf(&app, open.taak, &open.map, &open.naam, &open.inhoud, true)
                    .map_err(|f| schrijffout(&f, &open.map, Some(&doel)))
            }
            Keuze::Nummer => {
                schrijf_met_nummer(&app, open.taak, &open.map, &open.naam, &open.inhoud)
            }
        }
    })
    .await
    .map_err(|_| Foutmelding::nieuw("Opslaan is onverwacht gestopt.", Vervolg::OpnieuwProberen))?
}

/// Na het uitschrijven: zet het transcript in de werkrij voor automatisch opslaan, als de
/// gebruiker dat aanzette of het bestand uit de bewaakte map komt (dan altijd). Keert meteen
/// terug; het opslaan zelf loopt op de eigen thread van de werkrij.
///
/// `bewaakt` komt van de aanroeper, die het vastlegde toen het bestand klaar was: haalt de
/// gebruiker het bestand tussen "klaar" en het opslaan uit de lijst, dan weet de werkrij nog
/// steeds dat het uit de bewaakte map komt en noteert hij het als gedaan.
pub(crate) fn automatisch(app: &AppHandle, taak: TaakId, bewaakt: Option<Bewaakt>) {
    if bewaakt.is_none() && !lees_keuzes(app).automatisch {
        return;
    }
    let staat = app.state::<BewaarStaat>();
    let mut werkrij = slot(&staat.werkrij);
    let zender = werkrij.get_or_insert_with(|| {
        let (zender, ontvanger) = mpsc::channel::<AutomatischWerk>();
        let app = app.clone();
        std::thread::spawn(move || {
            for (taak, bewaakt) in ontvanger {
                automatisch_opslaan(&app, taak, bewaakt);
            }
        });
        zender
    });
    let _ = zender.send((taak, bewaakt));
}

/// Slaat automatisch op en onthoudt daarna, bij een bestand uit de bewaakte map, dat het
/// gedaan is. Dat gebeurt ook als er niets op te slaan was (een opname zonder spraak) of
/// opslaan mislukte: de fout staat dan bij het bestand in de lijst, en het bestand wordt niet
/// bij elke start opnieuw uitgeschreven.
fn automatisch_opslaan(app: &AppHandle, taak: TaakId, bewaakt: Option<Bewaakt>) {
    opslaan_na_uitschrijven(app, taak, bewaakt.as_ref());
    if let Some(bewaakt) = bewaakt {
        bewaakte_map::markeer_gedaan(app, &bewaakt);
    }
}

/// Slaat op in de standaardmap en het standaardformaat, of naast het bronbestand als de
/// bewaakte map dat vraagt. Bestaat de naam al, dan met volgnummer (in de achtergrond kan
/// Mondig niets vragen, en overschrijven doet het nooit stil). Een transcript zonder tekst
/// wordt niet opgeslagen.
fn opslaan_na_uitschrijven(app: &AppHandle, taak: TaakId, bewaakt: Option<&Bewaakt>) {
    let keuzes = lees_keuzes(app);
    let staat = app.state::<BestandenStaat>();
    let Some(document) = staat.document(taak) else {
        return;
    };
    if document.regels.iter().all(|r| r.tekst.trim().is_empty()) {
        return;
    }
    let naast = bewaakt
        .filter(|b| b.naast)
        .and_then(|b| b.pad.parent().map(Path::to_path_buf));
    let Some(map) = naast.or_else(|| keuzes.map.clone()) else {
        staat.noteer(app, taak, Err("Er is nog geen map gekozen.".to_string()));
        return;
    };
    let naam = naam_voor(&document, &keuzes, keuzes.formaat);
    let uitslag = maak_standaardmap(&map)
        .and_then(|()| maak(&document, None, keuzes.formaat, keuzes.tijden))
        .and_then(
            |inhoud| match schrijf(app, taak, &map, &naam, &inhoud, false) {
                Err(fout) if fout.kind() == io::ErrorKind::AlreadyExists => {
                    schrijf_met_nummer(app, taak, &map, &naam, &inhoud)
                }
                uitslag => uitslag.map_err(|f| schrijffout(&f, &map, None)),
            },
        );
    if let Err(fout) = uitslag {
        staat.noteer(app, taak, Err(fout.melding));
    }
}

// ---------------------------------------------------------------------------
// Bewaakte map: heeft een opname al een transcript?
// ---------------------------------------------------------------------------

/// De bestandsnaam zonder " (2)" voor de extensie: "2026-11-02 Opname (2).docx" wordt
/// "2026-11-02 Opname.docx".
fn zonder_volgnummer(naam: &str) -> String {
    let (stam, extensie) = naam.rfind('.').map_or((naam, ""), |p| naam.split_at(p));
    let kaal = stam.strip_suffix(')').and_then(|rest| {
        let begin = rest.rfind(" (")?;
        let cijfers = &rest[begin + 2..];
        (!cijfers.is_empty() && cijfers.chars().all(|c| c.is_ascii_digit())).then(|| &rest[..begin])
    });
    format!("{}{extensie}", kaal.unwrap_or(stam))
}

/// Is een van deze bestanden het transcript van de opname `bron_stam` (naam zonder extensie)?
/// Alleen als het bestand precies de naam heeft die de naamregel van 4-C zou geven (met of
/// zonder volgnummer), en niet ouder is dan de opname. Zo hoort "2026-10-01 VN0001.docx" bij
/// de dictafoonopname VN0001 van die dag, en niet bij een nieuwe opname die ook VN0001 heet.
/// Een klant in de naamregel kan niet worden nagegaan (de klant hoort bij een transcript, niet
/// bij een opname): dan telt alleen het register. De tijd van de naam mag een paar minuten
/// verschillen van de wijzigtijd, want het transcript was iets eerder klaar dan opgeslagen.
fn is_transcript_van(
    bron_stam: &str,
    bron_ms: u64,
    kandidaten: &[mondig_core::bewaak::Transcriptbestand],
    bouwstenen: &[Bouwsteen],
    model: &str,
) -> bool {
    kandidaten.iter().any(|kandidaat| {
        if kandidaat.gewijzigd_ms < bron_ms {
            return false;
        }
        let Some(formaat) = Path::new(&kandidaat.naam)
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|e| Formaat::uit_sleutel(&e.to_lowercase()))
        else {
            return false;
        };
        let gevonden = zonder_volgnummer(&kandidaat.naam);
        [0_u64, 60_000, 120_000].into_iter().any(|terug| {
            let moment = bewaren::lokaal_moment(kandidaat.gewijzigd_ms.saturating_sub(terug));
            naam::bestandsnaam(
                bouwstenen,
                &NaamGegevens {
                    moment: &moment,
                    datum: None,
                    klant: "",
                    onderwerp: bron_stam,
                    terugval: bron_stam,
                    model,
                },
                formaat,
            )
            .eq_ignore_ascii_case(&gevonden)
        })
    })
}

/// Heeft deze opname al een transcript in de map met transcripten of naast de opname? Zie
/// [`is_transcript_van`]. `bron_ms` is de wijzigtijd van de opname.
pub(crate) fn heeft_transcript(app: &AppHandle, bron: &Path, bron_ms: u64) -> bool {
    let Some(stam) = bron.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
        return false;
    };
    let keuzes = lees_keuzes(app);
    let model = model_voor(app, Gebruik::Bestanden)
        .map(|(m, _)| korte_naam(&m.naam))
        .unwrap_or_default();
    let kandidaten: Vec<_> = keuzes
        .map
        .iter()
        .map(PathBuf::as_path)
        .chain(bron.parent())
        .flat_map(mondig_core::bewaak::transcripten_in)
        .collect();
    is_transcript_van(&stam, bron_ms, &kandidaten, &keuzes.bouwstenen, &model)
}

/// Bewaart Klant, Onderwerp en Datum van een transcript (voor de naam en de gegevens van het
/// document). Een veld dat gelijk is aan de standaard (de naam van de opname, de datum van het
/// transcript) blijft "nooit aangeraakt"; een leeg veld valt weg uit de naam.
#[tauri::command]
pub(crate) fn bestand_zet_naamvelden(
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
    klant: String,
    onderwerp: String,
    datum: String,
) -> Result<(), Foutmelding> {
    if staat.zet_naamvelden(taak, &klant, &onderwerp, &datum) {
        Ok(())
    } else {
        Err(Foutmelding::nieuw(
            "Dit uitschrift is er niet meer.",
            Vervolg::Geen,
        ))
    }
}

/// De bestandsnaam die het transcript met deze velden krijgt, in het standaardformaat en met de
/// bouwstenen uit de instellingen. Rust rekent hem uit, zodat het voorbeeld in het scherm altijd
/// gelijk is aan het echte bestand. Er verandert niets aan het transcript.
#[tauri::command]
pub(crate) fn bestand_naamvoorbeeld(
    app: AppHandle,
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
    klant: String,
    onderwerp: String,
    datum: String,
) -> Result<String, Foutmelding> {
    let document = staat
        .naam_document(taak, &klant, &onderwerp, &datum)
        .ok_or_else(|| Foutmelding::nieuw("Dit uitschrift is er niet meer.", Vervolg::Geen))?;
    Ok(voorbeeldnaam(&app, &document))
}

// ---------------------------------------------------------------------------
// Openen, map tonen en verwijderen
// ---------------------------------------------------------------------------

/// Het laatst opgeslagen bestand van een transcript, als het er nog is.
fn laatste(staat: &BestandenStaat, taak: TaakId) -> Result<PathBuf, Foutmelding> {
    bestaand_opgeslagen(staat.opgeslagen(taak).pop().map(|o| o.pad))
}

/// Een opgeslagen bestand als het er nog staat; anders de melding die Bestanden en "Eerder
/// opgenomen" allebei geven.
pub(crate) fn bestaand_opgeslagen(pad: Option<PathBuf>) -> Result<PathBuf, Foutmelding> {
    pad.filter(|pad| pad.is_file()).ok_or_else(|| {
        Foutmelding::nieuw(
            "Het opgeslagen bestand staat niet meer op die plek.",
            Vervolg::Geen,
        )
    })
}

fn start(
    programma: &str,
    argumenten: impl FnOnce(&mut std::process::Command),
) -> Result<(), Foutmelding> {
    let mut opdracht = std::process::Command::new(programma);
    argumenten(&mut opdracht);
    opdracht.spawn().map(|_| ()).map_err(|_| {
        Foutmelding::nieuw(
            "Dit lukte niet. Open het bestand via de Verkenner.",
            Vervolg::Geen,
        )
    })
}

/// Opent het bestand dat Mondig net opsloeg met het programma dat erbij hoort.
#[tauri::command]
pub(crate) fn bestand_open_opgeslagen(
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
) -> Result<(), Foutmelding> {
    let pad = laatste(&staat, taak)?;
    #[cfg(windows)]
    return start("explorer", |o| {
        o.arg(&pad);
    });
    #[cfg(target_os = "macos")]
    return start("open", |o| {
        o.arg(&pad);
    });
    #[cfg(all(not(windows), not(target_os = "macos")))]
    return start("xdg-open", |o| {
        o.arg(&pad);
    });
}

/// Toont het bestand dat Mondig net opsloeg in de Verkenner, geselecteerd.
#[tauri::command]
pub(crate) fn bestand_toon_opgeslagen(
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
) -> Result<(), Foutmelding> {
    toon_in_map(&laatste(&staat, taak)?)
}

/// Opent Verkenner of Finder met `pad` geselecteerd. Ook voor een verslag in de lijst "Eerder
/// opgenomen", dat zijn pad zelf bewaart. Het pad komt nooit in het logboek.
pub(crate) fn toon_in_map(pad: &Path) -> Result<(), Foutmelding> {
    // Een pad uit `verslag.json` kan met de hand veranderd zijn. Een aanhalingsteken komt in een
    // echte Windows-bestandsnaam niet voor, en zou Explorer hieronder extra argumenten geven.
    if pad.to_string_lossy().contains('"') {
        return bestaand_opgeslagen(None).map(|_| ());
    }
    #[cfg(windows)]
    return start("explorer", |o| {
        use std::os::windows::process::CommandExt;
        // Explorer wil `/select,"pad"` als één argument, met de aanhalingstekens om het pad.
        o.raw_arg(format!("/select,\"{}\"", pad.display()));
    });
    #[cfg(target_os = "macos")]
    return start("open", |o| {
        o.arg("-R").arg(pad);
    });
    #[cfg(all(not(windows), not(target_os = "macos")))]
    return start("xdg-open", |o| {
        o.arg(pad.parent().unwrap_or(pad));
    });
}

/// Zorgt dat de opslagmap er is, zodat Verkenner iets kan openen. Een map die er al is, blijft
/// zoals hij is; ontbreekt hij (nog niets opgeslagen, of door de gebruiker weggehaald), dan
/// wordt hij aangemaakt.
fn maak_map_klaar(map: &Path) -> Result<(), Foutmelding> {
    fs::create_dir_all(map).map_err(|fout| {
        let melding = match fout.kind() {
            io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => format!(
                "Mondig mag de map {} niet aanmaken. Kies een andere map in Instellingen.",
                map.display()
            ),
            _ => format!(
                "De map {} bestaat niet en Mondig kon hem niet aanmaken. Kies een andere map in Instellingen.",
                map.display()
            ),
        };
        Foutmelding::nieuw(melding, Vervolg::Geen)
    })
}

/// Opent de opslagmap uit de instellingen in de Verkenner, ook als er nog niets in staat. Is de
/// map er niet, dan maakt Mondig hem aan.
#[tauri::command]
pub(crate) fn bewaren_open_map(app: AppHandle) -> Result<(), Foutmelding> {
    open_map(&app)
}

/// Opent de opslagmap (zie [`bewaren_open_map`]); ook voor het menu in het systeemvak.
pub(crate) fn open_map(app: &AppHandle) -> Result<(), Foutmelding> {
    let map = lees_keuzes(app).map.ok_or_else(|| {
        Foutmelding::nieuw(
            "Mondig vindt geen map om in te openen. Kies een map in Instellingen.",
            Vervolg::Geen,
        )
    })?;
    maak_map_klaar(&map)?;
    open_in_verkenner(&map)
}

/// Opent een map in de Verkenner (Finder, bestandsbeheer). Ook voor de logmap.
pub(crate) fn open_in_verkenner(map: &Path) -> Result<(), Foutmelding> {
    #[cfg(windows)]
    return start("explorer", |o| {
        o.arg(map);
    });
    #[cfg(target_os = "macos")]
    return start("open", |o| {
        o.arg(map);
    });
    #[cfg(all(not(windows), not(target_os = "macos")))]
    return start("xdg-open", |o| {
        o.arg(map);
    });
}

/// Een opgeslagen bestand zoals de bevestiging van Verwijderen het toont.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpgeslagenInfo {
    naam: String,
    map: MapInfo,
}

/// De bestanden die Verwijderen zou weghalen, op dit moment: dezelfde lijst die
/// [`bestand_verwijder`] daarna gebruikt.
#[tauri::command]
pub(crate) fn bestand_opgeslagen_lijst(
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
) -> Vec<OpgeslagenInfo> {
    staat
        .opgeslagen(taak)
        .iter()
        .map(|o| OpgeslagenInfo {
            naam: o.naam(),
            map: MapInfo::van(o.pad.parent().unwrap_or(&o.pad)),
        })
        .collect()
}

/// Wat er met één opgeslagen bestand gebeurt bij Verwijderen.
#[derive(Debug, PartialEq, Eq)]
enum Weghalen {
    /// Verwijderd.
    Weg,
    /// Gewijzigd of verplaatst na het opslaan: blijft staan, Mondig vergeet het.
    Gebleven,
    /// Al weg: Mondig vergeet het.
    Verdwenen,
    /// Open in een ander programma: blijft staan, het transcript ook.
    InGebruik,
    /// Een andere fout: blijft staan, het transcript ook.
    Mislukt,
}

fn haal_bestand_weg(bestand: &Opgeslagen) -> Weghalen {
    if !bestand.pad.exists() {
        return Weghalen::Verdwenen;
    }
    if !bestand.ongewijzigd() {
        return Weghalen::Gebleven;
    }
    match fs::remove_file(&bestand.pad) {
        Ok(()) => Weghalen::Weg,
        Err(fout) if in_gebruik(&fout) => Weghalen::InGebruik,
        Err(_) => Weghalen::Mislukt,
    }
}

/// Hoe Verwijderen afliep.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VerwijderUitslag {
    stand: BestandenStand,
    /// Is het transcript uit de lijst? Niet als er een bestand open is in een ander programma
    /// of niet weg kon: dan kan Verwijderen later nog eens.
    weg: bool,
    /// De namen van de bestanden die weg zijn.
    verwijderd: Vec<String>,
    /// Bestanden die bleven staan omdat ze na het opslaan gewijzigd of verplaatst zijn.
    gebleven: Vec<String>,
    /// Bestanden die open zijn in een ander programma.
    in_gebruik: Vec<String>,
    /// Bestanden die om een andere reden niet weg konden.
    mislukt: Vec<String>,
}

/// Verwijdert de bestanden die Mondig van dit transcript opsloeg, maar alleen als ze sinds het
/// opslaan niet gewijzigd zijn, en haalt het transcript uit de lijst. Is een bestand open in
/// een ander programma of lukt verwijderen niet, dan blijft het transcript staan met die
/// bestanden, zodat het later opnieuw kan. De interface vraagt eerst om bevestiging, met de
/// lijst uit [`bestand_opgeslagen_lijst`].
#[tauri::command]
pub(crate) fn bestand_verwijder(
    app: AppHandle,
    staat: State<'_, BestandenStaat>,
    taak: TaakId,
) -> VerwijderUitslag {
    let mut verwijderd = Vec::new();
    let mut gebleven = Vec::new();
    let mut in_gebruik = Vec::new();
    let mut mislukt = Vec::new();
    let mut vergeten = Vec::new();
    for bestand in staat.opgeslagen(taak) {
        let lijst = match haal_bestand_weg(&bestand) {
            Weghalen::Weg => Some(&mut verwijderd),
            Weghalen::Gebleven => Some(&mut gebleven),
            Weghalen::Verdwenen => None,
            Weghalen::InGebruik => {
                in_gebruik.push(bestand.naam());
                continue;
            }
            Weghalen::Mislukt => {
                mislukt.push(bestand.naam());
                continue;
            }
        };
        if let Some(lijst) = lijst {
            lijst.push(bestand.naam());
        }
        vergeten.push(bestand.pad);
    }
    let weg = in_gebruik.is_empty() && mislukt.is_empty();
    let stand = if weg {
        staat.haal_weg(&app, taak)
    } else {
        staat.vergeet(&app, taak, &vergeten)
    };
    VerwijderUitslag {
        stand,
        weg,
        verwijderd,
        gebleven,
        in_gebruik,
        mislukt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mondig_core::bewaak::Transcriptbestand;

    fn tr(naam: &str, ms: u64) -> Transcriptbestand {
        Transcriptbestand {
            naam: naam.to_string(),
            gewijzigd_ms: ms,
        }
    }

    /// 2 november 2026, 14:00 (UTC), als wijzigtijd van een transcript.
    const NA: u64 = 1_793_628_000_000;

    #[test]
    fn het_volgnummer_valt_van_de_naam_af() {
        assert_eq!(
            zonder_volgnummer("2026-11-02 Opname (2).docx"),
            "2026-11-02 Opname.docx"
        );
        assert_eq!(
            zonder_volgnummer("2026-11-02 Opname.docx"),
            "2026-11-02 Opname.docx"
        );
        assert_eq!(zonder_volgnummer("Opname (x).docx"), "Opname (x).docx");
        assert_eq!(zonder_volgnummer("zonder extensie (12)"), "zonder extensie");
    }

    #[test]
    fn alleen_een_nieuwer_transcript_met_precies_de_naam_van_de_naamregel_telt() {
        let regel = [Bouwsteen::Datum, Bouwsteen::Onderwerp];
        let datum = bewaren::lokaal_moment(NA).strftime("%Y-%m-%d").to_string();
        let eigen = format!("{datum} Teams-opname.docx");
        let met_nummer = format!("{datum} Teams-opname (2).docx");
        let jaren_ouder = NA - 400 * 24 * 3_600_000;

        // Het transcript van deze opname: precies de naam, en nieuwer dan de opname.
        assert!(is_transcript_van(
            "Teams-opname",
            jaren_ouder,
            &[tr(&eigen, NA)],
            &regel,
            ""
        ));
        assert!(is_transcript_van(
            "Teams-opname",
            jaren_ouder,
            &[tr(&met_nummer, NA)],
            &regel,
            ""
        ));
        // Hetzelfde transcript is ouder dan een nieuwe opname met dezelfde naam (VN0001 na het
        // leegmaken van de dictafoon): geen transcript van deze opname.
        assert!(!is_transcript_van(
            "Teams-opname",
            NA + 1,
            &[tr(&eigen, NA)],
            &regel,
            ""
        ));
        // Een naam die er alleen op lijkt, telt niet.
        let langer = format!("{datum} Teams-opname vervolg.docx");
        assert!(!is_transcript_van(
            "Teams-opname",
            jaren_ouder,
            &[tr(&langer, NA)],
            &regel,
            ""
        ));
        assert!(!is_transcript_van(
            "Opname",
            jaren_ouder,
            &[tr(&met_nummer, NA)],
            &regel,
            ""
        ));
        // Een naam zonder de datum van de naamregel telt niet.
        assert!(!is_transcript_van(
            "Teams-opname",
            jaren_ouder,
            &[tr("Teams-opname.docx", NA)],
            &regel,
            ""
        ));
    }

    #[test]
    fn met_alleen_de_naam_als_regel_hoort_hetzelfde_bestand_bij_de_opname() {
        assert!(is_transcript_van(
            "Interview",
            1,
            &[tr("Interview.pdf", NA)],
            &[Bouwsteen::Onderwerp],
            ""
        ));
    }

    #[test]
    fn de_nieuwe_standaard_met_een_lege_klant_herkent_een_bewaakt_transcript_nog() {
        // Na de migratie van "datum,naam" staat de klant tussen datum en onderwerp; zonder klant
        // verandert de naam niet, dus de bewaakte map vindt het transcript nog.
        let regel = lees_bouwstenen("datum,naam");
        assert_eq!(regel, STANDAARD_BOUWSTENEN);
        let datum = bewaren::lokaal_moment(NA).strftime("%Y-%m-%d").to_string();
        assert!(is_transcript_van(
            "Teams-opname",
            1,
            &[tr(&format!("{datum} Teams-opname.docx"), NA)],
            &regel,
            ""
        ));
    }

    #[test]
    fn het_voorbeeld_volgt_klant_onderwerp_en_datum_van_het_document() {
        let moment = bewaren::lokaal_moment(NA);
        let mut document = Document {
            titel: "Verslag".to_string(),
            bron: String::new(),
            model: "Parakeet".to_string(),
            klant: None,
            onderwerp: None,
            datum: None,
            standaard_onderwerp: "Interview".to_string(),
            moment,
            duur_ms: 0,
            melding: None,
            regels: Vec::new(),
        };
        let keuzes = Keuzes {
            map: None,
            bouwstenen: STANDAARD_BOUWSTENEN.to_vec(),
            formaat: Formaat::Docx,
            tijden: true,
            automatisch: false,
            audio: false,
        };
        let datum = bewaren::lokaal_moment(NA).strftime("%Y-%m-%d").to_string();
        assert_eq!(
            naam_voor(&document, &keuzes, keuzes.formaat),
            format!("{datum} Interview.docx")
        );
        document.klant = Some("Gemeente Vlaardingen".to_string());
        document.onderwerp = Some("Intake".to_string());
        document.datum = Some("2026-10-30".to_string());
        assert_eq!(
            naam_voor(&document, &keuzes, keuzes.formaat),
            "2026-10-30 Gemeente Vlaardingen Intake.docx"
        );
    }

    fn tijdelijke_map(naam: &str) -> PathBuf {
        let map =
            std::env::temp_dir().join(format!("mondig-bewaren-{naam}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&map);
        fs::create_dir_all(&map).expect("map");
        map
    }

    /// Opent een bestand zoals Word dat doet: anderen mogen het niet schrijven of verwijderen.
    #[cfg(windows)]
    fn houd_open(pad: &Path) -> fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(1) // FILE_SHARE_READ: geen schrijven, geen verwijderen
            .open(pad)
            .expect("openen")
    }

    #[test]
    fn atomair_schrijven_overschrijft_nooit_stil_en_laat_niets_achter() {
        let map = tijdelijke_map("atomair");
        let pad = map.join("a.txt");
        schrijf_atomair(&pad, b"eerste", false).expect("eerste keer");
        let tweede = schrijf_atomair(&pad, b"tweede", false).expect_err("bestaat al");
        assert_eq!(tweede.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&pad).expect("lezen"), b"eerste");
        schrijf_atomair(&pad, b"tweede", true).expect("vervangen");
        assert_eq!(fs::read(&pad).expect("lezen"), b"tweede");
        // Geen tijdelijk bestand achtergebleven, ook niet na de mislukte poging.
        assert_eq!(fs::read_dir(&map).expect("lijst").count(), 1);
        let _ = fs::remove_dir_all(&map);
    }

    #[test]
    fn de_map_wordt_aangemaakt_als_hij_er_niet_is() {
        let basis = tijdelijke_map("openmap");
        let map = basis.join("Mondig").join("Uitschriften");
        assert!(!map.exists());
        maak_map_klaar(&map).expect("aanmaken");
        assert!(map.is_dir());
        // Een tweede keer, met inhoud: de map blijft zoals hij was.
        fs::write(map.join("a.txt"), b"x").expect("schrijven");
        maak_map_klaar(&map).expect("bestaat al");
        assert!(map.join("a.txt").is_file());
        let _ = fs::remove_dir_all(&basis);
    }

    #[test]
    fn een_bestand_in_plaats_van_een_map_geeft_een_melding_in_gewone_taal() {
        let basis = tijdelijke_map("openmap-bestand");
        let pad = basis.join("geen-map");
        fs::write(&pad, b"x").expect("schrijven");
        let fout = maak_map_klaar(&pad).expect_err("is een bestand");
        assert!(fout.melding.contains("Kies een andere map in Instellingen"));
        let _ = fs::remove_dir_all(&basis);
    }

    #[test]
    fn eerste_nummer_leest_de_map_een_keer() {
        let map = tijdelijke_map("nummer");
        assert_eq!(eerste_nummer(&map, "Naam.docx"), 2);
        fs::write(map.join("Naam.docx"), b"x").expect("schrijven");
        fs::write(map.join("naam (7).DOCX"), b"x").expect("schrijven");
        assert_eq!(eerste_nummer(&map, "Naam.docx"), 8);
        // Een map die niet bestaat: gewoon bij 2 beginnen; schrijven geeft dan de fout.
        assert_eq!(eerste_nummer(&map.join("weg"), "Naam.docx"), 2);
        let _ = fs::remove_dir_all(&map);
    }

    #[test]
    fn de_vraag_blijft_staan_bij_een_verkeerd_id() {
        let houder = Mutex::new(Some(Vraag {
            id: 7,
            taak: TaakId::nieuw(),
            map: PathBuf::new(),
            naam: String::new(),
            inhoud: Vec::new(),
        }));
        assert!(neem_vraag(&houder, 6).is_none());
        assert!(slot(&houder).is_some());
        assert_eq!(neem_vraag(&houder, 7).map(|v| v.id), Some(7));
        assert!(slot(&houder).is_none());
    }

    #[test]
    fn verwijderen_alleen_als_ongewijzigd() {
        let map = tijdelijke_map("ongewijzigd");
        let pad = map.join("b.txt");
        fs::write(&pad, b"van mondig").expect("schrijven");
        let opgeslagen = Opgeslagen::van(pad.clone()).expect("gegevens");
        assert!(opgeslagen.ongewijzigd());
        fs::write(&pad, b"door iemand anders aangepast").expect("schrijven");
        assert!(!opgeslagen.ongewijzigd());
        assert_eq!(haal_bestand_weg(&opgeslagen), Weghalen::Gebleven);
        assert!(pad.exists());
        fs::remove_file(&pad).expect("weg");
        assert_eq!(haal_bestand_weg(&opgeslagen), Weghalen::Verdwenen);
        let _ = fs::remove_dir_all(&map);
    }

    #[cfg(windows)]
    #[test]
    fn verwijderen_van_een_open_bestand_is_in_gebruik() {
        let map = tijdelijke_map("verwijder-open");
        let pad = map.join("c.docx");
        fs::write(&pad, b"van mondig").expect("schrijven");
        let opgeslagen = Opgeslagen::van(pad.clone()).expect("gegevens");
        let open = houd_open(&pad);
        assert_eq!(haal_bestand_weg(&opgeslagen), Weghalen::InGebruik);
        assert!(pad.exists());
        drop(open);
        assert_eq!(haal_bestand_weg(&opgeslagen), Weghalen::Weg);
        let _ = fs::remove_dir_all(&map);
    }

    #[cfg(windows)]
    #[test]
    fn vervangen_van_een_open_bestand_zegt_dat_het_open_is() {
        let map = tijdelijke_map("vervang-open");
        let pad = map.join("d.docx");
        fs::write(&pad, b"oud").expect("schrijven");
        let open = houd_open(&pad);
        let fout = schrijf_atomair(&pad, b"nieuw", true).expect_err("in gebruik");
        assert_eq!(schrijffout(&fout, &map, Some(&pad)).melding, IN_GEBRUIK);
        drop(open);
        assert_eq!(fs::read(&pad).expect("lezen"), b"oud");
        // Het tijdelijke bestand is opgeruimd.
        assert_eq!(fs::read_dir(&map).expect("lijst").count(), 1);
        let _ = fs::remove_dir_all(&map);
    }

    #[test]
    fn de_schakelaar_voor_audio_staat_standaard_uit_en_gaat_heen_en_terug() {
        assert!(!standaard_uit(None), "standaard uit");
        assert!(!standaard_uit(Some("iets")));
        assert!(standaard_uit(Some(aan_uit(true))));
        assert!(!standaard_uit(Some(aan_uit(false))));
    }

    #[test]
    fn de_wav_krijgt_dezelfde_basisnaam_als_het_verslag() {
        assert_eq!(
            als_wav("2026-11-02 Vergadering 14.00.docx"),
            "2026-11-02 Vergadering 14.00.wav"
        );
        assert_eq!(als_wav("Notitie (2).md"), "Notitie (2).wav");
        assert_eq!(als_wav("zonder"), "zonder.wav");
    }

    #[test]
    fn de_voorgestelde_wav_volgt_de_bouwstenen_van_het_verslag() {
        let moment = bewaren::lokaal_moment(0);
        let document = Document {
            titel: "Vergadering van vandaag".to_string(),
            bron: String::new(),
            model: "Parakeet".to_string(),
            klant: Some("Z-12".to_string()),
            onderwerp: None,
            datum: None,
            standaard_onderwerp: "Vergadering van vandaag".to_string(),
            moment,
            duur_ms: 0,
            melding: None,
            regels: Vec::new(),
        };
        let keuzes = Keuzes {
            map: None,
            bouwstenen: vec![Bouwsteen::Onderwerp, Bouwsteen::Klant],
            formaat: Formaat::Docx,
            tijden: true,
            automatisch: false,
            audio: true,
        };
        assert_eq!(
            als_wav(&naam_voor(&document, &keuzes, keuzes.formaat)),
            "Vergadering van vandaag Z-12.wav"
        );
    }

    #[test]
    fn schrijffout_in_gewone_taal() {
        let map = PathBuf::from("X:/weg");
        if cfg!(windows) {
            let bezet = io::Error::from_raw_os_error(32);
            assert_eq!(schrijffout(&bezet, &map, None).melding, IN_GEBRUIK);
            // Fout 5 zonder bestaand doel: de map is het probleem, niet een open bestand.
            let geweigerd = io::Error::from_raw_os_error(5);
            assert!(
                schrijffout(&geweigerd, &map, Some(&map.join("x.docx")))
                    .melding
                    .contains("mag niet schrijven")
            );
        }
        let weg = io::Error::from(io::ErrorKind::NotFound);
        assert!(
            schrijffout(&weg, &map, None)
                .melding
                .contains("bestaat niet")
        );
    }
}
