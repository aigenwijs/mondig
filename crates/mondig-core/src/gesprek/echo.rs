//! Meten hoeveel van "Anderen" via de luidspreker in de microfoon terechtkomt (sprint 4b-B).
//!
//! Dit is een meting om een keuze te onderbouwen, geen onderdeel van het uitschrijven: bij
//! een vergadering zonder koptelefoon hoort de microfoon ook de anderen. Dan zijn er twee
//! wegen, ontdubbelen (uitingen van "Jij" die op "Anderen" lijken weglaten) of
//! echo-onderdrukking op het signaal. Twee maten laten zien welke nodig is:
//!
//! - **Tekst** ([`tekst_echo`]): een uiting van "Jij" is verdacht als de meeste woorden ook
//!   in de uitingen van "Anderen" rond dat moment staan.
//! - **Signaal** ([`beste_vertraging`], [`niveau_bij_systeem`]): hoe sterk de luidheid van
//!   de microfoon de luidheid van het systeemspoor volgt, en hoe luid de microfoon is als
//!   het systeemspoor luid of stil is.
//!
//! Alle rekenregels zijn pure functies. Alleen [`omhullende_van_spoor`] leest een spoor, in
//! blokken, zodat er nooit meer audio in het geheugen staat dan één blok.
//!
//! De tekstdrempels ([`ECHO_PERCENTAGE`], [`ECHO_MARGE`]) zijn afgesteld op AMI `ES2002a` met
//! Canary (ADR 0008, aanvulling 4b-B). De drempels van het signaal zijn nog niet gemeten.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use crate::engine::protocol::Segment;
use crate::spoor::{SpoorFout, Spoorlezer};

use super::MICROFOON_VERTRAGING;

/// Een uiting van "Jij" is echo-verdacht als ten minste zoveel procent van haar woorden bij
/// "Anderen" staat, binnen [`ECHO_MARGE`] van de uiting (woorden met een geschatte tijd, elk
/// woord van "Anderen" telt één keer). Afgesteld op AMI `ES2002a` met Canary: bij 42 klopt
/// 85,5 procent van de beurten met de luidspreker (97,1 met de koptelefoon) en gaat 0,2
/// procent van de echte woorden van "Jij" verloren met de koptelefoon. Bij 40 of lager vallen
/// er met de koptelefoon 1,0 procent of meer weg (36 geeft 2,2 procent), bij 43 zakt de
/// luidspreker naar 84,8 procent; 56 gaf 81,6 procent met 8,0 procent verloren aan de
/// luidsprekerkant. Bewust de veilige kant van de koptelefoon, de opstelling die we aanraden:
/// één Engelse vergadering met nagebootste echo; zie ADR 0008.
pub const ECHO_PERCENTAGE: usize = 42;

/// Zoveel ruimte krijgt een uiting van "Jij" aan beide kanten om "Anderen" te overlappen: de
/// luidspreker loopt achter op het systeemspoor en het model zet tijden niet op de milliseconde.
/// Bij 1,5 tot 2,5 s zijn de uitkomsten op AMI `ES2002a` gelijk; 1 s is iets slechter (84,9
/// procent), en vanaf 3 s vallen er met de koptelefoon meer echte woorden weg.
pub const ECHO_MARGE: Duration = Duration::from_secs(2);

/// Een uiting van "Jij" met minder woorden valt nooit weg als echo. Een los "ja" of "nee" zegt
/// iemand met een koptelefoon ook terwijl een ander toevallig hetzelfde woord gebruikt; dat
/// weglaten kost meer dan een dubbel "ja" in het verslag.
pub const ECHO_MIN_WOORDEN: usize = 2;

/// Zo breed is een bin van de omhullende, in milliseconden.
pub const BIN_MS: u64 = 10;

/// Zoveel samples (16 kHz) vallen in één bin.
const BIN_SAMPLES: usize = 160;

/// Zoveel samples worden per keer van het spoor gelezen: een veelvoud van [`BIN_SAMPLES`],
/// zodat elk blok op een bin begint.
const LEES_SAMPLES: usize = BIN_SAMPLES * 100;

/// Hoogste vertraging die [`beste_vertraging`] zoekt, in milliseconden. Nog niet gemeten.
pub const MAX_VERTRAGING_MS: u64 = 500;

/// Zo veel bins moeten er minstens overlappen om een correlatie te geloven (1 seconde).
const MIN_BINS: usize = 100;

/// Zo breed is een venster voor het niveau, in bins (100 ms).
pub const NIVEAU_VENSTER_BINS: usize = 10;

/// Een venster waarin het systeemspoor boven dit niveau komt, heet luid. Nog niet gemeten.
pub const LUID_DREMPEL_DBFS: f32 = -40.0;

/// Een venster waarin het systeemspoor onder dit niveau blijft, heet stil. Nog niet gemeten.
pub const STIL_DREMPEL_DBFS: f32 = -60.0;

/// Zo laag zakt een niveau hooguit in de weergave: stilte is geen min oneindig.
const VLOER_DBFS: f32 = -120.0;

/// Zoveel bins (ongeveer 28 uur) mag het logboek hooguit aanwijzen. Verder is het logboek
/// kapot; dat geluid telt niet mee, zodat het geheugen niet onbegrensd groeit.
const MAX_BINS: usize = 10_000_000;

// ---------------------------------------------------------------------------
// Tekst
// ---------------------------------------------------------------------------

/// De woorden van een tekst, genormaliseerd: kleine letters, zonder leestekens.
#[must_use]
pub fn woorden(tekst: &str) -> Vec<String> {
    tekst
        .split_whitespace()
        .map(|woord| {
            woord
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|woord| !woord.is_empty())
        .collect()
}

/// Een uiting van "Jij" waarvan de woorden grotendeels in "Anderen" voorkomen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerdachteUiting {
    /// Begin op de tijdlijn, na de microfoonvertraging, in milliseconden.
    pub start_ms: u64,
    /// Eind op de tijdlijn, na de microfoonvertraging, in milliseconden.
    pub eind_ms: u64,
    /// Hoeveel procent van de woorden ook bij "Anderen" staat (afgerond naar beneden).
    pub procent: usize,
}

/// De uitkomst van [`tekst_echo`]. Bewust zonder tekst, zodat die nergens in een log komt.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TekstEcho {
    /// Hoeveel uitingen van "Jij" woorden hebben.
    pub jij_uitingen: usize,
    /// De verdachte uitingen, op volgorde van de tijd.
    pub verdacht: Vec<VerdachteUiting>,
}

impl TekstEcho {
    /// Het aandeel verdachte uitingen in procenten (afgerond naar beneden); 0 zonder uitingen.
    #[must_use]
    pub fn procent_verdacht(&self) -> usize {
        (self.verdacht.len() * 100)
            .checked_div(self.jij_uitingen)
            .unwrap_or(0)
    }
}

/// De afstelling van het ontdubbelen: wanneer een uiting van "Jij" op "Anderen" lijkt.
/// Eén bron voor [`tekst_echo`] (de meting) en [`ontdubbel`] (het uitschrijven).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ontdubbelen {
    /// Ten minste zoveel procent van de woorden moet ook bij "Anderen" staan.
    pub percentage: usize,
    /// Zoveel ruimte krijgt een uiting aan beide kanten om "Anderen" te overlappen.
    pub marge: Duration,
}

impl Default for Ontdubbelen {
    fn default() -> Self {
        Self {
            percentage: ECHO_PERCENTAGE,
            marge: ECHO_MARGE,
        }
    }
}

/// Het oordeel over één uiting van "Jij" met woorden.
struct Oordeel {
    /// Plaats van de uiting in de rij van "Jij".
    index: usize,
    start_ms: u64,
    eind_ms: u64,
    procent: usize,
    verdacht: bool,
}

/// De woorden van de uitingen van "Anderen", elk met een geschatte tijd. Het model geeft alleen
/// begin en eind van een uiting, dus de woorden worden gelijk over de uiting verdeeld. Zo telt
/// een woord dat een ander twintig seconden eerder in een lange uiting zei niet mee als echo.
fn woorden_met_tijd(anderen: &[Segment]) -> Vec<(u64, String)> {
    let mut uit = Vec::new();
    for uiting in anderen {
        let woorden = woorden(&uiting.tekst);
        let duur = uiting.eind_ms.saturating_sub(uiting.start_ms);
        let aantal = woorden.len() as u64;
        for (i, woord) in (0u64..).zip(woorden) {
            // Het midden van het i-de deel van de uiting.
            let tijd = uiting.start_ms + (duur * (2 * i + 1)) / (2 * aantal);
            uit.push((tijd, woord));
        }
    }
    uit.sort_by_key(|(tijd, _)| *tijd);
    uit
}

/// Beoordeelt elke uiting van "Jij" met woorden; de enige plek waar de echoregel staat.
fn beoordeel(jij: &[Segment], anderen: &[Segment], regel: &Ontdubbelen) -> Vec<Oordeel> {
    let vertraging = millis(MICROFOON_VERTRAGING);
    let marge = millis(regel.marge);
    let anderen = woorden_met_tijd(anderen);
    let mut uit = Vec::new();
    for (index, uiting) in jij.iter().enumerate() {
        let eigen = woorden(&uiting.tekst);
        if eigen.is_empty() {
            continue;
        }
        let start = uiting.start_ms.saturating_sub(vertraging);
        let eind = uiting.eind_ms.saturating_sub(vertraging);
        // De woorden van Anderen binnen het venster, met hoe vaak ze voorkomen: elk woord van
        // Anderen telt voor hooguit één woord van Jij ("nee nee nee" tegen één "nee" is 1 van 3).
        let van = start.saturating_sub(marge);
        let tot = eind.saturating_add(marge);
        let begin = anderen.partition_point(|(tijd, _)| *tijd < van);
        let mut dichtbij: HashMap<&str, usize> = HashMap::new();
        for (_, woord) in anderen[begin..].iter().take_while(|(tijd, _)| *tijd <= tot) {
            *dichtbij.entry(woord.as_str()).or_default() += 1;
        }
        let mut gevonden = 0;
        for woord in &eigen {
            if let Some(over) = dichtbij.get_mut(woord.as_str()).filter(|over| **over > 0) {
                *over -= 1;
                gevonden += 1;
            }
        }
        uit.push(Oordeel {
            index,
            start_ms: start,
            eind_ms: eind,
            procent: gevonden * 100 / eigen.len(),
            verdacht: eigen.len() >= ECHO_MIN_WOORDEN
                && gevonden * 100 >= eigen.len() * regel.percentage,
        });
    }
    uit
}

/// Zoekt de uitingen van "Jij" die op "Anderen" lijken.
///
/// De tijden van "Jij" schuiven eerst [`MICROFOON_VERTRAGING`] naar voren (zoals in
/// [`super::beurten`]). Een uiting is verdacht als ten minste [`ECHO_PERCENTAGE`] procent van
/// haar woorden voorkomt in de uitingen van "Anderen" die binnen [`ECHO_MARGE`] van haar
/// liggen. Uitingen zonder woorden tellen niet mee.
#[must_use]
pub fn tekst_echo(jij: &[Segment], anderen: &[Segment]) -> TekstEcho {
    let mut uit = TekstEcho::default();
    for oordeel in beoordeel(jij, anderen, &Ontdubbelen::default()) {
        uit.jij_uitingen += 1;
        if oordeel.verdacht {
            uit.verdacht.push(VerdachteUiting {
                start_ms: oordeel.start_ms,
                eind_ms: oordeel.eind_ms,
                procent: oordeel.procent,
            });
        }
    }
    uit.verdacht.sort_by_key(|v| v.start_ms);
    uit
}

/// Wat [`ontdubbel`] overlaat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ontdubbeld {
    /// De uitingen van "Jij" die blijven, ongewijzigd en met de oorspronkelijke tijden.
    pub over: Vec<Segment>,
    /// Hoeveel uitingen er als echo wegvielen.
    pub weggelaten: usize,
    /// Hoeveel uitingen van "Jij" woorden hadden (de noemer van de meting, [`TekstEcho`]).
    pub met_woorden: usize,
}

/// Laat de uitingen van "Jij" weg die volgens `regel` echo van "Anderen" zijn. Dezelfde regel
/// als [`tekst_echo`], dus wat de meting verdacht noemt, valt weg.
#[must_use]
pub fn ontdubbel(jij: &[Segment], anderen: &[Segment], regel: &Ontdubbelen) -> Ontdubbeld {
    let oordelen = beoordeel(jij, anderen, regel);
    let met_woorden = oordelen.len();
    let weg: HashSet<usize> = oordelen
        .into_iter()
        .filter(|o| o.verdacht)
        .map(|o| o.index)
        .collect();
    let over = jij
        .iter()
        .enumerate()
        .filter(|(index, _)| !weg.contains(index))
        .map(|(_, segment)| segment.clone())
        .collect();
    Ontdubbeld {
        over,
        weggelaten: weg.len(),
        met_woorden,
    }
}

fn millis(duur: Duration) -> u64 {
    u64::try_from(duur.as_millis()).unwrap_or(u64::MAX)
}

// ---------------------------------------------------------------------------
// Signaal
// ---------------------------------------------------------------------------

/// De luidheid van een spoor per bin van [`BIN_MS`] op de tijdlijn van de opname.
///
/// Elke bin houdt de som van de kwadraten en het aantal samples bij, zodat samples die over
/// een stuk verspreid binnenkomen goed samenkomen. Een bin waar geen geluid viel (een gat
/// tussen twee stukken) blijft stil.
#[derive(Debug, Clone, Default)]
pub struct Omhullende {
    som: Vec<f64>,
    aantal: Vec<u32>,
}

impl Omhullende {
    /// Een lege omhullende.
    #[must_use]
    pub fn nieuw() -> Self {
        Self::default()
    }

    /// Voegt samples toe die beginnen op `ms` op de tijdlijn. Bij een negatieve tijd gaat het
    /// geluid naar de eerste bin; wat veel te ver buiten de opname valt, wordt genegeerd.
    pub fn voeg_toe(&mut self, ms: i64, samples: &[f32]) {
        let bin = usize::try_from(ms.max(0) / i64::try_from(BIN_MS).unwrap_or(10)).unwrap_or(0);
        if bin >= MAX_BINS {
            return;
        }
        if self.som.len() <= bin {
            self.som.resize(bin + 1, 0.0);
            self.aantal.resize(bin + 1, 0);
        }
        for s in samples {
            self.som[bin] += f64::from(*s) * f64::from(*s);
        }
        self.aantal[bin] =
            self.aantal[bin].saturating_add(u32::try_from(samples.len()).unwrap_or(u32::MAX));
    }

    /// De effectieve waarde (RMS) per bin, 0 tot 1.
    #[must_use]
    pub fn rms(&self) -> Vec<f32> {
        self.som
            .iter()
            .zip(&self.aantal)
            .map(|(som, aantal)| {
                if *aantal == 0 {
                    0.0
                } else {
                    #[allow(clippy::cast_possible_truncation, reason = "een RMS van 0 tot 1")]
                    let waarde = (som / f64::from(*aantal)).sqrt() as f32;
                    waarde
                }
            })
            .collect()
    }
}

/// Leest een spoor (`<opnamemap>/microfoon` of `<opnamemap>/systeem`) streamend en maakt er
/// de [`Omhullende`] van op de tijdlijn van de opname. Er staat nooit meer dan één blok van
/// ongeveer een seconde in het geheugen.
///
/// # Errors
///
/// Geeft een [`SpoorFout`] als de opname niet is afgesloten of een stuk niet te lezen is.
pub fn omhullende_van_spoor(spoormap: &Path) -> Result<Omhullende, SpoorFout> {
    let mut lezer = Spoorlezer::open(spoormap)?;
    let mut omhullende = Omhullende::nieuw();
    loop {
        let begin = lezer.positie();
        let blok = lezer.lees(LEES_SAMPLES)?;
        if blok.is_empty() {
            return Ok(omhullende);
        }
        for (nummer, deel) in blok.chunks(BIN_SAMPLES).enumerate() {
            let positie = begin + (nummer * BIN_SAMPLES) as u64;
            omhullende.voeg_toe(lezer.tijdlijn().ms(positie), deel);
        }
    }
}

/// Hoeveel de microfoon achterloopt op het systeemspoor, en hoe sterk ze dan samenhangen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertraging {
    /// De vertraging in milliseconden, een veelvoud van [`BIN_MS`].
    pub ms: u64,
    /// De correlatie (Pearson) tussen de omhullenden, -1 tot 1.
    pub correlatie: f32,
}

#[allow(
    clippy::cast_precision_loss,
    reason = "tellen van een paar miljoen bins"
)]
fn als_f64(n: usize) -> f64 {
    n as f64
}

/// De correlatie tussen `x` en `y` over de eerste `n` waarden, of `None` bij te weinig
/// waarden of een signaal zonder verloop.
fn pearson(systeem: &[f32], microfoon: &[f32], aantal: usize) -> Option<f32> {
    if aantal < MIN_BINS {
        return None;
    }
    // Sommen over x, y, x², y² en x·y.
    let mut som = [0.0_f64; 5];
    for (s, m) in systeem.iter().zip(microfoon).take(aantal) {
        let (s, m) = (f64::from(*s), f64::from(*m));
        som[0] += s;
        som[1] += m;
        som[2] += s * s;
        som[3] += m * m;
        som[4] += s * m;
    }
    let n = als_f64(aantal);
    let var_x = som[2] - som[0] * som[0] / n;
    let var_y = som[3] - som[1] * som[1] / n;
    if var_x <= 1e-18 || var_y <= 1e-18 {
        return None;
    }
    let kov = som[4] - som[0] * som[1] / n;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "een correlatie van -1 tot 1"
    )]
    let uit = (kov / (var_x * var_y).sqrt()) as f32;
    Some(uit)
}

/// De vertraging van 0 tot [`MAX_VERTRAGING_MS`] waarbij de omhullende van de microfoon het
/// best bij die van het systeemspoor past, met de correlatie erbij. De microfoon volgt het
/// systeemspoor: `microfoon[i + vertraging]` wordt vergeleken met `systeem[i]`.
///
/// `None` als een van beide te kort is of geen verloop heeft (bijvoorbeeld een stil spoor).
#[must_use]
pub fn beste_vertraging(systeem: &[f32], microfoon: &[f32]) -> Option<Vertraging> {
    let lengte = systeem.len().max(microfoon.len());
    let mut x = systeem.to_vec();
    let mut y = microfoon.to_vec();
    x.resize(lengte, 0.0);
    y.resize(lengte, 0.0);
    let max = usize::try_from(MAX_VERTRAGING_MS / BIN_MS).unwrap_or(0);
    let mut beste: Option<(usize, f32)> = None;
    for bins in 0..=max {
        let Some(n) = lengte.checked_sub(bins) else {
            break;
        };
        if let Some(c) = pearson(&x, &y[bins..], n)
            && beste.is_none_or(|(_, b)| c > b)
        {
            beste = Some((bins, c));
        }
    }
    beste.map(|(bins, correlatie)| Vertraging {
        ms: bins as u64 * BIN_MS,
        correlatie,
    })
}

/// Het niveau van de microfoon in een groep vensters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Niveau {
    /// Hoeveel vensters er in de groep zitten.
    pub vensters: usize,
    /// Het niveau over alle vensters samen (energiegemiddelde), in dBFS; `None` zonder vensters.
    pub gemiddeld_dbfs: Option<f32>,
    /// Het middelste venster, in dBFS: minder gevoelig voor korte uitschieters, zoals jijzelf
    /// die iets zegt. `None` zonder vensters.
    pub mediaan_dbfs: Option<f32>,
}

/// Het microfoonniveau als het systeemspoor luid is, naast dat als het systeemspoor stil is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NiveauMeting {
    /// Vensters waarin het systeemspoor boven [`LUID_DREMPEL_DBFS`] zat.
    pub bij_luid: Niveau,
    /// Vensters waarin het systeemspoor onder [`STIL_DREMPEL_DBFS`] bleef.
    pub bij_stil: Niveau,
}

fn naar_dbfs(kwadraat: f64) -> f32 {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "een niveau in dB past in een f32"
    )]
    let db = (10.0 * kwadraat.max(1e-30).log10()) as f32;
    db.max(VLOER_DBFS)
}

fn groep_niveau(kwadraten: &mut [f64]) -> Niveau {
    if kwadraten.is_empty() {
        return Niveau {
            vensters: 0,
            gemiddeld_dbfs: None,
            mediaan_dbfs: None,
        };
    }
    let gemiddeld = kwadraten.iter().sum::<f64>() / als_f64(kwadraten.len());
    kwadraten.sort_by(f64::total_cmp);
    let midden = kwadraten.len() / 2;
    let mediaan = if kwadraten.len().is_multiple_of(2) {
        f64::midpoint(kwadraten[midden - 1], kwadraten[midden])
    } else {
        kwadraten[midden]
    };
    Niveau {
        vensters: kwadraten.len(),
        gemiddeld_dbfs: Some(naar_dbfs(gemiddeld)),
        mediaan_dbfs: Some(naar_dbfs(mediaan)),
    }
}

/// Meet het niveau van de microfoon in vensters van [`NIVEAU_VENSTER_BINS`] bins, verdeeld naar
/// wat het systeemspoor in datzelfde venster doet: luid, stil, of daartussen (die tellen niet
/// mee). Het microfoonvenster schuift `vertraging_bins` op, de uitkomst van
/// [`beste_vertraging`], zodat de echo in het venster valt waarin hij hoort.
#[must_use]
pub fn niveau_bij_systeem(
    systeem: &[f32],
    microfoon: &[f32],
    vertraging_bins: usize,
) -> NiveauMeting {
    let kwadraat_gemiddeld = |bins: &[f32], van: usize| -> Option<f64> {
        let deel = bins.get(van..)?;
        let deel = &deel[..deel.len().min(NIVEAU_VENSTER_BINS)];
        // Een venster dat voorbij het eind van het spoor reikt, telt niet.
        (deel.len() == NIVEAU_VENSTER_BINS).then(|| {
            deel.iter()
                .map(|w| f64::from(*w) * f64::from(*w))
                .sum::<f64>()
                / als_f64(NIVEAU_VENSTER_BINS)
        })
    };
    let (mut luid, mut stil) = (Vec::new(), Vec::new());
    let mut van = 0;
    while van + NIVEAU_VENSTER_BINS <= systeem.len() {
        if let (Some(s), Some(m)) = (
            kwadraat_gemiddeld(systeem, van),
            kwadraat_gemiddeld(microfoon, van + vertraging_bins),
        ) {
            let db = naar_dbfs(s);
            if db > LUID_DREMPEL_DBFS {
                luid.push(m);
            } else if db < STIL_DREMPEL_DBFS {
                stil.push(m);
            }
        }
        van += NIVEAU_VENSTER_BINS;
    }
    NiveauMeting {
        bij_luid: groep_niveau(&mut luid),
        bij_stil: groep_niveau(&mut stil),
    }
}

/// Het niveau van een kwadratisch gemiddelde als dBFS, voor de weergave in de opdrachtregel.
#[must_use]
pub fn dbfs_van_rms(rms: f32) -> f32 {
    naar_dbfs(f64::from(rms) * f64::from(rms))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::spoor::stukken::{LOGBOEK, StukInfo, stuknaam};
    use crate::spoor::{EINDE, MICROFOON, SYSTEEM};

    fn seg(start_ms: u64, eind_ms: u64, tekst: &str) -> Segment {
        Segment::nieuw(start_ms, eind_ms, tekst)
    }

    #[test]
    fn woorden_zijn_klein_en_zonder_leestekens() {
        assert_eq!(
            woorden("  Dank u wel, Wethouder!  Het gaat om 't \"beleid\"... (ja)"),
            [
                "dank",
                "u",
                "wel",
                "wethouder",
                "het",
                "gaat",
                "om",
                "t",
                "beleid",
                "ja"
            ]
        );
        assert_eq!(woorden("É, ÉÉN"), ["é", "één"]);
        assert!(woorden(" ... ?! ").is_empty());
    }

    #[test]
    fn een_uiting_is_verdacht_vanaf_het_percentage_woorden_van_anderen() {
        let anderen = [seg(
            10_000,
            14_000,
            "We bespreken vandaag de begroting voor volgend jaar",
        )];
        // 3 van 5 woorden (60 procent) is genoeg; microfoon 100 ms vertraagd.
        let jij = [seg(10_600, 12_100, "De begroting voor mijn buurt")];
        let uit = tekst_echo(&jij, &anderen);
        assert_eq!(uit.jij_uitingen, 1);
        assert_eq!(
            uit.verdacht,
            vec![VerdachteUiting {
                start_ms: 10_500,
                eind_ms: 12_000,
                procent: 60
            }]
        );
        // 2 van 5 (40 procent) niet.
        let jij = [seg(10_600, 12_100, "De begroting van mijn buurt")];
        let uit = tekst_echo(&jij, &anderen);
        assert_eq!(uit.jij_uitingen, 1);
        assert!(uit.verdacht.is_empty());
        assert_eq!(uit.procent_verdacht(), 0);
    }

    #[test]
    fn alleen_woorden_van_anderen_binnen_twee_seconden_tellen() {
        // Een uiting zonder duur: alle woorden vallen op 12 s.
        let anderen = [seg(12_000, 12_000, "begroting voor volgend jaar")];
        let tekst = "begroting voor volgend jaar";
        // Net binnen de marge: Jij eindigt (na vertraging) 2 s voor de woorden van Anderen.
        let binnen = tekst_echo(&[seg(9_000, 10_100, tekst)], &anderen);
        assert_eq!(binnen.verdacht.len(), 1);
        // Een milliseconde te ver.
        let buiten = tekst_echo(&[seg(8_999, 10_099, tekst)], &anderen);
        assert!(buiten.verdacht.is_empty());
        // En aan de andere kant: Jij begint 2 s na de woorden van Anderen.
        let erna = tekst_echo(&[seg(14_100, 15_000, tekst)], &anderen);
        assert_eq!(erna.verdacht.len(), 1);
        let te_laat = tekst_echo(&[seg(14_101, 15_000, tekst)], &anderen);
        assert!(te_laat.verdacht.is_empty());
    }

    #[test]
    fn uitingen_zonder_woorden_en_zonder_anderen_tellen_goed() {
        let jij = [
            seg(0, 1000, "ja"),
            seg(2000, 3000, " ... "),
            seg(4000, 5000, ""),
            seg(6000, 7000, "oké goed"),
        ];
        let uit = tekst_echo(&jij, &[]);
        assert_eq!(uit.jij_uitingen, 2);
        assert!(uit.verdacht.is_empty());
        assert_eq!(TekstEcho::default().procent_verdacht(), 0);
        // De helft van de uitingen verdacht: 50 procent. Het losse "ja" valt nooit weg
        // ([`ECHO_MIN_WOORDEN`]), "oké goed" wel.
        let anderen = [seg(0, 1000, "ja"), seg(6000, 7000, "oké goed")];
        let uit = tekst_echo(&jij, &anderen);
        assert_eq!((uit.jij_uitingen, uit.verdacht.len()), (2, 1));
        assert_eq!(uit.procent_verdacht(), 50);
    }

    #[test]
    fn de_verdachte_uitingen_staan_op_volgorde_van_de_tijd() {
        let anderen = [
            seg(10_000, 11_000, "een twee drie"),
            seg(30_000, 31_000, "een twee drie"),
        ];
        let jij = [
            seg(30_100, 31_000, "een twee drie"),
            seg(10_100, 11_000, "drie twee een"),
        ];
        let uit = tekst_echo(&jij, &anderen);
        let tijden: Vec<_> = uit.verdacht.iter().map(|v| v.start_ms).collect();
        assert_eq!(tijden, [10_000, 30_000]);
    }

    #[test]
    fn ontdubbelen_laat_alleen_de_verdachte_uitingen_weg() {
        let anderen = [seg(
            10_000,
            14_000,
            "We bespreken de begroting voor volgend jaar",
        )];
        let jij = [
            // Echo: alle woorden staan bij Anderen.
            seg(10_600, 12_100, "de begroting voor volgend jaar"),
            // Eigen woorden, ver van Anderen.
            seg(30_000, 31_000, "Ik ben het daar niet mee eens"),
            // Zonder woorden: blijft staan, telt niet mee.
            seg(40_000, 41_000, " ... "),
        ];
        let uit = ontdubbel(&jij, &anderen, &Ontdubbelen::default());
        assert_eq!((uit.weggelaten, uit.met_woorden), (1, 2));
        assert_eq!(uit.over, vec![jij[1].clone(), jij[2].clone()]);
        // Dezelfde regel als de meting: wat daar verdacht is, valt hier weg.
        assert_eq!(tekst_echo(&jij, &anderen).verdacht.len(), uit.weggelaten);
        // Zonder Anderen valt er niets weg.
        let uit = ontdubbel(&jij, &[], &Ontdubbelen::default());
        assert_eq!((uit.over.len(), uit.weggelaten), (3, 0));
    }

    #[test]
    fn woorden_van_lang_geleden_in_een_lange_uiting_tellen_niet_mee() {
        // Anderen praat 25 s en zegt de woorden aan het begin; Jij onderbreekt op 20 s met
        // dezelfde woorden (met koptelefoon, dus geen echo).
        let mut tekst = String::from("maar dat is niet wat we hebben afgesproken");
        for _ in 0..40 {
            tekst.push_str(" bla");
        }
        let anderen = [seg(0, 25_000, &tekst)];
        let jij = [seg(
            20_100,
            22_100,
            "maar dat is niet wat we hebben afgesproken",
        )];
        assert_eq!(
            ontdubbel(&jij, &anderen, &Ontdubbelen::default()).weggelaten,
            0
        );
        // Zegt Anderen ze vlak ervoor, dan is het wel echo.
        let anderen = [seg(
            19_000,
            21_500,
            "maar dat is niet wat we hebben afgesproken",
        )];
        assert_eq!(
            ontdubbel(&jij, &anderen, &Ontdubbelen::default()).weggelaten,
            1
        );
    }

    #[test]
    fn een_woord_van_anderen_telt_voor_hooguit_een_woord_van_jij() {
        let anderen = [seg(10_000, 11_000, "nee")];
        let jij = [seg(10_100, 11_100, "nee nee nee nee")];
        assert_eq!(
            ontdubbel(&jij, &anderen, &Ontdubbelen::default()).weggelaten,
            0
        );
    }

    #[test]
    fn een_los_woord_van_jij_valt_nooit_weg() {
        let anderen = [seg(10_000, 11_000, "nee")];
        let jij = [seg(10_100, 11_100, "Nee.")];
        let uit = ontdubbel(&jij, &anderen, &Ontdubbelen::default());
        assert_eq!((uit.weggelaten, uit.met_woorden), (0, 1));
    }

    #[test]
    fn de_afstelling_bepaalt_drempel_en_venster() {
        // De woorden van Anderen vallen op 13,125, 13,375, 13,625 en 13,875 s.
        let anderen = [seg(13_000, 14_000, "een twee drie vier")];
        // 2 van 4 woorden (50 procent); Jij begint na de vertraging op 16,5 s, dus "drie" en
        // "vier" vallen binnen 3 s en niet binnen 2 s.
        let jij = [seg(16_600, 18_000, "drie vier vijf zes")];
        let ruim = Ontdubbelen {
            percentage: 50,
            marge: Duration::from_secs(3),
        };
        assert_eq!(ontdubbel(&jij, &anderen, &ruim).weggelaten, 1);
        // Strenger percentage of kleiner venster laat de uiting staan.
        let streng = Ontdubbelen {
            percentage: 60,
            ..ruim
        };
        assert_eq!(ontdubbel(&jij, &anderen, &streng).weggelaten, 0);
        let nauw = Ontdubbelen {
            marge: Duration::from_secs(2),
            ..ruim
        };
        assert_eq!(ontdubbel(&jij, &anderen, &nauw).weggelaten, 0);
    }

    /// Een kleine vaste "ruis" zonder extra afhankelijkheid.
    fn ruis(n: usize, zaad: u32) -> Vec<f32> {
        let mut toestand = zaad;
        (0..n)
            .map(|_| {
                toestand = toestand.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                #[allow(clippy::cast_precision_loss, reason = "testsignaal")]
                let waarde = (toestand >> 8) as f32 / (1u32 << 24) as f32;
                waarde * 2.0 - 1.0
            })
            .collect()
    }

    /// Bursts van ruis met onregelmatige stiltes: 16 kHz, `seconden` lang.
    fn bursts(seconden: usize) -> Vec<f32> {
        let mut uit = Vec::new();
        let lengtes = [300, 700, 450, 900, 250, 600, 1_100, 350];
        let mut nummer = 0;
        while uit.len() < seconden * 16_000 {
            let burst = lengtes[nummer % lengtes.len()] * 16;
            let stil = lengtes[(nummer + 3) % lengtes.len()] * 16;
            uit.extend(
                ruis(burst, u32::try_from(nummer).unwrap() + 1)
                    .iter()
                    .map(|s| s * 0.5),
            );
            uit.extend(std::iter::repeat_n(0.0, stil));
            nummer += 1;
        }
        uit.truncate(seconden * 16_000);
        uit
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "testsignaal binnen het bereik van een i16"
    )]
    fn naar_i16(samples: &[f32]) -> Vec<i16> {
        samples
            .iter()
            .map(|s| (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16)
            .collect()
    }

    /// Schrijft een spoor in stukken: per stuk de samples en het anker op de tijdlijn.
    fn schrijf_spoor(opname: &Path, naam: &str, stukken: &[(Vec<i16>, i64)]) {
        let map = opname.join(naam);
        fs::create_dir_all(&map).unwrap();
        let mut logboek = String::new();
        for (nummer, (samples, begin_ms)) in stukken.iter().enumerate() {
            let nummer = u32::try_from(nummer).unwrap();
            let bestand = stuknaam(nummer);
            let data = u32::try_from(samples.len() * 2).unwrap();
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"RIFF");
            bytes.extend_from_slice(&(data + 36).to_le_bytes());
            bytes.extend_from_slice(b"WAVEfmt ");
            bytes.extend_from_slice(&16u32.to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes());
            bytes.extend_from_slice(&16_000u32.to_le_bytes());
            bytes.extend_from_slice(&32_000u32.to_le_bytes());
            bytes.extend_from_slice(&2u16.to_le_bytes());
            bytes.extend_from_slice(&16u16.to_le_bytes());
            bytes.extend_from_slice(b"data");
            bytes.extend_from_slice(&data.to_le_bytes());
            for s in samples {
                bytes.extend_from_slice(&s.to_le_bytes());
            }
            fs::write(map.join(&bestand), bytes).unwrap();
            let info = StukInfo {
                stuk: nummer,
                bestand,
                samples: samples.len() as u64,
                begin_ms: *begin_ms,
                hersteld: false,
            };
            logboek.push_str(&serde_json::to_string(&info).unwrap());
            logboek.push('\n');
        }
        fs::write(map.join(LOGBOEK), logboek).unwrap();
        fs::write(opname.join(EINDE), "{}").unwrap();
    }

    #[test]
    fn bins_volgen_de_tijdlijn_ook_na_een_gat() {
        let opname = tempfile::tempdir().unwrap();
        // Stuk 1: 1 s op 0,5 amplitude; stuk 2 begint pas op 2 s (een gat van 1 s) op 0,25.
        let hard = vec![i16::MAX / 2; 16_000];
        let zacht = vec![i16::MAX / 4; 16_000];
        schrijf_spoor(opname.path(), SYSTEEM, &[(hard, 0), (zacht, 2_000)]);
        let rms = omhullende_van_spoor(&opname.path().join(SYSTEEM))
            .unwrap()
            .rms();
        assert_eq!(rms.len(), 300);
        assert!((rms[0] - 0.5).abs() < 0.001 && (rms[99] - 0.5).abs() < 0.001);
        // Het gat is stil.
        assert!(rms[100..200].iter().all(|w| *w == 0.0));
        assert!((rms[200] - 0.25).abs() < 0.001 && (rms[299] - 0.25).abs() < 0.001);
    }

    #[test]
    fn de_vertraging_van_de_microfoon_wordt_teruggevonden() {
        let opname = tempfile::tempdir().unwrap();
        let systeem = bursts(60);
        // De microfoon hoort 30 procent van het systeem, 120 ms later, plus een beetje ruis.
        let mut microfoon = vec![0.0; 120 * 16];
        microfoon.extend(systeem.iter().map(|s| s * 0.3));
        microfoon.truncate(systeem.len());
        for (m, r) in microfoon.iter_mut().zip(ruis(systeem.len(), 99)) {
            *m += r * 0.003;
        }
        schrijf_spoor(opname.path(), SYSTEEM, &[(naar_i16(&systeem), 0)]);
        schrijf_spoor(opname.path(), MICROFOON, &[(naar_i16(&microfoon), 0)]);
        let s = omhullende_van_spoor(&opname.path().join(SYSTEEM))
            .unwrap()
            .rms();
        let m = omhullende_van_spoor(&opname.path().join(MICROFOON))
            .unwrap()
            .rms();
        let vertraging = beste_vertraging(&s, &m).expect("verloop");
        assert_eq!(vertraging.ms, 120);
        assert!(vertraging.correlatie > 0.9, "{}", vertraging.correlatie);

        // De microfoon is bij luid systeemgeluid veel luider dan bij stil: 30 procent van
        // 0,5 amplitude is ruim boven de ruisvloer.
        let niveau = niveau_bij_systeem(&s, &m, 12);
        assert!(niveau.bij_luid.vensters > 10 && niveau.bij_stil.vensters > 10);
        let luid = niveau.bij_luid.mediaan_dbfs.expect("luid");
        let stil = niveau.bij_stil.mediaan_dbfs.expect("stil");
        assert!(luid > stil + 20.0, "{luid} tegenover {stil}");
        assert!(stil < -50.0, "{stil}");
    }

    #[test]
    fn zonder_verband_is_de_correlatie_laag_en_een_stil_spoor_geeft_niets() {
        let systeem: Vec<f32> = ruis(2_000, 5).iter().map(|s| s.abs()).collect();
        let andere: Vec<f32> = ruis(2_000, 77).iter().map(|s| s.abs()).collect();
        let v = beste_vertraging(&systeem, &andere).expect("verloop");
        assert!(v.correlatie < 0.2, "{}", v.correlatie);
        // Geen verloop, of te kort: geen uitkomst.
        assert_eq!(beste_vertraging(&systeem, &vec![0.0; 2_000]), None);
        assert_eq!(beste_vertraging(&systeem[..50], &andere[..50]), None);
        assert_eq!(beste_vertraging(&[], &[]), None);
    }

    #[test]
    fn niveaus_in_dbfs_en_lege_groepen() {
        // Systeem luid (0,5) in de eerste vensters en stil (0) in de volgende; de
        // microfoon is overal 0,01 (-40 dBFS).
        let mut systeem = vec![0.5; 5 * NIVEAU_VENSTER_BINS];
        systeem.extend(vec![0.0; 3 * NIVEAU_VENSTER_BINS]);
        let microfoon = vec![0.01; 8 * NIVEAU_VENSTER_BINS];
        let meting = niveau_bij_systeem(&systeem, &microfoon, 0);
        assert_eq!(meting.bij_luid.vensters, 5);
        assert_eq!(meting.bij_stil.vensters, 3);
        let db = meting.bij_luid.gemiddeld_dbfs.expect("luid");
        assert!((db + 40.0).abs() < 0.01, "{db}");
        assert!((dbfs_van_rms(0.5) + 6.02).abs() < 0.01);
        // Een venster tussen de drempels telt niet mee, en zonder vensters is er geen niveau.
        let midden = niveau_bij_systeem(&[0.003; 20], &[0.01; 20], 0);
        assert_eq!(midden.bij_luid.vensters + midden.bij_stil.vensters, 0);
        assert_eq!(midden.bij_luid.gemiddeld_dbfs, None);
        // Een vertraging die voorbij het eind van de microfoon reikt, telt die vensters niet.
        let kort = niveau_bij_systeem(&systeem, &microfoon[..40], 0);
        assert_eq!(kort.bij_luid.vensters, 4);
        // Stilte op de microfoon blijft op de vloer van -120 dBFS.
        let stil = niveau_bij_systeem(&systeem, &vec![0.0; 80], 0);
        assert_eq!(stil.bij_luid.gemiddeld_dbfs, Some(VLOER_DBFS));
    }

    #[test]
    fn de_bin_komt_overeen_met_de_samplerate() {
        assert_eq!(
            BIN_SAMPLES as u64,
            u64::from(crate::audio::DOEL_SAMPLERATE) * BIN_MS / 1000
        );
    }
}
