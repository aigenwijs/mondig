//! Opnemen van de microfoon (ADR 0012 en 0013), in het geheugen.
//!
//! De rest van Mondig kent alleen de traits in deze module. De eerste en enige bron is
//! `handy-recorder` (module `handy`); wordt die ooit vervangen door `cpal`, dan verandert
//! alleen die ene module.
//!
//! De opname staat uitsluitend in het geheugen en komt nooit op schijf. Ze bestaat uit
//! 16 kHz mono `f32`-samples, het formaat dat het werkproces verwacht. Na negen minuten
//! komt het signaal [`OpnameGebeurtenis::BijnaVol`], na tien minuten
//! [`OpnameGebeurtenis::Vol`]: dan neemt de uitgang niets meer aan en moet de eigenaar
//! [`Opname::stop`] aanroepen (de bron zelf mag niet vanuit de uitgang gestopt worden).
//! Fouten bij het openen en tijdens het opnemen (headset weg, geen microfoon, toegang
//! geweigerd) worden een [`Foutmelding`] in gewone taal met een vervolgactie.
//!
//! Voor vergaderingen (fase 4b) is er een tweede uitgang, naar schijf in stukken (modules
//! `vergadering` en `stukken`; die laatste staat in [`crate::spoor`], zodat ook het
//! werkproces de sporen kan lezen), en een tweede bron: het geluid van de computer zelf (module
//! `systeem`, via cpal). De microfoon blijft dezelfde bron als bij dicteren.

pub mod handy;
pub mod niveau;
pub use crate::spoor::stukken;
pub mod systeem;
pub mod vergadering;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::audio::DOEL_SAMPLERATE;
use crate::engine::supervisor::MAX_DICTAAT;
use crate::taak::{Foutmelding, Vervolg};

/// Na zoveel opnemen krijgt de gebruiker een waarschuwing: een minuut voor de grens.
pub const BIJNA_VOL_NA: Duration = Duration::from_mins(9);

/// Een dictaat duurt hooguit tien minuten (ADR 0012); dit is [`MAX_DICTAAT`].
pub const MAX_OPNAME: Duration = MAX_DICTAAT;

/// Een microfoon die de gebruiker kan kiezen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Microfoon {
    /// Herkenning om de microfoon te openen. Bij sommige systemen blijft die niet
    /// hetzelfde na opnieuw aansluiten.
    pub id: String,
    /// Naam in gewone taal.
    pub naam: String,
    /// Is dit de microfoon die het systeem nu als standaard kiest?
    pub standaard: bool,
}

/// Wat er misging bij het openen of tijdens het opnemen, onafhankelijk van de bron.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoutSoort {
    /// Er is geen (zo'n) microfoon.
    GeenMicrofoon,
    /// Een ander programma gebruikt de microfoon.
    Bezet,
    /// Het besturingssysteem laat Mondig de microfoon niet gebruiken.
    ToegangGeweigerd,
    /// De microfoon werd niet op tijd klaar.
    Reageert,
    /// De microfoon gaf nooit geluid door.
    GeenGeluid,
    /// De microfoon is losgekoppeld (bijvoorbeeld een headset).
    Losgekoppeld,
    /// De microfoon of de standaardkeuze van het systeem veranderde tijdens het opnemen.
    Veranderd,
    /// De microfoon stopte met geluid doorgeven.
    Gestopt,
    /// Een ander probleem met de microfoon of het geluidssysteem.
    Anders,
    /// Een fout in Mondig zelf (verkeerd gebruik van de opnamelaag).
    Programma,
    /// Er is geen luidspreker of koptelefoon (afspeelapparaat) om het geluid van de computer
    /// van op te nemen.
    GeenAfspeelapparaat,
    /// Het afspeelapparaat viel weg tijdens het opnemen.
    AfspeelapparaatWeg,
    /// Het geluid van de computer opnemen kan op dit besturingssysteem (nog) niet.
    SysteemgeluidNietMogelijk,
    /// Een ander probleem bij het opnemen van het geluid van de computer.
    SysteemgeluidAnders,
    /// Het omrekenen van het geluid van de computer naar de opnameindeling mislukte.
    SysteemgeluidOmrekenen,
    /// Het omrekenen van het geluid van de computer is bij het starten al niet te maken.
    SysteemgeluidOmrekenenStart,
    /// Een testgeluid afspelen op een gekozen apparaat kan op dit besturingssysteem (nog)
    /// niet.
    TestgeluidNietMogelijk,
    /// Het testgeluid kon niet worden afgespeeld (apparaat weg, bezet of een andere fout).
    TestgeluidAnders,
}

/// Een fout van een bron. `detail` is de eigen tekst van het systeem, alleen voor in het
/// logboek: die komt nooit in een melding voor de gebruiker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpnameFout {
    /// De soort fout; die bepaalt melding en vervolgactie.
    pub soort: FoutSoort,
    /// Tekst van het systeem, voor het logboek.
    pub detail: Option<String>,
}

impl OpnameFout {
    /// Een fout zonder detail.
    #[must_use]
    pub fn nieuw(soort: FoutSoort) -> Self {
        Self {
            soort,
            detail: None,
        }
    }
}

impl From<&OpnameFout> for Foutmelding {
    fn from(fout: &OpnameFout) -> Self {
        let (melding, vervolg) = match fout.soort {
            FoutSoort::GeenMicrofoon => (
                "Mondig vindt geen microfoon. Sluit een microfoon of headset aan.",
                Vervolg::MicrofooninstellingOpenen,
            ),
            FoutSoort::Bezet => (
                "Een ander programma gebruikt de microfoon. Sluit dat programma en probeer het opnieuw.",
                Vervolg::OpnieuwProberen,
            ),
            FoutSoort::ToegangGeweigerd => (
                "Mondig mag de microfoon niet gebruiken. Zet de toegang aan in de privacy-instellingen van je computer.",
                Vervolg::MicrofooninstellingOpenen,
            ),
            FoutSoort::Reageert => (
                "De microfoon reageert niet. Probeer het opnieuw of kies een andere microfoon.",
                Vervolg::OpnieuwProberen,
            ),
            FoutSoort::GeenGeluid => (
                "De microfoon geeft geen geluid door. Controleer of hij aan staat en niet gedempt is.",
                Vervolg::MicrofooninstellingOpenen,
            ),
            FoutSoort::Losgekoppeld => (
                "De microfoon is losgekoppeld tijdens het opnemen. Sluit hem opnieuw aan of kies een andere.",
                Vervolg::MicrofooninstellingOpenen,
            ),
            FoutSoort::Veranderd => (
                "De microfoon is tijdens het opnemen veranderd. Probeer het opnieuw.",
                Vervolg::OpnieuwProberen,
            ),
            FoutSoort::Gestopt => (
                "De microfoon gaf niet meer door wat je zei. Probeer het opnieuw.",
                Vervolg::OpnieuwProberen,
            ),
            FoutSoort::Anders => (
                "Er ging iets mis met de microfoon. Probeer het opnieuw of kies een andere microfoon.",
                Vervolg::OpnieuwProberen,
            ),
            FoutSoort::Programma => ("Het opnemen is mislukt.", Vervolg::Geen),
            FoutSoort::GeenAfspeelapparaat => (
                "Mondig vindt geen luidspreker of koptelefoon. Zonder afspeelapparaat kan Mondig het geluid van de anderen niet opnemen.",
                Vervolg::OpnieuwProberen,
            ),
            FoutSoort::AfspeelapparaatWeg => (
                "De luidspreker of koptelefoon viel weg tijdens het opnemen. Het geluid van de anderen is bewaard tot dat moment.",
                Vervolg::OpnieuwProberen,
            ),
            FoutSoort::SysteemgeluidNietMogelijk => (
                "Het geluid van de computer opnemen kan op dit systeem nog niet. Alleen de microfoon wordt opgenomen.",
                Vervolg::Geen,
            ),
            FoutSoort::SysteemgeluidAnders => (
                "Er ging iets mis bij het opnemen van het geluid van de computer. Probeer het opnieuw.",
                Vervolg::OpnieuwProberen,
            ),
            FoutSoort::SysteemgeluidOmrekenenStart => (
                "Het geluid van de computer kan niet worden omgezet voor de opname. Alleen de microfoon wordt opgenomen.",
                Vervolg::Geen,
            ),
            FoutSoort::SysteemgeluidOmrekenen => (
                "Het geluid van de computer kon niet worden omgezet voor de opname. Het geluid van de anderen is bewaard tot dat moment.",
                Vervolg::OpnieuwProberen,
            ),
            FoutSoort::TestgeluidNietMogelijk => (
                "Een testgeluid afspelen kan op dit systeem nog niet.",
                Vervolg::Geen,
            ),
            FoutSoort::TestgeluidAnders => (
                "Mondig kon het testgeluid niet afspelen. Kies een ander apparaat of probeer het opnieuw.",
                Vervolg::OpnieuwProberen,
            ),
        };
        Foutmelding::nieuw(melding, vervolg)
    }
}

impl From<OpnameFout> for Foutmelding {
    fn from(fout: OpnameFout) -> Self {
        Self::from(&fout)
    }
}

/// Neemt de samples van een opname aan: 16 kHz, mono, `f32`.
///
/// De bron roept dit aan op een gewone thread (niet op de audiothread van het systeem),
/// maar een trage uitgang laat de bron geluid verliezen. Houd het dus kort.
pub trait Uitgang: Send + 'static {
    /// Neemt een blok samples aan.
    fn ontvang(&mut self, samples: &[f32]);

    /// Neemt een blok samples aan waarvan de bron weet wanneer het eerste sample werd
    /// opgenomen, op de klok van [`Instant`]. Een uitgang die daar niets mee doet, hoeft dit
    /// niet te implementeren.
    fn ontvang_op(&mut self, samples: &[f32], opgenomen: Instant) {
        let _ = opgenomen;
        self.ontvang(samples);
    }
}

/// Hoe een opname eindigde, naast de samples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Einde {
    /// Is de bron zelf uitgevallen (headset weg)? Wat tot dan opgenomen is, is bewaard.
    pub uitgevallen: Option<OpnameFout>,
    /// Aantal samples dat de bron kwijtraakte omdat de uitgang te traag was.
    pub verloren: u64,
}

/// Een geopende microfoon. Elke opname loopt van `start` tot `stop`.
pub trait Bron: Send {
    /// Begint een opname naar `uitgang`. Wacht niet op het eerste geluid.
    ///
    /// # Errors
    ///
    /// Geeft een [`OpnameFout`] als de bron al opneemt of eerder uitviel.
    fn start(&mut self, uitgang: Box<dyn Uitgang>) -> Result<(), OpnameFout>;

    /// Beëindigt de opname.
    ///
    /// # Errors
    ///
    /// Geeft een [`OpnameFout`] als er geen opname loopt.
    fn stop(&mut self) -> Result<Einde, OpnameFout>;
}

/// Wordt één keer aangeroepen als de bron uitvalt terwijl hij open staat.
pub type UitvalMelder = Box<dyn FnOnce(OpnameFout) + Send + 'static>;

/// De microfoons van deze computer: opsommen en openen.
pub trait Apparatuur {
    /// De microfoons die nu aangesloten zijn.
    ///
    /// # Errors
    ///
    /// Geeft een [`OpnameFout`] als het systeem ze niet wil opsommen.
    fn microfoons(&self) -> Result<Vec<Microfoon>, OpnameFout>;

    /// Opent een microfoon (`None` is de standaard van het systeem). Bij een
    /// Bluetooth-headset kan dit seconden duren.
    ///
    /// # Errors
    ///
    /// Geeft een [`OpnameFout`] als de microfoon niet te openen is.
    fn open(
        &self,
        microfoon: Option<&str>,
        bij_uitval: UitvalMelder,
    ) -> Result<Box<dyn Bron>, OpnameFout>;
}

/// Zoekt een microfoon op id of naam, zoals de gebruiker of de CLI die opgeeft. Eerst op
/// id, dan op de hele naam, dan op een deel van de naam (zonder letterverschil); een deel
/// dat meerdere microfoons raakt is niet duidelijk genoeg.
///
/// # Errors
///
/// Geeft een [`Foutmelding`] als er niets of meer dan één past.
pub fn kies_microfoon<'a>(
    lijst: &'a [Microfoon],
    zoek: &str,
) -> Result<&'a Microfoon, Foutmelding> {
    kies_op_naam(
        lijst,
        zoek,
        |m| (m.id.as_str(), m.naam.as_str()),
        "microfoon",
        Vervolg::MicrofooninstellingOpenen,
    )
}

/// Zoekt in `lijst` op id, dan de hele naam, dan een deel van de naam (zie
/// [`kies_microfoon`]). `wat` is het soort apparaat in de melding.
pub(crate) fn kies_op_naam<'a, T>(
    lijst: &'a [T],
    zoek: &str,
    id_en_naam: impl Fn(&T) -> (&str, &str),
    wat: &str,
    niet_gevonden: Vervolg,
) -> Result<&'a T, Foutmelding> {
    let zoek_klein = zoek.to_lowercase();
    let treffers: [Vec<&T>; 3] = [
        lijst.iter().filter(|m| id_en_naam(m).0 == zoek).collect(),
        lijst
            .iter()
            .filter(|m| id_en_naam(m).1.to_lowercase() == zoek_klein)
            .collect(),
        lijst
            .iter()
            .filter(|m| id_en_naam(m).1.to_lowercase().contains(&zoek_klein))
            .collect(),
    ];
    for gevonden in treffers {
        match gevonden.as_slice() {
            [] => {}
            [enige] => return Ok(enige),
            _ => {
                return Err(Foutmelding::nieuw(
                    format!("Meer dan één {wat} past bij \"{zoek}\". Geef een langere naam op."),
                    Vervolg::Geen,
                ));
            }
        }
    }
    Err(Foutmelding::nieuw(
        format!("Er is geen {wat} met de naam \"{zoek}\"."),
        niet_gevonden,
    ))
}

/// Welke microfoon opent bij een dictaat: de opgeslagen keuze als die er nog is, anders
/// `None` (de standaard van het systeem). Een headset die losgekoppeld is, mag een dictaat
/// dus nooit laten mislukken: dan nemen we gewoon de standaardmicrofoon.
#[must_use]
pub fn opgeslagen_microfoon(opgeslagen: Option<&str>, lijst: &[Microfoon]) -> Option<String> {
    let id = opgeslagen?;
    lijst.iter().find(|m| m.id == id).map(|m| m.id.clone())
}

/// Wat een lopende opname meldt, op de thread waarop de bron levert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpnameGebeurtenis {
    /// Het eerste blok samples is binnen, met de tijd sinds [`Opname::start`]. Vanaf nu
    /// hoort de microfoon wat er gezegd wordt.
    EersteBlok(Duration),
    /// Er is negen minuten opgenomen: nog een minuut tot de grens.
    BijnaVol,
    /// Er is tien minuten opgenomen. De uitgang neemt niets meer aan; roep
    /// [`Opname::stop`] aan.
    Vol,
    /// De bron viel uit tijdens het opnemen (headset weg). Roep [`Opname::stop`] aan om
    /// te bewaren wat er tot dan was.
    Uitgevallen(Foutmelding),
}

/// Ontvanger van [`OpnameGebeurtenis`]sen. Wordt op een thread van de bron aangeroepen;
/// stop de opname dus niet hieruit maar stuur een bericht naar de eigen thread.
pub type Melder = Arc<dyn Fn(OpnameGebeurtenis) + Send + Sync>;

/// Een melder die niets doet.
#[must_use]
pub fn geen_melder() -> Melder {
    Arc::new(|_| {})
}

/// De grenzen van een opname in samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grenzen {
    /// Vanaf zoveel samples komt [`OpnameGebeurtenis::BijnaVol`].
    pub bijna_vol: usize,
    /// Zoveel samples nemen we hooguit aan; dan komt [`OpnameGebeurtenis::Vol`].
    pub max: usize,
}

impl Grenzen {
    /// De grenzen van ADR 0012: waarschuwing na negen minuten, einde na tien.
    #[must_use]
    pub fn standaard() -> Self {
        Self {
            bijna_vol: duur_naar_samples(BIJNA_VOL_NA),
            max: duur_naar_samples(MAX_OPNAME),
        }
    }
}

fn duur_naar_samples(duur: Duration) -> usize {
    usize::try_from(duur.as_secs()).unwrap_or(usize::MAX) * DOEL_SAMPLERATE as usize
}

fn slot<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// De toestand die uitgang en eigenaar delen.
struct Gedeeld {
    grenzen: Grenzen,
    melder: Melder,
    samples: Mutex<Vec<f32>>,
    /// Het moment van `start`, voor de meting van het eerste blok.
    begin: Mutex<Option<Instant>>,
    eerste_blok: Mutex<Option<Duration>>,
    bijna_vol_gemeld: AtomicBool,
    vol_gemeld: AtomicBool,
}

impl Gedeeld {
    fn wis(&self) {
        let mut samples = slot(&self.samples);
        samples.fill(0.0);
        samples.clear();
        samples.shrink_to_fit();
    }
}

/// Krijgt tijdens een opname het niveau (0 tot 1), ongeveer twintig keer per seconde.
pub type NiveauMelder = Box<dyn FnMut(f32) + Send + 'static>;

/// De uitgang naar het geheugen: verzamelt de samples van één dictaat, tot de grens. Met
/// een niveaumelder geeft hij ook het niveau door, voor de meter in beeld; dat niveau
/// wordt nergens bewaard.
struct GeheugenUitgang {
    gedeeld: Arc<Gedeeld>,
    niveau: Option<niveau::NiveauUitgang<NiveauMelder>>,
}

impl Uitgang for GeheugenUitgang {
    fn ontvang(&mut self, blok: &[f32]) {
        let gedeeld = &self.gedeeld;
        if blok.is_empty() {
            return;
        }
        if let Some(niveau) = self.niveau.as_mut() {
            niveau.ontvang(blok);
        }
        let eerste_duur = {
            let mut eerste = slot(&gedeeld.eerste_blok);
            let begin = *slot(&gedeeld.begin);
            match (eerste.is_none(), begin) {
                (true, Some(begin)) => {
                    let duur = begin.elapsed();
                    *eerste = Some(duur);
                    Some(duur)
                }
                _ => None,
            }
        };
        if let Some(duur) = eerste_duur {
            (gedeeld.melder)(OpnameGebeurtenis::EersteBlok(duur));
        }
        let nu = {
            let mut samples = slot(&gedeeld.samples);
            let ruimte = gedeeld.grenzen.max.saturating_sub(samples.len());
            samples.extend_from_slice(&blok[..blok.len().min(ruimte)]);
            samples.len()
        };
        if nu >= gedeeld.grenzen.bijna_vol && !gedeeld.bijna_vol_gemeld.swap(true, Ordering::SeqCst)
        {
            (gedeeld.melder)(OpnameGebeurtenis::BijnaVol);
        }
        if nu >= gedeeld.grenzen.max && !gedeeld.vol_gemeld.swap(true, Ordering::SeqCst) {
            (gedeeld.melder)(OpnameGebeurtenis::Vol);
        }
    }
}

/// Wat een opname opleverde. De samples worden bij het loslaten op nul gezet.
#[derive(Debug)]
pub struct Opgenomen {
    samples: Vec<f32>,
    /// Is de opname onderbroken (bijvoorbeeld door een headset die wegviel)? Wat tot dan
    /// is opgenomen staat in [`Opgenomen::samples`].
    pub onderbroken: Option<Foutmelding>,
    /// Aantal samples dat verloren ging omdat de uitgang te traag was.
    pub verloren: u64,
    /// Tijd van `start` tot het eerste blok samples, als dat kwam.
    pub tijd_tot_eerste_blok: Option<Duration>,
    /// Zat de opname tegen de grens van tien minuten aan?
    pub vol: bool,
}

impl Opgenomen {
    /// De samples (16 kHz, mono).
    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// De duur van de opname.
    #[must_use]
    pub fn duur(&self) -> Duration {
        crate::audio::duur_van_samples(self.samples.len())
    }

    /// Het grootste absolute sample (0 tot 1), als maat voor het niveau.
    #[must_use]
    pub fn piek(&self) -> f32 {
        self.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
    }
}

impl Drop for Opgenomen {
    fn drop(&mut self) {
        // Een dictaat blijft niet in het geheugen achter.
        self.samples.fill(0.0);
    }
}

/// Een geopende microfoon met een geheugen-uitgang: één dictaat tegelijk.
pub struct Opname {
    bron: Box<dyn Bron>,
    gedeeld: Arc<Gedeeld>,
    uitval: Arc<Mutex<Option<OpnameFout>>>,
    open_duur: Duration,
    bezig: bool,
    niveau: Option<NiveauMelder>,
}

impl Opname {
    /// Opent een microfoon (`None` is de standaard) met de grenzen van ADR 0012. Dit kan
    /// bij een Bluetooth-headset seconden duren; de duur staat in [`Opname::open_duur`].
    ///
    /// # Errors
    ///
    /// Geeft een [`Foutmelding`] in gewone taal als de microfoon niet te openen is.
    pub fn open(
        apparatuur: &dyn Apparatuur,
        microfoon: Option<&str>,
        melder: Melder,
    ) -> Result<Self, Foutmelding> {
        Self::open_met_grenzen(apparatuur, microfoon, melder, Grenzen::standaard())
    }

    /// Als [`Opname::open`], met eigen grenzen (voor tests).
    ///
    /// # Errors
    ///
    /// Zie [`Opname::open`].
    pub fn open_met_grenzen(
        apparatuur: &dyn Apparatuur,
        microfoon: Option<&str>,
        melder: Melder,
        grenzen: Grenzen,
    ) -> Result<Self, Foutmelding> {
        let uitval: Arc<Mutex<Option<OpnameFout>>> = Arc::new(Mutex::new(None));
        let voor_melder = Arc::clone(&uitval);
        let meld = Arc::clone(&melder);
        let bij_uitval: UitvalMelder = Box::new(move |fout| {
            let melding = Foutmelding::from(&fout);
            *slot(&voor_melder) = Some(fout);
            meld(OpnameGebeurtenis::Uitgevallen(melding));
        });
        let begin = Instant::now();
        let bron = apparatuur
            .open(microfoon, bij_uitval)
            .map_err(|fout| Foutmelding::from(&fout))?;
        let open_duur = begin.elapsed();
        Ok(Self {
            bron,
            gedeeld: Arc::new(Gedeeld {
                grenzen,
                melder,
                samples: Mutex::new(Vec::new()),
                begin: Mutex::new(None),
                eerste_blok: Mutex::new(None),
                bijna_vol_gemeld: AtomicBool::new(false),
                vol_gemeld: AtomicBool::new(false),
            }),
            uitval,
            open_duur,
            bezig: false,
            niveau: None,
        })
    }

    /// Hoe lang het openen van de microfoon duurde.
    #[must_use]
    pub fn open_duur(&self) -> Duration {
        self.open_duur
    }

    /// Geeft tijdens de opname het niveau door aan `melder` (zie
    /// [`niveau::NiveauUitgang`]), op een thread van de bron. Roep dit aan voor
    /// [`Opname::start`]; het geldt voor die ene opname.
    pub fn bij_niveau(&mut self, melder: impl FnMut(f32) + Send + 'static) {
        self.niveau = Some(Box::new(melder));
    }

    /// Begint met opnemen. Wacht niet op het eerste geluid: de meting van het eerste blok
    /// loopt vanaf dit moment.
    ///
    /// # Errors
    ///
    /// Geeft een [`Foutmelding`] als de opname al loopt of de microfoon eerder uitviel.
    pub fn start(&mut self) -> Result<(), Foutmelding> {
        if self.bezig {
            return Err(OpnameFout::nieuw(FoutSoort::Programma).into());
        }
        if let Some(fout) = slot(&self.uitval).as_ref() {
            return Err(fout.into());
        }
        self.gedeeld.wis();
        *slot(&self.gedeeld.eerste_blok) = None;
        self.gedeeld.bijna_vol_gemeld.store(false, Ordering::SeqCst);
        self.gedeeld.vol_gemeld.store(false, Ordering::SeqCst);
        *slot(&self.gedeeld.begin) = Some(Instant::now());
        let uitgang = GeheugenUitgang {
            gedeeld: Arc::clone(&self.gedeeld),
            niveau: self.niveau.take().map(niveau::NiveauUitgang::nieuw),
        };
        self.bron
            .start(Box::new(uitgang))
            .map_err(|fout| Foutmelding::from(&fout))?;
        self.bezig = true;
        Ok(())
    }

    /// Tijd van `start` tot het eerste blok samples, zodra dat er is.
    #[must_use]
    pub fn tijd_tot_eerste_blok(&self) -> Option<Duration> {
        *slot(&self.gedeeld.eerste_blok)
    }

    /// Hoeveel er tot nu toe is opgenomen.
    #[must_use]
    pub fn lengte(&self) -> Duration {
        crate::audio::duur_van_samples(slot(&self.gedeeld.samples).len())
    }

    /// Beëindigt de opname en geeft de samples terug. Is de microfoon intussen
    /// uitgevallen, dan staat dat in [`Opgenomen::onderbroken`] en zijn de samples tot dat
    /// moment bewaard.
    ///
    /// # Errors
    ///
    /// Geeft een [`Foutmelding`] als er geen opname liep.
    pub fn stop(mut self) -> Result<Opgenomen, Foutmelding> {
        if !self.bezig {
            return Err(OpnameFout::nieuw(FoutSoort::Programma).into());
        }
        self.bezig = false;
        let einde = self.bron.stop().map_err(|fout| Foutmelding::from(&fout))?;
        let samples = std::mem::take(&mut *slot(&self.gedeeld.samples));
        let uitgevallen = einde.uitgevallen.or_else(|| slot(&self.uitval).clone());
        Ok(Opgenomen {
            samples,
            onderbroken: uitgevallen.as_ref().map(Foutmelding::from),
            verloren: einde.verloren,
            tijd_tot_eerste_blok: *slot(&self.gedeeld.eerste_blok),
            vol: self.gedeeld.vol_gemeld.load(Ordering::SeqCst),
        })
    }
}

impl Drop for Opname {
    fn drop(&mut self) {
        // Weggooien zonder stop (annuleren): de bron stoppen en het geheugen schoonmaken.
        if self.bezig {
            let _ = self.bron.stop();
        }
        self.gedeeld.wis();
    }
}

#[cfg(test)]
mod tests;
