//! `mondig-cli bench`: woordfouten en snelheid meten op een testset.
//!
//! Een set is een map met audiobestanden en per bestand een `.txt` met dezelfde naam als
//! referentie. Per model komt er een regel met woordfouten (WER, gewogen naar aantal
//! referentiewoorden), tekenfouten (CER), realtime-factor en laadtijd. Met een basislijn
//! (JSON van een eerdere run) blokkeert een achteruitgang boven de drempel de merge
//! (plan hoofdstuk 11).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use clap::Args;
use mondig_core::aanbieder::{AanbiederFout, Invoer, LokaleEngine};
use mondig_core::download::{self, ModelStatus};
use mondig_core::engine::protocol::Apparaat;
use mondig_core::engine::supervisor::Instellingen;
use mondig_core::filter::{FilterInstellingen, Uitkomst};
use mondig_core::meten::{Tekenfouten, Woordfouten, tekenfouten, woordfouten};
use mondig_core::model::{self, Model};
use serde::{Deserialize, Serialize};

use crate::{Melding, engine_melding, engine_pad, uit};

/// Wat de bench meeneemt: alles wat Mondig kan lezen (één lijst in `mondig-core`).
const AUDIO_EXTENSIES: &[&str] = mondig_core::audio::EXTENSIES;

/// Keuze van het apparaat voor de meting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum ApparaatKeuze {
    /// Alleen de processor.
    Cpu,
    /// De videokaart (met terugval op de processor).
    Gpu,
}

/// Opties van `mondig-cli bench`.
#[derive(Args)]
pub(crate) struct BenchOpties {
    /// Map met audiobestanden en per bestand een .txt met de referentie.
    #[arg(long)]
    set: PathBuf,
    /// Model uit `mondig-cli modellen`; herhaalbaar. Standaard alle gedownloade modellen.
    #[arg(long = "model")]
    modellen: Vec<String>,
    /// Geef de uitkomst als JSON op stdout.
    #[arg(long)]
    json: bool,
    /// JSON-bestand van een eerdere run om mee te vergelijken.
    #[arg(long)]
    basislijn: Option<PathBuf>,
    /// Toegestane achteruitgang in woordfouten, in procentpunt, voor de exitcode 1.
    #[arg(long, default_value_t = 1.0)]
    drempel: f64,
    /// Zet het filter tegen verzonnen tekst uit (voor ruwe metingen).
    #[arg(long)]
    geen_filter: bool,
    /// Taalcode (ISO 639-1), voor modellen die een taal nodig hebben.
    #[arg(long, default_value = "nl")]
    taal: String,
    /// Gewenst apparaat: `cpu` (standaard) of `gpu`. Bij een videokaartfout valt de
    /// supervisor terug op de processor; de tabel toont waar het model echt draaide.
    #[arg(long, value_enum, default_value_t = ApparaatKeuze::Cpu)]
    apparaat: ApparaatKeuze,
    /// Meet alleen de eerste n bestanden (op volgorde van naam), bijvoorbeeld voor trage modellen.
    #[arg(long)]
    max_bestanden: Option<usize>,
}

/// Meetresultaat van één model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelMeting {
    model: String,
    naam: String,
    /// Vaste commit van het modelbestand op Hugging Face, ter herleiding.
    #[serde(default)]
    modelcommit: String,
    bestanden: usize,
    audio_s: f64,
    referentiewoorden: usize,
    vervangen: usize,
    weggelaten: usize,
    ingevoegd: usize,
    /// Fractie, bijvoorbeeld 0,05 voor 5 procent.
    wer: f64,
    cer: f64,
    /// Verwerkingstijd gedeeld door audioduur; lager is sneller.
    realtime_factor: f64,
    laadtijd_s: f64,
    /// Apparaat waarop het model echt draaide (`cpu` of `gpu`); leeg bij oudere metingen.
    #[serde(default)]
    apparaat: String,
    /// Bestanden waar het filter een andere uitkomst gaf dan gewone tekst.
    #[serde(default)]
    filter_afwijkend: usize,
    /// Aantal ingrepen van het filter (inkorten, verwijderen).
    #[serde(default)]
    filter_ingrepen: usize,
}

/// Alle metingen van één run, ook het formaat van de basislijn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Rapport {
    datum: String,
    platform: String,
    set: String,
    filter: bool,
    modellen: Vec<ModelMeting>,
}

/// Eén testbestand met zijn referentie.
struct Geval {
    naam: String,
    audio: PathBuf,
    referentie: String,
}

/// Zoekt de audiobestanden met een `.txt` ernaast, op volgorde van naam.
fn lees_set(map: &Path) -> Result<Vec<Geval>, Melding> {
    let mut paden: Vec<PathBuf> = fs::read_dir(map)
        .map_err(|e| Melding(format!("De set {} is niet te lezen: {e}", map.display())))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| AUDIO_EXTENSIES.contains(&e.to_lowercase().as_str()))
        })
        .collect();
    paden.sort();

    let mut gevallen: Vec<Geval> = Vec::new();
    for pad in paden {
        let txt = pad.with_extension("txt");
        let naam = pad
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Een .opus naast een .wav met dezelfde naam is hetzelfde fragment: eerste wint.
        if gevallen.iter().any(|g| g.naam == naam) {
            continue;
        }
        if !txt.is_file() {
            eprintln!("Overgeslagen: {naam} heeft geen .txt met referentie.");
            continue;
        }
        let referentie = fs::read_to_string(&txt)
            .map_err(|e| Melding(format!("{} is niet te lezen: {e}", txt.display())))?;
        gevallen.push(Geval {
            naam,
            audio: pad,
            referentie,
        });
    }
    // Liever wav boven opus als beide bestaan, zodat de meting vergelijkbaar blijft met de
    // basislijn (die op wav gemeten is). Mondig leest Opus sinds fase 4 ook zelf.
    for geval in &mut gevallen {
        let wav = geval.audio.with_extension("wav");
        if wav.is_file() {
            geval.audio = wav;
        }
    }
    if gevallen.is_empty() {
        return Err(Melding(format!(
            "In {} staan geen audiobestanden met een .txt-referentie.",
            map.display()
        )));
    }
    Ok(gevallen)
}

/// Commit uit het adres van het eerste modelbestand (`/resolve/<commit>/`).
fn modelcommit(model: &Model) -> String {
    model
        .bestanden
        .first()
        .and_then(|b| b.url.split_once("/resolve/"))
        .and_then(|(_, rest)| rest.split('/').next())
        .unwrap_or_default()
        .to_string()
}

/// Meet één model op alle gevallen.
fn meet_model(
    model: &Model,
    gevallen: &[Geval],
    opties: &BenchOpties,
    engine_exe: &Path,
) -> Result<ModelMeting, Melding> {
    let map = model::modelmap(&model.id)?;
    if download::status(model, &map) != ModelStatus::Compleet {
        return Err(Melding(format!(
            "Het model {} is nog niet (helemaal) gedownload. Haal het binnen met: mondig-cli modellen download {}",
            model.naam, model.id
        )));
    }
    let mut engine = LokaleEngine::nieuw(
        Instellingen::standaard(engine_exe.to_path_buf()),
        model,
        map,
        match opties.apparaat {
            ApparaatKeuze::Cpu => Apparaat::Cpu,
            ApparaatKeuze::Gpu => Apparaat::Gpu,
        },
        Some(opties.taal.clone()),
    );
    eprintln!("== {} ==", model.naam);
    let start = Instant::now();
    let gebruikt = engine.laad().map_err(|f| engine_melding(&f))?;
    let laadtijd = start.elapsed().as_secs_f64();

    // Zonder stopwoorden: de basislijn is zonder gemeten, en een voorgelezen referentie kan een
    // "uh" bevatten (ADR 0028). Zo blijft de bench vergelijkbaar en telt het weghalen niet mee
    // als ingreep.
    let filter = FilterInstellingen::standaard().met_stopwoorden(false);
    let mut woorden = Woordfouten::default();
    let mut tekens = Tekenfouten::default();
    let mut verwerkingstijd = 0.0_f64;
    let mut audio_s = 0.0_f64;
    let mut afwijkend = 0;
    let mut ingrepen = 0;

    for (nr, geval) in gevallen.iter().enumerate() {
        let start = Instant::now();
        let (tekst, duur, uitkomst, aantal_ingrepen) = if opties.geen_filter {
            use mondig_core::aanbieder::Aanbieder;
            let t = engine
                .transcribeer(Invoer::Bestand(&geval.audio))
                .map_err(aanbieder_melding)?;
            (t.tekst(), t.audio_duur, Uitkomst::Tekst, 0)
        } else {
            let g = engine
                .transcribeer_gefilterd(Invoer::Bestand(&geval.audio), &filter)
                .map_err(aanbieder_melding)?;
            (
                g.transcript.tekst(),
                g.transcript.audio_duur,
                g.uitkomst,
                g.ingrepen.len(),
            )
        };
        let tijd = start.elapsed().as_secs_f64();
        verwerkingstijd += tijd;
        audio_s += duur.as_secs_f64();
        if uitkomst != Uitkomst::Tekst {
            afwijkend += 1;
        }
        ingrepen += aantal_ingrepen;

        let w = woordfouten(&geval.referentie, &tekst);
        woorden += w;
        tekens += tekenfouten(&geval.referentie, &tekst);
        eprintln!(
            "  [{}/{}] {}: WER {} ({} s)",
            nr + 1,
            gevallen.len(),
            geval.naam,
            procent(w.wer()),
            komma(tijd, 1)
        );
    }

    Ok(ModelMeting {
        model: model.id.clone(),
        naam: model.naam.clone(),
        modelcommit: modelcommit(model),
        bestanden: gevallen.len(),
        audio_s,
        referentiewoorden: woorden.referentiewoorden,
        vervangen: woorden.vervangen,
        weggelaten: woorden.weggelaten,
        ingevoegd: woorden.ingevoegd,
        wer: woorden.wer(),
        cer: tekens.cer(),
        realtime_factor: if audio_s > 0.0 {
            verwerkingstijd / audio_s
        } else {
            0.0
        },
        laadtijd_s: laadtijd,
        apparaat: match gebruikt {
            Apparaat::Cpu => "cpu",
            Apparaat::Gpu => "gpu",
        }
        .to_string(),
        filter_afwijkend: afwijkend,
        filter_ingrepen: ingrepen,
    })
}

fn aanbieder_melding(fout: AanbiederFout) -> Melding {
    match fout {
        AanbiederFout::Audio(f) => Melding(f.to_string()),
        AanbiederFout::Engine(f) => engine_melding(&f),
    }
}

/// Getal met decimale komma.
fn komma(waarde: f64, cijfers: usize) -> String {
    format!("{waarde:.cijfers$}").replace('.', ",")
}

fn procent(fractie: f64) -> String {
    format!("{} %", komma(fractie * 100.0, 1))
}

/// Datum van vandaag (UTC) als `jjjj-mm-dd`, zonder extra crate.
fn datum_vandaag() -> String {
    let seconden = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let dagen = i64::try_from(seconden / 86_400).unwrap_or(0);
    // Burgerlijke datum uit dagen sinds 1970-01-01 (algoritme van Howard Hinnant).
    let z = dagen + 719_468;
    let era = z.div_euclid(146_097);
    let dag_van_era = z.rem_euclid(146_097);
    let jaar_van_era =
        (dag_van_era - dag_van_era / 1460 + dag_van_era / 36_524 - dag_van_era / 146_096) / 365;
    let dag_van_jaar = dag_van_era - (365 * jaar_van_era + jaar_van_era / 4 - jaar_van_era / 100);
    let mp = (5 * dag_van_jaar + 2) / 153;
    let dag = dag_van_jaar - (153 * mp + 2) / 5 + 1;
    let maand = if mp < 10 { mp + 3 } else { mp - 9 };
    let jaar = jaar_van_era + era * 400 + i64::from(maand <= 2);
    format!("{jaar:04}-{maand:02}-{dag:02}")
}

fn toon_tabel(rapport: &Rapport, bestanden: usize, audio_s: f64) -> Result<(), Melding> {
    uit(&format!(
        "Set: {} ({bestanden} bestanden, {} s audio), filter {}",
        rapport.set,
        komma(audio_s, 0),
        if rapport.filter { "aan" } else { "uit" }
    ))?;
    uit(&format!(
        "{:<30} {:>5} {:>8} {:>8} {:>8} {:>9} {:>7} {:>4}",
        "Model", "Best.", "Woorden", "WER", "CER", "Realtime", "Laden", "App."
    ))?;
    for m in &rapport.modellen {
        uit(&format!(
            "{:<30} {:>5} {:>8} {:>8} {:>8} {:>9} {:>6}s {:>4}",
            m.model,
            m.bestanden,
            m.referentiewoorden,
            procent(m.wer),
            procent(m.cer),
            komma(m.realtime_factor, 3),
            komma(m.laadtijd_s, 1),
            m.apparaat
        ))?;
    }
    Ok(())
}

/// Vergelijkt met de basislijn. Geeft `true` als een model meer dan `drempel`
/// procentpunt slechter is. Met `naar_stderr` gaan de regels naar stderr.
fn vergelijk(
    rapport: &Rapport,
    basislijn: &Rapport,
    drempel: f64,
    naar_stderr: bool,
) -> Result<bool, Melding> {
    // Bij JSON op stdout moeten de vergelijkingsregels op stderr, anders is de JSON niet leesbaar.
    let uit = |regel: &str| -> Result<(), Melding> {
        if naar_stderr {
            eprintln!("{regel}");
            Ok(())
        } else {
            uit(regel)
        }
    };
    uit(&format!(
        "\nVergelijking met basislijn van {} ({}), drempel {} procentpunt:",
        basislijn.datum,
        basislijn.platform,
        komma(drempel, 1)
    ))?;
    let mut regressie = false;
    for m in &rapport.modellen {
        let Some(basis) = basislijn.modellen.iter().find(|b| b.model == m.model) else {
            uit(&format!(
                "  {}: niet in de basislijn, niet vergeleken.",
                m.model
            ))?;
            continue;
        };
        let verschil = (m.wer - basis.wer) * 100.0;
        let oordeel = if verschil > drempel {
            regressie = true;
            "TE SLECHT"
        } else {
            "ok"
        };
        let mut regel = format!(
            "  {}: WER {} tegen {} ({}{} procentpunt) {oordeel}",
            m.model,
            procent(m.wer),
            procent(basis.wer),
            if verschil >= 0.0 { "+" } else { "" },
            komma(verschil, 2)
        );
        if m.bestanden != basis.bestanden {
            regel = format!(
                "{regel} [let op: {} bestanden tegen {} in de basislijn]",
                m.bestanden, basis.bestanden
            );
        }
        uit(&regel)?;
    }
    if regressie {
        uit(&format!(
            "\nAchteruitgang: de woordfouten zijn bij minstens een model meer dan {} procentpunt gestegen. Zoek de oorzaak of overleg of de basislijn bewust omhoog mag.",
            komma(drempel, 1)
        ))?;
    }
    Ok(regressie)
}

/// Voert `bench` uit.
pub(crate) fn voer_uit(opties: &BenchOpties) -> Result<ExitCode, Melding> {
    let mut gevallen = lees_set(&opties.set)?;
    if let Some(max) = opties.max_bestanden {
        gevallen.truncate(max);
    }
    let catalogus = model::spraakmodellen()?;
    let modellen: Vec<Model> = if opties.modellen.is_empty() {
        catalogus
            .into_iter()
            .filter(|m| {
                model::modelmap(&m.id)
                    .is_ok_and(|map| download::status(m, &map) == ModelStatus::Compleet)
            })
            .collect()
    } else {
        opties
            .modellen
            .iter()
            .map(|id| model::zoek(id).map_err(Melding::from))
            .collect::<Result<_, _>>()?
    };
    if modellen.is_empty() {
        return Err(Melding(
            "Er is geen gedownload model. Haal er een binnen met: mondig-cli modellen download <id>".into(),
        ));
    }
    let basislijn = opties
        .basislijn
        .as_ref()
        .map(|pad| {
            let tekst = fs::read_to_string(pad).map_err(|e| {
                Melding(format!(
                    "De basislijn {} is niet te lezen: {e}",
                    pad.display()
                ))
            })?;
            serde_json::from_str::<Rapport>(&tekst).map_err(|e| {
                Melding(format!(
                    "De basislijn {} is geen geldig bench-bestand: {e}",
                    pad.display()
                ))
            })
        })
        .transpose()?;

    let engine_exe = engine_pad()?;
    let mut metingen = Vec::new();
    for m in &modellen {
        metingen.push(meet_model(m, &gevallen, opties, &engine_exe)?);
    }
    let audio_s = metingen.first().map_or(0.0, |m| m.audio_s);
    let rapport = Rapport {
        datum: datum_vandaag(),
        platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        set: opties.set.file_name().map_or_else(
            || opties.set.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        ),
        filter: !opties.geen_filter,
        modellen: metingen,
    };

    if opties.json {
        uit(&serde_json::to_string_pretty(&rapport)?)?;
    } else {
        eprintln!();
        toon_tabel(&rapport, gevallen.len(), audio_s)?;
    }
    if let Some(basislijn) = basislijn
        && vergelijk(&rapport, &basislijn, opties.drempel, opties.json)?
    {
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datum_is_burgerlijk_formaat() {
        let d = datum_vandaag();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20"));
    }

    #[test]
    fn komma_en_procent() {
        assert_eq!(komma(1.234, 1), "1,2");
        assert_eq!(procent(0.0512), "5,1 %");
    }

    fn meting(model: &str, wer: f64) -> ModelMeting {
        ModelMeting {
            model: model.into(),
            naam: model.into(),
            modelcommit: String::new(),
            bestanden: 10,
            audio_s: 100.0,
            referentiewoorden: 100,
            vervangen: 0,
            weggelaten: 0,
            ingevoegd: 0,
            wer,
            cer: 0.0,
            realtime_factor: 0.1,
            laadtijd_s: 1.0,
            apparaat: "cpu".into(),
            filter_afwijkend: 0,
            filter_ingrepen: 0,
        }
    }

    fn rapport(wer: f64) -> Rapport {
        Rapport {
            datum: "2026-10-31".into(),
            platform: "test".into(),
            set: "set".into(),
            filter: true,
            modellen: vec![meting("a", wer)],
        }
    }

    #[test]
    fn vergelijking_slaagt_binnen_de_drempel() {
        assert!(!vergelijk(&rapport(0.050), &rapport(0.045), 1.0, false).unwrap());
        assert!(!vergelijk(&rapport(0.040), &rapport(0.045), 1.0, false).unwrap());
    }

    #[test]
    fn vergelijking_faalt_boven_de_drempel() {
        assert!(vergelijk(&rapport(0.060), &rapport(0.045), 1.0, false).unwrap());
    }

    #[test]
    fn rapport_overleeft_json() {
        let tekst = serde_json::to_string(&rapport(0.05)).unwrap();
        let terug: Rapport = serde_json::from_str(&tekst).unwrap();
        assert_eq!(terug.modellen.len(), 1);
    }
}
