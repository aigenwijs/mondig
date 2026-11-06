//! `mondig-cli taalmodel`: de sleutel in de sleutelbos zetten, de modellen van een aanbieder
//! tonen en een korte testaanvraag doen (ADR 0024 en 0025).
//!
//! De sleutel komt alleen via stdin binnen en wordt nooit afgedrukt. Typen in de terminal kan
//! niet: zonder extra afhankelijkheid is de echo daar niet uit te zetten, dus vraagt de opdracht
//! om een pipe (bijvoorbeeld vanaf het klembord).

use std::io::{self, IsTerminal};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use clap::{Args, Subcommand, ValueEnum};
use mondig_core::sleutel::{self, Sleutel, SleutelFout};
use mondig_core::taalmodel::{
    Aanvraag, Antwoord, JsonSchema, Opties, TaalmodelFout, Verbinding, Voorinstelling,
};

use crate::{Melding, uit};

/// Zoveel tekens van een testantwoord komen op het scherm.
const MAX_GETOOND: usize = 120;

/// De opties van `taalmodel`.
#[derive(Args)]
pub(crate) struct TaalmodelOpties {
    #[command(subcommand)]
    actie: Actie,
}

#[derive(Subcommand)]
enum Actie {
    /// Zet, controleert of wist de sleutel in de sleutelbos van deze computer.
    Sleutel {
        #[command(subcommand)]
        actie: SleutelActie,
    },
    /// Toont de modellen die de aanbieder heeft.
    Modellen {
        /// De aanbieder.
        #[arg(long, value_enum)]
        aanbieder: Aanbieder,
    },
    /// Doet een korte testaanvraag (vrije tekst en een klein JSON-schema) en toont tijd,
    /// tokengebruik en het antwoord.
    Test {
        /// De aanbieder.
        #[arg(long, value_enum)]
        aanbieder: Aanbieder,
        /// De modelnaam bij de aanbieder, bijvoorbeeld `mistral-small-latest`.
        #[arg(long)]
        model: String,
    },
}

#[derive(Subcommand)]
enum SleutelActie {
    /// Leest de sleutel van stdin en bewaart hem in de sleutelbos. Hij komt niet op het scherm.
    /// Bijvoorbeeld in PowerShell: `Get-Clipboard | mondig-cli taalmodel sleutel zet --aanbieder mistral`.
    Zet {
        /// De aanbieder.
        #[arg(long, value_enum, default_value = "mistral")]
        aanbieder: Aanbieder,
    },
    /// Haalt de sleutel uit de sleutelbos.
    Wis {
        /// De aanbieder.
        #[arg(long, value_enum, default_value = "mistral")]
        aanbieder: Aanbieder,
    },
    /// Zegt alleen of er een sleutel is: ja of nee.
    Staat {
        /// De aanbieder.
        #[arg(long, value_enum, default_value = "mistral")]
        aanbieder: Aanbieder,
    },
}

/// De aanbieder op de opdrachtregel.
#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum Aanbieder {
    /// Mistral (Europees, eigen sleutel).
    Mistral,
    /// Ollama op deze computer (lokaal, geen sleutel).
    Ollama,
}

impl Aanbieder {
    pub(crate) fn voorinstelling(self) -> Voorinstelling {
        match self {
            Self::Mistral => Voorinstelling::Mistral,
            Self::Ollama => Voorinstelling::OllamaLokaal,
        }
    }
}

pub(crate) fn voer_uit(opties: &TaalmodelOpties) -> Result<ExitCode, Melding> {
    match &opties.actie {
        Actie::Sleutel { actie } => sleutel_actie(actie),
        Actie::Modellen { aanbieder } => toon_modellen(aanbieder.voorinstelling()),
        Actie::Test { aanbieder, model } => test(aanbieder.voorinstelling(), model),
    }
}

fn sleutel_actie(actie: &SleutelActie) -> Result<ExitCode, Melding> {
    match actie {
        SleutelActie::Zet { aanbieder } => {
            let voorinstelling = aanbieder.voorinstelling();
            if !voorinstelling.sleutel_nodig() {
                return Err(Melding(format!(
                    "{} heeft geen sleutel nodig.",
                    voorinstelling.naam()
                )));
            }
            if io::stdin().is_terminal() {
                return Err(Melding(
                    "Geef de sleutel via een pipe, zodat hij niet op het scherm komt. Bijvoorbeeld in PowerShell: Get-Clipboard | mondig-cli taalmodel sleutel zet --aanbieder mistral".into(),
                ));
            }
            // `Sleutel::lees` leest in een buffer die vooraf groot genoeg is en daarna gewist
            // wordt; de sleutel zelf wordt gewist zodra hij hier uit beeld gaat.
            let sleutel = Sleutel::lees(io::stdin().lock())?;
            sleutel::zet(voorinstelling, &sleutel)?;
            uit(&format!(
                "De sleutel voor {} staat in de sleutelbos.",
                voorinstelling.naam()
            ))?;
        }
        SleutelActie::Wis { aanbieder } => {
            let voorinstelling = aanbieder.voorinstelling();
            let tekst = if sleutel::wis(voorinstelling)? {
                "De sleutel is gewist."
            } else {
                "Er stond geen sleutel."
            };
            uit(tekst)?;
        }
        SleutelActie::Staat { aanbieder } => {
            let staat = sleutel::bestaat(aanbieder.voorinstelling())?;
            uit(if staat { "ja" } else { "nee" })?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Een vlag die Ctrl+C aanzet.
pub(crate) fn annuleren_bij_ctrl_c() -> Result<Arc<AtomicBool>, Melding> {
    let annuleer = Arc::new(AtomicBool::new(false));
    let vlag = Arc::clone(&annuleer);
    ctrlc::set_handler(move || vlag.store(true, Ordering::Relaxed))?;
    Ok(annuleer)
}

fn toon_modellen(voorinstelling: Voorinstelling) -> Result<ExitCode, Melding> {
    let annuleer = annuleren_bij_ctrl_c()?;
    let verbinding = Verbinding::voor(voorinstelling).map_err(cli_melding)?;
    for naam in verbinding.modellen(&annuleer).map_err(cli_melding)? {
        uit(&naam)?;
    }
    Ok(ExitCode::SUCCESS)
}

fn test(voorinstelling: Voorinstelling, model: &str) -> Result<ExitCode, Melding> {
    let annuleer = annuleren_bij_ctrl_c()?;
    let verbinding = Verbinding::voor(voorinstelling)
        .map_err(cli_melding)?
        .met_opties(Opties {
            verbinden: Duration::from_secs(10),
            totaal: Duration::from_mins(3),
        });
    uit(&format!(
        "Aanbieder: {} ({}), model: {model}",
        voorinstelling.naam(),
        voorinstelling.soevereiniteit().label_nl()
    ))?;

    let vrij = Aanvraag {
        model: model.to_string(),
        systeem: "Je bent een testassistent. Antwoord zo kort mogelijk.".into(),
        gebruiker: "Antwoord met het woord klaar.".into(),
        temperatuur: 0.0,
        schema: None,
        max_tokens: Some(20),
        redeneren: false,
    };
    meet("Vrije tekst", &verbinding, &vrij, &annuleer)?;

    let met_schema = Aanvraag {
        gebruiker: "Antwoord met het woord klaar in het veld antwoord.".into(),
        schema: Some(JsonSchema {
            naam: "test".into(),
            schema: serde_json::json!({
                "type": "object",
                "properties": { "antwoord": { "type": "string" } },
                "required": ["antwoord"],
                "additionalProperties": false,
            }),
        }),
        max_tokens: Some(50),
        ..vrij
    };
    let antwoord = meet("JSON-schema", &verbinding, &met_schema, &annuleer)?;
    let geldig = serde_json::from_str::<serde_json::Value>(&antwoord.tekst)
        .is_ok_and(|w| w.get("antwoord").is_some_and(serde_json::Value::is_string));
    uit(&format!(
        "  Volgt het schema: {}",
        if geldig { "ja" } else { "nee" }
    ))?;
    Ok(if geldig {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// De melding voor de opdrachtregel. De kern verwijst bij een sleutelprobleem naar Instellingen
/// in de app; hier is de opdracht om de sleutel te zetten nuttiger.
pub(crate) fn cli_melding(fout: TaalmodelFout) -> Melding {
    const ZET: &str = "Zet de sleutel met: mondig-cli taalmodel sleutel zet --aanbieder mistral";
    match fout {
        TaalmodelFout::SleutelOntbreekt(naam) => {
            Melding(format!("Er is nog geen sleutel voor {naam}. {ZET}"))
        }
        TaalmodelFout::SleutelOngeldig(naam) => {
            Melding(format!("{naam} accepteert de sleutel niet. {ZET}"))
        }
        TaalmodelFout::Sleutel(SleutelFout::BewaardOngeldig) => {
            Melding(format!("De bewaarde sleutel is ongeldig. {ZET}"))
        }
        fout => Melding::from(fout),
    }
}

/// Doet één aanvraag en toont tijd, tokens, tokens per seconde en het (ingekorte) antwoord.
fn meet(
    titel: &str,
    verbinding: &Verbinding,
    aanvraag: &Aanvraag,
    annuleer: &AtomicBool,
) -> Result<Antwoord, Melding> {
    let begin = Instant::now();
    let antwoord = verbinding.vraag(aanvraag, annuleer).map_err(cli_melding)?;
    let duur = begin.elapsed().as_secs_f64();
    let tokens = antwoord.gebruik.map_or_else(
        || "tokens: niet gemeld".to_string(),
        |g| {
            let per_seconde = if duur > 0.0 {
                f64::from(g.uitvoer) / duur
            } else {
                0.0
            };
            format!(
                "tokens: {} in, {} uit ({per_seconde:.1} uit per seconde, inclusief wachten)",
                g.invoer, g.uitvoer
            )
        },
    );
    let mut kort: String = antwoord.tekst.trim().chars().take(MAX_GETOOND).collect();
    if antwoord.tekst.trim().chars().count() > MAX_GETOOND {
        kort.push_str("...");
    }
    uit(&format!("{titel}: {duur:.2} s, {tokens}"))?;
    uit(&format!("  Antwoord: {kort}"))?;
    if antwoord.afgekapt {
        uit("  Het antwoord is afgekapt (max_tokens op).")?;
    }
    Ok(antwoord)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleutelfouten_wijzen_naar_de_opdracht_om_de_sleutel_te_zetten() {
        for fout in [
            TaalmodelFout::SleutelOntbreekt("Mistral"),
            TaalmodelFout::SleutelOngeldig("Mistral"),
            TaalmodelFout::Sleutel(SleutelFout::BewaardOngeldig),
        ] {
            let Melding(tekst) = cli_melding(fout);
            assert!(tekst.contains("mondig-cli taalmodel sleutel zet --aanbieder mistral"));
            assert!(!tekst.contains("Instellingen"));
        }
        let Melding(tekst) = cli_melding(TaalmodelFout::Geannuleerd);
        assert_eq!(tekst, "Gestopt.");
    }
}
