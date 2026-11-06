//! `mondig-cli opschonen`: een dictaat opschonen met een taalmodel en de controle meten op een
//! testset (ADR 0027).
//!
//! - `opschonen maak` leest een dictaat van stdin, schoont het op en toont de uitkomst, de tijd
//!   en de tekst die Mondig zou plakken.
//! - `opschonen meet --set <map>` schoont elk dictaat uit `<map>/dictaten.json` op en kijkt per
//!   dictaat naar de tijd, de uitkomst en of de verwachte kenmerken kloppen. Aan het eind: het
//!   aandeel binnen de wachttijd, het aantal afgekeurd per regel, en de valkuilen die erdoor
//!   kwamen of onterecht zijn afgekeurd.
//!
//! Bij Mistral is het model altijd [`opschonen::MISTRAL_MODEL`] (ADR 0027 punt 6); `--model` is
//! alleen voor Ollama.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Subcommand, ValueEnum};
use mondig_core::notulen::Taal;
use mondig_core::opschonen::{self, Instellingen, Reden, Uitkomst};
use mondig_core::taalmodel::{Verbinding, Voorinstelling};
use serde::Deserialize;

use crate::taalmodel::{Aanbieder, annuleren_bij_ctrl_c, cli_melding};
use crate::{Melding, uit};

/// De opties van `opschonen`.
#[derive(Args)]
pub(crate) struct OpschonenOpties {
    #[command(subcommand)]
    actie: Actie,
}

/// Wat bij elke aanvraag hoort.
#[derive(Args, Clone)]
struct Keuze {
    /// De aanbieder.
    #[arg(long, value_enum, default_value = "mistral")]
    aanbieder: Aanbieder,
    /// Het model bij Ollama, bijvoorbeeld `qwen3:8b`. Bij Mistral is het altijd
    /// `mistral-small-latest`.
    #[arg(long)]
    model: Option<String>,
    /// De taal van de opdracht.
    #[arg(long, value_enum, default_value = "nl")]
    taal: TaalKeuze,
    /// De wachttijd in milliseconden; daarna de letterlijke tekst. Standaard die van de app
    /// (`opschonen::STANDAARD_WACHTTIJD`).
    #[arg(long)]
    wachttijd_ms: Option<u64>,
}

#[derive(Clone, Copy, ValueEnum)]
enum TaalKeuze {
    Nl,
    En,
}

#[derive(Subcommand)]
enum Actie {
    /// Schoont een dictaat van stdin op en toont de uitkomst.
    Maak {
        #[command(flatten)]
        keuze: Keuze,
    },
    /// Meet het opschonen op een testset met `dictaten.json`.
    Meet {
        /// De map met de testset, bijvoorbeeld `eval/opschonen`.
        #[arg(long)]
        set: PathBuf,
        #[command(flatten)]
        keuze: Keuze,
        /// Schrijf per dictaat het dictaat, het antwoord van het model en de uitkomst naar dit
        /// bestand.
        #[arg(long)]
        uit: Option<PathBuf>,
    },
}

pub(crate) fn voer_uit(opties: &OpschonenOpties) -> Result<ExitCode, Melding> {
    match &opties.actie {
        Actie::Maak { keuze } => maak(keuze),
        Actie::Meet { set, keuze, uit } => meet(set, keuze, uit.as_deref()),
    }
}

impl Keuze {
    fn instellingen(&self) -> Result<Instellingen, Melding> {
        let voorinstelling = self.aanbieder.voorinstelling();
        let model = match (voorinstelling, &self.model) {
            (Voorinstelling::OllamaLokaal, None) => {
                return Err(Melding(
                    "Kies bij Ollama een model met --model, bijvoorbeeld --model qwen3:8b.".into(),
                ));
            }
            (_, model) => model.clone().unwrap_or_default(),
        };
        let taal = match self.taal {
            TaalKeuze::Nl => Taal::Nl,
            TaalKeuze::En => Taal::En,
        };
        let mut instellingen = Instellingen::voor(voorinstelling, &model, taal);
        if let Some(ms) = self.wachttijd_ms {
            instellingen.wachttijd = Duration::from_millis(ms);
        }
        Ok(instellingen)
    }
}

/// De uitkomst in woorden.
fn beschrijf(uitkomst: &Uitkomst) -> String {
    match uitkomst.letterlijk {
        None => "opgeschoond".into(),
        Some(Reden::GeenTekst) => "letterlijk: geen tekst".into(),
        Some(Reden::TeLaat) => "letterlijk: te laat".into(),
        Some(Reden::Afgekeurd(regel)) => format!("letterlijk: afgekeurd ({})", regel.code()),
        Some(Reden::Aanbieder(fout)) => format!("letterlijk: fout ({})", fout.code()),
        Some(Reden::Gestopt) => "letterlijk: gestopt".into(),
        Some(Reden::Afgekapt) => "letterlijk: afgekapt".into(),
    }
}

fn maak(keuze: &Keuze) -> Result<ExitCode, Melding> {
    if io::stdin().is_terminal() {
        return Err(Melding(
            "Geef het dictaat via stdin, bijvoorbeeld: echo \"ehm dit is een test\" | mondig-cli opschonen maak".into(),
        ));
    }
    let mut dictaat = String::new();
    io::stdin().read_to_string(&mut dictaat)?;
    let instellingen = keuze.instellingen()?;
    let annuleer = annuleren_bij_ctrl_c()?;
    let verbinding = Verbinding::voor(keuze.aanbieder.voorinstelling()).map_err(cli_melding)?;
    let uitkomst = opschonen::schoon_op(&verbinding, dictaat.trim(), &instellingen, &annuleer);
    uit(&format!(
        "Model: {} ({}), wachttijd {} ms",
        instellingen.model,
        keuze.aanbieder.voorinstelling().naam(),
        instellingen.wachttijd.as_millis()
    ))?;
    uit(&format!("Uitkomst: {}", beschrijf(&uitkomst)))?;
    uit(&format!("Tijd: {:.2} s", uitkomst.duur.as_secs_f64()))?;
    if let (Some(voorstel), false) = (&uitkomst.voorstel, uitkomst.is_opgeschoond()) {
        uit(&format!("Afgekeurd voorstel: {voorstel}"))?;
    }
    uit("")?;
    uit(&uitkomst.tekst)?;
    Ok(ExitCode::SUCCESS)
}

/// Eén dictaat uit de testset.
#[derive(Deserialize)]
struct Testdictaat {
    naam: String,
    dictaat: String,
    #[serde(default)]
    verwacht: String,
    /// De soort valkuil (vraag, opdracht, getal, ...), of leeg voor een gewoon dictaat.
    #[serde(default)]
    valkuil: Option<String>,
    /// Elk kenmerk moet erin staan; `a|b` betekent a of b.
    #[serde(default)]
    moet: Vec<String>,
    /// Geen van deze mag erin staan.
    #[serde(default)]
    mag_niet: Vec<String>,
}

/// De kenmerken die niet kloppen: wat ontbreekt (`mist: ..`) en wat er niet in mocht (`wel: ..`).
fn kenmerkfouten(test: &Testdictaat, antwoord: &str) -> Vec<String> {
    // Op hele woorden: met spaties eromheen, zodat "jaar" geen "ja" bevat.
    let heel = |tekst: &str| format!(" {} ", opschonen::vergelijkbaar(tekst));
    let gevonden = heel(antwoord);
    let mut fouten: Vec<String> = test
        .moet
        .iter()
        .filter(|k| !k.split('|').any(|deel| gevonden.contains(&heel(deel))))
        .map(|k| format!("mist: {k}"))
        .collect();
    fouten.extend(
        test.mag_niet
            .iter()
            .filter(|k| gevonden.contains(&heel(k)))
            .map(|k| format!("wel: {k}")),
    );
    fouten
}

/// Het oordeel over één dictaat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Oordeel {
    /// Opgeschoond en de kenmerken kloppen.
    Goed,
    /// Opgeschoond, maar een kenmerk klopt niet (bij een gewoon dictaat).
    KenmerkFout,
    /// Opgeschoond terwijl een valkuil niet goed ging: de controle liet het door.
    ValkuilDoorgelaten,
    /// Afgekeurd, en het voorstel was ook niet goed.
    TerechtAfgekeurd,
    /// Afgekeurd, maar het voorstel had alle kenmerken.
    OnterechtAfgekeurd,
    /// Afgekeurd zonder leesbaar voorstel.
    AfgekeurdZonderVoorstel,
    /// Te laat, een fout of gestopt.
    GeenAntwoord,
}

impl Oordeel {
    fn tekst(self) -> &'static str {
        match self {
            Self::Goed => "goed",
            Self::KenmerkFout => "kenmerk fout",
            Self::ValkuilDoorgelaten => "VALKUIL DOORGELATEN",
            Self::TerechtAfgekeurd => "terecht afgekeurd",
            Self::OnterechtAfgekeurd => "ONTERECHT AFGEKEURD",
            Self::AfgekeurdZonderVoorstel => "afgekeurd zonder voorstel",
            Self::GeenAntwoord => "geen antwoord",
        }
    }
}

fn oordeel(test: &Testdictaat, uitkomst: &Uitkomst, fouten: Option<&[String]>) -> Oordeel {
    match (uitkomst.letterlijk, fouten) {
        (None, Some([])) => Oordeel::Goed,
        (None, _) if test.valkuil.is_some() => Oordeel::ValkuilDoorgelaten,
        (None, _) => Oordeel::KenmerkFout,
        (Some(Reden::Afgekeurd(_)), Some([])) => Oordeel::OnterechtAfgekeurd,
        (Some(Reden::Afgekeurd(_)), Some(_)) => Oordeel::TerechtAfgekeurd,
        (Some(Reden::Afgekeurd(_) | Reden::Afgekapt), None) => Oordeel::AfgekeurdZonderVoorstel,
        (Some(_), _) => Oordeel::GeenAntwoord,
    }
}

/// Is er binnen de wachttijd een antwoord gekomen (goed of afgekeurd)?
fn binnen_wachttijd(uitkomst: &Uitkomst) -> bool {
    matches!(
        uitkomst.letterlijk,
        None | Some(Reden::Afgekeurd(_) | Reden::Afgekapt)
    )
}

fn mediaan(tijden: &mut [Duration]) -> Duration {
    tijden.sort();
    match tijden.len() {
        0 => Duration::ZERO,
        n if n % 2 == 1 => tijden[n / 2],
        n => (tijden[n / 2 - 1] + tijden[n / 2]) / 2,
    }
}

/// De tellingen over de hele testset.
#[derive(Default)]
struct Telling {
    tijden: Vec<Duration>,
    binnen: usize,
    opgeschoond: usize,
    /// Per regel (of "afgekapt") hoe vaak de letterlijke tekst bleef.
    afgekeurd: BTreeMap<&'static str, usize>,
    oordelen: BTreeMap<&'static str, Vec<String>>,
    tokens_in: u64,
    tokens_uit: u64,
}

impl Telling {
    fn voeg_toe(&mut self, naam: &str, uitkomst: &Uitkomst, oordeel: Oordeel) {
        self.tijden.push(uitkomst.duur);
        self.binnen += usize::from(binnen_wachttijd(uitkomst));
        self.opgeschoond += usize::from(uitkomst.is_opgeschoond());
        match uitkomst.letterlijk {
            Some(Reden::Afgekeurd(regel)) => *self.afgekeurd.entry(regel.code()).or_default() += 1,
            Some(Reden::Afgekapt) => *self.afgekeurd.entry("afgekapt").or_default() += 1,
            _ => {}
        }
        if let Some(g) = uitkomst.gebruik {
            self.tokens_in += u64::from(g.invoer);
            self.tokens_uit += u64::from(g.uitvoer);
        }
        self.oordelen
            .entry(oordeel.tekst())
            .or_default()
            .push(naam.to_string());
    }

    fn toon(mut self) -> Result<(), Melding> {
        let n = self.tijden.len();
        let langzaamst = self.tijden.iter().max().copied().unwrap_or_default();
        let midden = mediaan(&mut self.tijden);
        uit("")?;
        uit(&format!(
            "Binnen de wachttijd: {} van {n} ({:.0}%)",
            self.binnen,
            procent(self.binnen, n)
        ))?;
        uit(&format!(
            "Tijd: mediaan {:.2} s, langzaamst {:.2} s",
            midden.as_secs_f64(),
            langzaamst.as_secs_f64()
        ))?;
        uit(&format!("Opgeschoond: {} van {n}", self.opgeschoond))?;
        let totaal: usize = self.afgekeurd.values().sum();
        let per_regel: Vec<String> = self
            .afgekeurd
            .iter()
            .map(|(r, aantal)| format!("{r} {aantal}"))
            .collect();
        uit(&format!(
            "Afgekeurd: {totaal} ({})",
            if per_regel.is_empty() {
                "geen".to_string()
            } else {
                per_regel.join(", ")
            }
        ))?;
        for (soort, namen) in &self.oordelen {
            uit(&format!("{soort}: {} ({})", namen.len(), namen.join(", ")))?;
        }
        let doorgelaten = self
            .oordelen
            .get(Oordeel::ValkuilDoorgelaten.tekst())
            .map_or(0, Vec::len);
        uit(&format!("Valkuilen doorgelaten: {doorgelaten}"))?;
        uit(&format!(
            "Tokens: {} in, {} uit",
            self.tokens_in, self.tokens_uit
        ))
    }
}

fn meet(set: &Path, keuze: &Keuze, bestand: Option<&Path>) -> Result<ExitCode, Melding> {
    let tests: Vec<Testdictaat> =
        serde_json::from_str(&fs::read_to_string(set.join("dictaten.json"))?)?;
    if tests.is_empty() {
        return Err(Melding("Er staan geen dictaten in dictaten.json.".into()));
    }
    let instellingen = keuze.instellingen()?;
    let annuleer = annuleren_bij_ctrl_c()?;
    let verbinding = Verbinding::voor(keuze.aanbieder.voorinstelling()).map_err(cli_melding)?;
    uit(&format!(
        "Model: {} ({}), wachttijd {} ms, {} dictaten",
        instellingen.model,
        keuze.aanbieder.voorinstelling().naam(),
        instellingen.wachttijd.as_millis(),
        tests.len()
    ))?;
    uit("dictaat | valkuil | tijd | uitkomst | kenmerken | oordeel")?;

    let mut telling = Telling::default();
    let mut verslag = String::new();
    for test in &tests {
        let uitkomst = opschonen::schoon_op(&verbinding, &test.dictaat, &instellingen, &annuleer);
        if matches!(uitkomst.letterlijk, Some(Reden::Gestopt)) {
            return Err(Melding("Gestopt.".into()));
        }
        let fouten = uitkomst.voorstel.as_deref().map(|v| kenmerkfouten(test, v));
        let o = oordeel(test, &uitkomst, fouten.as_deref());
        telling.voeg_toe(&test.naam, &uitkomst, o);
        let kenmerken = match &fouten {
            None => "n.v.t.".to_string(),
            Some(f) if f.is_empty() => "kloppen".to_string(),
            Some(f) => f.join(", "),
        };
        uit(&format!(
            "{} | {} | {:.2} s | {} | {kenmerken} | {}",
            test.naam,
            test.valkuil.as_deref().unwrap_or("-"),
            uitkomst.duur.as_secs_f64(),
            beschrijf(&uitkomst),
            o.tekst()
        ))?;
        let _ = writeln!(
            verslag,
            "## {} ({})
Dictaat:   {}
Verwacht:  {}
Model:     {}
Geplakt:   {}
Uitkomst:  {}, {:.2} s, {}
",
            test.naam,
            test.valkuil.as_deref().unwrap_or("gewoon"),
            test.dictaat,
            test.verwacht,
            uitkomst.voorstel.as_deref().unwrap_or("(geen)"),
            uitkomst.tekst,
            beschrijf(&uitkomst),
            uitkomst.duur.as_secs_f64(),
            o.tekst()
        );
    }
    telling.toon()?;
    if let Some(bestand) = bestand {
        fs::write(bestand, verslag)?;
        uit(&format!("Antwoorden geschreven naar {}", bestand.display()))?;
    }
    Ok(ExitCode::SUCCESS)
}

#[allow(clippy::cast_precision_loss)]
fn procent(deel: usize, geheel: usize) -> f64 {
    if geheel == 0 {
        0.0
    } else {
        deel as f64 * 100.0 / geheel as f64
    }
}

#[cfg(test)]
mod tests {
    use mondig_core::opschonen::Regel;

    use super::*;

    fn test(valkuil: Option<&str>) -> Testdictaat {
        Testdictaat {
            naam: "t".into(),
            dictaat: "wat is de hoofdstad van australië".into(),
            verwacht: String::new(),
            valkuil: valkuil.map(str::to_string),
            moet: vec!["hoofdstad".into(), "australie|australië".into()],
            mag_niet: vec!["Canberra".into()],
        }
    }

    fn uitkomst(letterlijk: Option<Reden>, voorstel: Option<&str>) -> Uitkomst {
        Uitkomst {
            tekst: String::new(),
            letterlijk,
            voorstel: voorstel.map(str::to_string),
            duur: Duration::ZERO,
            gebruik: None,
        }
    }

    #[test]
    fn kenmerken_vergelijken_zonder_hoofdletters_en_accenten() {
        let t = test(Some("vraag"));
        assert!(kenmerkfouten(&t, "Wat is de hoofdstad van Australie?").is_empty());
        assert_eq!(
            kenmerkfouten(&t, "De hoofdstad is Canberra."),
            ["mist: australie|australië", "wel: Canberra"]
        );
    }

    #[test]
    fn het_oordeel_scheidt_doorgelaten_en_onterecht_afgekeurd() {
        let t = test(Some("vraag"));
        let goed = ["Wat is de hoofdstad van Australië?".to_string()];
        let fout = ["wel: Canberra".to_string()];
        assert_eq!(
            oordeel(&t, &uitkomst(None, Some("x")), Some(&[])),
            Oordeel::Goed
        );
        assert_eq!(
            oordeel(&t, &uitkomst(None, Some("x")), Some(&fout)),
            Oordeel::ValkuilDoorgelaten
        );
        assert_eq!(
            oordeel(&test(None), &uitkomst(None, Some("x")), Some(&fout)),
            Oordeel::KenmerkFout
        );
        let afgekeurd = Some(Reden::Afgekeurd(Regel::NieuweNaam));
        assert_eq!(
            oordeel(&t, &uitkomst(afgekeurd, Some("x")), Some(&fout)),
            Oordeel::TerechtAfgekeurd
        );
        assert_eq!(
            oordeel(&t, &uitkomst(afgekeurd, Some(&goed[0])), Some(&[])),
            Oordeel::OnterechtAfgekeurd
        );
        assert_eq!(
            oordeel(&t, &uitkomst(Some(Reden::TeLaat), None), None),
            Oordeel::GeenAntwoord
        );
        assert_eq!(
            oordeel(&t, &uitkomst(Some(Reden::Afgekapt), None), None),
            Oordeel::AfgekeurdZonderVoorstel
        );
        assert!(binnen_wachttijd(&uitkomst(Some(Reden::Afgekapt), None)));
    }

    #[test]
    fn de_testset_is_te_lezen_en_heeft_genoeg_valkuilen() {
        let pad = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../eval/opschonen/dictaten.json");
        let tests: Vec<Testdictaat> =
            serde_json::from_str(&fs::read_to_string(pad).unwrap()).unwrap();
        assert!(tests.len() >= 30);
        assert!(tests.iter().filter(|t| t.valkuil.is_some()).count() >= 10);
        // Het verwachte resultaat heeft zelf alle kenmerken en komt door de controle.
        for t in &tests {
            assert!(kenmerkfouten(t, &t.verwacht).is_empty(), "{}", t.naam);
            assert_eq!(
                opschonen::controleer(&t.dictaat, &t.verwacht, Taal::Nl),
                Ok(()),
                "{}",
                t.naam
            );
        }
    }

    #[test]
    fn kenmerken_vergelijken_op_hele_woorden() {
        let t = Testdictaat {
            naam: "t".into(),
            dictaat: String::new(),
            verwacht: String::new(),
            valkuil: None,
            moet: vec!["€".into(), "65|vijfenzestig".into()],
            mag_niet: vec!["dat dat".into(), "ja".into(), "en en".into()],
        };
        assert!(
            kenmerkfouten(&t, "Omdat dat dit jaar €65 kost, boodschappen en meer.").is_empty(),
            "{:?}",
            kenmerkfouten(&t, "Omdat dat dit jaar €65 kost, boodschappen en meer.")
        );
        assert_eq!(
            kenmerkfouten(&t, "Ja, 650 euro."),
            ["mist: €", "mist: 65|vijfenzestig", "wel: ja"]
        );
    }

    #[test]
    fn de_mediaan() {
        let s = Duration::from_secs;
        assert_eq!(mediaan(&mut [s(3), s(1), s(2)]), s(2));
        assert_eq!(
            mediaan(&mut [s(4), s(1), s(2), s(3)]),
            Duration::from_millis(2500)
        );
        assert_eq!(mediaan(&mut []), Duration::ZERO);
    }
}
