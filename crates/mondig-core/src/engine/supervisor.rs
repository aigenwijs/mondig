//! De supervisor start het werkproces met de spraakengine, bewaakt het en vertaalt
//! crashes en time-outs naar een melding in gewone taal (ADR 0009).
//!
//! Bij een poging op de videokaart (`Apparaat::Gpu`) die faalt door een crash, een
//! time-out of een fout van de engine zelf, probeert de supervisor het één keer
//! opnieuw met alleen de processor. Een kapot audiobestand telt daarbij niet mee.
//!
//! Het werkproces wordt netjes gestopt zodra de [`Supervisor`] valt (`Drop`).
//!
//! Geluid uit het geheugen (een dictaat) gaat met [`Supervisor::transcribeer_geluid`]; de
//! invoer is begrensd op [`MAX_DICTAAT`].
//!
//! Een bestand gaat met [`Supervisor::transcribeer_bestand`]: het werkproces schrijft het
//! in stukken uit en meldt na elk stuk zijn [`Voortgang`]. De wachttijd geldt per stuk,
//! zodat een bestand van een uur geen uur zonder bericht mag duren. Annuleren stuurt
//! [`Verzoek::Annuleer`]; antwoordt het werkproces dan niet binnen
//! [`Instellingen::annuleer_wachttijd`], dan wordt het afgebroken. Een spoor van een
//! vergadering gaat net zo met [`Supervisor::transcribeer_spoor`], met een voortgang per
//! venster in plaats van per stuk. Sprekers herkennen op een spoor gaat met
//! [`Supervisor::herken_sprekers`], met dezelfde terugval en hetzelfde annuleren.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::protocol::{
    Antwoord, Apparaat, FoutSoort, PROTOCOL_VERSIE, Segment, SprekerSegment, Verzoek, Videokaart,
    naar_regel, samples_naar_base64, zoek_bericht,
};
use crate::audio::{DOEL_SAMPLERATE, Leeswaarschuwing, STUK_DUUR, duur_van_samples};
use crate::filter::Meting;
use crate::spoor::uitingen;
use crate::taak::Stap;

/// Hoe lang een dictaat hooguit mag duren (ADR 0012). Langer opnemen is een vergadering
/// of een bestand. Bij deze grens is het geluid 38 MB aan samples.
pub const MAX_DICTAAT: Duration = Duration::from_secs(600);

/// Het grootste aantal samples (16 kHz mono) in één dictaat.
const MAX_DICTAAT_SAMPLES: usize = 600 * DOEL_SAMPLERATE as usize;

/// Zoveel regels van het foutkanaal (stderr) van het werkproces bewaren we voor diagnose.
const LOGREGELS: usize = 20;

/// Hoe lang we het werkproces de tijd geven om zelf te stoppen voordat we het afbreken.
const STOPTIJD: Duration = Duration::from_secs(3);

/// Zo vaak kijkt de supervisor of er geannuleerd is terwijl een bestand loopt.
const PEILTIJD: Duration = Duration::from_millis(50);

/// Mac en Linux: hiermee start het achtergrondwerkproces met een lagere prioriteit. Een
/// vast pad, nooit via PATH, zodat geen ander programma met die naam kan meeliften.
#[cfg(unix)]
const NICE: &str = "/usr/bin/nice";

/// Mac en Linux: hoeveel lager de prioriteit van het achtergrondwerkproces is (0 is
/// normaal, 19 het laagst). 10 is de standaard van `nice`.
#[cfg(unix)]
const NICE_STAP: &str = "10";

/// Windows: geen consolevenster tonen voor het werkproces.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Windows: het proces start op een lagere procesprioriteit (`BELOW_NORMAL_PRIORITY_CLASS`).
#[cfg(windows)]
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;

/// Welke taak het werkproces heeft (ADR 0012). Bepaalt de procesprioriteit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rol {
    /// Het dicteerwerkproces: normale prioriteit, zodat een dictaat voorrang heeft.
    #[default]
    Dicteren,
    /// Het achtergrondwerkproces voor bestanden en vergaderingen: lagere prioriteit
    /// zodat het dicteren er geen last van heeft: op Windows
    /// `BELOW_NORMAL_PRIORITY_CLASS`, op Mac en Linux via `/usr/bin/nice -n 10`.
    Achtergrond,
}

/// Het programma en de argumenten waarmee het werkproces start. Op Mac en Linux gaat het
/// achtergrondwerkproces via `nice`, dat zichzelf vervangt door het werkproces (het
/// proces-id blijft dus dat van het werkproces). Staat `nice` er niet, dan start het
/// werkproces gewoon, met normale prioriteit.
fn startopdracht(rol: Rol, engine_pad: &Path) -> (PathBuf, Vec<std::ffi::OsString>) {
    #[cfg(unix)]
    if rol == Rol::Achtergrond && Path::new(NICE).is_file() {
        return (
            PathBuf::from(NICE),
            vec![
                "-n".into(),
                NICE_STAP.into(),
                engine_pad.as_os_str().to_owned(),
            ],
        );
    }
    let _ = rol;
    (engine_pad.to_path_buf(), Vec::new())
}

/// Hoe ver het uitschrijven van een bestand is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Voortgang {
    /// Tot hoever het bestand is uitgeschreven.
    pub klaar: Duration,
    /// De duur van het hele bestand, als die bekend is.
    pub totaal: Option<Duration>,
    /// Welke stap er loopt: uitschrijven, of bij een vergadering daarna de sprekers herkennen.
    pub stap: Stap,
}

impl Voortgang {
    /// Het percentage klaar (0 tot en met 100), of `None` als de duur onbekend is.
    #[must_use]
    pub fn procent(&self) -> Option<u8> {
        let totaal = self.totaal?.as_millis();
        if totaal == 0 {
            return Some(100);
        }
        let procent = (self.klaar.as_millis().min(totaal) * 100) / totaal;
        u8::try_from(procent).ok()
    }
}

/// De creation flags voor het werkproces op Windows, afhankelijk van de rol.
#[cfg(windows)]
fn creation_flags(rol: Rol) -> u32 {
    match rol {
        Rol::Dicteren => CREATE_NO_WINDOW,
        Rol::Achtergrond => CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS,
    }
}

/// Waar in de levensloop van het werkproces iets misging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fase {
    /// Het opstarten, tot het eerste bericht.
    Opstarten,
    /// Het laden van het model.
    Laden,
    /// Het uitschrijven van audio.
    Transcriberen,
    /// Het zoeken naar videokaarten.
    Apparaten,
}

impl std::fmt::Display for Fase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Opstarten => "het opstarten",
            Self::Laden => "het laden van het model",
            Self::Transcriberen => "het uitschrijven",
            Self::Apparaten => "het zoeken naar de videokaart",
        })
    }
}

/// Wat er mis kan gaan met het werkproces.
#[derive(Debug, thiserror::Error)]
pub enum EngineFout {
    /// Het werkproces kon niet gestart worden.
    #[error(
        "De spraakengine kon niet gestart worden ({reden}). Controleer of Mondig volledig is geïnstalleerd."
    )]
    Starten {
        /// Wat het besturingssysteem meldde.
        reden: String,
    },
    /// Het werkproces is onverwacht gestopt, bijvoorbeeld door een fout in een driver.
    #[error(
        "De spraakengine is onverwacht gestopt{}. Probeer het opnieuw; helpt dat niet, kies dan een ander model.",
        .code.map(|c| format!(" (code {c})")).unwrap_or_default()
    )]
    Gecrasht {
        /// Afsluitcode van het proces, als het besturingssysteem die geeft.
        code: Option<i32>,
        /// De laatste regels die het proces op het foutkanaal schreef (nooit transcripttekst).
        laatste_log: Vec<String>,
    },
    /// Het werkproces reageerde niet op tijd en is gestopt.
    #[error(
        "De spraakengine reageerde niet binnen {} seconden bij {fase} en is gestopt. Probeer het opnieuw of kies een kleiner model.",
        .na.as_secs()
    )]
    TijdOverschreden {
        /// Waar het misging.
        fase: Fase,
        /// Hoe lang er gewacht is.
        na: Duration,
    },
    /// De engine meldde zelf een fout.
    #[error("{melding}")]
    Engine {
        /// Soort fout.
        soort: FoutSoort,
        /// Melding in gewone taal van de engine.
        melding: String,
    },
    /// Het werkproces komt uit een andere uitgave dan de app, bijvoorbeeld doordat een
    /// installatie het oude bestand niet kon vervangen.
    #[error(
        "Mondig is niet volledig bijgewerkt. Sluit Mondig af en installeer de nieuwste versie opnieuw."
    )]
    NietBijgewerkt {
        /// De bouwcode van het werkproces, of `None` bij een ouder werkproces dat er nog
        /// geen meldde.
        gevonden: Option<String>,
        /// De bouwcode van de app.
        verwacht: String,
        /// De protocolversie die het werkproces meldde. Wijkt die af van
        /// [`PROTOCOL_VERSIE`], dan is het protocol niet te spreken.
        protocol: u32,
    },
    /// Het werkproces stuurde iets onverwachts.
    #[error("De spraakengine stuurde een bericht dat Mondig niet begrijpt ({0}).")]
    Protocol(String),
    /// Er is nog geen model geladen.
    #[error("Er is nog geen model geladen.")]
    NietGeladen,
    /// Het uitschrijven is op verzoek van de gebruiker afgebroken.
    #[error("Het uitschrijven is geannuleerd.")]
    Geannuleerd,
    /// Er ging iets mis in Mondig zelf (bijvoorbeeld een paniek in een thread).
    #[error("Er ging iets mis in Mondig zelf. Probeer het opnieuw.")]
    Intern,
    /// Het dictaat is langer dan [`MAX_DICTAAT`]; het werkproces krijgt het niet te zien.
    #[error(
        "Dit dictaat duurt langer dan {} minuten. Neem een lange opname op als bestand of vergadering.",
        .max.as_secs() / 60
    )]
    TeLang {
        /// Duur van het aangeboden geluid.
        duur: Duration,
        /// De grens, [`MAX_DICTAAT`].
        max: Duration,
    },
}

impl EngineFout {
    /// Kan dit aan het apparaat (de videokaart of zijn driver) liggen, zodat een
    /// nieuwe poging op de processor zin heeft?
    #[must_use]
    pub fn kan_aan_apparaat_liggen(&self) -> bool {
        match self {
            Self::Gecrasht { .. } | Self::TijdOverschreden { .. } => true,
            Self::Engine { soort, .. } => soort.kan_aan_apparaat_liggen(),
            Self::Starten { .. }
            | Self::NietBijgewerkt { .. }
            | Self::Protocol(_)
            | Self::NietGeladen
            | Self::TeLang { .. }
            | Self::Geannuleerd
            | Self::Intern => false,
        }
    }
}

/// Wat het laatst gestarte werkproces in zijn hallo meldde, voor "Versiegegevens kopiëren"
/// (ADR 0012, aanvulling 2-11-2026). Een gewone Windows-app heeft geen console, dus een
/// regel op het foutkanaal ziet niemand; via de versiegegevens kan de gebruiker de
/// bouwcode van het werkproces wel doorgeven. Eén waarde voor het hele proces: alle
/// werkprocessen komen uit hetzelfde bestand naast de app.
static GEMELDE_BOUW: Mutex<Option<String>> = Mutex::new(None);

/// De bouwcode en protocolversie van het laatst gestarte werkproces, als
/// `0.3.0-dev+afd2907 (protocol 4)`, of `None` als er nog geen werkproces gestart is.
#[must_use]
pub fn laatst_gemelde_bouw() -> Option<String> {
    GEMELDE_BOUW
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// De tekst voor [`laatst_gemelde_bouw`].
fn beschrijf_hallo(bouw: Option<&str>, protocol: u32) -> String {
    format!(
        "{} (protocol {protocol})",
        bouw.unwrap_or("geen bouwcode gemeld")
    )
}

/// Of de supervisor controleert dat het werkproces bij de app past (ADR 0012, aanvulling
/// 2-11-2026).
///
/// Een werkproces uit een andere uitgave kent nieuwe modellen of berichten misschien niet
/// en geeft dan een verwarrende fout als "Onbekend model". Die fout vangen we liever bij
/// het opstarten af, met een melding die zegt wat de gebruiker moet doen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Bouwcontrole {
    /// Niet controleren (tests en gereedschap dat zijn eigen werkproces bouwt).
    #[default]
    Uit,
    /// Een verschil alleen in het log melden. Voor ontwikkelbuilds, waar app en werkproces
    /// vaak los van elkaar opnieuw gebouwd worden.
    Waarschuwen(String),
    /// Een verschil is een fout ([`EngineFout::NietBijgewerkt`]), behalve als het
    /// werkproces zelf een bouw met `-dirty` is: dan alleen een waarschuwing.
    Afdwingen(String),
}

impl Bouwcontrole {
    /// De controle voor een app met deze bouwcode (`versie+commit`). Een debugbuild of een
    /// bouwcode met `-dirty` waarschuwt alleen: daar bouwt de ontwikkelaar app en
    /// werkproces los van elkaar. Een uitgave dwingt af; meldt het werkproces zelf `-dirty`,
    /// dan waarschuwt ook een uitgave alleen (zie `beoordeel`).
    #[must_use]
    pub fn voor_app(bouwcode: &str, debugbuild: bool) -> Self {
        if debugbuild || bouwcode.contains("-dirty") {
            Self::Waarschuwen(bouwcode.to_string())
        } else {
            Self::Afdwingen(bouwcode.to_string())
        }
    }

    /// Beoordeelt wat het werkproces meldde. `Ok(Some(tekst))` is een waarschuwing voor
    /// het log; `Ok(None)` betekent dat alles klopt (of dat er niet gecontroleerd wordt).
    fn beoordeel(&self, gevonden: Option<&str>) -> Result<Option<String>, EngineFout> {
        let (verwacht, afdwingen) = match self {
            Self::Uit => return Ok(None),
            Self::Waarschuwen(verwacht) => (verwacht, false),
            Self::Afdwingen(verwacht) => (verwacht, true),
        };
        // Letterlijk vergelijken: een bouwcode met `-dirty` zegt niets over de inhoud.
        if gevonden == Some(verwacht.as_str()) {
            return Ok(None);
        }
        // Een werkproces met `-dirty` is lokaal gebouwd, net als een app met `-dirty` (die
        // krijgt al `Waarschuwen`). Dan alleen waarschuwen, aan welke kant het ook staat:
        // anders weigert een schone app een werkproces uit dezelfde commit.
        let werkproces_dirty = gevonden.is_some_and(|g| g.contains("-dirty"));
        if afdwingen && !werkproces_dirty {
            return Err(EngineFout::NietBijgewerkt {
                gevonden: gevonden.map(str::to_string),
                verwacht: verwacht.clone(),
                protocol: PROTOCOL_VERSIE,
            });
        }
        Ok(Some(format!(
            "werkproces is een andere bouw dan de app (werkproces: {}, app: {verwacht})",
            gevonden.unwrap_or("geen bouwcode gemeld")
        )))
    }

    /// Een protocolversie die niet past, wijst op een werkproces uit een andere uitgave.
    /// Bij een actieve controle is dat een [`EngineFout::NietBijgewerkt`], ook als we voor
    /// de bouwcode alleen waarschuwen: het protocol is dan echt niet te spreken. Zonder
    /// controle blijft het een gewone protocolfout.
    fn protocolfout(&self, versie: u32, gevonden: Option<&str>) -> EngineFout {
        match self {
            Self::Uit => EngineFout::Protocol(format!(
                "protocolversie {versie}, verwacht {PROTOCOL_VERSIE}"
            )),
            Self::Waarschuwen(verwacht) | Self::Afdwingen(verwacht) => EngineFout::NietBijgewerkt {
                gevonden: gevonden.map(str::to_string),
                verwacht: verwacht.clone(),
                protocol: versie,
            },
        }
    }
}

/// Instellingen van de supervisor.
#[derive(Debug, Clone)]
pub struct Instellingen {
    /// Pad naar het uitvoerbare bestand van het werkproces.
    pub engine_pad: PathBuf,
    /// Hoe lang we wachten op het eerste bericht na het starten.
    pub hallo_timeout: Duration,
    /// Hoe lang we wachten op het laden van een model.
    pub laad_timeout: Duration,
    /// Vaste ruimte voor het uitschrijven, boven op de tijd die de audio zelf kost.
    pub transcribeer_basis: Duration,
    /// Per seconde audio zoveel seconden uitschrijftijd extra toestaan.
    pub transcribeer_factor: u32,
    /// Extra omgevingsvariabelen voor het werkproces (naam, waarde). Normaal leeg.
    pub omgeving: Vec<(String, String)>,
    /// Welke taak het werkproces heeft; bepaalt de procesprioriteit. Standaard [`Rol::Dicteren`].
    pub rol: Rol,
    /// Hoe lang het werkproces na een annulering mag doen over zijn antwoord, voordat
    /// de supervisor het afbreekt.
    pub annuleer_wachttijd: Duration,
    /// Of het werkproces bij de app moet passen. Standaard [`Bouwcontrole::Uit`].
    pub bouwcontrole: Bouwcontrole,
}

impl Instellingen {
    /// Standaardwaarden (ADR 0009): 60 s voor het eerste bericht, 180 s voor laden en
    /// voor het uitschrijven 60 s plus vijf keer de duur van de audio.
    #[must_use]
    pub fn standaard(engine_pad: PathBuf) -> Self {
        Self {
            engine_pad,
            hallo_timeout: Duration::from_secs(60),
            laad_timeout: Duration::from_secs(180),
            transcribeer_basis: Duration::from_secs(60),
            transcribeer_factor: 5,
            omgeving: Vec::new(),
            rol: Rol::Dicteren,
            annuleer_wachttijd: Duration::from_secs(10),
            bouwcontrole: Bouwcontrole::Uit,
        }
    }

    /// Als [`Instellingen::standaard`], voor het achtergrondwerkproces (bestanden en
    /// vergaderingen, [`Rol::Achtergrond`]).
    #[must_use]
    pub fn achtergrond(engine_pad: PathBuf) -> Self {
        Self {
            rol: Rol::Achtergrond,
            ..Self::standaard(engine_pad)
        }
    }

    /// Hoe lang het uitschrijven van audio van deze duur maximaal mag duren.
    #[must_use]
    pub fn transcribeer_timeout(&self, audio_duur: Duration) -> Duration {
        self.transcribeer_basis
            .saturating_add(audio_duur.saturating_mul(self.transcribeer_factor))
    }
}

/// Welk model waarop geladen moet worden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaadOpdracht {
    /// Id uit de modelcatalogus.
    pub model_id: String,
    /// Map met de modelbestanden.
    pub modelmap: PathBuf,
    /// Gewenst apparaat.
    pub apparaat: Apparaat,
    /// Taalcode voor modellen die een taal nodig hebben.
    pub taal: Option<String>,
}

impl LaadOpdracht {
    fn als_verzoek(&self) -> Verzoek {
        Verzoek::Laad {
            model_id: self.model_id.clone(),
            modelmap: self.modelmap.clone(),
            apparaat: self.apparaat,
            taal: self.taal.clone(),
        }
    }

    fn op_processor(&self) -> Self {
        Self {
            apparaat: Apparaat::Cpu,
            ..self.clone()
        }
    }
}

/// De uitkomst van een transcriptie door het werkproces.
#[derive(Debug, Clone, PartialEq)]
pub struct Uitkomst {
    /// De tekst in stukken met tijdcodes.
    pub segmenten: Vec<Segment>,
    /// Duur van de audio in milliseconden.
    pub duur_ms: u64,
    /// Meting van de audio, voor het filter tegen verzonnen tekst.
    pub meting: Meting,
    /// Wat bij het lezen van het bestand opviel, zoals een afgebroken bestand.
    pub waarschuwing: Option<Leeswaarschuwing>,
}

/// Weigert geluid dat langer duurt dan [`MAX_DICTAAT`].
///
/// # Errors
///
/// Geeft [`EngineFout::TeLang`] als het geluid te lang is.
pub fn controleer_dictaatlengte(samples: &[f32]) -> Result<(), EngineFout> {
    if samples.len() > MAX_DICTAAT_SAMPLES {
        return Err(EngineFout::TeLang {
            duur: duur_van_samples(samples.len()),
            max: MAX_DICTAAT,
        });
    }
    Ok(())
}

/// Leest een stroom regel voor regel tot het einde (EOF) en geeft elke regel aan
/// `per_regel`; geeft die `false`, dan stopt het lezen.
///
/// Anders dan `BufRead::lines` stopt dit niet bij bytes die geen geldige UTF-8 zijn:
/// native bibliotheken kunnen dat schrijven, en een lezer die dan afbreekt laat stderr
/// volstromen (het proces blokkeert) of lijkt op een crash van stdout. Ongeldige bytes
/// worden vervangen door een vervangteken.
fn lees_regels(mut bron: impl BufRead, mut per_regel: impl FnMut(String) -> bool) {
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match bron.read_until(b'\n', &mut buffer) {
            Ok(0) => break,
            Ok(_) => {}
            Err(fout) if fout.kind() == ErrorKind::Interrupted => continue,
            // Een echte leesfout: de pipe is stuk, het proces is weg.
            Err(_) => break,
        }
        let tekst = String::from_utf8_lossy(&buffer);
        let regel = tekst.trim_end_matches(['\n', '\r']).to_string();
        if !per_regel(regel) {
            break;
        }
    }
}

/// Een draaiend werkproces met de kanalen eromheen.
struct Proces {
    kind: Child,
    stdin: Option<ChildStdin>,
    regels: Receiver<String>,
    log: Arc<Mutex<VecDeque<String>>>,
    lezers: Vec<JoinHandle<()>>,
}

impl Proces {
    fn start(instellingen: &Instellingen) -> Result<Self, EngineFout> {
        let (programma, argumenten) = startopdracht(instellingen.rol, &instellingen.engine_pad);
        let mut opdracht = Command::new(programma);
        opdracht
            .args(argumenten)
            .envs(instellingen.omgeving.iter().cloned())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            opdracht.creation_flags(creation_flags(instellingen.rol));
        }
        let mut kind = opdracht.spawn().map_err(|fout| EngineFout::Starten {
            reden: fout.to_string(),
        })?;

        let stdin = kind.stdin.take();
        let (Some(stdout), Some(stderr)) = (kind.stdout.take(), kind.stderr.take()) else {
            let _ = kind.kill();
            let _ = kind.wait();
            return Err(EngineFout::Starten {
                reden: "de kanalen naar het proces ontbreken".into(),
            });
        };

        // Eén draad leest stdout regel voor regel; het einde van de stroom (proces weg)
        // zien we als het kanaal sluit.
        let (zender, regels) = mpsc::channel();
        let stdout_lezer = thread::spawn(move || {
            lees_regels(BufReader::new(stdout), |regel| zender.send(regel).is_ok());
        });

        // Een andere draad leegt stderr, zodat het proces nooit op een vol kanaal blijft hangen.
        let log = Arc::new(Mutex::new(VecDeque::with_capacity(LOGREGELS)));
        let log_voor_lezer = Arc::clone(&log);
        let stderr_lezer = thread::spawn(move || {
            lees_regels(BufReader::new(stderr), |regel| {
                let mut log = log_voor_lezer
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                if log.len() == LOGREGELS {
                    log.pop_front();
                }
                log.push_back(regel);
                true
            });
        });

        Ok(Self {
            kind,
            stdin,
            regels,
            log,
            lezers: vec![stdout_lezer, stderr_lezer],
        })
    }

    fn stuur(&mut self, verzoek: &Verzoek) -> Result<(), EngineFout> {
        let regel = naar_regel(verzoek).map_err(|fout| EngineFout::Protocol(fout.to_string()))?;
        let geschreven = match self.stdin.as_mut() {
            Some(stdin) => writeln!(stdin, "{regel}").and_then(|()| stdin.flush()),
            None => Err(std::io::ErrorKind::BrokenPipe.into()),
        };
        if geschreven.is_err() {
            // Schrijven mislukt: het proces is weg.
            return Err(self.crash());
        }
        Ok(())
    }

    /// Wacht op het volgende protocolbericht. Regels die geen bericht zijn (bijvoorbeeld
    /// uitvoer van een native bibliotheek op stdout) worden overgeslagen; staat zulke
    /// uitvoer zonder regeleinde voor een bericht, dan wordt het bericht er toch uit gehaald.
    fn ontvang(&mut self, wachttijd: Duration, fase: Fase) -> Result<Antwoord, EngineFout> {
        let tot = Instant::now() + wachttijd;
        loop {
            let over = tot.saturating_duration_since(Instant::now());
            match self.regels.recv_timeout(over) {
                Ok(regel) => {
                    if let Some(antwoord) = zoek_bericht::<Antwoord>(&regel) {
                        return Ok(antwoord);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.afbreken();
                    return Err(EngineFout::TijdOverschreden {
                        fase,
                        na: wachttijd,
                    });
                }
                Err(RecvTimeoutError::Disconnected) => return Err(self.crash()),
            }
        }
    }

    /// Wacht op de uitkomst van een uitschrijving. Een [`Antwoord::Voortgang`] gaat naar
    /// `voortgang` en zet de klok terug op `wachttijd`. Wordt `annuleer` waar, dan gaat
    /// er één keer [`Verzoek::Annuleer`] heen en krijgt het werkproces nog hooguit
    /// `annuleer_wachttijd`; daarna wordt het afgebroken en is de uitkomst
    /// [`EngineFout::Geannuleerd`].
    fn ontvang_uitkomst(
        &mut self,
        wachttijd: Duration,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: Option<&AtomicBool>,
        annuleer_wachttijd: Duration,
    ) -> Result<Antwoord, EngineFout> {
        let mut tot = Instant::now() + wachttijd;
        let mut geannuleerd = false;
        loop {
            if !geannuleerd && annuleer.is_some_and(|vlag| vlag.load(Ordering::SeqCst)) {
                geannuleerd = true;
                if self.stuur(&Verzoek::Annuleer).is_err() {
                    // Het proces is al weg (`stuur` heeft opgeruimd); er valt niets af te breken.
                    return Err(EngineFout::Geannuleerd);
                }
                tot = tot.min(Instant::now() + annuleer_wachttijd);
            }
            let over = tot.saturating_duration_since(Instant::now());
            let stap = if annuleer.is_some() && !geannuleerd {
                over.min(PEILTIJD)
            } else {
                over
            };
            match self.regels.recv_timeout(stap) {
                Ok(regel) => match zoek_bericht::<Antwoord>(&regel) {
                    Some(Antwoord::Voortgang {
                        klaar_ms,
                        totaal_ms,
                    }) => {
                        voortgang(Voortgang {
                            klaar: Duration::from_millis(klaar_ms),
                            totaal: totaal_ms.map(Duration::from_millis),
                            stap: Stap::Uitschrijven,
                        });
                        if !geannuleerd {
                            tot = Instant::now() + wachttijd;
                        }
                    }
                    Some(antwoord) => return Ok(antwoord),
                    None => {}
                },
                Err(RecvTimeoutError::Timeout) if Instant::now() < tot => {}
                Err(RecvTimeoutError::Timeout) => {
                    self.afbreken();
                    return Err(if geannuleerd {
                        EngineFout::Geannuleerd
                    } else {
                        EngineFout::TijdOverschreden {
                            fase: Fase::Transcriberen,
                            na: wachttijd,
                        }
                    });
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let crash = self.crash();
                    return Err(if geannuleerd {
                        EngineFout::Geannuleerd
                    } else {
                        crash
                    });
                }
            }
        }
    }

    /// Breekt het proces af en wacht tot het weg is.
    fn afbreken(&mut self) -> Option<i32> {
        let _ = self.kind.kill();
        let status = self.kind.wait().ok();
        self.stdin = None;
        self.opruimen();
        status.and_then(|s| s.code())
    }

    /// Het proces is onverwacht weg: bouw de fout met afsluitcode en laatste logregels.
    fn crash(&mut self) -> EngineFout {
        let code = self.afbreken();
        let laatste_log = self
            .log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect();
        EngineFout::Gecrasht { code, laatste_log }
    }

    /// Vraagt het proces netjes te stoppen en breekt het af als het te lang duurt.
    fn stoppen(&mut self) {
        if self.stuur_stop().is_ok() {
            self.stdin = None; // sluit stdin; ook dat is voor het proces een stopsein
            let tot = Instant::now() + STOPTIJD;
            while Instant::now() < tot {
                match self.kind.try_wait() {
                    Ok(Some(_)) => {
                        self.opruimen();
                        return;
                    }
                    Ok(None) => thread::sleep(Duration::from_millis(20)),
                    Err(_) => break,
                }
            }
        }
        self.afbreken();
    }

    fn stuur_stop(&mut self) -> std::io::Result<()> {
        let regel = naar_regel(&Verzoek::Stop).map_err(std::io::Error::other)?;
        match self.stdin.as_mut() {
            Some(stdin) => writeln!(stdin, "{regel}").and_then(|()| stdin.flush()),
            None => Ok(()),
        }
    }

    /// Wacht tot de leesdraden klaar zijn (ze eindigen als het proces weg is).
    fn opruimen(&mut self) {
        for lezer in self.lezers.drain(..) {
            let _ = lezer.join();
        }
    }
}

/// Bestuurt één werkproces met de spraakengine.
pub struct Supervisor {
    instellingen: Instellingen,
    proces: Option<Proces>,
    geladen: Option<LaadOpdracht>,
}

impl Supervisor {
    /// Maakt een supervisor. Het werkproces start pas bij [`Supervisor::laad`].
    #[must_use]
    pub fn nieuw(instellingen: Instellingen) -> Self {
        Self {
            instellingen,
            proces: None,
            geladen: None,
        }
    }

    /// Het apparaat waarop het model nu geladen is, of `None` als er geen model geladen is.
    #[must_use]
    pub fn apparaat(&self) -> Option<Apparaat> {
        self.geladen.as_ref().map(|o| o.apparaat)
    }

    /// Laadt een model in het werkproces. Mislukt een poging op de videokaart, dan
    /// probeert de supervisor het één keer met alleen de processor.
    ///
    /// Geeft het apparaat terug waarop het model uiteindelijk draait.
    ///
    /// # Errors
    ///
    /// Geeft een [`EngineFout`] als het proces niet start, crasht, te lang duurt of
    /// het model niet laadt (ook niet op de processor).
    pub fn laad(&mut self, opdracht: &LaadOpdracht) -> Result<Apparaat, EngineFout> {
        match self.probeer_laden(opdracht) {
            Ok(()) => Ok(opdracht.apparaat),
            Err(fout) if opdracht.apparaat == Apparaat::Gpu && fout.kan_aan_apparaat_liggen() => {
                self.stop_proces();
                self.probeer_laden(&opdracht.op_processor())?;
                Ok(Apparaat::Cpu)
            }
            Err(fout) => Err(fout),
        }
    }

    /// Schrijft een audiobestand uit. Crasht of hangt het proces terwijl het model op de
    /// videokaart draait, dan laadt de supervisor het opnieuw op de processor en probeert
    /// het één keer opnieuw.
    ///
    /// `audio_duur` bepaalt hoe lang we maximaal wachten.
    ///
    /// # Errors
    ///
    /// Geeft [`EngineFout::NietGeladen`] als er nog geen model is geladen, anders de
    /// fout van het werkproces.
    pub fn transcribeer(
        &mut self,
        pad: &Path,
        audio_duur: Duration,
    ) -> Result<Uitkomst, EngineFout> {
        self.transcribeer_bestand(pad, Some(audio_duur), &mut |_| {}, &AtomicBool::new(false))
    }

    /// Schrijft een audiobestand uit en meldt na elk stuk de [`Voortgang`]. Zet de
    /// aanroeper `annuleer` op `true`, dan breekt het werkproces het bestand af en geeft
    /// dit [`EngineFout::Geannuleerd`]; het werkproces blijft daarna draaien. Is het
    /// bestand al klaar voordat de annulering aankomt, dan komt gewoon de uitkomst.
    ///
    /// `audio_duur` is de duur volgens de container, of `None` als die onbekend is. De
    /// wachttijd per bericht is die van één stuk ([`STUK_DUUR`]), of van het hele bestand
    /// als dat korter is. Meldt het werkproces geen totaal, dan telt `audio_duur` als
    /// totaal, zodat er een percentage is; zonder beide telt het werkproces zelf.
    ///
    /// # Errors
    ///
    /// Als [`Supervisor::transcribeer`], plus [`EngineFout::Geannuleerd`].
    pub fn transcribeer_bestand(
        &mut self,
        pad: &Path,
        audio_duur: Option<Duration>,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Uitkomst, EngineFout> {
        let wachttijd = self
            .instellingen
            .transcribeer_timeout(audio_duur.unwrap_or(STUK_DUUR).min(STUK_DUUR));
        let mut met_totaal = |stand: Voortgang| {
            voortgang(Voortgang {
                totaal: stand.totaal.or(audio_duur),
                ..stand
            });
        };
        self.transcribeer_verzoek(
            &Verzoek::Transcribeer {
                pad: pad.to_path_buf(),
            },
            wachttijd,
            &mut met_totaal,
            Some(annuleer),
        )
    }

    /// Schrijft een spoor van een vergaderopname uit per uiting (zie
    /// [`Verzoek::TranscribeerSpoor`]), met [`Voortgang`] na elk venster en de mogelijkheid
    /// om te annuleren, zoals [`Supervisor::transcribeer_bestand`].
    ///
    /// `spoor_duur` is de lengte van het spoor volgens het logboek. De wachttijd per bericht
    /// is die van één venster ([`uitingen::VENSTER`]), of van het hele spoor als dat korter
    /// is: het werkproces meldt zich na elk venster.
    ///
    /// # Errors
    ///
    /// Als [`Supervisor::transcribeer_bestand`].
    pub fn transcribeer_spoor(
        &mut self,
        map: &Path,
        spoor_duur: Duration,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Uitkomst, EngineFout> {
        let wachttijd = self
            .instellingen
            .transcribeer_timeout(spoor_duur.min(uitingen::VENSTER));
        let mut met_totaal = |stand: Voortgang| {
            voortgang(Voortgang {
                totaal: stand.totaal.or(Some(spoor_duur)),
                ..stand
            });
        };
        self.transcribeer_verzoek(
            &Verzoek::TranscribeerSpoor {
                map: map.to_path_buf(),
            },
            wachttijd,
            &mut met_totaal,
            Some(annuleer),
        )
    }

    /// Herkent de sprekers op een spoor van een vergaderopname (zie
    /// [`Verzoek::HerkenSprekers`]), met dezelfde terugval op de processor en hetzelfde
    /// annuleren als [`Supervisor::transcribeer_spoor`]. Het spraakmodel moet geladen zijn.
    ///
    /// Het werkproces meldt zich tijdens het lezen van het spoor; daarna rekent het sprekermodel
    /// in één keer over het hele spoor, zonder tussenbericht. De wachttijd geldt daarom voor
    /// het hele spoor (`spoor_duur`), net als bij een uitschrijving in één keer.
    ///
    /// # Errors
    ///
    /// Als [`Supervisor::transcribeer_spoor`]; ontbreekt het sprekermodel, dan een
    /// [`EngineFout::Engine`] met [`FoutSoort::ModelOntbreekt`].
    pub fn herken_sprekers(
        &mut self,
        map: &Path,
        spoor_duur: Duration,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Vec<SprekerSegment>, EngineFout> {
        let wachttijd = self.instellingen.transcribeer_timeout(spoor_duur);
        let antwoord = self.verzoek_met_terugval(
            &Verzoek::HerkenSprekers {
                map: map.to_path_buf(),
            },
            wachttijd,
            voortgang,
            Some(annuleer),
            |a| matches!(a, Antwoord::Sprekers { .. }),
        )?;
        match antwoord {
            Antwoord::Sprekers { segmenten } => Ok(segmenten),
            // `verzoek_met_terugval` laat alleen het verwachte antwoord door.
            ander => Err(EngineFout::Protocol(format!(
                "sprekers verwacht, kreeg {ander:?}"
            ))),
        }
    }

    /// Herkent de sprekers in een audiobestand (zie [`Verzoek::HerkenSprekersBestand`]), met
    /// dezelfde terugval op de processor en hetzelfde annuleren als
    /// [`Supervisor::herken_sprekers`]. `audio_duur` bepaalt hoe lang we maximaal wachten; het
    /// spraakmodel moet geladen zijn.
    ///
    /// # Errors
    ///
    /// Als [`Supervisor::herken_sprekers`].
    pub fn herken_sprekers_bestand(
        &mut self,
        pad: &Path,
        audio_duur: Duration,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Vec<SprekerSegment>, EngineFout> {
        let wachttijd = self.instellingen.transcribeer_timeout(audio_duur);
        let mut met_totaal = |stand: Voortgang| {
            voortgang(Voortgang {
                totaal: stand.totaal.or(Some(audio_duur)),
                ..stand
            });
        };
        let antwoord = self.verzoek_met_terugval(
            &Verzoek::HerkenSprekersBestand {
                pad: pad.to_path_buf(),
            },
            wachttijd,
            &mut met_totaal,
            Some(annuleer),
            |a| matches!(a, Antwoord::Sprekers { .. }),
        )?;
        match antwoord {
            Antwoord::Sprekers { segmenten } => Ok(segmenten),
            ander => Err(EngineFout::Protocol(format!(
                "sprekers verwacht, kreeg {ander:?}"
            ))),
        }
    }

    /// Schrijft geluid uit het geheugen uit (16 kHz mono), met dezelfde terugval op de
    /// processor als [`Supervisor::transcribeer`]. De wachttijd volgt uit de duur van
    /// het geluid (samples gedeeld door 16000).
    ///
    /// # Errors
    ///
    /// Geeft [`EngineFout::TeLang`] bij meer dan [`MAX_DICTAAT`] (zonder het werkproces
    /// aan te raken), [`EngineFout::NietGeladen`] als er nog geen model is geladen, anders
    /// de fout van het werkproces.
    ///
    /// Met `knip_stilte` knipt het werkproces stilte aan begin en eind weg (Silero);
    /// zie [`Verzoek::TranscribeerGeluid`].
    pub fn transcribeer_geluid(
        &mut self,
        samples: &[f32],
        knip_stilte: bool,
    ) -> Result<Uitkomst, EngineFout> {
        controleer_dictaatlengte(samples)?;
        // Eén keer coderen: bij een nieuwe poging op de processor gaat dezelfde regel nog eens.
        let verzoek = Verzoek::TranscribeerGeluid {
            samples: samples_naar_base64(samples),
            knip_stilte,
        };
        let wachttijd = self
            .instellingen
            .transcribeer_timeout(duur_van_samples(samples.len()));
        self.transcribeer_verzoek(&verzoek, wachttijd, &mut |_| {}, None)
    }

    /// De gedeelde route voor elk uitschrijfverzoek, met de nieuwe poging op de processor.
    fn transcribeer_verzoek(
        &mut self,
        verzoek: &Verzoek,
        wachttijd: Duration,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: Option<&AtomicBool>,
    ) -> Result<Uitkomst, EngineFout> {
        let antwoord = self.verzoek_met_terugval(verzoek, wachttijd, voortgang, annuleer, |a| {
            matches!(a, Antwoord::Resultaat { .. })
        })?;
        match antwoord {
            Antwoord::Resultaat {
                segmenten,
                duur_ms,
                meting,
                waarschuwing,
            } => Ok(Uitkomst {
                segmenten,
                duur_ms,
                meting,
                waarschuwing,
            }),
            // `verzoek_met_terugval` laat alleen het verwachte antwoord door.
            ander => Err(EngineFout::Protocol(format!(
                "resultaat verwacht, kreeg {ander:?}"
            ))),
        }
    }

    /// Stuurt een verzoek dat een geladen model nodig heeft, met de nieuwe poging op de
    /// processor als de videokaart faalde. `verwacht` zegt welk antwoord erbij hoort; een
    /// ander antwoord is een protocolfout.
    fn verzoek_met_terugval(
        &mut self,
        verzoek: &Verzoek,
        wachttijd: Duration,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: Option<&AtomicBool>,
        verwacht: fn(&Antwoord) -> bool,
    ) -> Result<Antwoord, EngineFout> {
        let Some(opdracht) = self.geladen.clone() else {
            return Err(EngineFout::NietGeladen);
        };
        // Een eerdere crash heeft het proces weggehaald: start opnieuw met hetzelfde model.
        if self.proces.is_none() {
            self.laad(&opdracht)?;
        }
        match self.eenmaal_vragen(verzoek, wachttijd, voortgang, annuleer, verwacht) {
            Err(fout)
                if self.apparaat() == Some(Apparaat::Gpu) && fout.kan_aan_apparaat_liggen() =>
            {
                self.stop_proces();
                self.probeer_laden(&opdracht.op_processor())?;
                self.eenmaal_vragen(verzoek, wachttijd, voortgang, annuleer, verwacht)
            }
            overig => overig,
        }
    }

    /// Draait het werkproces nu? Na een crash niet, ook al is er nog een model onthouden
    /// dat bij de volgende opdracht opnieuw geladen wordt.
    #[must_use]
    pub fn draait(&self) -> bool {
        self.proces.is_some()
    }

    /// Stopt het werkproces. Het model is daarna ontladen.
    pub fn stop(&mut self) {
        self.stop_proces();
        self.geladen = None;
    }

    fn stop_proces(&mut self) {
        if let Some(mut proces) = self.proces.take() {
            proces.stoppen();
        }
    }

    /// Eén poging om te laden, zo nodig inclusief het starten van het proces.
    fn probeer_laden(&mut self, opdracht: &LaadOpdracht) -> Result<(), EngineFout> {
        self.geladen = None;
        let resultaat = self.laden_in_proces(opdracht);
        match resultaat {
            Ok(()) => self.geladen = Some(opdracht.clone()),
            Err(ref fout) => {
                // Na een crash of time-out is het proces al weg; bij een engine-fout
                // laten we het niet met een half geladen model draaien.
                if !matches!(fout, EngineFout::NietGeladen) {
                    self.stop_proces();
                }
            }
        }
        resultaat
    }

    /// Start het werkproces als het niet draait en wacht op zijn hallo met de juiste
    /// protocolversie.
    fn zorg_voor_proces(&mut self) -> Result<(), EngineFout> {
        if self.proces.is_some() {
            return Ok(());
        }
        let mut proces = Proces::start(&self.instellingen)?;
        let antwoord = proces.ontvang(self.instellingen.hallo_timeout, Fase::Opstarten)?;
        if let Antwoord::Hallo { versie, bouw } = &antwoord {
            *GEMELDE_BOUW.lock().unwrap_or_else(PoisonError::into_inner) =
                Some(beschrijf_hallo(bouw.as_deref(), *versie));
        }
        match antwoord {
            Antwoord::Hallo { versie, bouw } if versie == PROTOCOL_VERSIE => {
                match self.instellingen.bouwcontrole.beoordeel(bouw.as_deref()) {
                    Ok(None) => {}
                    Ok(Some(waarschuwing)) => eprintln!("[mondig] waarschuwing: {waarschuwing}"),
                    Err(fout) => {
                        proces.stoppen();
                        return Err(fout);
                    }
                }
            }
            Antwoord::Hallo { versie, bouw } => {
                proces.stoppen();
                return Err(self
                    .instellingen
                    .bouwcontrole
                    .protocolfout(versie, bouw.as_deref()));
            }
            ander => {
                proces.stoppen();
                return Err(EngineFout::Protocol(format!(
                    "eerst een hallo verwacht, kreeg {ander:?}"
                )));
            }
        }
        self.proces = Some(proces);
        Ok(())
    }

    /// Start het werkproces, wacht tot het zich meldt met de juiste protocolversie en
    /// stopt het weer. Laadt geen model; bedoeld voor de rooktest van de app.
    ///
    /// # Errors
    ///
    /// Geeft een [`EngineFout`] als het proces niet start, te lang doet over zijn eerste
    /// bericht of een andere protocolversie spreekt.
    pub fn controleer_start(&mut self) -> Result<(), EngineFout> {
        let resultaat = self.zorg_voor_proces();
        self.stop();
        resultaat
    }

    /// Vraagt een vers werkproces welke videokaarten het ziet ([`Verzoek::Apparaten`]) en
    /// stopt het daarna: het proces heeft de videokaart-backend geregistreerd, en een model
    /// op de processor hoort in een proces zonder (ADR 0011). Een geladen model is daarna
    /// ontladen. Crasht de driver, dan raakt dat alleen dit werkproces (ADR 0009).
    ///
    /// # Errors
    ///
    /// Geeft een [`EngineFout`] als het proces niet start, crasht, te lang doet (de
    /// laadtijd geldt als grens) of iets anders antwoordt.
    pub fn apparaten(&mut self) -> Result<Vec<Videokaart>, EngineFout> {
        self.stop();
        let resultaat = self.vraag_apparaten();
        self.stop();
        resultaat
    }

    fn vraag_apparaten(&mut self) -> Result<Vec<Videokaart>, EngineFout> {
        self.zorg_voor_proces()?;
        let wachttijd = self.instellingen.laad_timeout;
        let proces = self.proces.as_mut().ok_or(EngineFout::NietGeladen)?;
        proces.stuur(&Verzoek::Apparaten)?;
        match proces.ontvang(wachttijd, Fase::Apparaten)? {
            Antwoord::Apparaten { videokaarten } => Ok(videokaarten),
            Antwoord::Fout { soort, melding } => Err(EngineFout::Engine { soort, melding }),
            ander => Err(EngineFout::Protocol(format!(
                "apparaten verwacht, kreeg {ander:?}"
            ))),
        }
    }

    fn laden_in_proces(&mut self, opdracht: &LaadOpdracht) -> Result<(), EngineFout> {
        self.zorg_voor_proces()?;
        let laad_timeout = self.instellingen.laad_timeout;
        let proces = self.proces.as_mut().ok_or(EngineFout::NietGeladen)?;
        proces.stuur(&opdracht.als_verzoek())?;
        match proces.ontvang(laad_timeout, Fase::Laden)? {
            Antwoord::Geladen => Ok(()),
            Antwoord::Fout { soort, melding } => Err(EngineFout::Engine { soort, melding }),
            ander => Err(EngineFout::Protocol(format!(
                "geladen verwacht, kreeg {ander:?}"
            ))),
        }
    }

    fn eenmaal_vragen(
        &mut self,
        verzoek: &Verzoek,
        wachttijd: Duration,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: Option<&AtomicBool>,
        verwacht: fn(&Antwoord) -> bool,
    ) -> Result<Antwoord, EngineFout> {
        let annuleer_wachttijd = self.instellingen.annuleer_wachttijd;
        let proces = self.proces.as_mut().ok_or(EngineFout::NietGeladen)?;
        let antwoord = proces.stuur(verzoek).and_then(|()| {
            proces.ontvang_uitkomst(wachttijd, voortgang, annuleer, annuleer_wachttijd)
        });
        match antwoord {
            // Het werkproces brak het bestand af en draait nog: klaar voor het volgende.
            Ok(Antwoord::Geannuleerd) => Err(EngineFout::Geannuleerd),
            Ok(antwoord) if verwacht(&antwoord) => Ok(antwoord),
            Ok(Antwoord::Fout { soort, melding }) => Err(EngineFout::Engine { soort, melding }),
            Ok(ander) => {
                // Vraag en antwoord liepen uit de pas: een volgend antwoord zou bij de
                // verkeerde vraag horen. Stop het proces, zoals het laadpad ook doet.
                self.stop_proces();
                Err(EngineFout::Protocol(format!(
                    "antwoord verwacht bij {}, kreeg {ander:?}",
                    soort_van(verzoek)
                )))
            }
            Err(fout) => {
                // Bij een crash of time-out is het proces al afgebroken; ruim de rest op.
                self.stop_proces();
                Err(fout)
            }
        }
    }
}

/// De naam van een verzoek voor een foutmelding (nooit met inhoud).
fn soort_van(verzoek: &Verzoek) -> &'static str {
    match verzoek {
        Verzoek::Laad { .. } => "laad",
        Verzoek::Transcribeer { .. } => "transcribeer",
        Verzoek::TranscribeerGeluid { .. } => "transcribeer_geluid",
        Verzoek::TranscribeerSpoor { .. } => "transcribeer_spoor",
        Verzoek::HerkenSprekers { .. } => "herken_sprekers",
        Verzoek::HerkenSprekersBestand { .. } => "herken_sprekers_bestand",
        Verzoek::Annuleer => "annuleer",
        Verzoek::Apparaten => "apparaten",
        Verzoek::Stop => "stop",
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop_proces();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictaatgrens_is_tien_minuten() {
        assert_eq!(MAX_DICTAAT_SAMPLES, 9_600_000);
        assert!(controleer_dictaatlengte(&vec![0.0; MAX_DICTAAT_SAMPLES]).is_ok());
        let fout = controleer_dictaatlengte(&vec![0.0; MAX_DICTAAT_SAMPLES + 1]).unwrap_err();
        assert!(matches!(fout, EngineFout::TeLang { .. }));
        assert!(!fout.kan_aan_apparaat_liggen());
        assert!(fout.to_string().contains("10 minuten"), "{fout}");
    }

    const BOUW: &str = "0.3.0-dev+afd2907";

    #[test]
    fn dezelfde_bouwcode_is_goed() {
        for controle in [
            Bouwcontrole::Afdwingen(BOUW.into()),
            Bouwcontrole::Waarschuwen(BOUW.into()),
        ] {
            assert!(matches!(controle.beoordeel(Some(BOUW)), Ok(None)));
        }
    }

    #[test]
    fn andere_bouwcode_is_een_fout_bij_afdwingen() {
        let controle = Bouwcontrole::Afdwingen(BOUW.into());
        let fout = controle.beoordeel(Some("0.3.0-dev+1234567")).unwrap_err();
        assert!(
            matches!(&fout, EngineFout::NietBijgewerkt { gevonden: Some(g), verwacht, protocol }
                if g == "0.3.0-dev+1234567" && verwacht == BOUW && *protocol == PROTOCOL_VERSIE),
            "{fout:?}"
        );
        assert!(!fout.kan_aan_apparaat_liggen());
        assert!(
            fout.to_string().contains("niet volledig bijgewerkt"),
            "{fout}"
        );
    }

    #[test]
    fn oud_werkproces_zonder_bouwcode_is_ook_een_verkeerde_versie() {
        let fout = Bouwcontrole::Afdwingen(BOUW.into())
            .beoordeel(None)
            .unwrap_err();
        assert!(
            matches!(fout, EngineFout::NietBijgewerkt { gevonden: None, .. }),
            "{fout:?}"
        );
    }

    #[test]
    fn in_ontwikkeling_is_een_verschil_alleen_een_waarschuwing() {
        let controle = Bouwcontrole::Waarschuwen(BOUW.into());
        let waarschuwing = controle.beoordeel(None).unwrap().unwrap();
        assert!(
            waarschuwing.contains("geen bouwcode gemeld"),
            "{waarschuwing}"
        );
        assert!(
            controle
                .beoordeel(Some("0.3.0-dev+1234567"))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn zonder_controle_past_alles() {
        assert!(matches!(Bouwcontrole::Uit.beoordeel(None), Ok(None)));
        assert!(matches!(
            Bouwcontrole::Uit.protocolfout(3, None),
            EngineFout::Protocol(_)
        ));
    }

    #[test]
    fn debugbuild_en_dirty_waarschuwen_een_uitgave_dwingt_af() {
        assert!(matches!(
            Bouwcontrole::voor_app(BOUW, true),
            Bouwcontrole::Waarschuwen(_)
        ));
        assert!(matches!(
            Bouwcontrole::voor_app("0.3.0-dev+afd2907-dirty", false),
            Bouwcontrole::Waarschuwen(_)
        ));
        assert_eq!(
            Bouwcontrole::voor_app(BOUW, false),
            Bouwcontrole::Afdwingen(BOUW.into())
        );
    }

    #[test]
    fn andere_protocolversie_is_niet_bijgewerkt_zodra_er_gecontroleerd_wordt() {
        let fout = Bouwcontrole::Waarschuwen(BOUW.into()).protocolfout(3, Some("0.2.0+oud1234"));
        assert!(
            matches!(&fout, EngineFout::NietBijgewerkt { gevonden: Some(g), protocol: 3, .. }
                if g == "0.2.0+oud1234"),
            "{fout:?}"
        );
    }

    #[test]
    fn een_dirty_werkproces_waarschuwt_ook_bij_een_schone_app() {
        let controle = Bouwcontrole::Afdwingen(BOUW.into());
        let waarschuwing = controle
            .beoordeel(Some("0.3.0-dev+afd2907-dirty"))
            .unwrap()
            .unwrap();
        assert!(waarschuwing.contains("-dirty"), "{waarschuwing}");
        // Een schoon werkproces uit een andere commit blijft een fout.
        assert!(controle.beoordeel(Some("0.3.0-dev+1234567")).is_err());
    }

    #[test]
    fn de_hallo_wordt_leesbaar_beschreven() {
        assert_eq!(
            beschrijf_hallo(Some(BOUW), 4),
            "0.3.0-dev+afd2907 (protocol 4)"
        );
        assert_eq!(
            beschrijf_hallo(None, 3),
            "geen bouwcode gemeld (protocol 3)"
        );
    }

    #[cfg(windows)]
    #[test]
    fn achtergrond_start_met_lagere_prioriteit() {
        assert_eq!(creation_flags(Rol::Dicteren), 0x0800_0000);
        assert_eq!(creation_flags(Rol::Achtergrond), 0x0800_4000);
        assert_eq!(Instellingen::standaard(PathBuf::new()).rol, Rol::Dicteren);
    }

    #[test]
    fn voortgang_in_procenten() {
        let v = |klaar, totaal: Option<u64>| Voortgang {
            klaar: Duration::from_secs(klaar),
            totaal: totaal.map(Duration::from_secs),
            stap: Stap::Uitschrijven,
        };
        assert_eq!(v(0, Some(3600)).procent(), Some(0));
        assert_eq!(v(1800, Some(3600)).procent(), Some(50));
        assert_eq!(v(3599, Some(3600)).procent(), Some(99));
        // Een container die de duur iets te kort opgeeft: niet boven de 100.
        assert_eq!(v(3700, Some(3600)).procent(), Some(100));
        assert_eq!(v(10, None).procent(), None);
        assert_eq!(v(0, Some(0)).procent(), Some(100));
    }

    #[test]
    fn dicteren_start_het_werkproces_rechtstreeks() {
        let pad = PathBuf::from("mondig-engine");
        assert_eq!(startopdracht(Rol::Dicteren, &pad), (pad, Vec::new()));
        assert_eq!(
            Instellingen::achtergrond(PathBuf::new()).rol,
            Rol::Achtergrond
        );
    }

    #[cfg(unix)]
    #[test]
    fn achtergrond_gaat_op_mac_en_linux_via_nice() {
        let pad = PathBuf::from("/opt/mondig/mondig-engine");
        let (programma, argumenten) = startopdracht(Rol::Achtergrond, &pad);
        if Path::new(NICE).is_file() {
            assert_eq!(programma, PathBuf::from(NICE));
            assert_eq!(argumenten, vec!["-n", "10", "/opt/mondig/mondig-engine"]);
        } else {
            assert_eq!(programma, pad);
        }
    }

    #[cfg(windows)]
    #[test]
    fn achtergrond_start_op_windows_rechtstreeks_met_de_prioriteitsvlag() {
        let pad = PathBuf::from("mondig-engine.exe");
        assert_eq!(startopdracht(Rol::Achtergrond, &pad), (pad, Vec::new()));
    }

    #[test]
    fn lees_regels_gaat_door_na_ongeldige_utf8() {
        let invoer: &[u8] = b"eerste\nkapot \xff\xfe stuk\r\nlaatste zonder einde";
        let mut regels = Vec::new();
        lees_regels(invoer, |r| {
            regels.push(r);
            true
        });
        assert_eq!(
            regels,
            vec![
                "eerste",
                "kapot \u{fffd}\u{fffd} stuk",
                "laatste zonder einde"
            ]
        );
    }

    #[test]
    fn lees_regels_stopt_als_de_ontvanger_klaar_is() {
        let mut aantal = 0;
        lees_regels(&b"a\nb\nc\n"[..], |_| {
            aantal += 1;
            false
        });
        assert_eq!(aantal, 1);
    }
}
