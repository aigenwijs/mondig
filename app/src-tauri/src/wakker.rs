//! De computer wakker houden zolang Mondig werk heeft (fase B-2, Vibe-tip 5): tijdens het
//! opnemen van een vergadering, het dicteren en het uitschrijven van bestanden of een
//! vergadering. Alleen de computer, niet het scherm.
//!
//! De logica van de houders (namen, eerst loslaten dan opnieuw vastpakken) staat in
//! `mondig_core::wakker`. Hier loopt één thread die elke twee seconden kijkt wat Mondig nu
//! doet en de houders daarop zet. Kijken in plaats van op elk beginpunt en eindpunt een
//! melding sturen, omdat dicteren, vergaderen en de wachtrij elk op andere plekken starten en
//! stoppen: een vergeten stoppunt zou de computer eindeloos wakker houden. Een dictaat van
//! enkele seconden hoeft de computer niet wakker te houden; de slaapstand komt pas na
//! minuten zonder toetsaanslag, en een dictaat begint met een toetsaanslag.
//!
//! Op Windows met `SetThreadExecutionState`, op de Mac met een activiteit van `NSProcessInfo`
//! (crate `mondig-mac`, mijlpaal M1). Op Linux doet het voorlopig niets.

#[cfg(windows)]
mod win32;

#[cfg(not(windows))]
use std::io;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use mondig_core::wakker::{Beheer, Slot, Voortgangswacht};
use tauri::{AppHandle, Manager};

/// Hoe vaak Mondig kijkt wat het doet.
const RITME: Duration = Duration::from_secs(2);

/// Een slot dat niets doet, voor platforms waar dit nog niet is gebouwd.
#[cfg(not(any(windows, target_os = "macos")))]
struct Niets;

#[cfg(not(any(windows, target_os = "macos")))]
impl Slot for Niets {
    fn pak(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn laat_los(&mut self) {}
}

/// Doet het slot van dit platform.
#[cfg(windows)]
fn platformslot() -> impl Slot {
    win32::Windows
}

/// Het slot van de Mac: een activiteit die de computer wakker houdt (niet het scherm).
#[cfg(target_os = "macos")]
struct Mac(mondig_mac::wakker::Activiteit);

#[cfg(target_os = "macos")]
impl Slot for Mac {
    fn pak(&mut self) -> io::Result<()> {
        self.0.pak();
        Ok(())
    }
    fn laat_los(&mut self) {
        self.0.laat_los();
    }
}

#[cfg(target_os = "macos")]
fn platformslot() -> impl Slot {
    Mac(mondig_mac::wakker::Activiteit::nieuw())
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platformslot() -> impl Slot {
    Niets
}

/// Wat Mondig nu doet dat de computer wakker moet houden.
struct Werk {
    /// Er loopt een vergaderopname.
    vergadering: bool,
    /// Er loopt een dictaat (opnemen of uitschrijven) of een proef.
    dicteren: bool,
    /// Er wordt een bestand of vergadering uitgeschreven, of er staan er in de wachtrij.
    uitschrijven: bool,
}

fn kijk(app: &AppHandle) -> Werk {
    let dicteer = app.state::<crate::dicteerketen::DicteerStaat>();
    Werk {
        vergadering: crate::vergadering::loopt(app),
        dicteren: dicteer.dictaat_bezig.load(Ordering::SeqCst)
            || dicteer.proef_bezig.load(Ordering::SeqCst),
        uitschrijven: crate::bestanden::is_bezig(app) || crate::vergadering::schrijft_iets_uit(app),
    }
}

/// Start de thread die de computer wakker houdt zolang er werk is.
pub(crate) fn start(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        let mut beheer = Beheer::nieuw(platformslot());
        let begin = Instant::now();
        // Een vastgelopen werkproces laat bestanden op "wacht" staan zonder dat er iets gebeurt;
        // de computer blijft dan niet eindeloos wakker.
        let mut wacht = Voortgangswacht::nieuw(begin.elapsed());
        loop {
            let werk = kijk(&app);
            let uitschrijven = wacht.beweegt(
                begin.elapsed(),
                werk.uitschrijven,
                crate::bestanden::activiteit(&app),
            );
            beheer.zet("vergadering", werk.vergadering);
            beheer.zet("dicteren", werk.dicteren);
            beheer.zet("uitschrijven", uitschrijven);
            thread::sleep(RITME);
        }
    });
}
