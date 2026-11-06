//! `mondig-cli notulen`: notulen maken van een verslag en de kwaliteit meten op een testset
//! (ADR 0026).
//!
//! - `notulen maak --verslag <opnamemap, verslag.json of vergadering.txt>` maakt notulen. Op het
//!   scherm komen standaard alleen getallen (geen samenvatting, punten, citaten of namen), zodat
//!   een echte vergadering buiten beeld blijft. De inhoud komt alleen op het scherm met
//!   `--toon-inhoud`, of in het bestand van `--uit`. Het verslag zelf verandert niet.
//! - `notulen meet --set <map>` maakt notulen van elke vergadering in de testset
//!   (`eval/notulen/<naam>/vergadering.txt` met `verwacht.json`) en vergelijkt ze met wat er
//!   verwacht wordt.
//!
//! Testformaat `vergadering.txt`: per beurt een regel `mm:ss Naam: tekst` (of `h:mm:ss`); regels
//! die met `#` beginnen en lege regels tellen niet. Beurt 1 is de eerste beurtregel.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Subcommand, ValueEnum};
use mondig_core::gesprek::{Beurt, Gesprek, Kant, Spoorstand, Vergadersoort};
use mondig_core::notulen::maken::{self, Instellingen, NotulenFout, Statistiek};
use mondig_core::notulen::{BewaardeNotulen, Controle, Taal, tijdcode};
use mondig_core::persoonsgegevens::Markeringen;
use mondig_core::taalmodel::{Opties, Verbinding};
use mondig_core::verslag;
use serde::Deserialize;

use crate::taalmodel::{Aanbieder, annuleren_bij_ctrl_c, cli_melding};
use crate::{Melding, uit};

/// De opties van `notulen`.
#[derive(Args)]
pub(crate) struct NotulenOpties {
    #[command(subcommand)]
    actie: Actie,
}

/// Wat bij elke aanvraag hoort.
#[derive(Args, Clone)]
struct ModelKeuze {
    /// De aanbieder.
    #[arg(long, value_enum)]
    aanbieder: Aanbieder,
    /// De modelnaam bij de aanbieder, bijvoorbeeld `mistral-small-latest`.
    #[arg(long)]
    model: String,
    /// De taal van de notulen.
    #[arg(long, value_enum, default_value = "nl")]
    taal: TaalKeuze,
    /// Het budget per aanvraag in geschatte tokens; standaard per aanbieder.
    #[arg(long)]
    budget: Option<usize>,
}

#[derive(Clone, Copy, ValueEnum)]
enum TaalKeuze {
    Nl,
    En,
}

#[derive(Subcommand)]
enum Actie {
    /// Maakt notulen van een verslag.
    Maak {
        /// De map van de opname, het bestand `verslag.json`, of een `vergadering.txt` uit de
        /// testset.
        #[arg(long)]
        verslag: PathBuf,
        #[command(flatten)]
        model: ModelKeuze,
        /// Schrijf de volledige notulen (met bronnen) naar dit bestand.
        #[arg(long)]
        uit: Option<PathBuf>,
        /// Toon de notulen zelf op het scherm. Zonder deze vlag komen er alleen getallen op het
        /// scherm (de standaard).
        #[arg(long)]
        toon_inhoud: bool,
        /// Alleen getallen op het scherm. Dat is al de standaard; de vlag blijft voor bestaande
        /// opdrachten en kan niet samen met `--toon-inhoud`.
        #[arg(long, conflicts_with = "toon_inhoud")]
        alleen_cijfers: bool,
    },
    /// Meet de notulen op een testset en vergelijkt ze met `verwacht.json`.
    Meet {
        /// De map met de testset, bijvoorbeeld `eval/notulen`.
        #[arg(long)]
        set: PathBuf,
        #[command(flatten)]
        model: ModelKeuze,
        /// Schrijf de notulen per vergadering naar deze map.
        #[arg(long)]
        uit: Option<PathBuf>,
    },
}

pub(crate) fn voer_uit(opties: &NotulenOpties) -> Result<ExitCode, Melding> {
    match &opties.actie {
        Actie::Maak {
            verslag,
            model,
            uit,
            toon_inhoud,
            alleen_cijfers: _,
        } => maak(verslag, model, uit.as_deref(), *toon_inhoud),
        Actie::Meet { set, model, uit } => meet(set, model, uit.as_deref()),
    }
}

impl ModelKeuze {
    fn instellingen(&self) -> Instellingen {
        let taal = match self.taal {
            TaalKeuze::Nl => Taal::Nl,
            TaalKeuze::En => Taal::En,
        };
        let mut instellingen =
            Instellingen::voor(self.aanbieder.voorinstelling(), &self.model, taal);
        if let Some(budget) = self.budget {
            instellingen.budget_tokens = budget;
        }
        instellingen
    }

    fn verbinding(&self) -> Result<Verbinding, Melding> {
        // Ruim: een deel bij een lokaal model op de processor kan minuten duren.
        Ok(Verbinding::voor(self.aanbieder.voorinstelling())
            .map_err(cli_melding)?
            .met_opties(Opties {
                verbinden: Duration::from_secs(10),
                totaal: Duration::from_mins(15),
            }))
    }
}

/// Een verslag om notulen van te maken.
struct Bron {
    gesprek: Gesprek,
    markeringen: Markeringen,
}

/// Leest een verslag: een opnamemap, een `verslag.json` of een `vergadering.txt`.
fn lees_bron(pad: &Path) -> Result<Bron, Melding> {
    if pad
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("txt"))
    {
        let tekst = fs::read_to_string(pad)?;
        return Ok(Bron {
            gesprek: lees_vergadering(&tekst)?,
            markeringen: Markeringen::default(),
        });
    }
    let map = if pad.is_dir() {
        pad
    } else {
        pad.parent().unwrap_or(pad)
    };
    // Geen inhoud in de melding.
    let bewaard = verslag::lees(map)
        .ok_or_else(|| Melding("Er staat geen leesbaar verslag.json in die map.".into()))?;
    Ok(Bron {
        gesprek: bewaard.gesprek,
        markeringen: bewaard.markeringen,
    })
}

/// Leest het testformaat: per beurt `mm:ss Naam: tekst` of `h:mm:ss Naam: tekst`.
fn lees_vergadering(tekst: &str) -> Result<Gesprek, Melding> {
    let mut beurten: Vec<Beurt> = Vec::new();
    for (nummer, regel) in tekst.lines().enumerate() {
        let regel = regel.trim();
        if regel.is_empty() || regel.starts_with('#') {
            continue;
        }
        let fout = || {
            Melding(format!(
                "Regel {} past niet in het testformaat.",
                nummer + 1
            ))
        };
        let (tijd, rest) = regel.split_once(' ').ok_or_else(fout)?;
        let (naam, zin) = rest.split_once(':').ok_or_else(fout)?;
        let mut seconden = 0_u64;
        for deel in tijd.split(':') {
            seconden = seconden * 60 + deel.parse::<u64>().map_err(|_| fout())?;
        }
        beurten.push(Beurt {
            kant: Kant::Anderen,
            sprekernummer: None,
            start_ms: seconden * 1000,
            eind_ms: seconden * 1000,
            tekst: zin.trim().to_string(),
            naam: Some(naam.trim().to_string()),
            verbeterd: false,
        });
    }
    // Het eind van een beurt is het begin van de volgende.
    for i in 0..beurten.len() {
        let volgende = beurten.get(i + 1).map(|b| b.start_ms);
        let beurt = &mut beurten[i];
        beurt.eind_ms = volgende
            .unwrap_or(beurt.start_ms + 5_000)
            .max(beurt.start_ms);
    }
    let duur = beurten.last().map_or(0, |b| b.eind_ms);
    Ok(Gesprek {
        beurten,
        duur: Duration::from_millis(duur),
        meldingen: Vec::new(),
        jij: Spoorstand::Ontbrak,
        anderen: Spoorstand::Opgenomen,
        weggelaten: 0,
        soort: Vergadersoort::Online,
    })
}

fn maak(
    pad: &Path,
    model: &ModelKeuze,
    bestand: Option<&Path>,
    toon_inhoud: bool,
) -> Result<ExitCode, Melding> {
    let bron = lees_bron(pad)?;
    let annuleer = annuleren_bij_ctrl_c()?;
    let verbinding = model.verbinding()?;
    let instellingen = model.instellingen();
    let uitkomst = maken::maak(
        &verbinding,
        &bron.gesprek,
        &bron.markeringen,
        &instellingen,
        &annuleer,
        &mut |klaar, totaal| eprintln!("Aanvraag {klaar} van {totaal} klaar."),
    )
    .map_err(notulen_melding)?;
    let notulen = &uitkomst.notulen;
    for regel in cijfers(&uitkomst.statistiek, notulen) {
        uit(&regel)?;
    }
    if let Some(bestand) = bestand {
        fs::write(bestand, als_tekst(notulen, &bron.gesprek, &model.model))?;
        // Alleen de plek, niet de inhoud.
        uit(&format!("Notulen geschreven naar {}", bestand.display()))?;
    }
    if toon_inhoud {
        uit("")?;
        uit(&als_tekst(notulen, &bron.gesprek, &model.model))?;
    }
    Ok(ExitCode::SUCCESS)
}

fn notulen_melding(fout: NotulenFout) -> Melding {
    match fout {
        NotulenFout::Taalmodel(fout) => cli_melding(fout),
        fout => Melding::from(fout),
    }
}

/// Alleen getallen, nooit inhoud.
fn cijfers(statistiek: &Statistiek, notulen: &BewaardeNotulen) -> Vec<String> {
    let (gevonden, niet) = notulen.telling();
    vec![
        format!("Beurten: {}", statistiek.beurten),
        format!("Geschatte tokens: {}", statistiek.geschatte_tokens),
        format!("Delen: {}", statistiek.delen),
        format!(
            "Aanvragen: {} (waarvan {} herhaald na ongeldig JSON)",
            statistiek.aanvragen, statistiek.herhaald
        ),
        format!("Besluiten: {}", notulen.besluiten.len()),
        format!("Actiepunten: {}", notulen.actiepunten.len()),
        format!("Bronnen gevonden: {gevonden}, niet teruggevonden: {niet}"),
        format!(
            "Actiepunten met wie of wanneer niet in het verslag gevonden: {}",
            notulen.veldmarkeringen()
        ),
        format!("Tijd: {:.1} s", statistiek.duur.as_secs_f64()),
        format!(
            "Tokens: {} in, {} uit",
            statistiek.invoer_tokens, statistiek.uitvoer_tokens
        ),
    ]
}

/// De notulen als leesbare tekst, met bij elk punt de bron en de uitkomst van de controle.
fn als_tekst(notulen: &BewaardeNotulen, gesprek: &Gesprek, model: &str) -> String {
    let tijd = |beurt: usize| {
        gesprek
            .beurten
            .get(beurt.wrapping_sub(1))
            .map_or_else(|| "--:--".to_string(), |b| tijdcode(b.start_ms))
    };
    let controle = |c: Controle| match c {
        Controle::Gevonden { beurt } => format!("gevonden in beurt {beurt} ({})", tijd(beurt)),
        Controle::NietTeruggevonden => "NIET TERUGGEVONDEN in het verslag".to_string(),
    };
    let mut uit = String::new();
    let _ = writeln!(uit, "Notulen (model {model})\n");
    let _ = writeln!(uit, "Samenvatting\n{}\n", notulen.samenvatting);
    let _ = writeln!(uit, "Besluiten");
    for b in &notulen.besluiten {
        let _ = writeln!(uit, "- {}", b.punt.tekst);
        let _ = writeln!(
            uit,
            "    bron: beurt {}, \"{}\"; {}",
            b.punt.bron.beurt,
            b.punt.bron.citaat,
            controle(b.controle)
        );
    }
    let _ = writeln!(uit, "\nActiepunten");
    for a in &notulen.actiepunten {
        let markering = |niet: bool| {
            if niet {
                " (niet in het verslag gevonden)"
            } else {
                ""
            }
        };
        let _ = writeln!(
            uit,
            "- {} (wie: {}{}; wanneer: {}{})",
            a.punt.wat,
            leeg_of(&a.punt.wie),
            markering(a.wie_niet_teruggevonden),
            leeg_of(&a.punt.wanneer),
            markering(a.wanneer_niet_teruggevonden)
        );
        let _ = writeln!(
            uit,
            "    bron: beurt {}, \"{}\"; {}",
            a.punt.bron.beurt,
            a.punt.bron.citaat,
            controle(a.controle)
        );
    }
    uit
}

fn leeg_of(tekst: &str) -> &str {
    if tekst.trim().is_empty() {
        "niet gezegd"
    } else {
        tekst
    }
}

/// Wat er in een vergadering van de testset verwacht wordt.
#[derive(Deserialize)]
struct Verwacht {
    besluiten: Vec<VerwachtPunt>,
    actiepunten: Vec<VerwachtActie>,
    #[serde(default)]
    geen_besluit: Vec<VerwachtPunt>,
}

#[derive(Deserialize)]
struct VerwachtPunt {
    beurt: usize,
}

#[derive(Deserialize)]
struct VerwachtActie {
    beurt: usize,
    #[serde(default)]
    wie: String,
    #[serde(default)]
    wanneer: String,
}

/// De uitslag van één soort punten.
#[derive(Default, Clone, Copy)]
struct Uitslag {
    verwacht: usize,
    gevonden: usize,
    extra: usize,
    /// Extra punten waarvan de bron ook niet is teruggevonden (wel gemarkeerd).
    extra_gemarkeerd: usize,
}

/// De beurt waar een punt over gaat: waar de bron gevonden is, anders wat het model noemde.
fn beurt_van(bron_beurt: i64, controle: Controle) -> usize {
    match controle {
        Controle::Gevonden { beurt } => beurt,
        Controle::NietTeruggevonden => usize::try_from(bron_beurt).unwrap_or(0),
    }
}

/// Koppelt gemaakte punten één op één aan verwachte punten binnen één beurt afstand. Geeft per
/// gemaakt punt het verwachte punt (index) of `None`.
fn koppel(gemaakt: &[usize], verwacht: &[usize]) -> Vec<Option<usize>> {
    let mut vrij = vec![true; verwacht.len()];
    let mut uit = vec![None; gemaakt.len()];
    // Eerst exacte beurten, dan één ernaast.
    for afstand in 0..=1 {
        for (i, g) in gemaakt.iter().enumerate() {
            if uit[i].is_some() {
                continue;
            }
            if let Some(j) =
                (0..verwacht.len()).find(|&j| vrij[j] && g.abs_diff(verwacht[j]) == afstand)
            {
                vrij[j] = false;
                uit[i] = Some(j);
            }
        }
    }
    uit
}

fn uitslag(gemaakt: &[(i64, Controle)], verwacht: &[usize]) -> (Uitslag, Vec<Option<usize>>) {
    let beurten: Vec<usize> = gemaakt.iter().map(|(b, c)| beurt_van(*b, *c)).collect();
    let koppeling = koppel(&beurten, verwacht);
    let gevonden = koppeling.iter().flatten().count();
    let extra_gemarkeerd = koppeling
        .iter()
        .zip(gemaakt)
        .filter(|(k, (_, c))| k.is_none() && !c.gevonden())
        .count();
    (
        Uitslag {
            verwacht: verwacht.len(),
            gevonden,
            extra: gemaakt.len() - gevonden,
            extra_gemarkeerd,
        },
        koppeling,
    )
}

/// Vergelijkt de notulen van één vergadering met wat er verwacht wordt: één regel voor de tabel.
fn score(naam: &str, n: &BewaardeNotulen, s: &Statistiek, verwacht: &Verwacht) -> String {
    let (besluiten, _) = uitslag(
        &n.besluiten
            .iter()
            .map(|b| (b.punt.bron.beurt, b.controle))
            .collect::<Vec<_>>(),
        &verwacht
            .besluiten
            .iter()
            .map(|b| b.beurt)
            .collect::<Vec<_>>(),
    );
    let gemaakte_acties: Vec<(i64, Controle)> = n
        .actiepunten
        .iter()
        .map(|a| (a.punt.bron.beurt, a.controle))
        .collect();
    let (acties, koppeling) = uitslag(
        &gemaakte_acties,
        &verwacht
            .actiepunten
            .iter()
            .map(|a| a.beurt)
            .collect::<Vec<_>>(),
    );
    // Twijfelzinnen die toch een besluit werden.
    let twijfel = n
        .besluiten
        .iter()
        .filter(|b| {
            let beurt = beurt_van(b.punt.bron.beurt, b.controle);
            verwacht
                .geen_besluit
                .iter()
                .any(|g| g.beurt.abs_diff(beurt) <= 1)
        })
        .count();
    // Wie of wanneer ingevuld terwijl dat in het verslag niet gezegd is.
    let mut wie_geraden = 0;
    let mut wanneer_geraden = 0;
    for (punt, k) in n.actiepunten.iter().zip(&koppeling) {
        if let Some(j) = k {
            let v = &verwacht.actiepunten[*j];
            if v.wie.trim().is_empty() && !punt.punt.wie.trim().is_empty() {
                wie_geraden += 1;
            }
            if v.wanneer.trim().is_empty() && !punt.punt.wanneer.trim().is_empty() {
                wanneer_geraden += 1;
            }
        }
    }
    let (_, niet) = n.telling();
    let velden = n.veldmarkeringen();
    format!(
        "{naam} | {} | {} | {} | {}/{} ({}, {}) | {}/{} ({}, {}) | {twijfel} | {wie_geraden} | {wanneer_geraden} | {velden} | {niet} | {} | {:.0} s | {}/{}",
        s.beurten,
        s.geschatte_tokens,
        s.delen,
        besluiten.gevonden,
        besluiten.verwacht,
        besluiten.extra,
        besluiten.extra_gemarkeerd,
        acties.gevonden,
        acties.verwacht,
        acties.extra,
        acties.extra_gemarkeerd,
        s.herhaald,
        s.duur.as_secs_f64(),
        s.invoer_tokens,
        s.uitvoer_tokens,
    )
}

fn meet(set: &Path, model: &ModelKeuze, uitmap: Option<&Path>) -> Result<ExitCode, Melding> {
    let annuleer = annuleren_bij_ctrl_c()?;
    let verbinding = model.verbinding()?;
    let instellingen = model.instellingen();
    let mut mappen: Vec<PathBuf> = fs::read_dir(set)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("vergadering.txt").is_file() && p.join("verwacht.json").is_file())
        .collect();
    mappen.sort();
    if mappen.is_empty() {
        return Err(Melding("Geen vergaderingen gevonden in die map.".into()));
    }
    if let Some(map) = uitmap {
        fs::create_dir_all(map)?;
    }
    uit(&format!(
        "Model: {} ({}), budget {} tokens",
        model.model,
        model.aanbieder.voorinstelling().naam(),
        instellingen.budget_tokens
    ))?;
    uit(
        "vergadering | beurten | tokens | delen | besluiten gevonden/verwacht (extra, waarvan gemarkeerd) | actiepunten idem | twijfelzin als besluit | wie geraden | wanneer geraden | wie/wanneer gemarkeerd | bronnen niet teruggevonden | herhaald | tijd | tokens in/uit",
    )?;
    for map in mappen {
        let naam = map
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let gesprek = lees_vergadering(&fs::read_to_string(map.join("vergadering.txt"))?)?;
        let verwacht: Verwacht =
            serde_json::from_str(&fs::read_to_string(map.join("verwacht.json"))?)?;
        let uitkomst = match maken::maak(
            &verbinding,
            &gesprek,
            &Markeringen::default(),
            &instellingen,
            &annuleer,
            &mut |_, _| {},
        ) {
            Ok(u) => u,
            Err(fout) => {
                uit(&format!("{naam} | fout: {}", notulen_melding(fout).0))?;
                continue;
            }
        };
        let n = &uitkomst.notulen;
        uit(&score(&naam, n, &uitkomst.statistiek, &verwacht))?;
        if let Some(uitmap) = uitmap {
            let bestand = uitmap.join(format!(
                "{naam}.{}.txt",
                model.model.replace([':', '/', '\\'], "_")
            ));
            fs::write(bestand, als_tekst(n, &gesprek, &model.model))?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn het_testformaat_wordt_een_gesprek() {
        let g = lees_vergadering(
            "# Titel\n\n00:00 Anouk: Goedemorgen: welkom.\n1:02:03 Pieter de Vries: Ja.\n",
        )
        .unwrap();
        assert_eq!(g.beurten.len(), 2);
        assert_eq!(g.beurten[0].naam.as_deref(), Some("Anouk"));
        assert_eq!(g.beurten[0].tekst, "Goedemorgen: welkom.");
        assert_eq!(g.beurten[1].start_ms, 3_723_000);
        assert_eq!(g.beurten[0].eind_ms, 3_723_000);
        assert!(lees_vergadering("geen tijd hier").is_err());
    }

    #[test]
    fn koppelen_is_een_op_een_binnen_een_beurt() {
        assert_eq!(
            koppel(&[10, 10, 30], &[10, 11, 50]),
            vec![Some(0), Some(1), None]
        );
        assert_eq!(koppel(&[12], &[10]), vec![None]);
    }
}
