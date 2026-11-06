//! De sporen van een vergaderopname samen in één WAV-bestand, om de opname te bewaren
//! (besluit, 2-11-2026: "opname bewaren als één gemengde WAV").
//!
//! Beide sporen gaan op de gezamenlijke tijdlijn: elk stuk op de plek van zijn anker, de
//! microfoon [`MICROFOON_VERTRAGING`] naar voren (zoals bij het uitschrijven), en een gat
//! tussen twee stukken wordt stilte. De samples worden opgeteld en begrensd op wat 16 bit
//! aankan, zodat twee luide sporen samen niet omslaan maar hooguit afgetopt worden.
//!
//! Het mengen loopt in blokken van een paar seconden: er staat nooit een heel spoor in het
//! geheugen, ook niet bij een opname van uren. [`meng_naar_bestand`] schrijft eerst naar een
//! tijdelijk bestand naast het doel en hernoemt pas als alles goed op schijf staat.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::stukken::{self, kop};
use super::{MICROFOON, SYSTEEM, SpoorFout, lees_afsluiting};
use crate::audio::DOEL_SAMPLERATE;
use crate::gesprek::MICROFOON_VERTRAGING;

/// Zoveel samples mengt Mondig in één keer: vier seconden.
const BLOK: usize = DOEL_SAMPLERATE as usize * 4;

/// Grootte van de WAV-kop van een stuk.
const KOP: u64 = 44;

/// Wat er mis kan gaan bij het mengen.
#[derive(Debug, thiserror::Error)]
pub enum MengFout {
    /// Een spoor is niet te lezen, of de opname is niet afgesloten.
    #[error(transparent)]
    Spoor(#[from] SpoorFout),
    /// Het doel is niet te schrijven. De technische reden staat er voor diagnose bij.
    #[error("Mondig kan de opname niet op die plek bewaren.")]
    Schrijven(#[source] io::Error),
    /// Langer dan in één WAV-bestand past (ruim 37 uur).
    #[error("De opname is te lang om als één bestand te bewaren.")]
    TeLang,
}

impl MengFout {
    /// De I/O-fout bij het schrijven, als het daar misging (voor een melding als "schijf vol").
    #[must_use]
    pub fn schrijffout(&self) -> Option<&io::Error> {
        match self {
            Self::Schrijven(fout) => Some(fout),
            _ => None,
        }
    }
}

/// Een stuk van een spoor op de tijdlijn, in samples.
#[derive(Debug, Clone)]
struct Plaatsing {
    pad: PathBuf,
    /// Waar het eerste sample op de tijdlijn valt (kan voor nul liggen door de verschuiving).
    begin: i64,
    samples: u64,
}

impl Plaatsing {
    fn eind(&self) -> i64 {
        self.begin
            .saturating_add(i64::try_from(self.samples).unwrap_or(i64::MAX))
    }
}

/// De stukken van een spoor, op volgorde van begin, `verschuiving` samples naar voren.
fn plaatsingen(spoormap: &Path, verschuiving: i64) -> Result<Vec<Plaatsing>, SpoorFout> {
    let per_ms = i64::from(DOEL_SAMPLERATE / 1000);
    let mut lijst = Vec::new();
    for info in stukken::lees_logboek(spoormap).map_err(SpoorFout::Lezen)? {
        let pad = spoormap.join(&info.bestand);
        let lengte = fs::metadata(&pad).map_err(SpoorFout::Lezen)?.len();
        // Een stuk dat korter is dan het logboek zegt, telt tot waar het bestand reikt.
        let samples = info.samples.min(lengte.saturating_sub(KOP) / 2);
        if samples == 0 {
            continue;
        }
        lijst.push(Plaatsing {
            pad,
            begin: info.begin_ms.saturating_mul(per_ms) - verschuiving,
            samples,
        });
    }
    lijst.sort_by_key(|p| p.begin);
    Ok(lijst)
}

/// Legt het deel van een spoor dat in `[van, van + buffer.len())` valt in `buffer`. Een later
/// stuk dat over een eerder heen valt, gaat voor; waar geen stuk is, blijft het stilte.
fn vul(stukken: &[Plaatsing], van: i64, buffer: &mut [i32]) -> Result<(), SpoorFout> {
    let tot = van.saturating_add(i64::try_from(buffer.len()).unwrap_or(i64::MAX));
    let mut bytes = Vec::new();
    for stuk in stukken {
        let begin = stuk.begin.max(van);
        let eind = stuk.eind().min(tot);
        if begin >= eind {
            continue;
        }
        let in_stuk = u64::try_from(begin - stuk.begin).unwrap_or(0);
        let aantal = usize::try_from(eind - begin).unwrap_or(0);
        let mut bestand = File::open(&stuk.pad).map_err(SpoorFout::Lezen)?;
        bestand
            .seek(SeekFrom::Start(KOP + in_stuk * 2))
            .map_err(SpoorFout::Lezen)?;
        bytes.resize(aantal * 2, 0);
        bestand.read_exact(&mut bytes).map_err(SpoorFout::Lezen)?;
        let plek = usize::try_from(begin - van).unwrap_or(0);
        for (doel, paar) in buffer[plek..plek + aantal]
            .iter_mut()
            .zip(bytes.as_chunks::<2>().0)
        {
            *doel = i32::from(i16::from_le_bytes(*paar));
        }
    }
    Ok(())
}

/// Telt twee samples op en begrenst ze op 16 bit.
fn tel_op(a: i32, b: i32) -> i16 {
    let som = a
        .saturating_add(b)
        .clamp(i32::from(i16::MIN), i32::from(i16::MAX));
    i16::try_from(som).unwrap_or(0)
}

/// Mengt de sporen van een afgesloten opname tot één WAV-stroom (16 kHz mono 16 bit) naar
/// `uit`. Geeft het aantal samples. Zonder systeemspoor is het alleen de microfoon, zonder
/// verschuiving (er is niets om op te wachten).
///
/// # Errors
///
/// Geeft [`MengFout::Spoor`] als de opname niet is afgesloten of een stuk niet te lezen is,
/// [`MengFout::Schrijven`] als `uit` weigert, en [`MengFout::TeLang`] boven ruim 37 uur.
pub fn meng(opnamemap: &Path, uit: &mut dyn Write) -> Result<u64, MengFout> {
    meng_deel(opnamemap, 0, u64::MAX, uit)
}

/// Zoals [`meng`], maar alleen het deel van `begin_ms` tot `eind_ms` op de tijdlijn van de
/// opname (dezelfde tijdlijn als de tijden in het verslag). Dat is wat de interface nodig heeft
/// om één beurt af te spelen: nooit de hele opname in het geheugen. Een deel buiten de opname
/// wordt afgekapt; een deel dat helemaal buiten valt, geeft een WAV zonder samples.
///
/// # Errors
///
/// Zie [`meng`].
pub fn meng_deel(
    opnamemap: &Path,
    begin_ms: u64,
    eind_ms: u64,
    uit: &mut dyn Write,
) -> Result<u64, MengFout> {
    let afsluiting = lees_afsluiting(opnamemap)?;
    let systeemmap = opnamemap.join(SYSTEEM);
    let met_systeem = afsluiting.systeem.is_some() && systeemmap.is_dir();
    let verschuiving = if met_systeem {
        let ms = i64::try_from(MICROFOON_VERTRAGING.as_millis()).unwrap_or(0);
        ms * i64::from(DOEL_SAMPLERATE / 1000)
    } else {
        0
    };
    let microfoonmap = opnamemap.join(MICROFOON);
    let microfoon = if microfoonmap.is_dir() {
        plaatsingen(&microfoonmap, verschuiving)?
    } else {
        Vec::new()
    };
    let systeem = if met_systeem {
        plaatsingen(&systeemmap, 0)?
    } else {
        Vec::new()
    };
    let lengte = microfoon
        .iter()
        .chain(&systeem)
        .map(Plaatsing::eind)
        .max()
        .unwrap_or(0)
        .max(0);
    let lengte = u64::try_from(lengte).unwrap_or(0);
    let per_ms = u64::from(DOEL_SAMPLERATE / 1000);
    let tot = eind_ms.saturating_mul(per_ms).min(lengte);
    let begin = begin_ms.saturating_mul(per_ms).min(tot);
    let data = u32::try_from((tot - begin) * 2)
        .ok()
        .filter(|d| *d <= u32::MAX - 36)
        .ok_or(MengFout::TeLang)?;
    uit.write_all(&kop(data)).map_err(MengFout::Schrijven)?;

    let mut jij = vec![0i32; BLOK];
    let mut anderen = vec![0i32; BLOK];
    let mut bytes = Vec::with_capacity(BLOK * 2);
    let mut van = begin;
    while van < tot {
        let aantal = usize::try_from((tot - van).min(BLOK as u64)).unwrap_or(BLOK);
        let blokbegin = i64::try_from(van).unwrap_or(i64::MAX);
        let jij = &mut jij[..aantal];
        let anderen = &mut anderen[..aantal];
        jij.fill(0);
        anderen.fill(0);
        vul(&microfoon, blokbegin, jij)?;
        vul(&systeem, blokbegin, anderen)?;
        bytes.clear();
        for (a, b) in jij.iter().zip(anderen.iter()) {
            bytes.extend_from_slice(&tel_op(*a, *b).to_le_bytes());
        }
        uit.write_all(&bytes).map_err(MengFout::Schrijven)?;
        van += aantal as u64;
    }
    Ok(tot - begin)
}

/// Mengt de sporen (zie [`meng`]) naar het bestand `doel`, alles-of-niets: eerst naar een
/// nieuw tijdelijk bestand naast het doel, dat naar schijf wordt gezet en dan hernoemd. Een
/// bestaand doel wordt vervangen; vraag de gebruiker dat dus eerst (de bewaarkiezer van het
/// besturingssysteem doet dat). Mislukt er iets, dan blijft er geen half bestand achter.
///
/// # Errors
///
/// Zie [`meng`]; ook [`MengFout::Schrijven`] als het tijdelijke bestand niet te maken of te
/// hernoemen is.
pub fn meng_naar_bestand(opnamemap: &Path, doel: &Path) -> Result<u64, MengFout> {
    let (tijdelijk, bestand) = maak_tijdelijk(doel).map_err(MengFout::Schrijven)?;
    let schrijf = || -> Result<u64, MengFout> {
        let mut uit = BufWriter::with_capacity(256 * 1024, bestand);
        let samples = meng(opnamemap, &mut uit)?;
        let bestand = uit
            .into_inner()
            .map_err(|f| MengFout::Schrijven(f.into_error()))?;
        bestand.sync_all().map_err(MengFout::Schrijven)?;
        drop(bestand);
        fs::rename(&tijdelijk, doel).map_err(MengFout::Schrijven)?;
        Ok(samples)
    };
    let uitslag = schrijf();
    if uitslag.is_err() {
        let _ = fs::remove_file(&tijdelijk);
    }
    uitslag
}

/// Een nieuw, leeg tijdelijk bestand naast `doel`, met een naam die nog niet bestaat.
fn maak_tijdelijk(doel: &Path) -> io::Result<(PathBuf, File)> {
    let naam = doel.file_name().unwrap_or_default().to_string_lossy();
    for poging in 0..100u32 {
        let pad = doel.with_file_name(format!(".{naam}.{poging}.mondig-tijdelijk"));
        match OpenOptions::new().write(true).create_new(true).open(&pad) {
            Ok(bestand) => return Ok((pad, bestand)),
            Err(fout) if fout.kind() == io::ErrorKind::AlreadyExists => {}
            Err(fout) => return Err(fout),
        }
    }
    Err(io::Error::from(io::ErrorKind::AlreadyExists))
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;
    use crate::spoor::EINDE;
    use crate::spoor::stukken::{LOGBOEK, stuknaam};

    /// Schrijft een spoor: per stuk de samples en het anker op de tijdlijn (ms).
    fn schrijf_spoor(map: &Path, stukken: &[(Vec<i16>, i64)]) {
        fs::create_dir_all(map).unwrap();
        let mut logboek = String::new();
        for (nummer, (samples, begin_ms)) in stukken.iter().enumerate() {
            let nummer = u32::try_from(nummer).unwrap();
            let data = u32::try_from(samples.len() * 2).unwrap();
            let mut bytes = kop(data).to_vec();
            for s in samples {
                bytes.extend_from_slice(&s.to_le_bytes());
            }
            fs::write(map.join(stuknaam(nummer)), bytes).unwrap();
            writeln!(
                logboek,
                "{{\"stuk\":{nummer},\"bestand\":\"{}\",\"samples\":{},\"begin_ms\":{begin_ms}}}",
                stuknaam(nummer),
                samples.len()
            )
            .unwrap();
        }
        fs::write(map.join(LOGBOEK), logboek).unwrap();
    }

    fn schrijf_einde(map: &Path, met_systeem: bool) {
        let spoor = r#"{"stukken":1,"samples":0,"verloren":0,"stil":false,"onderbroken":false}"#;
        let systeem = if met_systeem { spoor } else { "null" };
        fs::write(
            map.join(EINDE),
            format!(
                r#"{{"duur_ms":1000,"hersteld":false,"microfoon":{spoor},"systeem":{systeem}}}"#
            ),
        )
        .unwrap();
    }

    /// Leest een WAV terug: de samples na de kop, en controleert de kop.
    fn lees(bytes: &[u8]) -> Vec<i16> {
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let data = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
        assert_eq!(data, bytes.len() - 44, "de kop klopt met de lengte");
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            16_000
        );
        bytes[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| i16::from_le_bytes(*p))
            .collect()
    }

    fn gemengd(map: &Path) -> Vec<i16> {
        let mut uit = Vec::new();
        let samples = meng(map, &mut uit).unwrap();
        let lijst = lees(&uit);
        assert_eq!(lijst.len() as u64, samples);
        lijst
    }

    #[test]
    fn beide_sporen_komen_op_de_tijdlijn_met_de_microfoon_naar_voren() {
        let opname = tempfile::tempdir().unwrap();
        // Microfoon: 3200 samples van 100 vanaf 0 ms; na 100 ms naar voren valt de eerste
        // 1600 samples voor nul en weg.
        schrijf_spoor(&opname.path().join(MICROFOON), &[(vec![100; 3200], 0)]);
        // Systeem: 800 samples van 1000 vanaf 50 ms (sample 800).
        schrijf_spoor(&opname.path().join(SYSTEEM), &[(vec![1000; 800], 50)]);
        schrijf_einde(opname.path(), true);
        let mix = gemengd(opname.path());
        assert_eq!(mix.len(), 1600, "tot het eind van het langste spoor");
        assert_eq!(mix[0], 100);
        assert_eq!(mix[799], 100);
        assert_eq!(mix[800], 1100, "opgeteld waar beide klinken");
        assert_eq!(mix[1599], 1100);
    }

    #[test]
    fn een_gat_tussen_stukken_wordt_stilte() {
        let opname = tempfile::tempdir().unwrap();
        schrijf_spoor(
            &opname.path().join(MICROFOON),
            &[(vec![5; 160], 0), (vec![7; 160], 20)],
        );
        schrijf_einde(opname.path(), false);
        let mix = gemengd(opname.path());
        // Zonder systeemspoor geen verschuiving: stuk 2 begint op 20 ms (sample 320).
        assert_eq!(mix.len(), 480);
        assert!(mix[..160].iter().all(|s| *s == 5));
        assert!(mix[160..320].iter().all(|s| *s == 0));
        assert!(mix[320..].iter().all(|s| *s == 7));
    }

    #[test]
    fn twee_luide_sporen_worden_afgetopt_en_slaan_niet_om() {
        let opname = tempfile::tempdir().unwrap();
        schrijf_spoor(
            &opname.path().join(MICROFOON),
            &[(vec![30_000; 3200], 0), (vec![-30_000; 3200], 200)],
        );
        schrijf_spoor(
            &opname.path().join(SYSTEEM),
            &[(vec![30_000; 1600], 0), (vec![-30_000; 3200], 100)],
        );
        schrijf_einde(opname.path(), true);
        let mix = gemengd(opname.path());
        assert_eq!(mix[0], i16::MAX);
        assert_eq!(mix[mix.len() - 1], i16::MIN);
        assert_eq!(tel_op(20_000, -5_000), 15_000);
    }

    #[test]
    fn een_lange_opname_gaat_in_blokken_en_blijft_op_zijn_plek() {
        let opname = tempfile::tempdir().unwrap();
        // Twee stukken van 3 s achter elkaar: langer dan een blok van 4 s samen.
        let a: Vec<i16> = (0..48_000)
            .map(|i| i16::try_from(i % 1000).unwrap())
            .collect();
        schrijf_spoor(
            &opname.path().join(MICROFOON),
            &[(a.clone(), 0), (a.clone(), 3000)],
        );
        schrijf_einde(opname.path(), false);
        let mix = gemengd(opname.path());
        assert_eq!(mix.len(), 96_000);
        assert_eq!(&mix[..48_000], &a[..]);
        assert_eq!(&mix[48_000..], &a[..]);
    }

    #[test]
    fn een_deel_geeft_precies_dat_stuk_van_de_tijdlijn() {
        let opname = tempfile::tempdir().unwrap();
        // Microfoon 100 vanaf 0 ms, systeem 1000 vanaf 50 ms; met de verschuiving van 100 ms
        // valt de microfoon op de tijdlijn van 0 tot 100 ms (de eerste 100 ms zijn weg).
        schrijf_spoor(&opname.path().join(MICROFOON), &[(vec![100; 3200], 0)]);
        schrijf_spoor(&opname.path().join(SYSTEEM), &[(vec![1000; 800], 50)]);
        schrijf_einde(opname.path(), true);
        let mut uit = Vec::new();
        // 40 tot 70 ms: 480 samples, het systeem begint op 50 ms.
        let samples = meng_deel(opname.path(), 40, 70, &mut uit).unwrap();
        assert_eq!(samples, 480);
        let deel = lees(&uit);
        assert_eq!(deel.len(), 480);
        assert_eq!(deel[0], 100, "voor 50 ms alleen de microfoon");
        assert_eq!(deel[159], 100);
        assert_eq!(deel[160], 1100, "op 50 ms komt het systeem erbij");
        // Hetzelfde stuk uit de hele mix.
        let geheel = gemengd(opname.path());
        assert_eq!(deel, geheel[640..1120]);
    }

    #[test]
    fn een_deel_voorbij_het_eind_wordt_afgekapt_en_ver_erbuiten_is_leeg() {
        let opname = tempfile::tempdir().unwrap();
        schrijf_spoor(&opname.path().join(MICROFOON), &[(vec![7; 1600], 0)]);
        schrijf_einde(opname.path(), false);
        let mut uit = Vec::new();
        assert_eq!(meng_deel(opname.path(), 50, 10_000, &mut uit).unwrap(), 800);
        assert_eq!(lees(&uit).len(), 800);
        let mut leeg = Vec::new();
        assert_eq!(
            meng_deel(opname.path(), 5_000, 6_000, &mut leeg).unwrap(),
            0
        );
        assert!(lees(&leeg).is_empty());
    }

    #[test]
    fn een_opname_die_niet_is_afgesloten_wordt_geweigerd() {
        let opname = tempfile::tempdir().unwrap();
        schrijf_spoor(&opname.path().join(MICROFOON), &[(vec![1; 16], 0)]);
        let mut uit = Vec::new();
        assert!(matches!(
            meng(opname.path(), &mut uit),
            Err(MengFout::Spoor(SpoorFout::NietAfgesloten))
        ));
    }

    #[test]
    fn naar_een_bestand_alles_of_niets_zonder_tijdelijk_bestand_achter() {
        let opname = tempfile::tempdir().unwrap();
        schrijf_spoor(&opname.path().join(MICROFOON), &[(vec![42; 1600], 0)]);
        schrijf_einde(opname.path(), false);
        let doelmap = tempfile::tempdir().unwrap();
        let doel = doelmap.path().join("vergadering.wav");
        fs::write(&doel, b"oud").unwrap();
        assert_eq!(meng_naar_bestand(opname.path(), &doel).unwrap(), 1600);
        let mix = lees(&fs::read(&doel).unwrap());
        assert!(mix.iter().all(|s| *s == 42));
        let namen: Vec<_> = fs::read_dir(doelmap.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(namen, vec![std::ffi::OsString::from("vergadering.wav")]);

        // Mislukt het mengen, dan blijft het oude bestand staan en geen tijdelijk bestand.
        let kapot = tempfile::tempdir().unwrap();
        assert!(meng_naar_bestand(kapot.path(), &doel).is_err());
        assert_eq!(fs::read_dir(doelmap.path()).unwrap().count(), 1);
        assert_eq!(lees(&fs::read(&doel).unwrap()).len(), 1600);
    }
}
