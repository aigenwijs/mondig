//! Nep-werkproces voor tests van de supervisor. Wordt niet meegeleverd: het zit achter
//! de Cargo-feature `nep` (standaard uit), dus een gewone `cargo build --release`
//! bouwt het niet mee. Tests draaien met `--features nep` of `--all-features`.
//!
//! Het gedrag kies je met de omgevingsvariabele `MONDIG_NEP`:
//!
//! - `ok` (standaard): antwoordt altijd netjes; de tekst noemt het apparaat.
//! - `crash_laad` / `crash_transcribeer`: sluit abrupt af bij die opdracht.
//! - `crash_laad_gpu` / `crash_transcribeer_gpu`: idem, maar alleen op `Gpu`.
//! - `hang_hallo` / `hang_laad` / `hang_transcribeer`: blijft hangen.
//! - `fout_laad` / `fout_transcribeer` / `fout_audio`: stuurt een `Fout`.
//! - `rommel`: schrijft eerst regels die geen bericht zijn op stdout.
//! - `fragment`: schrijft voor elk antwoord tekst zonder regeleinde op stdout, zoals een
//!   native bibliotheek dat kan doen.
//! - `ongeldig_utf8`: schrijft bytes die geen geldige UTF-8 zijn op stdout en stderr.
//! - `verkeerd_antwoord`: antwoordt op een transcriptie met `Geladen`.
//! - `ontbreekt_gpu`: meldt `ModelOntbreekt` bij laden op `Gpu` (op `Cpu` gaat het goed).
//! - `fout_laad_gpu`: meldt `ModelLaden` bij laden op `Gpu` (op `Cpu` gaat het goed).
//! - `stil`: antwoordt met een stille meting en een verzonnen stiltezin.
//! - `geen_spraak`: antwoordt op een uitschrijving met `knip_stilte` zonder segmenten, zoals
//!   het echte werkproces als Silero geen spraak vindt.
//! - `foute_versie`: meldt een andere protocolversie.
//! - `oud_werkproces`: meldt zich zonder bouwcode, zoals een werkproces van een oudere
//!   uitgave.
//! - `andere_bouw`: meldt een bouwcode van een andere commit.
//! - `traag_transcribeer`: antwoordt netjes, maar pas na 400 ms (voor de test dat de
//!   rusttijd niet loopt tijdens het uitschrijven).
//! - `stukken`: schrijft een bestand uit in `MONDIG_NEP_STUKKEN` stukken (standaard 5)
//!   van elk een seconde audio, met `MONDIG_NEP_STUKTIJD_MS` rekentijd per stuk
//!   (standaard 50), en een voortgang na elk stuk. Luistert naar annuleren.
//! - `stukken_doof`: als `stukken`, maar negeert annuleren (voor de test dat de
//!   supervisor het proces dan afbreekt).
//! - `stukken_fout_audio`: meldt na het tweede stuk dat het bestand beschadigd is.
//!   Met `MONDIG_NEP_ZONDER_TOTAAL` sturen de stukken geen totaal mee.
//! - Op `Apparaten`: `geen_kaart` meldt geen videokaart, `ingebouwd` een ingebouwde chip,
//!   `crash_apparaten` crasht; elke andere modus meldt een losse kaart.
//!
//! Een spoor van een vergadering (`TranscribeerSpoor`) leest het nep-werkproces echt, met
//! de spoorlezer uit `mondig-core`: elke seconde geluid boven -40 dBFS wordt een segment met
//! als tekst de naam van de spoormap en het volgnummer van de seconde (`microfoon 3`), in de
//! stroom van het spoor (zoals het echte werkproces). Na elke seconde komt een voortgang, met
//! `MONDIG_NEP_STUKTIJD_MS` rekentijd per seconde (standaard 0); annuleren werkt. Met
//! `geen_silero` komt er een fout zoals bij het echte werkproces zonder stiltedetectie, en
//! `crash_transcribeer` en `hang_transcribeer` gelden ook hier.
//!
//! Sprekers herkennen (`HerkenSprekers`) leest het spoor op dezelfde manier: elke seconde met
//! geluid wordt een sprekersegment, en de spreker wisselt om de twee seconden tussen 1 en 2
//! (seconde 0 en 1 spreker 1, seconde 2 en 3 spreker 2, enzovoort). Met `een_spreker` is het
//! steeds spreker 1, `fout_sprekers` meldt dat het sprekermodel faalde (ook op de processor), `crash_sprekers_gpu`
//! crasht alleen op `Gpu` (voor de terugval op de processor) en `geen_sprekermodel` meldt
//! `ModelOntbreekt`. `MONDIG_NEP_STUKTIJD_MS` geldt per seconde (standaard 50); annuleren werkt. Op een bestand
//! (`HerkenSprekersBestand`) geeft het nep-werkproces een spreker per seconde van de
//! `MONDIG_NEP_STUKKEN` (standaard 5) seconden, met dezelfde wisseling en dezelfde modi.
//!
//! Met `MONDIG_NEP_PIDBESTAND` schrijft het proces bij het starten zijn proces-id in dat
//! bestand, zodat een test kan zien of een tweede bestand hetzelfde proces gebruikte.
//!
//! Met `MONDIG_NEP_TIJDLIJN` schrijft de modus `stukken` per stuk een regel met het tijdstip
//! (milliseconden sinds 1970), of de annulering al gezien is en hoe lang het slapen echt
//! duurde. Zo kan een falende test laten zien waar de tijd bleef.
//!
//! Een uitschrijving uit het geheugen (`TranscribeerGeluid`) volgt dezelfde modi als een
//! uitschrijving van een bestand.
//!
//! Als `MONDIG_NEP_STOPBESTAND` gezet is, schrijft het proces dat bestand wanneer het
//! een `Stop` krijgt of stdin sluit. Zo kan een test zien dat het netjes gestopt is.

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use mondig_core::engine::protocol::{
    Antwoord, Apparaat, FoutSoort, KaartSoort, PROTOCOL_VERSIE, Segment, SprekerSegment, Videokaart,
};
use mondig_core::filter::Meting;
use mondig_core::spoor::Spoorlezer;
use mondig_engine::{Annulering, Werker, draai, draai_met_hallo, draai_met_versie, stuur};

struct Nep {
    modus: String,
    apparaat: Apparaat,
}

fn hang() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

fn crash() -> ! {
    // Een abrupte exit lijkt op een driver-crash, zonder Windows-foutdialoog.
    eprintln!("[nep] simulatie van een crash");
    std::process::exit(139)
}

/// Schrijft ruwe bytes op stdout, zonder regeleinde of nette opmaak.
fn ruw_op_stdout(bytes: &[u8]) {
    use std::io::Write;
    let mut uit = std::io::stdout().lock();
    let _ = uit.write_all(bytes);
    let _ = uit.flush();
}

/// Schrijft ruwe bytes op stderr.
fn ruw_op_stderr(bytes: &[u8]) {
    use std::io::Write;
    let mut uit = std::io::stderr().lock();
    let _ = uit.write_all(bytes);
    let _ = uit.flush();
}

/// Een meting van een duidelijk hoorbare opname.
fn luide_meting() -> Meting {
    Meting {
        rms: 0.1,
        frame_rms: vec![0.1; 34],
    }
}

impl Nep {
    /// Uitvoer die een native bibliotheek zou kunnen schrijven, voor elk antwoord.
    fn vervuil(&self) {
        match self.modus.as_str() {
            "fragment" => ruw_op_stdout(b"bibliotheek zegt iets zonder regeleinde "),
            "ongeldig_utf8" => {
                ruw_op_stdout(b"kapot \xff\xfe op stdout\n");
                ruw_op_stderr(b"kapot \xff\xfe op stderr\n");
            }
            _ => {}
        }
    }
}

impl Werker for Nep {
    fn laad(
        &mut self,
        _id: &str,
        _map: &Path,
        apparaat: Apparaat,
        _taal: Option<&str>,
    ) -> Antwoord {
        self.apparaat = apparaat;
        eprintln!("[nep] laad op {apparaat:?}");
        self.vervuil();
        match self.modus.as_str() {
            "ontbreekt_gpu" if apparaat == Apparaat::Gpu => Antwoord::Fout {
                soort: FoutSoort::ModelOntbreekt,
                melding: "Nep: het model ontbreekt.".into(),
            },
            "fout_laad_gpu" if apparaat == Apparaat::Gpu => Antwoord::Fout {
                soort: FoutSoort::ModelLaden,
                melding: "Nep: de videokaart wil het model niet laden.".into(),
            },
            "crash_laad" => crash(),
            "crash_laad_gpu" if apparaat == Apparaat::Gpu => crash(),
            "hang_laad" => hang(),
            "fout_laad" => Antwoord::Fout {
                soort: FoutSoort::ModelLaden,
                melding: "Nep: het model kon niet geladen worden.".into(),
            },
            _ => Antwoord::Geladen,
        }
    }

    fn transcribeer(&mut self, _pad: &Path, annulering: &Annulering) -> Antwoord {
        match self.modus.as_str() {
            "stukken" => Self::in_stukken(Some(annulering), false),
            "stukken_doof" => Self::in_stukken(None, false),
            "stukken_fout_audio" => Self::in_stukken(Some(annulering), true),
            _ => self.uitschrijven(false),
        }
    }

    fn transcribeer_spoor(&mut self, map: &Path, annulering: &Annulering) -> Antwoord {
        match self.modus.as_str() {
            "geen_silero" => Antwoord::Fout {
                soort: FoutSoort::ModelOntbreekt,
                melding: "Nep: de stiltedetectie staat niet klaar.".into(),
            },
            "crash_transcribeer" => crash(),
            "hang_transcribeer" => hang(),
            _ => Self::spoor(map, annulering),
        }
    }

    fn herken_sprekers(&mut self, map: &Path, annulering: &Annulering) -> Antwoord {
        match self.modus.as_str() {
            "geen_sprekermodel" => Antwoord::Fout {
                soort: FoutSoort::ModelOntbreekt,
                melding: "Nep: het sprekermodel staat niet klaar.".into(),
            },
            "fout_sprekers" => Antwoord::Fout {
                soort: FoutSoort::Sprekermodel,
                melding: "Nep: het herkennen van de sprekers is mislukt.".into(),
            },
            "crash_sprekers_gpu" if self.apparaat == Apparaat::Gpu => crash(),
            _ => Self::sprekers(map, annulering, self.modus == "een_spreker"),
        }
    }

    fn herken_sprekers_bestand(&mut self, pad: &Path, annulering: &Annulering) -> Antwoord {
        match self.modus.as_str() {
            "geen_sprekermodel" => Antwoord::Fout {
                soort: FoutSoort::ModelOntbreekt,
                melding: "Nep: het sprekermodel staat niet klaar.".into(),
            },
            "fout_sprekers" => Antwoord::Fout {
                soort: FoutSoort::Sprekermodel,
                melding: "Nep: het herkennen van de sprekers is mislukt.".into(),
            },
            "crash_sprekers_gpu" if self.apparaat == Apparaat::Gpu => crash(),
            _ => Self::sprekers_bestand(pad, annulering, self.modus == "een_spreker"),
        }
    }

    fn transcribeer_geluid(&mut self, _samples: &[f32], knip_stilte: bool) -> Antwoord {
        self.uitschrijven(knip_stilte)
    }

    fn apparaten(&mut self) -> Antwoord {
        let kaart = |naam: &str, soort| Videokaart {
            naam: naam.into(),
            soort,
        };
        match self.modus.as_str() {
            "geen_kaart" => Antwoord::Apparaten {
                videokaarten: Vec::new(),
            },
            "ingebouwd" => Antwoord::Apparaten {
                videokaarten: vec![kaart("Nep ingebouwde chip", KaartSoort::Ingebouwd)],
            },
            "crash_apparaten" => crash(),
            _ => Antwoord::Apparaten {
                videokaarten: vec![kaart("Nep losse kaart", KaartSoort::Los)],
            },
        }
    }
}

/// Voegt een regel toe aan het bestand uit `MONDIG_NEP_TIJDLIJN`, met het tijdstip in
/// milliseconden sinds 1970 ervoor (dezelfde klok als de test, zodat die de regels naast
/// zijn eigen tijdstippen kan leggen).
fn tijdlijn(tekst: &str) {
    use std::io::Write;
    let Some(pad) = std::env::var_os("MONDIG_NEP_TIJDLIJN") else {
        return;
    };
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    if let Ok(mut bestand) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(pad)
    {
        let _ = writeln!(bestand, "{ms} {tekst}");
    }
}

/// Een getal uit een omgevingsvariabele, of de standaardwaarde.
fn getal(naam: &str, standaard: u64) -> u64 {
    std::env::var(naam)
        .ok()
        .and_then(|w| w.parse().ok())
        .unwrap_or(standaard)
}

impl Nep {
    /// Een bestand in stukken van een seconde, met een voortgang na elk stuk. Zonder
    /// `annulering` wordt annuleren genegeerd. Met `fout_na_twee` faalt het derde stuk.
    fn in_stukken(annulering: Option<&Annulering>, fout_na_twee: bool) -> Antwoord {
        let stukken = getal("MONDIG_NEP_STUKKEN", 5);
        let stuktijd = Duration::from_millis(getal("MONDIG_NEP_STUKTIJD_MS", 50));
        let mut segmenten = Vec::new();
        for stuk in 0..stukken {
            let geannuleerd = annulering.is_some_and(Annulering::is_geannuleerd);
            tijdlijn(&format!(
                "stuk {} begint, geannuleerd: {geannuleerd}",
                stuk + 1
            ));
            if geannuleerd {
                return Antwoord::Geannuleerd;
            }
            if fout_na_twee && stuk == 2 {
                return Antwoord::Fout {
                    soort: FoutSoort::Audio,
                    melding: "Nep: het bestand is halverwege beschadigd.".into(),
                };
            }
            let slaap = std::time::Instant::now();
            std::thread::sleep(stuktijd);
            tijdlijn(&format!(
                "stuk {} geslapen: {} ms",
                stuk + 1,
                slaap.elapsed().as_millis()
            ));
            segmenten.push(Segment::nieuw(
                stuk * 1000,
                (stuk + 1) * 1000,
                format!("stuk {}", stuk + 1),
            ));
            // Met MONDIG_NEP_ZONDER_TOTAAL zoals een bestand zonder duur in de container.
            let zonder_totaal = std::env::var_os("MONDIG_NEP_ZONDER_TOTAAL").is_some();
            let voortgang = Antwoord::Voortgang {
                klaar_ms: (stuk + 1) * 1000,
                totaal_ms: (!zonder_totaal).then_some(stukken * 1000),
            };
            if stuur(&voortgang).is_err() {
                return Antwoord::Geannuleerd;
            }
        }
        Antwoord::Resultaat {
            waarschuwing: None,
            segmenten,
            duur_ms: stukken * 1000,
            meting: Meting {
                rms: 0.1,
                frame_rms: vec![0.1; usize::try_from(stukken * 34).unwrap_or(0)],
            },
        }
    }

    /// Een spoor per seconde: elke seconde met geluid wordt een segment op de tijdlijn.
    fn spoor(map: &Path, annulering: &Annulering) -> Antwoord {
        let mut lezer = match Spoorlezer::open(map) {
            Ok(lezer) => lezer,
            Err(f) => {
                return Antwoord::Fout {
                    soort: FoutSoort::Audio,
                    melding: f.to_string(),
                };
            }
        };
        let naam = map
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let totaal_ms = lezer.tijdlijn().stroom_ms();
        let stuktijd = Duration::from_millis(getal("MONDIG_NEP_STUKTIJD_MS", 0));
        let ms = |positie: u64| {
            u64::try_from(mondig_core::spoor::stukken::samples_naar_ms(positie)).unwrap_or(0)
        };
        let mut meting = Meting::opbouw();
        let mut segmenten = Vec::new();
        let mut seconde = 0;
        loop {
            if annulering.is_geannuleerd() {
                return Antwoord::Geannuleerd;
            }
            let begin = lezer.positie();
            let blok = match lezer.lees(16_000) {
                Ok(blok) if blok.is_empty() => break,
                Ok(blok) => blok,
                Err(f) => {
                    return Antwoord::Fout {
                        soort: FoutSoort::Audio,
                        melding: f.to_string(),
                    };
                }
            };
            std::thread::sleep(stuktijd);
            meting.voeg_toe(&blok);
            if blok.iter().any(|s| s.abs() > 0.01) {
                segmenten.push(Segment::nieuw(
                    ms(begin),
                    ms(lezer.positie()),
                    format!("{naam} {seconde}"),
                ));
            }
            seconde += 1;
            let voortgang = Antwoord::Voortgang {
                klaar_ms: seconde * 1000,
                totaal_ms: Some(totaal_ms),
            };
            if stuur(&voortgang).is_err() {
                return Antwoord::Geannuleerd;
            }
        }
        Antwoord::Resultaat {
            waarschuwing: None,
            segmenten,
            duur_ms: totaal_ms,
            meting: meting.klaar(),
        }
    }

    /// De sprekers op een spoor, per seconde met geluid (zie de kop van dit bestand).
    fn sprekers(map: &Path, annulering: &Annulering, een_spreker: bool) -> Antwoord {
        let mut lezer = match Spoorlezer::open(map) {
            Ok(lezer) => lezer,
            Err(f) => {
                return Antwoord::Fout {
                    soort: FoutSoort::Audio,
                    melding: f.to_string(),
                };
            }
        };
        let totaal_ms = lezer.tijdlijn().stroom_ms();
        // Standaard 50 ms per seconde, net als bij een bestand: zonder rekentijd is het antwoord
        // er soms al voordat een annulering aankomt (zo faalde de annuleertest op Linux).
        let stuktijd = Duration::from_millis(getal("MONDIG_NEP_STUKTIJD_MS", 50));
        let ms = |positie: u64| {
            u64::try_from(mondig_core::spoor::stukken::samples_naar_ms(positie)).unwrap_or(0)
        };
        let mut segmenten = Vec::new();
        let mut seconde: u64 = 0;
        loop {
            if annulering.is_geannuleerd() {
                return Antwoord::Geannuleerd;
            }
            let begin = lezer.positie();
            let blok = match lezer.lees(16_000) {
                Ok(blok) if blok.is_empty() => break,
                Ok(blok) => blok,
                Err(f) => {
                    return Antwoord::Fout {
                        soort: FoutSoort::Audio,
                        melding: f.to_string(),
                    };
                }
            };
            std::thread::sleep(stuktijd);
            if blok.iter().any(|s| s.abs() > 0.01) {
                let spreker = if een_spreker {
                    1
                } else {
                    1 + u8::from((seconde / 2) % 2 == 1)
                };
                segmenten.push(SprekerSegment {
                    spreker,
                    start_ms: ms(begin),
                    eind_ms: ms(lezer.positie()),
                });
            }
            seconde += 1;
            let voortgang = Antwoord::Voortgang {
                klaar_ms: seconde * 1000,
                totaal_ms: Some(totaal_ms),
            };
            if stuur(&voortgang).is_err() {
                return Antwoord::Geannuleerd;
            }
        }
        Antwoord::Sprekers { segmenten }
    }

    /// De sprekers in een bestand: het nep-bestand duurt `MONDIG_NEP_STUKKEN` (5) seconden en
    /// heeft per seconde een spreker, die om de twee seconden wisselt tussen 1 en 2, zoals bij
    /// een spoor. Annuleren werkt; er komt een voortgang na elke seconde.
    fn sprekers_bestand(_pad: &Path, annulering: &Annulering, een_spreker: bool) -> Antwoord {
        let stukken = getal("MONDIG_NEP_STUKKEN", 5);
        let stuktijd = Duration::from_millis(getal("MONDIG_NEP_STUKTIJD_MS", 50));
        let mut segmenten = Vec::new();
        for seconde in 0..stukken {
            if annulering.is_geannuleerd() {
                return Antwoord::Geannuleerd;
            }
            std::thread::sleep(stuktijd);
            let spreker = if een_spreker {
                1
            } else {
                1 + u8::from((seconde / 2) % 2 == 1)
            };
            segmenten.push(SprekerSegment {
                spreker,
                start_ms: seconde * 1000,
                eind_ms: (seconde + 1) * 1000,
            });
            let voortgang = Antwoord::Voortgang {
                klaar_ms: (seconde + 1) * 1000,
                totaal_ms: Some(stukken * 1000),
            };
            if stuur(&voortgang).is_err() {
                return Antwoord::Geannuleerd;
            }
        }
        Antwoord::Sprekers { segmenten }
    }

    /// Het gedrag bij een uitschrijving, voor een bestand en voor geluid hetzelfde.
    /// `knip_stilte` zegt of om wegknippen van stilte gevraagd is.
    fn uitschrijven(&mut self, knip_stilte: bool) -> Antwoord {
        self.vervuil();
        match self.modus.as_str() {
            "traag_transcribeer" => {
                std::thread::sleep(Duration::from_millis(400));
                self.netjes(false)
            }
            "verkeerd_antwoord" => Antwoord::Geladen,
            "stil" => Antwoord::Resultaat {
                waarschuwing: None,
                segmenten: vec![Segment::nieuw(
                    0,
                    1000,
                    "Ondertiteling ingediend door de Amara.org gemeenschap",
                )],
                duur_ms: 1000,
                meting: Meting {
                    rms: 0.0,
                    frame_rms: vec![0.0; 34],
                },
            },
            "crash_transcribeer" => crash(),
            "crash_transcribeer_gpu" if self.apparaat == Apparaat::Gpu => crash(),
            "hang_transcribeer" => hang(),
            "fout_transcribeer" => Antwoord::Fout {
                soort: FoutSoort::Transcriberen,
                melding: "Nep: het uitschrijven is mislukt.".into(),
            },
            "fout_audio" => Antwoord::Fout {
                soort: FoutSoort::Audio,
                melding: "Nep: het bestand is niet te lezen.".into(),
            },
            "geen_spraak" if knip_stilte => Antwoord::Resultaat {
                waarschuwing: None,
                // Zoals het echte werkproces als Silero geen spraak vindt.
                segmenten: Vec::new(),
                duur_ms: 1000,
                meting: luide_meting(),
            },
            _ => self.netjes(knip_stilte),
        }
    }

    /// Een gewoon, goed antwoord. Is om knippen gevraagd, dan zegt de tekst dat.
    fn netjes(&self, knip_stilte: bool) -> Antwoord {
        let knip = if knip_stilte { " met knippen" } else { "" };
        Antwoord::Resultaat {
            waarschuwing: None,
            segmenten: vec![Segment::nieuw(
                0,
                1000,
                format!("nep op {:?}{knip}", self.apparaat),
            )],
            duur_ms: 1000,
            meting: luide_meting(),
        }
    }
}

fn main() -> ExitCode {
    let modus = std::env::var("MONDIG_NEP").unwrap_or_else(|_| "ok".into());
    if let Ok(pad) = std::env::var("MONDIG_NEP_PIDBESTAND") {
        let _ = std::fs::write(pad, std::process::id().to_string());
    }
    let mut nep = Nep {
        modus: modus.clone(),
        apparaat: Apparaat::Cpu,
    };
    if modus == "hang_hallo" {
        hang();
    }
    if modus == "ongeldig_utf8" {
        nep.vervuil();
    }
    if modus == "rommel" {
        // Regels die geen (geldig) bericht zijn: de supervisor moet ze overslaan.
        // De antwoorden gaan via `stuur`; dit is bewust ruwe uitvoer.
        let _ = stuur_rommel();
    }
    let uitkomst = match modus.as_str() {
        "foute_versie" => draai_met_versie(&mut nep, PROTOCOL_VERSIE + 1),
        "oud_werkproces" => draai_met_hallo(&mut nep, PROTOCOL_VERSIE, None),
        // Een schone bouwcode van een andere commit: met `-dirty` erin zou de supervisor
        // alleen waarschuwen, en een gewijzigde werkmap zet `-dirty` in `mondig_engine::BOUWCODE`.
        "andere_bouw" => draai_met_hallo(
            &mut nep,
            PROTOCOL_VERSIE,
            Some(concat!(env!("CARGO_PKG_VERSION"), "+0000000")),
        ),
        _ => draai(&mut nep),
    };
    if let Ok(pad) = std::env::var("MONDIG_NEP_STOPBESTAND") {
        let _ = std::fs::write(pad, "gestopt");
    }
    if uitkomst.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Schrijft twee regels die geen geldig bericht zijn.
fn stuur_rommel() -> std::io::Result<()> {
    use std::io::Write;
    let mut uit = std::io::stdout().lock();
    writeln!(uit, "dit is geen bericht")?;
    writeln!(uit, "{{\"type\":\"onbekend\"}}")?;
    uit.flush()
}
