//! Opdrachtregel van Mondig voor ontwikkelaars en tests.
//!
//! Opdrachten in deze fase: `modellen` (lijst), `modellen download <id>`,
//! `audio <bestand>`, `transcribe <bestand>...` (een of meer bestanden, met voortgang),
//! `opneem` (microfoon meten, geen uitschrijven), `vergader` (een vergadering opnemen als
//! twee sporen, uitschrijven en de echo meten), `bench` (nauwkeurigheid en snelheid meten) en
//! `taalmodel` (sleutel in de sleutelbos, modellen van een aanbieder, testaanvraag) en
//! `notulen` (notulen van een verslag maken en meten op de testset) en `opschonen` (een dictaat
//! opschonen met een taalmodel en meten op de testset).

#![forbid(unsafe_code)]

mod bench;
mod notulen;
mod opneem;
mod opschonen;
mod taalmodel;
mod vergader;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use mondig_core::aanbieder::{AanbiederFout, LokaleEngine};
use mondig_core::audio;
use mondig_core::bestanden::{Bestandsuitslag, Wachtrij};
use mondig_core::download::{self, DownloadFout, ModelStatus, Voortgang};
use mondig_core::engine::protocol::Apparaat;
use mondig_core::engine::supervisor::{EngineFout, Instellingen};
use mondig_core::filter::{FilterInstellingen, Gefilterd};
use mondig_core::model::{self, STANDAARD_MODEL};
use mondig_core::soevereiniteit::Soevereiniteit;
use mondig_core::taak::{Gebeurtenis, TaakId, Toestand};

/// Naam van het werkproces met de spraakengine (naast deze exe).
const ENGINE_NAAM: &str = mondig_core::engine::WERKPROCES_NAAM;

/// Mondig: soevereine dicteer- en transcriptietool (opdrachtregel).
#[derive(Parser)]
#[command(name = "mondig-cli", version, about)]
struct Cli {
    #[command(subcommand)]
    opdracht: Opdracht,
}

#[derive(Subcommand)]
enum Opdracht {
    /// Toont de beschikbare spraakmodellen, of downloadt er een.
    Modellen {
        #[command(subcommand)]
        actie: Option<ModelActie>,
    },
    /// Leest een audiobestand en toont wat Mondig ervan maakt (16 kHz mono).
    Audio {
        /// Het audiobestand (wav, mp3, m4a, flac, ogg, opus, of een video in mp4, mov, mkv of webm).
        bestand: PathBuf,
    },
    /// Schrijft een of meer bestanden na elkaar uit naar tekst, lokaal, via het
    /// achtergrondwerkproces `mondig-engine`, met voortgang. Ctrl+C annuleert.
    Transcribe {
        /// De bestanden (wav, mp3, m4a, flac, ogg, opus, of een video in mp4, mov, mkv of webm).
        #[arg(required = true)]
        bestanden: Vec<PathBuf>,
        /// Het model uit `mondig-cli modellen`.
        #[arg(long, default_value = STANDAARD_MODEL)]
        model: String,
        /// Taalcode (ISO 639-1), voor modellen die een taal nodig hebben.
        #[arg(long, default_value = "nl")]
        taal: String,
        /// Geef de uitkomst als JSON, met tijdcodes per segment.
        #[arg(long)]
        json: bool,
        /// Zet het filter tegen verzonnen tekst uit.
        #[arg(long)]
        geen_filter: bool,
    },
    /// Meet het openen van de microfoon en neemt een paar seconden op in het geheugen. Er gaat
    /// niets naar schijf en er wordt niets uitgeschreven.
    Opneem(opneem::OpneemOpties),
    /// Neemt een vergadering op als twee sporen (microfoon en het geluid van de computer), in
    /// stukken naar de tijdelijke map, schrijft zo'n opname uit als gesprek (`uitschrijven`) en
    /// meet de echo van de luidspreker in de microfoon (`echo`).
    Vergader(vergader::VergaderOpties),
    /// Meet woordfouten en snelheid op een set audiobestanden met referentietekst.
    Bench(bench::BenchOpties),
    /// Taalmodellen voor notulen: de sleutel in de sleutelbos zetten, de modellen van een
    /// aanbieder tonen en een korte testaanvraag doen.
    Taalmodel(taalmodel::TaalmodelOpties),
    /// Notulen maken van een verslag, of meten op de testset in `eval/notulen`.
    Notulen(notulen::NotulenOpties),
    /// Een dictaat opschonen met een taalmodel, of meten op de testset in `eval/opschonen`.
    Opschonen(opschonen::OpschonenOpties),
}

#[derive(Subcommand)]
enum ModelActie {
    /// Downloadt een model (hervat een eerdere, afgebroken download).
    Download {
        /// Het id uit de lijst, bijvoorbeeld `canary-1b-v2-q8`.
        id: String,
    },
}

/// Een fout met een melding in gewone taal.
#[derive(Debug)]
struct Melding(String);

impl<T: std::fmt::Display> From<T> for Melding {
    fn from(fout: T) -> Self {
        Self(fout.to_string())
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let resultaat = match cli.opdracht {
        Opdracht::Modellen { actie: None } => toon_modellen(),
        Opdracht::Modellen {
            actie: Some(ModelActie::Download { id }),
        } => download_model(&id),
        Opdracht::Audio { bestand } => toon_audio(&bestand),
        Opdracht::Transcribe {
            bestanden,
            model,
            taal,
            json,
            geen_filter,
        } => transcribeer(&bestanden, &model, &taal, json, geen_filter),
        Opdracht::Opneem(opties) => opneem::voer_uit(&opties),
        Opdracht::Vergader(opties) => vergader::voer_uit(&opties),
        Opdracht::Bench(opties) => bench::voer_uit(&opties),
        Opdracht::Taalmodel(opties) => taalmodel::voer_uit(&opties),
        Opdracht::Notulen(opties) => notulen::voer_uit(&opties),
        Opdracht::Opschonen(opties) => opschonen::voer_uit(&opties),
    };
    match resultaat {
        Ok(code) => code,
        Err(Melding(tekst)) => {
            eprintln!("Fout: {tekst}");
            ExitCode::FAILURE
        }
    }
}

/// Schrijft een regel naar stdout en meldt een fout (bijvoorbeeld een gesloten pipe) netjes.
fn uit(regel: &str) -> Result<(), Melding> {
    writeln!(io::stdout().lock(), "{regel}")?;
    Ok(())
}

/// Grootte in gewone eenheden (decimaal: 1 MB is 1.000.000 bytes).
fn leesbare_grootte(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        let tienden = bytes / 100_000_000;
        format!("{},{} GB", tienden / 10, tienden % 10)
    } else {
        format!("{} MB", (bytes + 500_000) / 1_000_000)
    }
}

fn status_tekst(status: ModelStatus) -> &'static str {
    match status {
        ModelStatus::Ontbreekt => "niet gedownload",
        ModelStatus::Gedeeltelijk => "gedeeltelijk",
        ModelStatus::Compleet => "klaar",
    }
}

fn toon_modellen() -> Result<ExitCode, Melding> {
    for m in model::spraakmodellen()? {
        let status = download::status(&m, &model::modelmap(&m.id)?);
        uit(&format!(
            "{}  {}\n    {}, {}, {}",
            m.id,
            m.naam,
            leesbare_grootte(m.totale_grootte()),
            m.licentie,
            status_tekst(status)
        ))?;
    }
    Ok(ExitCode::SUCCESS)
}

fn download_model(id: &str) -> Result<ExitCode, Melding> {
    let model = model::zoek(id)?;
    let map = model::modelmap(&model.id)?;
    let annuleer = Arc::new(AtomicBool::new(false));
    let vlag = Arc::clone(&annuleer);
    ctrlc::set_handler(move || vlag.store(true, Ordering::Relaxed))?;

    let reeds = download::aanwezige_bytes(&model, &map);
    eprintln!(
        "{}: {} te downloaden naar {}",
        model.naam,
        leesbare_grootte(model.totale_grootte()),
        map.display()
    );
    if reeds > 0 {
        eprintln!(
            "Er staat al {} klaar; daar gaat het verder.",
            leesbare_grootte(reeds)
        );
    }

    let mut laatste_tekening = Instant::now();
    let mut laatste_procent = None;
    let resultaat = download::download(
        &model,
        &map,
        |v| teken_voortgang(v, &mut laatste_tekening, &mut laatste_procent),
        &annuleer,
    );
    eprintln!();

    match resultaat {
        Ok(()) => {
            uit(&format!("{} is klaar.", model.naam))?;
            Ok(ExitCode::SUCCESS)
        }
        Err(DownloadFout::Geannuleerd) => {
            eprintln!(
                "Gestopt na {}. Voer dezelfde opdracht opnieuw uit om verder te gaan.",
                leesbare_grootte(download::aanwezige_bytes(&model, &map))
            );
            Ok(ExitCode::from(130))
        }
        Err(fout) => Err(fout.into()),
    }
}

/// Tekent de voortgang op één regel op stderr, hooguit een paar keer per seconde.
fn teken_voortgang(v: Voortgang, laatste_tekening: &mut Instant, laatste_procent: &mut Option<u8>) {
    let klaar = v.binnen >= v.totaal;
    let nieuw = Some(v.procent());
    if !klaar
        && (*laatste_procent == nieuw || laatste_tekening.elapsed() < Duration::from_millis(250))
    {
        return;
    }
    *laatste_tekening = Instant::now();
    *laatste_procent = nieuw;
    eprint!(
        "\r  {:>3}%  ({} van {})   ",
        v.procent(),
        leesbare_grootte(v.binnen),
        leesbare_grootte(v.totaal)
    );
    let _ = io::stderr().flush();
}

fn toon_audio(bestand: &Path) -> Result<ExitCode, Melding> {
    let geluid = audio::laad(bestand)?;
    let bron = &geluid.bron;
    let kanalen = match bron.kanalen {
        1 => "mono".to_string(),
        2 => "stereo".to_string(),
        n => format!("{n} kanalen"),
    };
    uit(&format!(
        "Formaat: {} ({kanalen}, {} Hz)",
        bron.formaat, bron.samplerate
    ))?;
    uit(&format!(
        "Oorspronkelijke duur: {:.2} s",
        bron.duur.as_secs_f64()
    ))?;
    uit(&format!(
        "Na omzetting: {} samples (16 kHz mono, {:.2} s)",
        geluid.samples.len(),
        geluid.duur.as_secs_f64()
    ))?;
    Ok(ExitCode::SUCCESS)
}

/// Zoekt het werkproces naast deze exe (de gedeelde logica staat in `mondig-core`).
fn engine_pad() -> Result<PathBuf, Melding> {
    let exe = std::env::current_exe()?;
    mondig_core::engine::werkproces_naast(&exe).map_err(|_| {
        let map = exe.parent().unwrap_or(&exe);
        Melding(format!(
            "Het werkproces {ENGINE_NAAM} staat niet naast mondig-cli ({}). Bouw het mee met `cargo build --workspace`.",
            map.display()
        ))
    })
}

/// Zoekt het model, controleert dat het er helemaal is en maakt de lokale engine (het
/// werkproces start pas bij de eerste transcriptie).
fn lokale_engine(model_id: &str, taal: &str) -> Result<(model::Model, LokaleEngine), Melding> {
    let model = model::zoek(model_id)?;
    let map = model::modelmap(&model.id)?;
    if download::status(&model, &map) != ModelStatus::Compleet {
        return Err(Melding(format!(
            "Het model {} is nog niet (helemaal) gedownload. Haal het binnen met: mondig-cli modellen download {}",
            model.naam, model.id
        )));
    }
    let engine = LokaleEngine::nieuw(
        Instellingen::achtergrond(engine_pad()?),
        &model,
        map,
        Apparaat::Cpu,
        Some(taal.to_string()),
    );
    Ok((model, engine))
}

/// Een bestand in de wachtrij van `transcribe`.
struct InRij {
    bestand: PathBuf,
    id: TaakId,
    uitslag: Receiver<Bestandsuitslag>,
    /// Wanneer het uitschrijven begon (de eerste "verwerkt").
    begin: Option<Instant>,
}

/// Schrijft bestanden na elkaar uit via de wachtrij uit `mondig-core`, met de voortgang
/// op stderr en de tekst op stdout. Ctrl+C annuleert wat loopt en wat nog wacht.
fn transcribeer(
    bestanden: &[PathBuf],
    model_id: &str,
    taal: &str,
    json: bool,
    geen_filter: bool,
) -> Result<ExitCode, Melding> {
    let (model, engine) = lokale_engine(model_id, taal)?;
    let annuleren = Arc::new(AtomicBool::new(false));
    let vlag = Arc::clone(&annuleren);
    ctrlc::set_handler(move || vlag.store(true, Ordering::SeqCst))?;

    let filter = (!geen_filter).then(FilterInstellingen::bestand);
    let (zender, gebeurtenissen) = mpsc::channel();
    let wachtrij = Wachtrij::start(engine, filter, zender);
    let mut rij = Vec::new();
    for bestand in bestanden {
        let (id, uitslag) = wachtrij.voeg_toe(bestand.clone())?;
        rij.push(InRij {
            bestand: bestand.clone(),
            id,
            uitslag,
            begin: None,
        });
    }
    eprintln!(
        "Model: {} ({})",
        model.naam,
        Soevereiniteit::Lokaal.label_nl()
    );

    let mut code = ExitCode::SUCCESS;
    let mut json_uitvoer = Vec::new();
    for (nummer, item) in rij.iter_mut().enumerate() {
        let uitslag = wacht_op_uitslag(item, &gebeurtenissen, &wachtrij, &annuleren);
        eprint!("\r{:79}\r", "");
        let naam = item.bestand.display();
        let duur = item.begin.map(|b| b.elapsed());
        match uitslag {
            Bestandsuitslag::Klaar(uitgeschreven) => {
                let gefilterd = uitgeschreven.gefilterd;
                if let Some(waarschuwing) = uitgeschreven.waarschuwing {
                    eprintln!("{}", waarschuwing.melding());
                }
                toon_klaar(&gefilterd, duur);
                if json {
                    json_uitvoer.push(json_van(&item.bestand, &model.id, taal, &gefilterd));
                } else {
                    if bestanden.len() > 1 {
                        uit(&format!("== {naam} =="))?;
                    }
                    uit(&gefilterd.transcript.tekst())?;
                }
            }
            Bestandsuitslag::Geannuleerd => {
                eprintln!("{naam}: geannuleerd.");
                code = ExitCode::from(130);
            }
            Bestandsuitslag::Fout(fout) => {
                let melding = match fout {
                    AanbiederFout::Audio(f) => Melding(f.to_string()),
                    AanbiederFout::Engine(f) => engine_melding(&f),
                };
                eprintln!("{naam}: {}", melding.0);
                if code == ExitCode::SUCCESS {
                    code = ExitCode::FAILURE;
                }
            }
        }
        if nummer + 1 < bestanden.len() {
            eprintln!();
        }
    }
    wachtrij.stop();
    if json {
        let uitvoer = if json_uitvoer.len() == 1 {
            json_uitvoer.remove(0)
        } else {
            serde_json::Value::Array(json_uitvoer)
        };
        uit(&serde_json::to_string_pretty(&uitvoer)?)?;
    }
    Ok(code)
}

/// Wacht op de uitslag van één bestand en tekent intussen de voortgang. Annuleert alles
/// zodra Ctrl+C is ingedrukt.
fn wacht_op_uitslag(
    item: &mut InRij,
    gebeurtenissen: &Receiver<Gebeurtenis>,
    wachtrij: &Wachtrij,
    annuleren: &AtomicBool,
) -> Bestandsuitslag {
    let mut geannuleerd = false;
    loop {
        if let Ok(uitslag) = item.uitslag.try_recv() {
            return uitslag;
        }
        if !geannuleerd && annuleren.load(Ordering::SeqCst) {
            geannuleerd = true;
            wachtrij.annuleer_alles();
        }
        match gebeurtenissen.recv_timeout(Duration::from_millis(100)) {
            Ok(g) if g.taak == item.id => {
                if let Toestand::Verwerkt { voortgang, .. } = g.toestand {
                    let begin = *item.begin.get_or_insert_with(Instant::now);
                    teken_bestandsvoortgang(&item.bestand, voortgang, begin.elapsed());
                }
            }
            _ => {}
        }
    }
}

/// De voortgang van een bestand op één regel op stderr, met een schatting van de tijd die
/// nog nodig is zodra die iets betekent.
fn teken_bestandsvoortgang(bestand: &Path, voortgang: Option<u8>, verstreken: Duration) {
    let naam = bestand.file_name().map_or_else(
        || bestand.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let stand = match voortgang {
        Some(procent) => {
            let rest = resterend(procent, verstreken)
                .map(|r| format!(", nog ongeveer {}", leesbare_duur(r)))
                .unwrap_or_default();
            format!("{procent:>3}%{rest}")
        }
        None => "bezig".to_string(),
    };
    eprint!("\r{:79}\r  {naam}: {stand}", "");
    let _ = io::stderr().flush();
}

/// Hoe lang het nog duurt als het zo doorgaat, of `None` als er te weinig gegevens zijn.
fn resterend(procent: u8, verstreken: Duration) -> Option<Duration> {
    if procent == 0 || procent >= 100 || verstreken < Duration::from_secs(2) {
        return None;
    }
    let rest = verstreken.as_secs_f64() * f64::from(100 - procent) / f64::from(procent);
    Some(Duration::from_secs_f64(rest))
}

/// Een duur in gewone taal, afgerond: "12 s", "3 min", "1 u 5 min".
fn leesbare_duur(duur: Duration) -> String {
    let s = duur.as_secs();
    if s < 60 {
        format!("{} s", s.max(1))
    } else if s < 3600 {
        format!("{} min", (s + 30) / 60)
    } else {
        format!("{} u {} min", s / 3600, (s % 3600 + 30) / 60)
    }
}

/// Wat er bij een klaar bestand op stderr komt: meldingen van het filter, de duur en de
/// realtime-factor.
fn toon_klaar(gefilterd: &Gefilterd, duur: Option<Duration>) {
    for ingreep in &gefilterd.ingrepen {
        eprintln!("Filter: {ingreep:?}");
    }
    if let Some(melding) = gefilterd.uitkomst.melding() {
        eprintln!("{melding}");
    }
    let audio_s = gefilterd.transcript.audio_duur.as_secs_f64();
    let verwerking_s = duur.map_or(0.0, |d| d.as_secs_f64());
    let factor = if audio_s > 0.0 {
        verwerking_s / audio_s
    } else {
        0.0
    };
    eprintln!(
        "Audio: {audio_s:.1} s\nUitschrijven: {verwerking_s:.1} s (realtime-factor {factor:.2}, lager is sneller, inclusief laden)"
    );
}

/// De JSON-uitvoer van één bestand.
fn json_van(bestand: &Path, model: &str, taal: &str, g: &Gefilterd) -> serde_json::Value {
    serde_json::json!({
        "bestand": bestand,
        "model": model,
        "taal": taal,
        "duur_ms": u64::try_from(g.transcript.audio_duur.as_millis()).unwrap_or(u64::MAX),
        "tekst": g.transcript.tekst(),
        "melding": g.uitkomst.melding(),
        "segmenten": g.transcript.segmenten,
    })
}

/// Maakt van een enginefout een melding, met de laatste logregels van het werkproces
/// op stderr als dat is gecrasht (die bevatten nooit transcripttekst).
fn engine_melding(fout: &EngineFout) -> Melding {
    if let EngineFout::Gecrasht { laatste_log, .. } = fout
        && !laatste_log.is_empty()
    {
        eprintln!("Technische details van het werkproces:");
        for regel in laatste_log.iter().rev().take(5).rev() {
            eprintln!("  {regel}");
        }
    }
    Melding(fout.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resterende_tijd_pas_als_er_iets_te_schatten_valt() {
        assert_eq!(resterend(50, Duration::from_secs(1)), None);
        assert_eq!(resterend(0, Duration::from_secs(10)), None);
        assert_eq!(resterend(100, Duration::from_secs(10)), None);
        assert_eq!(
            resterend(25, Duration::from_secs(10)),
            Some(Duration::from_secs(30))
        );
        assert_eq!(leesbare_duur(Duration::from_millis(300)), "1 s");
        assert_eq!(leesbare_duur(Duration::from_secs(45)), "45 s");
        assert_eq!(leesbare_duur(Duration::from_secs(150)), "3 min");
        assert_eq!(leesbare_duur(Duration::from_mins(65)), "1 u 5 min");
    }

    #[test]
    fn groottes_worden_leesbaar() {
        assert_eq!(leesbare_grootte(139_764), "0 MB");
        assert_eq!(leesbare_grootte(487_601_967), "488 MB");
        assert_eq!(leesbare_grootte(874_188_075), "874 MB");
        assert_eq!(leesbare_grootte(1_624_555_275), "1,6 GB");
    }
}
