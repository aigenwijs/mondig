//! `mondig-cli opneem`: de microfoons tonen en het openen van de microfoon meten.
//!
//! Dit is een meting en geen opnametool: de opname blijft in het geheugen, gaat nooit naar
//! schijf en wordt niet uitgeschreven. De uitvoer bevat alleen getallen (tijden, aantal
//! samples, piekniveau), nooit audio of tekst.

use std::process::ExitCode;
use std::time::Duration;

use clap::Args;
use mondig_core::opname::handy::HandyApparatuur;
use mondig_core::opname::{Apparatuur, Opname, geen_melder, kies_microfoon};
use mondig_core::stilte::controleer_nulsignaal;

use crate::{Melding, uit};

/// Opties van `mondig-cli opneem`.
#[derive(Args)]
pub(crate) struct OpneemOpties {
    /// Toon de microfoons en stop.
    #[arg(long)]
    lijst: bool,
    /// De microfoon (id of naam, ook een deel van de naam). Zonder opgave: de standaard van het systeem.
    #[arg(long)]
    microfoon: Option<String>,
    /// Hoeveel seconden er opgenomen wordt.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u64).range(1..=60))]
    seconden: u64,
    /// Hoe vaak de meting herhaald wordt (steeds opnieuw openen).
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=20))]
    keer: u32,
}

/// Voert `opneem` uit.
pub(crate) fn voer_uit(opties: &OpneemOpties) -> Result<ExitCode, Melding> {
    let apparatuur = HandyApparatuur;
    let lijst = apparatuur
        .microfoons()
        .map_err(|f| Melding(mondig_core::taak::Foutmelding::from(f).melding))?;

    if opties.lijst {
        if lijst.is_empty() {
            uit("Geen microfoons gevonden.")?;
        }
        for m in &lijst {
            let standaard = if m.standaard { "  (standaard)" } else { "" };
            uit(&format!("{}  {}{standaard}", m.id, m.naam))?;
        }
        return Ok(ExitCode::SUCCESS);
    }

    // De opgegeven microfoon eerst opzoeken, zodat een typefout een duidelijke melding geeft.
    let gekozen = match &opties.microfoon {
        Some(zoek) => Some(kies_microfoon(&lijst, zoek).map_err(|f| Melding(f.melding))?),
        None => None,
    };
    let naam = gekozen
        .or_else(|| lijst.iter().find(|m| m.standaard))
        .map_or("onbekend", |m| m.naam.as_str());
    uit(&format!("Microfoon: {naam}"))?;

    for keer in 1..=opties.keer {
        meet(gekozen.map(|m| m.id.as_str()), opties.seconden, keer)?;
    }
    Ok(ExitCode::SUCCESS)
}

/// Eén meting: openen, opnemen in het geheugen, stoppen, getallen tonen.
fn meet(microfoon: Option<&str>, seconden: u64, keer: u32) -> Result<(), Melding> {
    let mut opname =
        Opname::open(&HandyApparatuur, microfoon, geen_melder()).map_err(|f| Melding(f.melding))?;
    let open_duur = opname.open_duur();
    opname.start().map_err(|f| Melding(f.melding))?;
    std::thread::sleep(Duration::from_secs(seconden));
    let opgenomen = opname.stop().map_err(|f| Melding(f.melding))?;

    let eerste = opgenomen.tijd_tot_eerste_blok.map_or_else(
        || "geen blok ontvangen".to_string(),
        |t| format!("{} ms", t.as_millis()),
    );
    let piek = opgenomen.piek();
    uit(&format!(
        "Meting {keer}: openen {} ms, start tot eerste blok {eerste}, {} samples ({:.2} s), piek {piek:.4} ({})",
        open_duur.as_millis(),
        opgenomen.samples().len(),
        opgenomen.duur().as_secs_f64(),
        dbfs(piek),
    ))?;
    if let Some(melding) = &opgenomen.onderbroken {
        uit(&format!("Let op: {}", melding.melding))?;
    }
    if opgenomen.verloren > 0 {
        uit(&format!(
            "Let op: ongeveer {} samples verloren.",
            opgenomen.verloren
        ))?;
    }
    if let Err(melding) = controleer_nulsignaal(
        opgenomen.samples(),
        &mondig_core::filter::FilterInstellingen::standaard(),
    ) {
        uit(&format!("Let op: {}", melding.melding))?;
    }
    Ok(())
}

/// Piekniveau in dB ten opzichte van vol bereik, of een streepje bij nul.
fn dbfs(piek: f32) -> String {
    if piek <= 0.0 {
        "stilte".to_string()
    } else {
        format!("{:.1} dBFS", 20.0 * piek.log10())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn niveau_in_decibel() {
        assert_eq!(dbfs(0.0), "stilte");
        assert_eq!(dbfs(1.0), "0.0 dBFS");
        assert_eq!(dbfs(0.1), "-20.0 dBFS");
    }
}
