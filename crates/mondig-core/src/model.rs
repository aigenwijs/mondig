//! Modelcatalogus: welke spraakmodellen Mondig kent, met bestanden, controlesommen en licenties.
//!
//! De catalogus staat als JSON in de crate (`modellen.json`) en wordt bij het gebruik
//! ingelezen en gecontroleerd. Deze module bevat geen netwerkcode; downloaden zit in
//! de module `download` (Cargo-feature `download`).
//!
//! Elke URL heeft een vaste commit in het pad, zodat een bestand nooit stil verandert.
//! Het basisadres is later te vervangen (ADR 0005), zie [`Model::met_basisadres`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

/// De ingebouwde catalogus.
const CATALOGUS_JSON: &str = include_str!("modellen.json");

/// Welke engine het model uitvoert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum Engine {
    /// NVIDIA Parakeet als GGUF (ggml, via transcribe-cpp).
    Parakeet,
    /// NVIDIA Canary als GGUF (ggml, via transcribe-cpp).
    Canary,
    /// Whisper als GGUF (ggml, via transcribe-cpp).
    Whisper,
    /// Silero als GGUF (ggml, via transcribe-cpp): detecteert spraak, schrijft niets uit.
    Silero,
    /// Sortformer als GGUF (ggml, via transcribe-cpp): herkent wie er spreekt, schrijft niets
    /// uit (ADR 0018).
    Sortformer,
}

/// Waar een model voor dient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rol {
    /// Schrijft spraak uit. Dit zijn de modellen die de gebruiker ziet en kiest.
    #[default]
    Spraak,
    /// Vindt uit waar gesproken wordt (stiltedetectie). Een hulpmodel: het komt mee met
    /// een spraakmodel en staat niet in lijsten of keuzes.
    Vad,
    /// Herkent wie er spreekt (Sortformer, ADR 0018). Ook een hulpmodel, maar het komt niet
    /// vanzelf mee met een spraakmodel: alleen wie sprekers wil herkennen, downloadt het.
    Sprekers,
}

/// Wat een model de gebruiker biedt; de pagina Modellen deelt hierop in. Een nieuw soort
/// komt erbij zodra er een model voor is (sprekerherkenning, persoonsgegevens vinden).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Soort {
    /// Spraak naar tekst.
    Spraak,
    /// Sprekers herkennen en uit elkaar houden.
    Sprekers,
    /// Persoonsgegevens in een tekst vinden.
    Privacy,
    /// Hulpmodel (zoals Silero): komt mee met een ander model en is niet zichtbaar.
    Hulp,
}

/// De twee taken waarvoor de gebruiker een eigen spraakmodel kiest (sprint 5-A). Dicteren
/// vraagt een model dat direct antwoordt; bestanden mogen langer duren voor meer
/// nauwkeurigheid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Gebruik {
    /// Dicteren met de sneltoets (en de proef op het dicteerscherm).
    Dicteren,
    /// Opnames en video's uitschrijven op het scherm Bestanden.
    Bestanden,
}

impl Gebruik {
    /// Beide taken, in de volgorde waarin de interface ze toont.
    pub const ALLE: [Gebruik; 2] = [Gebruik::Dicteren, Gebruik::Bestanden];

    /// De naam zoals die in de instellingen en in het verkeer met de interface staat.
    #[must_use]
    pub const fn sleutel(self) -> &'static str {
        match self {
            Self::Dicteren => "dicteren",
            Self::Bestanden => "bestanden",
        }
    }

    /// Het omgekeerde van [`Gebruik::sleutel`].
    #[must_use]
    pub fn uit_sleutel(sleutel: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|g| g.sleutel() == sleutel)
    }
}

/// Wat we over een model zeggen voor één taak. Maximaal één model is per taak `Aanbevolen`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Label {
    /// De beste keuze voor deze taak.
    Aanbevolen,
    /// Sneller dan het aanbevolen model.
    Sneller,
    /// Nauwkeuriger dan het aanbevolen model, maar langzamer.
    Nauwkeuriger,
    /// Te langzaam om bij deze taak te kiezen; het model staat dan ook in `niet_voor`.
    TeTraag,
}

/// Eén bestand dat bij een model hoort.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModelBestand {
    /// Bestandsnaam in de modelmap (geen submappen).
    pub pad: String,
    /// Volledig downloadadres, met vaste commit in het pad.
    pub url: String,
    /// SHA-256 van het bestand, 64 hexadecimale tekens (kleine letters).
    pub sha256: String,
    /// Grootte in bytes.
    pub grootte: u64,
}

/// Een spraakmodel uit de catalogus.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Model {
    /// Vaste, korte naam; ook de mapnaam op schijf.
    pub id: String,
    /// Naam in gewone taal voor in de interface.
    pub naam: String,
    /// Welke engine het model draait.
    pub engine: Engine,
    /// Waar het model voor dient. Ontbreekt de sleutel in de catalogus, dan is het een
    /// spraakmodel.
    #[serde(default)]
    pub rol: Rol,
    /// Waar het model de gebruiker mee helpt; bepaalt de sectie op de pagina Modellen.
    /// Verplicht in de catalogus. `hulp` hoort bij rol `vad` en andersom.
    pub soort: Soort,
    /// Nauwkeurigheid in vijf vaste stappen (1 tot 5), voor de balkjes op de pagina. Afgeleid
    /// van onze bench op de CI-set (MLS, processor), uit het woordfoutpercentage: tot 9 % = 5,
    /// tot 10 % = 4, tot 12 % = 3, tot 15 % = 2, daarboven 1. Gemeten: Parakeet Ultra
    /// 10,5 %, Canary 1b-v2 9,7 %, Whisper large-v3 turbo 8,8 % (`eval/basislijn.json`).
    /// Verplicht bij elk model dat de gebruiker ziet; ontbreekt bij een hulpmodel.
    #[serde(default)]
    pub nauwkeurigheid: Option<u8>,
    /// Snelheid in vijf vaste stappen (1 tot 5), afgeleid van dezelfde bench uit de
    /// realtime-factor op de processor: tot 0,06 = 5, tot 0,12 = 4, tot 0,25 = 3, tot 0,5 = 2,
    /// daarboven 1. Gemeten: Ultra 0,046, Canary 0,088, Whisper turbo 0,436.
    #[serde(default)]
    pub snelheid: Option<u8>,
    /// Licentie als SPDX-id, bijvoorbeeld `MIT` of `CC-BY-4.0`.
    pub licentie: String,
    /// Ondersteunde talen als ISO 639-codes.
    pub talen: Vec<String>,
    /// Ruwe schatting van het werkgeheugen dat nodig is, in megabytes.
    pub min_geheugen_mb: u32,
    /// De bestanden die samen het model vormen.
    pub bestanden: Vec<ModelBestand>,
    /// Id's van hulpmodellen die met dit model meekomen (bijvoorbeeld Silero). Wie dit
    /// model downloadt, krijgt ze erbij als ze er nog niet staan.
    #[serde(default)]
    pub hulp: Vec<String>,
    /// Wat we over dit model zeggen per taak. Een taak zonder label heeft geen oordeel.
    #[serde(default)]
    pub labels: BTreeMap<Gebruik, Label>,
    /// Taken waarvoor dit model niet te kiezen is (Whisper turbo is te traag om mee te
    /// dicteren). De regel geldt in de app, niet alleen in de interface.
    #[serde(default)]
    pub niet_voor: Vec<Gebruik>,
}

impl Model {
    /// Mag de gebruiker dit model kiezen voor deze taak?
    #[must_use]
    pub fn kiesbaar_voor(&self, gebruik: Gebruik) -> bool {
        self.is_spraakmodel() && !self.niet_voor.contains(&gebruik)
    }

    /// Is dit een model dat spraak uitschrijft (en dus in lijsten en keuzes hoort)?
    #[must_use]
    pub fn is_spraakmodel(&self) -> bool {
        self.rol == Rol::Spraak
    }

    /// Totale downloadgrootte van alle bestanden, in bytes.
    #[must_use]
    pub fn totale_grootte(&self) -> u64 {
        self.bestanden.iter().map(|b| b.grootte).sum()
    }

    /// Geeft een kopie waarvan alle adressen naar een ander basisadres wijzen,
    /// bijvoorbeeld `https://modellen.aigenwijs.com` (ADR 0005). Het pad na de host
    /// blijft gelijk, dus de mirror bevat dezelfde mappenstructuur als Hugging Face.
    ///
    /// # Errors
    ///
    /// Geeft [`ModelFout::OngeldigAdres`] als het basisadres of een bestandsadres
    /// geen `schema://host/pad` is.
    pub fn met_basisadres(&self, basis: &str) -> Result<Model, ModelFout> {
        let basis = basis.trim_end_matches('/');
        if !basis.contains("://") || basis.ends_with("://") {
            return Err(ModelFout::OngeldigAdres(basis.to_string()));
        }
        let mut kopie = self.clone();
        for bestand in &mut kopie.bestanden {
            let pad = pad_na_host(&bestand.url)
                .ok_or_else(|| ModelFout::OngeldigAdres(bestand.url.clone()))?;
            bestand.url = format!("{basis}{pad}");
        }
        Ok(kopie)
    }

    /// Controleert of de catalogusregel compleet en veilig is.
    fn valideer(&self) -> Result<(), ModelFout> {
        let fout = |reden: &str| ModelFout::OngeldigeCatalogus(format!("{}: {reden}", self.id));
        if self.id.is_empty() || self.naam.is_empty() {
            return Err(fout("id of naam ontbreekt"));
        }
        if self.licentie.trim().is_empty() {
            return Err(fout("licentie ontbreekt"));
        }
        if self.bestanden.is_empty() {
            return Err(fout("geen bestanden"));
        }
        if (self.soort == Soort::Hulp) != (self.rol == Rol::Vad) {
            return Err(fout("soort hulp hoort bij rol vad en andersom"));
        }
        if (self.soort == Soort::Sprekers) != (self.rol == Rol::Sprekers) {
            return Err(fout("soort sprekers hoort bij rol sprekers en andersom"));
        }
        // De balkjes komen uit de bench van spraakmodellen; een hulpmodel heeft ze niet.
        for (naam, stap) in [
            ("nauwkeurigheid", self.nauwkeurigheid),
            ("snelheid", self.snelheid),
        ] {
            match (self.is_spraakmodel(), stap) {
                (true, Some(1..=5)) | (false, None) => {}
                (true, _) => return Err(fout(&format!("{naam} moet 1 tot 5 zijn"))),
                (false, Some(_)) => return Err(fout(&format!("een hulpmodel heeft geen {naam}"))),
            }
        }
        if self.is_spraakmodel() && self.talen.is_empty() {
            return Err(fout("een spraakmodel heeft talen nodig"));
        }
        if !self.is_spraakmodel() && !self.hulp.is_empty() {
            return Err(fout("een hulpmodel heeft zelf geen hulpmodellen"));
        }
        if !self.is_spraakmodel() && (!self.labels.is_empty() || !self.niet_voor.is_empty()) {
            return Err(fout("een hulpmodel heeft geen labels of taakbeperkingen"));
        }
        for (gebruik, label) in &self.labels {
            if *label == Label::TeTraag && self.kiesbaar_voor(*gebruik) {
                return Err(fout(
                    "een model dat te traag is, moet ook niet kiesbaar zijn",
                ));
            }
        }
        for b in &self.bestanden {
            if b.pad.is_empty() || b.pad.contains(['/', '\\']) || b.pad.contains("..") {
                return Err(fout("bestandsnaam is geen gewone bestandsnaam"));
            }
            if !is_sha256(&b.sha256) {
                return Err(fout("sha256 ontbreekt of is ongeldig"));
            }
            if b.grootte == 0 {
                return Err(fout("grootte ontbreekt"));
            }
            if !b.url.starts_with("https://") {
                return Err(fout("adres is geen https-adres"));
            }
        }
        Ok(())
    }
}

/// Alles vanaf het eerste `/` na de host, inclusief die `/`.
fn pad_na_host(url: &str) -> Option<&str> {
    let na_schema = url.split_once("://")?.1;
    let begin = na_schema.find('/')?;
    Some(&na_schema[begin..])
}

/// Is dit een geldige SHA-256 in kleine hexadecimale letters?
fn is_sha256(tekst: &str) -> bool {
    tekst.len() == 64
        && tekst
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Wat er mis kan gaan met de catalogus.
#[derive(Debug, thiserror::Error)]
pub enum ModelFout {
    /// De ingebouwde catalogus is niet te lezen of klopt niet. Dit is een programmeerfout.
    #[error("De modelcatalogus klopt niet: {0}")]
    OngeldigeCatalogus(String),
    /// Er is geen model met dit id.
    #[error("Onbekend model: {0}")]
    Onbekend(String),
    /// Een adres heeft niet de vorm `schema://host/pad`.
    #[error("Ongeldig adres: {0}")]
    OngeldigAdres(String),
    /// Het besturingssysteem kent geen gegevensmap voor deze gebruiker.
    #[error("Er is geen gegevensmap gevonden op deze computer.")]
    GeenGegevensmap,
}

/// Bundle-identifier (ADR 0005); CLI en app gebruiken dezelfde map voor hun gegevens.
pub(crate) const IDENTIFIER: &str = "com.aigenwijs.spraak";

/// Model dat Mondig standaard gebruikt: snel en goed in het Nederlands (Parakeet Ultra).
pub const STANDAARD_MODEL: &str = "parakeet-ultra-q8";

/// Het hulpmodel dat stilte aan begin en eind van een dictaat wegknipt (Silero).
pub const SILERO_MODEL: &str = "silero-vad-v6.2";

/// Het model dat sprekers herkent (Sortformer, ADR 0018). Het staat naast het spraakmodel, net
/// als Silero, maar komt niet mee met een spraakmodel: de gebruiker downloadt het apart.
pub const SPREKERMODEL: &str = "sortformer-4spk-v2.1-q8";

/// Het bestand van het sprekermodel naast de map van het spraakmodel, als het er helemaal
/// staat (juiste grootte; de hash is bij het downloaden al gecontroleerd). Een half of
/// ontbrekend bestand geeft `None`. Het werkproces en de app gebruiken dezelfde regel, zodat
/// de app weet of het zin heeft om sprekers te vragen.
#[must_use]
pub fn sprekermodel_naast(modelmap: &Path) -> Option<PathBuf> {
    let model = zoek(SPREKERMODEL).ok()?;
    let bestand = model.bestanden.first()?;
    let pad = hulpmodelmap_naast(modelmap, &model.id).join(&bestand.pad);
    let compleet = std::fs::metadata(&pad).is_ok_and(|m| m.len() == bestand.grootte);
    compleet.then_some(pad)
}

/// Waar een hulpmodel staat als je de map van het spraakmodel kent: naast die map, onder
/// zijn eigen id. Het werkproces vindt Silero zo zonder dat het protocol een extra pad
/// nodig heeft; de downloader zet het op dezelfde plek als [`modelmap`].
#[must_use]
pub fn hulpmodelmap_naast(modelmap: &Path, hulp_id: &str) -> PathBuf {
    modelmap
        .parent()
        .map_or_else(|| PathBuf::from(hulp_id), |ouder| ouder.join(hulp_id))
}

/// Map waarin de bestanden van één model staan.
///
/// Op Windows in `AppData\Local`, niet in `Roaming`: een meereizend profiel (bij
/// gemeenten gangbaar) kopieert `Roaming` bij elke aan- en afmelding naar de server,
/// en modellen zijn honderden MB's per stuk. Op macOS en Linux is dit dezelfde map
/// als de gewone gegevensmap.
///
/// # Errors
///
/// Geeft [`ModelFout::GeenGegevensmap`] als het besturingssysteem geen gegevensmap kent.
pub fn modelmap(id: &str) -> Result<PathBuf, ModelFout> {
    Ok(modellen_map()?.join(id))
}

/// De map waarin elk model een eigen submap heeft (zie [`modelmap`]).
///
/// # Errors
///
/// Geeft [`ModelFout::GeenGegevensmap`] als het systeem geen lokale gegevensmap kent.
pub fn modellen_map() -> Result<PathBuf, ModelFout> {
    let basis = dirs::data_local_dir().ok_or(ModelFout::GeenGegevensmap)?;
    Ok(basis.join(IDENTIFIER).join("modellen"))
}

/// De ingebouwde catalogus, één keer gelezen en gecontroleerd: hij staat in de crate en
/// verandert niet zolang het programma draait. Zo kost een vraag als "welk model geldt nu"
/// (bij elke druk op de sneltoets) geen nieuwe JSON-parse.
static CATALOGUS: LazyLock<Result<Vec<Model>, String>> = LazyLock::new(|| {
    lees_catalogus().map_err(|fout| match fout {
        ModelFout::OngeldigeCatalogus(tekst) => tekst,
        ander => ander.to_string(),
    })
});

/// De ingebouwde catalogus, gecontroleerd (zie [`lees_catalogus`]).
///
/// # Errors
///
/// Geeft [`ModelFout::OngeldigeCatalogus`] als de JSON niet klopt, een regel onvolledig is
/// of een id dubbel voorkomt.
pub fn catalogus() -> Result<Vec<Model>, ModelFout> {
    CATALOGUS.clone().map_err(ModelFout::OngeldigeCatalogus)
}

/// Leest en controleert de ingebouwde catalogus.
///
/// # Errors
///
/// Geeft [`ModelFout::OngeldigeCatalogus`] als de JSON niet klopt, een regel onvolledig is
/// of een id dubbel voorkomt.
fn lees_catalogus() -> Result<Vec<Model>, ModelFout> {
    #[derive(Deserialize)]
    struct Bestand {
        modellen: Vec<Model>,
    }
    let bestand: Bestand = serde_json::from_str(CATALOGUS_JSON)
        .map_err(|e| ModelFout::OngeldigeCatalogus(e.to_string()))?;
    for (i, model) in bestand.modellen.iter().enumerate() {
        model.valideer()?;
        if bestand.modellen[..i].iter().any(|m| m.id == model.id) {
            return Err(ModelFout::OngeldigeCatalogus(format!(
                "{}: id komt dubbel voor",
                model.id
            )));
        }
        for hulp in &model.hulp {
            let is_hulpmodel = bestand
                .modellen
                .iter()
                .any(|m| &m.id == hulp && m.rol == Rol::Vad);
            if !is_hulpmodel {
                return Err(ModelFout::OngeldigeCatalogus(format!(
                    "{}: hulpmodel {hulp} staat niet in de catalogus als hulpmodel",
                    model.id
                )));
            }
        }
    }
    for gebruik in Gebruik::ALLE {
        let aanbevolen = bestand
            .modellen
            .iter()
            .filter(|m| m.labels.get(&gebruik) == Some(&Label::Aanbevolen))
            .count();
        if aanbevolen > 1 {
            return Err(ModelFout::OngeldigeCatalogus(format!(
                "meer dan één aanbevolen model voor {}",
                gebruik.sleutel()
            )));
        }
    }
    Ok(bestand.modellen)
}

/// Alleen de spraakmodellen: wat de gebruiker in lijsten en keuzes ziet. Hulpmodellen
/// als Silero horen er niet bij.
///
/// # Errors
///
/// Zie [`catalogus`].
pub fn spraakmodellen() -> Result<Vec<Model>, ModelFout> {
    Ok(catalogus()?
        .into_iter()
        .filter(Model::is_spraakmodel)
        .collect())
}

/// Hoe een spraakmodel heet als de gebruiker een model kiest om een verslag opnieuw uit te
/// schrijven (ADR 0023, punt 6): bij zijn rol, niet bij zijn naam (N-B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Modelrol {
    /// Het snelste model.
    Snel,
    /// Nauwkeuriger dan het snelle model.
    Nauwkeurig,
    /// Het nauwkeurigste model, als er naast het snelle nog twee zijn.
    ExtraNauwkeurig,
}

/// De rol van spraakmodel `id` tussen de modellen in `modellen` die voor bestanden te kiezen
/// zijn. Het model met de hoogste snelheid is het snelle; van de andere is het nauwkeurigste
/// "extra nauwkeurig" als er nog een is, en de rest "nauwkeurig". Zo heten Parakeet, Canary en
/// Whisper turbo snel, nauwkeurig en extra nauwkeurig. `None` als het geen spraakmodel voor
/// bestanden is.
#[must_use]
pub fn modelrol(modellen: &[Model], id: &str) -> Option<Modelrol> {
    let mut kiesbaar: Vec<&Model> = modellen
        .iter()
        .filter(|m| m.kiesbaar_voor(Gebruik::Bestanden))
        .collect();
    if !kiesbaar.iter().any(|m| m.id == id) {
        return None;
    }
    // Het snelste eerst; bij gelijke snelheid het minst nauwkeurige, dan op id (vast).
    kiesbaar.sort_by(|a, b| {
        b.snelheid
            .cmp(&a.snelheid)
            .then(a.nauwkeurigheid.cmp(&b.nauwkeurigheid))
            .then(a.id.cmp(&b.id))
    });
    let (snelste, rest) = kiesbaar.split_first()?;
    if snelste.id == id {
        return Some(Modelrol::Snel);
    }
    let nauwkeurigste = rest
        .iter()
        .max_by(|a, b| {
            a.nauwkeurigheid
                .cmp(&b.nauwkeurigheid)
                .then(b.id.cmp(&a.id))
        })
        .map(|m| m.id.as_str());
    if rest.len() >= 2 && nauwkeurigste == Some(id) {
        Some(Modelrol::ExtraNauwkeurig)
    } else {
        Some(Modelrol::Nauwkeurig)
    }
}

/// De rol van spraakmodel `id` in de ingebouwde catalogus (zie [`modelrol`]), zonder de
/// catalogus te kopiëren. `None` als de catalogus niet klopt of het geen spraakmodel voor
/// bestanden is.
#[must_use]
pub fn rol_van(id: &str) -> Option<Modelrol> {
    CATALOGUS
        .as_ref()
        .ok()
        .and_then(|modellen| modelrol(modellen, id))
}

/// De hulpmodellen die bij een model horen (leeg als er geen zijn).
///
/// # Errors
///
/// Zie [`catalogus`].
pub fn hulpmodellen_van(model: &Model) -> Result<Vec<Model>, ModelFout> {
    let alle = catalogus()?;
    model
        .hulp
        .iter()
        .map(|id| {
            alle.iter()
                .find(|m| &m.id == id)
                .cloned()
                .ok_or_else(|| ModelFout::Onbekend(id.clone()))
        })
        .collect()
}

/// De naamsvermelding van een model (ADR 0016). De velden staan in dezelfde `modellen.json`
/// als de rest; app en `MODEL_LICENSES.md` lezen ze daaruit. Een eigen type naast [`Model`]:
/// het bevat alleen wat de vermelding extra vraagt (naam, licentie en rol staan al in
/// [`Model`]), zodat de catalogus zelf niet verandert.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Vermelding {
    /// Id van het model in de catalogus.
    pub id: String,
    /// Maker van het oorspronkelijke model, bijvoorbeeld `NVIDIA`.
    pub maker: String,
    /// Titel van het oorspronkelijke model, zoals de maker die noemt.
    pub titel: String,
    /// Copyrightregel van de maker, als de bron er een geeft.
    pub copyright: Option<String>,
    /// Modelpagina van het oorspronkelijke model.
    pub bron_url: String,
    /// Modelpagina van de omgezette (GGUF) versie die Mondig downloadt.
    pub gguf_url: String,
    /// Tekst van de licentie.
    pub licentie_url: String,
    /// Het deel van de licentie met de garantiedisclaimer.
    pub disclaimer_url: String,
    /// Wat er aan het model is gewijzigd (omzetting, kwantisatie).
    pub wijziging: String,
}

/// De regels voor een complete naamsvermelding. Dezelfde `vermelding-regels.json` leest
/// `scripts/model-licenties.mjs`, zodat Rust en het script nooit andere regels hanteren.
const REGELS_JSON: &str = include_str!("vermelding-regels.json");

#[derive(Deserialize)]
struct Regels {
    tekstvelden: Vec<String>,
    adresvelden: Vec<String>,
    adres_begint_met: String,
    nul_of_tekst: Vec<String>,
}

/// Past de regels toe op één model uit de catalogus zoals het in de JSON staat.
fn controleer_regels(model: &serde_json::Value, regels: &Regels) -> Result<(), String> {
    let id = model.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let fout = |reden: String| format!("{id}: {reden}");
    for veld in &regels.tekstvelden {
        let ok = model
            .get(veld)
            .and_then(|v| v.as_str())
            .is_some_and(|t| !t.trim().is_empty());
        if !ok {
            return Err(fout(format!("veld \"{veld}\" ontbreekt of is leeg")));
        }
    }
    for veld in &regels.adresvelden {
        let ok = model
            .get(veld)
            .and_then(|v| v.as_str())
            .is_some_and(|t| t.starts_with(&regels.adres_begint_met));
        if !ok {
            return Err(fout(format!(
                "veld \"{veld}\" ontbreekt of begint niet met {}",
                regels.adres_begint_met
            )));
        }
    }
    for veld in &regels.nul_of_tekst {
        let ok = match model.get(veld) {
            Some(serde_json::Value::Null) => true,
            Some(serde_json::Value::String(t)) => !t.trim().is_empty(),
            _ => false,
        };
        if !ok {
            return Err(fout(format!(
                "veld \"{veld}\" ontbreekt (gebruik null als de bron er geen geeft) of is leeg"
            )));
        }
    }
    Ok(())
}

/// De naamsvermelding van alle modellen in de catalogus, in catalogusvolgorde.
///
/// # Errors
///
/// Geeft [`ModelFout::OngeldigeCatalogus`] als de JSON niet klopt of een model de regels uit
/// `vermelding-regels.json` overtreedt, of als de vermeldingen niet bij de modellen passen.
pub fn vermeldingen() -> Result<Vec<Vermelding>, ModelFout> {
    #[derive(Deserialize)]
    struct Bestand {
        modellen: Vec<Vermelding>,
    }
    let ongeldig = |reden: String| ModelFout::OngeldigeCatalogus(reden);
    let regels: Regels = serde_json::from_str(REGELS_JSON).map_err(|e| ongeldig(e.to_string()))?;
    let ruw: serde_json::Value =
        serde_json::from_str(CATALOGUS_JSON).map_err(|e| ongeldig(e.to_string()))?;
    for model in ruw
        .get("modellen")
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
    {
        controleer_regels(model, &regels).map_err(ongeldig)?;
    }
    let bestand: Bestand = serde_json::from_value(ruw).map_err(|e| ongeldig(e.to_string()))?;
    Ok(bestand.modellen)
}

/// Zoekt een model op id.
///
/// # Errors
///
/// Geeft [`ModelFout::Onbekend`] als het id niet in de catalogus staat.
pub fn zoek(id: &str) -> Result<Model, ModelFout> {
    catalogus()?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| ModelFout::Onbekend(id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Is dit een 40 tekens lange commit-hash tussen `/resolve/` en de bestandsnaam?
    fn heeft_vaste_commit(url: &str) -> bool {
        let Some((_, rest)) = url.split_once("/resolve/") else {
            return false;
        };
        let commit = rest.split('/').next().unwrap_or_default();
        commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit())
    }

    #[test]
    fn catalogus_is_geldig_en_bevat_de_verwachte_modellen() {
        let modellen = catalogus().unwrap();
        // Drie spraakmodellen, het hulpmodel Silero en het sprekermodel Sortformer.
        assert_eq!(modellen.len(), 5);
        assert_eq!(spraakmodellen().unwrap().len(), 3);
        for m in &modellen {
            assert!(!m.licentie.is_empty(), "{}", m.id);
            assert_eq!(m.talen.is_empty(), !m.is_spraakmodel(), "{}", m.id);
            assert!(m.min_geheugen_mb > 0, "{}", m.id);
            for b in &m.bestanden {
                assert!(is_sha256(&b.sha256), "{} {}", m.id, b.pad);
                assert!(b.grootte > 0, "{} {}", m.id, b.pad);
                assert!(b.url.starts_with("https://"), "{} {}", m.id, b.pad);
                assert!(heeft_vaste_commit(&b.url), "{} {}", m.id, b.pad);
                assert!(b.url.ends_with(&b.pad), "{} {}", m.id, b.pad);
            }
        }
    }

    #[test]
    fn silero_is_een_hulpmodel_en_geen_spraakmodel() {
        let silero = zoek(SILERO_MODEL).unwrap();
        assert_eq!(silero.id, "silero-vad-v6.2");
        assert_eq!(silero.rol, Rol::Vad);
        assert_eq!(silero.engine, Engine::Silero);
        assert_eq!(silero.licentie, "MIT");
        assert!(!silero.is_spraakmodel());
        assert_eq!(silero.bestanden.len(), 1);
        let bestand = &silero.bestanden[0];
        assert_eq!(bestand.pad, "silero-vad-v6.2-F32.gguf");
        assert_eq!(bestand.grootte, 1_241_056);
        assert_eq!(
            bestand.sha256,
            "0d871a800c420fdd5d1b17a3aea4296d1c39608f0ff4a9295eb76457eb1c88e6"
        );
        assert!(
            bestand
                .url
                .contains("/resolve/a9b2d61c20ae4ddf3644edb34969e8185a485448/")
        );
        // Niet in de lijst die de gebruiker te zien krijgt.
        assert!(
            spraakmodellen()
                .unwrap()
                .iter()
                .all(|m| m.id != SILERO_MODEL)
        );
    }

    #[test]
    fn sortformer_is_een_sprekermodel_dat_niet_vanzelf_meekomt() {
        let m = zoek(SPREKERMODEL).unwrap();
        assert_eq!(m.rol, Rol::Sprekers);
        assert_eq!(m.soort, Soort::Sprekers);
        assert_eq!(m.engine, Engine::Sortformer);
        assert!(!m.is_spraakmodel());
        assert_eq!((m.nauwkeurigheid, m.snelheid), (None, None));
        // Het model, de licentie en het Notice-bestand, alle drie van dezelfde commit (ADR 0018).
        let paden: Vec<&str> = m.bestanden.iter().map(|b| b.pad.as_str()).collect();
        assert_eq!(
            paden,
            [
                "diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf",
                "LICENSE",
                "NOTICE"
            ]
        );
        assert_eq!(m.bestanden[0].grootte, 139_310_336);
        assert_eq!(
            m.bestanden[0].sha256,
            "a5dacdc650790266c7a362e54e6bf51952015487edaa606c4e11632bc32442a9"
        );
        for b in &m.bestanden {
            assert!(
                b.url
                    .contains("/resolve/60ebab3ef7cae9869be0abcde13360347e96ba03/"),
                "{}",
                b.pad
            );
        }
        // Geen spraakmodel brengt het mee: 139 MB alleen voor wie het gebruikt.
        for s in spraakmodellen().unwrap() {
            assert!(!s.hulp.contains(&m.id), "{}", s.id);
        }
        assert!(!m.kiesbaar_voor(Gebruik::Bestanden));
    }

    #[test]
    fn het_sprekermodel_telt_pas_als_het_helemaal_naast_het_spraakmodel_staat() {
        let basis =
            std::env::temp_dir().join(format!("mondig-sprekermodel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        let spraak = basis.join(STANDAARD_MODEL);
        std::fs::create_dir_all(&spraak).unwrap();
        assert_eq!(sprekermodel_naast(&spraak), None);
        let m = zoek(SPREKERMODEL).unwrap();
        let map = basis.join(SPREKERMODEL);
        std::fs::create_dir_all(&map).unwrap();
        let pad = map.join(&m.bestanden[0].pad);
        std::fs::write(&pad, b"half").unwrap();
        assert_eq!(sprekermodel_naast(&spraak), None);
        std::fs::File::create(&pad)
            .unwrap()
            .set_len(m.bestanden[0].grootte)
            .unwrap();
        assert_eq!(sprekermodel_naast(&spraak), Some(pad));
        let _ = std::fs::remove_dir_all(&basis);
    }

    #[test]
    fn elk_spraakmodel_brengt_silero_mee() {
        for m in spraakmodellen().unwrap() {
            let hulp = hulpmodellen_van(&m).unwrap();
            assert_eq!(hulp.len(), 1, "{}", m.id);
            assert_eq!(hulp[0].id, SILERO_MODEL);
        }
        let silero = zoek(SILERO_MODEL).unwrap();
        assert!(hulpmodellen_van(&silero).unwrap().is_empty());
    }

    #[test]
    fn hulpmodel_staat_naast_de_map_van_het_spraakmodel() {
        let map = modelmap("parakeet-ultra-q8").unwrap();
        assert_eq!(
            hulpmodelmap_naast(&map, SILERO_MODEL),
            modelmap(SILERO_MODEL).unwrap()
        );
    }

    #[test]
    fn modelmap_staat_onder_de_lokale_gegevensmap_van_de_app() {
        let map = modelmap("parakeet-ultra-q8").unwrap();
        assert!(map.ends_with("com.aigenwijs.spraak/modellen/parakeet-ultra-q8"));
    }

    #[test]
    fn standaardmodel_staat_in_de_catalogus() {
        assert_eq!(zoek(STANDAARD_MODEL).unwrap().engine, Engine::Parakeet);
    }

    #[test]
    fn elk_model_heeft_een_complete_naamsvermelding() {
        let vermeldingen = vermeldingen().unwrap();
        let modellen = catalogus().unwrap();
        let ids: Vec<&str> = vermeldingen.iter().map(|v| v.id.as_str()).collect();
        let verwacht: Vec<&str> = modellen.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, verwacht);
    }

    #[test]
    fn de_regels_noemen_alleen_velden_die_het_type_kent() {
        // Haalt elk veld uit de regels weg: het type moet dan niet meer te lezen zijn, behalve
        // bij velden die ook bij Model horen (naam, licentie) of null mogen zijn (copyright).
        let regels: Regels = serde_json::from_str(REGELS_JSON).unwrap();
        let ruw: serde_json::Value = serde_json::from_str(CATALOGUS_JSON).unwrap();
        let eerste = ruw["modellen"][0].clone();
        for veld in regels.adresvelden.iter().chain(&regels.tekstvelden) {
            if ["naam", "licentie"].contains(&veld.as_str()) {
                continue;
            }
            let mut kopie = eerste.clone();
            kopie.as_object_mut().unwrap().remove(veld);
            assert!(
                serde_json::from_value::<Vermelding>(kopie).is_err(),
                "{veld} hoort verplicht te zijn in Vermelding"
            );
        }
    }

    #[test]
    fn regels_weigeren_een_onvolledige_vermelding() {
        let regels: Regels = serde_json::from_str(REGELS_JSON).unwrap();
        let ruw: serde_json::Value = serde_json::from_str(CATALOGUS_JSON).unwrap();
        let goed = ruw["modellen"][0].clone();
        assert!(controleer_regels(&goed, &regels).is_ok());
        for (veld, waarde) in [
            ("maker", serde_json::json!("  ")),
            ("bron_url", serde_json::json!("http://voorbeeld.invalid")),
            ("copyright", serde_json::json!("")),
        ] {
            let mut slecht = goed.clone();
            slecht[veld] = waarde;
            assert!(controleer_regels(&slecht, &regels).is_err(), "{veld}");
        }
        let mut zonder = goed;
        zonder.as_object_mut().unwrap().remove("copyright");
        assert!(controleer_regels(&zonder, &regels).is_err());
    }

    #[test]
    fn labels_per_taak_volgen_het_ontwerp_van_5a() {
        let label = |id: &str, gebruik| zoek(id).unwrap().labels.get(&gebruik).copied();
        assert_eq!(
            label(STANDAARD_MODEL, Gebruik::Dicteren),
            Some(Label::Aanbevolen)
        );
        assert_eq!(
            label(STANDAARD_MODEL, Gebruik::Bestanden),
            Some(Label::Sneller)
        );
        assert_eq!(
            label("canary-1b-v2-q8", Gebruik::Dicteren),
            Some(Label::Nauwkeuriger)
        );
        assert_eq!(
            label("canary-1b-v2-q8", Gebruik::Bestanden),
            Some(Label::Aanbevolen)
        );
        assert_eq!(
            label("whisper-large-v3-turbo-q8", Gebruik::Dicteren),
            Some(Label::TeTraag)
        );
        assert_eq!(
            label("whisper-large-v3-turbo-q8", Gebruik::Bestanden),
            Some(Label::Nauwkeuriger)
        );
    }

    #[test]
    fn whisper_turbo_is_niet_te_kiezen_om_mee_te_dicteren() {
        let whisper = zoek("whisper-large-v3-turbo-q8").unwrap();
        assert!(!whisper.kiesbaar_voor(Gebruik::Dicteren));
        assert!(whisper.kiesbaar_voor(Gebruik::Bestanden));
        // Alle andere spraakmodellen mogen overal voor.
        for m in spraakmodellen()
            .unwrap()
            .iter()
            .filter(|m| m.id != whisper.id)
        {
            assert!(m.kiesbaar_voor(Gebruik::Dicteren), "{}", m.id);
            assert!(m.kiesbaar_voor(Gebruik::Bestanden), "{}", m.id);
        }
        // Silero is een hulpmodel en nergens te kiezen.
        assert!(
            !zoek(SILERO_MODEL)
                .unwrap()
                .kiesbaar_voor(Gebruik::Bestanden)
        );
    }

    #[test]
    fn de_taken_hebben_een_sleutel_die_de_interface_en_de_instellingen_delen() {
        for gebruik in Gebruik::ALLE {
            assert_eq!(Gebruik::uit_sleutel(gebruik.sleutel()), Some(gebruik));
        }
        assert_eq!(Gebruik::uit_sleutel("vergaderen"), None);
    }

    #[test]
    fn valideer_weigert_een_te_traag_model_dat_wel_te_kiezen_is() {
        let mut model = zoek("whisper-large-v3-turbo-q8").unwrap();
        model.niet_voor.clear();
        assert!(model.valideer().is_err());
    }

    #[test]
    fn ids_zijn_uniek() {
        let modellen = catalogus().unwrap();
        let mut ids: Vec<&str> = modellen.iter().map(|m| m.id.as_str()).collect();
        ids.sort_unstable();
        let aantal = ids.len();
        ids.dedup();
        assert_eq!(aantal, ids.len());
    }

    #[test]
    fn zoeken_op_id_werkt() {
        assert_eq!(
            zoek("whisper-large-v3-turbo-q8").unwrap().engine,
            Engine::Whisper
        );
        assert!(matches!(zoek("bestaat-niet"), Err(ModelFout::Onbekend(_))));
    }

    #[test]
    fn alternatief_basisadres_vervangt_alleen_de_host() {
        let model = zoek("whisper-large-v3-turbo-q8").unwrap();
        let mirror = model
            .met_basisadres("https://modellen.aigenwijs.com/")
            .unwrap();
        let oud = &model.bestanden[0];
        let nieuw = &mirror.bestanden[0];
        assert!(
            nieuw
                .url
                .starts_with("https://modellen.aigenwijs.com/handy-computer/")
        );
        assert!(nieuw.url.ends_with(&oud.pad));
        assert!(heeft_vaste_commit(&nieuw.url));
        // De rest van het model verandert niet.
        assert_eq!(nieuw.sha256, oud.sha256);
        assert_eq!(mirror.id, model.id);
        // Het origineel blijft ongemoeid.
        assert!(oud.url.starts_with("https://huggingface.co/"));
    }

    #[test]
    fn ongeldig_basisadres_wordt_geweigerd() {
        let model = zoek("whisper-large-v3-turbo-q8").unwrap();
        assert!(model.met_basisadres("modellen.aigenwijs.com").is_err());
        assert!(model.met_basisadres("https://").is_err());
    }

    #[test]
    fn valideer_weigert_onvolledige_regels() {
        let basis = zoek("whisper-large-v3-turbo-q8").unwrap();

        let mut zonder_licentie = basis.clone();
        zonder_licentie.licentie = String::new();
        assert!(zonder_licentie.valideer().is_err());

        let mut slechte_hash = basis.clone();
        slechte_hash.bestanden[0].sha256 = "abc".into();
        assert!(slechte_hash.valideer().is_err());

        let mut http = basis.clone();
        http.bestanden[0].url = http.bestanden[0].url.replace("https://", "http://");
        assert!(http.valideer().is_err());

        let mut pad = basis.clone();
        pad.bestanden[0].pad = "../buiten.gguf".into();
        assert!(pad.valideer().is_err());

        let mut zonder_talen = basis;
        zonder_talen.talen.clear();
        assert!(zonder_talen.valideer().is_err());
    }

    #[test]
    fn soort_en_stappen_volgen_de_bench() {
        let stappen = |id: &str| {
            let m = zoek(id).unwrap();
            (m.soort, m.nauwkeurigheid, m.snelheid)
        };
        assert_eq!(
            stappen("parakeet-ultra-q8"),
            (Soort::Spraak, Some(3), Some(5))
        );
        assert_eq!(
            stappen("canary-1b-v2-q8"),
            (Soort::Spraak, Some(4), Some(4))
        );
        assert_eq!(
            stappen("whisper-large-v3-turbo-q8"),
            (Soort::Spraak, Some(5), Some(2))
        );
        assert_eq!(stappen(SILERO_MODEL), (Soort::Hulp, None, None));
        assert_eq!(stappen(SPREKERMODEL), (Soort::Sprekers, None, None));
    }

    #[test]
    fn valideer_weigert_een_verkeerde_soort_of_stap() {
        let basis = zoek("canary-1b-v2-q8").unwrap();

        let mut hulp_zonder_vad = basis.clone();
        hulp_zonder_vad.soort = Soort::Hulp;
        assert!(hulp_zonder_vad.valideer().is_err());

        let mut zonder_stap = basis.clone();
        zonder_stap.snelheid = None;
        assert!(zonder_stap.valideer().is_err());

        for slecht in [0, 6] {
            let mut buiten = basis.clone();
            buiten.nauwkeurigheid = Some(slecht);
            assert!(buiten.valideer().is_err(), "{slecht}");
        }

        let mut silero = zoek(SILERO_MODEL).unwrap();
        silero.nauwkeurigheid = Some(3);
        assert!(silero.valideer().is_err());

        let mut sprekers_als_vad = zoek(SPREKERMODEL).unwrap();
        sprekers_als_vad.rol = Rol::Vad;
        assert!(sprekers_als_vad.valideer().is_err());
    }

    #[test]
    fn de_rol_van_een_model_volgt_snelheid_en_nauwkeurigheid() {
        let modellen = catalogus().unwrap();
        assert_eq!(
            modelrol(&modellen, "parakeet-ultra-q8"),
            Some(Modelrol::Snel)
        );
        assert_eq!(
            modelrol(&modellen, "canary-1b-v2-q8"),
            Some(Modelrol::Nauwkeurig)
        );
        assert_eq!(
            modelrol(&modellen, "whisper-large-v3-turbo-q8"),
            Some(Modelrol::ExtraNauwkeurig)
        );
        // Een hulpmodel of een onbekend model heeft geen rol.
        assert_eq!(modelrol(&modellen, "silero-vad-v6.2"), None);
        assert_eq!(modelrol(&modellen, "bestaat-niet"), None);
        // Met twee modellen is er geen "extra nauwkeurig".
        let twee: Vec<Model> = modellen
            .iter()
            .filter(|m| m.id != "whisper-large-v3-turbo-q8")
            .cloned()
            .collect();
        assert_eq!(
            modelrol(&twee, "canary-1b-v2-q8"),
            Some(Modelrol::Nauwkeurig)
        );
    }
}
