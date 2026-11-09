//! `mondig-cli vergader`: een vergadering opnemen als twee sporen (microfoon en het geluid
//! van de computer), in stukken naar de tijdelijke map, zonder uitschrijven (sprint 4b-A).
//!
//! Opnemen, herstellen en wissen tonen alleen getallen en mappen, nooit audio of tekst. De audio
//! staat in de tijdelijke map voor vergaderingen; `vergader wis <map>` haalt een opname weg.
//!
//! `vergader uitschrijven` schrijft een afgesloten opname uit als gesprek (Jij en Anderen), naar
//! stdout of een bestand. Staat het sprekermodel naast het spraakmodel, dan worden de sprekers
//! herkend (online op Anderen, in de ruimte op de microfoon; `--zonder-sprekers` zet het uit). `vergader echo` meet hoeveel van Anderen via de luidspreker in de
//! microfoon terechtkomt (alleen getallen en tijden, geen tekst).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use clap::{Args, Subcommand};
use mondig_core::aanbieder::AanbiederFout;
use mondig_core::bewaren::{self, Document, Formaat};
use mondig_core::engine::supervisor::{EngineFout, Voortgang};
use mondig_core::filter::FilterInstellingen;
use mondig_core::gesprek::{Gesprek, echo, schrijf_gesprek_uit};
use mondig_core::model::{SPREKERMODEL, STANDAARD_MODEL};
use mondig_core::opname::handy::HandyApparatuur;
use mondig_core::opname::stukken::samples_naar_ms;
use mondig_core::opname::systeem::{SysteemApparatuur, kies_afspeelapparaat};
use mondig_core::opname::vergadering::{
    self, Afsluiting, MICROFOON, SYSTEEM, Spoor, Vergaderopname, Vergaderuitslag,
};
use mondig_core::opname::{Apparatuur, FoutSoort, OpnameFout, kies_microfoon};
use mondig_core::soevereiniteit::Soevereiniteit;
use mondig_core::spoor::{self, Spoorlezer};
use mondig_core::taak::{Foutmelding, Stap};

use crate::{Melding, uit};

/// Opties van `mondig-cli vergader`.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct VergaderOpties {
    #[command(subcommand)]
    actie: Option<VergaderActie>,
    #[command(flatten)]
    opnemen: OpneemArgs,
}

#[derive(Subcommand)]
enum VergaderActie {
    /// Toont de microfoons en afspeelapparaten.
    Apparaten,
    /// Zoekt afgebroken opnames en sluit ze af, tot en met het laatste stuk op schijf.
    Herstel {
        /// De map met opnames (standaard de tijdelijke map van Mondig).
        #[arg(long)]
        map: Option<PathBuf>,
    },
    /// Gooit een opname weg, met alle audio.
    Wis {
        /// De map van de opname, zoals `vergader` die toont.
        opname: PathBuf,
        /// Gooi de opname ook weg als Mondig niet kan nagaan of ze nog loopt (het slotbestand
        /// is niet te openen). Een opname die zeker loopt, blijft geweigerd.
        #[arg(long)]
        toch: bool,
    },
    /// Schrijft een afgesloten opname uit als gesprek (Jij en Anderen), lokaal.
    Uitschrijven(UitschrijfArgs),
    /// Meet hoeveel van Anderen via de luidspreker in de microfoon terechtkomt, om te kiezen
    /// tussen ontdubbelen en echo-onderdrukking.
    Echo(EchoArgs),
}

#[derive(Args)]
struct OpneemArgs {
    /// Hoeveel seconden er opgenomen wordt. Zonder opgave: tot Ctrl+C.
    #[arg(long)]
    duur: Option<u64>,
    /// De microfoon (id of naam, ook een deel ervan). Zonder opgave: de standaard.
    #[arg(long)]
    microfoon: Option<String>,
    /// Het afspeelapparaat waarvan het geluid wordt opgenomen (id of naam). Zonder opgave:
    /// de standaard, die een wissel tijdens de opname volgt.
    #[arg(long)]
    afspeelapparaat: Option<String>,
    /// Neem alleen de microfoon op.
    #[arg(long)]
    zonder_systeemgeluid: bool,
    /// De map waarin de opname komt (standaard de tijdelijke map van Mondig).
    #[arg(long)]
    map: Option<PathBuf>,
}

/// Voert `vergader` uit.
pub(crate) fn voer_uit(opties: &VergaderOpties) -> Result<ExitCode, Melding> {
    match &opties.actie {
        Some(VergaderActie::Apparaten) => toon_apparaten(),
        Some(VergaderActie::Herstel { map }) => herstel(map.as_deref()),
        Some(VergaderActie::Wis { opname, toch }) => {
            let gewist = if *toch {
                vergadering::wis_toch(opname)
            } else {
                vergadering::wis(opname)
            };
            gewist.map_err(|f| Melding(f.melding))?;
            uit(&format!("Weggegooid: {}", opname.display()))?;
            Ok(ExitCode::SUCCESS)
        }
        Some(VergaderActie::Uitschrijven(args)) => schrijf_uit(args),
        Some(VergaderActie::Echo(args)) => echo(args),
        None => neem_op(&opties.opnemen),
    }
}

fn basis(map: Option<&Path>) -> Result<PathBuf, Melding> {
    match map {
        Some(map) => Ok(map.to_path_buf()),
        None => vergadering::vergadermap().map_err(|f| Melding(f.melding)),
    }
}

fn melding(fout: OpnameFout) -> Melding {
    Melding(Foutmelding::from(fout).melding)
}

fn toon_apparaten() -> Result<ExitCode, Melding> {
    uit("Microfoons:")?;
    for m in HandyApparatuur.microfoons().map_err(melding)? {
        let standaard = if m.standaard { "  (standaard)" } else { "" };
        uit(&format!("  {}  {}{standaard}", m.id, m.naam))?;
    }
    uit("Afspeelapparaten (geluid van de computer):")?;
    match SysteemApparatuur.afspeelapparaten() {
        Ok(lijst) if lijst.is_empty() => uit("  geen")?,
        Ok(lijst) => {
            for a in lijst {
                let standaard = if a.standaard { "  (standaard)" } else { "" };
                uit(&format!("  {}  {}{standaard}", a.id, a.naam))?;
            }
        }
        Err(fout) => uit(&format!("  {}", Foutmelding::from(fout).melding))?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Wat het herstellen van een rij opnames opleverde: per opname gelukt of mislukt.
struct HerstelUitslag {
    gelukt: Vec<(PathBuf, Afsluiting)>,
    mislukt: Vec<(PathBuf, Foutmelding)>,
}

/// Herstelt elke opname apart: een opname die niet lukt, houdt de rest niet tegen.
fn herstel_lijst(lijst: Vec<PathBuf>) -> HerstelUitslag {
    let mut uitslag = HerstelUitslag {
        gelukt: Vec::new(),
        mislukt: Vec::new(),
    };
    for opname in lijst {
        match vergadering::herstel(&opname) {
            Ok(afsluiting) => uitslag.gelukt.push((opname, afsluiting)),
            Err(fout) => uitslag.mislukt.push((opname, fout)),
        }
    }
    uitslag
}

fn herstel(map: Option<&Path>) -> Result<ExitCode, Melding> {
    let basis = basis(map)?;
    let lijst: Vec<PathBuf> = vergadering::onafgemaakt_met_stand(&basis)
        .into_iter()
        .map(|o| o.map)
        .collect();
    if lijst.is_empty() {
        uit("Geen afgebroken opnames gevonden.")?;
        return Ok(ExitCode::SUCCESS);
    }
    // `herstel` slaat een opname over waarvan het slot niet te lezen is, met een melding.
    let uitslag = herstel_lijst(lijst);
    for (opname, afsluiting) in &uitslag.gelukt {
        uit(&format!(
            "Hersteld: {} ({})",
            opname.display(),
            tijd(Duration::from_millis(afsluiting.duur_ms))
        ))?;
        uit(&format!(
            "  microfoon: {} stukken, {}",
            afsluiting.microfoon.stukken,
            tijd(samples_duur(afsluiting.microfoon.samples))
        ))?;
        if let Some(systeem) = &afsluiting.systeem {
            let stil = if systeem.stil { ", stil" } else { "" };
            uit(&format!(
                "  systeem: {} stukken, {}{stil}",
                systeem.stukken,
                tijd(samples_duur(systeem.samples))
            ))?;
        }
    }
    for (opname, fout) in &uitslag.mislukt {
        eprintln!("Niet hersteld: {}: {}", opname.display(), fout.melding);
    }
    if uitslag.mislukt.is_empty() {
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!(
            "Fout: {} van de {} opnames is niet hersteld.",
            uitslag.mislukt.len(),
            uitslag.mislukt.len() + uitslag.gelukt.len()
        );
        Ok(ExitCode::FAILURE)
    }
}

/// De duur van een aantal samples op de samplerate van de sporen.
fn samples_duur(samples: u64) -> Duration {
    Duration::from_millis(u64::try_from(samples_naar_ms(samples)).unwrap_or(0))
}

/// Tijd als uu:mm:ss.
fn tijd(duur: Duration) -> String {
    let s = duur.as_secs();
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// Wat een bron meldt terwijl de opname loopt.
enum Uitval {
    Microfoon(OpnameFout),
    Systeem(OpnameFout),
}

fn neem_op(args: &OpneemArgs) -> Result<ExitCode, Melding> {
    let basis = basis(args.map.as_deref())?;
    let afgebroken = vergadering::onafgemaakt_met_stand(&basis);
    if !afgebroken.is_empty() {
        eprintln!(
            "Let op: er staan {} afgebroken opname(s). Haal ze terug met `mondig-cli vergader herstel`.",
            afgebroken.len()
        );
    }

    let (meld, uitvallen) = mpsc::channel();
    let microfoon_id = match &args.microfoon {
        Some(zoek) => {
            let lijst = HandyApparatuur.microfoons().map_err(melding)?;
            Some(
                kies_microfoon(&lijst, zoek)
                    .map_err(|f| Melding(f.melding))?
                    .id
                    .clone(),
            )
        }
        None => None,
    };
    let zender = meld.clone();
    let microfoon = HandyApparatuur
        .open(
            microfoon_id.as_deref(),
            Box::new(move |f| {
                let _ = zender.send(Uitval::Microfoon(f));
            }),
        )
        .map_err(melding)?;

    let systeem = if args.zonder_systeemgeluid {
        None
    } else {
        open_systeem(args.afspeelapparaat.as_deref(), meld)?
    };
    let met_systeem = systeem.is_some();

    let stoppen = Arc::new(AtomicBool::new(false));
    let vlag = Arc::clone(&stoppen);
    ctrlc::set_handler(move || vlag.store(true, Ordering::SeqCst))?;

    let opname =
        Vergaderopname::start(&basis, microfoon, systeem).map_err(|f| Melding(f.melding))?;
    eprintln!("Opname loopt in {}", opname.map().display());
    eprintln!(
        "Vertel de anderen dat je opneemt. {}",
        match args.duur {
            Some(s) => format!("De opname stopt na {s} seconden of met Ctrl+C."),
            None => "Stop met Ctrl+C.".to_string(),
        }
    );
    if !met_systeem {
        eprintln!("Alleen de microfoon wordt opgenomen.");
    }

    let grens = args.duur.map(Duration::from_secs);
    let mut volgende_melding = Duration::from_secs(30);
    let begin = Instant::now();
    while !stoppen.load(Ordering::SeqCst) && grens.is_none_or(|g| begin.elapsed() < g) {
        std::thread::sleep(Duration::from_millis(100));
        while let Ok(uitval) = uitvallen.try_recv() {
            let (spoor, fout) = match uitval {
                Uitval::Microfoon(f) => ("microfoon", f),
                Uitval::Systeem(f) => ("systeem", f),
            };
            eprintln!(
                "Let op ({spoor}): {} De opname loopt door met wat er nog werkt.",
                Foutmelding::from(fout).melding
            );
        }
        if let Some(fout) = opname.schrijffout() {
            eprintln!("Fout: {}", fout.melding);
            break;
        }
        if opname.duur() >= volgende_melding {
            eprintln!("{} opgenomen", tijd(opname.duur()));
            volgende_melding += Duration::from_secs(30);
        }
    }
    let uitslag = opname.stop().map_err(|f| Melding(f.melding))?;
    toon_uitslag(&uitslag)?;
    Ok(ExitCode::SUCCESS)
}

/// Opent het geluid van de computer. Is er geen afspeelapparaat, of kan dit systeem het
/// nog niet, dan gaat de opname door met alleen de microfoon en een duidelijke melding.
fn open_systeem(
    zoek: Option<&str>,
    meld: mpsc::Sender<Uitval>,
) -> Result<Option<Box<dyn mondig_core::opname::Bron>>, Melding> {
    let id = match zoek {
        Some(zoek) => {
            let lijst = SysteemApparatuur.afspeelapparaten().map_err(melding)?;
            Some(
                kies_afspeelapparaat(&lijst, zoek)
                    .map_err(|f| Melding(f.melding))?
                    .id
                    .clone(),
            )
        }
        None => None,
    };
    let bij_uitval = Box::new(move |f| {
        let _ = meld.send(Uitval::Systeem(f));
    });
    match SysteemApparatuur.open(id.as_deref(), bij_uitval) {
        Ok(bron) => Ok(Some(bron)),
        Err(fout)
            if zoek.is_none()
                && matches!(
                    fout.soort,
                    FoutSoort::GeenAfspeelapparaat
                        | FoutSoort::SysteemgeluidNietMogelijk
                        | FoutSoort::SysteemgeluidOmrekenenStart
                ) =>
        {
            eprintln!("Let op: {}", Foutmelding::from(fout).melding);
            Ok(None)
        }
        Err(fout) => Err(melding(fout)),
    }
}

fn dbfs(piek: f32) -> String {
    if piek <= 0.0 {
        "stilte".to_string()
    } else {
        format!("{:.1} dBFS", 20.0 * piek.log10())
    }
}

fn toon_spoor(naam: &str, spoor: &Spoor) -> Result<(), Melding> {
    let u = &spoor.uitslag;
    let bytes: u64 = std::fs::read_dir(&u.map).map_or(0, |lijst| {
        lijst
            .filter_map(Result::ok)
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum()
    });
    let tiende_mb = (bytes + 50_000) / 100_000;
    uit(&format!(
        "  {naam}: {} stukken, {}, {},{} MB, piek {}{}",
        u.stukken,
        tijd(u.duur()),
        tiende_mb / 10,
        tiende_mb % 10,
        dbfs(u.piek),
        if u.opgevuld > 0 {
            format!(", {} stilte ingevuld", tijd(samples_duur(u.opgevuld)))
        } else {
            String::new()
        }
    ))?;
    if spoor.verloren > 0 {
        uit(&format!(
            "    Let op: ongeveer {} ms verloren.",
            samples_duur(spoor.verloren).as_millis()
        ))?;
    }
    if let Some(fout) = &spoor.onderbroken {
        uit(&format!("    Let op: {}", fout.melding))?;
    }
    Ok(())
}

fn toon_uitslag(uitslag: &Vergaderuitslag) -> Result<(), Melding> {
    uit(&format!(
        "Opgenomen: {} in {}",
        tijd(uitslag.duur),
        uitslag.map.display()
    ))?;
    toon_spoor(MICROFOON, &uitslag.microfoon)?;
    if let Some(systeem) = &uitslag.systeem {
        toon_spoor(SYSTEEM, systeem)?;
    }
    if let Some(tekst) = uitslag.melding_stil_systeem() {
        uit(&format!("Let op: {tekst}"))?;
    }
    Ok(())
}

/// Opties van `vergader uitschrijven`.
#[derive(Args)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "losse vlaggen van de opdrachtregel"
)]
struct UitschrijfArgs {
    /// De map van de opname, zoals `vergader` die toont. Zonder opgave: de nieuwste
    /// afgesloten opname in de tijdelijke map.
    opname: Option<PathBuf>,
    /// Het model uit `mondig-cli modellen`.
    #[arg(long, default_value = STANDAARD_MODEL)]
    model: String,
    /// Taalcode (ISO 639-1), voor modellen die een taal nodig hebben.
    #[arg(long, default_value = "nl")]
    taal: String,
    /// Zet het filter tegen verzonnen tekst uit.
    #[arg(long)]
    geen_filter: bool,
    /// Laat dubbele tekst staan: zonder koptelefoon hoort de microfoon de anderen ook, en
    /// normaal vallen die uitingen van Jij weg.
    #[arg(long)]
    geen_ontdubbelen: bool,
    /// Herken geen sprekers, ook als het sprekermodel klaarstaat (dan staat online alleen
    /// "Jij" en "Anderen" in het verslag, zoals zonder sprekermodel).
    #[arg(long)]
    zonder_sprekers: bool,
    /// Het bestandsformaat: txt, docx, odt, pdf, srt of vtt. Zonder opgave volgt het uit de
    /// extensie van `--uit`; zonder beide gaan de beurten als tekst naar stdout.
    #[arg(long, value_parser = parse_formaat, conflicts_with = "json")]
    formaat: Option<Formaat>,
    /// Waar het bestand komt. Het mag nog niet bestaan.
    #[arg(long, conflicts_with = "json")]
    uit: Option<PathBuf>,
    /// Zet de tijd bij elke beurt (in txt, docx, odt en pdf; srt en vtt hebben altijd tijden).
    #[arg(long)]
    tijden: bool,
    /// Geef de uitkomst als JSON op stdout, met tijden per beurt.
    #[arg(long)]
    json: bool,
}

/// Opties van `vergader echo`.
#[derive(Args)]
struct EchoArgs {
    /// De map van de opname. Zonder opgave: de nieuwste afgesloten opname.
    opname: Option<PathBuf>,
    /// Het model uit `mondig-cli modellen`.
    #[arg(long, default_value = STANDAARD_MODEL)]
    model: String,
    /// Taalcode (ISO 639-1), voor modellen die een taal nodig hebben.
    #[arg(long, default_value = "nl")]
    taal: String,
}

fn parse_formaat(tekst: &str) -> Result<Formaat, String> {
    Formaat::uit_sleutel(&tekst.to_lowercase())
        .ok_or_else(|| "kies txt, docx, odt, pdf, srt of vtt".to_string())
}

/// Waar de uitkomst van `uitschrijven` heen gaat.
#[derive(Debug, PartialEq, Eq)]
enum Uitvoer {
    /// De beurten als tekst op stdout.
    Beurten,
    /// De beurten als JSON op stdout.
    Json,
    /// Een bestand in dit formaat, naar dit pad of (alleen tekstformaten) naar stdout.
    Bestand(Formaat, Option<PathBuf>),
}

/// Bepaalt vooraf wat er met de uitkomst gebeurt, zodat een verkeerde combinatie meteen een
/// fout geeft en niet pas na een uur uitschrijven.
fn bepaal_uitvoer(args: &UitschrijfArgs) -> Result<Uitvoer, Melding> {
    if args.json {
        return Ok(Uitvoer::Json);
    }
    let uit_formaat = args
        .uit
        .as_deref()
        .and_then(Path::extension)
        .and_then(|e| Formaat::uit_sleutel(&e.to_string_lossy().to_lowercase()));
    let formaat = match (args.formaat, uit_formaat, &args.uit) {
        (Some(formaat), _, _) | (None, Some(formaat), _) => formaat,
        (None, None, Some(pad)) => {
            return Err(Melding(format!(
                "Mondig kan aan {} niet zien welk formaat het moet worden. Voeg --formaat toe (txt, docx, odt, pdf, srt of vtt).",
                pad.display()
            )));
        }
        (None, None, None) => return Ok(Uitvoer::Beurten),
    };
    if args.uit.is_none() && !matches!(formaat, Formaat::Txt | Formaat::Srt | Formaat::Vtt) {
        return Err(Melding(format!(
            "Een {}-bestand kan niet naar stdout. Geef met --uit op waar het bestand moet komen.",
            formaat.extensie()
        )));
    }
    // Een bestaand bestand weigeren we nu al: niet pas na een uur uitschrijven.
    if let Some(pad) = args.uit.as_deref().filter(|p| p.exists()) {
        return Err(bestaat_al(pad));
    }
    Ok(Uitvoer::Bestand(formaat, args.uit.clone()))
}

fn bestaat_al(pad: &Path) -> Melding {
    Melding(format!(
        "{} bestaat al. Kies een andere naam met --uit, of haal het bestand eerst weg.",
        pad.display()
    ))
}

/// De nieuwste afgesloten opname in de tijdelijke map: die met het laatste begin. Een map
/// zonder `einde.json` (een opname die loopt of afgebroken is) telt niet.
fn nieuwste_opname() -> Result<PathBuf, Melding> {
    let basis = vergadering::vergadermap().map_err(|f| Melding(f.melding))?;
    nieuwste_in(&basis).ok_or_else(|| {
        Melding(format!(
            "Er staat geen afgesloten opname in {}. Neem er een op met `mondig-cli vergader`, of herstel een afgebroken opname met `mondig-cli vergader herstel`.",
            basis.display()
        ))
    })
}

fn nieuwste_in(basis: &Path) -> Option<PathBuf> {
    std::fs::read_dir(basis)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|map| map.join(spoor::EINDE).is_file())
        .max_by_key(|map| {
            let begin = vergadering::opzet(map).map_or(0, |o| o.begin_unix_ms);
            (begin, map.clone())
        })
}

fn kies_opname(opname: Option<&Path>) -> Result<PathBuf, Melding> {
    match opname {
        Some(map) => Ok(map.to_path_buf()),
        None => nieuwste_opname(),
    }
}

/// Tekent de voortgang op één regel op stderr, hooguit een paar keer per seconde.
struct Voortgangsregel {
    begin: Instant,
    laatste: Option<Instant>,
}

impl Voortgangsregel {
    fn nieuw() -> Self {
        Self {
            begin: Instant::now(),
            laatste: None,
        }
    }

    fn teken(&mut self, voortgang: Voortgang) {
        let klaar = voortgang
            .totaal
            .is_some_and(|totaal| voortgang.klaar >= totaal);
        if !klaar
            && self
                .laatste
                .is_some_and(|l| l.elapsed() < Duration::from_millis(250))
        {
            return;
        }
        self.laatste = Some(Instant::now());
        let stand = match voortgang.procent() {
            Some(procent) => {
                let rest = crate::resterend(procent, self.begin.elapsed())
                    .map(|r| format!(", nog ongeveer {}", crate::leesbare_duur(r)))
                    .unwrap_or_default();
                format!("{procent:>3}%{rest}")
            }
            None => "bezig".to_string(),
        };
        let wat = match voortgang.stap {
            Stap::Uitschrijven => "Uitschrijven",
            Stap::Sprekers => "Sprekers herkennen",
        };
        eprint!("\r{:79}\r  {wat}: {stand}", "");
        let _ = std::io::stderr().flush();
    }

    /// Wist de regel.
    fn wis() {
        eprint!("\r{:79}\r", "");
    }
}

/// De afloop als het uitschrijven niet lukte: een fout, of na Ctrl+C een nette stop (130).
fn mislukt(fout: AanbiederFout) -> Result<ExitCode, Melding> {
    match fout {
        AanbiederFout::Engine(EngineFout::Geannuleerd) => {
            eprintln!("Geannuleerd.");
            Ok(ExitCode::from(130))
        }
        AanbiederFout::Engine(f) => Err(crate::engine_melding(&f)),
        AanbiederFout::Audio(f) => Err(Melding(f.to_string())),
    }
}

/// Zet Ctrl+C om in een vlag die het uitschrijven afbreekt (zoals bij `transcribe`).
fn annuleer_bij_ctrl_c() -> Result<Arc<AtomicBool>, Melding> {
    let annuleren = Arc::new(AtomicBool::new(false));
    let vlag = Arc::clone(&annuleren);
    ctrlc::set_handler(move || vlag.store(true, Ordering::SeqCst))?;
    Ok(annuleren)
}

fn schrijf_uit(args: &UitschrijfArgs) -> Result<ExitCode, Melding> {
    let uitvoer = bepaal_uitvoer(args)?;
    let map = kies_opname(args.opname.as_deref())?;
    // Het slot houdt het automatische wissen in de app tegen zolang dit loopt.
    let _slot = vergadering::vergrendel(&map).map_err(|f| Melding(f.melding))?;
    let (model, mut engine) = crate::lokale_engine(&args.model, &args.taal)?;
    eprintln!(
        "Opname: {}\nModel: {} ({})",
        map.display(),
        model.naam,
        Soevereiniteit::Lokaal.label_nl()
    );
    let sprekers = !args.zonder_sprekers;
    if sprekers && !engine.heeft_sprekermodel() {
        eprintln!(
            "Sprekers: het model {SPREKERMODEL} staat niet naast het spraakmodel; zonder sprekerherkenning."
        );
    }
    let annuleren = annuleer_bij_ctrl_c()?;
    let filter = (!args.geen_filter).then(FilterInstellingen::bestand);
    let ontdubbelen = (!args.geen_ontdubbelen).then(echo::Ontdubbelen::default);
    let mut regel = Voortgangsregel::nieuw();
    let gesprek = schrijf_gesprek_uit(
        &mut engine,
        &map,
        filter.as_ref(),
        ontdubbelen.as_ref(),
        sprekers,
        &mut |v| regel.teken(v),
        &annuleren,
    );
    Voortgangsregel::wis();
    let gesprek = match gesprek {
        Ok(gesprek) => gesprek,
        Err(fout) => {
            return mislukt(fout);
        }
    };
    for melding in &gesprek.meldingen {
        eprintln!("Let op: {melding}");
    }
    if gesprek.beurten.is_empty() {
        eprintln!("Er is geen tekst herkend.");
    }
    match uitvoer {
        Uitvoer::Beurten => {
            for beurt in &gesprek.beurten {
                let tijd = tijd(Duration::from_millis(beurt.start_ms));
                // Zonder sprekers, bij Alleen ik en bij een alinea die in de ruimte niemand
                // aanwees: alleen de alinea.
                uit(&match gesprek.verslaglabel(beurt) {
                    Some(naam) => format!("[{tijd}] {naam}: {}", beurt.tekst),
                    None => format!("[{tijd}] {}", beurt.tekst),
                })?;
            }
        }
        Uitvoer::Json => uit(&serde_json::to_string_pretty(&json_van(
            &map, &model.id, &args.taal, &gesprek,
        ))?)?,
        Uitvoer::Bestand(formaat, pad) => {
            let moment = vergadering::opzet(&map).map_or_else(
                |_| bewaren::Zoned::now(),
                |o| bewaren::lokaal_moment(o.begin_unix_ms),
            );
            let document = Document::van_gesprek(&gesprek, &model.naam, moment);
            let bytes = bewaren::maak(&document, formaat, args.tijden)?;
            match pad {
                Some(pad) => {
                    schrijf_atomair(&pad, &bytes)?;
                    eprintln!("Bewaard: {}", pad.display());
                }
                None => std::io::stdout().lock().write_all(&bytes)?,
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// De JSON-uitvoer van een uitgeschreven vergadering.
fn json_van(map: &Path, model: &str, taal: &str, gesprek: &Gesprek) -> serde_json::Value {
    serde_json::json!({
        "opname": map,
        "model": model,
        "taal": taal,
        "duur_ms": u64::try_from(gesprek.duur.as_millis()).unwrap_or(u64::MAX),
        "jij": gesprek.jij,
        "anderen": gesprek.anderen,
        "meldingen": gesprek.meldingen,
        "weggelaten": gesprek.weggelaten,
        "soort": gesprek.soort,
        "sprekers": gesprek.sprekernummers(),
        "beurten": gesprek.beurten.iter().map(|b| serde_json::json!({
            "kant": b.kant,
            // Het nummer van de spreker (vanaf 1) als de sprekers herkend zijn, anders null.
            "sprekernummer": b.sprekernummer,
            "spreker": gesprek.verslaglabel(b),
            "start_ms": b.start_ms,
            "eind_ms": b.eind_ms,
            "tekst": b.tekst,
        })).collect::<Vec<_>>(),
    })
}

/// Schrijft een bestand atomair, zoals de app: eerst een tijdelijk bestand in dezelfde map,
/// naar schijf geschreven, dan hernoemd. Een bestaand bestand wordt nooit overschreven.
fn schrijf_atomair(pad: &Path, bytes: &[u8]) -> Result<(), Melding> {
    let map = pad
        .parent()
        .filter(|m| !m.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let fout = |f: std::io::Error| {
        if f.kind() == std::io::ErrorKind::AlreadyExists {
            return bestaat_al(pad);
        }
        Melding(match f.kind() {
            std::io::ErrorKind::NotFound => {
                format!("De map {} bestaat niet.", map.display())
            }
            std::io::ErrorKind::PermissionDenied => {
                format!("Mondig mag niet schrijven in {}.", map.display())
            }
            _ => format!("Opslaan in {} is niet gelukt: {f}", pad.display()),
        })
    };
    let mut tijdelijk = tempfile::Builder::new()
        .prefix(".mondig-")
        .suffix(".tijdelijk")
        .tempfile_in(map)
        .map_err(fout)?;
    tijdelijk.write_all(bytes).map_err(fout)?;
    tijdelijk.as_file().sync_all().map_err(fout)?;
    tijdelijk
        .persist_noclobber(pad)
        .map(|_| ())
        .map_err(|f| fout(f.error))
}

fn echo(args: &EchoArgs) -> Result<ExitCode, Melding> {
    let map = kies_opname(args.opname.as_deref())?;
    // Het slot houdt het automatische wissen in de app tegen zolang dit loopt.
    let _slot = vergadering::vergrendel(&map).map_err(|f| Melding(f.melding))?;
    let afsluiting = spoor::lees_afsluiting(&map).map_err(|f| Melding(f.to_string()))?;
    let microfoon = map.join(MICROFOON);
    let systeem = map.join(SYSTEEM);
    if afsluiting.systeem.is_none() || !systeem.is_dir() {
        return Err(Melding(
            "Deze opname heeft geen geluid van de computer. Er valt niets te meten.".to_string(),
        ));
    }
    let (model, mut engine) = crate::lokale_engine(&args.model, &args.taal)?;
    eprintln!(
        "Opname: {}\nModel: {} ({})",
        map.display(),
        model.naam,
        Soevereiniteit::Lokaal.label_nl()
    );
    let annuleren = annuleer_bij_ctrl_c()?;

    // (a) Tekst: beide sporen uitschrijven.
    let lengte = |spoormap: &Path| -> Result<Duration, Melding> {
        let lezer = Spoorlezer::open(spoormap).map_err(|f| Melding(f.to_string()))?;
        Ok(Duration::from_millis(lezer.tijdlijn().stroom_ms()))
    };
    let lengte_microfoon = lengte(&microfoon)?;
    let totaal = Some(lengte_microfoon + lengte(&systeem)?);
    let filter = FilterInstellingen::bestand();
    let mut regel = Voortgangsregel::nieuw();
    let sporen = (|| {
        let jij = engine.transcribeer_spoor(
            &microfoon,
            Some(&filter),
            &mut |v| {
                regel.teken(Voortgang {
                    klaar: v.klaar,
                    totaal,
                    ..v
                });
            },
            &annuleren,
        )?;
        let anderen = engine.transcribeer_spoor(
            &systeem,
            Some(&filter),
            &mut |v| {
                regel.teken(Voortgang {
                    klaar: lengte_microfoon + v.klaar,
                    totaal,
                    ..v
                });
            },
            &annuleren,
        )?;
        Ok::<_, AanbiederFout>((jij, anderen))
    })();
    Voortgangsregel::wis();
    let (jij, anderen) = match sporen {
        Ok(sporen) => sporen,
        Err(fout) => {
            return mislukt(fout);
        }
    };
    let tekst = echo::tekst_echo(
        &jij.gefilterd.transcript.segmenten,
        &anderen.gefilterd.transcript.segmenten,
    );

    // (b) Signaal: de omhullenden, streamend uit de sporen.
    eprintln!("Signaal meten...");
    let spoorfout = |f: spoor::SpoorFout| Melding(f.to_string());
    let omhullende_systeem = echo::omhullende_van_spoor(&systeem)
        .map_err(spoorfout)?
        .rms();
    let omhullende_microfoon = echo::omhullende_van_spoor(&microfoon)
        .map_err(spoorfout)?
        .rms();
    let vertraging = echo::beste_vertraging(&omhullende_systeem, &omhullende_microfoon);
    let vertraging_bins =
        vertraging.map_or(0, |v| usize::try_from(v.ms / echo::BIN_MS).unwrap_or(0));
    let niveau =
        echo::niveau_bij_systeem(&omhullende_systeem, &omhullende_microfoon, vertraging_bins);

    toon_echo(&map, &tekst, vertraging, &niveau)?;
    Ok(ExitCode::SUCCESS)
}

/// Het aantal verdachte uitingen dat `echo` toont.
const TOON_VERDACHT: usize = 5;

fn toon_echo(
    map: &Path,
    tekst: &echo::TekstEcho,
    vertraging: Option<echo::Vertraging>,
    niveau: &echo::NiveauMeting,
) -> Result<(), Melding> {
    uit(&format!("Echo-meting van {}", map.display()))?;
    uit(&format!(
        "Tekst: uitingen van Jij waarvan ten minste {} procent van de woorden ook bij Anderen staat (binnen {} s)",
        echo::ECHO_PERCENTAGE,
        echo::ECHO_MARGE.as_secs()
    ))?;
    uit(&format!("  Uitingen van Jij: {}", tekst.jij_uitingen))?;
    uit(&format!(
        "  Verdacht: {} ({} procent)",
        tekst.verdacht.len(),
        tekst.procent_verdacht()
    ))?;
    for v in tekst.verdacht.iter().take(TOON_VERDACHT) {
        uit(&format!(
            "    [{}] {} procent van de woorden ook bij Anderen",
            tijd(Duration::from_millis(v.start_ms)),
            v.procent
        ))?;
    }
    uit("Signaal: luidheid per 10 ms op de tijdlijn van de opname")?;
    match vertraging {
        Some(v) => uit(&format!(
            "  Beste vertraging (0 tot {} ms): {} ms, correlatie {:.2}",
            echo::MAX_VERTRAGING_MS,
            v.ms,
            v.correlatie
        ))?,
        None => uit(
            "  Beste vertraging: niet te bepalen (een van de sporen is te kort of heeft geen verloop)",
        )?,
    }
    toon_niveau(
        &format!(
            "systeemspoor luid (boven {:.0} dBFS)",
            echo::LUID_DREMPEL_DBFS
        ),
        &niveau.bij_luid,
    )?;
    toon_niveau(
        &format!(
            "systeemspoor stil (onder {:.0} dBFS)",
            echo::STIL_DREMPEL_DBFS
        ),
        &niveau.bij_stil,
    )?;
    uit(
        "De drempels voor verdacht, luid en stil zijn nog niet gemeten: lees dit als een eerste indruk.",
    )?;
    Ok(())
}

fn toon_niveau(wanneer: &str, niveau: &echo::Niveau) -> Result<(), Melding> {
    let db = |waarde: Option<f32>| waarde.map_or("geen".to_string(), |w| format!("{w:.1} dBFS"));
    uit(&format!(
        "  Microfoon bij {wanneer}: gemiddeld {}, mediaan {} ({} vensters van {} ms)",
        db(niveau.gemiddeld_dbfs),
        db(niveau.mediaan_dbfs),
        niveau.vensters,
        echo::NIVEAU_VENSTER_BINS as u64 * echo::BIN_MS
    ))
}

#[cfg(test)]
mod tests {
    use mondig_core::audio::DOEL_SAMPLERATE;
    use mondig_core::opname::vergadering::Opzet;

    use super::*;

    #[test]
    fn tijd_als_uren_minuten_seconden() {
        assert_eq!(tijd(Duration::from_secs(0)), "00:00:00");
        assert_eq!(tijd(Duration::from_secs(3_725)), "01:02:05");
    }

    #[test]
    fn samples_naar_duur_volgt_de_samplerate_van_de_sporen() {
        assert_eq!(
            samples_duur(u64::from(DOEL_SAMPLERATE) * 3),
            Duration::from_secs(3)
        );
        assert_eq!(
            samples_duur(u64::from(DOEL_SAMPLERATE) / 2),
            Duration::from_millis(500)
        );
    }

    /// Een afgebroken opname zonder sporen: genoeg om te herstellen.
    fn lege_opname(basis: &Path, naam: &str) -> PathBuf {
        let map = basis.join(naam);
        std::fs::create_dir(&map).unwrap();
        let opzet = Opzet {
            versie: 1,
            begin_unix_ms: 1,
            samplerate: DOEL_SAMPLERATE,
            stuk_seconden: 60,
            systeemgeluid: false,
            soort: vergadering::Vergadersoort::Online,
        };
        std::fs::write(
            map.join("opname.json"),
            serde_json::to_string(&opzet).unwrap(),
        )
        .unwrap();
        map
    }

    #[test]
    fn een_opname_die_niet_te_herstellen_is_houdt_de_rest_niet_tegen() {
        let basis = tempfile::tempdir().unwrap();
        // Komt als eerste aan de beurt (op naam) en faalt: het logboek is een map.
        let kapot = lege_opname(basis.path(), "vergadering-1");
        std::fs::create_dir_all(kapot.join("microfoon").join("stukken.jsonl")).unwrap();
        let goed = lege_opname(basis.path(), "vergadering-2");

        let lijst = vergadering::onafgemaakt(basis.path());
        assert_eq!(lijst, vec![kapot.clone(), goed.clone()]);
        let uitslag = herstel_lijst(lijst);
        assert_eq!(uitslag.mislukt.len(), 1);
        assert_eq!(uitslag.mislukt[0].0, kapot);
        assert_eq!(uitslag.gelukt.len(), 1);
        assert_eq!(uitslag.gelukt[0].0, goed);
        assert!(goed.join("einde.json").exists());
    }

    fn args(formaat: Option<Formaat>, uit: Option<&str>, json: bool) -> UitschrijfArgs {
        UitschrijfArgs {
            opname: None,
            model: STANDAARD_MODEL.to_string(),
            taal: "nl".to_string(),
            geen_filter: false,
            geen_ontdubbelen: false,
            zonder_sprekers: false,
            formaat,
            uit: uit.map(PathBuf::from),
            tijden: false,
            json,
        }
    }

    #[test]
    fn de_uitvoer_volgt_uit_formaat_en_pad() {
        use Uitvoer::{Bestand, Beurten, Json};
        let uitvoer = |a: &UitschrijfArgs| bepaal_uitvoer(a).map_err(|m| m.0);
        assert_eq!(uitvoer(&args(None, None, false)), Ok(Beurten));
        assert_eq!(uitvoer(&args(None, None, true)), Ok(Json));
        // Het formaat volgt uit de extensie, hoofdletters of niet; --formaat wint.
        assert_eq!(
            uitvoer(&args(None, Some("verslag.DOCX"), false)),
            Ok(Bestand(Formaat::Docx, Some(PathBuf::from("verslag.DOCX"))))
        );
        assert_eq!(
            uitvoer(&args(Some(Formaat::Pdf), Some("verslag.docx"), false)),
            Ok(Bestand(Formaat::Pdf, Some(PathBuf::from("verslag.docx"))))
        );
        // Tekstformaten mogen naar stdout, de rest niet.
        assert_eq!(
            uitvoer(&args(Some(Formaat::Srt), None, false)),
            Ok(Bestand(Formaat::Srt, None))
        );
        let fout = uitvoer(&args(Some(Formaat::Pdf), None, false)).unwrap_err();
        assert!(fout.contains("--uit"), "{fout}");
        // Een bestand dat er al is, weigeren we voordat het uitschrijven begint.
        let bestaand = tempfile::NamedTempFile::new().unwrap();
        let fout = uitvoer(&args(
            None,
            Some(bestaand.path().with_extension("txt").to_str().unwrap()),
            false,
        ));
        assert!(fout.is_ok(), "{fout:?}");
        let fout = uitvoer(&args(Some(Formaat::Txt), bestaand.path().to_str(), false)).unwrap_err();
        assert!(fout.contains("bestaat al"), "{fout}");
        // Een extensie die Mondig niet kent, zonder --formaat.
        let fout = uitvoer(&args(None, Some("verslag.bin"), false)).unwrap_err();
        assert!(fout.contains("--formaat"), "{fout}");
    }

    #[test]
    fn json_en_formaat_gaan_niet_samen() {
        #[derive(clap::Parser)]
        struct Proef {
            #[command(flatten)]
            args: UitschrijfArgs,
        }
        use clap::Parser as _;
        let ok = Proef::try_parse_from(["x", "--formaat", "VTT", "--uit", "a.vtt", "--tijden"]);
        let ok = ok.unwrap().args;
        assert_eq!(ok.formaat, Some(Formaat::Vtt));
        assert!(ok.tijden && ok.opname.is_none() && ok.taal == "nl");
        assert!(Proef::try_parse_from(["x", "--json", "--formaat", "txt"]).is_err());
        assert!(Proef::try_parse_from(["x", "--json", "--uit", "a.txt"]).is_err());
        assert!(Proef::try_parse_from(["x", "--formaat", "exe"]).is_err());
        let met_map = Proef::try_parse_from(["x", "C:/opnames/vergadering-1"]).unwrap();
        assert_eq!(
            met_map.args.opname,
            Some(PathBuf::from("C:/opnames/vergadering-1"))
        );
    }

    /// Een afgesloten opname zonder sporen: genoeg voor het zoeken naar de nieuwste.
    fn afgesloten_opname(
        basis: &Path,
        naam: &str,
        begin_unix_ms: u64,
        afgesloten: bool,
    ) -> PathBuf {
        let map = lege_opname(basis, naam);
        let opzet = Opzet {
            versie: 1,
            begin_unix_ms,
            samplerate: DOEL_SAMPLERATE,
            stuk_seconden: 60,
            systeemgeluid: false,
            soort: vergadering::Vergadersoort::Online,
        };
        std::fs::write(
            map.join("opname.json"),
            serde_json::to_string(&opzet).unwrap(),
        )
        .unwrap();
        if afgesloten {
            std::fs::write(map.join(spoor::EINDE), "{}").unwrap();
        }
        map
    }

    #[test]
    fn de_nieuwste_afgesloten_opname_wordt_gekozen() {
        let basis = tempfile::tempdir().unwrap();
        assert_eq!(nieuwste_in(basis.path()), None);
        let oud = afgesloten_opname(basis.path(), "vergadering-a", 1_000, true);
        let nieuw = afgesloten_opname(basis.path(), "vergadering-b", 5_000, true);
        // Nog nieuwer, maar niet afgesloten: telt niet.
        afgesloten_opname(basis.path(), "vergadering-c", 9_000, false);
        assert_eq!(nieuwste_in(basis.path()), Some(nieuw));
        assert_ne!(nieuwste_in(basis.path()), Some(oud));
        assert_eq!(nieuwste_in(&basis.path().join("bestaat-niet")), None);
    }

    #[test]
    fn atomair_schrijven_overschrijft_niets_en_laat_geen_rommel_achter() {
        let map = tempfile::tempdir().unwrap();
        let pad = map.path().join("verslag.txt");
        schrijf_atomair(&pad, b"een").unwrap();
        assert_eq!(std::fs::read(&pad).unwrap(), b"een");
        // Een tweede keer weigert, en het bestaande bestand blijft zoals het was.
        let fout = schrijf_atomair(&pad, b"twee").unwrap_err().0;
        assert!(fout.contains("bestaat al"), "{fout}");
        assert_eq!(std::fs::read(&pad).unwrap(), b"een");
        let fout = schrijf_atomair(&map.path().join("weg").join("a.txt"), b"x")
            .unwrap_err()
            .0;
        assert!(fout.contains("bestaat niet"), "{fout}");
        // Alleen het bestand zelf staat in de map, geen tijdelijk bestand.
        let namen: Vec<_> = std::fs::read_dir(map.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(namen, ["verslag.txt"]);
    }

    #[test]
    fn de_json_heeft_de_beurten_met_tijden() {
        use mondig_core::gesprek::{Beurt, Kant, Spoorstand};
        let gesprek = Gesprek {
            beurten: vec![Beurt {
                kant: Kant::Anderen,
                sprekernummer: None,
                start_ms: 1_000,
                eind_ms: 2_500,
                tekst: "Welkom.".to_string(),
                naam: None,
                verbeterd: false,
            }],
            duur: Duration::from_secs(61),
            meldingen: vec!["Een melding.".to_string()],
            jij: Spoorstand::Opgenomen,
            anderen: Spoorstand::Ontbrak,
            weggelaten: 0,
            soort: vergadering::Vergadersoort::Online,
        };
        let json = json_van(Path::new("opname"), "m", "nl", &gesprek);
        assert_eq!(json["duur_ms"], 61_000);
        assert_eq!(json["anderen"], "ontbrak");
        assert_eq!(json["meldingen"][0], "Een melding.");
        assert_eq!(json["weggelaten"], 0);
        assert_eq!(json["soort"], "online");
        assert_eq!(json["beurten"][0]["kant"], "anderen");
        assert_eq!(json["beurten"][0]["start_ms"], 1_000);
        assert_eq!(json["beurten"][0]["tekst"], "Welkom.");
        // Zonder sprekerherkenning: geen nummer, de naam van de kant.
        assert!(json["beurten"][0]["sprekernummer"].is_null());
        assert_eq!(json["beurten"][0]["spreker"], "Anderen");
        assert_eq!(json["sprekers"], serde_json::json!([]));
        // Met: het nummer per beurt en "Spreker 2" als naam.
        let mut met = gesprek;
        met.beurten[0].sprekernummer = Some(2);
        let json = json_van(Path::new("opname"), "m", "nl", &met);
        assert_eq!(json["beurten"][0]["sprekernummer"], 2);
        assert_eq!(json["beurten"][0]["spreker"], "Spreker 2");
        assert_eq!(json["sprekers"], serde_json::json!([2]));
    }
}
