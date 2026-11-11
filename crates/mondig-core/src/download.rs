//! Modellen downloaden, met voortgang, annuleren en hervatten.
//!
//! Samen met `taalmodel` de enige module in `mondig-core` met netwerkcode, en ze zit achter de
//! Cargo-feature `download` (standaard uit). Het basisadres komt uit de catalogus
//! (zie [`crate::model::Model::met_basisadres`]); deze module beslist niet waar
//! modellen op schijf staan, de aanroeper geeft de map mee.
//!
//! Werkwijze per bestand: eerst naar `<naam>.part`, bij een bestaand `.part`
//! hervatten met een `Range`-header, daarna grootte en SHA-256 controleren en pas
//! dan hernoemen. Een bestand heeft dus alleen zijn definitieve naam als het
//! gecontroleerd is. Na elke geslaagde controle komt het bestand in een klein
//! markeerbestand, zodat [`status`] niet telkens gigabytes hoeft te hashen.
//!
//! Valt de verbinding weg of ligt ze [`STILTE`] lang stil, dan probeert de download het
//! zelf opnieuw, tot [`NIEUWE_POGINGEN`] keer met een oplopende wachttijd, en gaat daarbij
//! verder waar het `.part` bleef. Pas daarna komt er een fout. Een download eindigt dus
//! altijd: een vastgelopen verbinding valt na de stilte weg en het aantal pogingen is
//! begrensd (valkuil Handy #2100).

use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::model::{self, Model, ModelBestand, ModelFout};
use crate::probleem::Probleem;
use crate::taak::{Gebeurtenis, Soort, Taak, TaakId, Toestand, Vervolg};

/// Naam van het markeerbestand in de modelmap.
const MARKEERBESTAND: &str = "geverifieerd.json";
/// Grootte van de leesbuffer bij het downloaden en hashen.
const BUFFER: usize = 128 * 1024;
/// Hoe vaak we het hernoemen opnieuw proberen als Windows het bestand nog vasthoudt.
const HERNOEM_POGINGEN: u32 = 10;
/// Pauze tussen twee pogingen om te hernoemen.
const HERNOEM_PAUZE: Duration = Duration::from_millis(300);
/// Zo lang mag een verbinding niets sturen voordat we haar opgeven en opnieuw proberen. Dit
/// geldt per leesactie, voor de kopregels en voor de inhoud (`timeout_per_read` in ureq).
pub const STILTE: Duration = Duration::from_secs(30);
/// Zo vaak probeert een download het zelf opnieuw na een weggevallen verbinding, voordat de
/// gebruiker een fout ziet.
pub const NIEUWE_POGINGEN: u32 = 5;
/// Groeit het deelbestand in een poging minstens zoveel voorbij het verste punt tot dan toe,
/// dan begint de telling van de pogingen opnieuw: een lange download op wankele wifi mag vaker
/// dan vijf keer haperen, zolang hij echt opschiet. Omdat elke nieuwe telling een megabyte
/// nieuwe grond vraagt, blijft het aantal pogingen begrensd.
const VOORUITGANG_BYTES: u64 = 1_000_000;

/// Hoe een download met haperingen omgaat. [`Opties::default`] is wat de app gebruikt; de
/// tests maken alles korter.
#[derive(Debug, Clone, Copy)]
pub struct Opties {
    /// Hoe lang een verbinding maken mag duren.
    pub verbinden: Duration,
    /// Hoe lang een verbinding niets mag sturen (zie [`STILTE`]).
    pub stilte: Duration,
    /// Hoe vaak we het zelf opnieuw proberen (zie [`NIEUWE_POGINGEN`]).
    pub nieuwe_pogingen: u32,
    /// Wachttijd voor nieuwe poging `n` (1, 2, ...).
    pub wachttijd: fn(u32) -> Duration,
    /// Hoe vaak hernoemen geprobeerd wordt als Windows het bestand vasthoudt.
    pub hernoem_pogingen: u32,
    /// Pauze tussen twee pogingen om te hernoemen.
    pub hernoem_pauze: Duration,
}

impl Default for Opties {
    fn default() -> Self {
        Self {
            verbinden: Duration::from_secs(30),
            stilte: STILTE,
            nieuwe_pogingen: NIEUWE_POGINGEN,
            wachttijd: standaard_wachttijd,
            hernoem_pogingen: HERNOEM_POGINGEN,
            hernoem_pauze: HERNOEM_PAUZE,
        }
    }
}

/// Oplopende wachttijd voor nieuwe poging `n`: 2, 4, 8, 16 en daarna 30 seconden. Zo krijgt
/// een router die opnieuw opstart of een wifi die even weg is de tijd, zonder dat een korte
/// hapering lang duurt.
#[must_use]
pub fn standaard_wachttijd(poging: u32) -> Duration {
    let seconden = 2_u64.saturating_pow(poging.max(1)).min(30);
    Duration::from_secs(seconden)
}

/// Wat een download onderweg meldt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Melding {
    /// Hoe ver de download is.
    Voortgang(Voortgang),
    /// De verbinding viel weg; na een korte pauze volgt nieuwe poging `poging` van `van`.
    NieuwePoging {
        /// De hoeveelste nieuwe poging dit is (vanaf 1).
        poging: u32,
        /// Hoeveel nieuwe pogingen er hooguit komen.
        van: u32,
    },
}

/// Zoveel extra vrije schijfruimte vragen we boven wat er nog te downloaden is, in procenten
/// (besluit, 1-11-2026). De download schrijft naar een deelbestand dat na de controle
/// wordt hernoemd, dus er is niet meer dan één keer de modelgrootte nodig; de marge is voor
/// al het andere dat de schijf vult terwijl Mondig downloadt.
pub const RUIMTE_MARGE_PERCENTAGE: u64 = 20;
/// De kleinste marge (100 MB, decimaal): ook een klein model of het laatste stukje van een
/// hervatte download laat de schijf niet vrijwel vol achter.
pub const RUIMTE_MARGE_MINIMUM: u64 = 100_000_000;
/// Hoe vaak de interface hooguit een byte-voortgang krijgt: vier keer per seconde.
pub const VOORTGANG_INTERVAL: Duration = Duration::from_millis(250);

/// Hoe ver een download is, over alle bestanden van het model samen.
/// Serialiseert als `{ "binnen": .., "totaal": .. }` (bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Voortgang {
    /// Aantal bytes dat al binnen is (inclusief eerder gedownloade delen).
    pub binnen: u64,
    /// Totaal aantal bytes van het model.
    pub totaal: u64,
}

impl Voortgang {
    /// Voortgang in hele procenten (0 tot en met 100).
    #[must_use]
    pub fn procent(&self) -> u8 {
        if self.totaal == 0 {
            return 100;
        }
        u8::try_from((self.binnen.min(self.totaal) * 100) / self.totaal).unwrap_or(100)
    }
}

/// Hoe ver een model op schijf is. Serialiseert als `ontbreekt`, `gedeeltelijk` of `compleet`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelStatus {
    /// Er is niets van het model aanwezig.
    Ontbreekt,
    /// Er is een deelbestand aanwezig of gecontroleerd; downloaden hervat waar het bleef.
    Gedeeltelijk,
    /// Alle bestanden zijn aanwezig en gecontroleerd.
    Compleet,
}

/// Wat er mis kan gaan bij het downloaden.
#[derive(Debug, thiserror::Error)]
pub enum DownloadFout {
    /// Geen verbinding of de verbinding viel weg.
    #[error("Geen verbinding met de downloadserver: {0}")]
    Netwerk(String),
    /// De server antwoordde met een foutcode.
    #[error("De downloadserver gaf een foutmelding ({status}) voor {bestand}.")]
    Server {
        /// HTTP-statuscode.
        status: u16,
        /// Het bestand waar het om ging.
        bestand: String,
    },
    /// De schijf is vol. Wat al binnen is, blijft staan.
    #[error("De schijf is vol.")]
    SchijfVol,
    /// Lezen of schrijven op schijf mislukte.
    #[error("Er ging iets mis op de schijf: {0}")]
    Schijf(String),
    /// De verbinding viel weg voordat het bestand binnen was. Opnieuw proberen hervat.
    #[error(
        "De download van {bestand} is niet afgemaakt. Probeer het opnieuw, dan gaat het verder waar het bleef."
    )]
    Onvolledig {
        /// Het bestand waar het om ging.
        bestand: String,
    },
    /// Het bestand is groter dan de catalogus zegt.
    #[error("Het gedownloade bestand {bestand} is groter dan verwacht en is verwijderd.")]
    TeGroot {
        /// Het bestand waar het om ging.
        bestand: String,
    },
    /// De controlesom klopt niet; het deelbestand is verwijderd.
    #[error(
        "Het gedownloade bestand {bestand} is niet intact (controlesom klopt niet) en is verwijderd."
    )]
    HashKlopptNiet {
        /// Het bestand waar het om ging.
        bestand: String,
    },
    /// De server stuurde iets anders dan het bestand (bijvoorbeeld de inlogpagina van een
    /// wifi-netwerk of een proxy). Het deelbestand blijft onaangeroerd; opnieuw proberen kan.
    #[error("De downloadserver stuurde iets anders dan {bestand}.")]
    OnverwachtAntwoord {
        /// Het bestand waar het om ging.
        bestand: String,
    },
    /// Het bestand op de server heeft een andere grootte dan de catalogus zegt. Zelf opnieuw
    /// proberen helpt dan niet; het deelbestand blijft staan.
    #[error("Het bestand {bestand} op de downloadserver is anders dan verwacht.")]
    AnderBestand {
        /// Het bestand waar het om ging.
        bestand: String,
    },
    /// De gebruiker heeft de download gestopt; het deelbestand blijft staan.
    #[error("De download is gestopt. Je kunt later verdergaan waar het bleef.")]
    Geannuleerd,
    /// Een bestand van het model kon niet worden verwijderd.
    #[error("Het bestand {bestand} kon niet worden verwijderd.")]
    NietVerwijderd {
        /// Het bestand waar het om ging.
        bestand: String,
    },
}

impl From<io::Error> for DownloadFout {
    fn from(fout: io::Error) -> Self {
        match Probleem::uit_io(fout.kind()) {
            Some(Probleem::SchijfVol) => Self::SchijfVol,
            _ => Self::Schijf(fout.to_string()),
        }
    }
}

/// Een bestand dat gecontroleerd is, zoals vastgelegd in het markeerbestand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Gecontroleerd {
    pad: String,
    sha256: String,
    grootte: u64,
    /// Wijzigingstijd van het bestand bij de controle, in nanoseconden sinds 1970. Is het
    /// bestand daarna veranderd (ook bij gelijke grootte), dan telt het niet meer als
    /// gecontroleerd. Ontbreekt in markeerbestanden van vóór dit veld; die tellen dan
    /// niet als gecontroleerd, zodat het bestand één keer opnieuw gehasht wordt.
    #[serde(default)]
    gewijzigd: Option<u64>,
}

/// Wijzigingstijd van een bestand in nanoseconden sinds 1970, als het OS die geeft.
fn wijzigingstijd(pad: &Path) -> Option<u64> {
    let tijd = fs::metadata(pad).ok()?.modified().ok()?;
    let sinds = tijd.duration_since(std::time::UNIX_EPOCH).ok()?;
    u64::try_from(sinds.as_nanos()).ok()
}

/// Inhoud van het markeerbestand.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Markering {
    bestanden: Vec<Gecontroleerd>,
}

fn lees_markering(modelmap: &Path) -> Markering {
    fs::read_to_string(modelmap.join(MARKEERBESTAND))
        .ok()
        .and_then(|tekst| serde_json::from_str(&tekst).ok())
        .unwrap_or_default()
}

fn schrijf_markering(modelmap: &Path, markering: &Markering) -> Result<(), DownloadFout> {
    let tekst =
        serde_json::to_string_pretty(markering).map_err(|e| DownloadFout::Schijf(e.to_string()))?;
    let tijdelijk = modelmap.join(format!("{MARKEERBESTAND}.tmp"));
    fs::write(&tijdelijk, tekst)?;
    fs::rename(&tijdelijk, modelmap.join(MARKEERBESTAND))?;
    Ok(())
}

/// Staat het bestand in de markering, en zijn grootte en wijzigingstijd op schijf nog
/// gelijk aan die bij de controle?
fn is_geverifieerd(markering: &Markering, bestand: &ModelBestand, modelmap: &Path) -> bool {
    let pad = modelmap.join(&bestand.pad);
    let gewijzigd = wijzigingstijd(&pad);
    gewijzigd.is_some()
        && markering.bestanden.iter().any(|g| {
            g.pad == bestand.pad
                && g.sha256 == bestand.sha256
                && g.grootte == bestand.grootte
                && g.gewijzigd == gewijzigd
        })
        && fs::metadata(&pad).is_ok_and(|m| m.len() == bestand.grootte)
}

fn deelpad(modelmap: &Path, bestand: &ModelBestand) -> PathBuf {
    modelmap.join(format!("{}.part", bestand.pad))
}

/// Hoe ver staat dit model op schijf? Hasht niets: het vertrouwt op het markeerbestand
/// en controleert alleen of de bestanden nog even groot zijn.
#[must_use]
pub fn status(model: &Model, modelmap: &Path) -> ModelStatus {
    let markering = lees_markering(modelmap);
    if model
        .bestanden
        .iter()
        .all(|b| is_geverifieerd(&markering, b, modelmap))
    {
        return ModelStatus::Compleet;
    }
    let iets_aanwezig = model
        .bestanden
        .iter()
        .any(|b| modelmap.join(&b.pad).exists() || deelpad(modelmap, b).exists());
    if iets_aanwezig {
        ModelStatus::Gedeeltelijk
    } else {
        ModelStatus::Ontbreekt
    }
}

/// Status van één model voor de interface: wat er is en hoeveel er nog moet komen.
/// Serialiseert met camelCase-sleutels (`grootteBytes`, `aanwezigBytes`, `vrijBytes`,
/// `tekortBytes`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOverzicht {
    /// Id uit de catalogus.
    pub id: String,
    /// Hoe ver het model op schijf is.
    pub status: ModelStatus,
    /// Totale downloadgrootte in bytes.
    pub grootte_bytes: u64,
    /// Bytes die nu op schijf staan (ook van een afgebroken download).
    pub aanwezig_bytes: u64,
    /// Vrije schijfruimte op de schijf van de modelmap, als die te meten is.
    pub vrij_bytes: Option<u64>,
    /// Hoeveel ruimte er nog te weinig is om te downloaden (inclusief de marge van
    /// [`RUIMTE_MARGE_PERCENTAGE`]); 0 als er genoeg is, het model compleet is of de ruimte
    /// onbekend is.
    pub tekort_bytes: u64,
    /// Host waar het model vandaan komt, uit het adres van het eerste bestand.
    pub bron: String,
}

/// Bundelt [`status`] en [`aanwezige_bytes`] van een model voor de interface, inclusief
/// zijn hulpmodellen (Silero). Een spraakmodel waarvan het hulpmodel ontbreekt is niet
/// "compleet": de grootte en wat er aanwezig is tellen beide mee, zodat het scherm alleen
/// laat zien wat er nog gedownload moet worden.
#[must_use]
pub fn overzicht(model: &Model, modelmap: &Path) -> ModelOverzicht {
    // Lukt het opzoeken van de hulpmodellen niet, dan telt alleen het model zelf.
    let doelen = doelen_voor(model, modelmap)
        .unwrap_or_else(|_| vec![(model.clone(), modelmap.to_path_buf())]);
    overzicht_van(model, modelmap, &doelen)
}

/// Het overzicht van een model over een gegeven lijst doelen (het model zelf en zijn hulpmodellen).
fn overzicht_van(model: &Model, modelmap: &Path, doelen: &[(Model, PathBuf)]) -> ModelOverzicht {
    let statussen: Vec<ModelStatus> = doelen.iter().map(|(m, map)| status(m, map)).collect();
    let status = samengestelde_status(&statussen);
    let aanwezig_bytes: u64 = doelen.iter().map(|(m, map)| aanwezige_bytes(m, map)).sum();
    let grootte_bytes: u64 = doelen.iter().map(|(m, _)| m.totale_grootte()).sum();
    let vrij_bytes = vrije_ruimte(modelmap);
    let tekort_bytes = if status == ModelStatus::Compleet {
        0
    } else {
        ruimtetekort(vrij_bytes, grootte_bytes.saturating_sub(aanwezig_bytes))
    };
    ModelOverzicht {
        id: model.id.clone(),
        status,
        grootte_bytes,
        aanwezig_bytes,
        vrij_bytes,
        tekort_bytes,
        bron: bron_host(model),
    }
}

/// De status van een model met zijn hulpmodellen: pas compleet als alles compleet is, en
/// "ontbreekt" alleen als er nergens iets staat.
fn samengestelde_status(statussen: &[ModelStatus]) -> ModelStatus {
    if statussen.iter().all(|s| *s == ModelStatus::Compleet) {
        ModelStatus::Compleet
    } else if statussen.iter().all(|s| *s == ModelStatus::Ontbreekt) {
        ModelStatus::Ontbreekt
    } else {
        ModelStatus::Gedeeltelijk
    }
}

/// Vrije schijfruimte voor de gebruiker op de schijf waar `pad` staat. Bestaat `pad`
/// nog niet (de modelmap wordt pas bij de eerste download gemaakt), dan wordt de
/// dichtstbijzijnde bestaande bovenliggende map gemeten. `None` als het niet lukt.
#[must_use]
pub fn vrije_ruimte(pad: &Path) -> Option<u64> {
    pad.ancestors()
        .find(|map| map.is_dir())
        .and_then(|map| fs4::available_space(map).ok())
}

/// Hoeveel ruimte er te weinig is voor `nog_nodig` bytes plus de marge:
/// [`RUIMTE_MARGE_PERCENTAGE`] procent van wat nog nodig is, maar minstens
/// [`RUIMTE_MARGE_MINIMUM`]. Is de vrije ruimte onbekend, dan is er geen reden om te
/// weigeren: 0.
#[must_use]
pub fn ruimtetekort(vrij: Option<u64>, nog_nodig: u64) -> u64 {
    vrij.map_or(0, |vrij| {
        let marge =
            (nog_nodig.saturating_mul(RUIMTE_MARGE_PERCENTAGE) / 100).max(RUIMTE_MARGE_MINIMUM);
        nog_nodig.saturating_add(marge).saturating_sub(vrij)
    })
}

/// De host uit het adres van het eerste bestand van het model, bijvoorbeeld `huggingface.co`.
/// Leeg als het model geen bestanden heeft of het adres geen host bevat.
#[must_use]
pub fn bron_host(model: &Model) -> String {
    model
        .bestanden
        .first()
        .and_then(|b| host_van_url(&b.url))
        .unwrap_or_default()
        .to_string()
}

/// De host uit een adres als `https://gebruiker@host:443/pad`: zonder schema, gebruiker, poort en pad.
fn host_van_url(url: &str) -> Option<&str> {
    let na_schema = url.split_once("://")?.1;
    let autoriteit = na_schema.split(['/', '?', '#']).next()?;
    let zonder_gebruiker = autoriteit.rsplit('@').next()?;
    let host = zonder_gebruiker.split(':').next()?;
    (!host.is_empty()).then_some(host)
}

/// Overzicht van een model (met Silero) op de vaste plek in de gegevensmap van de gebruiker.
///
/// # Errors
///
/// Geeft een [`ModelFout`] als het model niet in de catalogus staat of de gegevensmap ontbreekt.
pub fn overzicht_van_id(id: &str) -> Result<ModelOverzicht, ModelFout> {
    let model = model::zoek(id)?;
    let map = model::modelmap(&model.id)?;
    Ok(overzicht(&model, &map))
}

/// Staat er iets van dit model op schijf (een compleet bestand of een deel)?
#[must_use]
pub fn staat_op_schijf(model: &Model, modelmap: &Path) -> bool {
    status(model, modelmap) != ModelStatus::Ontbreekt
}

/// Staat dit model helemaal op de computer, met zijn hulpmodellen? Hetzelfde als
/// `overzicht(..).status == Compleet`, maar zonder vrije ruimte te meten of bytes te tellen:
/// dit vraagt de app bij elk dictaat.
#[must_use]
pub fn is_compleet(model: &Model, modelmap: &Path) -> bool {
    doelen_voor(model, modelmap).is_ok_and(|doelen| {
        doelen
            .iter()
            .all(|(m, map)| status(m, map) == ModelStatus::Compleet)
    })
}

/// Verwijdert alles van één model uit zijn map: de bestanden uit de catalogus, de
/// deelbestanden van een afgebroken download en het markeerbestand. Daarna gaat de map weg
/// als ze leeg is. Andere bestanden in die map blijven staan. Geeft het aantal vrijgemaakte
/// bytes. Een model dat niet (meer) op schijf staat is geen fout: het resultaat is 0.
///
/// Het markeerbestand gaat als eerste: lukt daarna een bestand niet, dan geldt het model
/// nooit meer als compleet (het staat er "gedeeltelijk"). Een bestand dat niet weg kan,
/// houdt de rest niet tegen: wat wel weg kan, gaat weg, zodat een tweede poging alleen de
/// rest nog hoeft.
///
/// # Errors
///
/// Geeft [`DownloadFout::NietVerwijderd`] voor het eerste bestand dat niet weg kon,
/// bijvoorbeeld omdat Windows het nog vasthoudt.
pub fn verwijder_model(model: &Model, modelmap: &Path) -> Result<u64, DownloadFout> {
    let mut vrij = 0;
    let mut eerste_fout = None;
    let mut verwijder = |pad: PathBuf| {
        let Ok(meta) = fs::metadata(&pad) else {
            return;
        };
        match fs::remove_file(&pad) {
            Ok(()) => vrij += meta.len(),
            Err(_) => {
                eerste_fout.get_or_insert_with(|| DownloadFout::NietVerwijderd {
                    bestand: pad
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                });
            }
        }
    };
    verwijder(modelmap.join(MARKEERBESTAND));
    verwijder(modelmap.join(format!("{MARKEERBESTAND}.tmp")));
    for bestand in &model.bestanden {
        verwijder(modelmap.join(&bestand.pad));
        verwijder(deelpad(modelmap, bestand));
    }
    if let Some(fout) = eerste_fout {
        return Err(fout);
    }
    // Leeg: weg. Niet leeg (iets van de gebruiker, of van een andere versie): laten staan.
    let _ = fs::remove_dir(modelmap);
    Ok(vrij)
}

/// Heeft een ander spraakmodel dan `model` dit hulpmodel nog nodig? Ja als zo'n model (iets)
/// op de computer heeft staan, of als het in de downloadrij staat (`binnenkomend`: wachtend of
/// lopend): zo'n download heeft misschien nog niets op schijf, maar haalt het hulpmodel straks
/// op of gebruikt het.
fn hulp_nog_nodig(
    catalogus: &[Model],
    model: &Model,
    hulp_id: &str,
    binnenkomend: &[String],
    op_schijf: &dyn Fn(&Model) -> bool,
) -> bool {
    catalogus.iter().any(|m| {
        m.id != model.id
            && m.hulp.iter().any(|h| h == hulp_id)
            && (binnenkomend.contains(&m.id) || op_schijf(m))
    })
}

/// Verwijdert een spraakmodel van de computer, met de hulpmodellen (Silero) die geen ander
/// model nog nodig heeft: niet op de computer, en niet in de downloadrij (`binnenkomend`, de ids
/// van de modellen die wachten of binnenkomen). Geeft het aantal vrijgemaakte bytes.
///
/// # Errors
///
/// Geeft een [`DownloadFout`] als de catalogus niet klopt, de gegevensmap ontbreekt of een
/// bestand niet weg kan.
pub fn verwijder(id: &str, binnenkomend: &[String]) -> Result<u64, DownloadFout> {
    let schijf = |fout: ModelFout| DownloadFout::Schijf(fout.to_string());
    let catalogus = model::catalogus().map_err(schijf)?;
    let model = model::zoek(id).map_err(schijf)?;
    let map = model::modelmap(&model.id).map_err(schijf)?;
    let mut vrij = verwijder_model(&model, &map)?;
    let op_schijf =
        |m: &Model| model::modelmap(&m.id).is_ok_and(|andere| staat_op_schijf(m, &andere));
    for hulp_id in &model.hulp {
        if hulp_nog_nodig(&catalogus, &model, hulp_id, binnenkomend, &op_schijf) {
            continue;
        }
        let hulp = model::zoek(hulp_id).map_err(schijf)?;
        let hulpmap = model::hulpmodelmap_naast(&map, &hulp.id);
        vrij += verwijder_model(&hulp, &hulpmap)?;
    }
    Ok(vrij)
}

/// Aantal bytes van dit model dat nu op schijf staat (gecontroleerde bestanden plus
/// deelbestanden). Handig om te laten zien hoe ver een hervatte download al was.
#[must_use]
pub fn aanwezige_bytes(model: &Model, modelmap: &Path) -> u64 {
    model
        .bestanden
        .iter()
        .map(|b| {
            let lengte = |pad: PathBuf| fs::metadata(pad).map_or(0, |m| m.len());
            let definitief = lengte(modelmap.join(&b.pad));
            if definitief > 0 {
                definitief
            } else {
                lengte(deelpad(modelmap, b))
            }
        })
        .sum()
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut uit, b| {
            let _ = write!(uit, "{b:02x}");
            uit
        })
}

fn sha256_van_bestand(pad: &Path) -> io::Result<String> {
    let mut bestand = File::open(pad)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; BUFFER];
    loop {
        let n = bestand.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Houdt de voortgang bij en meldt die aan de aanroeper.
struct Teller<'a> {
    voortgang: Voortgang,
    terugroep: &'a mut dyn FnMut(Melding),
}

impl Teller<'_> {
    fn tel_op(&mut self, bytes: u64) {
        self.voortgang.binnen += bytes;
        (self.terugroep)(Melding::Voortgang(self.voortgang));
    }

    fn meld(&mut self, melding: Melding) {
        (self.terugroep)(melding);
    }
}

/// Downloadt alle bestanden van een model naar `modelmap` en controleert ze.
///
/// Heeft het model hulpmodellen (Silero, zie [`Model::hulp`]), dan komen die erbij als ze
/// er nog niet staan: naast `modelmap`, binnen dezelfde voortgang en met dezelfde
/// hash-controle en hervatting. Een hulpmodel dat al compleet is, kost niets.
///
/// Bestanden die al gecontroleerd zijn worden overgeslagen; een bestaand `.part`
/// wordt hervat. Zet `annuleer` op `true` om te stoppen; het `.part` blijft dan staan.
///
/// # Errors
///
/// Geeft een [`DownloadFout`] bij netwerkproblemen, schijffouten, een verkeerde
/// controlesom of annuleren.
pub fn download(
    model: &Model,
    modelmap: &Path,
    mut voortgang: impl FnMut(Voortgang),
    annuleer: &Arc<AtomicBool>,
) -> Result<(), DownloadFout> {
    download_met(
        model,
        modelmap,
        &Opties::default(),
        &mut |melding| {
            if let Melding::Voortgang(stand) = melding {
                voortgang(stand);
            }
        },
        annuleer,
    )
}

/// Als [`download`], met eigen [`Opties`] en ook de meldingen over nieuwe pogingen.
///
/// # Errors
///
/// Zie [`download`].
pub fn download_met(
    model: &Model,
    modelmap: &Path,
    opties: &Opties,
    melding: &mut dyn FnMut(Melding),
    annuleer: &Arc<AtomicBool>,
) -> Result<(), DownloadFout> {
    let doelen = doelen_voor(model, modelmap)?;
    download_doelen(&doelen, opties, melding, annuleer)
}

/// Het model zelf eerst, daarna zijn hulpmodellen, elk in een eigen map.
fn doelen_voor(model: &Model, modelmap: &Path) -> Result<Vec<(Model, PathBuf)>, DownloadFout> {
    let mut doelen = vec![(model.clone(), modelmap.to_path_buf())];
    for hulp in model::hulpmodellen_van(model).map_err(|f| DownloadFout::Schijf(f.to_string()))? {
        let map = model::hulpmodelmap_naast(modelmap, &hulp.id);
        doelen.push((hulp, map));
    }
    Ok(doelen)
}

/// Downloadt de bestanden van alle doelen achter elkaar, met één gedeelde voortgang.
fn download_doelen(
    doelen: &[(Model, PathBuf)],
    opties: &Opties,
    voortgang: &mut dyn FnMut(Melding),
    annuleer: &Arc<AtomicBool>,
) -> Result<(), DownloadFout> {
    // `timeout_per_read` geldt per leesactie op de verbinding, voor de kopregels én de inhoud
    // (ureq 3.5, `ConfigBuilder::timeout_per_read`): een server die blijft hangen, valt zo na
    // de stilte weg, waarna een nieuwe poging volgt.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .user_agent("mondig")
        .timeout_connect(Some(opties.verbinden))
        .timeout_per_read(Some(opties.stilte))
        // Certificaten controleren met de opslag van het besturingssysteem, niet met een vaste
        // lijst: bij een organisatie met TLS-inspectie staat haar eigen basiscertificaat daar
        // (ureq 3.5, `RootCerts::PlatformVerifier`, feature `platform-verifier`).
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into();

    let mut teller = Teller {
        voortgang: Voortgang {
            binnen: 0,
            totaal: doelen.iter().map(|(m, _)| m.totale_grootte()).sum(),
        },
        terugroep: voortgang,
    };
    for (doel, map) in doelen {
        fs::create_dir_all(map)?;
        let markering = lees_markering(map);
        for bestand in &doel.bestanden {
            if annuleer.load(Ordering::Relaxed) {
                return Err(DownloadFout::Geannuleerd);
            }
            if is_geverifieerd(&markering, bestand, map) {
                teller.tel_op(bestand.grootte);
                continue;
            }
            download_bestand(&agent, bestand, map, opties, annuleer, &mut teller)?;
        }
    }
    Ok(())
}

impl DownloadFout {
    /// Wat de gebruiker kan doen: een tijdelijk probleem (verbinding, server, schijf) is
    /// "opnieuw proberen" en hervat waar het bleef; een beschadigd bestand moet "opnieuw
    /// gedownload" worden. De soort fout beslist, niet de tekst van de melding.
    #[must_use]
    pub fn vervolg(&self) -> Vervolg {
        match self {
            Self::Netwerk(_)
            | Self::Server { .. }
            | Self::Onvolledig { .. }
            | Self::Schijf(_)
            | Self::SchijfVol
            | Self::OnverwachtAntwoord { .. }
            | Self::AnderBestand { .. }
            | Self::NietVerwijderd { .. } => Vervolg::OpnieuwProberen,
            Self::HashKlopptNiet { .. } | Self::TeGroot { .. } => Vervolg::OpnieuwDownloaden,
            Self::Geannuleerd => Vervolg::Geen,
        }
    }

    /// Melding in gewone taal voor de gebruiker, zonder technische details of paden.
    #[must_use]
    pub fn melding_voor_gebruiker(&self) -> String {
        match self {
            Self::Netwerk(_) | Self::Onvolledig { .. } => {
                "De verbinding viel steeds weg, ook na een paar nieuwe pogingen. Wat al binnen is, blijft bewaard. Probeer het opnieuw als je internet weer werkt."
            }
            Self::Server { .. } => {
                "De downloadserver gaf een foutmelding. Wat al binnen is, blijft bewaard. Probeer het later opnieuw."
            }
            Self::OnverwachtAntwoord { .. } => {
                "De downloadserver stuurde iets anders dan het model. Moet je op dit netwerk eerst inloggen, zoals in een hotel of trein? Wat al binnen is, blijft bewaard. Probeer het daarna opnieuw."
            }
            Self::AnderBestand { .. } => {
                "Het model op de downloadserver is anders dan Mondig verwacht. Wat al binnen is, blijft bewaard. Probeer het later opnieuw; helpt dat niet, werk Mondig dan bij."
            }
            // Dezelfde vaste tekst als elders in Mondig: zo herkent het logboek hem
            // (`Probleem::uit_melding`) zonder vrije tekst op te slaan.
            Self::SchijfVol => Probleem::SchijfVol.melding(),
            Self::Schijf(_) => {
                "Er ging iets mis op de schijf. Controleer of er genoeg ruimte vrij is en probeer het opnieuw."
            }
            Self::HashKlopptNiet { .. } | Self::TeGroot { .. } => {
                "Het gedownloade bestand is beschadigd en is verwijderd. Download het opnieuw."
            }
            Self::Geannuleerd => "De download is gestopt. Je kunt later verdergaan waar het bleef.",
            Self::NietVerwijderd { .. } => {
                "Mondig kon het model niet verwijderen, omdat een ander programma het nog gebruikt. Sluit Mondig af en probeer het opnieuw."
            }
        }
        .to_string()
    }

    /// Is dit een hapering waarna het zin heeft het zelf opnieuw te proberen? Een weggevallen
    /// of stilgevallen verbinding wel, en een server die tijdelijk niet kan (5xx, 408, 429).
    /// Een beschadigd bestand, een volle schijf of een bestand dat niet bestaat (404) niet:
    /// dan helpt nog eens proberen niet.
    #[must_use]
    pub fn is_tijdelijk(&self) -> bool {
        match self {
            Self::Netwerk(_) | Self::Onvolledig { .. } | Self::OnverwachtAntwoord { .. } => true,
            Self::Server { status, .. } => *status >= 500 || *status == 408 || *status == 429,
            _ => false,
        }
    }

    /// De toestand die bij deze uitkomst van een download hoort.
    #[must_use]
    pub fn naar_toestand(&self) -> Toestand {
        match self {
            Self::Geannuleerd => Toestand::Geannuleerd,
            fout => Toestand::fout(fout.melding_voor_gebruiker(), fout.vervolg()),
        }
    }
}

/// Laat hooguit één melding per interval door, zodat de interface niet overspoeld wordt.
#[derive(Debug)]
pub struct Afknijper {
    interval: Duration,
    laatste: Option<Instant>,
}

impl Afknijper {
    /// Een afknijper die hooguit één keer per `interval` doorlaat.
    #[must_use]
    pub fn nieuw(interval: Duration) -> Self {
        Self {
            interval,
            laatste: None,
        }
    }

    /// Mag een melding om tijdstip `nu` door? De eerste altijd; daarna pas als het interval
    /// sinds de laatste doorgelaten melding verstreken is.
    pub fn laat_door(&mut self, nu: Instant) -> bool {
        let door = self
            .laatste
            .is_none_or(|laatste| nu.saturating_duration_since(laatste) >= self.interval);
        if door {
            self.laatste = Some(nu);
        }
        door
    }
}

/// Voert een download uit als [`Taak`] van de soort [`Soort::Download`] en meldt elke
/// toestandswijziging aan `taak_melding`: "verwerkt" met het percentage (alleen als dat
/// verandert), daarna klaar, geannuleerd of fout. De byte-voortgang gaat hooguit
/// vier keer per seconde naar `melding` (de laatste, 100 procent, altijd); een nieuwe poging
/// gaat altijd meteen door.
///
/// Geeft de eindtoestand terug.
pub fn voer_download_uit(
    model: &Model,
    modelmap: &Path,
    id: TaakId,
    annuleer: &Arc<AtomicBool>,
    opties: &Opties,
    mut taak_melding: impl FnMut(Gebeurtenis),
    mut melding: impl FnMut(Melding),
) -> Toestand {
    let mut taak = Taak::nieuw(id, Soort::Download);
    let mut zet = |taak: &mut Taak, toestand: Toestand| {
        // Een verboden overgang kan hier niet voorkomen; zou het toch, dan negeren we hem.
        if let Ok(gebeurtenis) = taak.ga_naar(toestand) {
            taak_melding(gebeurtenis);
        }
    };
    zet(&mut taak, Toestand::verwerkt(Some(0)));

    let mut afknijper = Afknijper::nieuw(VOORTGANG_INTERVAL);
    let mut laatste_procent = 0;
    let uitkomst = download_met(
        model,
        modelmap,
        opties,
        &mut |nieuw| match nieuw {
            Melding::Voortgang(stand) => {
                let procent = stand.procent();
                if procent != laatste_procent {
                    laatste_procent = procent;
                    zet(&mut taak, Toestand::verwerkt(Some(procent)));
                }
                if stand.binnen >= stand.totaal || afknijper.laat_door(Instant::now()) {
                    melding(nieuw);
                }
            }
            Melding::NieuwePoging { .. } => melding(nieuw),
        },
        annuleer,
    );
    let eind = match uitkomst {
        Ok(()) => Toestand::Klaar,
        Err(fout) => fout.naar_toestand(),
    };
    zet(&mut taak, eind.clone());
    eind
}

fn download_bestand(
    agent: &ureq::Agent,
    bestand: &ModelBestand,
    modelmap: &Path,
    opties: &Opties,
    annuleer: &Arc<AtomicBool>,
    teller: &mut Teller<'_>,
) -> Result<(), DownloadFout> {
    let definitief = modelmap.join(&bestand.pad);
    let deelbestand = deelpad(modelmap, bestand);

    // Staat het definitieve bestand er al (bijvoorbeeld na een update van het
    // markeerbestand), dan eerst lokaal controleren in plaats van opnieuw te downloaden.
    if fs::metadata(&definitief).is_ok_and(|m| m.len() == bestand.grootte) {
        if sha256_van_bestand(&definitief)? == bestand.sha256 {
            teller.tel_op(bestand.grootte);
            return markeer_gecontroleerd(modelmap, bestand, &definitief);
        }
        fs::remove_file(&definitief)?;
    }

    haal_op_met_nieuwe_pogingen(agent, bestand, &deelbestand, opties, annuleer, teller)?;

    let lengte = fs::metadata(&deelbestand)?.len();
    if lengte > bestand.grootte {
        fs::remove_file(&deelbestand)?;
        return Err(DownloadFout::TeGroot {
            bestand: bestand.pad.clone(),
        });
    }
    if sha256_van_bestand(&deelbestand)? != bestand.sha256 {
        fs::remove_file(&deelbestand)?;
        return Err(DownloadFout::HashKlopptNiet {
            bestand: bestand.pad.clone(),
        });
    }

    // Lukt hernoemen ook na de nieuwe pogingen niet, dan blijft het gecontroleerde `.part`
    // staan: de volgende keer hoeft het alleen nog gecontroleerd en hernoemd te worden.
    hernoem_met_nieuwe_pogingen(
        &deelbestand,
        &definitief,
        opties.hernoem_pogingen,
        opties.hernoem_pauze,
    )?;
    markeer_gecontroleerd(modelmap, bestand, &definitief)
}

/// Haalt de rest van een bestand op naar het deelbestand, en probeert het zelf opnieuw als de
/// verbinding wegvalt of stilvalt: tot [`Opties::nieuwe_pogingen`] keer, met de wachttijd uit
/// [`Opties::wachttijd`] ertussen, en telkens verder vanaf wat er al binnen is. Daarna is het
/// deelbestand precies even groot als het bestand, of te groot (de aanroeper gooit het dan weg).
fn haal_op_met_nieuwe_pogingen(
    agent: &ureq::Agent,
    bestand: &ModelBestand,
    deelbestand: &Path,
    opties: &Opties,
    annuleer: &Arc<AtomicBool>,
    teller: &mut Teller<'_>,
) -> Result<(), DownloadFout> {
    let lengte = |pad: &Path| fs::metadata(pad).map_or(0, |m| m.len());
    let mut poging = 0;
    let mut verste = lengte(deelbestand);
    loop {
        let mut reeds = lengte(deelbestand);
        if reeds > bestand.grootte {
            fs::remove_file(deelbestand)?;
            reeds = 0;
        }
        if reeds == bestand.grootte {
            // Het deelbestand was al even groot: alleen nog controleren.
            teller.tel_op(reeds);
            return Ok(());
        }
        let voor = teller.voortgang.binnen;
        let fout = match haal_op(agent, bestand, deelbestand, reeds, annuleer, teller) {
            Ok(()) if lengte(deelbestand) >= bestand.grootte => return Ok(()),
            // De server sloot netjes af, maar te vroeg.
            Ok(()) => DownloadFout::Onvolledig {
                bestand: bestand.pad.clone(),
            },
            Err(fout) => fout,
        };
        if !fout.is_tijdelijk() {
            return Err(fout);
        }
        let nu = lengte(deelbestand);
        if nu >= verste.saturating_add(VOORUITGANG_BYTES) {
            poging = 0;
        }
        verste = verste.max(nu);
        if poging >= opties.nieuwe_pogingen {
            return Err(fout);
        }
        poging += 1;
        // De volgende poging telt zelf weer op wat er al binnen is.
        teller.voortgang.binnen = voor;
        teller.meld(Melding::NieuwePoging {
            poging,
            van: opties.nieuwe_pogingen,
        });
        wacht_of_stop((opties.wachttijd)(poging), annuleer)?;
    }
}

/// Wacht `duur`, maar stopt meteen als de gebruiker de download stopt.
fn wacht_of_stop(duur: Duration, annuleer: &AtomicBool) -> Result<(), DownloadFout> {
    let einde = Instant::now() + duur;
    loop {
        if annuleer.load(Ordering::Relaxed) {
            return Err(DownloadFout::Geannuleerd);
        }
        let over = einde.saturating_duration_since(Instant::now());
        if over.is_zero() {
            return Ok(());
        }
        sleep(over.min(Duration::from_millis(50)));
    }
}

/// Legt in het markeerbestand vast dat dit bestand gecontroleerd is.
fn markeer_gecontroleerd(
    modelmap: &Path,
    bestand: &ModelBestand,
    definitief: &Path,
) -> Result<(), DownloadFout> {
    let mut markering = lees_markering(modelmap);
    markering.bestanden.retain(|g| g.pad != bestand.pad);
    markering.bestanden.push(Gecontroleerd {
        pad: bestand.pad.clone(),
        sha256: bestand.sha256.clone(),
        grootte: bestand.grootte,
        gewijzigd: wijzigingstijd(definitief),
    });
    schrijf_markering(modelmap, &markering)
}

/// Haalt (de rest van) een bestand op en schrijft het naar het deelbestand. De
/// aanroeper controleert daarna zelf de lengte en de hash van het deelbestand.
fn haal_op(
    agent: &ureq::Agent,
    bestand: &ModelBestand,
    deelbestand: &Path,
    reeds: u64,
    annuleer: &Arc<AtomicBool>,
    teller: &mut Teller<'_>,
) -> Result<(), DownloadFout> {
    let mut start = reeds;
    let antwoord = loop {
        let mut verzoek = agent
            .get(&bestand.url)
            .header("Accept-Encoding", "identity");
        if start > 0 {
            verzoek = verzoek.header("Range", format!("bytes={start}-"));
        }
        let antwoord = verzoek
            .call()
            .map_err(|e| DownloadFout::Netwerk(e.to_string()))?;
        let ander = || DownloadFout::AnderBestand {
            bestand: bestand.pad.clone(),
        };
        // Zegt de server hoe groot het hele bestand is, dan moet dat kloppen met de catalogus:
        // anders zou een kleiner bestand eindeloos "onvolledig" blijven en opnieuw beginnen.
        if content_range_totaal(&antwoord).is_some_and(|totaal| totaal != bestand.grootte) {
            return Err(ander());
        }
        match antwoord.status().as_u16() {
            // De server gaat verder waar we bleven, maar alleen als `Content-Range` ook
            // echt bij onze startpositie begint. Anders zou het bestand met verkeerde
            // bytes aaneengeplakt worden.
            206 if start > 0 && begint_bij(&antwoord, start) => break antwoord,
            // Een andere range dan gevraagd: opnieuw beginnen vanaf nul.
            206 if start > 0 => {
                start = 0;
            }
            // Een 200 op een Range-verzoek is alleen het hele bestand als de lengte klopt (of,
            // zonder lengte bij chunked, als het geen webpagina is). Een inlogpagina van een
            // wifi-netwerk of een proxy is dat niet: dan blijft het deelbestand onaangeroerd
            // en volgt later een nieuwe poging.
            200 if start > 0 && !is_heel_bestand(&antwoord, bestand.grootte) => {
                return Err(DownloadFout::OnverwachtAntwoord {
                    bestand: bestand.pad.clone(),
                });
            }
            // Zonder Range: een lengte die niet klopt, is niet het model. Een webpagina is
            // waarschijnlijk een inlogpagina (later opnieuw); anders staat er op de server
            // een ander bestand dan de catalogus zegt, en helpt opnieuw proberen niet.
            200 if content_length(&antwoord).is_some_and(|lengte| lengte != bestand.grootte) => {
                return Err(if is_webpagina(&antwoord) {
                    DownloadFout::OnverwachtAntwoord {
                        bestand: bestand.pad.clone(),
                    }
                } else {
                    ander()
                });
            }
            // De server negeert Range of we begonnen al bij nul: opnieuw beginnen.
            200 | 206 => {
                start = 0;
                break antwoord;
            }
            // Ons deelbestand past niet bij het bestand op de server: vers beginnen. (Een
            // ander totaal is hierboven al een fout.)
            416 if start > 0 => {
                fs::remove_file(deelbestand)?;
                start = 0;
            }
            status => {
                return Err(DownloadFout::Server {
                    status,
                    bestand: bestand.pad.clone(),
                });
            }
        }
    };

    let mut uitvoer = if start > 0 {
        OpenOptions::new().append(true).open(deelbestand)?
    } else {
        File::create(deelbestand)?
    };
    teller.tel_op(start);

    // Een byte extra lezen laat een te groot bestand opvallen zonder eindeloos door te lezen.
    let resterend = bestand.grootte - start;
    let mut lezer = antwoord.into_body().into_reader().take(resterend + 1);
    let mut buffer = vec![0_u8; BUFFER];
    loop {
        if annuleer.load(Ordering::Relaxed) {
            uitvoer.flush()?;
            return Err(DownloadFout::Geannuleerd);
        }
        let n = match lezer.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(fout) if fout.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                uitvoer.flush()?;
                return Err(DownloadFout::Onvolledig {
                    bestand: bestand.pad.clone(),
                });
            }
        };
        uitvoer.write_all(&buffer[..n])?;
        teller.tel_op(n as u64);
    }
    uitvoer.flush()?;
    Ok(())
}

/// Begint een 206-antwoord (volgens `Content-Range`) precies bij byte `start`?
fn begint_bij(antwoord: &ureq::http::Response<ureq::Body>, start: u64) -> bool {
    antwoord
        .headers()
        .get("content-range")
        .and_then(|waarde| waarde.to_str().ok())
        .and_then(content_range_begin)
        == Some(start)
}

/// Het totaal uit `Content-Range` (`bytes 0-9/1000` of `bytes */1000`), als de server het noemt.
fn content_range_totaal(antwoord: &ureq::http::Response<ureq::Body>) -> Option<u64> {
    let waarde = antwoord.headers().get("content-range")?.to_str().ok()?;
    let (_, totaal) = waarde.trim().rsplit_once('/')?;
    totaal.trim().parse().ok()
}

/// Kan een 200-antwoord het hele bestand zijn? Ja als `Content-Length` de grootte uit de
/// catalogus is, of als er geen lengte is (chunked) en het geen webpagina is.
fn is_heel_bestand(antwoord: &ureq::http::Response<ureq::Body>, grootte: u64) -> bool {
    match content_length(antwoord) {
        Some(lengte) => lengte == grootte,
        None => !is_webpagina(antwoord),
    }
}

/// Is het antwoord een webpagina (`Content-Type: text/html`)?
fn is_webpagina(antwoord: &ureq::http::Response<ureq::Body>) -> bool {
    antwoord
        .headers()
        .get("content-type")
        .and_then(|waarde| waarde.to_str().ok())
        .is_some_and(|soort| soort.trim().to_ascii_lowercase().starts_with("text/html"))
}

/// De `Content-Length` van een antwoord, als die er is.
fn content_length(antwoord: &ureq::http::Response<ureq::Body>) -> Option<u64> {
    antwoord
        .headers()
        .get("content-length")?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Haalt het beginbyte uit een `Content-Range`-waarde zoals `bytes 400000-999999/1000000`.
fn content_range_begin(waarde: &str) -> Option<u64> {
    let bereik = waarde.trim().strip_prefix("bytes ")?;
    let (begin, _) = bereik.split_once('-')?;
    begin.trim().parse().ok()
}

/// Hernoemt `van` naar `naar`; probeert het een paar keer opnieuw omdat Windows
/// (virusscanner, indexering) een net geschreven bestand soms nog even vasthoudt.
/// Het patroon (een paar keer opnieuw, het `.part` blijft staan als het niet lukt) komt van
/// Vibe (`desktop/src-tauri/src/cmd/download.rs`, `publish_download`, MIT).
fn hernoem_met_nieuwe_pogingen(
    van: &Path,
    naar: &Path,
    pogingen: u32,
    pauze: Duration,
) -> Result<(), DownloadFout> {
    let mut poging = 1;
    loop {
        match fs::rename(van, naar) {
            Ok(()) => return Ok(()),
            Err(fout) if poging >= pogingen => return Err(fout.into()),
            Err(_) => {
                poging += 1;
                sleep(pauze);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Engine, Rol};
    use std::sync::Mutex;
    use std::thread::JoinHandle;

    /// Hoe de testserver met `Range`-verzoeken omgaat.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum RangeGedrag {
        /// Negeert `Range` en stuurt altijd het hele bestand (200).
        Negeert,
        /// Stuurt een correcte 206 met `Content-Range`.
        Correct,
        /// Stuurt een 206 met bytes vanaf nul en een `Content-Range` die bij nul begint.
        Verkeerd,
    }

    /// Kleine HTTP-server voor de tests, met (optioneel) ondersteuning voor `Range`.
    struct TestServer {
        adres: String,
        verzoeken: Arc<Mutex<Vec<Option<String>>>>,
        stop: Arc<AtomicBool>,
        draad: Option<JoinHandle<()>>,
    }

    impl TestServer {
        fn start(inhoud: Vec<u8>, ondersteunt_range: bool) -> Self {
            let gedrag = if ondersteunt_range {
                RangeGedrag::Correct
            } else {
                RangeGedrag::Negeert
            };
            Self::start_met(inhoud, gedrag)
        }

        fn start_met(inhoud: Vec<u8>, gedrag: RangeGedrag) -> Self {
            let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
            let adres = format!("http://{}", server.server_addr().to_ip().unwrap());
            let verzoeken = Arc::new(Mutex::new(Vec::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let (v, s) = (Arc::clone(&verzoeken), Arc::clone(&stop));
            let draad = std::thread::spawn(move || {
                while !s.load(Ordering::Relaxed) {
                    let Ok(Some(verzoek)) = server.recv_timeout(Duration::from_millis(50)) else {
                        continue;
                    };
                    let range = verzoek
                        .headers()
                        .iter()
                        .find(|h| h.field.equiv("Range"))
                        .map(|h| h.value.as_str().to_string());
                    v.lock().unwrap().push(range.clone());
                    let begin = range
                        .as_deref()
                        .and_then(|r| r.strip_prefix("bytes="))
                        .and_then(|r| r.strip_suffix('-'))
                        .and_then(|r| r.parse::<usize>().ok());
                    let antwoord = match begin {
                        Some(begin) if gedrag != RangeGedrag::Negeert => {
                            // Bij `Verkeerd` zijn de bytes ook echt van het begin van het bestand.
                            let (opgegeven_begin, deelbestand) = if gedrag == RangeGedrag::Verkeerd
                            {
                                (0, inhoud[..inhoud.len() - begin].to_vec())
                            } else {
                                (begin, inhoud[begin..].to_vec())
                            };
                            let header = tiny_http::Header::from_bytes(
                                "Content-Range",
                                format!(
                                    "bytes {opgegeven_begin}-{}/{}",
                                    inhoud.len() - 1,
                                    inhoud.len()
                                ),
                            )
                            .unwrap();
                            tiny_http::Response::from_data(deelbestand)
                                .with_status_code(206)
                                .with_header(header)
                        }
                        _ => tiny_http::Response::from_data(inhoud.clone()),
                    };
                    let _ = verzoek.respond(antwoord);
                }
            });
            Self {
                adres,
                verzoeken,
                stop,
                draad: Some(draad),
            }
        }

        fn bereiken(&self) -> Vec<Option<String>> {
            self.verzoeken.lock().unwrap().clone()
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(draad) = self.draad.take() {
                let _ = draad.join();
            }
        }
    }

    /// Wat de ruwe testserver bij één verzoek doet.
    #[derive(Clone, Copy, Debug)]
    enum Antwoord {
        /// Het (rest)bestand, met een 206 als er een `Range` gevraagd is.
        Heel,
        /// Kopregels voor het hele (rest)bestand, maar na zoveel bytes gaat de verbinding dicht.
        Breek(usize),
        /// Kopregels en zoveel bytes, en daarna niets meer: een vastgelopen verbinding.
        Stil(usize),
        /// Negeert `Range` en stuurt een 200 met het hele bestand.
        NegeerRange,
        /// Een inlogpagina van een wifi-netwerk: een 200 met een stukje HTML, wat er ook gevraagd is.
        Portaal,
    }

    /// Een kale HTTP-server op `std::net::TcpListener`, die een verbinding halverwege kan
    /// verbreken of kan laten stilvallen. Per verzoek volgt hij het script; daarna `Heel`.
    struct RuweServer {
        adres: String,
        verzoeken: Arc<Mutex<Vec<Option<String>>>>,
        stop: Arc<AtomicBool>,
        draad: Option<JoinHandle<()>>,
    }

    impl RuweServer {
        fn start(inhoud: Vec<u8>, script: Vec<Antwoord>) -> Self {
            use std::net::TcpListener;
            let luisteraar = TcpListener::bind("127.0.0.1:0").unwrap();
            luisteraar.set_nonblocking(true).unwrap();
            let adres = format!("http://{}", luisteraar.local_addr().unwrap());
            let verzoeken = Arc::new(Mutex::new(Vec::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let (v, s) = (Arc::clone(&verzoeken), Arc::clone(&stop));
            let inhoud = Arc::new(inhoud);
            let draad = std::thread::spawn(move || {
                let mut volgnummer = 0;
                while !s.load(Ordering::Relaxed) {
                    let Ok((verbinding, _)) = luisteraar.accept() else {
                        sleep(Duration::from_millis(5));
                        continue;
                    };
                    let antwoord = script.get(volgnummer).copied().unwrap_or(Antwoord::Heel);
                    volgnummer += 1;
                    let (v, s, inhoud) = (Arc::clone(&v), Arc::clone(&s), Arc::clone(&inhoud));
                    // Elke verbinding op een eigen draad: een stille verbinding houdt de
                    // volgende niet tegen.
                    std::thread::spawn(move || {
                        let _ = beantwoord(verbinding, &inhoud, antwoord, &v, &s);
                    });
                }
            });
            Self {
                adres,
                verzoeken,
                stop,
                draad: Some(draad),
            }
        }

        fn bereiken(&self) -> Vec<Option<String>> {
            self.verzoeken.lock().unwrap().clone()
        }
    }

    impl Drop for RuweServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(draad) = self.draad.take() {
                let _ = draad.join();
            }
        }
    }

    fn beantwoord(
        mut verbinding: std::net::TcpStream,
        inhoud: &[u8],
        antwoord: Antwoord,
        verzoeken: &Mutex<Vec<Option<String>>>,
        stop: &AtomicBool,
    ) -> io::Result<()> {
        verbinding.set_nonblocking(false)?;
        let mut kop = Vec::new();
        let mut byte = [0_u8; 1];
        while !kop.ends_with(b"\r\n\r\n") {
            if verbinding.read(&mut byte)? == 0 {
                return Ok(());
            }
            kop.push(byte[0]);
        }
        let kop = String::from_utf8_lossy(&kop).to_string();
        let range = kop.lines().find_map(|regel| {
            let (naam, waarde) = regel.split_once(':')?;
            naam.eq_ignore_ascii_case("range")
                .then(|| waarde.trim().to_string())
        });
        verzoeken.lock().unwrap().push(range.clone());
        if matches!(antwoord, Antwoord::Portaal) {
            let html = b"<html><body>Log eerst in op het gastennetwerk</body></html>";
            write!(
                verbinding,
                "HTTP/1.1 200 OK
Content-Type: text/html
Content-Length: {}
Connection: close

",
                html.len()
            )?;
            verbinding.write_all(html)?;
            return verbinding.flush();
        }
        let begin = range
            .as_deref()
            .and_then(|r| r.strip_prefix("bytes="))
            .and_then(|r| r.strip_suffix('-'))
            .and_then(|r| r.parse::<usize>().ok())
            .filter(|_| !matches!(antwoord, Antwoord::NegeerRange));
        let (status, extra, lichaam) = match begin {
            Some(begin) => (
                "206 Partial Content",
                format!(
                    "Content-Range: bytes {begin}-{}/{}\r\n",
                    inhoud.len() - 1,
                    inhoud.len()
                ),
                &inhoud[begin..],
            ),
            None => ("200 OK", String::new(), inhoud),
        };
        write!(
            verbinding,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
            lichaam.len()
        )?;
        match antwoord {
            Antwoord::Heel | Antwoord::NegeerRange | Antwoord::Portaal => {
                verbinding.write_all(lichaam)?;
            }
            Antwoord::Breek(n) => verbinding.write_all(&lichaam[..n.min(lichaam.len())])?,
            Antwoord::Stil(n) => {
                verbinding.write_all(&lichaam[..n.min(lichaam.len())])?;
                verbinding.flush()?;
                // Niets meer sturen tot de test klaar is (of hooguit vijf seconden).
                let tot = Instant::now() + Duration::from_secs(5);
                while !stop.load(Ordering::Relaxed) && Instant::now() < tot {
                    sleep(Duration::from_millis(20));
                }
            }
        }
        verbinding.flush()
    }

    fn model_op_adres(adres: &str, data: &[u8]) -> Model {
        let mut model = model_voor(&TestServer::start(Vec::new(), true), data);
        model.bestanden[0].url = format!("{adres}/a.bin");
        model
    }

    /// Uitkomst van een snelle download: het resultaat, de nieuwe pogingen en de voortgang.
    type SnelleUitkomst = (Result<(), DownloadFout>, Vec<(u32, u32)>, Vec<Voortgang>);

    /// Downloadt met korte tijden en geeft de nieuwe pogingen en de voortgang terug.
    fn download_snel(model: &Model, map: &Path, annuleer: &Arc<AtomicBool>) -> SnelleUitkomst {
        let mut pogingen = Vec::new();
        let mut stand = Vec::new();
        let uitkomst = download_met(
            model,
            map,
            &snel(),
            &mut |m| match m {
                Melding::Voortgang(v) => stand.push(v),
                Melding::NieuwePoging { poging, van } => pogingen.push((poging, van)),
            },
            annuleer,
        );
        (uitkomst, pogingen, stand)
    }

    #[test]
    fn een_verbroken_verbinding_hervat_vanzelf_met_de_juiste_inhoud() {
        let data = testdata(1_000_000);
        let server = RuweServer::start(
            data.clone(),
            vec![Antwoord::Breek(300_000), Antwoord::Breek(200_000)],
        );
        let model = model_op_adres(&server.adres, &data);
        let map = tempfile::tempdir().unwrap();
        let (uitkomst, pogingen, stand) = download_snel(&model, map.path(), &geen_annulering());
        uitkomst.unwrap();
        assert_eq!(fs::read(map.path().join("a.bin")).unwrap(), data);
        assert_eq!(status(&model, map.path()), ModelStatus::Compleet);
        assert_eq!(
            server.bereiken(),
            vec![
                None,
                Some("bytes=300000-".to_string()),
                Some("bytes=500000-".to_string())
            ]
        );
        assert_eq!(pogingen, vec![(1, NIEUWE_POGINGEN), (2, NIEUWE_POGINGEN)]);
        // De voortgang telt niets dubbel na een nieuwe poging.
        assert!(stand.iter().all(|v| v.binnen <= v.totaal), "{stand:?}");
        assert_eq!(stand.last().map(|v| v.binnen), Some(1_000_000));
    }

    #[test]
    fn een_stilgevallen_verbinding_krijgt_een_nieuwe_poging() {
        let data = testdata(500_000);
        let server = RuweServer::start(data.clone(), vec![Antwoord::Stil(100_000)]);
        let model = model_op_adres(&server.adres, &data);
        let map = tempfile::tempdir().unwrap();
        let begin = Instant::now();
        let (uitkomst, pogingen, _) = download_snel(&model, map.path(), &geen_annulering());
        uitkomst.unwrap();
        assert_eq!(fs::read(map.path().join("a.bin")).unwrap(), data);
        assert_eq!(pogingen, vec![(1, NIEUWE_POGINGEN)]);
        let verzoeken = server.bereiken();
        assert_eq!(verzoeken.len(), 2, "{verzoeken:?}");
        assert!(
            verzoeken[1]
                .as_deref()
                .is_some_and(|r| r.starts_with("bytes=") && r != "bytes=0-"),
            "de tweede poging hervat: {verzoeken:?}"
        );
        // Ruim binnen de vijf seconden dat de server stil blijft: de stilte gaf de doorslag.
        assert!(
            begin.elapsed() < Duration::from_secs(4),
            "{:?}",
            begin.elapsed()
        );
    }

    #[test]
    fn een_server_die_range_negeert_na_een_breuk_geeft_geen_kapot_bestand() {
        let data = testdata(800_000);
        let server = RuweServer::start(
            data.clone(),
            vec![Antwoord::Breek(300_000), Antwoord::NegeerRange],
        );
        let model = model_op_adres(&server.adres, &data);
        let map = tempfile::tempdir().unwrap();
        let (uitkomst, pogingen, _) = download_snel(&model, map.path(), &geen_annulering());
        uitkomst.unwrap();
        // Zonder de controle stond hier 300 kB plus het hele bestand aan elkaar.
        assert_eq!(fs::read(map.path().join("a.bin")).unwrap(), data);
        assert_eq!(
            server.bereiken(),
            vec![None, Some("bytes=300000-".to_string())]
        );
        assert_eq!(pogingen.len(), 1);
    }

    #[test]
    fn na_vijf_nieuwe_pogingen_volgt_een_fout_en_het_deelbestand_blijft() {
        let data = testdata(500_000);
        let server = RuweServer::start(data.clone(), vec![Antwoord::Breek(1_000); 10]);
        let model = model_op_adres(&server.adres, &data);
        let map = tempfile::tempdir().unwrap();
        let (uitkomst, pogingen, _) = download_snel(&model, map.path(), &geen_annulering());
        let fout = uitkomst.unwrap_err();
        assert!(fout.is_tijdelijk(), "{fout:?}");
        assert_eq!(fout.vervolg(), Vervolg::OpnieuwProberen);
        // Eén keer, en daarna vijf nieuwe pogingen: de download eindigt (Handy #2100).
        assert_eq!(server.bereiken().len(), 1 + NIEUWE_POGINGEN as usize);
        assert_eq!(
            pogingen,
            (1..=NIEUWE_POGINGEN)
                .map(|p| (p, NIEUWE_POGINGEN))
                .collect::<Vec<_>>()
        );
        assert!(!map.path().join("a.bin").exists());
        assert_eq!(
            fs::metadata(map.path().join("a.bin.part")).unwrap().len(),
            6_000
        );
        // Een half bestand blokkeert niets (Handy #858): de volgende keer gaat het verder.
        drop(server);
        let server = RuweServer::start(data.clone(), Vec::new());
        let model = model_op_adres(&server.adres, &data);
        download_snel(&model, map.path(), &geen_annulering())
            .0
            .unwrap();
        assert_eq!(fs::read(map.path().join("a.bin")).unwrap(), data);
        assert_eq!(server.bereiken(), vec![Some("bytes=6000-".to_string())]);
    }

    #[test]
    fn een_inlogpagina_op_een_range_verzoek_laat_het_deelbestand_heel() {
        let data = testdata(500_000);
        let server = RuweServer::start(data.clone(), vec![Antwoord::Portaal; 10]);
        let model = model_op_adres(&server.adres, &data);
        let map = tempfile::tempdir().unwrap();
        fs::write(map.path().join("a.bin.part"), &data[..300_000]).unwrap();
        let (uitkomst, pogingen, _) = download_snel(&model, map.path(), &geen_annulering());
        let fout = uitkomst.unwrap_err();
        assert!(
            matches!(fout, DownloadFout::OnverwachtAntwoord { .. }),
            "{fout:?}"
        );
        // Een hapering: de gewone nieuwe pogingen, en het deelbestand blijft precies zoals het was.
        assert_eq!(pogingen.len(), NIEUWE_POGINGEN as usize);
        assert_eq!(
            fs::read(map.path().join("a.bin.part")).unwrap(),
            &data[..300_000]
        );
        // Is het netwerk weer gewoon, dan gaat het verder bij 300 kB.
        drop(server);
        let server = RuweServer::start(data.clone(), Vec::new());
        let model = model_op_adres(&server.adres, &data);
        download_snel(&model, map.path(), &geen_annulering())
            .0
            .unwrap();
        assert_eq!(fs::read(map.path().join("a.bin")).unwrap(), data);
        assert_eq!(server.bereiken(), vec![Some("bytes=300000-".to_string())]);
    }

    #[test]
    fn een_ander_bestand_op_de_server_geeft_meteen_een_fout() {
        // De server heeft een kleiner bestand dan de catalogus zegt.
        let data = testdata(500_000);
        let server = RuweServer::start(data[..400_000].to_vec(), Vec::new());
        let model = model_op_adres(&server.adres, &data);

        // Zonder deelbestand: de lengte klopt niet, dus geen enkele byte en geen nieuwe poging.
        let leeg = tempfile::tempdir().unwrap();
        let (uitkomst, pogingen, _) = download_snel(&model, leeg.path(), &geen_annulering());
        let fout = uitkomst.unwrap_err();
        assert!(
            matches!(fout, DownloadFout::AnderBestand { .. }),
            "{fout:?}"
        );
        assert!(!fout.is_tijdelijk());
        assert!(pogingen.is_empty());
        assert!(!leeg.path().join("a.bin.part").exists());

        // Met een deelbestand: het totaal in Content-Range klopt niet, het deelbestand blijft.
        let map = tempfile::tempdir().unwrap();
        fs::write(map.path().join("a.bin.part"), &data[..100_000]).unwrap();
        let (uitkomst, pogingen, _) = download_snel(&model, map.path(), &geen_annulering());
        assert!(
            matches!(uitkomst, Err(DownloadFout::AnderBestand { .. })),
            "{uitkomst:?}"
        );
        assert!(pogingen.is_empty());
        assert_eq!(
            fs::metadata(map.path().join("a.bin.part")).unwrap().len(),
            100_000
        );
        assert_eq!(server.bereiken().len(), 2);
    }

    #[test]
    fn stoppen_tijdens_het_wachten_op_een_nieuwe_poging_stopt_meteen() {
        let data = testdata(200_000);
        let server = RuweServer::start(data.clone(), vec![Antwoord::Breek(1_000)]);
        let model = model_op_adres(&server.adres, &data);
        let map = tempfile::tempdir().unwrap();
        let annuleer = geen_annulering();
        let vlag = Arc::clone(&annuleer);
        let mut opties = snel();
        opties.wachttijd = |_| Duration::from_secs(30);
        let begin = Instant::now();
        let fout = download_met(
            &model,
            map.path(),
            &opties,
            &mut |m| {
                if matches!(m, Melding::NieuwePoging { .. }) {
                    vlag.store(true, Ordering::Relaxed);
                }
            },
            &annuleer,
        )
        .unwrap_err();
        assert!(matches!(fout, DownloadFout::Geannuleerd), "{fout:?}");
        assert!(begin.elapsed() < Duration::from_secs(5));
        assert!(map.path().join("a.bin.part").exists());
    }

    #[test]
    fn alleen_een_hapering_krijgt_een_nieuwe_poging() {
        let server = |status| DownloadFout::Server {
            status,
            bestand: String::new(),
        };
        assert!(!server(404).is_tijdelijk());
        assert!(server(503).is_tijdelijk());
        assert!(server(429).is_tijdelijk());
        assert!(
            !DownloadFout::HashKlopptNiet {
                bestand: String::new()
            }
            .is_tijdelijk()
        );
        assert!(!DownloadFout::Schijf(String::new()).is_tijdelijk());
        assert!(!DownloadFout::Geannuleerd.is_tijdelijk());
    }

    #[test]
    fn de_wachttijd_loopt_op_tot_een_halve_minuut() {
        let tijden: Vec<u64> = (1..=6).map(|p| standaard_wachttijd(p).as_secs()).collect();
        assert_eq!(tijden, vec![2, 4, 8, 16, 30, 30]);
    }

    #[test]
    fn hernoemen_wacht_tot_het_doel_wordt_losgelaten() {
        // Een map op de plek van het doel houdt het hernoemen tegen, zoals een bestand dat
        // Windows (virusscanner, indexering) nog even vasthoudt.
        let map = tempfile::tempdir().unwrap();
        let van = map.path().join("a.bin.part");
        let naar = map.path().join("a.bin");
        fs::write(&van, b"inhoud").unwrap();
        fs::create_dir(&naar).unwrap();
        let weg = naar.clone();
        let loslaten = std::thread::spawn(move || {
            sleep(Duration::from_millis(150));
            fs::remove_dir(weg).unwrap();
        });
        hernoem_met_nieuwe_pogingen(&van, &naar, 50, Duration::from_millis(20)).unwrap();
        loslaten.join().unwrap();
        assert_eq!(fs::read(&naar).unwrap(), b"inhoud");
        assert!(!van.exists());

        // Blijft het doel vast, dan een fout, en het deelbestand blijft staan.
        let van = map.path().join("b.bin.part");
        let naar = map.path().join("b.bin");
        fs::write(&van, b"inhoud").unwrap();
        fs::create_dir(&naar).unwrap();
        let fout = hernoem_met_nieuwe_pogingen(&van, &naar, 3, Duration::from_millis(5));
        assert!(matches!(fout, Err(DownloadFout::Schijf(_))), "{fout:?}");
        assert!(van.exists());
    }

    /// Voorspelbare "willekeurige" bytes.
    fn testdata(lengte: usize) -> Vec<u8> {
        let mut x: u32 = 12345;
        (0..lengte)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (x >> 24) as u8
            })
            .collect()
    }

    fn model_voor(server: &TestServer, data: &[u8]) -> Model {
        Model {
            id: "test".into(),
            naam: "Testmodel".into(),
            engine: Engine::Whisper,
            rol: Rol::Spraak,
            soort: crate::model::Soort::Spraak,
            nauwkeurigheid: Some(3),
            snelheid: Some(3),
            licentie: "MIT".into(),
            talen: vec!["nl".into()],
            min_geheugen_mb: 1,
            hulp: Vec::new(),
            labels: std::collections::BTreeMap::new(),
            niet_voor: Vec::new(),
            bestanden: vec![ModelBestand {
                pad: "a.bin".into(),
                url: format!("{}/a.bin", server.adres),
                sha256: hex(&Sha256::digest(data)),
                grootte: data.len() as u64,
            }],
        }
    }

    /// Korte stilte en wachttijden, zodat de tests met haperingen snel zijn.
    fn snel() -> Opties {
        Opties {
            verbinden: Duration::from_secs(2),
            stilte: Duration::from_millis(400),
            nieuwe_pogingen: NIEUWE_POGINGEN,
            wachttijd: |_| Duration::from_millis(10),
            hernoem_pogingen: HERNOEM_POGINGEN,
            hernoem_pauze: Duration::from_millis(10),
        }
    }

    /// Zoals [`snel`], maar zonder nieuwe pogingen: een geweigerde verbinding duurt op Windows
    /// een paar seconden per keer.
    fn zonder_pogingen() -> Opties {
        Opties {
            nieuwe_pogingen: 0,
            ..snel()
        }
    }

    fn geen_annulering() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn veranderd_bestand_met_gelijke_grootte_telt_niet_meer_als_gecontroleerd() {
        let data = testdata(10_000);
        let server = TestServer::start(data.clone(), true);
        let model = model_voor(&server, &data);
        let map = tempfile::tempdir().unwrap();
        download(&model, map.path(), |_| {}, &geen_annulering()).unwrap();
        assert_eq!(status(&model, map.path()), ModelStatus::Compleet);

        // Doe alsof het bestand na de controle veranderd is: andere wijzigingstijd in de
        // markering, zelfde grootte op schijf.
        let mut markering = lees_markering(map.path());
        for g in &mut markering.bestanden {
            g.gewijzigd = g.gewijzigd.map(|t| t + 1);
        }
        schrijf_markering(map.path(), &markering).unwrap();
        assert_ne!(status(&model, map.path()), ModelStatus::Compleet);

        // De inhoud is in werkelijkheid ongewijzigd: opnieuw "downloaden" hasht het
        // bestaande bestand lokaal en haalt niets op.
        let aantal = server.bereiken().len();
        download(&model, map.path(), |_| {}, &geen_annulering()).unwrap();
        assert_eq!(server.bereiken().len(), aantal);
        assert_eq!(status(&model, map.path()), ModelStatus::Compleet);

        // Nu echt een ander bestand van dezelfde grootte: dat wordt opnieuw opgehaald.
        let mut anders = data.clone();
        anders[0] ^= 0xFF;
        fs::write(map.path().join("a.bin"), &anders).unwrap();
        assert_ne!(status(&model, map.path()), ModelStatus::Compleet);
        download(&model, map.path(), |_| {}, &geen_annulering()).unwrap();
        assert!(server.bereiken().len() > aantal);
        assert_eq!(fs::read(map.path().join("a.bin")).unwrap(), data);
        assert_eq!(status(&model, map.path()), ModelStatus::Compleet);
    }

    #[test]
    fn volledige_download_slaagt_en_status_wordt_compleet() {
        let data = testdata(1_000_000);
        let server = TestServer::start(data.clone(), true);
        let model = model_voor(&server, &data);
        let map = tempfile::tempdir().unwrap();

        assert_eq!(status(&model, map.path()), ModelStatus::Ontbreekt);
        let mut laatste = None;
        download(
            &model,
            map.path(),
            |v| laatste = Some(v),
            &geen_annulering(),
        )
        .unwrap();

        assert_eq!(fs::read(map.path().join("a.bin")).unwrap(), data);
        assert!(!map.path().join("a.bin.part").exists());
        assert_eq!(status(&model, map.path()), ModelStatus::Compleet);
        let laatste = laatste.unwrap();
        assert_eq!(laatste.binnen, laatste.totaal);
        assert_eq!(laatste.procent(), 100);
        assert_eq!(aanwezige_bytes(&model, map.path()), data.len() as u64);

        // Een tweede keer downloaden doet niets: geen nieuw verzoek.
        let aantal = server.bereiken().len();
        download(&model, map.path(), |_| {}, &geen_annulering()).unwrap();
        assert_eq!(server.bereiken().len(), aantal);
    }

    #[test]
    fn model_met_hulpmodel_krijgt_twee_doelen_naast_elkaar() {
        let parakeet = model::zoek(model::STANDAARD_MODEL).unwrap();
        let map = model::modelmap(&parakeet.id).unwrap();
        let doelen = doelen_voor(&parakeet, &map).unwrap();
        assert_eq!(doelen.len(), 2);
        assert_eq!(doelen[0].0.id, parakeet.id);
        assert_eq!(doelen[1].0.id, model::SILERO_MODEL);
        assert_eq!(doelen[1].1, model::modelmap(model::SILERO_MODEL).unwrap());
        // Silero zelf brengt niets mee.
        let silero = model::zoek(model::SILERO_MODEL).unwrap();
        assert_eq!(doelen_voor(&silero, &doelen[1].1).unwrap().len(), 1);
    }

    #[test]
    fn hulpmodel_komt_mee_in_dezelfde_voortgang_en_wordt_niet_dubbel_gehaald() {
        let hoofd_data = testdata(300_000);
        let hulp_data = testdata(50_000);
        let hoofd_server = TestServer::start(hoofd_data.clone(), true);
        let hulp_server = TestServer::start(hulp_data.clone(), true);
        let hoofd = model_voor(&hoofd_server, &hoofd_data);
        let mut hulp = model_voor(&hulp_server, &hulp_data);
        hulp.id = "hulp".into();
        hulp.rol = Rol::Vad;
        hulp.talen.clear();
        let basis = tempfile::tempdir().unwrap();
        let doelen = vec![
            (hoofd.clone(), basis.path().join("test")),
            (hulp.clone(), basis.path().join("hulp")),
        ];

        let mut stand = Vec::new();
        download_doelen(
            &doelen,
            &snel(),
            &mut |m| {
                if let Melding::Voortgang(v) = m {
                    stand.push(v);
                }
            },
            &geen_annulering(),
        )
        .unwrap();

        // Eén voortgang over beide bestanden samen, eindigend op 100 procent.
        let totaal = (hoofd_data.len() + hulp_data.len()) as u64;
        assert!(stand.iter().all(|v| v.totaal == totaal));
        let laatste = stand.last().unwrap();
        assert_eq!(laatste.binnen, totaal);
        assert_eq!(laatste.procent(), 100);
        assert_eq!(
            fs::read(basis.path().join("hulp").join("a.bin")).unwrap(),
            hulp_data
        );
        assert_eq!(
            status(&hulp, &basis.path().join("hulp")),
            ModelStatus::Compleet
        );

        // Opnieuw: alles staat er, dus geen nieuwe verzoeken bij een van beide servers.
        let (n_hoofd, n_hulp) = (hoofd_server.bereiken().len(), hulp_server.bereiken().len());
        download_doelen(&doelen, &snel(), &mut |_| {}, &geen_annulering()).unwrap();
        assert_eq!(hoofd_server.bereiken().len(), n_hoofd);
        assert_eq!(hulp_server.bereiken().len(), n_hulp);
    }

    #[test]
    fn hulpmodel_hervat_na_een_half_bestand_en_controleert_de_hash() {
        let hulp_data = testdata(200_000);
        let hulp_server = TestServer::start(hulp_data.clone(), true);
        let mut hulp = model_voor(&hulp_server, &hulp_data);
        hulp.id = "hulp".into();
        let basis = tempfile::tempdir().unwrap();
        let map = basis.path().join("hulp");
        fs::create_dir_all(&map).unwrap();
        fs::write(map.join("a.bin.part"), &hulp_data[..80_000]).unwrap();
        let doelen = vec![(hulp.clone(), map.clone())];
        download_doelen(&doelen, &snel(), &mut |_| {}, &geen_annulering()).unwrap();
        assert_eq!(fs::read(map.join("a.bin")).unwrap(), hulp_data);
        assert_eq!(
            hulp_server.bereiken(),
            vec![Some("bytes=80000-".to_string())]
        );

        // Met een verkeerde hash blijft er geen definitief bestand achter.
        let mut slecht = hulp;
        slecht.bestanden[0].sha256 = "0".repeat(64);
        let andere = basis.path().join("slecht");
        let fout = download_doelen(
            &[(slecht, andere.clone())],
            &snel(),
            &mut |_| {},
            &geen_annulering(),
        )
        .unwrap_err();
        assert!(
            matches!(fout, DownloadFout::HashKlopptNiet { .. }),
            "{fout:?}"
        );
        assert!(!andere.join("a.bin").exists());
    }

    #[test]
    fn download_hervat_na_een_half_bestand() {
        let data = testdata(1_000_000);
        let server = TestServer::start(data.clone(), true);
        let model = model_voor(&server, &data);
        let map = tempfile::tempdir().unwrap();
        fs::write(map.path().join("a.bin.part"), &data[..400_000]).unwrap();
        assert_eq!(status(&model, map.path()), ModelStatus::Gedeeltelijk);

        let mut eerste = None;
        download(
            &model,
            map.path(),
            |v| {
                eerste.get_or_insert(v);
            },
            &geen_annulering(),
        )
        .unwrap();

        assert_eq!(fs::read(map.path().join("a.bin")).unwrap(), data);
        assert_eq!(server.bereiken(), vec![Some("bytes=400000-".to_string())]);
        // De voortgang begon bij wat er al was, niet bij nul.
        assert!(eerste.unwrap().binnen >= 400_000);
    }

    #[test]
    fn download_begint_opnieuw_als_de_server_geen_range_kent() {
        let data = testdata(500_000);
        let server = TestServer::start(data.clone(), false);
        let model = model_voor(&server, &data);
        let map = tempfile::tempdir().unwrap();
        fs::write(map.path().join("a.bin.part"), &data[..100_000]).unwrap();

        download(&model, map.path(), |_| {}, &geen_annulering()).unwrap();
        assert_eq!(fs::read(map.path().join("a.bin")).unwrap(), data);
        assert_eq!(server.bereiken(), vec![Some("bytes=100000-".to_string())]);
    }

    #[test]
    fn download_begint_opnieuw_bij_een_verkeerde_content_range() {
        let data = testdata(500_000);
        let server = TestServer::start_met(data.clone(), RangeGedrag::Verkeerd);
        let model = model_voor(&server, &data);
        let map = tempfile::tempdir().unwrap();
        fs::write(map.path().join("a.bin.part"), &data[..100_000]).unwrap();

        download(&model, map.path(), |_| {}, &geen_annulering()).unwrap();
        // Zonder de controle was het deelbestand met de verkeerde bytes aaneengeplakt.
        assert_eq!(fs::read(map.path().join("a.bin")).unwrap(), data);
        assert_eq!(
            server.bereiken(),
            vec![Some("bytes=100000-".to_string()), None],
            "eerst Range, daarna opnieuw zonder Range"
        );
    }

    #[test]
    fn content_range_begin_leest_het_beginbyte() {
        assert_eq!(
            content_range_begin("bytes 400000-999999/1000000"),
            Some(400_000)
        );
        assert_eq!(content_range_begin("bytes 0-9/*"), Some(0));
        assert_eq!(content_range_begin("bytes */1000"), None);
        assert_eq!(content_range_begin("items 5-9/10"), None);
        assert_eq!(content_range_begin(""), None);
    }

    #[test]
    fn verkeerde_hash_laat_geen_definitief_bestand_achter() {
        let data = testdata(300_000);
        let server = TestServer::start(data.clone(), true);
        let mut model = model_voor(&server, &data);
        model.bestanden[0].sha256 = "0".repeat(64);
        let map = tempfile::tempdir().unwrap();

        let fout = download(&model, map.path(), |_| {}, &geen_annulering()).unwrap_err();
        assert!(
            matches!(fout, DownloadFout::HashKlopptNiet { .. }),
            "{fout:?}"
        );
        assert!(!map.path().join("a.bin").exists());
        assert!(!map.path().join("a.bin.part").exists());
        assert_eq!(status(&model, map.path()), ModelStatus::Ontbreekt);
        // Een beschadigd bestand krijgt geen nieuwe poging: één verzoek.
        assert_eq!(server.bereiken().len(), 1);
    }

    #[test]
    fn annuleren_laat_het_deelbestand_staan_en_hervatten_werkt() {
        let data = testdata(2_000_000);
        let server = TestServer::start(data.clone(), true);
        let model = model_voor(&server, &data);
        let map = tempfile::tempdir().unwrap();

        let annuleer = geen_annulering();
        let vlag = Arc::clone(&annuleer);
        let fout = download(
            &model,
            map.path(),
            |v| {
                if v.binnen > 0 {
                    vlag.store(true, Ordering::Relaxed);
                }
            },
            &annuleer,
        )
        .unwrap_err();
        assert!(matches!(fout, DownloadFout::Geannuleerd), "{fout:?}");
        assert!(!map.path().join("a.bin").exists());
        let deelbestand = fs::metadata(map.path().join("a.bin.part")).unwrap().len();
        assert!(
            deelbestand > 0 && deelbestand < data.len() as u64,
            "deelbestand = {deelbestand}"
        );
        assert_eq!(status(&model, map.path()), ModelStatus::Gedeeltelijk);

        // Opnieuw starten gaat verder met een Range-verzoek.
        download(&model, map.path(), |_| {}, &geen_annulering()).unwrap();
        assert_eq!(fs::read(map.path().join("a.bin")).unwrap(), data);
        let verzoeken = server.bereiken();
        assert_eq!(
            verzoeken.last().unwrap(),
            &Some(format!("bytes={deelbestand}-"))
        );
        assert_eq!(status(&model, map.path()), ModelStatus::Compleet);
    }

    #[test]
    fn status_ziet_dat_een_bestand_verdwenen_of_veranderd_is() {
        let data = testdata(200_000);
        let server = TestServer::start(data.clone(), true);
        let model = model_voor(&server, &data);
        let map = tempfile::tempdir().unwrap();
        download(&model, map.path(), |_| {}, &geen_annulering()).unwrap();
        assert_eq!(status(&model, map.path()), ModelStatus::Compleet);

        fs::write(map.path().join("a.bin"), b"te kort").unwrap();
        assert_eq!(status(&model, map.path()), ModelStatus::Gedeeltelijk);
        // Alleen het markeerbestand weg, het bestand zelf is er nog: niet meer vertrouwd.
        fs::write(map.path().join("a.bin"), &data).unwrap();
        fs::remove_file(map.path().join(MARKEERBESTAND)).unwrap();
        assert_eq!(status(&model, map.path()), ModelStatus::Gedeeltelijk);
        fs::remove_file(map.path().join("a.bin")).unwrap();
        assert_eq!(status(&model, map.path()), ModelStatus::Ontbreekt);
    }

    #[test]
    fn serverfout_geeft_duidelijke_melding() {
        let data = testdata(1000);
        let server = TestServer::start(data.clone(), true);
        let mut model = model_voor(&server, &data);
        model.bestanden[0].url = "http://127.0.0.1:1/bestaat-niet".into();
        let map = tempfile::tempdir().unwrap();
        let fout = download_met(
            &model,
            map.path(),
            &zonder_pogingen(),
            &mut |_| {},
            &geen_annulering(),
        )
        .unwrap_err();
        assert!(matches!(fout, DownloadFout::Netwerk(_)), "{fout:?}");
    }

    #[test]
    fn overzicht_serialiseert_voor_de_interface() {
        let model = crate::model::zoek(crate::model::STANDAARD_MODEL).unwrap();
        let map = tempfile::tempdir().unwrap();
        let overzicht = overzicht(&model, map.path());
        assert_eq!(overzicht.status, ModelStatus::Ontbreekt);
        assert_eq!(overzicht.aanwezig_bytes, 0);
        let json = serde_json::to_value(&overzicht).unwrap();
        assert_eq!(json["id"], model.id);
        assert_eq!(json["status"], "ontbreekt");
        // De grootte telt het hulpmodel (Silero) mee.
        let silero = crate::model::zoek(crate::model::SILERO_MODEL).unwrap();
        assert_eq!(
            json["grootteBytes"],
            model.totale_grootte() + silero.totale_grootte()
        );
        assert_eq!(json["aanwezigBytes"], 0);
        assert_eq!(json["bron"], "huggingface.co");
        // De tempmap bestaat, dus de vrije ruimte is te meten; een hele schijf past ruim.
        assert!(json["vrijBytes"].as_u64().is_some_and(|v| v > 0));
        assert_eq!(json["tekortBytes"], 0);
    }

    #[test]
    fn samengestelde_status_is_pas_compleet_als_alles_compleet_is() {
        use ModelStatus::{Compleet, Gedeeltelijk, Ontbreekt};
        assert_eq!(samengestelde_status(&[Compleet, Compleet]), Compleet);
        assert_eq!(samengestelde_status(&[Ontbreekt, Ontbreekt]), Ontbreekt);
        // Parakeet staat er, Silero nog niet: een bestaande installatie.
        assert_eq!(samengestelde_status(&[Compleet, Ontbreekt]), Gedeeltelijk);
        assert_eq!(
            samengestelde_status(&[Gedeeltelijk, Ontbreekt]),
            Gedeeltelijk
        );
    }

    #[test]
    fn model_zonder_hulpmodel_is_gedeeltelijk_en_downloadt_alleen_wat_ontbreekt() {
        let hoofd_data = testdata(300_000);
        let hulp_data = testdata(50_000);
        let hoofd_server = TestServer::start(hoofd_data.clone(), true);
        let hulp_server = TestServer::start(hulp_data.clone(), true);
        let hoofd = model_voor(&hoofd_server, &hoofd_data);
        let mut hulp = model_voor(&hulp_server, &hulp_data);
        hulp.id = "hulp".into();
        hulp.rol = Rol::Vad;
        hulp.talen.clear();
        let basis = tempfile::tempdir().unwrap();
        let hoofdmap = basis.path().join("test");
        let hulpmap = basis.path().join("hulp");
        let doelen = vec![(hoofd.clone(), hoofdmap.clone()), (hulp, hulpmap)];

        // Eerst alleen het spraakmodel, zoals op een pc van voor Silero.
        download(&hoofd, &hoofdmap, |_| {}, &geen_annulering()).unwrap();
        let voor = overzicht_van(&hoofd, &hoofdmap, &doelen);
        assert_eq!(voor.status, ModelStatus::Gedeeltelijk);
        assert_eq!(voor.grootte_bytes, 350_000);
        assert_eq!(voor.aanwezig_bytes, 300_000);

        // Daarna alleen het ontbrekende: het spraakmodel krijgt geen nieuw verzoek.
        let aantal = hoofd_server.bereiken().len();
        download_doelen(&doelen, &snel(), &mut |_| {}, &geen_annulering()).unwrap();
        assert_eq!(hoofd_server.bereiken().len(), aantal);
        let na = overzicht_van(&hoofd, &hoofdmap, &doelen);
        assert_eq!(na.status, ModelStatus::Compleet);
        assert_eq!(na.tekort_bytes, 0);
    }

    #[test]
    fn vrije_ruimte_meet_de_dichtstbijzijnde_bestaande_map() {
        let map = tempfile::tempdir().unwrap();
        let nog_niet_gemaakt = map.path().join("modellen").join("model");
        assert!(!nog_niet_gemaakt.exists());
        // Een map die nog niet bestaat meet de bestaande map erboven.
        assert!(vrije_ruimte(&nog_niet_gemaakt).is_some_and(|vrij| vrij > 0));
        assert!(vrije_ruimte(map.path()).is_some_and(|vrij| vrij > 0));
    }

    #[test]
    fn ruimtetekort_telt_de_marge_van_twintig_procent_mee() {
        // 740 MB nodig plus 20 procent (148 MB) is 888 MB; bij 310 MB vrij is 578 MB te weinig.
        assert_eq!(ruimtetekort(Some(310_000_000), 740_000_000), 578_000_000);
        // Precies genoeg is genoeg.
        assert_eq!(ruimtetekort(Some(888_000_000), 740_000_000), 0);
        assert_eq!(ruimtetekort(Some(887_999_999), 740_000_000), 1);
        // De marge schaalt mee: een model van 1,1 GB vraagt meer marge dan een van 740 MB.
        assert_eq!(ruimtetekort(Some(1_320_000_000), 1_100_000_000), 0);
        assert_eq!(ruimtetekort(Some(1_319_999_999), 1_100_000_000), 1);
        // Onbekend: niet weigeren.
        assert_eq!(ruimtetekort(None, 740_000_000), 0);
    }

    #[test]
    fn de_marge_is_minstens_honderd_megabyte() {
        // Een hervatte download met nog 50 MB te gaan: 20 procent is 10 MB, maar de marge
        // is minstens 100 MB, dus er moet 150 MB vrij zijn.
        assert_eq!(ruimtetekort(Some(150_000_000), 50_000_000), 0);
        assert_eq!(ruimtetekort(Some(149_999_999), 50_000_000), 1);
        assert_eq!(ruimtetekort(Some(60_000_000), 50_000_000), 90_000_000);
        // Vanaf 500 MB nog nodig wint de 20 procent.
        assert_eq!(ruimtetekort(Some(600_000_000), 500_000_000), 0);
        assert_eq!(ruimtetekort(Some(599_999_999), 500_000_000), 1);
        // Ook als er niets meer nodig is, blijft de marge staan.
        assert_eq!(ruimtetekort(Some(0), 0), RUIMTE_MARGE_MINIMUM);
    }

    #[test]
    fn een_half_mislukte_verwijdering_ruimt_de_rest_op_en_is_nooit_compleet() {
        let basis = tempfile::tempdir().unwrap();
        let map = basis.path().join("test");
        let mut model = model_op_schijf(&map);
        // b.bin staat er nu ook helemaal; a.bin wordt een map, zodat hij niet weg kan (zoals
        // een bestand dat Windows nog vasthoudt).
        fs::write(map.join("b.bin"), [0_u8; 10]).unwrap();
        fs::remove_file(map.join("a.bin")).unwrap();
        fs::create_dir(map.join("a.bin")).unwrap();
        model.bestanden[0].pad = "a.bin".into();
        let fout = verwijder_model(&model, &map).unwrap_err();
        assert!(
            matches!(&fout, DownloadFout::NietVerwijderd { bestand } if bestand == "a.bin"),
            "{fout:?}"
        );
        // De rest is wel weg, ook wat na a.bin kwam.
        assert!(!map.join("b.bin").exists());
        assert!(!map.join("b.bin.part").exists());
        assert!(!map.join(MARKEERBESTAND).exists());
        // Zonder markering geldt het model niet meer als compleet.
        assert_ne!(status(&model, &map), ModelStatus::Compleet);
    }

    #[test]
    fn een_hulpmodel_blijft_als_een_ander_model_het_op_schijf_of_in_de_download_heeft() {
        let catalogus = model::catalogus().unwrap();
        let ultra = catalogus
            .iter()
            .find(|m| m.id == model::STANDAARD_MODEL)
            .unwrap();
        let hulp = ultra
            .hulp
            .first()
            .expect("Ultra heeft een hulpmodel")
            .clone();
        let ander = catalogus
            .iter()
            .find(|m| m.id != ultra.id && m.hulp.contains(&hulp))
            .expect("een ander model met hetzelfde hulpmodel");
        let niets = |_: &Model| false;
        let ander_op_schijf = |m: &Model| m.id == ander.id;
        // Niemand anders heeft het: weg.
        assert!(!hulp_nog_nodig(&catalogus, ultra, &hulp, &[], &niets));
        // Een ander model staat (deels) op schijf: blijven.
        assert!(hulp_nog_nodig(
            &catalogus,
            ultra,
            &hulp,
            &[],
            &ander_op_schijf
        ));
        // Een ander model komt nu binnen, nog zonder deelbestand: ook blijven.
        assert!(hulp_nog_nodig(
            &catalogus,
            ultra,
            &hulp,
            std::slice::from_ref(&ander.id),
            &niets
        ));
        // Een ander model dat nog wacht achter een lopende download telt ook mee.
        assert!(hulp_nog_nodig(
            &catalogus,
            ultra,
            &hulp,
            &["iets-anders".to_string(), ander.id.clone()],
            &niets
        ));
        // Het model dat zelf weggaat, telt niet mee, ook niet als het binnenkomt.
        assert!(!hulp_nog_nodig(
            &catalogus,
            ultra,
            &hulp,
            std::slice::from_ref(&ultra.id),
            &niets
        ));
    }

    #[test]
    fn compleet_vraagt_ook_het_hulpmodel() {
        let data = testdata(20_000);
        let server = TestServer::start(data.clone(), true);
        let mut model = model_voor(&server, &data);
        let basis = tempfile::tempdir().unwrap();
        let map = basis.path().join("test");
        download(&model, &map, |_| {}, &geen_annulering()).unwrap();
        assert!(is_compleet(&model, &map));
        assert_eq!(overzicht(&model, &map).status, ModelStatus::Compleet);
        // Hetzelfde model met Silero erbij, dat er (naast deze map) niet staat: niet compleet,
        // net als in het overzicht.
        model.hulp = vec![model::SILERO_MODEL.to_string()];
        assert_eq!(status(&model, &map), ModelStatus::Compleet);
        assert!(!is_compleet(&model, &map));
        assert_ne!(overzicht(&model, &map).status, ModelStatus::Compleet);
    }

    /// Een model met twee bestanden op schijf in `map`, gecontroleerd en met een deelbestand.
    fn model_op_schijf(map: &Path) -> Model {
        let mut model = model_voor(&TestServer::start(Vec::new(), true), b"");
        model.bestanden = ["a.bin", "b.bin"]
            .iter()
            .map(|pad| ModelBestand {
                pad: (*pad).to_string(),
                url: format!("https://voorbeeld.invalid/{pad}"),
                sha256: "0".repeat(64),
                grootte: 10,
            })
            .collect();
        fs::create_dir_all(map).unwrap();
        fs::write(map.join("a.bin"), [0_u8; 10]).unwrap();
        fs::write(map.join("b.bin.part"), [0_u8; 4]).unwrap();
        fs::write(map.join(MARKEERBESTAND), "{}").unwrap();
        model
    }

    #[test]
    fn verwijderen_haalt_bestanden_deel_en_markering_weg_en_telt_de_vrijgekomen_bytes() {
        let basis = tempfile::tempdir().unwrap();
        let map = basis.path().join("test");
        let model = model_op_schijf(&map);
        assert_eq!(verwijder_model(&model, &map).unwrap(), 10 + 4 + 2);
        assert!(!map.exists(), "een lege modelmap gaat mee weg");
        assert_eq!(status(&model, &map), ModelStatus::Ontbreekt);
    }

    #[test]
    fn verwijderen_laat_vreemde_bestanden_in_de_map_staan() {
        let basis = tempfile::tempdir().unwrap();
        let map = basis.path().join("test");
        let model = model_op_schijf(&map);
        fs::write(map.join("notities.txt"), "van de gebruiker").unwrap();
        verwijder_model(&model, &map).unwrap();
        assert!(map.join("notities.txt").exists());
        assert!(!map.join("a.bin").exists());
    }

    #[test]
    fn verwijderen_van_een_model_dat_er_niet_staat_is_geen_fout() {
        let basis = tempfile::tempdir().unwrap();
        let map = basis.path().join("bestaat-niet");
        let model = model_op_schijf(&basis.path().join("ergens-anders"));
        assert_eq!(verwijder_model(&model, &map).unwrap(), 0);
    }

    #[test]
    fn staat_op_schijf_ziet_ook_een_afgebroken_download() {
        let basis = tempfile::tempdir().unwrap();
        let map = basis.path().join("test");
        let model = model_op_schijf(&map);
        assert!(staat_op_schijf(&model, &map));
        verwijder_model(&model, &map).unwrap();
        assert!(!staat_op_schijf(&model, &map));
    }

    #[test]
    fn host_komt_uit_het_adres() {
        assert_eq!(
            host_van_url("https://huggingface.co/org/repo/resolve/abc/m.gguf"),
            Some("huggingface.co")
        );
        assert_eq!(
            host_van_url("http://gebruiker@127.0.0.1:8080/a.bin"),
            Some("127.0.0.1")
        );
        assert_eq!(
            host_van_url("https://modellen.aigenwijs.com"),
            Some("modellen.aigenwijs.com")
        );
        assert_eq!(host_van_url("geen adres"), None);
        assert_eq!(host_van_url("https:///pad"), None);
    }

    #[test]
    fn afknijper_laat_vier_keer_per_seconde_door() {
        let begin = Instant::now();
        let mut afknijper = Afknijper::nieuw(VOORTGANG_INTERVAL);
        assert!(afknijper.laat_door(begin));
        assert!(!afknijper.laat_door(begin + Duration::from_millis(100)));
        assert!(!afknijper.laat_door(begin + Duration::from_millis(249)));
        assert!(afknijper.laat_door(begin + Duration::from_millis(250)));
        assert!(!afknijper.laat_door(begin + Duration::from_millis(300)));
        // Een seconde met elke milliseconde een melding levert er vier of vijf op, niet duizend.
        let mut afknijper = Afknijper::nieuw(VOORTGANG_INTERVAL);
        let doorgelaten = (0..1000)
            .filter(|ms| afknijper.laat_door(begin + Duration::from_millis(*ms)))
            .count();
        assert_eq!(doorgelaten, 4);
    }

    #[test]
    fn een_volle_schijf_is_een_eigen_fout_met_de_vaste_melding() {
        let fout = DownloadFout::from(io::Error::from(io::ErrorKind::StorageFull));
        assert!(matches!(fout, DownloadFout::SchijfVol), "{fout:?}");
        assert!(!fout.is_tijdelijk());
        assert_eq!(fout.vervolg(), Vervolg::OpnieuwProberen);
        // De vaste tekst: het logboek herkent hem, zonder vrije tekst op te slaan.
        let melding = fout.melding_voor_gebruiker();
        assert_eq!(melding, Probleem::SchijfVol.melding());
        assert_eq!(Probleem::uit_melding(&melding), Some(Probleem::SchijfVol));
        // Een andere schijffout blijft de gewone schijffout.
        let ander = DownloadFout::from(io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(matches!(ander, DownloadFout::Schijf(_)), "{ander:?}");
    }

    #[test]
    fn foutvertaling_volgt_de_soort_en_niet_de_tekst() {
        let bestand = || "a.bin".to_string();
        let gevallen = [
            (DownloadFout::Netwerk("x".into()), Vervolg::OpnieuwProberen),
            (
                DownloadFout::Server {
                    status: 503,
                    bestand: bestand(),
                },
                Vervolg::OpnieuwProberen,
            ),
            (
                DownloadFout::Onvolledig { bestand: bestand() },
                Vervolg::OpnieuwProberen,
            ),
            (DownloadFout::Schijf("vol".into()), Vervolg::OpnieuwProberen),
            (
                DownloadFout::HashKlopptNiet { bestand: bestand() },
                Vervolg::OpnieuwDownloaden,
            ),
            (
                DownloadFout::TeGroot { bestand: bestand() },
                Vervolg::OpnieuwDownloaden,
            ),
            (
                DownloadFout::OnverwachtAntwoord { bestand: bestand() },
                Vervolg::OpnieuwProberen,
            ),
            (
                DownloadFout::AnderBestand { bestand: bestand() },
                Vervolg::OpnieuwProberen,
            ),
        ];
        for (fout, verwacht) in gevallen {
            assert_eq!(fout.vervolg(), verwacht, "{fout:?}");
            let Toestand::Fout { melding, vervolg } = fout.naar_toestand() else {
                unreachable!("{fout:?} had een fout moeten worden");
            };
            assert_eq!(vervolg, verwacht);
            // Geen technische details of bestandsnamen in de melding voor de gebruiker.
            assert!(
                !melding.contains("a.bin") && !melding.contains("503"),
                "{melding}"
            );
        }
        assert_eq!(
            DownloadFout::Geannuleerd.naar_toestand(),
            Toestand::Geannuleerd
        );
    }

    #[test]
    fn download_als_taak_meldt_procenten_en_eindigt_klaar() {
        let data = testdata(300_000);
        let server = TestServer::start(data.clone(), true);
        let model = model_voor(&server, &data);
        let map = tempfile::tempdir().unwrap();
        let mut gebeurtenissen = Vec::new();
        let mut voortgangen = Vec::new();
        let eind = voer_download_uit(
            &model,
            map.path(),
            TaakId(9),
            &geen_annulering(),
            &snel(),
            |g| gebeurtenissen.push(g),
            |m| {
                if let Melding::Voortgang(v) = m {
                    voortgangen.push(v);
                }
            },
        );
        assert_eq!(eind, Toestand::Klaar);
        assert!(
            gebeurtenissen
                .iter()
                .all(|g| g.taak == TaakId(9) && g.soort == Soort::Download)
        );
        assert_eq!(
            gebeurtenissen.first().map(|g| &g.toestand),
            Some(&Toestand::verwerkt(Some(0)))
        );
        assert_eq!(
            gebeurtenissen.last().map(|g| &g.toestand),
            Some(&Toestand::Klaar)
        );
        // Procenten lopen op en komen elk hooguit één keer voor.
        let procenten: Vec<u8> = gebeurtenissen
            .iter()
            .filter_map(|g| match g.toestand {
                Toestand::Verwerkt { voortgang, .. } => voortgang,
                _ => None,
            })
            .collect();
        assert!(procenten.windows(2).all(|w| w[0] < w[1]), "{procenten:?}");
        assert_eq!(procenten.last(), Some(&100));
        // De laatste byte-voortgang komt altijd door.
        assert_eq!(
            voortgangen.last().copied(),
            Some(Voortgang {
                binnen: 300_000,
                totaal: 300_000
            })
        );
        assert_eq!(status(&model, map.path()), ModelStatus::Compleet);
    }

    #[test]
    fn download_als_taak_eindigt_geannuleerd_of_met_een_fout() {
        let data = testdata(300_000);
        let server = TestServer::start(data.clone(), true);
        let model = model_voor(&server, &data);

        let map = tempfile::tempdir().unwrap();
        let annuleer = Arc::new(AtomicBool::new(true));
        let mut gebeurtenissen = Vec::new();
        let eind = voer_download_uit(
            &model,
            map.path(),
            TaakId(1),
            &annuleer,
            &snel(),
            |g| gebeurtenissen.push(g),
            |_| {},
        );
        assert_eq!(eind, Toestand::Geannuleerd);
        assert_eq!(
            gebeurtenissen.last().map(|g| &g.toestand),
            Some(&Toestand::Geannuleerd)
        );

        let mut kapot = model.clone();
        kapot.bestanden[0].url = "http://127.0.0.1:1/bestaat-niet".into();
        let map = tempfile::tempdir().unwrap();
        let eind = voer_download_uit(
            &kapot,
            map.path(),
            TaakId(2),
            &geen_annulering(),
            &zonder_pogingen(),
            |_| {},
            |_| {},
        );
        assert!(
            matches!(
                eind,
                Toestand::Fout {
                    vervolg: Vervolg::OpnieuwProberen,
                    ..
                }
            ),
            "{eind:?}"
        );
    }
}
