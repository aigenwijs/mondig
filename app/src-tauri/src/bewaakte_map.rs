//! De bewaakte map (fase B-1, hoofdplan 4-D): één map die Mondig in de gaten houdt, zodat
//! nieuwe opnames (Teams, dictafoon, telefoon) vanzelf worden uitgeschreven.
//!
//! - **Instellingen** (groep Opslaan): aan of uit, de map, submappen (standaard uit),
//!   "Overslaan als er al een transcript is" (standaard aan) en "Transcript naast het
//!   bronbestand zetten" (standaard uit). Alles in de store, zoals de rest van Instellingen.
//! - **Een eigen thread** leest de map elke paar seconden (`mondig_core::bewaak`), wacht tot
//!   een bestand niet meer groeit en zet het dan achteraan in dezelfde wachtrij als Bestanden
//!   (`bestanden::voeg_toe_bewaakt`). Het werk zelf doet het achtergrondwerkproces (ADR 0009).
//!   De thread draait zolang Mondig draait, ook als het venster verborgen is (systeemvak).
//! - **Het transcript** gaat naar de standaardmap in het standaardformaat met de naamregel van
//!   4-C, of naast het bronbestand als de gebruiker dat koos (`bewaren::automatisch`). Het
//!   origineel blijft onaangeroerd: Mondig leest het alleen.
//! - **Beginstand.** Zet de gebruiker de bewaking aan (of kiest hij een andere map of zet hij
//!   de submappen aan of uit), dan tellen de bestanden die er al staan als bestaand: alleen
//!   nieuwe bestanden komen aan de beurt. Anders zou een map met een jaar aan opnames in één
//!   keer uur na uur rekenwerk vragen. Die bestanden kan de gebruiker nog steeds zelf slepen.
//! - **Onthouden** wat gedaan is, ook na een herstart: `bewaakte-map.json` in de gegevensmap
//!   (pad, grootte en wijzigtijd, zonder inhoud). Gedaan is: het transcript is opgeslagen, de
//!   gebruiker annuleerde het bestand, of Mondig kon het bestand niet lezen. Een bestand dat
//!   bij het afsluiten nog in de wachtrij stond, wordt bij de volgende start opnieuw
//!   opgepakt; een bestand dat onderweg mislukte ook.
//! - **Nooit de eigen bestanden**: de map met transcripten en de gegevensmap van Mondig zijn
//!   uitgesloten, en de bewaakte map zelf mag niet de map met transcripten zijn.
//!
//! Nergens komt de inhoud van een bestand of een transcript in een log of in het register.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de argumenten per waarde mee; een verwijzing kan niet"
)]

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use mondig_core::bewaak::{
    Handtekening, RITME, Register, STILSTAND, Stilstand, gegevensmap, instelling_sleutel,
    is_leesbaar, ligt_in, registerbestand, scan,
};
use mondig_core::taak::{Foutmelding, Vervolg};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::bestanden::{Toevoeging, voeg_overgeslagen_toe, voeg_toe_bewaakt};
use crate::bewaren::{MapInfo, heeft_transcript, kies_map, transcriptmap};
use crate::dicteerketen::slot;
use crate::sneltoets::{lees_instelling, schrijf_instelling};

const SLEUTEL_AAN: &str = "bewaak_aan";
const SLEUTEL_MAP: &str = "bewaak_map";
const SLEUTEL_SUBMAPPEN: &str = "bewaak_submappen";
const SLEUTEL_OVERSLAAN: &str = "bewaak_overslaan";
const SLEUTEL_NAAST: &str = "bewaak_naast";
const AAN: &str = "aan";
const UIT: &str = "uit";

/// Een bestand uit de bewaakte map, zoals de lijst van Bestanden het onthoudt: waar het staat,
/// hoe het er toen uitzag, en waar het transcript heen moet.
#[derive(Debug, Clone)]
pub(crate) struct Bewaakt {
    pub(crate) pad: PathBuf,
    pub(crate) handtekening: Handtekening,
    /// Het transcript naast het bronbestand zetten in plaats van in de standaardmap.
    pub(crate) naast: bool,
}

/// Wat de bewaakte map in het geheugen houdt: het register van wat gedaan is, geladen bij het
/// eerste gebruik.
#[derive(Default)]
pub(crate) struct BewaakStaat {
    register: Mutex<Option<Register>>,
    /// Stonden er bij de laatste lezing meer bestanden dan Mondig leest? Voor Instellingen.
    afgekapt: AtomicBool,
    /// Waarom een bestand niet in de wachtrij kon (geen model): een keer in Instellingen in
    /// plaats van elke paar seconden in de foutbalk van Bestanden.
    wachtrijfout: Mutex<Option<String>>,
}

impl BewaakStaat {
    /// Voert `werk` uit op het register (geladen als dat nog niet was).
    fn met_register<T>(&self, werk: impl FnOnce(&mut Register) -> T) -> T {
        let mut houder = slot(&self.register);
        let register = houder.get_or_insert_with(|| {
            registerbestand().map_or_else(Register::default, |pad| Register::laad(&pad))
        });
        werk(register)
    }

    /// Bewaakt het register op schijf. Lukt dat niet, dan gaat de bewaking gewoon door; na een
    /// herstart kan een bestand dan nog eens worden uitgeschreven.
    fn bewaar(register: &Register) {
        if let Some(pad) = registerbestand() {
            if let Some(map) = pad.parent() {
                let _ = std::fs::create_dir_all(map);
            }
            let _ = register.bewaar(&pad);
        }
    }
}

/// Noteert een bestand als gedaan, in het geheugen en op schijf.
pub(crate) fn markeer_gedaan(app: &AppHandle, bewaakt: &Bewaakt) {
    app.state::<BewaakStaat>().met_register(|register| {
        register.noteer(&bewaakt.pad, bewaakt.handtekening);
        BewaakStaat::bewaar(register);
    });
}

// ---------------------------------------------------------------------------
// Instellingen
// ---------------------------------------------------------------------------

/// Waarom de bewaking niet kan werken, als de gebruiker hem wel aanzette. De interface zet er
/// zelf een tekst bij.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Probleem {
    /// Er is nog geen map gekozen.
    GeenMap,
    /// De gekozen map bestaat niet (meer), bijvoorbeeld een netwerkschijf die weg is.
    MapOntbreekt,
    /// De gekozen map is de map met transcripten of de gegevensmap van Mondig zelf.
    ZelfdeAlsOpslag,
    /// Er staan meer bestanden in de map dan Mondig leest; het teveel blijft ongezien.
    TeVeel,
}

/// De keuzes, met de standaard waar niets bewaard is.
#[expect(
    clippy::struct_excessive_bools,
    reason = "vier onafhankelijke schakelaars uit Instellingen; een enum maakt ze niet duidelijker"
)]
struct Keuzes {
    aan: bool,
    map: Option<PathBuf>,
    submappen: bool,
    overslaan: bool,
    naast: bool,
}

fn lees_keuzes(app: &AppHandle) -> Keuzes {
    let aan_uit = |sleutel: &str, standaard: bool| match lees_instelling(app, sleutel).as_deref() {
        Some(AAN) => true,
        Some(UIT) => false,
        _ => standaard,
    };
    Keuzes {
        aan: aan_uit(SLEUTEL_AAN, false),
        map: lees_instelling(app, SLEUTEL_MAP).map(PathBuf::from),
        submappen: aan_uit(SLEUTEL_SUBMAPPEN, false),
        overslaan: aan_uit(SLEUTEL_OVERSLAAN, true),
        naast: aan_uit(SLEUTEL_NAAST, false),
    }
}

/// De mappen waar de bewaking nooit in kijkt: de map met de eigen transcripten en de eigen
/// gegevens van Mondig (modellen, tijdelijke vergaderopnames, dit register).
fn uitgesloten(app: &AppHandle) -> Vec<PathBuf> {
    transcriptmap(app)
        .into_iter()
        .chain(gegevensmap())
        .collect()
}

/// Kan er in deze map bewaakt worden? `None` als dat kan.
fn bepaal_probleem(
    map: Option<&Path>,
    transcripten: Option<&Path>,
    gegevens: Option<&Path>,
) -> Option<Probleem> {
    // Zonder map is er niets mis met de map zelf; "geen map" meldt `probleem_van`.
    let map = map?;
    if transcripten.is_some_and(|t| ligt_in(map, t)) || gegevens.is_some_and(|g| ligt_in(map, g)) {
        return Some(Probleem::ZelfdeAlsOpslag);
    }
    (!map.is_dir()).then_some(Probleem::MapOntbreekt)
}

/// Het probleem van de keuzes, of `None` als de bewaking kan draaien.
fn probleem_van(app: &AppHandle, keuzes: &Keuzes) -> Option<Probleem> {
    if keuzes.map.is_none() {
        return Some(Probleem::GeenMap);
    }
    bepaal_probleem(
        keuzes.map.as_deref(),
        transcriptmap(app).as_deref(),
        gegevensmap().as_deref(),
    )
}

/// De keuzes van de bewaakte map, voor Instellingen.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "vier onafhankelijke schakelaars uit Instellingen, zoals de interface ze toont"
)]
pub(crate) struct BewaakInstellingen {
    aan: bool,
    map: Option<MapInfo>,
    submappen: bool,
    overslaan: bool,
    naast: bool,
    /// Waarom de bewaking nu niet werkt, als de gebruiker hem aanzette.
    probleem: Option<Probleem>,
    /// Waarom het laatste bestand niet in de wachtrij kon (bijvoorbeeld: het spraakmodel is er
    /// nog niet), in gewone taal. Mondig probeert het later opnieuw.
    melding: Option<String>,
}

fn instellingen(app: &AppHandle) -> BewaakInstellingen {
    let keuzes = lees_keuzes(app);
    let staat = app.state::<BewaakStaat>();
    let probleem = if keuzes.aan {
        probleem_van(app, &keuzes).or_else(|| {
            staat
                .afgekapt
                .load(Ordering::Relaxed)
                .then_some(Probleem::TeVeel)
        })
    } else {
        None
    };
    let melding = if keuzes.aan {
        slot(&staat.wachtrijfout).clone()
    } else {
        None
    };
    BewaakInstellingen {
        aan: keuzes.aan,
        map: keuzes.map.as_deref().map(MapInfo::van),
        submappen: keuzes.submappen,
        overslaan: keuzes.overslaan,
        naast: keuzes.naast,
        probleem,
        melding,
    }
}

fn aan_uit_tekst(aan: bool) -> &'static str {
    if aan { AAN } else { UIT }
}

/// De keuzes van de bewaakte map.
#[tauri::command]
pub(crate) fn bewaak_instellingen(app: AppHandle) -> BewaakInstellingen {
    instellingen(&app)
}

/// Opent de mapkiezer (in Rust) en bewaart de gekozen map. Sluit de gebruiker de kiezer, dan
/// blijft de map zoals hij was. De map met transcripten en de gegevensmap van Mondig worden
/// geweigerd: dan zou Mondig zijn eigen bestanden weer uitschrijven.
#[tauri::command]
pub(crate) async fn bewaak_kies_map(app: AppHandle) -> Result<BewaakInstellingen, Foutmelding> {
    tauri::async_runtime::spawn_blocking(move || {
        let huidig = lees_keuzes(&app).map;
        let Some(gekozen) = kies_map(&app, huidig.as_deref()) else {
            return Ok(instellingen(&app));
        };
        if bepaal_probleem(
            Some(&gekozen),
            transcriptmap(&app).as_deref(),
            gegevensmap().as_deref(),
        ) == Some(Probleem::ZelfdeAlsOpslag)
        {
            return Err(Foutmelding::nieuw(
                "Dit is de map waarin Mondig je uitschriften bewaart. Kies een andere map om in de gaten te houden.",
                Vervolg::Geen,
            ));
        }
        schrijf_instelling(&app, SLEUTEL_MAP, &gekozen.to_string_lossy());
        Ok(instellingen(&app))
    })
    .await
    .map_err(|_| Foutmelding::nieuw("Kiezen is onverwacht gestopt.", Vervolg::Geen))?
}

/// Zet de bewaking aan of uit. Aanzetten kan pas als er een map is gekozen.
#[tauri::command]
pub(crate) fn bewaak_zet_aan(app: AppHandle, aan: bool) -> Result<BewaakInstellingen, Foutmelding> {
    if aan && lees_keuzes(&app).map.is_none() {
        return Err(Foutmelding::nieuw(
            "Kies eerst een map om in de gaten te houden.",
            Vervolg::Geen,
        ));
    }
    schrijf_instelling(&app, SLEUTEL_AAN, aan_uit_tekst(aan));
    Ok(instellingen(&app))
}

/// Zet "Ook submappen" aan of uit.
#[tauri::command]
pub(crate) fn bewaak_zet_submappen(app: AppHandle, aan: bool) -> BewaakInstellingen {
    schrijf_instelling(&app, SLEUTEL_SUBMAPPEN, aan_uit_tekst(aan));
    instellingen(&app)
}

/// Zet "Overslaan als er al een transcript is" aan of uit.
#[tauri::command]
pub(crate) fn bewaak_zet_overslaan(app: AppHandle, aan: bool) -> BewaakInstellingen {
    schrijf_instelling(&app, SLEUTEL_OVERSLAAN, aan_uit_tekst(aan));
    instellingen(&app)
}

/// Zet "Transcript naast het bronbestand zetten" aan of uit.
#[tauri::command]
pub(crate) fn bewaak_zet_naast(app: AppHandle, aan: bool) -> BewaakInstellingen {
    schrijf_instelling(&app, SLEUTEL_NAAST, aan_uit_tekst(aan));
    instellingen(&app)
}

// ---------------------------------------------------------------------------
// De bewakingsthread
// ---------------------------------------------------------------------------

/// Zo lang wacht de bewaking na een mislukte poging om een bestand in de wachtrij te zetten,
/// en hoe ver dat oploopt: 30 seconden, dan een minuut, tot tien minuten.
const UITSTEL_BEGIN: Duration = Duration::from_secs(30);
const UITSTEL_MAX: Duration = Duration::from_mins(10);

/// Het wachten na een mislukte poging, met een wachttijd die verdubbelt.
struct Uitstel {
    tot: Duration,
    wacht: Duration,
}

impl Uitstel {
    fn nieuw() -> Self {
        Self {
            tot: Duration::ZERO,
            wacht: UITSTEL_BEGIN,
        }
    }

    /// De poging mislukte op tijdstip `nu`: wacht, en de volgende keer langer.
    fn mislukt(&mut self, nu: Duration) {
        self.tot = nu + self.wacht;
        self.wacht = (self.wacht * 2).min(UITSTEL_MAX);
    }

    /// De poging lukte: de volgende fout begint weer met de korte wachttijd.
    fn gelukt(&mut self) {
        self.wacht = UITSTEL_BEGIN;
    }

    fn wacht_nog(&self, nu: Duration) -> bool {
        nu < self.tot
    }
}

/// Start de bewakingsthread. Hij leest de instellingen bij elke ronde, dus een wijziging in
/// Instellingen werkt binnen een paar seconden door.
pub(crate) fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let begin = Instant::now();
        let mut bewaker = Stilstand::nieuw(STILSTAND);
        let mut uitstel = Uitstel::nieuw();
        loop {
            ronde(&app, &mut bewaker, &mut uitstel, begin.elapsed());
            // Sliep de computer tijdens deze pauze, dan telt de klok die tijd misschien mee:
            // de bewaker begint dan opnieuw te tellen.
            let voor = Instant::now();
            std::thread::sleep(RITME);
            bewaker.pauze(voor.elapsed());
        }
    });
}

/// Eén ronde: lezen, kijken wat stilstaat en dat in de wachtrij zetten.
fn ronde(app: &AppHandle, bewaker: &mut Stilstand, uitstel: &mut Uitstel, nu: Duration) {
    let keuzes = lees_keuzes(app);
    let staat = app.state::<BewaakStaat>();
    if !keuzes.aan {
        // Uit: vergeten wat we zagen, en de beginstand vervalt, zodat aanzetten later weer
        // met een beginstand begint in plaats van met alles wat er intussen is bijgekomen.
        bewaker.reset();
        staat.afgekapt.store(false, Ordering::Relaxed);
        *slot(&staat.wachtrijfout) = None;
        staat.met_register(|register| {
            if register.heeft_instelling() {
                register.zet_instelling(None);
                BewaakStaat::bewaar(register);
            }
        });
        return;
    }
    let Some(map) = keuzes.map.clone() else {
        bewaker.reset();
        return;
    };
    if probleem_van(app, &keuzes).is_some() {
        // Een map die tijdelijk weg is (netwerk): wachten tot hij terug is.
        bewaker.reset();
        return;
    }
    let uitgesloten = uitgesloten(app);
    let sleutel = instelling_sleutel(&map, keuzes.submappen);
    // Lezen gebeurt buiten het slot van het register: op een netwerkmap kan dat lang duren en
    // `voeg_toe` en het noteren van gedaan werk wachten op datzelfde slot.
    let uitkomst = scan(&map, keuzes.submappen, &uitgesloten);
    staat.afgekapt.store(uitkomst.afgekapt, Ordering::Relaxed);

    if !staat.met_register(|register| register.is_voor(&sleutel)) {
        // Beginstand: wat er al staat, telt als bestaand. Was de map niet helemaal te lezen,
        // dan wachten we tot dat wel kan, zodat er geen bestaand bestand achterblijft.
        if !uitkomst.volledig && !uitkomst.afgekapt {
            return;
        }
        staat.met_register(|register| {
            register.zet_instelling(Some(sleutel));
            for (pad, handtekening) in &uitkomst.bestanden {
                register.noteer(pad, *handtekening);
            }
            BewaakStaat::bewaar(register);
        });
        bewaker.reset();
        return;
    }

    // Wat nog niet gedaan is, en het register opgeruimd van wat weg is.
    let nieuw = staat.met_register(|register| {
        let (nieuw, veranderd) = register.verwerk(&uitkomst.bestanden, uitkomst.volledig);
        if veranderd {
            BewaakStaat::bewaar(register);
        }
        nieuw
    });
    // Alleen wat nog niet gedaan is wordt geopend, om te zien of een ander programma het nog
    // vasthoudt; een bestand dat nog niet leesbaar is, telt nog niet als stil.
    let leesbaar: Vec<_> = nieuw
        .into_iter()
        .filter(|(pad, _)| is_leesbaar(pad))
        .collect();
    if uitstel.wacht_nog(nu) {
        return;
    }
    let klaar = bewaker.kijk(nu, &leesbaar);
    let mut gestopt = false;
    for (pad, handtekening) in klaar {
        if gestopt {
            bewaker.vergeet(&pad);
        } else if !verwerk(app, bewaker, uitstel, nu, &keuzes, pad, handtekening) {
            gestopt = true;
        }
    }
}

/// Een bestand dat stilstaat: overslaan als er al een transcript van is, anders de wachtrij
/// in. Geeft `false` als toevoegen mislukte en de bewaking even moet wachten.
fn verwerk(
    app: &AppHandle,
    bewaker: &mut Stilstand,
    uitstel: &mut Uitstel,
    nu: Duration,
    keuzes: &Keuzes,
    pad: PathBuf,
    handtekening: Handtekening,
) -> bool {
    let bewaakt = Bewaakt {
        pad: pad.clone(),
        handtekening,
        naast: keuzes.naast,
    };
    if keuzes.overslaan && heeft_transcript(app, &pad, handtekening.gewijzigd_ms) {
        // Zichtbaar in de lijst van Bestanden, en onthouden als gedaan.
        voeg_overgeslagen_toe(app, &bewaakt);
        return true;
    }
    let staat = app.state::<BewaakStaat>();
    match voeg_toe_bewaakt(app, &bewaakt) {
        Toevoeging::Behandeld => {
            *slot(&staat.wachtrijfout) = None;
            uitstel.gelukt();
            true
        }
        Toevoeging::Mislukt(fout) => {
            // Geen model of een gestopte wachtrij: een melding in Instellingen, en later
            // opnieuw, met een wachttijd die oploopt.
            *slot(&staat.wachtrijfout) = Some(fout.melding);
            bewaker.vergeet(&pad);
            uitstel.mislukt(nu);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn na_een_mislukte_poging_wacht_de_bewaking_langer_en_langer() {
        let s = Duration::from_secs;
        let mut uitstel = Uitstel::nieuw();
        assert!(!uitstel.wacht_nog(s(0)));
        uitstel.mislukt(s(100));
        assert!(uitstel.wacht_nog(s(129)));
        assert!(!uitstel.wacht_nog(s(130)));
        uitstel.mislukt(s(130));
        assert!(uitstel.wacht_nog(s(189)));
        assert!(!uitstel.wacht_nog(s(190)));
        // Nooit langer dan tien minuten.
        for _ in 0..10 {
            uitstel.mislukt(s(1_000));
        }
        assert!(!uitstel.wacht_nog(s(1_000) + UITSTEL_MAX));
        // Na een gelukte poging begint het weer kort.
        uitstel.gelukt();
        uitstel.mislukt(s(5_000));
        assert!(!uitstel.wacht_nog(s(5_030)));
    }

    #[test]
    fn zonder_map_geen_probleem_van_de_map_zelf() {
        assert_eq!(bepaal_probleem(None, None, None), None);
    }

    #[test]
    fn de_map_met_transcripten_en_de_gegevensmap_zijn_geen_geldige_keuze() {
        let basis = tempfile::tempdir().unwrap();
        let transcripten = basis.path().join("Mondig");
        let gegevens = basis.path().join("gegevens");
        std::fs::create_dir_all(transcripten.join("onder")).unwrap();
        std::fs::create_dir_all(&gegevens).unwrap();
        let kies = |map: &Path| bepaal_probleem(Some(map), Some(&transcripten), Some(&gegevens));
        assert_eq!(kies(&transcripten), Some(Probleem::ZelfdeAlsOpslag));
        assert_eq!(
            kies(&transcripten.join("onder")),
            Some(Probleem::ZelfdeAlsOpslag)
        );
        assert_eq!(kies(&gegevens), Some(Probleem::ZelfdeAlsOpslag));
        // De map eromheen mag wel: de map met transcripten is dan uitgesloten van het lezen.
        assert_eq!(kies(basis.path()), None);
    }

    #[test]
    fn een_map_die_niet_bestaat_is_een_probleem_en_een_bestaande_niet() {
        let basis = tempfile::tempdir().unwrap();
        assert_eq!(bepaal_probleem(Some(basis.path()), None, None), None);
        assert_eq!(
            bepaal_probleem(Some(&basis.path().join("weg")), None, None),
            Some(Probleem::MapOntbreekt)
        );
    }
}
