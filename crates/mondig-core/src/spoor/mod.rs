//! De sporen van een vergaderopname op schijf: het formaat (module [`stukken`]), de
//! afsluiting in `einde.json`, en het teruglezen van een spoor als één doorlopende stroom
//! (ADR 0008; opzet in ADR 0013, aanvulling 4b-A).
//!
//! Deze module staat los van de feature `opname`: het werkproces (`mondig-engine`) leest de
//! sporen ook, maar neemt zelf niets op en hoort geen geluidskaart te laden.
//!
//! Een spoor bestaat uit WAV-stukken van een minuut met elk een anker op de gezamenlijke
//! tijdlijn van de opname. [`Spoorlezer`] leest die stukken achter elkaar als één stroom,
//! zodat een woord op een minuutgrens heel blijft, en houdt daarbij nooit meer dan het
//! gevraagde blok in het geheugen. [`Tijdlijn`] rekent een plek in die stroom (een
//! sample-index) terug naar milliseconden op de tijdlijn, met het anker van het stuk waarin
//! die plek valt.

pub mod mengen;
pub mod stukken;
pub mod uitingen;

use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use stukken::{StukInfo, samples_naar_ms};

/// Map van het microfoonspoor ("Jij").
pub const MICROFOON: &str = "microfoon";
/// Map van het systeemspoor ("Anderen").
pub const SYSTEEM: &str = "systeem";
/// Het bestand dat er pas is als de opname netjes is afgesloten of hersteld.
pub const EINDE: &str = "einde.json";
/// Het bestand met de opzet van de opname: begin, samplerate en de soort vergadering.
pub const OPZET: &str = "opname.json";

/// Wat voor vergadering er is opgenomen (besluit, 2-11-2026). Staat in `opname.json`;
/// een opname van voor dit veld is [`Vergadersoort::Online`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vergadersoort {
    /// Teams, Zoom of Meet: de microfoon ("Jij") en het geluid van de computer ("Anderen")
    /// apart, met ontdubbelen.
    #[default]
    Online,
    /// Iedereen aan tafel: alleen de microfoon, zonder sprekers in het verslag.
    Ruimte,
    /// Alleen ik (besluit, 2-11-2026): je eigen gedachten inspreken. Gaat als
    /// [`Vergadersoort::Ruimte`] (alleen de microfoon, alinea's zonder sprekers), maar het
    /// verslag heet een notitie.
    Alleen,
}

impl Vergadersoort {
    /// Wordt ook het geluid van de computer opgenomen? Alleen online.
    #[must_use]
    pub fn met_systeem(self) -> bool {
        self == Self::Online
    }

    /// Staan er sprekers in het verslag ("Jij", "Anderen" of een naam)? Alleen online; de
    /// andere soorten hebben alleen de microfoon en dus alinea's.
    #[must_use]
    pub fn met_sprekers(self) -> bool {
        self == Self::Online
    }

    /// Het eerste woord van de titel van het verslag: "Vergadering" of, bij Alleen ik,
    /// "Notitie". Komt ook in de bestandsnaam.
    #[must_use]
    pub fn titelwoord(self) -> &'static str {
        match self {
            Self::Online | Self::Ruimte => "Vergadering",
            Self::Alleen => "Notitie",
        }
    }
}

/// De soort van een opname volgens `opname.json`. Ontbreekt het bestand of het veld, of is
/// het niet te lezen, dan [`Vergadersoort::Online`]: zo gedroeg elke opname zich voor er
/// soorten waren.
#[must_use]
pub fn lees_soort(opnamemap: &Path) -> Vergadersoort {
    #[derive(Deserialize)]
    struct AlleenSoort {
        #[serde(default)]
        soort: Vergadersoort,
    }
    fs::read_to_string(opnamemap.join(OPZET))
        .ok()
        .and_then(|tekst| serde_json::from_str::<AlleenSoort>(&tekst).ok())
        .map(|a| a.soort)
        .unwrap_or_default()
}

/// De melding als het systeemspoor stil bleef.
pub const MELDING_STIL_SYSTEEM: &str = "Er was geen geluid van de computer te horen. Speelde de vergadering via een ander apparaat dan de standaardluidspreker?";

/// Grootte van de WAV-kop van een stuk.
const KOP: u64 = 44;

/// Zoveel samples leest de lezer hooguit in één keer van schijf.
const LEESBLOK: usize = 8192;

/// Samenvatting van een spoor in `einde.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpoorSamenvatting {
    /// Aantal stukken.
    pub stukken: u32,
    /// Alle samples.
    pub samples: u64,
    /// Samples die de bron kwijtraakte (een te trage uitgang of schijf).
    pub verloren: u64,
    /// Was er niets te horen?
    pub stil: bool,
    /// Viel de bron tijdens het opnemen uit?
    pub onderbroken: bool,
}

/// De inhoud van `einde.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Afsluiting {
    /// Lengte van de opname op de tijdlijn, in milliseconden.
    pub duur_ms: u64,
    /// Is de opname na een afbreking hersteld (in plaats van netjes gestopt)?
    pub hersteld: bool,
    /// Het microfoonspoor.
    pub microfoon: SpoorSamenvatting,
    /// Het systeemspoor, als dat er is.
    pub systeem: Option<SpoorSamenvatting>,
}

impl Afsluiting {
    /// [`MELDING_STIL_SYSTEEM`] als het systeemspoor er is en stil bleef.
    #[must_use]
    pub fn melding_stil_systeem(&self) -> Option<String> {
        let systeem = self.systeem.as_ref()?;
        systeem.stil.then(|| MELDING_STIL_SYSTEEM.to_string())
    }
}

/// Wat er mis kan gaan bij het lezen van een spoor.
#[derive(Debug, thiserror::Error)]
pub enum SpoorFout {
    /// `einde.json` ontbreekt: de opname loopt nog of is afgebroken.
    #[error(
        "Deze opname is nog niet afgesloten. Stop de opname eerst, of laat Mondig een afgebroken opname herstellen."
    )]
    NietAfgesloten,
    /// De map van het spoor is er niet.
    #[error("Dit deel van de opname ontbreekt.")]
    Ontbreekt,
    /// Een stuk of het logboek is niet te lezen. De technische reden staat er alleen voor
    /// diagnose bij en komt niet in de melding.
    #[error("Een deel van de opname kon niet gelezen worden.")]
    Lezen(#[source] io::Error),
}

/// Leest `einde.json` van een opnamemap.
///
/// # Errors
///
/// Geeft [`SpoorFout::NietAfgesloten`] als het bestand er niet is, en
/// [`SpoorFout::Lezen`] als het er wel is maar niet te lezen.
pub fn lees_afsluiting(opnamemap: &Path) -> Result<Afsluiting, SpoorFout> {
    let tekst = match fs::read_to_string(opnamemap.join(EINDE)) {
        Ok(tekst) => tekst,
        Err(fout) if fout.kind() == io::ErrorKind::NotFound => {
            return Err(SpoorFout::NietAfgesloten);
        }
        Err(fout) => return Err(SpoorFout::Lezen(fout)),
    };
    serde_json::from_str(&tekst)
        .map_err(|f| SpoorFout::Lezen(io::Error::new(io::ErrorKind::InvalidData, f)))
}

/// Waar een stuk in de stroom begint en waar op de tijdlijn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Anker {
    /// Eerste sample van het stuk in de stroom.
    begin: u64,
    /// Waar dat sample op de tijdlijn valt, in milliseconden.
    anker_ms: i64,
}

/// Rekent een plek in de stroom van een spoor om naar de tijdlijn van de opname.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tijdlijn {
    ankers: Vec<Anker>,
    totaal: u64,
}

impl Tijdlijn {
    /// Een tijdlijn uit stukken op volgorde: per stuk het aantal samples en het anker in
    /// milliseconden. Stukken zonder samples tellen niet mee.
    #[must_use]
    pub fn nieuw(stukken: impl IntoIterator<Item = (u64, i64)>) -> Self {
        let mut ankers = Vec::new();
        let mut totaal = 0u64;
        for (samples, anker_ms) in stukken {
            if samples == 0 {
                continue;
            }
            ankers.push(Anker {
                begin: totaal,
                anker_ms,
            });
            totaal += samples;
        }
        Self { ankers, totaal }
    }

    /// Alle samples van de stroom.
    #[must_use]
    pub fn totaal_samples(&self) -> u64 {
        self.totaal
    }

    /// De lengte van de stroom in milliseconden (zonder de gaten tussen de ankers).
    #[must_use]
    pub fn stroom_ms(&self) -> u64 {
        u64::try_from(samples_naar_ms(self.totaal)).unwrap_or(0)
    }

    /// Waar sample `positie` op de tijdlijn valt, in milliseconden. Een plek precies op de
    /// grens van twee stukken hoort bij het volgende stuk (het is daar het eerste sample).
    #[must_use]
    pub fn ms(&self, positie: u64) -> i64 {
        let na = self.ankers.partition_point(|a| a.begin <= positie);
        self.vanaf(na, positie)
    }

    /// Waar een stuk geluid dat vlak voor `positie` ophoudt op de tijdlijn eindigt. Anders
    /// dan [`Tijdlijn::ms`] hoort een plek op de grens hier bij het vorige stuk: daar zit
    /// het laatste sample.
    #[must_use]
    pub fn eind_ms(&self, positie: u64) -> i64 {
        let na = self.ankers.partition_point(|a| a.begin < positie).max(1);
        self.vanaf(na, positie)
    }

    /// Waar de stroom op de tijdlijn ophoudt.
    #[must_use]
    pub fn eind(&self) -> i64 {
        self.eind_ms(self.totaal)
    }

    /// Rekent vanaf het anker voor index `na` (het eerste anker na de plek).
    fn vanaf(&self, na: usize, positie: u64) -> i64 {
        match na.checked_sub(1).and_then(|i| self.ankers.get(i)) {
            Some(anker) => anker
                .anker_ms
                .saturating_add(samples_naar_ms(positie.saturating_sub(anker.begin))),
            None => samples_naar_ms(positie),
        }
    }
}

/// Een stuk zoals de lezer het gebruikt.
#[derive(Debug, Clone)]
struct Leesstuk {
    pad: PathBuf,
    samples: u64,
}

/// Leest een spoor als één doorlopende stroom van 16 kHz mono samples, stuk na stuk, met
/// begrensd geheugen: er staat nooit meer in dan het blok dat [`Spoorlezer::lees`] vraagt.
#[derive(Debug)]
pub struct Spoorlezer {
    stukken: Vec<Leesstuk>,
    tijdlijn: Tijdlijn,
    /// Het volgende stuk dat geopend moet worden.
    volgend: usize,
    /// Het stuk dat nu open is, met de samples die er nog in zitten.
    open: Option<(BufReader<File>, u64)>,
    positie: u64,
    bytes: Vec<u8>,
}

impl Spoorlezer {
    /// Opent de map van een spoor (`<opnamemap>/microfoon` of `<opnamemap>/systeem`).
    ///
    /// Een opname zonder `einde.json` in de opnamemap wordt geweigerd: dan loopt ze nog,
    /// of is ze afgebroken en nog niet hersteld. Een stuk dat korter is dan het logboek
    /// zegt, telt mee tot waar het bestand reikt.
    ///
    /// # Errors
    ///
    /// Geeft [`SpoorFout::NietAfgesloten`], [`SpoorFout::Ontbreekt`] als de map er niet is,
    /// of [`SpoorFout::Lezen`] als het logboek of een stuk niet te lezen is.
    pub fn open(spoormap: &Path) -> Result<Self, SpoorFout> {
        let opnamemap = spoormap.parent().ok_or(SpoorFout::Ontbreekt)?;
        if !opnamemap.join(EINDE).is_file() {
            return Err(SpoorFout::NietAfgesloten);
        }
        if !spoormap.is_dir() {
            return Err(SpoorFout::Ontbreekt);
        }
        let logboek = stukken::lees_logboek(spoormap).map_err(SpoorFout::Lezen)?;
        let mut stukken = Vec::new();
        let mut ankers = Vec::new();
        for StukInfo {
            bestand,
            samples,
            begin_ms,
            ..
        } in logboek
        {
            let pad = spoormap.join(&bestand);
            let lengte = fs::metadata(&pad).map_err(SpoorFout::Lezen)?.len();
            let samples = samples.min(lengte.saturating_sub(KOP) / 2);
            if samples == 0 {
                continue;
            }
            ankers.push((samples, begin_ms));
            stukken.push(Leesstuk { pad, samples });
        }
        Ok(Self {
            stukken,
            tijdlijn: Tijdlijn::nieuw(ankers),
            volgend: 0,
            open: None,
            positie: 0,
            bytes: Vec::new(),
        })
    }

    /// De afbeelding van stroompositie naar tijdlijn.
    #[must_use]
    pub fn tijdlijn(&self) -> &Tijdlijn {
        &self.tijdlijn
    }

    /// Hoeveel samples er tot nu toe gelezen zijn: de plek van het volgende sample.
    #[must_use]
    pub fn positie(&self) -> u64 {
        self.positie
    }

    /// Leest de volgende samples, hooguit `max`, over stukgrenzen heen. Minder dan `max`
    /// komt alleen aan het eind van het spoor; een leeg blok betekent dat het spoor op is.
    ///
    /// # Errors
    ///
    /// Geeft [`SpoorFout::Lezen`] als een stuk niet te openen of te lezen is.
    pub fn lees(&mut self, max: usize) -> Result<Vec<f32>, SpoorFout> {
        let over = usize::try_from(self.tijdlijn.totaal - self.positie).unwrap_or(usize::MAX);
        let mut uit = Vec::with_capacity(max.min(over));
        while uit.len() < max {
            let Some((bestand, rest)) = self.open.as_mut() else {
                let Some(stuk) = self.stukken.get(self.volgend) else {
                    break;
                };
                self.volgend += 1;
                let mut bestand = File::open(&stuk.pad).map_err(SpoorFout::Lezen)?;
                bestand
                    .seek(SeekFrom::Start(KOP))
                    .map_err(SpoorFout::Lezen)?;
                self.open = Some((BufReader::with_capacity(64 * 1024, bestand), stuk.samples));
                continue;
            };
            if *rest == 0 {
                self.open = None;
                continue;
            }
            let aantal = (max - uit.len())
                .min(LEESBLOK)
                .min(usize::try_from(*rest).unwrap_or(usize::MAX));
            self.bytes.resize(aantal * 2, 0);
            bestand
                .read_exact(&mut self.bytes)
                .map_err(SpoorFout::Lezen)?;
            uit.extend(
                self.bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|paar| f32::from(i16::from_le_bytes(*paar)) / f32::from(i16::MAX)),
            );
            *rest -= aantal as u64;
            self.positie += aantal as u64;
        }
        Ok(uit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn de_soort_alleen_ik_wordt_geschreven_en_gelezen() {
        let basis = tempfile::tempdir().unwrap();
        let map = basis.path();
        // Zo staat de soort in opname.json: "alleen".
        let tekst = serde_json::to_string(&Vergadersoort::Alleen).unwrap();
        assert_eq!(tekst, "\"alleen\"");
        fs::write(map.join(OPZET), format!("{{\"soort\": {tekst}}}")).unwrap();
        assert_eq!(lees_soort(map), Vergadersoort::Alleen);
        for (waarde, soort) in [
            ("online", Vergadersoort::Online),
            ("ruimte", Vergadersoort::Ruimte),
            ("alleen", Vergadersoort::Alleen),
        ] {
            fs::write(map.join(OPZET), format!("{{\"soort\": \"{waarde}\"}}")).unwrap();
            assert_eq!(lees_soort(map), soort, "{waarde}");
        }
        // Een soort die deze versie niet kent: online, zoals een opname van voor de soorten.
        fs::write(map.join(OPZET), "{\"soort\": \"iets\"}").unwrap();
        assert_eq!(lees_soort(map), Vergadersoort::Online);
        // Zonder veld ook online.
        fs::write(map.join(OPZET), "{}").unwrap();
        assert_eq!(lees_soort(map), Vergadersoort::Online);
    }

    #[test]
    fn alleen_online_heeft_systeemgeluid_en_sprekers() {
        assert!(Vergadersoort::Online.met_systeem());
        assert!(Vergadersoort::Online.met_sprekers());
        for soort in [Vergadersoort::Ruimte, Vergadersoort::Alleen] {
            assert!(!soort.met_systeem());
            assert!(!soort.met_sprekers());
        }
        assert_eq!(Vergadersoort::Online.titelwoord(), "Vergadering");
        assert_eq!(Vergadersoort::Ruimte.titelwoord(), "Vergadering");
        assert_eq!(Vergadersoort::Alleen.titelwoord(), "Notitie");
    }

    /// Schrijft een stuk zoals de opname dat doet: 44 bytes kop, dan 16-bit samples.
    fn schrijf_stuk(map: &Path, naam: &str, samples: &[i16]) {
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
        fs::write(map.join(naam), bytes).unwrap();
    }

    /// Een afgesloten opname met één spoor; per stuk de samples en het anker.
    fn opname(stukken: &[(Vec<i16>, i64, bool)]) -> (tempfile::TempDir, PathBuf) {
        let basis = tempfile::tempdir().unwrap();
        let spoormap = basis.path().join(MICROFOON);
        fs::create_dir_all(&spoormap).unwrap();
        let mut logboek = String::new();
        for (nummer, (samples, begin_ms, hersteld)) in stukken.iter().enumerate() {
            let nummer = u32::try_from(nummer).unwrap();
            let bestand = stukken::stuknaam(nummer);
            schrijf_stuk(&spoormap, &bestand, samples);
            let info = StukInfo {
                stuk: nummer,
                bestand,
                samples: samples.len() as u64,
                begin_ms: *begin_ms,
                hersteld: *hersteld,
            };
            logboek.push_str(&serde_json::to_string(&info).unwrap());
            logboek.push('\n');
        }
        fs::write(spoormap.join(stukken::LOGBOEK), logboek).unwrap();
        fs::write(basis.path().join(EINDE), "{}").unwrap();
        (basis, spoormap)
    }

    /// Een oplopende reeks, zodat te zien is of er iets wegvalt of dubbel komt.
    fn reeks(van: i16, aantal: i16) -> Vec<i16> {
        (van..van + aantal).collect()
    }

    fn als_i16(samples: &[f32]) -> Vec<i16> {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "terug naar de 16 bit van het stuk"
        )]
        samples
            .iter()
            .map(|s| (s * f32::from(i16::MAX)).round() as i16)
            .collect()
    }

    #[test]
    fn meerdere_stukken_lezen_als_een_stroom_over_de_grens_heen() {
        let (_basis, map) = opname(&[
            (reeks(0, 1000), 0, false),
            (reeks(1000, 1000), 63, false),
            (reeks(2000, 500), 125, false),
        ]);
        let mut lezer = Spoorlezer::open(&map).unwrap();
        assert_eq!(lezer.tijdlijn().totaal_samples(), 2500);
        // Blokken die niet op de stukgrenzen vallen.
        let mut alles = Vec::new();
        loop {
            let blok = lezer.lees(700).unwrap();
            if blok.is_empty() {
                break;
            }
            assert!(blok.len() == 700 || lezer.positie() == 2500);
            alles.extend(als_i16(&blok));
        }
        assert_eq!(alles, reeks(0, 2500));
        assert_eq!(lezer.positie(), 2500);
        assert!(lezer.lees(10).unwrap().is_empty());
    }

    #[test]
    fn een_gat_tussen_de_ankers_komt_terug_op_de_tijdlijn() {
        // Stuk 1 duurt 1000 ms, maar stuk 2 begint pas op 1500 ms.
        let (_basis, map) = opname(&[(vec![0; 16_000], 0, false), (vec![0; 16_000], 1500, false)]);
        let lezer = Spoorlezer::open(&map).unwrap();
        let t = lezer.tijdlijn();
        assert_eq!(t.ms(0), 0);
        assert_eq!(t.ms(8000), 500);
        // Het laatste sample van stuk 1 en het eerste van stuk 2.
        assert_eq!(t.ms(15_999), 999);
        assert_eq!(t.ms(16_000), 1500);
        // Een einde precies op de grens hoort bij het vorige stuk.
        assert_eq!(t.eind_ms(16_000), 1000);
        assert_eq!(t.eind_ms(16_001), 1500);
        assert_eq!(t.eind(), 2500);
        assert_eq!(t.stroom_ms(), 2000);
    }

    #[test]
    fn een_hersteld_stuk_leest_tot_waar_het_bestand_reikt() {
        let (_basis, map) = opname(&[(reeks(0, 100), 0, false), (reeks(100, 100), 6, true)]);
        // Het logboek zegt meer dan er op schijf staat (afgebroken schrijven).
        let mut logboek = stukken::lees_logboek(&map).unwrap();
        logboek[1].samples = 400;
        let tekst: String = logboek
            .iter()
            .map(|i| serde_json::to_string(i).unwrap() + "\n")
            .collect();
        fs::write(map.join(stukken::LOGBOEK), tekst).unwrap();
        let mut lezer = Spoorlezer::open(&map).unwrap();
        assert_eq!(lezer.tijdlijn().totaal_samples(), 200);
        assert_eq!(lezer.tijdlijn().ms(100), 6);
        assert_eq!(als_i16(&lezer.lees(1000).unwrap()), reeks(0, 200));
    }

    #[test]
    fn een_leeg_spoor_geeft_een_lege_stroom() {
        let (_basis, map) = opname(&[]);
        let mut lezer = Spoorlezer::open(&map).unwrap();
        assert_eq!(lezer.tijdlijn().totaal_samples(), 0);
        assert!(lezer.lees(1000).unwrap().is_empty());
        assert_eq!(lezer.tijdlijn().eind(), 0);
        // Ook zonder logboek.
        fs::remove_file(map.join(stukken::LOGBOEK)).unwrap();
        assert!(Spoorlezer::open(&map).unwrap().lees(10).unwrap().is_empty());
    }

    #[test]
    fn een_opname_zonder_einde_wordt_geweigerd() {
        let (basis, map) = opname(&[(vec![1; 10], 0, false)]);
        fs::remove_file(basis.path().join(EINDE)).unwrap();
        let fout = Spoorlezer::open(&map).unwrap_err();
        assert!(matches!(fout, SpoorFout::NietAfgesloten), "{fout:?}");
        assert!(fout.to_string().contains("nog niet afgesloten"), "{fout}");
        assert!(matches!(
            lees_afsluiting(basis.path()),
            Err(SpoorFout::NietAfgesloten)
        ));
    }

    #[test]
    fn een_ontbrekend_spoor_is_een_eigen_fout() {
        let (basis, _map) = opname(&[]);
        let fout = Spoorlezer::open(&basis.path().join(SYSTEEM)).unwrap_err();
        assert!(matches!(fout, SpoorFout::Ontbreekt), "{fout:?}");
    }

    #[test]
    fn afsluiting_meldt_een_stil_systeemspoor() {
        let spoor = |stil| SpoorSamenvatting {
            stukken: 1,
            samples: 10,
            verloren: 0,
            stil,
            onderbroken: false,
        };
        let mut afsluiting = Afsluiting {
            duur_ms: 1000,
            hersteld: false,
            microfoon: spoor(false),
            systeem: Some(spoor(true)),
        };
        assert_eq!(
            afsluiting.melding_stil_systeem().as_deref(),
            Some(MELDING_STIL_SYSTEEM)
        );
        afsluiting.systeem = Some(spoor(false));
        assert_eq!(afsluiting.melding_stil_systeem(), None);
        afsluiting.systeem = None;
        assert_eq!(afsluiting.melding_stil_systeem(), None);
    }
}
