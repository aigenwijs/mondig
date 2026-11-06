//! Eén spoor van een vergaderopname, in stukken op schijf (ADR 0008; opzet in ADR 0013,
//! aanvulling 4b-A).
//!
//! Een spoor is een map met WAV-bestanden van een minuut (`00000.wav`, `00001.wav`, …),
//! 16 kHz mono, 16 bit, en een logboek `stukken.jsonl` met per afgesloten stuk één regel:
//! het aantal samples en waar het stuk begint op de gemeenschappelijke tijdlijn van de
//! opname (milliseconden sinds de start). Zo kan het uitschrijven (4b-B) de twee sporen in
//! tijdsvolgorde zetten, ook als de klok van een geluidskaart iets afwijkt: elk stuk krijgt
//! een vers anker.
//!
//! Het lopende stuk werkt zijn WAV-kop elke seconde bij. Valt het proces weg, dan staat er
//! hooguit de laatste seconde niet in de kop; [`herstel_spoor`] rekent de kop opnieuw uit
//! op de lengte van het bestand en vult het logboek aan.
//!
//! Geen geheugengroei: een blok gaat meteen naar het bestand, er wordt niets verzameld.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::audio::DOEL_SAMPLERATE;

/// Zo lang is één stuk. Een crash kost hooguit het deel van het laatste stuk dat nog niet
/// in de kop stond (een seconde); is ook dat stuk onleesbaar, dan hooguit een minuut.
pub const STUK_DUUR: Duration = Duration::from_mins(1);

/// Het logboek van een spoor.
pub const LOGBOEK: &str = "stukken.jsonl";

/// Een gat van meer dan dit op de tijdlijn wordt met stilte gevuld (alleen bij een bron die
/// zijn opnametijd meegeeft). Kleiner is gewone schommeling van de geluidskaart.
pub const GAT_DREMPEL: Duration = Duration::from_millis(40);

/// Onder dit piekniveau (-60 dBFS) heet een spoor stil.
pub const STIL_PIEK: f32 = 0.001;

/// Grootte van de WAV-kop.
const KOP: u64 = 44;

/// Zo vaak (in samples) wordt de kop van het lopende stuk bijgewerkt: elke seconde.
const VERVERS_ELKE: u64 = DOEL_SAMPLERATE as u64;

/// Blok nullen om stilte mee te schrijven, zonder groot geheugen.
const NULLEN: [f32; 4096] = [0.0; 4096];

/// Eén regel in het logboek: een afgesloten stuk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StukInfo {
    /// Volgnummer, vanaf 0.
    pub stuk: u32,
    /// Bestandsnaam in de map van het spoor.
    pub bestand: String,
    /// Aantal samples (16 kHz).
    pub samples: u64,
    /// Waar het eerste sample op de tijdlijn valt, in milliseconden sinds de start.
    pub begin_ms: i64,
    /// Is dit stuk na een afgebroken opname hersteld (en het anker dus geschat)?
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hersteld: bool,
}

/// De bestandsnaam van stuk `nummer`.
#[must_use]
pub fn stuknaam(nummer: u32) -> String {
    format!("{nummer:05}.wav")
}

fn ms(duur: Duration) -> i64 {
    i64::try_from((duur.as_micros() + 500) / 1000).unwrap_or(i64::MAX)
}

/// Hoeveel milliseconden `samples` samples (16 kHz) duren.
#[must_use]
pub fn samples_naar_ms(samples: u64) -> i64 {
    i64::try_from(samples * 1000 / u64::from(DOEL_SAMPLERATE)).unwrap_or(i64::MAX)
}

fn samples_in(duur: Duration) -> u64 {
    u64::try_from((duur.as_micros() * u128::from(DOEL_SAMPLERATE) + 500_000) / 1_000_000)
        .unwrap_or(u64::MAX)
}

/// De WAV-kop voor `data` bytes aan 16-bit mono-samples op 16 kHz.
pub(crate) fn kop(data: u32) -> [u8; 44] {
    let mut kop = [0u8; 44];
    let samplerate = DOEL_SAMPLERATE;
    kop[0..4].copy_from_slice(b"RIFF");
    kop[4..8].copy_from_slice(&data.saturating_add(36).to_le_bytes());
    kop[8..12].copy_from_slice(b"WAVE");
    kop[12..16].copy_from_slice(b"fmt ");
    kop[16..20].copy_from_slice(&16u32.to_le_bytes());
    kop[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    kop[22..24].copy_from_slice(&1u16.to_le_bytes()); // mono
    kop[24..28].copy_from_slice(&samplerate.to_le_bytes());
    kop[28..32].copy_from_slice(&(samplerate * 2).to_le_bytes());
    kop[32..34].copy_from_slice(&2u16.to_le_bytes());
    kop[34..36].copy_from_slice(&16u16.to_le_bytes());
    kop[36..40].copy_from_slice(b"data");
    kop[40..44].copy_from_slice(&data.to_le_bytes());
    kop
}

/// Een stuk dat open staat.
struct WavStuk {
    bestand: BufWriter<File>,
    samples: u64,
    sinds_verversen: u64,
    /// Herbruikbare buffer om een blok in één keer als bytes weg te schrijven.
    bytes: Vec<u8>,
}

impl WavStuk {
    fn maak(pad: &Path) -> io::Result<Self> {
        let bestand = OpenOptions::new().write(true).create_new(true).open(pad)?;
        let mut bestand = BufWriter::with_capacity(64 * 1024, bestand);
        bestand.write_all(&kop(0))?;
        Ok(Self {
            bestand,
            samples: 0,
            sinds_verversen: 0,
            bytes: Vec::new(),
        })
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "na het begrenzen en schalen past het in een i16"
    )]
    fn schrijf(&mut self, samples: &[f32]) -> io::Result<()> {
        self.bytes.clear();
        self.bytes.reserve(samples.len() * 2);
        for &sample in samples {
            let waarde = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
            self.bytes.extend_from_slice(&waarde.to_le_bytes());
        }
        self.bestand.write_all(&self.bytes)?;
        let aantal = samples.len() as u64;
        self.samples += aantal;
        self.sinds_verversen += aantal;
        if self.sinds_verversen >= VERVERS_ELKE {
            self.ververs()?;
        }
        Ok(())
    }

    /// Schrijft de buffer weg en zet de lengte in de kop. Daarna staat alles wat tot nu toe
    /// binnen is in de bestandscache van het systeem, en overleeft het een crash van Mondig.
    fn ververs(&mut self) -> io::Result<()> {
        self.sinds_verversen = 0;
        let data = u32::try_from(self.samples * 2).unwrap_or(u32::MAX);
        let kop = kop(data);
        self.bestand.flush()?;
        let bestand = self.bestand.get_mut();
        bestand.seek(SeekFrom::Start(4))?;
        bestand.write_all(&kop[4..8])?;
        bestand.seek(SeekFrom::Start(40))?;
        bestand.write_all(&kop[40..44])?;
        bestand.seek(SeekFrom::End(0))?;
        Ok(())
    }

    /// Sluit het stuk af en zet het ook op de schijf zelf (tegen stroomuitval).
    fn sluit(mut self) -> io::Result<u64> {
        self.ververs()?;
        self.bestand.get_ref().sync_data()?;
        Ok(self.samples)
    }
}

/// Zet de kop van een WAV-bestand van Mondig gelijk aan de lengte van het bestand. Geeft
/// het aantal samples, of `None` als het geen bruikbaar stuk is (te kort of geen kop van
/// ons).
///
/// # Errors
///
/// Geeft een I/O-fout als het bestand niet te lezen of te schrijven is.
pub fn herstel_wav(pad: &Path) -> io::Result<Option<u64>> {
    let mut bestand = OpenOptions::new().read(true).write(true).open(pad)?;
    let lengte = bestand.metadata()?.len();
    if lengte < KOP {
        return Ok(None);
    }
    let mut gelezen = [0u8; 44];
    bestand.read_exact(&mut gelezen)?;
    // Alleen een kop zoals wij die schrijven; de lengtes mogen afwijken.
    let verwacht = kop(0);
    let gelijk = |bereik: std::ops::Range<usize>| gelezen[bereik.clone()] == verwacht[bereik];
    if !(gelijk(0..4) && gelijk(8..40)) {
        return Ok(None);
    }
    // Een half geschreven sample valt weg.
    let data = (lengte - KOP) & !1;
    let data32 = u32::try_from(data).unwrap_or(u32::MAX - 1);
    bestand.set_len(KOP + u64::from(data32))?;
    let nieuw = kop(data32);
    bestand.seek(SeekFrom::Start(0))?;
    bestand.write_all(&nieuw)?;
    bestand.sync_data()?;
    Ok(Some(u64::from(data32) / 2))
}

/// Wat een spoor heeft opgeleverd.
#[derive(Debug, Clone, PartialEq)]
pub struct SpoorUitslag {
    /// De map met de stukken.
    pub map: PathBuf,
    /// Aantal stukken.
    pub stukken: u32,
    /// Alle samples, inclusief opgevulde stilte.
    pub samples: u64,
    /// Samples stilte die Mondig zelf invulde omdat de bron niets leverde (bij
    /// systeemgeluid: er speelde niets).
    pub opgevuld: u64,
    /// Het hoogste niveau (0 tot 1) van wat de bron leverde.
    pub piek: f32,
}

impl SpoorUitslag {
    /// De lengte van het spoor.
    #[must_use]
    pub fn duur(&self) -> Duration {
        Duration::from_micros(self.samples * 1_000_000 / u64::from(DOEL_SAMPLERATE))
    }

    /// Was er op dit spoor niets te horen?
    #[must_use]
    pub fn stil(&self) -> bool {
        self.piek < STIL_PIEK
    }
}

/// Schrijft één spoor weg, blok voor blok. Puur rekenwerk en bestanden, geen threads: de
/// opnamelaag roept dit aan vanaf een eigen schrijfthread.
pub struct SpoorSchrijver {
    map: PathBuf,
    begin: Instant,
    stuk_samples: u64,
    opvullen: bool,
    logboek: File,
    huidig: Option<WavStuk>,
    nummer: u32,
    /// Positie op het spoor waar het lopende stuk begon.
    stuk_begin: u64,
    /// Alle samples tot nu toe.
    totaal: u64,
    /// Anker van het lopende stuk volgens de opnametijd van de bron (exact).
    anker_bron: Option<i64>,
    /// Anker volgens de aankomsttijd van de blokken (de vroegste schatting telt).
    anker_aankomst: Option<i64>,
    opgevuld: u64,
    piek: f32,
}

impl SpoorSchrijver {
    /// Een nieuw spoor in `map`. `begin` is het nulpunt van de tijdlijn. Met `opvullen`
    /// vult de schrijver gaten op de tijdlijn met stilte (voor een bron die zijn opnametijd
    /// meegeeft en bij stilte niets levert, zoals systeemgeluid op Windows), van het begin
    /// tot het einde van de opname.
    ///
    /// # Errors
    ///
    /// Geeft een I/O-fout als de map of het logboek niet te maken is.
    pub fn nieuw(map: &Path, begin: Instant, opvullen: bool, stuk: Duration) -> io::Result<Self> {
        fs::create_dir_all(map)?;
        let logboek = OpenOptions::new()
            .create(true)
            .append(true)
            .open(map.join(LOGBOEK))?;
        Ok(Self {
            map: map.to_path_buf(),
            begin,
            stuk_samples: samples_in(stuk).max(1),
            opvullen,
            logboek,
            huidig: None,
            nummer: 0,
            stuk_begin: 0,
            totaal: 0,
            anker_bron: None,
            anker_aankomst: None,
            opgevuld: 0,
            piek: 0.0,
        })
    }

    /// Hoeveel samples er tot nu toe geschreven zijn.
    #[must_use]
    pub fn totaal(&self) -> u64 {
        self.totaal
    }

    /// Neemt een blok aan. `aangekomen` is wanneer het blok binnenkwam (na het laatste
    /// sample); `opgenomen` is, als de bron dat weet, wanneer het eerste sample werd
    /// opgenomen.
    ///
    /// # Errors
    ///
    /// Geeft een I/O-fout als er niet geschreven kan worden (schijf vol).
    pub fn blok(
        &mut self,
        samples: &[f32],
        aangekomen: Instant,
        opgenomen: Option<Instant>,
    ) -> io::Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        if self.opvullen
            && let Some(op) = opgenomen
        {
            let werkelijk = op.saturating_duration_since(self.begin);
            let verwacht =
                Duration::from_micros(self.totaal * 1_000_000 / u64::from(DOEL_SAMPLERATE));
            if werkelijk > verwacht + GAT_DREMPEL {
                self.vul(samples_in(werkelijk.saturating_sub(verwacht)))?;
            }
        }
        self.piek = samples.iter().fold(self.piek, |m, s| m.max(s.abs()));
        let lengte = samples.len() as u64;
        let mut gedaan = 0usize;
        while gedaan < samples.len() {
            let in_stuk = self.totaal - self.stuk_begin;
            let ruimte = usize::try_from(self.stuk_samples - in_stuk).unwrap_or(usize::MAX);
            let stukje = &samples[gedaan..samples.len().min(gedaan + ruimte)];
            // Waar het lopende stuk begint, gezien vanuit dit stukje.
            let voor = samples_naar_ms(in_stuk);
            if let Some(op) = opgenomen {
                let anker = ms(op.saturating_duration_since(self.begin))
                    + samples_naar_ms(gedaan as u64)
                    - voor;
                self.anker_bron.get_or_insert(anker);
            }
            let tot_einde = lengte - gedaan as u64;
            let anker = ms(aangekomen.saturating_duration_since(self.begin))
                - samples_naar_ms(tot_einde)
                - voor;
            self.anker_aankomst = Some(self.anker_aankomst.map_or(anker, |a| a.min(anker)));
            self.schrijf(stukje)?;
            gedaan += stukje.len();
        }
        Ok(())
    }

    /// Schrijft `aantal` samples stilte, over de grenzen van stukken heen. Opgevulde
    /// stilte ligt per definitie op de tijdlijn, dus die geeft een exact anker.
    fn vul(&mut self, mut aantal: u64) -> io::Result<()> {
        self.opgevuld += aantal;
        while aantal > 0 {
            let in_stuk = self.totaal - self.stuk_begin;
            let ruimte = (self.stuk_samples - in_stuk).min(NULLEN.len() as u64);
            let nu = aantal.min(ruimte);
            self.anker_bron
                .get_or_insert(samples_naar_ms(self.stuk_begin));
            self.schrijf(&NULLEN[..usize::try_from(nu).unwrap_or(NULLEN.len())])?;
            aantal -= nu;
        }
        Ok(())
    }

    /// Schrijft binnen het lopende stuk (`samples` past erin) en sluit het als het vol is.
    fn schrijf(&mut self, samples: &[f32]) -> io::Result<()> {
        if self.huidig.is_none() {
            self.huidig = Some(WavStuk::maak(&self.map.join(stuknaam(self.nummer)))?);
        }
        if let Some(stuk) = self.huidig.as_mut() {
            stuk.schrijf(samples)?;
        }
        self.totaal += samples.len() as u64;
        if self.totaal - self.stuk_begin >= self.stuk_samples {
            self.sluit_stuk()?;
        }
        Ok(())
    }

    fn sluit_stuk(&mut self) -> io::Result<()> {
        let Some(stuk) = self.huidig.take() else {
            return Ok(());
        };
        let samples = stuk.sluit()?;
        let begin_ms = self
            .anker_bron
            .or(self.anker_aankomst)
            .unwrap_or_else(|| samples_naar_ms(self.stuk_begin));
        let info = StukInfo {
            stuk: self.nummer,
            bestand: stuknaam(self.nummer),
            samples,
            begin_ms,
            hersteld: false,
        };
        schrijf_regel(&mut self.logboek, &info)?;
        self.nummer += 1;
        self.stuk_begin = self.totaal;
        self.anker_bron = None;
        self.anker_aankomst = None;
        Ok(())
    }

    /// Sluit het spoor af. Met `opvullen` wordt het spoor tot `tot` met stilte aangevuld,
    /// zodat het even lang is als de opname.
    ///
    /// # Errors
    ///
    /// Geeft een I/O-fout als het laatste stuk niet weg te schrijven is.
    pub fn sluit(mut self, tot: Option<Instant>) -> io::Result<SpoorUitslag> {
        if self.opvullen
            && let Some(tot) = tot
        {
            let doel = samples_in(tot.saturating_duration_since(self.begin));
            if doel > self.totaal {
                self.vul(doel - self.totaal)?;
            }
        }
        self.sluit_stuk()?;
        Ok(SpoorUitslag {
            map: self.map.clone(),
            stukken: self.nummer,
            samples: self.totaal,
            opgevuld: self.opgevuld,
            piek: self.piek,
        })
    }
}

fn schrijf_regel(logboek: &mut File, info: &StukInfo) -> io::Result<()> {
    let mut regel = serde_json::to_string(info).map_err(io::Error::other)?;
    regel.push('\n');
    logboek.write_all(regel.as_bytes())?;
    logboek.sync_data()
}

/// Het logboek als tekst: één regel per stuk.
fn logboek_tekst(stukken: &[StukInfo]) -> io::Result<String> {
    let mut tekst = String::new();
    for info in stukken {
        tekst.push_str(&serde_json::to_string(info).map_err(io::Error::other)?);
        tekst.push('\n');
    }
    Ok(tekst)
}

/// Schrijft een bestand in één keer en alles-of-niets: eerst naar een tijdelijk bestand
/// naast het doel, dat naar schijf wordt gezet, dan hernoemd. Een crash laat dus het oude
/// of het nieuwe bestand achter, nooit een half.
///
/// # Errors
///
/// Geeft een I/O-fout als het tijdelijke bestand niet te maken of te hernoemen is.
pub(crate) fn schrijf_atomair(pad: &Path, inhoud: &[u8]) -> io::Result<()> {
    let mut naam = pad.file_name().unwrap_or_default().to_os_string();
    naam.push(".nieuw");
    let tijdelijk = pad.with_file_name(naam);
    let schrijf = || -> io::Result<()> {
        let mut bestand = File::create(&tijdelijk)?;
        bestand.write_all(inhoud)?;
        bestand.sync_all()?;
        drop(bestand);
        fs::rename(&tijdelijk, pad)
    };
    if let Err(fout) = schrijf() {
        // Geen half tijdelijk bestand laten liggen.
        let _ = fs::remove_file(&tijdelijk);
        return Err(fout);
    }
    // De hernoeming zelf ook naar schijf, waar het besturingssysteem dat toestaat
    // (een map openen om te syncen kan niet op Windows).
    #[cfg(not(windows))]
    if let Some(map) = pad.parent()
        && let Ok(map) = File::open(map)
    {
        let _ = map.sync_all();
    }
    Ok(())
}

/// Leest het logboek van een spoor. Een half geschreven laatste regel (crash tijdens het
/// schrijven) telt niet mee.
///
/// # Errors
///
/// Geeft een I/O-fout als het logboek er wel is maar niet te lezen.
pub fn lees_logboek(map: &Path) -> io::Result<Vec<StukInfo>> {
    Ok(parse_logboek(&lees_logboek_bytes(map)?))
}

/// De ruwe inhoud van het logboek; een ontbrekend logboek is leeg.
fn lees_logboek_bytes(map: &Path) -> io::Result<Vec<u8>> {
    match fs::read(map.join(LOGBOEK)) {
        Ok(bytes) => Ok(bytes),
        Err(fout) if fout.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(fout) => Err(fout),
    }
}

/// De hele regels van een logboek; vanaf de eerste onleesbare regel (een half geschreven
/// regel na een crash) telt niets meer mee.
fn parse_logboek(bytes: &[u8]) -> Vec<StukInfo> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map_while(|regel| serde_json::from_str::<StukInfo>(regel).ok())
        .collect()
}

/// Wat het herstel van een spoor opleverde.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpoorHersteld {
    /// Alle stukken na het herstel, op volgorde.
    pub stukken: Vec<StukInfo>,
    /// Hoeveel stukken er hersteld zijn (meestal één: het laatste).
    pub hersteld: u32,
    /// Hoeveel onbruikbare bestanden zijn weggegooid (leeg of zonder kop).
    pub weggegooid: u32,
}

impl SpoorHersteld {
    /// Alle samples van het spoor.
    #[must_use]
    pub fn samples(&self) -> u64 {
        self.stukken.iter().map(|s| s.samples).sum()
    }

    /// Waar het spoor ophoudt op de tijdlijn, in milliseconden.
    #[must_use]
    pub fn eind_ms(&self) -> i64 {
        self.stukken
            .last()
            .map_or(0, |s| s.begin_ms + samples_naar_ms(s.samples))
    }
}

/// Maakt een spoor van een afgebroken opname weer leesbaar: elk stuk dat nog niet in het
/// logboek staat krijgt een kloppende kop en een regel, met een anker direct na het vorige
/// stuk. Een half geschreven regel in het logboek wordt eerst weggehaald.
///
/// # Errors
///
/// Geeft een I/O-fout als de map niet te lezen of te schrijven is.
pub fn herstel_spoor(map: &Path) -> io::Result<SpoorHersteld> {
    let pad = map.join(LOGBOEK);
    let huidig = lees_logboek_bytes(map)?;
    let mut stukken = parse_logboek(&huidig);
    // Het logboek alleen opnieuw schrijven als er iets af te halen valt (een half geschreven
    // regel), en dan via een tijdelijk bestand: een crash of volle schijf halverwege mag de
    // exacte ankers nooit kwijtmaken.
    let schoon = logboek_tekst(&stukken)?;
    if huidig != schoon.as_bytes() {
        schrijf_atomair(&pad, schoon.as_bytes())?;
    }
    let mut logboek = OpenOptions::new().create(true).append(true).open(&pad)?;
    let mut namen: Vec<String> = fs::read_dir(map)?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| {
            Path::new(n)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
        })
        .collect();
    namen.sort();
    let mut hersteld = 0;
    let mut weggegooid = 0;
    for naam in namen {
        if stukken.iter().any(|s| s.bestand == naam) {
            continue;
        }
        let pad = map.join(&naam);
        match herstel_wav(&pad)? {
            Some(samples) if samples > 0 => {
                let begin_ms = stukken
                    .last()
                    .map_or(0, |s| s.begin_ms + samples_naar_ms(s.samples));
                let info = StukInfo {
                    stuk: stukken.last().map_or(0, |s| s.stuk + 1),
                    bestand: naam,
                    samples,
                    begin_ms,
                    hersteld: true,
                };
                schrijf_regel(&mut logboek, &info)?;
                stukken.push(info);
                hersteld += 1;
            }
            _ => {
                fs::remove_file(&pad)?;
                weggegooid += 1;
            }
        }
    }
    logboek.sync_data()?;
    Ok(SpoorHersteld {
        stukken,
        hersteld,
        weggegooid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lees_samples(pad: &Path) -> Vec<i16> {
        let mut lezer = hound::WavReader::open(pad).unwrap();
        assert_eq!(lezer.spec().sample_rate, DOEL_SAMPLERATE);
        assert_eq!(lezer.spec().channels, 1);
        lezer.samples::<i16>().map(Result::unwrap).collect()
    }

    fn t(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn stukken_worden_op_de_grens_gesneden_en_zijn_leesbare_wav() {
        let map = tempfile::tempdir().unwrap();
        let begin = Instant::now();
        let mut s = SpoorSchrijver::nieuw(map.path(), begin, false, t(1000)).unwrap();
        // 2,5 seconde in blokken van 30 ms.
        let blok = vec![0.5_f32; 480];
        for i in 1..=83u32 {
            s.blok(&blok, begin + t(u64::from(i) * 30), None).unwrap();
        }
        let uitslag = s.sluit(None).unwrap();
        assert_eq!(uitslag.samples, 83 * 480);
        assert_eq!(uitslag.stukken, 3);
        let logboek = lees_logboek(map.path()).unwrap();
        assert_eq!(logboek.len(), 3);
        assert_eq!(logboek[0].samples, 16_000);
        assert_eq!(logboek[2].samples, 83 * 480 - 32_000);
        let samples = lees_samples(&map.path().join("00000.wav"));
        assert_eq!(samples.len(), 16_000);
        assert!(samples.iter().all(|&v| v == 16_384));
        assert!(!uitslag.stil());
    }

    #[test]
    fn anker_volgens_aankomst_is_de_vroegste_schatting() {
        let map = tempfile::tempdir().unwrap();
        let begin = Instant::now();
        let mut s = SpoorSchrijver::nieuw(map.path(), begin, false, t(60_000)).unwrap();
        let blok = vec![0.1_f32; 480]; // 30 ms
        // Het eerste sample werd op 200 ms opgenomen; blokken komen 15 tot 50 ms na hun
        // laatste sample binnen.
        let vertragingen = [50, 15, 40, 20];
        for (i, v) in vertragingen.iter().enumerate() {
            let eind = 200 + 30 * (i as u64 + 1);
            s.blok(&blok, begin + t(eind + v), None).unwrap();
        }
        s.sluit(None).unwrap();
        let logboek = lees_logboek(map.path()).unwrap();
        assert_eq!(logboek[0].begin_ms, 215);
    }

    #[test]
    fn bron_met_opnametijd_krijgt_stilte_in_de_gaten_en_tot_het_einde() {
        let map = tempfile::tempdir().unwrap();
        let begin = Instant::now();
        let mut s = SpoorSchrijver::nieuw(map.path(), begin, true, t(1000)).unwrap();
        let blok = vec![0.25_f32; 1600]; // 100 ms
        // Eerste geluid na 1,5 s, dan een gat van 2 s, dan stilte tot 6 s.
        s.blok(&blok, begin + t(1600), Some(begin + t(1500)))
            .unwrap();
        s.blok(&blok, begin + t(3700), Some(begin + t(3600)))
            .unwrap();
        // Kleine schommeling: geen opvulling.
        s.blok(&blok, begin + t(3800), Some(begin + t(3720)))
            .unwrap();
        let uitslag = s.sluit(Some(begin + t(6000))).unwrap();
        assert_eq!(uitslag.samples, 6 * 16_000);
        assert_eq!(uitslag.opgevuld, 6 * 16_000 - 3 * 1600);
        let logboek = lees_logboek(map.path()).unwrap();
        let ankers: Vec<i64> = logboek.iter().map(|s| s.begin_ms).collect();
        assert_eq!(ankers, vec![0, 1000, 2000, 3000, 4000, 5000]);
        // Het geluid staat op de juiste plek in het tweede stuk: vanaf 0,5 s.
        let tweede = lees_samples(&map.path().join("00001.wav"));
        assert_eq!(tweede[7_999], 0);
        assert_eq!(tweede[8_000], 8_192);
        // Het derde blok staat direct na het tweede, op 3,7 s.
        let vierde = lees_samples(&map.path().join("00003.wav"));
        assert_eq!(vierde[9_599], 0);
        assert_eq!(vierde[9_600], 8_192);
        assert_eq!(vierde[12_799], 8_192);
        assert_eq!(vierde[12_800], 0);
    }

    #[test]
    fn helemaal_stil_systeemspoor_wordt_een_stil_spoor_van_de_volle_lengte() {
        let map = tempfile::tempdir().unwrap();
        let begin = Instant::now();
        let s = SpoorSchrijver::nieuw(map.path(), begin, true, t(60_000)).unwrap();
        let uitslag = s.sluit(Some(begin + t(2000))).unwrap();
        assert_eq!(uitslag.samples, 32_000);
        assert!(uitslag.stil());
        assert_eq!(lees_samples(&map.path().join("00000.wav")).len(), 32_000);
    }

    #[test]
    fn exact_anker_van_de_bron_ook_midden_in_een_blok() {
        let map = tempfile::tempdir().unwrap();
        let begin = Instant::now();
        let mut s = SpoorSchrijver::nieuw(map.path(), begin, false, t(1000)).unwrap();
        // Blokken van 0,6 s met opnametijd 10 ms later dan de tijdlijn (klok loopt uit).
        let blok = vec![0.1_f32; 9_600];
        for i in 0..4u32 {
            let op = u64::from(i) * 600 + 10;
            s.blok(&blok, begin + t(op + 700), Some(begin + t(op)))
                .unwrap();
        }
        s.sluit(None).unwrap();
        let ankers: Vec<i64> = lees_logboek(map.path())
            .unwrap()
            .iter()
            .map(|s| s.begin_ms)
            .collect();
        // Stuk 1 begint 0,4 s in het tweede blok (opgenomen op 0,61 s): 1,01 s.
        assert_eq!(ankers, vec![10, 1010, 2010]);
    }

    #[test]
    fn afgebroken_stuk_wordt_hersteld_tot_de_laatste_bijgewerkte_kop() {
        let map = tempfile::tempdir().unwrap();
        let begin = Instant::now();
        let mut s = SpoorSchrijver::nieuw(map.path(), begin, false, t(2000)).unwrap();
        let blok = vec![0.5_f32; 1600];
        // 3 seconden: stuk 0 af, stuk 1 half.
        for i in 1..=30u32 {
            s.blok(&blok, begin + t(u64::from(i) * 100 + 20), None)
                .unwrap();
        }
        // Harde stop: de schrijver verdwijnt zonder af te sluiten. De buffer gaat verloren
        // zoals bij een crash (std::mem::forget laat geen Drop lopen).
        std::mem::forget(s);
        // Een half geschreven logboekregel erachter.
        let mut logboek = OpenOptions::new()
            .append(true)
            .open(map.path().join(LOGBOEK))
            .unwrap();
        logboek.write_all(b"{\"stuk\":1,\"bes").unwrap();
        drop(logboek);
        // Een leeg derde bestand, alsof het net was aangemaakt.
        fs::write(map.path().join("00002.wav"), b"").unwrap();

        let hersteld = herstel_spoor(map.path()).unwrap();
        assert_eq!(hersteld.hersteld, 1);
        assert_eq!(hersteld.weggegooid, 1);
        assert_eq!(hersteld.stukken.len(), 2);
        let tweede = &hersteld.stukken[1];
        assert!(tweede.hersteld);
        assert_eq!(tweede.begin_ms, hersteld.stukken[0].begin_ms + 2000);
        // De kop werd na 1 s van het tweede stuk bijgewerkt; wat in het bestand staat
        // (minstens die seconde) is terug.
        assert!(tweede.samples >= 16_000, "{}", tweede.samples);
        let samples = lees_samples(&map.path().join("00001.wav"));
        assert_eq!(samples.len() as u64, tweede.samples);
        assert!(!map.path().join("00002.wav").exists());
        // Nog een keer herstellen verandert niets.
        let opnieuw = herstel_spoor(map.path()).unwrap();
        assert_eq!(opnieuw.hersteld, 0);
        assert_eq!(opnieuw.stukken, hersteld.stukken);
    }

    /// Een spoor met twee afgesloten stukken en een half geschreven regel erachter.
    fn spoor_met_halve_regel(map: &Path) -> String {
        let begin = Instant::now();
        let mut s = SpoorSchrijver::nieuw(map, begin, false, t(1000)).unwrap();
        let blok = vec![0.5_f32; 1600];
        for i in 1..=20u32 {
            s.blok(&blok, begin + t(u64::from(i) * 100), None).unwrap();
        }
        s.sluit(None).unwrap();
        let schoon = fs::read_to_string(map.join(LOGBOEK)).unwrap();
        let mut logboek = OpenOptions::new()
            .append(true)
            .open(map.join(LOGBOEK))
            .unwrap();
        logboek.write_all(b"{\"stuk\":2,\"bes").unwrap();
        schoon
    }

    #[test]
    fn logboek_wordt_via_een_tijdelijk_bestand_vervangen() {
        let map = tempfile::tempdir().unwrap();
        let schoon = spoor_met_halve_regel(map.path());
        herstel_spoor(map.path()).unwrap();
        assert_eq!(
            fs::read_to_string(map.path().join(LOGBOEK)).unwrap(),
            schoon
        );
        assert!(!map.path().join("stukken.jsonl.nieuw").exists());
    }

    #[test]
    fn mislukt_het_herschrijven_dan_blijft_het_oude_logboek_staan() {
        let map = tempfile::tempdir().unwrap();
        spoor_met_halve_regel(map.path());
        let voor = fs::read(map.path().join(LOGBOEK)).unwrap();
        // Het tijdelijke bestand kan niet gemaakt worden: er staat een map op die naam.
        fs::create_dir(map.path().join("stukken.jsonl.nieuw")).unwrap();
        assert!(herstel_spoor(map.path()).is_err());
        assert_eq!(fs::read(map.path().join(LOGBOEK)).unwrap(), voor);
    }

    #[test]
    fn een_heel_logboek_wordt_niet_herschreven() {
        let map = tempfile::tempdir().unwrap();
        let begin = Instant::now();
        let mut s = SpoorSchrijver::nieuw(map.path(), begin, false, t(1000)).unwrap();
        let blok = vec![0.5_f32; 1600];
        for i in 1..=20u32 {
            s.blok(&blok, begin + t(u64::from(i) * 100), None).unwrap();
        }
        s.sluit(None).unwrap();
        // Zou het logboek herschreven worden, dan faalt dat op deze map.
        fs::create_dir(map.path().join("stukken.jsonl.nieuw")).unwrap();
        let hersteld = herstel_spoor(map.path()).unwrap();
        assert_eq!(hersteld.stukken.len(), 2);
    }

    #[test]
    fn schrijven_van_een_blok_geeft_dezelfde_bytes_als_sample_voor_sample() {
        let map = tempfile::tempdir().unwrap();
        let mut stuk = WavStuk::maak(&map.path().join("a.wav")).unwrap();
        let blok: Vec<f32> = (0..1000_i16)
            .map(|i| (f32::from(i) / 500.0) - 1.0)
            .collect();
        stuk.schrijf(&blok).unwrap();
        stuk.schrijf(&blok[..10]).unwrap();
        assert_eq!(stuk.sluit().unwrap(), 1010);
        let samples = lees_samples(&map.path().join("a.wav"));
        assert_eq!(samples.len(), 1010);
        assert_eq!(samples[0], -32767);
        assert_eq!(samples[1000], -32767);
    }

    #[test]
    fn mislukt_atomair_schrijven_laat_geen_tijdelijk_bestand_achter() {
        let map = tempfile::tempdir().unwrap();
        // Het doel is een map: de hernoeming faalt.
        let doel = map.path().join("doel");
        fs::create_dir(&doel).unwrap();
        assert!(schrijf_atomair(&doel, b"x").is_err());
        assert!(!map.path().join("doel.nieuw").exists());
        // En gewoon schrijven werkt en laat ook niets achter.
        let bestand = map.path().join("bestand");
        schrijf_atomair(&bestand, b"hallo").unwrap();
        assert_eq!(fs::read(&bestand).unwrap(), b"hallo");
        assert!(!map.path().join("bestand.nieuw").exists());
    }

    #[test]
    fn herstel_wav_weigert_een_vreemd_bestand() {
        let map = tempfile::tempdir().unwrap();
        let pad = map.path().join("vreemd.wav");
        fs::write(&pad, vec![7u8; 100]).unwrap();
        assert_eq!(herstel_wav(&pad).unwrap(), None);
    }
}
