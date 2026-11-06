//! De bron achter de opnamelaag: `handy-recorder` (MIT, de opnamecode van Handy, ADR 0013).
//!
//! Dit is de enige module die `handy-recorder` kent. De crate levert 16 kHz mono (via
//! `RecorderConfig::speech`) op een gewone thread; wij vertalen zijn fouten naar onze
//! eigen [`FoutSoort`] en geven de samples door aan de [`Uitgang`].

use handy_recorder::{
    AudioChunk, EndReason, Error, ErrorKind, Recorder, RecorderConfig, Sink, list_input_devices,
};

use super::{Apparatuur, Bron, Einde, FoutSoort, Microfoon, OpnameFout, Uitgang, UitvalMelder};

/// De microfoons van deze computer via `handy-recorder`.
#[derive(Debug, Clone, Copy, Default)]
pub struct HandyApparatuur;

impl Apparatuur for HandyApparatuur {
    fn microfoons(&self) -> Result<Vec<Microfoon>, OpnameFout> {
        let apparaten = list_input_devices().map_err(|f| vertaal_fout(&f))?;
        Ok(apparaten
            .into_iter()
            // Een "monitor" is geluid van het systeem, geen microfoon (alleen Linux).
            .filter(|a| !a.is_monitor)
            .map(|a| Microfoon {
                id: a.id,
                naam: a.name,
                standaard: a.is_default,
            })
            .collect())
    }

    fn open(
        &self,
        microfoon: Option<&str>,
        bij_uitval: UitvalMelder,
    ) -> Result<Box<dyn Bron>, OpnameFout> {
        let config = RecorderConfig {
            device: microfoon.map(str::to_string),
            ..RecorderConfig::speech()
        };
        let recorder = Recorder::open_with_failure_handler(config, move |fout| {
            bij_uitval(vertaal_fout(&fout));
        })
        .map_err(|f| vertaal_fout(&f))?;
        Ok(Box::new(HandyBron { recorder }))
    }
}

/// Een geopende microfoon van `handy-recorder`.
struct HandyBron {
    recorder: Recorder<Doorgeven>,
}

impl Bron for HandyBron {
    fn start(&mut self, uitgang: Box<dyn Uitgang>) -> Result<(), OpnameFout> {
        self.recorder
            .start(Doorgeven { uitgang })
            .map_err(|f| vertaal_fout(&f.error))
    }

    fn stop(&mut self) -> Result<Einde, OpnameFout> {
        let gestopt = self.recorder.stop().map_err(|f| vertaal_fout(&f))?;
        let uitgevallen = match &gestopt.end_reason {
            EndReason::StopCalled => None,
            EndReason::RecorderFailed(fout) => Some(vertaal_fout(fout)),
            // De uitgang is van ons: een paniek daarin is een fout in Mondig.
            EndReason::SinkPanicked(bericht) => Some(OpnameFout {
                soort: FoutSoort::Programma,
                detail: Some(bericht.clone()),
            }),
        };
        Ok(Einde {
            uitgevallen,
            verloren: gestopt.dropped_frames,
        })
    }
}

/// Geeft de blokken van `handy-recorder` door aan onze [`Uitgang`].
struct Doorgeven {
    uitgang: Box<dyn Uitgang>,
}

impl Sink for Doorgeven {
    fn process_chunk(&mut self, blok: AudioChunk<'_>) {
        let kanalen = usize::from(blok.channels.max(1));
        let echt = &blok.samples[..blok.valid_frames * kanalen];
        if kanalen == 1 {
            self.uitgang.ontvang(echt);
        } else {
            // `RecorderConfig::speech` vraagt mono; dit vangt een andere indeling op in
            // plaats van half geluid door te geven.
            #[allow(clippy::cast_precision_loss)]
            let mono: Vec<f32> = echt
                .chunks_exact(kanalen)
                .map(|frame| frame.iter().sum::<f32>() / kanalen as f32)
                .collect();
            self.uitgang.ontvang(&mono);
        }
    }
}

/// Vertaalt een fout van `handy-recorder` naar onze soort. De tekst van het systeem
/// (`detail`) gaat mee voor het logboek en komt nooit in een melding.
fn vertaal_fout(fout: &Error) -> OpnameFout {
    let soort = match fout.kind() {
        ErrorKind::DeviceUnavailable => FoutSoort::GeenMicrofoon,
        ErrorKind::DeviceBusy => FoutSoort::Bezet,
        ErrorKind::PermissionDenied => FoutSoort::ToegangGeweigerd,
        ErrorKind::OpenTimedOut => FoutSoort::Reageert,
        ErrorKind::NoAudio => FoutSoort::GeenGeluid,
        ErrorKind::DeviceLost => FoutSoort::Losgekoppeld,
        ErrorKind::StreamInvalidated => FoutSoort::Veranderd,
        ErrorKind::Stalled => FoutSoort::Gestopt,
        ErrorKind::Backend
        | ErrorKind::CloseTimedOut
        | ErrorKind::UnsupportedFormat
        | ErrorKind::InvalidChannel => FoutSoort::Anders,
        // Misbruik van de opnamelaag of een fout in de crate zelf, en alles wat later bijkomt
        // (de lijst is niet uitputtend): voor de gebruiker is dat een algemene fout.
        _ => FoutSoort::Programma,
    };
    OpnameFout {
        soort,
        detail: Some(fout.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::opname::{Opname, geen_melder};

    #[test]
    fn de_lijst_van_de_systeemmicrofoons_is_op_te_vragen() {
        // Zonder microfoon op de bouwmachine is een lege lijst of een fout ook goed; het gaat
        // erom dat het niet crasht.
        let _ = HandyApparatuur.microfoons();
    }

    #[test]
    #[ignore = "heeft een echte microfoon nodig"]
    fn echte_microfoon_levert_samples() {
        let mut opname = Opname::open(&HandyApparatuur, None, geen_melder()).unwrap();
        opname.start().unwrap();
        std::thread::sleep(Duration::from_secs(2));
        assert!(opname.tijd_tot_eerste_blok().is_some());
        let opgenomen = opname.stop().unwrap();
        assert!(opgenomen.onderbroken.is_none());
        // Ongeveer twee seconden, met ruim de ruimte voor het openen.
        assert!(opgenomen.duur() > Duration::from_millis(800));
    }
}
