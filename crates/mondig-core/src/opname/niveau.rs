//! De microfoontest van het instellingenscherm: dezelfde bron als bij een dictaat, maar
//! met een uitgang die alleen een niveau doorgeeft. Er wordt geen geluid bewaard of
//! teruggespeeld: elk blok wordt meteen omgerekend en weggegooid.
//!
//! Het niveau loopt van 0 (stilte) tot 1 (volle uitsturing) op een dB-schaal, ongeveer
//! twintig keer per seconde.
//!
//! Daarnaast de niveaumeter zonder opname (fase UX-C): [`Niveaumeting`] opent de microfoon
//! of het geluid van een afspeelapparaat en houdt alleen de piek bij ([`Piekmeter`]), op
//! dezelfde schaal als de meters tijdens een vergaderopname. De eigenaar leest die piek een
//! paar keer per seconde uit; ook dan wordt er niets bewaard.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use super::systeem::SysteemApparatuur;
use super::{Apparatuur, Bron, OpnameFout, Uitgang, UitvalMelder};
use crate::audio::DOEL_SAMPLERATE;
use crate::taak::Foutmelding;

/// Zoveel samples vormen één niveaumeting: 50 ms, dus twintig metingen per seconde.
pub const MEETVENSTER: usize = DOEL_SAMPLERATE as usize / 20;

/// Onder deze waarde (in dBFS) is het niveau 0. Stille kamers zitten rond de -60 dBFS.
const STILTE_DB: f32 = -60.0;

/// Het niveau (0 tot 1) bij een effectieve waarde (RMS) van de samples.
#[must_use]
pub fn niveau_van_rms(rms: f32) -> f32 {
    if rms <= 0.0 {
        return 0.0;
    }
    ((20.0 * rms.log10() - STILTE_DB) / -STILTE_DB).clamp(0.0, 1.0)
}

/// Rekent blokken samples om naar niveaus en geeft die door, zonder iets te bewaren.
pub struct NiveauUitgang<F> {
    bij_niveau: F,
    som_kwadraten: f64,
    geteld: usize,
}

impl<F: FnMut(f32) + Send + 'static> NiveauUitgang<F> {
    /// Een uitgang die elk meetvenster `bij_niveau` aanroept.
    #[must_use]
    pub fn nieuw(bij_niveau: F) -> Self {
        Self {
            bij_niveau,
            som_kwadraten: 0.0,
            geteld: 0,
        }
    }
}

impl<F: FnMut(f32) + Send + 'static> Uitgang for NiveauUitgang<F> {
    fn ontvang(&mut self, samples: &[f32]) {
        for &sample in samples {
            self.som_kwadraten += f64::from(sample) * f64::from(sample);
            self.geteld += 1;
            if self.geteld >= MEETVENSTER {
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let rms = (self.som_kwadraten / self.geteld as f64).sqrt() as f32;
                (self.bij_niveau)(niveau_van_rms(rms));
                self.som_kwadraten = 0.0;
                self.geteld = 0;
            }
        }
    }
}

/// De hoogste piek sinds de vorige keer uitlezen, zonder slot: de bron schrijft, een
/// andere thread leest. Wordt ook door de meters tijdens een vergaderopname gebruikt, zodat
/// beide meters dezelfde schaal hebben.
#[derive(Debug, Default)]
pub struct Piekmeter {
    /// De piek (0 tot 1) als bits van een `f32`. Voor getallen van nul of meer lopen de bits
    /// op met de waarde, dus `fetch_max` werkt.
    piek: AtomicU32,
}

impl Piekmeter {
    /// Neemt een blok samples mee en geeft de piek van dat blok terug (0 tot 1).
    pub fn meet(&self, samples: &[f32]) -> f32 {
        let piek = samples.iter().fold(0.0f32, |m, s| m.max(s.abs())).min(1.0);
        self.piek.fetch_max(piek.to_bits(), Ordering::Relaxed);
        piek
    }

    /// De piek sinds de vorige keer (0 tot 1), en weer op nul.
    pub fn neem_piek(&self) -> f32 {
        f32::from_bits(self.piek.swap(0, Ordering::Relaxed))
    }

    /// Het niveau sinds de vorige keer, op de schaal van [`niveau_van_rms`] (toegepast op
    /// de piek, zoals de meters tijdens een opname), en weer op nul.
    pub fn neem_niveau(&self) -> f32 {
        niveau_van_rms(self.neem_piek())
    }
}

/// Een uitgang die alleen de piek bijhoudt.
struct PiekUitgang(Arc<Piekmeter>);

impl Uitgang for PiekUitgang {
    fn ontvang(&mut self, samples: &[f32]) {
        self.0.meet(samples);
    }
}

/// Een lopende niveaumeting zonder opname. Weggooien zet de bron uit (bij de microfoon gaat
/// dan ook het microfoonlampje van het systeem uit).
pub struct Niveaumeting {
    bron: Box<dyn Bron>,
    meter: Arc<Piekmeter>,
}

impl Niveaumeting {
    /// Opent een microfoon (`None` is de standaard) en begint te meten. Valt de bron uit,
    /// dan komt de melding op `bij_uitval`, op een thread van de bron: stop de meting daar
    /// niet zelf, stuur een bericht naar een andere thread.
    ///
    /// # Errors
    ///
    /// Geeft een [`Foutmelding`] als de microfoon niet te openen of te starten is.
    pub fn microfoon(
        apparatuur: &dyn Apparatuur,
        microfoon: Option<&str>,
        bij_uitval: impl FnOnce(Foutmelding) + Send + 'static,
    ) -> Result<Self, Foutmelding> {
        Self::met_bron(|uitval| apparatuur.open(microfoon, uitval), bij_uitval)
    }

    /// Opent het geluid van een afspeelapparaat (`None` is de standaard, die een wissel
    /// volgt) en begint te meten. Speelt de computer niets af, dan blijft het niveau 0.
    ///
    /// # Errors
    ///
    /// Geeft een [`Foutmelding`] als het apparaat niet te openen is, of als dit systeem het
    /// geluid van de computer nog niet kan opnemen.
    pub fn systeem(
        apparatuur: &SysteemApparatuur,
        apparaat: Option<&str>,
        bij_uitval: impl FnOnce(Foutmelding) + Send + 'static,
    ) -> Result<Self, Foutmelding> {
        Self::met_bron(|uitval| apparatuur.open(apparaat, uitval), bij_uitval)
    }

    fn met_bron(
        open: impl FnOnce(UitvalMelder) -> Result<Box<dyn Bron>, OpnameFout>,
        bij_uitval: impl FnOnce(Foutmelding) + Send + 'static,
    ) -> Result<Self, Foutmelding> {
        let meter = Arc::new(Piekmeter::default());
        let bron = open_en_start(open, Box::new(PiekUitgang(Arc::clone(&meter))), bij_uitval)?;
        Ok(Self { bron, meter })
    }

    /// Het niveau (0 tot 1) sinds de vorige aanroep; 0 als er niets binnenkwam.
    #[must_use]
    pub fn neem_niveau(&self) -> f32 {
        self.meter.neem_niveau()
    }
}

impl Drop for Niveaumeting {
    fn drop(&mut self) {
        let _ = self.bron.stop();
    }
}

/// Opent een bron en start hem met `uitgang`; de gedeelde stap van test en meting.
fn open_en_start(
    open: impl FnOnce(UitvalMelder) -> Result<Box<dyn Bron>, OpnameFout>,
    uitgang: Box<dyn Uitgang>,
    bij_uitval: impl FnOnce(Foutmelding) + Send + 'static,
) -> Result<Box<dyn Bron>, Foutmelding> {
    let uitval: UitvalMelder = Box::new(move |fout| bij_uitval(Foutmelding::from(&fout)));
    let mut bron = open(uitval).map_err(|fout| Foutmelding::from(&fout))?;
    bron.start(uitgang)
        .map_err(|fout| Foutmelding::from(&fout))?;
    Ok(bron)
}

/// Een lopende microfoontest. Weggooien (of [`MicrofoonTest::stop`]) zet de microfoon uit.
pub struct MicrofoonTest {
    bron: Box<dyn Bron>,
}

impl MicrofoonTest {
    /// Opent de microfoon (`None` is de standaard) en geeft het niveau door aan
    /// `bij_niveau`, op een thread van de bron. Valt de microfoon uit, dan komt de melding
    /// op `bij_uitval`; stop de test daar niet zelf, stuur een bericht naar een andere thread.
    ///
    /// # Errors
    ///
    /// Geeft een [`Foutmelding`] in gewone taal als de microfoon niet te openen of te
    /// starten is.
    pub fn start(
        apparatuur: &dyn Apparatuur,
        microfoon: Option<&str>,
        bij_niveau: impl FnMut(f32) + Send + 'static,
        bij_uitval: impl FnOnce(Foutmelding) + Send + 'static,
    ) -> Result<Self, Foutmelding> {
        let bron = open_en_start(
            |uitval| apparatuur.open(microfoon, uitval),
            Box::new(NiveauUitgang::nieuw(bij_niveau)),
            bij_uitval,
        )?;
        Ok(Self { bron })
    }

    /// Zet de microfoon uit.
    pub fn stop(self) {
        drop(self);
    }
}

impl Drop for MicrofoonTest {
    fn drop(&mut self) {
        let _ = self.bron.stop();
    }
}
