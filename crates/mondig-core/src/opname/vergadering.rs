//! Een vergadering opnemen: microfoon en systeemgeluid tegelijk, als twee sporen in stukken
//! op schijf (ADR 0008; opzet in ADR 0013, aanvulling 4b-A).
//!
//! Elke opname is een eigen map in de tijdelijke map voor vergaderingen
//! ([`vergadermap`]), met:
//!
//! - `opname.json`: wanneer de opname begon en hoe ze is opgezet;
//! - `opname.slot`: een bestand dat vergrendeld is zolang de opname loopt (valt het proces
//!   weg, dan laat het besturingssysteem het slot los);
//! - `microfoon/` en `systeem/`: de sporen, zie module [`stukken`](super::stukken);
//! - `einde.json`: alleen als de opname netjes is afgesloten of hersteld.
//!
//! De twee sporen delen één tijdlijn: het moment van [`Vergaderopname::start`] is nul, en
//! elk stuk heeft een anker op die tijdlijn. Het systeemspoor wordt bovendien met stilte
//! opgevuld waar de computer niets afspeelde, zodat het even lang is als de opname.
//!
//! Elk spoor heeft een eigen schrijfthread, zodat een trage schijf de bron niet ophoudt.
//! Tussen bron en schrijver staat een begrensde wachtrij (ongeveer een halve minuut); is die
//! vol, dan tellen de samples als verloren in plaats van dat het geheugen groeit.
//!
//! De audio staat alleen tijdelijk op schijf: [`wis`] haalt een opname weg (na het
//! uitschrijven, 4b-C), [`onafgemaakt`] en [`herstel`] halen een afgebroken opname terug.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::niveau::Piekmeter;
use super::stukken::{self, STIL_PIEK, STUK_DUUR, SpoorSchrijver, SpoorUitslag};
use super::{Bron, FoutSoort, OpnameFout, Uitgang, slot};
use crate::audio::DOEL_SAMPLERATE;
use crate::taak::{Foutmelding, Vervolg};

pub use crate::spoor::{
    Afsluiting, EINDE, MELDING_STIL_SYSTEEM, MICROFOON, SYSTEEM, SpoorSamenvatting, Vergadersoort,
};

const KOP: &str = crate::spoor::OPZET;
const SLOT: &str = "opname.slot";

/// Versie van de opzet van een opnamemap.
const VERSIE: u32 = 1;

/// Na zo lang zonder geluid van de computer krijgt de gebruiker tijdens een online opname
/// [`MELDING_SYSTEEM_STIL_LIVE`]. Stil is hetzelfde als bij het afsluiten: geen sample boven
/// [`STIL_PIEK`].
pub const SYSTEEM_STIL_NA: Duration = Duration::from_mins(1);

/// De waarschuwing tijdens de opname als het systeemspoor [`SYSTEEM_STIL_NA`] stil is (tekst
/// uit het ontwerp van 4b-C).
pub const MELDING_SYSTEEM_STIL_LIVE: &str = "Er komt al een minuut geen geluid van de computer binnen. Speelt de vergadering via een ander apparaat?";

/// Zoveel blokken kunnen tussen bron en schrijver wachten: bij blokken van 30 ms ongeveer
/// een halve minuut, en hooguit enkele MB's.
const WACHTRIJ: usize = 1024;

/// De tijdelijke map voor vergaderopnames, in de lokale app-gegevens (op Windows
/// `AppData\Local`, niet `Roaming`: een meereizend profiel zou de audio naar de server
/// kopiëren).
///
/// # Errors
///
/// Geeft een [`Foutmelding`] als het besturingssysteem geen gegevensmap kent.
pub fn vergadermap() -> Result<PathBuf, Foutmelding> {
    let basis = dirs::data_local_dir().ok_or_else(|| {
        Foutmelding::nieuw(
            "Mondig vindt geen map om de opname tijdelijk te bewaren.",
            Vervolg::Geen,
        )
    })?;
    Ok(basis
        .join(crate::model::IDENTIFIER)
        .join("vergaderingen")
        .join("tijdelijk"))
}

/// Wat bij het begin van een opname wordt vastgelegd.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Opzet {
    /// Versie van deze opzet.
    pub versie: u32,
    /// Begin van de opname (milliseconden sinds 1970, UTC): het nulpunt van de tijdlijn.
    pub begin_unix_ms: u64,
    /// Samplerate van de sporen.
    pub samplerate: u32,
    /// Lengte van een stuk in seconden.
    pub stuk_seconden: u64,
    /// Wordt ook het geluid van de computer opgenomen?
    pub systeemgeluid: bool,
    /// Online of in de ruimte. Ontbreekt in opnames van voor sprint 4b-C: dan online.
    #[serde(default)]
    pub soort: Vergadersoort,
}

/// Wat er met een spoor gebeurde.
#[derive(Debug, Clone, PartialEq)]
pub struct Spoor {
    /// Bestanden, lengte en niveau.
    pub uitslag: SpoorUitslag,
    /// Samples die verloren gingen.
    pub verloren: u64,
    /// Viel de bron uit? Wat tot dan opgenomen was, is bewaard.
    pub onderbroken: Option<Foutmelding>,
}

impl Spoor {
    fn samenvatting(&self) -> SpoorSamenvatting {
        SpoorSamenvatting {
            stukken: self.uitslag.stukken,
            samples: self.uitslag.samples,
            verloren: self.verloren,
            stil: self.uitslag.stil(),
            onderbroken: self.onderbroken.is_some(),
        }
    }
}

/// Wat een vergaderopname opleverde.
#[derive(Debug, Clone, PartialEq)]
pub struct Vergaderuitslag {
    /// De map van de opname.
    pub map: PathBuf,
    /// Lengte van de opname.
    pub duur: Duration,
    /// Het microfoonspoor ("Jij").
    pub microfoon: Spoor,
    /// Het systeemspoor ("Anderen"), als dat er is.
    pub systeem: Option<Spoor>,
}

impl Vergaderuitslag {
    /// Een melding in gewone taal als het systeemspoor stil bleef: dan speelde de vergadering
    /// waarschijnlijk via een ander apparaat, of was er niemand anders.
    #[must_use]
    pub fn melding_stil_systeem(&self) -> Option<String> {
        let systeem = self.systeem.as_ref()?;
        systeem
            .uitslag
            .stil()
            .then(|| MELDING_STIL_SYSTEEM.to_string())
    }
}

/// Een bericht van bron naar schrijfthread.
enum Bericht {
    Blok {
        samples: Vec<f32>,
        aangekomen: Instant,
        opgenomen: Option<Instant>,
    },
    Einde {
        tot: Instant,
    },
}

/// Wat de interface tijdens de opname van een spoor wil weten, zonder de schrijver op te
/// houden: de piek sinds de vorige keer kijken en wanneer er voor het laatst geluid was.
#[derive(Debug)]
struct Meter {
    begin: Instant,
    /// De hoogste piek sinds de vorige keer kijken; dezelfde meter als de niveaumeter zonder
    /// opname, zodat beide dezelfde schaal hebben.
    piek: Piekmeter,
    /// Milliseconden na het begin van het laatste blok met geluid boven [`STIL_PIEK`]; 0 als
    /// er nog niets te horen was.
    luid_ms: AtomicU64,
}

impl Meter {
    fn nieuw(begin: Instant) -> Self {
        Self {
            begin,
            piek: Piekmeter::default(),
            luid_ms: AtomicU64::new(0),
        }
    }

    fn meet(&self, samples: &[f32]) {
        let piek = self.piek.meet(samples);
        if piek >= STIL_PIEK {
            let ms = u64::try_from(self.begin.elapsed().as_millis()).unwrap_or(u64::MAX);
            self.luid_ms.fetch_max(ms, Ordering::Relaxed);
        }
    }

    /// De piek sinds de vorige keer, en weer op nul.
    fn neem_piek(&self) -> f32 {
        self.piek.neem_piek()
    }

    /// Hoe lang er al geen geluid boven [`STIL_PIEK`] binnenkwam (sinds het begin als er nog
    /// nooit iets was).
    fn stil_sinds(&self) -> Duration {
        let luid = Duration::from_millis(self.luid_ms.load(Ordering::Relaxed));
        self.begin.elapsed().saturating_sub(luid)
    }
}

/// Het niveau van de sporen tijdens de opname, van 0 (stilte) tot 1, op dezelfde schaal in
/// decibel als de microfoontest ([`super::niveau::niveau_van_rms`], hier op de piek).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Niveaus {
    /// De microfoon ("Jij").
    pub microfoon: f32,
    /// Het geluid van de computer ("Anderen"), als dat wordt opgenomen.
    pub systeem: Option<f32>,
}

/// De uitgang naar schijf: geeft blokken door aan de schrijfthread.
struct SchijfUitgang {
    zender: SyncSender<Bericht>,
    verloren: Arc<AtomicU64>,
    meter: Arc<Meter>,
}

impl SchijfUitgang {
    fn stuur(&self, samples: &[f32], opgenomen: Option<Instant>) {
        if samples.is_empty() {
            return;
        }
        self.meter.meet(samples);
        let bericht = Bericht::Blok {
            samples: samples.to_vec(),
            aangekomen: Instant::now(),
            opgenomen,
        };
        match self.zender.try_send(bericht) {
            Ok(()) => {}
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.verloren
                    .fetch_add(samples.len() as u64, Ordering::Relaxed);
            }
        }
    }
}

impl Uitgang for SchijfUitgang {
    fn ontvang(&mut self, samples: &[f32]) {
        self.stuur(samples, None);
    }

    fn ontvang_op(&mut self, samples: &[f32], opgenomen: Instant) {
        self.stuur(samples, Some(opgenomen));
    }
}

/// Een lopend spoor: bron, wachtrij en schrijfthread.
struct Lopend {
    bron: Box<dyn Bron>,
    zender: SyncSender<Bericht>,
    draad: JoinHandle<io::Result<SpoorUitslag>>,
    verloren: Arc<AtomicU64>,
    schrijffout: Arc<Mutex<Option<io::ErrorKind>>>,
    meter: Arc<Meter>,
}

fn schrijf_draad(
    mut schrijver: SpoorSchrijver,
    ontvanger: Receiver<Bericht>,
    verloren: &AtomicU64,
    schrijffout: &Mutex<Option<io::ErrorKind>>,
) -> io::Result<SpoorUitslag> {
    let mut fout: Option<io::Error> = None;
    let mut tot = None;
    for bericht in ontvanger {
        match bericht {
            Bericht::Blok {
                samples,
                aangekomen,
                opgenomen,
            } => {
                if fout.is_some() {
                    verloren.fetch_add(samples.len() as u64, Ordering::Relaxed);
                } else if let Err(f) = schrijver.blok(&samples, aangekomen, opgenomen) {
                    *slot(schrijffout) = Some(f.kind());
                    fout = Some(f);
                }
            }
            Bericht::Einde { tot: einde } => {
                tot = Some(einde);
                break;
            }
        }
    }
    if let Some(f) = fout {
        // Wat er wel staat, netjes afsluiten; de fout gaat voor.
        let _ = schrijver.sluit(None);
        return Err(f);
    }
    schrijver.sluit(tot)
}

impl Lopend {
    fn start(
        mut bron: Box<dyn Bron>,
        map: &Path,
        begin: Instant,
        opvullen: bool,
        stuk: Duration,
    ) -> Result<Self, Foutmelding> {
        let schrijver =
            SpoorSchrijver::nieuw(map, begin, opvullen, stuk).map_err(|f| schijffout(&f))?;
        let (zender, ontvanger) = mpsc::sync_channel(WACHTRIJ);
        let verloren = Arc::new(AtomicU64::new(0));
        let schrijffout = Arc::new(Mutex::new(None));
        let draad = {
            let verloren = Arc::clone(&verloren);
            let schrijffout = Arc::clone(&schrijffout);
            std::thread::Builder::new()
                .name("mondig-vergadering-schrijven".into())
                .spawn(move || schrijf_draad(schrijver, ontvanger, &verloren, &schrijffout))
                .map_err(|_| Foutmelding::from(OpnameFout::nieuw(FoutSoort::Programma)))?
        };
        let meter = Arc::new(Meter::nieuw(begin));
        let uitgang = SchijfUitgang {
            zender: zender.clone(),
            verloren: Arc::clone(&verloren),
            meter: Arc::clone(&meter),
        };
        if let Err(fout) = bron.start(Box::new(uitgang)) {
            let _ = zender.send(Bericht::Einde { tot: begin });
            drop(zender);
            let _ = draad.join();
            return Err(Foutmelding::from(&fout));
        }
        Ok(Self {
            bron,
            zender,
            draad,
            verloren,
            schrijffout,
            meter,
        })
    }

    /// Stopt de bron, laat de schrijver alles wegschrijven en tot `tot` opvullen.
    fn stop(self, tot: Instant) -> Result<Spoor, Foutmelding> {
        self.stop_bron().rond_af(tot)
    }

    /// Stopt alleen de bron: daarna komen er geen blokken meer, dus `Einde` is echt het
    /// laatste bericht.
    fn stop_bron(mut self) -> Gestopt {
        let einde = self.bron.stop();
        Gestopt {
            einde,
            lopend: self,
        }
    }
}

/// Een spoor waarvan de bron gestopt is en de schrijver nog loopt.
struct Gestopt {
    einde: Result<super::Einde, OpnameFout>,
    lopend: Lopend,
}

impl Gestopt {
    fn rond_af(self, tot: Instant) -> Result<Spoor, Foutmelding> {
        let Self { einde, lopend } = self;
        let Lopend {
            zender,
            draad,
            verloren,
            ..
        } = lopend;
        let _ = zender.send(Bericht::Einde { tot });
        drop(zender);
        let uitslag = draad
            .join()
            .map_err(|_| Foutmelding::from(OpnameFout::nieuw(FoutSoort::Programma)))?
            .map_err(|f| schijffout(&f))?;
        let (onderbroken, verloren_bron) = match einde {
            Ok(einde) => (einde.uitgevallen, einde.verloren),
            Err(fout) => (Some(fout), 0),
        };
        Ok(Spoor {
            uitslag,
            verloren: verloren_bron + verloren.load(Ordering::Relaxed),
            onderbroken: onderbroken.as_ref().map(Foutmelding::from),
        })
    }
}

fn schijffout(fout: &io::Error) -> Foutmelding {
    let melding = if fout.kind() == io::ErrorKind::StorageFull {
        "De schijf is vol. Mondig kan de opname niet verder bewaren."
    } else {
        "Mondig kan de opname niet op schijf bewaren."
    };
    Foutmelding::nieuw(melding, Vervolg::Geen)
}

fn ongeldige_map() -> Foutmelding {
    Foutmelding::nieuw("Deze map is geen vergaderopname van Mondig.", Vervolg::Geen)
}

/// Een lopende vergaderopname.
pub struct Vergaderopname {
    map: PathBuf,
    begin: Instant,
    /// Houdt `opname.slot` vergrendeld zolang de opname loopt.
    _slot: File,
    microfoon: Option<Lopend>,
    systeem: Option<Lopend>,
}

impl Vergaderopname {
    /// Begint een opname in een nieuwe map onder `basis` (meestal [`vergadermap`]), met
    /// geopende bronnen voor de microfoon en (als die er is) het geluid van de computer.
    /// Het nulpunt van de tijdlijn ligt net voor het starten van de bronnen.
    ///
    /// # Errors
    ///
    /// Geeft een [`Foutmelding`] als de map niet te maken is of een bron niet start; dan
    /// is er niets achtergebleven.
    pub fn start(
        basis: &Path,
        microfoon: Box<dyn Bron>,
        systeem: Option<Box<dyn Bron>>,
    ) -> Result<Self, Foutmelding> {
        Self::start_met(basis, microfoon, systeem, STUK_DUUR)
    }

    /// Als [`Vergaderopname::start`], met de soort vergadering in `opname.json`. Bij
    /// [`Vergadersoort::Ruimte`] en [`Vergadersoort::Alleen`] hoort `systeem` `None` te zijn; komt er toch een, dan wordt
    /// die niet opgenomen.
    ///
    /// # Errors
    ///
    /// Zie [`Vergaderopname::start`].
    pub fn start_als(
        basis: &Path,
        soort: Vergadersoort,
        microfoon: Box<dyn Bron>,
        systeem: Option<Box<dyn Bron>>,
    ) -> Result<Self, Foutmelding> {
        Self::start_soort_met(basis, soort, microfoon, systeem, STUK_DUUR)
    }

    /// Als [`Vergaderopname::start_als`], met een eigen stuklengte (voor tests).
    ///
    /// # Errors
    ///
    /// Zie [`Vergaderopname::start`].
    pub fn start_soort_met(
        basis: &Path,
        soort: Vergadersoort,
        microfoon: Box<dyn Bron>,
        systeem: Option<Box<dyn Bron>>,
        stuk: Duration,
    ) -> Result<Self, Foutmelding> {
        let systeem = systeem.filter(|_| soort.met_systeem());
        Self::start_met_klok(basis, soort, microfoon, systeem, stuk, &nu_unix_ms)
    }

    /// Als [`Vergaderopname::start`], met een eigen stuklengte (voor tests).
    ///
    /// # Errors
    ///
    /// Zie [`Vergaderopname::start`].
    pub fn start_met(
        basis: &Path,
        microfoon: Box<dyn Bron>,
        systeem: Option<Box<dyn Bron>>,
        stuk: Duration,
    ) -> Result<Self, Foutmelding> {
        Self::start_met_klok(
            basis,
            Vergadersoort::Online,
            microfoon,
            systeem,
            stuk,
            &nu_unix_ms,
        )
    }

    /// Als [`Vergaderopname::start_met`], met een eigen klok voor het nulpunt in
    /// `opname.json` (voor tests, om te zien wanneer het nulpunt wordt genomen).
    fn start_met_klok(
        basis: &Path,
        soort: Vergadersoort,
        microfoon: Box<dyn Bron>,
        systeem: Option<Box<dyn Bron>>,
        stuk: Duration,
        klok: &dyn Fn() -> u64,
    ) -> Result<Self, Foutmelding> {
        let (map, slot_bestand, aangemaakt_ms) = maak_map(basis).map_err(|f| schijffout(&f))?;
        let mut opzet = Opzet {
            versie: VERSIE,
            begin_unix_ms: aangemaakt_ms,
            samplerate: DOEL_SAMPLERATE,
            stuk_seconden: stuk.as_secs(),
            systeemgeluid: systeem.is_some(),
            soort,
        };
        // Lukt het starten niet, dan blijft er niets achter: eerst het slot los, dan de map weg.
        let opruimen = |slot_bestand: File, fout: Foutmelding| {
            drop(slot_bestand);
            let _ = fs::remove_dir_all(&map);
            fout
        };
        // Eerst met het moment van aanmaken, zodat een opname die tijdens het starten
        // wegvalt al vindbaar is. Het echte nulpunt komt na het starten van de bronnen.
        if let Err(fout) = schrijf_json(&map.join(KOP), &opzet) {
            return Err(opruimen(slot_bestand, schijffout(&fout)));
        }
        // Het nulpunt: klok en stopwatch vlak na elkaar, na het schrijven van de voorlopige
        // kop (een mogelijk trage schijf) en net voor het starten van de bronnen.
        opzet.begin_unix_ms = klok();
        let begin = Instant::now();
        let microfoon = match Lopend::start(microfoon, &map.join(MICROFOON), begin, false, stuk) {
            Ok(lopend) => lopend,
            Err(fout) => return Err(opruimen(slot_bestand, fout)),
        };
        let systeem = match systeem {
            Some(bron) => match Lopend::start(bron, &map.join(SYSTEEM), begin, true, stuk) {
                Ok(lopend) => Some(lopend),
                Err(fout) => {
                    let _ = microfoon.stop(begin);
                    return Err(opruimen(slot_bestand, fout));
                }
            },
            None => None,
        };
        // Het nulpunt vastleggen kan pas nu, zonder de start op te houden.
        if let Err(fout) = schrijf_json(&map.join(KOP), &opzet) {
            let _ = microfoon.stop(begin);
            if let Some(systeem) = systeem {
                let _ = systeem.stop(begin);
            }
            return Err(opruimen(slot_bestand, schijffout(&fout)));
        }
        Ok(Self {
            map,
            begin,
            _slot: slot_bestand,
            microfoon: Some(microfoon),
            systeem,
        })
    }

    /// De map van deze opname.
    #[must_use]
    pub fn map(&self) -> &Path {
        &self.map
    }

    /// Hoe lang de opname al loopt.
    #[must_use]
    pub fn duur(&self) -> Duration {
        self.begin.elapsed()
    }

    /// Een fout bij het wegschrijven (schijf vol), als die er is. Dan neemt Mondig niets
    /// meer op en moet de eigenaar stoppen.
    #[must_use]
    pub fn schrijffout(&self) -> Option<Foutmelding> {
        [&self.microfoon, &self.systeem]
            .into_iter()
            .flatten()
            .find_map(|l| *slot(&l.schrijffout))
            .map(|soort| schijffout(&io::Error::from(soort)))
    }

    /// Het niveau van elk spoor: de piek sinds de vorige aanroep, op een schaal van 0 tot 1
    /// in decibel. Roep dit een paar keer per seconde aan voor een meter.
    #[must_use]
    pub fn niveaus(&self) -> Niveaus {
        let niveau = |l: &Lopend| super::niveau::niveau_van_rms(l.meter.neem_piek());
        Niveaus {
            microfoon: self.microfoon.as_ref().map_or(0.0, niveau),
            systeem: self.systeem.as_ref().map(niveau),
        }
    }

    /// Hoe lang er al geen geluid van de computer binnenkomt (sinds het begin als er nog niets
    /// was); `None` zonder systeemspoor. Vanaf [`SYSTEEM_STIL_NA`] hoort de gebruiker
    /// [`MELDING_SYSTEEM_STIL_LIVE`] te zien.
    #[must_use]
    pub fn systeem_stil_sinds(&self) -> Option<Duration> {
        self.systeem.as_ref().map(|l| l.meter.stil_sinds())
    }

    /// Hoeveel samples tot nu toe verloren gingen omdat de schijf het niet bijhield.
    #[must_use]
    pub fn verloren(&self) -> u64 {
        [&self.microfoon, &self.systeem]
            .into_iter()
            .flatten()
            .map(|l| l.verloren.load(Ordering::Relaxed))
            .sum()
    }

    /// Stopt de opname, schrijft alles weg en legt het einde vast.
    ///
    /// # Errors
    ///
    /// Geeft een [`Foutmelding`] als een spoor niet weg te schrijven was. Wat al op schijf
    /// staat, blijft staan en is met [`herstel`] terug te halen.
    pub fn stop(mut self) -> Result<Vergaderuitslag, Foutmelding> {
        self.stop_intern()
    }

    fn stop_intern(&mut self) -> Result<Vergaderuitslag, Foutmelding> {
        let tot = Instant::now();
        // Eerst beide bronnen stoppen, dan pas wegschrijven: anders loopt het ene spoor door
        // terwijl het andere zijn laatste stuk op schijf zet.
        let microfoon = self
            .microfoon
            .take()
            .ok_or_else(|| Foutmelding::from(OpnameFout::nieuw(FoutSoort::Programma)))?
            .stop_bron();
        let systeem = self.systeem.take().map(Lopend::stop_bron);
        let microfoon = microfoon.rond_af(tot);
        let systeem = systeem.map(|s| s.rond_af(tot)).transpose();
        let microfoon = microfoon?;
        let systeem = systeem?;
        let duur = tot.duration_since(self.begin);
        let afsluiting = Afsluiting {
            duur_ms: u64::try_from(duur.as_millis()).unwrap_or(u64::MAX),
            hersteld: false,
            microfoon: microfoon.samenvatting(),
            systeem: systeem.as_ref().map(Spoor::samenvatting),
        };
        schrijf_json(&self.map.join(EINDE), &afsluiting).map_err(|f| schijffout(&f))?;
        Ok(Vergaderuitslag {
            map: self.map.clone(),
            duur,
            microfoon,
            systeem,
        })
    }
}

impl Drop for Vergaderopname {
    fn drop(&mut self) {
        // Weggooien zonder stop: alles netjes afsluiten, zodat er geen half stuk achterblijft.
        if self.microfoon.is_some() {
            let _ = self.stop_intern();
        }
    }
}

fn nu_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Maakt een nieuwe, unieke map en vergrendelt het slot. Geeft ook het moment van aanmaken
/// (milliseconden sinds 1970), dat in de mapnaam zit.
fn maak_map(basis: &Path) -> io::Result<(PathBuf, File, u64)> {
    maak_map_met(basis, &maak_slot)
}

fn maak_slot(map: &Path) -> io::Result<File> {
    let slot_bestand = File::create(map.join(SLOT))?;
    slot_bestand.try_lock().map_err(io::Error::other)?;
    Ok(slot_bestand)
}

/// Als [`maak_map`], met een eigen manier om het slot te maken (voor tests). Mislukt dat,
/// dan blijft er geen lege map achter.
fn maak_map_met(
    basis: &Path,
    maak_slot: &dyn Fn(&Path) -> io::Result<File>,
) -> io::Result<(PathBuf, File, u64)> {
    fs::create_dir_all(basis)?;
    let aangemaakt_ms = nu_unix_ms();
    for poging in 0..100u32 {
        let naam = if poging == 0 {
            format!("vergadering-{aangemaakt_ms}")
        } else {
            format!("vergadering-{aangemaakt_ms}-{poging}")
        };
        let map = basis.join(naam);
        match fs::create_dir(&map) {
            Ok(()) => {
                return match maak_slot(&map) {
                    Ok(slot_bestand) => Ok((map, slot_bestand, aangemaakt_ms)),
                    Err(fout) => {
                        let _ = fs::remove_dir_all(&map);
                        Err(fout)
                    }
                };
            }
            Err(fout) if fout.kind() == io::ErrorKind::AlreadyExists => {}
            Err(fout) => return Err(fout),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "geen vrije naam voor de opname",
    ))
}

fn schrijf_json<T: Serialize>(pad: &Path, waarde: &T) -> io::Result<()> {
    let tekst = serde_json::to_string_pretty(waarde).map_err(io::Error::other)?;
    stukken::schrijf_atomair(pad, tekst.as_bytes())
}

fn lees_json<T: for<'de> Deserialize<'de>>(pad: &Path) -> io::Result<T> {
    let tekst = fs::read_to_string(pad)?;
    serde_json::from_str(&tekst).map_err(|f| io::Error::new(io::ErrorKind::InvalidData, f))
}

/// Leest de opzet van een opnamemap.
///
/// # Errors
///
/// Geeft een [`Foutmelding`] als de map geen opname van Mondig is.
pub fn opzet(map: &Path) -> Result<Opzet, Foutmelding> {
    lees_json(&map.join(KOP)).map_err(|_| ongeldige_map())
}

/// Of een opname nog loopt, volgens haar slotbestand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slotstand {
    /// Het slot is vergrendeld: de opname loopt (in dit of een ander proces).
    Loopt,
    /// Het slot is er niet of los: de opname loopt niet.
    LoopNiet,
    /// Het slot is er maar niet te openen (bijvoorbeeld alleen-lezen of geblokkeerd): Mondig
    /// kan niet nagaan of de opname nog loopt.
    Onbekend,
}

/// Leest de stand van het slot. Alleen "bestand bestaat niet" betekent dat de opname niet
/// loopt; elke andere fout is "onbekend", zodat een lopende opname van een ander proces
/// nooit voor afgebroken wordt aangezien (en gewist).
fn slotstand(map: &Path) -> Slotstand {
    match OpenOptions::new().write(true).open(map.join(SLOT)) {
        Ok(bestand) => {
            if bestand.try_lock().is_err() {
                Slotstand::Loopt
            } else {
                Slotstand::LoopNiet
            }
        }
        Err(fout) if fout.kind() == io::ErrorKind::NotFound => Slotstand::LoopNiet,
        Err(_) => Slotstand::Onbekend,
    }
}

/// Vergrendelt het slot van een opname zolang de teruggegeven [`File`] bestaat, voor werk aan
/// een opname die niet loopt (uitschrijven vanaf de opdrachtregel). Zo wist het automatische
/// opruimen haar niet midden in dat werk. Het slot wordt aangemaakt als het er niet is.
///
/// # Errors
///
/// Geeft een [`Foutmelding`] als de opname loopt of in gebruik is (het slot is al vergrendeld),
/// of als Mondig niet kan nagaan of dat zo is.
pub fn vergrendel(map: &Path) -> Result<File, Foutmelding> {
    if slotstand(map) == Slotstand::Onbekend {
        return Err(loopt_nog(true));
    }
    let slot_bestand = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(map.join(SLOT))
        .map_err(|f| schijffout(&f))?;
    if slot_bestand.try_lock().is_err() {
        return Err(Foutmelding::nieuw(
            "Deze opname is in gebruik: ze loopt nog, of een ander Mondig-venster of -programma werkt er nu mee. Sluit dat eerst.",
            Vervolg::Geen,
        ));
    }
    Ok(slot_bestand)
}

/// Loopt de opname zeker?
#[cfg(test)]
fn loopt(map: &Path) -> bool {
    slotstand(map) == Slotstand::Loopt
}

fn loopt_nog(onbekend: bool) -> Foutmelding {
    if onbekend {
        Foutmelding::nieuw(
            "Mondig kan niet nagaan of deze opname nog loopt, want het slotbestand is niet te openen. Controleer of Mondig niet nog opneemt.",
            Vervolg::Geen,
        )
    } else {
        Foutmelding::nieuw(
            "Deze opname loopt nog. Stop de opname eerst.",
            Vervolg::Geen,
        )
    }
}

/// Een opname die niet netjes is afgesloten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Onafgemaakt {
    /// De map van de opname.
    pub map: PathBuf,
    /// [`Slotstand::LoopNiet`] (het proces viel weg) of [`Slotstand::Onbekend`].
    pub stand: Slotstand,
}

/// De opnames onder `basis` die niet netjes zijn afgesloten en niet (zeker) meer lopen, oudste
/// eerst, met de stand van hun slot. Een lopende opname staat er niet bij.
#[must_use]
pub fn onafgemaakt_met_stand(basis: &Path) -> Vec<Onafgemaakt> {
    let Ok(lijst) = fs::read_dir(basis) else {
        return Vec::new();
    };
    let mut opnames: Vec<Onafgemaakt> = lijst
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|map| map.join(KOP).is_file() && !map.join(EINDE).exists())
        .map(|map| Onafgemaakt {
            stand: slotstand(&map),
            map,
        })
        .filter(|o| o.stand != Slotstand::Loopt)
        .collect();
    opnames.sort_by(|a, b| a.map.cmp(&b.map));
    opnames
}

/// De opnames onder `basis` die niet netjes zijn afgesloten en zeker niet meer lopen (het
/// proces viel weg), oudste eerst. Zie [`onafgemaakt_met_stand`] voor ook de opnames met een
/// slot dat niet te lezen is.
#[must_use]
pub fn onafgemaakt(basis: &Path) -> Vec<PathBuf> {
    onafgemaakt_met_stand(basis)
        .into_iter()
        .filter(|o| o.stand == Slotstand::LoopNiet)
        .map(|o| o.map)
        .collect()
}

/// Sluit een afgebroken opname af: elk spoor wordt hersteld tot en met het laatste stuk
/// dat op schijf stond (zie [`stukken::herstel_spoor`]) en `einde.json` wordt geschreven.
/// Een al afgesloten opname blijft zoals ze is.
///
/// # Errors
///
/// Geeft een [`Foutmelding`] als de map geen opname is, de opname nog loopt of de
/// bestanden niet te herstellen zijn.
pub fn herstel(map: &Path) -> Result<Afsluiting, Foutmelding> {
    let opzet = opzet(map)?;
    if let Ok(afsluiting) = lees_json::<Afsluiting>(&map.join(EINDE)) {
        return Ok(afsluiting);
    }
    // Een slot dat niet te openen is, wordt niet aangemaakt of overschreven.
    if slotstand(map) == Slotstand::Onbekend {
        return Err(loopt_nog(true));
    }
    let slot_bestand = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(map.join(SLOT))
        .map_err(|f| schijffout(&f))?;
    if slot_bestand.try_lock().is_err() {
        return Err(loopt_nog(false));
    }
    let spoor = |naam: &str| -> Result<Option<(SpoorSamenvatting, i64)>, Foutmelding> {
        let spoormap = map.join(naam);
        if !spoormap.is_dir() {
            return Ok(None);
        }
        let hersteld = stukken::herstel_spoor(&spoormap).map_err(|f| schijffout(&f))?;
        let piek = piek_van(&spoormap, &hersteld.stukken);
        Ok(Some((
            SpoorSamenvatting {
                stukken: u32::try_from(hersteld.stukken.len()).unwrap_or(u32::MAX),
                samples: hersteld.samples(),
                verloren: 0,
                stil: piek < stukken::STIL_PIEK,
                onderbroken: true,
            },
            hersteld.eind_ms(),
        )))
    };
    let (microfoon, eind_mic) = spoor(MICROFOON)?.unwrap_or((
        SpoorSamenvatting {
            stukken: 0,
            samples: 0,
            verloren: 0,
            stil: true,
            onderbroken: true,
        },
        0,
    ));
    let systeem = if opzet.systeemgeluid {
        spoor(SYSTEEM)?
    } else {
        None
    };
    let eind = systeem.as_ref().map_or(eind_mic, |(_, e)| eind_mic.max(*e));
    let afsluiting = Afsluiting {
        duur_ms: u64::try_from(eind).unwrap_or(0),
        hersteld: true,
        microfoon,
        systeem: systeem.map(|(s, _)| s),
    };
    schrijf_json(&map.join(EINDE), &afsluiting).map_err(|f| schijffout(&f))?;
    Ok(afsluiting)
}

/// Het hoogste niveau in de stukken van een spoor (0 tot 1), om een stil spoor te
/// herkennen. Leest de bestanden een voor een.
fn piek_van(spoormap: &Path, lijst: &[stukken::StukInfo]) -> f32 {
    let mut piek = 0i32;
    for info in lijst {
        let Ok(bytes) = fs::read(spoormap.join(&info.bestand)) else {
            continue;
        };
        for paar in bytes.get(44..).unwrap_or_default().as_chunks::<2>().0 {
            piek = piek.max(i32::from(i16::from_le_bytes([paar[0], paar[1]])).abs());
        }
    }
    #[allow(clippy::cast_precision_loss, reason = "hooguit 32768")]
    let piek = piek as f32 / f32::from(i16::MAX);
    piek
}

/// Gooit een opname weg, met alle audio. Weigert een map die geen opname van Mondig is, een
/// opname die nog loopt en een opname waarvan Mondig niet kan nagaan of ze nog loopt (zie
/// [`wis_toch`]).
///
/// # Errors
///
/// Geeft een [`Foutmelding`] als de map niet weg te halen is.
pub fn wis(map: &Path) -> Result<(), Foutmelding> {
    wis_met(map, false)
}

/// Als [`wis`], maar ook als Mondig niet kan nagaan of de opname nog loopt. Een opname die
/// zeker loopt, wordt nog steeds geweigerd.
///
/// # Errors
///
/// Zie [`wis`].
pub fn wis_toch(map: &Path) -> Result<(), Foutmelding> {
    wis_met(map, true)
}

fn wis_met(map: &Path, toch: bool) -> Result<(), Foutmelding> {
    // Een lege map met de naam van een opname is wat een eerdere wis achterliet nadat
    // `opname.json` al weg was: die mag ook weg, zodat opnieuw proberen altijd kan.
    if is_restmap(map) {
        return fs::remove_dir(map).map_err(|_| niet_weg());
    }
    opzet(map)?;
    match slotstand(map) {
        Slotstand::Loopt => return Err(loopt_nog(false)),
        Slotstand::Onbekend if !toch => return Err(loopt_nog(true)),
        Slotstand::Onbekend | Slotstand::LoopNiet => {}
    }
    wis_op_volgorde(map).map_err(|_| niet_weg())
}

fn niet_weg() -> Foutmelding {
    Foutmelding::nieuw(
        "Mondig kan de opname niet verwijderen. Is een bestand ervan nog open?",
        Vervolg::OpnieuwProberen,
    )
}

/// Een lege map met de naam van een opname van Mondig (`vergadering-...`).
fn is_restmap(map: &Path) -> bool {
    let naam_klopt = map
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("vergadering-"));
    naam_klopt
        && !map.join(KOP).exists()
        && fs::read_dir(map).is_ok_and(|mut lijst| lijst.next().is_none())
}

/// Verwijdert een bestand, een map met inhoud, of een link. Een link (symlink of junction)
/// gaat zelf weg; wat erachter staat blijft staan.
fn verwijder(pad: &Path) -> io::Result<()> {
    let soort = fs::symlink_metadata(pad)?.file_type();
    if soort.is_symlink() {
        // Een link naar een bestand gaat met `remove_file`, een link naar een map (op
        // Windows ook een junction) met `remove_dir`.
        fs::remove_file(pad).or_else(|_| fs::remove_dir(pad))
    } else if soort.is_dir() {
        fs::remove_dir_all(pad)
    } else {
        fs::remove_file(pad)
    }
}

/// Wist een opnamemap zo dat een wis die halverwege faalt, te herhalen is: eerst de audio en
/// de rest, dan het slot en `einde.json`, en `opname.json` als laatste. Zolang er nog iets
/// staat, staat ook `opname.json` er, dus de opname blijft herkenbaar (en in de lijst).
fn wis_op_volgorde(map: &Path) -> io::Result<()> {
    let laatst = [SLOT, EINDE, KOP];
    for item in fs::read_dir(map)? {
        let pad = item?.path();
        let naam = pad.file_name().unwrap_or_default().to_string_lossy();
        if laatst.contains(&naam.as_ref()) {
            continue;
        }
        verwijder(&pad)?;
    }
    for naam in laatst {
        // Een slot dat geen bestand is (zie `Slotstand::Onbekend`), gaat als map weg.
        match verwijder(&map.join(naam)) {
            Err(fout) if fout.kind() != io::ErrorKind::NotFound => return Err(fout),
            _ => {}
        }
    }
    fs::remove_dir(map)
}

mod verlopen;
pub use verlopen::{
    BEWAAR_KEUZES_DAGEN, BEWAAR_STANDAARD_DAGEN, Verlopen, WACHT_NA_MARKERING, bewaartijd,
    wis_verlopen, wistijd_ms,
};

#[cfg(test)]
mod tests;
