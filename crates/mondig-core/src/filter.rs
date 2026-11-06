//! Filter tegen verzonnen tekst.
//!
//! Spraakmodellen verzinnen soms tekst waar niets gezegd wordt: bij stilte schrijft
//! Whisper bijvoorbeeld "Ondertiteling ingediend door de Amara.org gemeenschap", en
//! een model kan in een lus blijven hangen. Geen van de onderzochte projecten (Handy,
//! Vibe) heeft daar een volledige poort voor. Dit filter
//! doet vijf dingen, allemaal op energie en tekst, zonder spraakdetectiemodel (Silero
//! komt in fase 3):
//!
//! 1. **Digitale stilte**: heeft de audio vrijwel geen energie, dan is de uitkomst
//!    [`Uitkomst::GeenGeluid`] en blijft er geen tekst over.
//! 2. **Herhalingen**: een woordgroep die drie keer of vaker direct achter elkaar komt,
//!    wordt tot één keer ingekort. Twee keer blijft staan.
//! 3. **Bekende stiltezinnen**: een korte lijst zinnen die modellen bij stilte
//!    verzinnen. Ze gaan alleen weg als de hele uitvoer eruit bestaat, of als de audio
//!    rond dat segment stil is. Midden in gewone spraak blijven ze staan.
//! 4. **Aandeel spraak**: een schatting op energie. Is dat heel laag terwijl de tekst
//!    lang is, dan wordt de uitkomst [`Uitkomst::Verdacht`]. De tekst blijft dan
//!    staan; de aanroeper beslist.
//! 5. **Stopwoorden**: klanken als "ehm" en "uh" gaan weg, met vaste regels (ADR 0028).
//!    Dit gebeurt vóór de andere stappen en dus ook vóór opschonen met een taalmodel
//!    (ADR 0027).
//!
//! Alle drempels staan als constanten in [`FilterInstellingen::standaard`].

use serde::{Deserialize, Serialize};

use crate::aanbieder::Transcript;
use crate::audio::DOEL_SAMPLERATE;
use crate::engine::protocol::{Segment, Woord as Woordtijd};
use crate::meten::normaliseer;

/// Lengte van een analyseframe in milliseconden.
const FRAME_MS: usize = 30;

/// Drempels van het filter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilterInstellingen {
    /// RMS van de hele opname onder deze waarde (0 tot 1) telt als stilte, ongeveer
    /// -70 dBFS. Dat vangt echte nullen en dithering, geen zachte ruis of fluisterspraak.
    pub stilte_rms: f32,
    /// Een frame van 30 ms telt als spraak vanaf deze RMS, ongeveer -40 dBFS. Gewone
    /// spraak ligt rond -30 tot -20 dBFS.
    pub spraak_rms: f32,
    /// Een herhaalde woordgroep wordt ingekort vanaf dit aantal keer achter elkaar.
    pub min_herhalingen: usize,
    /// Langste woordgroep (in woorden) die als herhaling wordt herkend.
    pub max_groep: usize,
    /// Marge in milliseconden rond een segment bij het meten van "stil eromheen".
    pub marge_ms: u64,
    /// Aandeel spraak onder deze fractie (met een lange tekst) maakt de uitkomst verdacht.
    pub min_aandeel_spraak: f32,
    /// Vanaf dit aantal woorden is een tekst "lang" voor de verdacht-regel.
    pub lange_tekst_woorden: usize,
    /// Haal stopwoorden (klanken als "ehm" en "uh") weg (ADR 0028). Standaard aan.
    pub stopwoorden: bool,
    /// De taal van de tekst als die bekend is. Bepaalt welke extra stopwoorden veilig zijn;
    /// zonder taal geldt alleen de vaste lijst.
    pub taal: Option<FilterTaal>,
}

/// De talen waarvoor de stopwoordenlijst iets extra's weet. Een andere taal geeft
/// [`FilterTaal::Andere`]: dan geldt alleen de vaste lijst.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterTaal {
    /// Nederlands.
    Nl,
    /// Engels.
    En,
    /// Een andere taal.
    Andere,
}

impl FilterTaal {
    /// De taal bij een taalcode als `nl`, `en` of `nl-NL` (hoofdletterongevoelig).
    #[must_use]
    pub fn uit_code(code: &str) -> Self {
        let basis = code.split(['-', '_']).next().unwrap_or("");
        if basis.eq_ignore_ascii_case("nl") {
            Self::Nl
        } else if basis.eq_ignore_ascii_case("en") {
            Self::En
        } else {
            Self::Andere
        }
    }
}

impl FilterInstellingen {
    /// Dezelfde instellingen met de taal van de tekst erbij.
    #[must_use]
    pub const fn met_taal(self, taal: Option<FilterTaal>) -> Self {
        Self { taal, ..self }
    }

    /// Dezelfde instellingen met stopwoorden weghalen aan of uit.
    #[must_use]
    pub const fn met_stopwoorden(self, aan: bool) -> Self {
        Self {
            stopwoorden: aan,
            ..self
        }
    }

    /// De vaste standaardwaarden.
    #[must_use]
    pub const fn standaard() -> Self {
        Self {
            stilte_rms: 0.000_3,
            spraak_rms: 0.01,
            min_herhalingen: 3,
            max_groep: 30,
            marge_ms: 500,
            min_aandeel_spraak: 0.10,
            lange_tekst_woorden: 10,
            stopwoorden: true,
            taal: None,
        }
    }
}

impl FilterInstellingen {
    /// Voor bestanden: zelfde regels, maar een veel lagere drempel voor "spraak" (0,002,
    /// ongeveer -54 dBFS in plaats van -40). Een vergadering of telefoongesprek is vaak veel
    /// zachter opgenomen dan een dictaat vlak bij de microfoon; met de dicteerdrempel kreeg
    /// zo'n opname altijd "bijna geen spraak", terwijl het transcript goed was. Digitale
    /// stilte blijft hetzelfde.
    #[must_use]
    pub const fn bestand() -> Self {
        Self {
            spraak_rms: 0.002,
            ..Self::standaard()
        }
    }
}

impl Default for FilterInstellingen {
    fn default() -> Self {
        Self::standaard()
    }
}

/// Zinnen die modellen bij stilte verzinnen, genormaliseerd (zie `meten::normaliseer`).
const STILTEZINNEN: &[&str] = &[
    "ondertiteling ingediend door de amara org gemeenschap",
    "ondertiteling door de amara org gemeenschap",
    "ondertitels ingediend door de amara org gemeenschap",
    "ondertiteld door de amara org gemeenschap",
    "bedankt voor het kijken",
    "abonneer op het kanaal",
    "abonneer je op het kanaal",
    "thank you for watching",
    "thanks for watching",
    "thank you",
];

/// Wat het filter over de uitkomst zegt. Serialiseert als `tekst`, `geen_geluid`,
/// `geen_tekst` of `verdacht`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Uitkomst {
    /// Gewone tekst; er is niets bijzonders gevonden.
    Tekst,
    /// De opname had vrijwel geen geluid; er is geen tekst.
    GeenGeluid,
    /// Er was wel geluid, maar het model herkende geen tekst (of er bleef niets over).
    GeenTekst,
    /// Zeer weinig spraak maar veel tekst: de tekst is waarschijnlijk deels verzonnen.
    Verdacht,
}

impl Uitkomst {
    /// Melding in gewone taal voor de gebruiker, of `None` bij gewone tekst.
    #[must_use]
    pub fn melding(self) -> Option<&'static str> {
        match self {
            Self::Tekst => None,
            Self::GeenGeluid => Some("Geen geluid gehoord."),
            Self::GeenTekst => Some("Er is geen tekst herkend."),
            Self::Verdacht => Some(
                "Er is bijna geen spraak in de opname, dus de tekst klopt waarschijnlijk niet.",
            ),
        }
    }
}

/// Een ingreep van het filter, voor het logboek en de meetresultaten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ingreep {
    /// Een herhaalde woordgroep is ingekort tot één keer.
    HerhalingIngekort {
        /// De woordgroep (zoals in de tekst geschreven).
        groep: String,
        /// Hoe vaak die direct achter elkaar stond.
        keer: usize,
    },
    /// Een segment zonder één woord (alleen tekens als `***` of `...`) is weggehaald.
    LeegSegmentVerwijderd {
        /// De weggehaalde tekst.
        tekst: String,
    },
    /// Een bekende stiltezin is weggehaald.
    StiltezinVerwijderd {
        /// De weggehaalde tekst.
        tekst: String,
    },
    /// Stopwoorden (klanken als "ehm") zijn weggehaald.
    StopwoordenVerwijderd {
        /// Hoeveel er weggingen.
        aantal: usize,
    },
}

/// Het resultaat van het filter.
#[derive(Debug, Clone, PartialEq)]
pub struct Gefilterd {
    /// De opgeschoonde tekst. Bij [`Uitkomst::GeenGeluid`] zonder segmenten.
    pub transcript: Transcript,
    /// Wat het filter ervan vindt.
    pub uitkomst: Uitkomst,
    /// Geschat aandeel spraak in de opname, 0 tot 1.
    pub aandeel_spraak: f32,
    /// RMS van de hele opname, 0 tot 1.
    pub rms: f32,
    /// Wat het filter heeft aangepast.
    pub ingrepen: Vec<Ingreep>,
}

/// Compacte meting van een opname: genoeg voor het filter, zonder de samples zelf.
///
/// Het werkproces maakt deze meting terwijl het de audio toch al inleest en stuurt haar
/// mee met het resultaat. Zo hoeft de app de audio niet zelf te decoderen (ADR 0009).
/// Per seconde audio zijn het ruim 33 getallen: een uur audio kost ongeveer 0,5 MB.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meting {
    /// RMS van de hele opname, 0 tot 1.
    pub rms: f32,
    /// RMS per frame van 30 ms (16 kHz mono). Het laatste frame kan korter zijn.
    pub frame_rms: Vec<f32>,
}

impl Meting {
    /// Meet audio van 16 kHz mono.
    #[must_use]
    pub fn van_samples(samples: &[f32]) -> Self {
        Self {
            rms: rms(samples),
            frame_rms: samples.chunks(frame_lengte()).map(rms).collect(),
        }
    }

    /// Een meting die stuk voor stuk wordt opgebouwd, voor een bestand dat in stukken
    /// binnenkomt. Geeft dezelfde uitkomst als [`Meting::van_samples`] op alles samen.
    #[must_use]
    pub fn opbouw() -> MetingOpbouw {
        MetingOpbouw::default()
    }

    /// Is de opname stil genoeg om geen tekst te verwachten?
    #[must_use]
    pub fn is_stil(&self, instellingen: &FilterInstellingen) -> bool {
        self.rms < instellingen.stilte_rms
    }

    /// Geschat aandeel van de opname dat spraak is: de fractie frames met een RMS vanaf
    /// `spraak_rms`. Een grove schatting: ruis boven die drempel telt mee.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn aandeel_spraak(&self, instellingen: &FilterInstellingen) -> f32 {
        if self.frame_rms.is_empty() {
            return 0.0;
        }
        let spraak = self
            .frame_rms
            .iter()
            .filter(|&&r| r >= instellingen.spraak_rms)
            .count();
        spraak as f32 / self.frame_rms.len() as f32
    }

    /// Is het stuk tussen `begin_ms` en `eind_ms` stil? Stil betekent hier: geen frame
    /// met spraakenergie. De frames die het stuk (deels) raken tellen mee.
    fn is_stil_tussen(
        &self,
        begin_ms: u64,
        eind_ms: u64,
        instellingen: &FilterInstellingen,
    ) -> bool {
        let frame_ms = FRAME_MS as u64;
        let begin = usize::try_from(begin_ms / frame_ms).unwrap_or(usize::MAX);
        let eind = usize::try_from(eind_ms.div_ceil(frame_ms)).unwrap_or(usize::MAX);
        let eind = eind.min(self.frame_rms.len());
        let begin = begin.min(eind);
        !self.frame_rms[begin..eind]
            .iter()
            .any(|&r| r >= instellingen.spraak_rms)
    }
}

/// Het transcript zoals het model het gaf, zonder ingrepen: voor wie het filter bewust
/// uitzet (`mondig-cli transcribe --geen-filter`). De uitkomst is altijd
/// [`Uitkomst::Tekst`]; de metingen staan er wel bij.
#[must_use]
pub fn zonder_filter(transcript: Transcript, meting: &Meting) -> Gefilterd {
    Gefilterd {
        transcript,
        uitkomst: Uitkomst::Tekst,
        aandeel_spraak: meting.aandeel_spraak(&FilterInstellingen::standaard()),
        rms: meting.rms,
        ingrepen: Vec::new(),
    }
}

/// Bouwt een [`Meting`] op uit stukken die na elkaar binnenkomen (zie [`Meting::opbouw`]).
/// Telt in dezelfde volgorde als [`rms`], dus de uitkomst is bit voor bit gelijk aan een
/// meting van alles in één keer.
#[derive(Debug, Clone, Default)]
pub struct MetingOpbouw {
    som: f64,
    aantal: usize,
    frame_som: f64,
    frame_aantal: usize,
    frame_rms: Vec<f32>,
}

impl MetingOpbouw {
    /// Telt de volgende samples (16 kHz mono) mee.
    pub fn voeg_toe(&mut self, samples: &[f32]) {
        let frame = frame_lengte();
        for &s in samples {
            let kwadraat = f64::from(s) * f64::from(s);
            self.som += kwadraat;
            self.frame_som += kwadraat;
            self.frame_aantal += 1;
            if self.frame_aantal == frame {
                self.frame_rms.push(wortel_gemiddeld(self.frame_som, frame));
                self.frame_som = 0.0;
                self.frame_aantal = 0;
            }
        }
        self.aantal += samples.len();
    }

    /// De meting van alles wat meegeteld is.
    #[must_use]
    pub fn klaar(mut self) -> Meting {
        if self.frame_aantal > 0 {
            self.frame_rms
                .push(wortel_gemiddeld(self.frame_som, self.frame_aantal));
        }
        Meting {
            rms: if self.aantal == 0 {
                0.0
            } else {
                wortel_gemiddeld(self.som, self.aantal)
            },
            frame_rms: self.frame_rms,
        }
    }
}

/// De wortel uit een som van kwadraten gedeeld door het aantal, zoals [`rms`] dat doet.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn wortel_gemiddeld(som: f64, aantal: usize) -> f32 {
    (som / aantal as f64).sqrt() as f32
}

/// Aantal samples in een analyseframe.
fn frame_lengte() -> usize {
    FRAME_MS * DOEL_SAMPLERATE as usize / 1000
}

/// Is de opname stil genoeg om geen tekst te verwachten?
#[must_use]
pub fn is_stil(samples: &[f32], instellingen: &FilterInstellingen) -> bool {
    rms(samples) < instellingen.stilte_rms
}

/// Gemiddeld vermogen (RMS) van een stuk audio. Lege audio geeft 0.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let som: f64 = samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    #[allow(clippy::cast_possible_truncation)]
    let uit = (som / samples.len() as f64).sqrt() as f32;
    uit
}

/// Geschat aandeel van de opname dat spraak is, gemeten op de samples zelf. Zie
/// [`Meting::aandeel_spraak`].
#[must_use]
pub fn aandeel_spraak(samples: &[f32], instellingen: &FilterInstellingen) -> f32 {
    Meting::van_samples(samples).aandeel_spraak(instellingen)
}

/// Past het filter toe op een transcript en de bijbehorende audio (16 kHz mono).
#[must_use]
pub fn filter(
    transcript: Transcript,
    samples: &[f32],
    instellingen: &FilterInstellingen,
) -> Gefilterd {
    pas_toe(transcript, &Meting::van_samples(samples), instellingen)
}

/// Past het filter toe op een transcript en de meting van de bijbehorende audio.
#[must_use]
pub fn pas_toe(
    transcript: Transcript,
    meting: &Meting,
    instellingen: &FilterInstellingen,
) -> Gefilterd {
    let rms_totaal = meting.rms;
    let aandeel = meting.aandeel_spraak(instellingen);

    if meting.is_stil(instellingen) {
        return Gefilterd {
            transcript: Transcript {
                segmenten: Vec::new(),
                audio_duur: transcript.audio_duur,
            },
            uitkomst: Uitkomst::GeenGeluid,
            aandeel_spraak: aandeel,
            rms: rms_totaal,
            ingrepen: Vec::new(),
        };
    }

    let mut ingrepen = Vec::new();
    let audio_duur = transcript.audio_duur;
    // Eerst de stopwoorden: wat daarna leeg is of alleen leestekens heeft, valt als leeg weg.
    let mut segmenten = haal_stopwoorden_weg(transcript.segmenten, instellingen, &mut ingrepen);
    segmenten = haal_lege_segmenten_weg(segmenten, &mut ingrepen);
    segmenten = kort_herhalingen_in(segmenten, instellingen, &mut ingrepen);
    segmenten = haal_stiltezinnen_weg(segmenten, meting, instellingen, &mut ingrepen);

    let transcript = Transcript {
        segmenten,
        audio_duur,
    };
    let tekst = transcript.tekst();
    let woorden = normaliseer(&tekst)
        .split(' ')
        .filter(|w| !w.is_empty())
        .count();
    let uitkomst = if woorden == 0 {
        Uitkomst::GeenTekst
    } else if aandeel < instellingen.min_aandeel_spraak
        && woorden >= instellingen.lange_tekst_woorden
    {
        Uitkomst::Verdacht
    } else {
        Uitkomst::Tekst
    };
    Gefilterd {
        transcript,
        uitkomst,
        aandeel_spraak: aandeel,
        rms: rms_totaal,
        ingrepen,
    }
}

/// Haalt segmenten weg die na normalisatie geen woord bevatten, zoals `***` dat Whisper
/// bij stilte soms geeft. Een echt leeg segment (alleen spaties) gaat zonder melding weg.
fn haal_lege_segmenten_weg(segmenten: Vec<Segment>, ingrepen: &mut Vec<Ingreep>) -> Vec<Segment> {
    segmenten
        .into_iter()
        .filter(|s| {
            let leeg = normaliseer(&s.tekst).is_empty();
            if leeg && !s.tekst.trim().is_empty() {
                ingrepen.push(Ingreep::LeegSegmentVerwijderd {
                    tekst: s.tekst.clone(),
                });
            }
            !leeg
        })
        .collect()
}

/// Een woord met het nummer van het segment waar het vandaan komt.
struct Woord {
    segment: usize,
    tekst: String,
    sleutel: String,
}

/// Kort woordgroepen in die `min_herhalingen` keer of vaker direct herhaald worden.
/// Werkt over segmentgrenzen heen, want een lus levert vaak een herhaald segment op.
fn kort_herhalingen_in(
    segmenten: Vec<Segment>,
    instellingen: &FilterInstellingen,
    ingrepen: &mut Vec<Ingreep>,
) -> Vec<Segment> {
    let woorden: Vec<Woord> = segmenten
        .iter()
        .enumerate()
        .flat_map(|(segment, s)| {
            s.tekst.split_whitespace().map(move |w| Woord {
                segment,
                tekst: w.to_string(),
                sleutel: normaliseer(w),
            })
        })
        .collect();

    let min = instellingen.min_herhalingen.max(2);
    let mut behouden = vec![true; woorden.len()];
    let mut i = 0;
    let mut ingekort = false;
    while i < woorden.len() {
        let mut gevonden = None;
        for k in 1..=instellingen.max_groep.min((woorden.len() - i) / min) {
            let mut keer = 1;
            while i + (keer + 1) * k <= woorden.len()
                && (0..k).all(|d| woorden[i + d].sleutel == woorden[i + keer * k + d].sleutel)
            {
                keer += 1;
            }
            if keer >= min {
                gevonden = Some((k, keer));
                break;
            }
        }
        if let Some((k, keer)) = gevonden {
            for flag in &mut behouden[i + k..i + keer * k] {
                *flag = false;
            }
            ingrepen.push(Ingreep::HerhalingIngekort {
                groep: woorden[i..i + k]
                    .iter()
                    .map(|w| w.tekst.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                keer,
            });
            ingekort = true;
            i += keer * k;
        } else {
            i += 1;
        }
    }
    if !ingekort {
        return segmenten;
    }

    // Segmenten opnieuw opbouwen uit de woorden die overbleven; lege segmenten vervallen.
    let mut nieuw = Vec::new();
    for (nr, segment) in segmenten.into_iter().enumerate() {
        let rest: Vec<&str> = woorden
            .iter()
            .zip(&behouden)
            .filter(|(w, b)| w.segment == nr && **b)
            .map(|(w, _)| w.tekst.as_str())
            .collect();
        if !rest.is_empty() {
            // Is er in dit segment iets weggevallen, dan passen de woordtijden niet meer bij
            // de tekst; zonder woordtijden koppelt de sprekerherkenning het hele segment.
            let ingekort = rest.len() != segment.tekst.split_whitespace().count();
            nieuw.push(Segment {
                tekst: rest.join(" "),
                woorden: if ingekort {
                    Vec::new()
                } else {
                    segment.woorden
                },
                ..segment
            });
        }
    }
    nieuw
}

/// Klanken die in geen enkele taal een echt woord zijn (ADR 0028).
const STOPWOORDEN_ALTIJD: &[&str] = &["uh", "uhm", "uhh", "ehm", "ehh", "ahm", "hm", "hmm", "mmm"];
/// Extra klanken die alleen bij Nederlands veilig zijn.
const STOPWOORDEN_NL: &[&str] = &["eh", "euh"];
/// Extra klanken die alleen bij Engels veilig zijn.
const STOPWOORDEN_EN: &[&str] = &["um", "ah", "eh"];
/// Leestekens die een zin afsluiten.
const ZINSLUITERS: [char; 4] = ['.', '!', '?', '\u{2026}'];

/// Is dit woord een stopwoord voor deze taal? Dan komt het leesteken dat een zin afsluit (punt,
/// vraagteken, uitroepteken, beletselteken) terug; een komma erachter telt niet mee en gaat
/// dus gewoon weg. Een woord in hoofdletters ("EH", "UH") is een afkorting en blijft staan.
fn stopwoord_eind(woord: &str, taal: Option<FilterTaal>) -> Option<String> {
    let kern = woord.trim_end_matches([',', '.', '!', '?', '\u{2026}']);
    if kern.is_empty() || kern.chars().all(char::is_uppercase) {
        return None;
    }
    let kern_klein = kern.to_lowercase();
    let extra: &[&str] = match taal {
        Some(FilterTaal::Nl) => STOPWOORDEN_NL,
        Some(FilterTaal::En) => STOPWOORDEN_EN,
        Some(FilterTaal::Andere) | None => &[],
    };
    if !STOPWOORDEN_ALTIJD.contains(&kern_klein.as_str()) && !extra.contains(&kern_klein.as_str()) {
        return None;
    }
    Some(woord[kern.len()..].chars().filter(|&c| c != ',').collect())
}

/// Sluit dit woord een zin af?
fn sluit_zin_af(woord: &str) -> bool {
    woord
        .trim_end_matches(['"', '\'', ')', '\u{201d}', '\u{2019}', '\u{bb}'])
        .ends_with(ZINSLUITERS)
}

fn heeft_letter(woord: &str) -> bool {
    woord.chars().any(char::is_alphabetic)
}

/// Een getal ("3", "3,5"): cijfers en leestekens, geen letters.
fn is_getal(woord: &str) -> bool {
    woord.chars().any(|c| c.is_ascii_digit()) && !heeft_letter(woord)
}

/// Mag dit woord een hoofdletter krijgen? Alleen als het met een kleine letter begint en verder
/// geen hoofdletters heeft: "3e", "'s-Hertogenbosch" en "iPhone" blijven zoals ze zijn.
fn hoofdletter_mogelijk(woord: &str) -> bool {
    let mut tekens = woord.chars();
    tekens.next().is_some_and(char::is_lowercase) && !tekens.any(char::is_uppercase)
}

/// Zet de eerste letter van het woord in hoofdletter, met "ij" als "IJ" (zoals bij een rivier).
fn met_hoofdletter(woord: &str) -> String {
    let mut tekens = woord.chars().peekable();
    let mut uit = String::with_capacity(woord.len());
    if let Some(eerste) = tekens.next() {
        if eerste == 'i' && tekens.peek() == Some(&'j') {
            uit.push_str("IJ");
            tekens.next();
        } else {
            uit.extend(eerste.to_uppercase());
        }
    }
    uit.extend(tekens);
    uit
}

/// Een woord van een segment tijdens het weghalen van stopwoorden.
struct Stoptoken {
    segment: usize,
    tekst: String,
    /// De woordtijd van dit woord, als het segment woordtijden heeft die bij de tekst passen.
    tijd: Option<Woordtijd>,
    weg: bool,
    veranderd: bool,
}

impl Stoptoken {
    /// Geeft het woord een nieuwe tekst (en zijn woordtijd mee).
    fn zet_tekst(&mut self, tekst: String) {
        if tekst != self.tekst {
            if let Some(tijd) = &mut self.tijd {
                tijd.tekst.clone_from(&tekst);
            }
            self.tekst = tekst;
            self.veranderd = true;
        }
    }
}

/// Haalt stopwoorden weg met vaste regels (ADR 0028). Werkwijze naar Handy
/// (`audio_toolkit/text.rs`, MIT), in Mondig-stijl herschreven.
///
/// Werkt over alle segmenten heen, zodat het begin van een zin ook over een segmentgrens klopt;
/// een segment dat leeg wordt, vervalt. Een komma direct achter het stopwoord gaat mee; een
/// zinsluitend leesteken blijft staan en sluit de zin ervoor af. Opende het stopwoord een zin,
/// dan krijgt het volgende woord de hoofdletter (als het met een kleine letter begint) en
/// verdwijnt een los komma erachter. Een woord in hoofdletters of direct na een getal blijft
/// staan. Van een segment dat verandert, blijven de woordtijden van de overgebleven woorden.
fn haal_stopwoorden_weg(
    segmenten: Vec<Segment>,
    instellingen: &FilterInstellingen,
    ingrepen: &mut Vec<Ingreep>,
) -> Vec<Segment> {
    if !instellingen.stopwoorden {
        return segmenten;
    }
    let mut tokens: Vec<Stoptoken> = Vec::new();
    let mut tijden_kloppen = Vec::with_capacity(segmenten.len());
    for (nr, segment) in segmenten.iter().enumerate() {
        let delen: Vec<&str> = segment.tekst.split_whitespace().collect();
        // De woordtijden horen bij de tekst als er per woord precies één staat.
        let klopt = !segment.woorden.is_empty()
            && segment.woorden.len() == delen.len()
            && delen
                .iter()
                .zip(&segment.woorden)
                .all(|(deel, w)| w.tekst.trim() == *deel);
        tijden_kloppen.push(klopt);
        for (i, deel) in delen.iter().enumerate() {
            tokens.push(Stoptoken {
                segment: nr,
                tekst: (*deel).to_string(),
                tijd: klopt.then(|| segment.woorden[i].clone()),
                weg: false,
                veranderd: false,
            });
        }
    }

    let mut weg = 0;
    let mut begin_zin = true;
    // Het volgende woord met letters krijgt een hoofdletter (een zin begint na een stopwoord).
    let mut hoofdletter = false;
    // Direct na een stopwoord dat de zin opende: een los komma mag niet blijven staan.
    let mut komma_weg = false;
    // Het laatste woord dat blijft staan.
    let mut vorige: Option<usize> = None;
    for i in 0..tokens.len() {
        let na_getal = vorige.is_some_and(|v| is_getal(&tokens[v].tekst));
        if !na_getal && let Some(eind) = stopwoord_eind(&tokens[i].tekst, instellingen.taal) {
            tokens[i].weg = true;
            weg += 1;
            if eind.is_empty() {
                if begin_zin {
                    hoofdletter = true;
                    komma_weg = true;
                }
            } else {
                // Het leesteken sluit de zin ervoor af; een komma ervoor valt weg.
                if let Some(v) = vorige {
                    let kaal = tokens[v].tekst.trim_end_matches(',').to_string();
                    let nieuw = if sluit_zin_af(&kaal) {
                        kaal
                    } else {
                        format!("{kaal}{eind}")
                    };
                    tokens[v].zet_tekst(nieuw);
                }
                begin_zin = true;
                hoofdletter = true;
                komma_weg = false;
            }
            continue;
        }

        let mut tekst = tokens[i].tekst.clone();
        if komma_weg {
            tekst = tekst.trim_start_matches(',').to_string();
            if tekst.is_empty() {
                tokens[i].weg = true;
                continue;
            }
        }
        if heeft_letter(&tekst) {
            komma_weg = false;
            if hoofdletter {
                hoofdletter = false;
                if hoofdletter_mogelijk(&tekst) {
                    tekst = met_hoofdletter(&tekst);
                }
            }
        }
        tokens[i].zet_tekst(tekst);
        // Een woord zonder letters ("-") verandert niets aan waar de zin staat, behalve een
        // beletselteken of punt.
        if heeft_letter(&tokens[i].tekst) || sluit_zin_af(&tokens[i].tekst) {
            begin_zin = sluit_zin_af(&tokens[i].tekst);
        }
        vorige = Some(i);
    }

    if tokens.iter().all(|t| !t.weg && !t.veranderd) {
        return segmenten;
    }
    if weg > 0 {
        ingrepen.push(Ingreep::StopwoordenVerwijderd { aantal: weg });
    }
    bouw_segmenten_op(segmenten, tokens, &tijden_kloppen)
}

/// Bouwt de segmenten opnieuw op uit de woorden die overbleven. Een segment zonder
/// verandering blijft precies zoals het was; een leeg geworden segment vervalt.
fn bouw_segmenten_op(
    segmenten: Vec<Segment>,
    tokens: Vec<Stoptoken>,
    tijden_kloppen: &[bool],
) -> Vec<Segment> {
    let mut over = Vec::with_capacity(segmenten.len());
    let mut tokens = tokens.into_iter().peekable();
    for (nr, segment) in segmenten.into_iter().enumerate() {
        let mut mijn = Vec::new();
        while let Some(token) = tokens.next_if(|t| t.segment == nr) {
            mijn.push(token);
        }
        if mijn.iter().all(|t| !t.weg && !t.veranderd) {
            // Niets aangeraakt: de tekst blijft precies zoals hij was.
            over.push(segment);
            continue;
        }
        let rest: Vec<&Stoptoken> = mijn.iter().filter(|t| !t.weg).collect();
        if rest.is_empty() {
            continue;
        }
        let woorden = if tijden_kloppen[nr] {
            rest.iter().filter_map(|t| t.tijd.clone()).collect()
        } else {
            // Zonder woordtijden die bij de tekst passen, kunnen ze niet kloppen (zie
            // `kort_herhalingen_in`).
            Vec::new()
        };
        over.push(Segment {
            tekst: rest
                .iter()
                .map(|t| t.tekst.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            woorden,
            ..segment
        });
    }
    over
}

/// Haalt de bekende stiltezinnen weg, alleen als de hele uitvoer ervan is of als de
/// audio rond het segment stil is.
fn haal_stiltezinnen_weg(
    segmenten: Vec<Segment>,
    meting: &Meting,
    instellingen: &FilterInstellingen,
    ingrepen: &mut Vec<Ingreep>,
) -> Vec<Segment> {
    let hele_tekst = segmenten
        .iter()
        .map(|s| s.tekst.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if !segmenten.is_empty() && bestaat_uit_stiltezinnen(&hele_tekst) {
        ingrepen.push(Ingreep::StiltezinVerwijderd { tekst: hele_tekst });
        return Vec::new();
    }

    let mut over = Vec::with_capacity(segmenten.len());
    for segment in segmenten {
        let is_zin = bestaat_uit_stiltezinnen(&segment.tekst);
        if is_zin && rond_segment_stil(meting, &segment, instellingen) {
            ingrepen.push(Ingreep::StiltezinVerwijderd {
                tekst: segment.tekst,
            });
        } else {
            over.push(segment);
        }
    }
    over
}

/// Bestaat deze tekst (genormaliseerd) helemaal uit een of meer stiltezinnen?
fn bestaat_uit_stiltezinnen(tekst: &str) -> bool {
    let mut rest = normaliseer(tekst);
    if rest.is_empty() {
        return false;
    }
    // Langste zinnen eerst, zodat "thank you for watching" niet als "thank you" begint.
    let mut zinnen: Vec<&str> = STILTEZINNEN.to_vec();
    zinnen.sort_by_key(|z| std::cmp::Reverse(z.len()));
    'buiten: while !rest.is_empty() {
        for zin in &zinnen {
            if let Some(na) = rest.strip_prefix(zin)
                && (na.is_empty() || na.starts_with(' '))
            {
                rest = na.trim_start().to_string();
                continue 'buiten;
            }
        }
        return false;
    }
    true
}

/// Is het stuk audio van het segment, plus een marge ervoor en erna, stil?
fn rond_segment_stil(
    meting: &Meting,
    segment: &Segment,
    instellingen: &FilterInstellingen,
) -> bool {
    let begin = segment.start_ms.saturating_sub(instellingen.marge_ms);
    let eind = segment.eind_ms.saturating_add(instellingen.marge_ms);
    meting.is_stil_tussen(begin, eind, instellingen)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const SR: usize = DOEL_SAMPLERATE as usize;

    fn stilte(seconden: usize) -> Vec<f32> {
        vec![0.0; SR * seconden]
    }

    #[allow(clippy::cast_precision_loss)]
    /// Sinus op 220 Hz met de opgegeven amplitude; RMS is amplitude / wortel 2.
    fn toon(seconden: usize, amplitude: f32) -> Vec<f32> {
        (0..SR * seconden)
            .map(|n| amplitude * (2.0 * std::f32::consts::PI * 220.0 * n as f32 / SR as f32).sin())
            .collect()
    }

    /// Zachte pseudo-ruis (eenvoudige LCG, geen afhankelijkheid) rond de opgegeven amplitude.
    fn ruis(seconden: usize, amplitude: f32) -> Vec<f32> {
        let mut staat: u32 = 12345;
        (0..SR * seconden)
            .map(|_| {
                staat = staat.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (f32::from((staat >> 16) as u16) / 32768.0 - 1.0) * amplitude
            })
            .collect()
    }

    fn transcript(delen: &[(u64, u64, &str)], seconden: u64) -> Transcript {
        Transcript {
            segmenten: delen
                .iter()
                .map(|&(start_ms, eind_ms, tekst)| Segment::nieuw(start_ms, eind_ms, tekst))
                .collect(),
            audio_duur: Duration::from_secs(seconden),
        }
    }

    fn standaard() -> FilterInstellingen {
        FilterInstellingen::standaard()
    }

    #[test]
    fn digitale_stilte_geeft_geen_geluid_en_geen_tekst() {
        let t = transcript(
            &[(
                0,
                5000,
                "Ondertiteling ingediend door de Amara.org gemeenschap",
            )],
            10,
        );
        let uit = filter(t, &stilte(10), &standaard());
        assert_eq!(uit.uitkomst, Uitkomst::GeenGeluid);
        assert!(uit.transcript.segmenten.is_empty());
        assert_eq!(uit.uitkomst.melding(), Some("Geen geluid gehoord."));
    }

    #[test]
    fn lege_audio_is_geen_geluid() {
        let uit = filter(transcript(&[], 0), &[], &standaard());
        assert_eq!(uit.uitkomst, Uitkomst::GeenGeluid);
    }

    #[test]
    fn is_stil_ziet_nullen_maar_geen_spraak() {
        assert!(is_stil(&stilte(2), &standaard()));
        assert!(!is_stil(&toon(2, 0.1), &standaard()));
    }

    #[test]
    fn gewone_tekst_blijft_ongewijzigd() {
        let t = transcript(
            &[
                (0, 3000, "Dit is een gewone zin."),
                (3000, 6000, "En nog een."),
            ],
            6,
        );
        let uit = filter(t.clone(), &toon(6, 0.2), &standaard());
        assert_eq!(uit.uitkomst, Uitkomst::Tekst);
        assert_eq!(uit.transcript, t);
        assert!(uit.ingrepen.is_empty());
        assert!(uit.aandeel_spraak > 0.9);
    }

    #[test]
    fn herhaling_van_drie_wordt_een() {
        let t = transcript(&[(0, 6000, "Dank u wel. Nee nee nee nee goed.")], 6);
        let uit = filter(t, &toon(6, 0.2), &standaard());
        assert_eq!(uit.transcript.tekst(), "Dank u wel. Nee goed.");
        assert_eq!(
            uit.ingrepen,
            vec![Ingreep::HerhalingIngekort {
                groep: "Nee".into(),
                keer: 4
            }]
        );
    }

    #[test]
    fn herhaling_van_twee_blijft() {
        let t = transcript(&[(0, 2000, "Nee nee dat klopt.")], 2);
        let uit = filter(t.clone(), &toon(2, 0.2), &standaard());
        assert_eq!(uit.transcript, t);
    }

    #[test]
    fn herhaalde_woordgroep_wordt_ingekort() {
        let t = transcript(
            &[(
                0,
                8000,
                "Dat is goed. Ik weet het niet, ik weet het niet, ik weet het niet, ik weet het niet. Klaar.",
            )],
            8,
        );
        let uit = filter(t, &toon(8, 0.2), &standaard());
        assert_eq!(
            uit.transcript.tekst(),
            "Dat is goed. Ik weet het niet, Klaar."
        );
    }

    #[test]
    fn herhaling_over_segmentgrenzen_heen() {
        let t = transcript(
            &[
                (0, 1000, "Hallo daar."),
                (1000, 2000, "Tot ziens."),
                (2000, 3000, "Tot ziens."),
                (3000, 4000, "Tot ziens."),
            ],
            4,
        );
        let uit = filter(t, &toon(4, 0.2), &standaard());
        assert_eq!(uit.transcript.segmenten.len(), 2);
        assert_eq!(uit.transcript.tekst(), "Hallo daar. Tot ziens.");
    }

    #[test]
    fn hele_uitvoer_stiltezin_gaat_weg_ook_bij_geluid() {
        let t = transcript(&[(0, 4000, "Thank you.")], 4);
        let uit = filter(t, &toon(4, 0.2), &standaard());
        assert!(uit.transcript.segmenten.is_empty());
        assert_eq!(uit.uitkomst, Uitkomst::GeenTekst);
    }

    #[test]
    fn herhaalde_stiltezin_gaat_weg() {
        let t = transcript(
            &[(0, 4000, "Thank you. Thank you. Thank you. Thank you.")],
            4,
        );
        let uit = filter(t, &ruis(4, 0.001), &standaard());
        assert!(uit.transcript.segmenten.is_empty());
    }

    #[test]
    fn stiltezin_midden_in_spraak_blijft() {
        let t = transcript(
            &[
                (0, 3000, "Ik zei thank you."),
                (3000, 6000, "Dank u wel voor alles."),
            ],
            6,
        );
        let uit = filter(t.clone(), &toon(6, 0.2), &standaard());
        assert_eq!(uit.transcript, t);
    }

    #[test]
    fn losse_stiltezin_in_luide_audio_blijft() {
        // Het segment "Thank you." is zelf geen hele uitvoer en de audio is luid.
        let t = transcript(
            &[
                (0, 3000, "Wat een mooi verhaal."),
                (3000, 4000, "Thank you."),
            ],
            4,
        );
        let uit = filter(t.clone(), &toon(4, 0.2), &standaard());
        assert_eq!(uit.transcript, t);
    }

    #[test]
    fn losse_stiltezin_in_stille_staart_gaat_weg() {
        // Drie seconden spraak, dan twee seconden bijna stil met een verzonnen zin.
        let mut audio = toon(3, 0.2);
        audio.extend(ruis(3, 0.001));
        let t = transcript(
            &[
                (0, 3000, "Wat een mooi verhaal."),
                (4500, 6000, "Thank you."),
            ],
            6,
        );
        let uit = filter(t, &audio, &standaard());
        assert_eq!(uit.transcript.tekst(), "Wat een mooi verhaal.");
        assert_eq!(uit.ingrepen.len(), 1);
    }

    #[test]
    fn veel_tekst_in_bijna_stille_audio_is_verdacht() {
        let tekst = "dit klinkt als een heel verhaal maar er is helemaal niemand aan het woord";
        let uit = filter(
            transcript(&[(0, 5000, tekst)], 5),
            &ruis(5, 0.001),
            &standaard(),
        );
        assert_eq!(uit.uitkomst, Uitkomst::Verdacht);
        assert_eq!(
            uit.transcript.tekst(),
            tekst,
            "verdachte tekst wordt niet weggegooid"
        );
        assert!(uit.aandeel_spraak < 0.01);
    }

    #[test]
    fn korte_tekst_in_bijna_stille_audio_is_niet_verdacht() {
        let uit = filter(
            transcript(&[(0, 2000, "Ja hoor.")], 2),
            &ruis(2, 0.001),
            &standaard(),
        );
        assert_eq!(uit.uitkomst, Uitkomst::Tekst);
    }

    #[test]
    fn segment_zonder_woorden_gaat_weg() {
        let t = transcript(&[(0, 3000, "***")], 3);
        let uit = filter(t, &ruis(3, 0.001), &standaard());
        assert!(uit.transcript.segmenten.is_empty());
        assert_eq!(uit.uitkomst, Uitkomst::GeenTekst);
        assert_eq!(uit.ingrepen.len(), 1);
    }

    #[test]
    fn geluid_zonder_tekst_is_geen_tekst() {
        let uit = filter(transcript(&[], 3), &toon(3, 0.2), &standaard());
        assert_eq!(uit.uitkomst, Uitkomst::GeenTekst);
        assert_eq!(uit.uitkomst.melding(), Some("Er is geen tekst herkend."));
    }

    #[test]
    fn aandeel_spraak_halve_opname() {
        let mut audio = toon(2, 0.2);
        audio.extend(stilte(2));
        let aandeel = aandeel_spraak(&audio, &standaard());
        assert!((aandeel - 0.5).abs() < 0.02, "aandeel was {aandeel}");
    }

    #[test]
    fn filter_op_een_meting_geeft_hetzelfde_als_filter_op_samples() {
        let mut audio = toon(2, 0.2);
        audio.extend(stilte(4));
        let t = transcript(
            &[
                (0, 1500, "Dit is gewoon een zin."),
                (4000, 5500, "Bedankt voor het kijken"),
            ],
            6,
        );
        let via_samples = filter(t.clone(), &audio, &standaard());
        let via_meting = pas_toe(t, &Meting::van_samples(&audio), &standaard());
        assert_eq!(via_samples, via_meting);
        assert_eq!(
            via_meting.ingrepen.len(),
            1,
            "de stiltezin in de staart gaat weg"
        );
    }

    #[test]
    fn een_zachte_opname_rond_min_45_dbfs_is_bij_bestanden_gewoon_tekst() {
        // Spraakachtig: steeds 0,6 s toon en 0,2 s pauze, met een RMS van ongeveer 0,0056
        // (-45 dBFS) in de toon.
        let mut audio = Vec::new();
        for _ in 0..10 {
            audio.extend(toon_met_rms(600, 0.005_6));
            audio.extend(vec![0.0; 16 * 200]);
        }
        let tekst = "dit is een zachte vergaderopname met gewoon een goed transcript erin";
        let transcript = || Transcript {
            segmenten: vec![Segment::nieuw(0, 8000, tekst)],
            audio_duur: Duration::from_secs(8),
        };
        let meting = Meting::van_samples(&audio);
        // Met de dicteerdrempel lijkt het bijna geen spraak.
        assert_eq!(
            pas_toe(transcript(), &meting, &FilterInstellingen::standaard()).uitkomst,
            Uitkomst::Verdacht
        );
        // Met de drempel voor bestanden is het gewoon tekst.
        let bestand = pas_toe(transcript(), &meting, &FilterInstellingen::bestand());
        assert_eq!(bestand.uitkomst, Uitkomst::Tekst);
        assert_eq!(bestand.transcript.tekst(), tekst);
    }

    /// Een toon van zoveel milliseconden met deze RMS (16 kHz).
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "testsignaal"
    )]
    fn toon_met_rms(ms: usize, rms_waarde: f32) -> Vec<f32> {
        let piek = f64::from(rms_waarde) * std::f64::consts::SQRT_2;
        (0..ms * 16)
            .map(|i| {
                ((i as f64 * 2.0 * std::f64::consts::PI * 200.0 / 16_000.0).sin() * piek) as f32
            })
            .collect()
    }

    #[test]
    fn meting_in_stukken_is_gelijk_aan_meting_in_een_keer() {
        let mut audio = toon(2, 0.2);
        audio.extend(stilte(1));
        audio.extend(toon(1, 0.05));
        audio.truncate(audio.len() - 123); // laatste frame korter dan 30 ms
        let mut opbouw = Meting::opbouw();
        let mut rest = &audio[..];
        for grootte in [1, 479, 480, 481, 16_000, 7].iter().cycle() {
            if rest.is_empty() {
                break;
            }
            let (stuk, over) = rest.split_at((*grootte).min(rest.len()));
            opbouw.voeg_toe(stuk);
            rest = over;
        }
        let in_stukken = opbouw.klaar();
        let in_een_keer = Meting::van_samples(&audio);
        assert_eq!(in_stukken.rms.to_bits(), in_een_keer.rms.to_bits());
        assert_eq!(in_stukken.frame_rms.len(), in_een_keer.frame_rms.len());
        assert!(
            in_stukken
                .frame_rms
                .iter()
                .zip(&in_een_keer.frame_rms)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
        assert_eq!(Meting::opbouw().klaar(), Meting::van_samples(&[]));
    }

    #[test]
    fn meting_heeft_een_frame_per_30_ms() {
        let meting = Meting::van_samples(&toon(1, 0.2));
        // 1 s = 33 volle frames van 30 ms plus een rest van 10 ms.
        assert_eq!(meting.frame_rms.len(), 34);
        assert!((meting.rms - 0.2 / 2.0_f32.sqrt()).abs() < 0.001);
    }

    #[test]
    fn rms_van_sinus_en_stilte() {
        assert!(rms(&stilte(1)).abs() < f32::EPSILON);
        assert!((rms(&toon(1, 0.5)) - 0.5 / 2.0_f32.sqrt()).abs() < 0.001);
        assert!(rms(&[]).abs() < f32::EPSILON);
    }

    #[test]
    fn stiltezinnen_staan_genormaliseerd_in_de_lijst() {
        for zin in STILTEZINNEN {
            assert_eq!(normaliseer(zin), *zin);
        }
    }

    // --- Stopwoorden (ADR 0028) ---

    /// Laat één tekst door het filter (met spraak in de audio) en geeft de tekst terug.
    fn stop(tekst: &str, taal: Option<FilterTaal>) -> String {
        stop_segmenten(&[(0, 4000, tekst)], taal)
    }

    fn stop_segmenten(delen: &[(u64, u64, &str)], taal: Option<FilterTaal>) -> String {
        let t = transcript(delen, 4);
        let instellingen = standaard().met_taal(taal);
        filter(t, &toon(4, 0.2), &instellingen).transcript.tekst()
    }

    #[test]
    fn de_vaste_lijst_gaat_weg_in_elke_taal() {
        for woord in [
            "uh", "uhm", "uhh", "ehm", "ehh", "ahm", "hm", "hmm", "mmm", "Uhm", "Ehm",
        ] {
            let tekst = format!("ik denk {woord} dat het goed is");
            for taal in [None, Some(FilterTaal::Nl), Some(FilterTaal::En)] {
                assert_eq!(stop(&tekst, taal), "ik denk dat het goed is", "{woord}");
            }
        }
    }

    #[test]
    fn euh_gaat_alleen_weg_bij_nederlands_en_eh_ook_bij_engels() {
        let tekst = "ik denk eh dat euh het goed is";
        assert_eq!(stop(tekst, Some(FilterTaal::Nl)), "ik denk dat het goed is");
        // "eh" hoort ook bij Engels, "euh" niet.
        assert_eq!(
            stop(tekst, Some(FilterTaal::En)),
            "ik denk dat euh het goed is"
        );
        assert_eq!(stop(tekst, None), tekst);
        assert_eq!(stop(tekst, Some(FilterTaal::Andere)), tekst);
    }

    #[test]
    fn um_en_ah_gaan_alleen_weg_bij_engels() {
        let tekst = "well um I think ah it works";
        assert_eq!(stop(tekst, Some(FilterTaal::En)), "well I think it works");
        assert_eq!(stop(tekst, Some(FilterTaal::Nl)), tekst);
        assert_eq!(stop(tekst, None), tekst);
        // "eh" hoort bij Engels en Nederlands.
        assert_eq!(stop("so eh yes", Some(FilterTaal::En)), "so yes");
    }

    #[test]
    fn echte_woorden_en_eenheden_blijven_staan() {
        let tekst = "Hè, nou zeg, dus eigenlijk maar 5 mm en het hmmm-effect";
        for taal in [None, Some(FilterTaal::Nl), Some(FilterTaal::En)] {
            assert_eq!(stop(tekst, taal), tekst);
        }
        // Alleen hele woorden: "ehm" in een ander woord blijft staan.
        assert_eq!(stop("de ehmer en de uhmen", None), "de ehmer en de uhmen");
    }

    #[test]
    fn een_woord_in_hoofdletters_of_na_een_getal_blijft_staan() {
        let tekst = "de EH en de HM en UH zeggen 3 hm en 3,5 uh";
        assert_eq!(stop(tekst, Some(FilterTaal::Nl)), tekst);
        // Een stopwoord na iets anders dan een getal gaat gewoon weg.
        assert_eq!(stop("3e hm keer", None), "3e keer");
    }

    #[test]
    fn komma_of_punt_direct_erna_gaat_mee() {
        assert_eq!(
            stop("ik denk, uh, dat het klopt", None),
            "ik denk, dat het klopt"
        );
        assert_eq!(stop("ik denk ehm, dat klopt", None), "ik denk dat klopt");
    }

    #[test]
    fn een_zinsluitend_teken_na_het_stopwoord_blijft_en_sluit_de_zin_ervoor_af() {
        assert_eq!(
            stop("Dat klopt uh. Nu verder", None),
            "Dat klopt. Nu verder"
        );
        assert_eq!(stop("Dat is alles, ehm.", None), "Dat is alles.");
        assert_eq!(stop("Is dat zo uh? Ja", None), "Is dat zo? Ja");
        assert_eq!(stop("Dat is alles uh...", None), "Dat is alles...");
        assert_eq!(stop("Echt waar uh! Ja", None), "Echt waar! Ja");
        // Staat er al een leesteken, dan komt er niet nog een bij.
        assert_eq!(
            stop("Dat klopt. Hmm. Nu verder", None),
            "Dat klopt. Nu verder"
        );
        // Een zin die met alleen een stopwoord en een punt begint, laat niets achter.
        assert_eq!(stop("Uh. Nu verder", None), "Nu verder");
        // Ook over een segmentgrens heen.
        assert_eq!(
            stop_segmenten(
                &[(0, 2000, "Dat klopt"), (2000, 4000, "uh. nu verder")],
                None
            ),
            "Dat klopt. Nu verder"
        );
    }

    #[test]
    fn een_hoofdletter_alleen_als_het_woord_met_een_kleine_letter_begint() {
        assert_eq!(stop("Uh 3e keer", None), "3e keer");
        assert_eq!(stop("Uh 's-Hertogenbosch", None), "'s-Hertogenbosch");
        assert_eq!(stop("Uh iPhone", None), "iPhone");
        assert_eq!(stop("Uh Jan komt", None), "Jan komt");
    }

    #[test]
    fn een_woord_zonder_letters_verbruikt_de_hoofdletter_niet() {
        assert_eq!(stop("Uh - ik denk het", None), "- Ik denk het");
        assert_eq!(stop("Uh ... ik denk het", None), "... Ik denk het");
    }

    #[test]
    fn een_segment_met_alleen_leestekens_telt_als_leeg() {
        let t = transcript(
            &[
                (0, 1000, "Uh ..."),
                (1000, 2000, "-"),
                (2000, 4000, "Hallo"),
            ],
            4,
        );
        let uit = filter(t, &toon(4, 0.2), &standaard());
        assert_eq!(uit.transcript.segmenten.len(), 1);
        assert_eq!(uit.transcript.tekst(), "Hallo");
    }

    #[test]
    fn dubbele_spaties_worden_een() {
        assert_eq!(stop("ik   denk  uh   dat   het", None), "ik denk dat het");
    }

    #[test]
    fn opende_het_stopwoord_de_zin_dan_krijgt_het_volgende_woord_een_hoofdletter() {
        assert_eq!(stop("Uh, ik denk het", None), "Ik denk het");
        assert_eq!(
            stop("Dat klopt. Ehm, nu de rest", None),
            "Dat klopt. Nu de rest"
        );
        assert_eq!(stop("Echt? Uh ijsselmeer", None), "Echt? IJsselmeer");
        // Twee stopwoorden achter elkaar: nog steeds een hoofdletter.
        assert_eq!(stop("Uh, ehm, ik denk het", None), "Ik denk het");
        // Midden in een zin verandert er niets aan de hoofdletters.
        assert_eq!(stop("ik denk uh Jan komt", None), "ik denk Jan komt");
    }

    #[test]
    fn een_komma_die_daardoor_aan_het_begin_van_de_zin_komt_gaat_weg() {
        assert_eq!(stop("Uh , ik denk het", None), "Ik denk het");
        assert_eq!(
            stop("Dat klopt. Uh ,nu de rest", None),
            "Dat klopt. Nu de rest"
        );
    }

    #[test]
    fn het_begin_van_een_zin_loopt_door_over_segmenten() {
        // Het eerste segment eindigt met een punt, het tweede opent met een stopwoord.
        assert_eq!(
            stop_segmenten(
                &[(0, 2000, "Dat klopt."), (2000, 4000, "uh ik ga door")],
                None
            ),
            "Dat klopt. Ik ga door"
        );
        // Het stopwoord staat aan het eind van het ene segment, midden in een zin.
        assert_eq!(
            stop_segmenten(
                &[(0, 2000, "ik denk uh"), (2000, 4000, "dat het klopt")],
                None
            ),
            "ik denk dat het klopt"
        );
    }

    #[test]
    fn een_segment_dat_leeg_wordt_valt_weg_en_de_rest_houdt_zijn_tijden() {
        let t = transcript(
            &[
                (0, 1000, "Uh,"),
                (1000, 3000, "ehm"),
                (3000, 4000, "Hallo daar"),
            ],
            4,
        );
        let uit = filter(t, &toon(4, 0.2), &standaard());
        assert_eq!(uit.transcript.segmenten.len(), 1);
        assert_eq!(uit.transcript.segmenten[0].start_ms, 3000);
        assert_eq!(uit.transcript.segmenten[0].eind_ms, 4000);
        assert_eq!(uit.transcript.segmenten[0].tekst, "Hallo daar");
        assert_eq!(
            uit.ingrepen,
            vec![Ingreep::StopwoordenVerwijderd { aantal: 2 }]
        );
    }

    #[test]
    fn een_segment_met_alleen_stopwoorden_geeft_geen_tekst() {
        let t = transcript(&[(0, 4000, "Uh, ehm, hmm.")], 4);
        let uit = filter(t, &toon(4, 0.2), &standaard());
        assert!(uit.transcript.segmenten.is_empty());
        assert_eq!(uit.uitkomst, Uitkomst::GeenTekst);
    }

    #[test]
    fn een_aangepast_segment_houdt_de_woordtijden_van_wat_overblijft() {
        let woord = |tekst: &str, start_ms, eind_ms| crate::engine::protocol::Woord {
            tekst: tekst.to_string(),
            start_ms,
            eind_ms,
        };
        let mut aangepast = Segment::nieuw(0, 2000, "ehm hallo");
        aangepast.woorden = vec![woord("ehm", 0, 500), woord("hallo", 500, 2000)];
        let mut gelijk = Segment::nieuw(2000, 4000, "tot straks");
        gelijk.woorden = vec![woord("tot", 2000, 3000), woord("straks", 3000, 4000)];
        let t = Transcript {
            segmenten: vec![aangepast, gelijk],
            audio_duur: Duration::from_secs(4),
        };
        let uit = filter(t, &toon(4, 0.2), &standaard());
        assert_eq!(uit.transcript.segmenten[0].tekst, "Hallo");
        // Alleen het woord "ehm" is weg; "hallo" houdt zijn tijden, met de nieuwe hoofdletter.
        assert_eq!(
            uit.transcript.segmenten[0].woorden,
            vec![woord("Hallo", 500, 2000)]
        );
        assert_eq!(uit.transcript.segmenten[1].woorden.len(), 2);
    }

    #[test]
    fn alleen_een_doorgeschoven_hoofdletter_laat_de_woordtijden_staan() {
        let woord = |tekst: &str, start_ms, eind_ms| crate::engine::protocol::Woord {
            tekst: tekst.to_string(),
            start_ms,
            eind_ms,
        };
        // Het stopwoord sloot het eerste segment af; het tweede krijgt alleen een hoofdletter.
        let mut eerste = Segment::nieuw(0, 2000, "Dat klopt uh.");
        eerste.woorden = vec![
            woord("Dat", 0, 500),
            woord("klopt", 500, 1000),
            woord("uh.", 1000, 2000),
        ];
        let mut tweede = Segment::nieuw(2000, 4000, "nu verder");
        tweede.woorden = vec![woord("nu", 2000, 3000), woord("verder", 3000, 4000)];
        let t = Transcript {
            segmenten: vec![eerste, tweede],
            audio_duur: Duration::from_secs(4),
        };
        let uit = filter(t, &toon(4, 0.2), &standaard());
        assert_eq!(uit.transcript.segmenten[0].tekst, "Dat klopt.");
        assert_eq!(
            uit.transcript.segmenten[0].woorden,
            vec![woord("Dat", 0, 500), woord("klopt.", 500, 1000)]
        );
        assert_eq!(uit.transcript.segmenten[1].tekst, "Nu verder");
        assert_eq!(
            uit.transcript.segmenten[1].woorden,
            vec![woord("Nu", 2000, 3000), woord("verder", 3000, 4000)]
        );
    }

    #[test]
    fn woordtijden_die_niet_bij_de_tekst_passen_gaan_weg() {
        let mut segment = Segment::nieuw(0, 2000, "ehm hallo daar");
        segment.woorden = vec![crate::engine::protocol::Woord {
            tekst: "ehm hallo daar".to_string(),
            start_ms: 0,
            eind_ms: 2000,
        }];
        let t = Transcript {
            segmenten: vec![segment],
            audio_duur: Duration::from_secs(2),
        };
        let uit = filter(t, &toon(2, 0.2), &standaard());
        assert_eq!(uit.transcript.segmenten[0].tekst, "Hallo daar");
        assert!(uit.transcript.segmenten[0].woorden.is_empty());
    }

    #[test]
    fn een_tekst_zonder_stopwoord_blijft_precies_zoals_hij_was() {
        let t = transcript(&[(0, 4000, "  ik  denk   het ")], 4);
        let uit = filter(t, &toon(4, 0.2), &standaard());
        assert_eq!(uit.transcript.segmenten[0].tekst, "  ik  denk   het ");
        assert!(uit.ingrepen.is_empty());
    }

    #[test]
    fn uitgezet_blijven_de_stopwoorden_staan() {
        let t = transcript(&[(0, 4000, "Uh, ik denk ehm het")], 4);
        let uit = filter(t, &toon(4, 0.2), &standaard().met_stopwoorden(false));
        assert_eq!(uit.transcript.tekst(), "Uh, ik denk ehm het");
        assert!(uit.ingrepen.is_empty());
    }

    #[test]
    fn de_taal_komt_uit_een_taalcode() {
        assert_eq!(FilterTaal::uit_code("nl"), FilterTaal::Nl);
        assert_eq!(FilterTaal::uit_code("NL-nl"), FilterTaal::Nl);
        assert_eq!(FilterTaal::uit_code("en_GB"), FilterTaal::En);
        assert_eq!(FilterTaal::uit_code("fr"), FilterTaal::Andere);
        assert_eq!(FilterTaal::uit_code(""), FilterTaal::Andere);
    }

    #[test]
    fn stopwoorden_gaan_weg_voordat_een_taalmodel_de_tekst_ziet() {
        // Wat het filter teruggeeft, is wat `schoon_op` en de geschiedenis krijgen (ADR 0027,
        // 0028): het dictaat is dan al zonder stopwoorden.
        let t = transcript(&[(0, 4000, "Ehm, ik wil dat uh iedereen komt")], 4);
        let uit = filter(
            t,
            &toon(4, 0.2),
            &standaard().met_taal(Some(FilterTaal::Nl)),
        );
        assert_eq!(uit.transcript.tekst(), "Ik wil dat iedereen komt");
    }
}
