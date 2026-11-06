//! Welke spraakmodellen een werkproces nu vasthoudt (sprint 5-A).
//!
//! Een model mag pas van schijf als geen enkel werkproces het nog gebruikt: Windows houdt een
//! bestand vast zolang een proces het open heeft. Een motor wordt soms op de achtergrond
//! gestopt (na een wissel van model), of pas als het laatste dictaat erop klaar is. Daarom
//! telt dit register per model hoeveel motoren er bestaan, en kan `model_verwijder` wachten
//! tot dat er nul zijn.
//!
//! Elke motor die de app start, gaat in een [`Gevolgd`]: die meldt zich bij het starten aan
//! en pas af als de motor weg is. Dat gebeurt nadat de beheerder of de wachtrij hem gestopt
//! heeft, dus nadat het werkproces de modelbestanden heeft losgelaten.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant};

use mondig_core::aanbieder::{AanbiederFout, Uitgeschreven};
use mondig_core::bestanden::Bestandsmotor;
use mondig_core::engine::beheer::Motor;
use mondig_core::engine::protocol::Apparaat;
use mondig_core::engine::supervisor::{EngineFout, Voortgang};
use mondig_core::filter::{FilterInstellingen, Gefilterd};
use mondig_core::gesprek::Gesprek;
use mondig_core::gesprek::echo::Ontdubbelen;

use crate::dicteerketen::slot;

/// Hoe lang `model_verwijder` hooguit wacht tot een motor zijn model loslaat. Netjes stoppen
/// duurt bij de supervisor hooguit drie seconden, daarna wordt het proces afgebroken.
pub(crate) const VRIJ_GRENS: Duration = Duration::from_secs(10);

/// Het register: per model het aantal motoren dat er nu is.
pub(crate) struct Modelgebruik {
    per_model: Mutex<BTreeMap<String, usize>>,
    veranderd: Condvar,
}

/// Het ene register van de app.
pub(crate) static GEBRUIK: LazyLock<Modelgebruik> = LazyLock::new(Modelgebruik::nieuw);

impl Modelgebruik {
    pub(crate) const fn nieuw() -> Self {
        Self {
            per_model: Mutex::new(BTreeMap::new()),
            veranderd: Condvar::new(),
        }
    }

    /// Meldt een motor voor dit model aan; de [`Aanmelding`] meldt hem af als ze weggaat.
    pub(crate) fn meld_aan(&'static self, model: &str) -> Aanmelding {
        *slot(&self.per_model).entry(model.to_string()).or_default() += 1;
        Aanmelding {
            register: self,
            model: model.to_string(),
        }
    }

    /// Gebruikt een motor dit model nu? (Voor tests; de app wacht met `wacht_tot_vrij`.)
    #[cfg(test)]
    pub(crate) fn in_gebruik(&self, model: &str) -> bool {
        slot(&self.per_model).get(model).is_some_and(|n| *n > 0)
    }

    /// Wacht tot geen motor dit model meer gebruikt, hooguit `grens`. Geeft `true` als het
    /// model vrij is.
    pub(crate) fn wacht_tot_vrij(&self, model: &str, grens: Duration) -> bool {
        let tot = Instant::now() + grens;
        let mut per_model = slot(&self.per_model);
        loop {
            if per_model.get(model).is_none_or(|n| *n == 0) {
                return true;
            }
            let rest = tot.saturating_duration_since(Instant::now());
            if rest.is_zero() {
                return false;
            }
            per_model = match self.veranderd.wait_timeout(per_model, rest) {
                Ok((guard, _)) => guard,
                Err(vergiftigd) => vergiftigd.into_inner().0,
            };
        }
    }
}

/// Eén motor die een model gebruikt; weg is weg.
pub(crate) struct Aanmelding {
    register: &'static Modelgebruik,
    model: String,
}

impl Drop for Aanmelding {
    fn drop(&mut self) {
        let mut per_model = slot(&self.register.per_model);
        if let Some(aantal) = per_model.get_mut(&self.model) {
            *aantal = aantal.saturating_sub(1);
            if *aantal == 0 {
                per_model.remove(&self.model);
            }
        }
        drop(per_model);
        self.register.veranderd.notify_all();
    }
}

/// Een motor met zijn aanmelding. De velden gaan in deze volgorde weg: eerst de motor (die
/// zijn werkproces stopt), dan de aanmelding.
pub(crate) struct Gevolgd<M> {
    motor: M,
    _aanmelding: Aanmelding,
}

impl<M> Gevolgd<M> {
    /// Volgt `motor` als gebruiker van `model` in het register van de app.
    pub(crate) fn nieuw(motor: M, model: &str) -> Self {
        Self::met_register(&GEBRUIK, motor, model)
    }

    /// Als [`Gevolgd::nieuw`], met een eigen register (voor tests).
    pub(crate) fn met_register(register: &'static Modelgebruik, motor: M, model: &str) -> Self {
        Self {
            motor,
            _aanmelding: register.meld_aan(model),
        }
    }
}

impl<M: Motor> Motor for Gevolgd<M> {
    fn laad(&mut self) -> Result<Apparaat, EngineFout> {
        self.motor.laad()
    }

    fn draait(&self) -> bool {
        self.motor.draait()
    }

    fn stop(&mut self) {
        Motor::stop(&mut self.motor);
    }

    fn schrijf_uit(
        &mut self,
        samples: &[f32],
        filter: &FilterInstellingen,
        knip_stilte: bool,
    ) -> Result<Gefilterd, AanbiederFout> {
        Motor::schrijf_uit(&mut self.motor, samples, filter, knip_stilte)
    }
}

impl<M: Bestandsmotor> Bestandsmotor for Gevolgd<M> {
    fn schrijf_uit(
        &mut self,
        bestand: &Path,
        filter: Option<&FilterInstellingen>,
        sprekers: bool,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Uitgeschreven, AanbiederFout> {
        Bestandsmotor::schrijf_uit(
            &mut self.motor,
            bestand,
            filter,
            sprekers,
            voortgang,
            annuleer,
        )
    }

    fn stop(&mut self) {
        Bestandsmotor::stop(&mut self.motor);
    }

    fn schrijf_gesprek_uit(
        &mut self,
        opnamemap: &Path,
        filter: Option<&FilterInstellingen>,
        ontdubbelen: Option<&Ontdubbelen>,
        sprekers: bool,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Gesprek, AanbiederFout> {
        Bestandsmotor::schrijf_gesprek_uit(
            &mut self.motor,
            opnamemap,
            filter,
            ontdubbelen,
            sprekers,
            voortgang,
            annuleer,
        )
    }

    fn schrijf_gesprek_opnieuw_uit(
        &mut self,
        opnamemap: &Path,
        filter: Option<&FilterInstellingen>,
        ontdubbelen: Option<&Ontdubbelen>,
        oud: &Gesprek,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Gesprek, AanbiederFout> {
        Bestandsmotor::schrijf_gesprek_opnieuw_uit(
            &mut self.motor,
            opnamemap,
            filter,
            ontdubbelen,
            oud,
            voortgang,
            annuleer,
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::thread;

    use mondig_core::aanbieder::Transcript;
    use mondig_core::filter::{self, Meting};

    use super::*;

    /// Een motor voor tests: schrijft na `schrijfduur` een leeg dictaat uit.
    pub(crate) struct NepMotor {
        pub(crate) draait: bool,
        pub(crate) schrijfduur: Duration,
        pub(crate) uitgeschreven: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl NepMotor {
        pub(crate) fn nieuw(schrijfduur: Duration) -> Self {
            Self {
                draait: false,
                schrijfduur,
                uitgeschreven: Arc::default(),
            }
        }
    }

    impl Motor for NepMotor {
        fn laad(&mut self) -> Result<Apparaat, EngineFout> {
            self.draait = true;
            Ok(Apparaat::Cpu)
        }

        fn draait(&self) -> bool {
            self.draait
        }

        fn stop(&mut self) {
            self.draait = false;
        }

        fn schrijf_uit(
            &mut self,
            samples: &[f32],
            filter: &FilterInstellingen,
            _knip_stilte: bool,
        ) -> Result<Gefilterd, AanbiederFout> {
            self.draait = true;
            thread::sleep(self.schrijfduur);
            self.uitgeschreven.fetch_add(1, Ordering::SeqCst);
            let transcript = Transcript {
                segmenten: Vec::new(),
                audio_duur: Duration::from_secs(1),
            };
            Ok(filter::pas_toe(
                transcript,
                &Meting::van_samples(samples),
                filter,
            ))
        }
    }

    /// Een eigen register per test, zodat tests elkaar niet zien.
    pub(crate) fn register() -> &'static Modelgebruik {
        Box::leak(Box::new(Modelgebruik::nieuw()))
    }

    #[test]
    fn een_model_is_in_gebruik_tot_de_laatste_motor_weg_is() {
        let register = register();
        let a = Gevolgd::met_register(register, NepMotor::nieuw(Duration::ZERO), "x");
        let b = Gevolgd::met_register(register, NepMotor::nieuw(Duration::ZERO), "x");
        assert!(register.in_gebruik("x"));
        assert!(!register.in_gebruik("y"));
        drop(a);
        assert!(register.in_gebruik("x"));
        assert!(!register.wacht_tot_vrij("x", Duration::from_millis(20)));
        drop(b);
        assert!(!register.in_gebruik("x"));
        assert!(register.wacht_tot_vrij("x", Duration::ZERO));
    }

    #[test]
    fn wachten_eindigt_zodra_een_motor_op_een_andere_thread_weggaat() {
        let register = register();
        let motor = Gevolgd::met_register(register, NepMotor::nieuw(Duration::ZERO), "x");
        let weg = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            drop(motor);
        });
        assert!(register.wacht_tot_vrij("x", Duration::from_secs(5)));
        weg.join().unwrap();
    }
}
