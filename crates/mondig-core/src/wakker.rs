//! De computer wakker houden zolang Mondig werk heeft (fase B-2): tijdens het opnemen van een
//! vergadering, het dicteren en het uitschrijven. Hier staat alleen de logica van de
//! houders; de aanroep naar het besturingssysteem zit achter [`Slot`] en staat in de app.
//!
//! Het patroon is van Vibe (MIT, `desktop/src-tauri/src/keepawake.rs`, commit `da8c2ca`),
//! zelf geschreven:
//!
//! - **Houders met een naam.** "vergadering", "dicteren" en "uitschrijven" houden de computer
//!   wakker onafhankelijk van elkaar. Laat één van hen los terwijl een ander nog werk heeft,
//!   dan blijft de computer wakker.
//! - **Eerst loslaten, dan opnieuw vastpakken**, bij elke wijziging. Op Windows zet het
//!   loslaten van een oude houder de stand terug naar wat hij zag toen hij werd gemaakt; wie
//!   eerst een nieuwe pakt en dan de oude loslaat, is de nieuwe weer kwijt.
//! - **Alleen de computer, niet het scherm.** Het scherm mag gewoon uit.
//!
//! [`Beheer`] is niet thread-safe bedoeld: het besturingssysteem kent de stand per thread,
//! dus één thread bezit het beheer en roept het steeds aan.

use std::collections::BTreeSet;
use std::io;

/// De aanroep naar het besturingssysteem.
pub trait Slot {
    /// Houdt de computer wakker (de slaapstand wordt tegengehouden, het scherm mag uit).
    ///
    /// # Errors
    ///
    /// Geeft een fout als het besturingssysteem het weigert. Niet erg: de computer kan dan
    /// slapen, maar het werk gaat door.
    fn pak(&mut self) -> io::Result<()>;

    /// Laat los: de computer mag weer slapen.
    fn laat_los(&mut self);
}

/// De houders met een naam, en het slot dat de computer wakker houdt zolang er één is.
pub struct Beheer<S: Slot> {
    slot: S,
    houders: BTreeSet<String>,
    /// Staat het slot nu vast?
    vast: bool,
}

impl<S: Slot> Beheer<S> {
    /// Een beheer zonder houders.
    #[must_use]
    pub fn nieuw(slot: S) -> Self {
        Self {
            slot,
            houders: BTreeSet::new(),
            vast: false,
        }
    }

    /// Zet de houder `naam` aan of uit en past het slot aan. Geeft `true` als er iets
    /// veranderde (een nieuwe of een vertrokken houder). Dezelfde stand opnieuw doorgeven
    /// doet niets.
    pub fn zet(&mut self, naam: &str, houdt_vast: bool) -> bool {
        let veranderd = if houdt_vast {
            self.houders.insert(naam.to_string())
        } else {
            self.houders.remove(naam)
        };
        if veranderd {
            self.pas_toe();
        }
        veranderd
    }

    /// Laat alle houders los (bij het afsluiten).
    pub fn laat_alles_los(&mut self) {
        self.houders.clear();
        self.pas_toe();
    }

    /// Houdt iemand de computer nu wakker?
    #[must_use]
    pub fn is_wakker(&self) -> bool {
        self.vast
    }

    /// De namen van de houders, op volgorde.
    #[must_use]
    pub fn houders(&self) -> Vec<&str> {
        self.houders.iter().map(String::as_str).collect()
    }

    /// Eerst loslaten, dan vastpakken als er nog een houder is.
    fn pas_toe(&mut self) {
        if self.vast {
            self.slot.laat_los();
            self.vast = false;
        }
        if !self.houders.is_empty() {
            // Weigert het besturingssysteem, dan slaapt de computer misschien, maar het werk
            // gaat door; er is niets wat de gebruiker eraan kan doen.
            self.vast = self.slot.pak().is_ok();
        }
    }
}

/// Zo lang zonder voortgang houdt het uitschrijven de computer nog wakker.
pub const GEEN_VOORTGANG: std::time::Duration = std::time::Duration::from_mins(15);

/// Houdt de computer alleen wakker voor uitschrijfwerk dat nog beweegt. Een bestand dat op
/// "wacht" blijft staan omdat het werkproces is vastgelopen, mag de computer niet eindeloos
/// wakker houden: komt er `GEEN_VOORTGANG` lang geen gebeurtenis meer, dan laat de houder los
/// tot er weer iets gebeurt.
pub struct Voortgangswacht {
    laatste: u64,
    sinds: std::time::Duration,
}

impl Voortgangswacht {
    /// Een nieuwe wacht, begonnen op tijdstip `nu` (de tijd sinds een vast beginpunt).
    #[must_use]
    pub fn nieuw(nu: std::time::Duration) -> Self {
        Self {
            laatste: 0,
            sinds: nu,
        }
    }

    /// Is er werk dat nog beweegt? `bezig` zegt of er iets wacht of loopt, `activiteit` is een
    /// teller die bij elke gebeurtenis (voortgang, wisseling van toestand) oploopt.
    pub fn beweegt(&mut self, nu: std::time::Duration, bezig: bool, activiteit: u64) -> bool {
        if !bezig || activiteit != self.laatste {
            self.laatste = activiteit;
            self.sinds = nu;
        }
        bezig && nu.saturating_sub(self.sinds) < GEEN_VOORTGANG
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Een slot dat bijhoudt wat het te horen kreeg, en of het nu vast staat.
    #[derive(Clone, Default)]
    struct Nep {
        log: Rc<RefCell<Vec<&'static str>>>,
        weigert: bool,
    }

    impl Slot for Nep {
        fn pak(&mut self) -> io::Result<()> {
            if self.weigert {
                return Err(io::Error::other("geweigerd"));
            }
            self.log.borrow_mut().push("pak");
            Ok(())
        }
        fn laat_los(&mut self) {
            self.log.borrow_mut().push("los");
        }
    }

    fn beheer() -> (Beheer<Nep>, Rc<RefCell<Vec<&'static str>>>) {
        let nep = Nep::default();
        let log = Rc::clone(&nep.log);
        (Beheer::nieuw(nep), log)
    }

    #[test]
    fn de_eerste_houder_pakt_vast_en_de_laatste_laat_los() {
        let (mut beheer, log) = beheer();
        assert!(!beheer.is_wakker());
        assert!(beheer.zet("vergadering", true));
        assert!(beheer.is_wakker());
        assert_eq!(*log.borrow(), vec!["pak"]);
        assert!(beheer.zet("vergadering", false));
        assert!(!beheer.is_wakker());
        assert_eq!(*log.borrow(), vec!["pak", "los"]);
    }

    #[test]
    fn houders_breken_elkaar_niet() {
        let (mut beheer, _log) = beheer();
        beheer.zet("vergadering", true);
        beheer.zet("uitschrijven", true);
        // Eén laat los, de ander houdt de computer wakker.
        beheer.zet("vergadering", false);
        assert!(beheer.is_wakker());
        assert_eq!(beheer.houders(), vec!["uitschrijven"]);
        beheer.zet("uitschrijven", false);
        assert!(!beheer.is_wakker());
    }

    #[test]
    fn bij_elke_wijziging_eerst_los_en_dan_pas_vast() {
        let (mut beheer, log) = beheer();
        beheer.zet("dicteren", true);
        beheer.zet("uitschrijven", true);
        beheer.zet("dicteren", false);
        // Nooit twee keer pak achter elkaar: telkens eerst los.
        assert_eq!(*log.borrow(), vec!["pak", "los", "pak", "los", "pak"]);
    }

    #[test]
    fn dezelfde_stand_nog_eens_doorgeven_doet_niets() {
        let (mut beheer, log) = beheer();
        assert!(beheer.zet("dicteren", true));
        assert!(!beheer.zet("dicteren", true));
        assert!(!beheer.zet("uitschrijven", false));
        assert_eq!(*log.borrow(), vec!["pak"]);
    }

    #[test]
    fn een_weigering_van_het_besturingssysteem_is_niet_fataal() {
        let nep = Nep {
            weigert: true,
            ..Nep::default()
        };
        let mut beheer = Beheer::nieuw(nep);
        beheer.zet("vergadering", true);
        assert!(!beheer.is_wakker());
        assert_eq!(beheer.houders(), vec!["vergadering"]);
        // Loslaten van een houder die niet vastzat, roept het slot niet aan.
        beheer.zet("vergadering", false);
    }

    #[test]
    fn een_vastgelopen_wachtrij_houdt_de_computer_niet_eindeloos_wakker() {
        use std::time::Duration;
        let min = |m: u64| Duration::from_secs(m * 60);
        let mut wacht = Voortgangswacht::nieuw(min(0));
        // Er beweegt iets: wakker, ook na een uur zolang de teller oploopt.
        assert!(wacht.beweegt(min(1), true, 1));
        assert!(wacht.beweegt(min(60), true, 2));
        // Daarna staat alles stil. Eerst nog wakker, na een kwartier niet meer.
        assert!(wacht.beweegt(min(70), true, 2));
        assert!(!wacht.beweegt(min(76), true, 2));
        assert!(!wacht.beweegt(min(200), true, 2));
        // Beweegt er weer iets, dan houdt Mondig de computer weer wakker.
        assert!(wacht.beweegt(min(201), true, 3));
        // Is er niets meer te doen, dan nooit.
        assert!(!wacht.beweegt(min(202), false, 3));
    }

    #[test]
    fn alles_loslaten_bij_het_afsluiten() {
        let (mut beheer, log) = beheer();
        beheer.zet("a", true);
        beheer.zet("b", true);
        beheer.laat_alles_los();
        assert!(!beheer.is_wakker());
        assert!(beheer.houders().is_empty());
        assert_eq!(log.borrow().last(), Some(&"los"));
    }
}
