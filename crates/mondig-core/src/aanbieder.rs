//! Aanbieders: alles wat audio in tekst omzet, achter één interface.
//!
//! Elke aanbieder heeft een [`Soevereiniteit`], zodat de app kan tonen en bewaken waar
//! audio wordt verwerkt. [`LokaleEngine`] is de eerste implementatie en draait de
//! spraakengine in een eigen werkproces via de [`Supervisor`]. Cloudaanbieders komen in
//! fase 6.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde::Serialize;

use crate::audio::{self, AudioFout, Leeswaarschuwing};
use crate::engine::protocol::{Apparaat, FoutSoort, Segment, SprekerSegment};
use crate::engine::supervisor::{
    EngineFout, Instellingen, LaadOpdracht, Supervisor, Voortgang, controleer_dictaatlengte,
};
use crate::filter::{self, FilterInstellingen, Gefilterd, Meting};
use crate::gesprek::sprekers::Toegewezen;
use crate::model::{self, Model};
use crate::soevereiniteit::Soevereiniteit;
use crate::spoor::{SpoorFout, Spoorlezer, Tijdlijn};

/// De uitkomst van een transcriptie: tekst in segmenten met tijdcodes.
/// Alle exportformaten worden hieruit gemaakt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    /// De segmenten in volgorde.
    pub segmenten: Vec<Segment>,
    /// Duur van de audio.
    pub audio_duur: Duration,
}

impl Transcript {
    /// Alle tekst achter elkaar, segmenten gescheiden door een spatie. Lege segmenten
    /// worden overgeslagen.
    #[must_use]
    pub fn tekst(&self) -> String {
        self.segmenten
            .iter()
            .map(|s| s.tekst.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Een uitgeschreven bestand: het gefilterde transcript plus wat er bij het lezen opviel,
/// zoals een afgebroken bestand. Bewust zonder `Debug`, zodat de tekst nooit in een log komt.
#[derive(Clone)]
pub struct Uitgeschreven {
    /// Het transcript na het filter (of zonder filter).
    pub gefilterd: Gefilterd,
    /// Wat bij het lezen opviel; `None` als er niets was.
    pub waarschuwing: Option<Leeswaarschuwing>,
    /// De uitingen per spreker als de sprekers herkend zijn (4b-D, fase D-2): de segmenten van
    /// het transcript, waar nodig gesplitst bij een sprekerwissel. `None` zonder sprekerherkenning,
    /// zonder sprekermodel, of als er maar één spreker is gevonden.
    pub sprekers: Option<Vec<Toegewezen>>,
    /// Waarom de sprekers niet herkend konden worden, als dat mislukte; het transcript is er
    /// dan wel, zonder sprekers.
    pub sprekerfout: Option<String>,
}

impl Uitgeschreven {
    /// De uitingen van het transcript met hun spreker, in volgorde: met herkende sprekers de
    /// delen uit [`Uitgeschreven::sprekers`], anders de segmenten zonder spreker. Elke export en
    /// het scherm lezen hieruit, zodat ze nooit uit elkaar lopen.
    #[must_use]
    pub fn uitingen(&self) -> Vec<(&Segment, Option<u8>)> {
        match &self.sprekers {
            Some(delen) => delen.iter().map(|d| (&d.segment, d.spreker)).collect(),
            None => self
                .gefilterd
                .transcript
                .segmenten
                .iter()
                .map(|s| (s, None))
                .collect(),
        }
    }
}

/// Wat een aanbieder uitschrijft: een bestand of geluid uit het geheugen (ADR 0012).
#[derive(Debug, Clone, Copy)]
pub enum Invoer<'a> {
    /// Een audiobestand op schijf.
    Bestand(&'a Path),
    /// Geluid in het geheugen: 16 kHz, mono, `f32` (bijvoorbeeld een dictaat).
    Geluid(&'a [f32]),
}

/// Wat er mis kan gaan bij het uitschrijven.
#[derive(Debug, thiserror::Error)]
pub enum AanbiederFout {
    /// Het audiobestand is niet te lezen.
    #[error(transparent)]
    Audio(#[from] AudioFout),
    /// De spraakengine faalde.
    #[error(transparent)]
    Engine(#[from] EngineFout),
}

/// De duur waarmee de time-out van de sprekerherkenning rekent als de container de duur van een
/// bestand niet zegt (vier uur).
const ONBEKENDE_DUUR: Duration = Duration::from_hours(4);

/// Zet de stroomtijden van een spoor om naar de tijdlijn van de opname. Een tijd kan net voor
/// nul liggen (een geschat anker van de microfoon); die wordt nul. De grenzen tussen stukken
/// vallen op hele milliseconden (een stuk is een minuut), dus heen en terug verliest niets.
fn naar_tijdlijn(transcript: &mut Transcript, tijdlijn: &Tijdlijn) {
    let positie = |ms: u64| ms.saturating_mul(u64::from(audio::DOEL_SAMPLERATE)) / 1000;
    let op_tijdlijn = |ms: i64| u64::try_from(ms).unwrap_or(0);
    for segment in &mut transcript.segmenten {
        segment.start_ms = op_tijdlijn(tijdlijn.ms(positie(segment.start_ms)));
        segment.eind_ms = op_tijdlijn(tijdlijn.eind_ms(positie(segment.eind_ms)));
        for woord in &mut segment.woorden {
            woord.start_ms = op_tijdlijn(tijdlijn.ms(positie(woord.start_ms)));
            woord.eind_ms = op_tijdlijn(tijdlijn.eind_ms(positie(woord.eind_ms)));
        }
    }
    transcript.audio_duur = Duration::from_millis(op_tijdlijn(tijdlijn.eind()));
}

/// Zet de stroomtijden van de sprekers op een spoor om naar de tijdlijn van de opname, op
/// dezelfde manier als [`naar_tijdlijn`]: zo vallen woorden en sprekers op dezelfde plek.
fn sprekers_naar_tijdlijn(sprekers: &mut [SprekerSegment], tijdlijn: &Tijdlijn) {
    let positie = |ms: u64| ms.saturating_mul(u64::from(audio::DOEL_SAMPLERATE)) / 1000;
    let op_tijdlijn = |ms: i64| u64::try_from(ms).unwrap_or(0);
    for s in sprekers {
        s.start_ms = op_tijdlijn(tijdlijn.ms(positie(s.start_ms)));
        s.eind_ms = op_tijdlijn(tijdlijn.eind_ms(positie(s.eind_ms)));
    }
}

/// Een spoor dat niet te lezen is, als [`AanbiederFout`]: een fout van de soort
/// [`FoutSoort::Audio`], zoals het werkproces die ook zou melden. Een eigen variant zou elke
/// `match` op [`AanbiederFout`] in de app en de CLI raken.
pub(crate) fn spoorfout(fout: &SpoorFout) -> AanbiederFout {
    AanbiederFout::Engine(EngineFout::Engine {
        soort: FoutSoort::Audio,
        melding: fout.to_string(),
    })
}

/// Iets dat audio in tekst omzet.
pub trait Aanbieder {
    /// Naam in gewone taal, bijvoorbeeld de modelnaam.
    fn naam(&self) -> String;

    /// Waar de audio wordt verwerkt.
    fn soevereiniteit(&self) -> Soevereiniteit;

    /// Schrijft een bestand of geluid uit het geheugen uit.
    ///
    /// # Errors
    ///
    /// Geeft een [`AanbiederFout`] in gewone taal als het bestand of de engine faalt.
    fn transcribeer(&mut self, invoer: Invoer<'_>) -> Result<Transcript, AanbiederFout>;
}

/// Waarop een lokale motor werkelijk rekent, voor de regel "In gebruik" in de app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rekenstand {
    /// De processor, zoals gevraagd.
    Processor,
    /// De videokaart, zoals gevraagd.
    Videokaart,
    /// De videokaart was gevraagd maar gaf een fout; de motor rekent nu op de processor
    /// (ADR 0009) en blijft daar tot hij opnieuw gemaakt wordt.
    Terugval,
}

impl Rekenstand {
    /// De stand bij het gevraagde en het werkelijke apparaat.
    #[must_use]
    pub fn uit(gevraagd: Apparaat, werkelijk: Apparaat) -> Self {
        match (gevraagd, werkelijk) {
            (_, Apparaat::Gpu) => Self::Videokaart,
            (Apparaat::Gpu, Apparaat::Cpu) => Self::Terugval,
            (Apparaat::Cpu, Apparaat::Cpu) => Self::Processor,
        }
    }
}

/// Krijgt elke nieuwe [`Rekenstand`] van een motor.
type Melder = Box<dyn Fn(Rekenstand) + Send>;

/// De lokale spraakengine in een eigen werkproces (label [`Soevereiniteit::Lokaal`]).
pub struct LokaleEngine {
    supervisor: Supervisor,
    opdracht: LaadOpdracht,
    model_naam: String,
    /// Het apparaat waar de app om vroeg. `opdracht.apparaat` wordt na een terugval de
    /// processor; dit blijft staan, zodat de stand "terugval" kan zeggen.
    gevraagd: Apparaat,
    /// De laatst gemelde stand, of `None` als het model nog nooit geladen is.
    stand: Option<Rekenstand>,
    melder: Option<Melder>,
}

impl LokaleEngine {
    /// Maakt een lokale engine voor een model. Het werkproces start pas bij
    /// [`LokaleEngine::laad`] of de eerste transcriptie.
    #[must_use]
    pub fn nieuw(
        instellingen: Instellingen,
        model: &Model,
        modelmap: PathBuf,
        apparaat: Apparaat,
        taal: Option<String>,
    ) -> Self {
        Self {
            supervisor: Supervisor::nieuw(instellingen),
            opdracht: LaadOpdracht {
                model_id: model.id.clone(),
                modelmap,
                apparaat,
                taal,
            },
            model_naam: model.naam.clone(),
            gevraagd: apparaat,
            stand: None,
            melder: None,
        }
    }

    /// Meldt elke nieuwe [`Rekenstand`] aan `melder`: na het eerste laden en na een terugval
    /// op de processor. De melder draait op de thread van de motor; hou hem kort.
    #[must_use]
    pub fn met_melder(mut self, melder: impl Fn(Rekenstand) + Send + 'static) -> Self {
        self.melder = Some(Box::new(melder));
        self
    }

    /// Waarop de motor het laatst rekende, of `None` als het model nog nooit geladen is.
    /// Na het ontladen (rusttijd) blijft de laatste stand staan.
    #[must_use]
    pub fn rekenstand(&self) -> Option<Rekenstand> {
        self.stand
    }

    /// Kijkt na een opdracht waarop het model nu draait. Viel de supervisor terug op de
    /// processor, dan laadt deze motor voortaan ook zelf op de processor: na het ontladen
    /// (rusttijd) probeert hij de videokaart niet opnieuw. Pas een nieuwe motor (de gebruiker
    /// zet de schakelaar om, of Mondig start opnieuw) vraagt weer de videokaart.
    fn werk_stand_bij(&mut self) {
        let Some(werkelijk) = self.supervisor.apparaat() else {
            return;
        };
        if werkelijk == Apparaat::Cpu {
            self.opdracht.apparaat = Apparaat::Cpu;
        }
        let stand = Rekenstand::uit(self.gevraagd, werkelijk);
        if self.stand != Some(stand) {
            self.stand = Some(stand);
            if let Some(melder) = &self.melder {
                melder(stand);
            }
        }
    }

    /// Laadt het model als dat nog niet gebeurd is. Geeft het apparaat terug waarop het
    /// model draait; dat kan de processor zijn als de videokaart niet werkte.
    ///
    /// # Errors
    ///
    /// Geeft een [`EngineFout`] als het werkproces niet start of het model niet laadt.
    pub fn laad(&mut self) -> Result<Apparaat, EngineFout> {
        let uitslag = match self.supervisor.apparaat() {
            Some(apparaat) => Ok(apparaat),
            None => self.supervisor.laad(&self.opdracht),
        };
        self.werk_stand_bij();
        uitslag
    }

    /// Draait het werkproces nu (zie [`Supervisor::draait`])?
    #[must_use]
    pub fn draait(&self) -> bool {
        self.supervisor.draait()
    }

    /// Stopt het werkproces. Het geheugen van het model komt daarmee terug; de volgende
    /// transcriptie laadt het model opnieuw.
    pub fn stop(&mut self) {
        self.supervisor.stop();
    }
}

impl LokaleEngine {
    /// Schrijft een bestand of geluid uit en past het filter tegen verzonnen tekst toe.
    ///
    /// De app decodeert de audio hiervoor niet zelf: de duur (voor de time-out) komt uit
    /// de metadata van het bestand en het werkproces stuurt de metingen voor het filter
    /// (RMS en energie per 30 ms) mee. Bij digitale stilte slaat het werkproces het
    /// model over: er valt niets te horen en het model zou alleen iets kunnen verzinnen.
    ///
    /// # Errors
    ///
    /// Geeft een [`AanbiederFout`] als het bestand of de engine faalt.
    pub fn transcribeer_gefilterd(
        &mut self,
        invoer: Invoer<'_>,
        instellingen: &FilterInstellingen,
    ) -> Result<Gefilterd, AanbiederFout> {
        let (transcript, meting) = self.schrijf_uit(invoer, false)?;
        Ok(filter::pas_toe(transcript, &meting, instellingen))
    }

    /// Schrijft een dictaat (geluid uit het geheugen) uit en past het filter toe, met de
    /// keuze om stilte aan begin en eind weg te knippen voordat het model het hoort.
    ///
    /// Het werkproces knipt met Silero. Staat dat model niet naast het spraakmodel, dan
    /// wordt er gewoon zonder knippen uitgeschreven; dat is geen fout. Vindt Silero geen
    /// spraak, dan komt er een resultaat zonder segmenten en zegt het filter "geen tekst".
    ///
    /// # Errors
    ///
    /// Geeft een [`AanbiederFout`] als de engine faalt of het dictaat te lang is.
    pub fn transcribeer_dictaat(
        &mut self,
        samples: &[f32],
        instellingen: &FilterInstellingen,
        knip_stilte: bool,
    ) -> Result<Gefilterd, AanbiederFout> {
        let (transcript, meting) = self.schrijf_uit(Invoer::Geluid(samples), knip_stilte)?;
        Ok(filter::pas_toe(transcript, &meting, instellingen))
    }

    /// Schrijft een bestand uit met [`Voortgang`] na elk stuk en de mogelijkheid om te
    /// annuleren (zie [`Supervisor::transcribeer_bestand`]), en past het filter toe als
    /// `filter` er is. Zonder filter komt het transcript zoals het model het gaf
    /// ([`filter::zonder_filter`]).
    ///
    /// # Errors
    ///
    /// Geeft een [`AanbiederFout`] als het bestand of de engine faalt, met
    /// [`EngineFout::Geannuleerd`] na een annulering.
    pub fn transcribeer_bestand(
        &mut self,
        bestand: &Path,
        filter: Option<&FilterInstellingen>,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Uitgeschreven, AanbiederFout> {
        // Alleen de kop van de container: de app decodeert niets (ADR 0009). Staat de duur
        // er niet in, dan telt het werkproces haar zelf.
        let audio_duur = audio::duur(bestand)?;
        self.laad()?;
        let uitkomst = self
            .supervisor
            .transcribeer_bestand(bestand, audio_duur, voortgang, annuleer);
        self.werk_stand_bij();
        let uitkomst = uitkomst?;
        let transcript = Transcript {
            segmenten: uitkomst.segmenten,
            audio_duur: Duration::from_millis(uitkomst.duur_ms),
        };
        let gefilterd = match filter {
            Some(instellingen) => filter::pas_toe(transcript, &uitkomst.meting, instellingen),
            None => filter::zonder_filter(transcript, &uitkomst.meting),
        };
        Ok(Uitgeschreven {
            gefilterd,
            waarschuwing: uitkomst.waarschuwing,
            sprekers: None,
            sprekerfout: None,
        })
    }

    /// Schrijft een spoor van een vergaderopname uit per uiting (sprint 4b-B), met
    /// [`Voortgang`] en de mogelijkheid om te annuleren (zie
    /// [`Supervisor::transcribeer_spoor`]), en past het filter toe als `filter` er is.
    ///
    /// `map` is de map van het spoor (`<opnamemap>/microfoon` of `<opnamemap>/systeem`). De
    /// segmenten staan op de tijdlijn van de opname: één per uiting, met de tijden van
    /// Silero. Het werkproces geeft stroomtijden (zoals de meting); het filter draait op die
    /// tijden en pas daarna gaan ze naar de tijdlijn, zodat het filter stilte zoekt op de
    /// plek waar de tekst werkelijk stond. De lengte van het spoor (voor de time-out) komt uit het logboek; een opname
    /// die nog niet is afgesloten, wordt geweigerd voordat het werkproces iets ziet.
    ///
    /// # Errors
    ///
    /// Geeft een [`AanbiederFout`] als het spoor niet te lezen is of de engine faalt
    /// (ook als de stiltedetectie niet klaarstaat), met [`EngineFout::Geannuleerd`] na een
    /// annulering. Een spoor dat niet te lezen is, komt als [`EngineFout::Engine`] met
    /// [`FoutSoort::Audio`], net als wanneer het werkproces het zelf meldt.
    pub fn transcribeer_spoor(
        &mut self,
        map: &Path,
        filter: Option<&FilterInstellingen>,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Uitgeschreven, AanbiederFout> {
        let tijdlijn = Spoorlezer::open(map)
            .map(|lezer| lezer.tijdlijn().clone())
            .map_err(|f| spoorfout(&f))?;
        let spoor_duur = Duration::from_millis(tijdlijn.stroom_ms());
        self.laad()?;
        let uitkomst = self
            .supervisor
            .transcribeer_spoor(map, spoor_duur, voortgang, annuleer);
        self.werk_stand_bij();
        let uitkomst = uitkomst?;
        let transcript = Transcript {
            segmenten: uitkomst.segmenten,
            audio_duur: Duration::from_millis(uitkomst.duur_ms),
        };
        let mut gefilterd = match filter {
            Some(instellingen) => filter::pas_toe(transcript, &uitkomst.meting, instellingen),
            None => filter::zonder_filter(transcript, &uitkomst.meting),
        };
        naar_tijdlijn(&mut gefilterd.transcript, &tijdlijn);
        Ok(Uitgeschreven {
            gefilterd,
            waarschuwing: uitkomst.waarschuwing,
            sprekers: None,
            sprekerfout: None,
        })
    }

    /// Staat het sprekermodel (Sortformer) helemaal naast het spraakmodel van deze motor? Alleen
    /// dan heeft [`LokaleEngine::herken_sprekers`] zin.
    #[must_use]
    pub fn heeft_sprekermodel(&self) -> bool {
        model::sprekermodel_naast(&self.opdracht.modelmap).is_some()
    }

    /// Herkent de sprekers op een spoor van een vergaderopname (4b-D), op dezelfde stroom als
    /// [`LokaleEngine::transcribeer_spoor`], met tijden op de tijdlijn van de opname. Staat het
    /// sprekermodel er niet (zie [`LokaleEngine::heeft_sprekermodel`]), dan `Ok(None)` en
    /// verandert er niets: het werkproces wordt dan niet eens gevraagd.
    ///
    /// # Errors
    ///
    /// Geeft een [`AanbiederFout`] als het spoor niet te lezen is of de engine faalt, met
    /// [`EngineFout::Geannuleerd`] na een annulering.
    pub fn herken_sprekers(
        &mut self,
        map: &Path,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Option<Vec<SprekerSegment>>, AanbiederFout> {
        if !self.heeft_sprekermodel() {
            return Ok(None);
        }
        let tijdlijn = Spoorlezer::open(map)
            .map(|lezer| lezer.tijdlijn().clone())
            .map_err(|f| spoorfout(&f))?;
        let spoor_duur = Duration::from_millis(tijdlijn.stroom_ms());
        self.laad()?;
        let uitkomst = self
            .supervisor
            .herken_sprekers(map, spoor_duur, voortgang, annuleer);
        self.werk_stand_bij();
        let mut sprekers = uitkomst?;
        sprekers_naar_tijdlijn(&mut sprekers, &tijdlijn);
        Ok(Some(sprekers))
    }

    /// Herkent de sprekers in een audiobestand (4b-D, fase D-2), met tijden van het bestand zelf.
    /// Net als [`LokaleEngine::herken_sprekers`]: staat het sprekermodel er niet (zie
    /// [`LokaleEngine::heeft_sprekermodel`]), dan `Ok(None)` en wordt het werkproces niet eens
    /// gevraagd; de terugval op de processor en het annuleren zijn die van de supervisor.
    ///
    /// # Errors
    ///
    /// Geeft een [`AanbiederFout`] als het bestand niet te lezen is of de engine faalt, met
    /// [`EngineFout::Geannuleerd`] na een annulering.
    pub fn herken_sprekers_bestand(
        &mut self,
        bestand: &Path,
        voortgang: &mut dyn FnMut(Voortgang),
        annuleer: &AtomicBool,
    ) -> Result<Option<Vec<SprekerSegment>>, AanbiederFout> {
        if !self.heeft_sprekermodel() {
            return Ok(None);
        }
        // Alleen de kop van de container, voor de time-out; kent die de duur niet, dan een
        // ruime grens, want de voortgang houdt het werkproces toch levend.
        let audio_duur = audio::duur(bestand)?.unwrap_or(ONBEKENDE_DUUR);
        self.laad()?;
        let uitkomst = self
            .supervisor
            .herken_sprekers_bestand(bestand, audio_duur, voortgang, annuleer);
        self.werk_stand_bij();
        Ok(Some(uitkomst?))
    }

    /// Laadt het model, schrijft het bestand uit en geeft transcript plus meting terug.
    /// `knip_stilte` geldt alleen voor geluid uit het geheugen.
    fn schrijf_uit(
        &mut self,
        invoer: Invoer<'_>,
        knip_stilte: bool,
    ) -> Result<(Transcript, Meting), AanbiederFout> {
        let uitkomst = match invoer {
            Invoer::Bestand(bestand) => {
                // Alleen de kop van de container, voor de time-out; decoderen doet het werkproces.
                let audio_duur = audio::duur(bestand)?;
                self.laad()?;
                self.supervisor.transcribeer_bestand(
                    bestand,
                    audio_duur,
                    &mut |_| {},
                    &AtomicBool::new(false),
                )
            }
            Invoer::Geluid(samples) => {
                // Eerst de lengte: een te lang dictaat mag het werkproces niet eens starten.
                controleer_dictaatlengte(samples)?;
                self.laad()?;
                self.supervisor.transcribeer_geluid(samples, knip_stilte)
            }
        };
        // Ook na een fout: de supervisor kan intussen op de processor zijn teruggevallen.
        self.werk_stand_bij();
        let uitkomst = uitkomst?;
        let transcript = Transcript {
            segmenten: uitkomst.segmenten,
            audio_duur: Duration::from_millis(uitkomst.duur_ms),
        };
        Ok((transcript, uitkomst.meting))
    }
}

impl Aanbieder for LokaleEngine {
    fn naam(&self) -> String {
        self.model_naam.clone()
    }

    fn soevereiniteit(&self) -> Soevereiniteit {
        Soevereiniteit::Lokaal
    }

    fn transcribeer(&mut self, invoer: Invoer<'_>) -> Result<Transcript, AanbiederFout> {
        self.schrijf_uit(invoer, false)
            .map(|(transcript, _)| transcript)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroomtijden_van_een_spoor_gaan_na_het_filter_naar_de_tijdlijn() {
        // Twee stukken van een minuut: het eerste met een anker net voor nul (een geschatte
        // microfoon), het tweede 250 ms later dan de stroom zegt (een gat).
        let tijdlijn = Tijdlijn::nieuw([(960_000, -40), (960_000, 60_210)]);
        let mut transcript = Transcript {
            segmenten: vec![
                Segment::nieuw(0, 1_000, "eerste"),
                Segment::nieuw(59_000, 60_000, "op de grens"),
                Segment::nieuw(60_000, 61_500, "tweede stuk"),
            ],
            audio_duur: Duration::from_secs(120),
        };
        transcript.segmenten[2].woorden = vec![crate::engine::protocol::Woord {
            start_ms: 60_100,
            eind_ms: 60_500,
            tekst: "tweede".into(),
        }];
        naar_tijdlijn(&mut transcript, &tijdlijn);
        let tijden: Vec<(u64, u64)> = transcript
            .segmenten
            .iter()
            .map(|s| (s.start_ms, s.eind_ms))
            .collect();
        assert_eq!(tijden, vec![(0, 960), (58_960, 59_960), (60_210, 61_710)]);
        assert_eq!(transcript.audio_duur, Duration::from_millis(120_210));
        // Woorden gaan mee, met hetzelfde anker als hun segment.
        let woord = &transcript.segmenten[2].woorden[0];
        assert_eq!((woord.start_ms, woord.eind_ms), (60_310, 60_710));
        // En de sprekers net zo.
        let mut sprekers = vec![
            SprekerSegment {
                spreker: 1,
                start_ms: 0,
                eind_ms: 1_000,
            },
            SprekerSegment {
                spreker: 2,
                start_ms: 60_000,
                eind_ms: 61_500,
            },
        ];
        sprekers_naar_tijdlijn(&mut sprekers, &tijdlijn);
        assert_eq!(
            sprekers
                .iter()
                .map(|s| (s.start_ms, s.eind_ms))
                .collect::<Vec<_>>(),
            vec![(0, 960), (60_210, 61_710)]
        );
    }

    fn segment(tekst: &str) -> Segment {
        Segment::nieuw(0, 1, tekst)
    }

    #[test]
    fn de_rekenstand_volgt_uit_gevraagd_en_werkelijk_apparaat() {
        assert_eq!(
            Rekenstand::uit(Apparaat::Cpu, Apparaat::Cpu),
            Rekenstand::Processor
        );
        assert_eq!(
            Rekenstand::uit(Apparaat::Gpu, Apparaat::Gpu),
            Rekenstand::Videokaart
        );
        assert_eq!(
            Rekenstand::uit(Apparaat::Gpu, Apparaat::Cpu),
            Rekenstand::Terugval
        );
    }

    #[test]
    fn de_rekenstand_gaat_als_snake_case_naar_de_interface() {
        let json = |s| serde_json::to_string(&s).unwrap();
        assert_eq!(json(Rekenstand::Processor), "\"processor\"");
        assert_eq!(json(Rekenstand::Videokaart), "\"videokaart\"");
        assert_eq!(json(Rekenstand::Terugval), "\"terugval\"");
    }

    #[test]
    fn tekst_voegt_segmenten_samen_en_slaat_lege_over() {
        let transcript = Transcript {
            segmenten: vec![segment(" Hallo"), segment("  "), segment("wereld. ")],
            audio_duur: Duration::from_secs(2),
        };
        assert_eq!(transcript.tekst(), "Hallo wereld.");
    }
}
