//! Het scherm Vergadering (sprint 4b-C, ADR 0008):
//! opnemen, uitschrijven, het verslag met namen, bewaren en wissen. De opname zelf en het
//! uitschrijven doet `mondig-core`; dit bestand houdt de toestand bij die de interface toont:
//!
//! - event `vergadering-gewijzigd`: de hele [`VergaderingStand`] na elke wijziging, met een
//!   oplopend volgnummer (zoals `bestanden-gewijzigd`);
//! - event `vergadering-niveau`: tijdens de opname het niveau per spoor ([`Niveaus`]),
//!   ongeveer tien keer per seconde;
//! - event `taak`: de gebeurtenissen van het uitschrijven (soort `vergadering`), via de
//!   wachtrij van Bestanden.
//!
//! Twee soorten (besluit, 2-11-2026): online (microfoon en het geluid van de computer,
//! "Jij" en "Anderen") en in de ruimte (alleen de microfoon, het verslag zonder sprekers). De
//! laatste keuze staat in de instellingen.
//!
//! Uitschrijven gaat door dezelfde wachtrij als Bestanden: één werkproces, één model in het
//! geheugen. Zolang er wordt opgenomen staat er een rood stipje op het taakbalkicoon en in het
//! systeemvak, en minimaliseert sluiten het venster in plaats van het te verbergen. De pil
//! verschijnt niet; de sneltoets geeft een melding in plaats van een dictaat (`dictaat.rs`).
//!
//! De audio staat alleen tijdelijk op schijf (in `AppData\Local`). Weggooien wist de opname;
//! "Later uitschrijven" laat haar staan, en dan staat ze in [`VergaderingStand::opnames`] tot ze
//! is uitgeschreven of weggegooid. Een uitgeschreven vergadering komt als `verslag.json` in de
//! map van haar opname (ADR 0022): sluiten wist niets, en het verslag staat daarna onder
//! [`VergaderingStand::verslagen`] ("Eerder opgenomen") tot de bewaartermijn om is of de
//! gebruiker het wist. De interface geeft nooit een pad: een opname of verslag heet bij zijn
//! mapnaam, en die moet in de lijst staan.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de argumenten per waarde mee; een verwijzing kan niet"
)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mondig_core::afspelen;
use mondig_core::bestanden::{Bestandsmotor, Gespreksuitslag};
use mondig_core::bewaren::{self, Document, Formaat};
use mondig_core::download;
use mondig_core::gesprek::{
    Gesprek, Kant, Naambereik, Vergadersoort, schone_verslagnaam, veranderde_beurten,
};
use mondig_core::model::{self as catalogus, Gebruik, Modelrol};
use mondig_core::notulen::OpgeslagenNotulen;

pub(crate) mod notulen_maken;
use mondig_core::opname::handy::HandyApparatuur;
use mondig_core::opname::systeem::SysteemApparatuur;
use mondig_core::opname::vergadering::{
    self, EINDE, MELDING_SYSTEEM_STIL_LIVE, MICROFOON, Niveaus, SYSTEEM_STIL_NA, Slotstand,
    Vergaderopname,
};
use mondig_core::opname::{Apparatuur, Bron, FoutSoort, OpnameFout, opgeslagen_microfoon};
use mondig_core::persoonsgegevens::Markeringen;
use mondig_core::probleem::Probleem;
use mondig_core::soevereiniteit::Soevereiniteit;
use mondig_core::spoor::OPZET;
use mondig_core::spoor::mengen::{MengFout, meng_naar_bestand};
use mondig_core::taak::{Foutmelding, Gebeurtenis, Stap, TaakId, Toestand, Vervolg};
use mondig_core::verslag::{self as verslagbestand, Bewaren};
use notulen_maken::{NotulenMaken, NotulenStand};
use serde::Serialize;
use tauri::image::Image;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::bewaren::BewaarUitslag;
use crate::dicteerketen::{DicteerStaat, slot};
use crate::persoon::{MarkeringInfo, PersoonInfo, markeerfout, markeringen_van, persoon_info};
use crate::sneltoets::{lees_instelling, lees_microfoon, schrijf_instelling};

/// Sleutel in de store: de laatst gekozen soort ("online" of "ruimte").
const SLEUTEL_SOORT: &str = "vergadering_soort";

/// De melding bij de sneltoets tijdens een opname (besluit, 2-11-2026).
pub(crate) const MELDING_DICTEREN_TIJDENS_OPNAME: &str =
    "Er loopt een vergaderopname. Dicteren kan weer als je de opname stopt.";

/// Zo vaak meet de tikker het niveau: tien keer per seconde.
const TIK: Duration = Duration::from_millis(100);

/// Zo vaak werkt de tikker de tooltip van het systeemvak bij.
const TOOLTIP_ELKE: Duration = Duration::from_secs(2);

/// De kleur van het opnamestipje (`.rec` in het ontwerp): #E5484D.
const ROOD: [u8; 3] = [0xE5, 0x48, 0x4D];

// ---------------------------------------------------------------------------
// Toestand
// ---------------------------------------------------------------------------

/// Wat er nu gebeurt. Bewust zonder `Debug`: een verslag bevat tekst.
#[derive(Default)]
enum Fase {
    /// Klaar om op te nemen.
    #[default]
    Klaar,
    /// De bronnen worden geopend (een Bluetooth-headset kan seconden duren).
    Starten { soort: Vergadersoort },
    /// De opname loopt.
    NeemtOp(Opnemen),
    /// Mondig is met een opname bezig voordat ze in de wachtrij staat: stoppen, herstellen
    /// (dat leest alle stukken en kan seconden duren) of in de wachtrij zetten. Deze fase is
    /// geclaimd onder hetzelfde slot als de controle, zodat er niets tussendoor kan.
    Voorbereiden(Voorbereiden),
    /// De opname staat in de wachtrij of wordt uitgeschreven.
    Uitschrijven(Uitschrijven),
    /// Het verslag is klaar. In een doos: het verslag is veel groter dan de andere fasen.
    Verslag(Box<Verslag>),
}

/// Een lopende opname, zoals de interface haar ziet. De opname zelf staat apart in
/// [`VergaderingStaat::opname`], zodat deze toestand zonder geluidskaart te testen is.
struct Opnemen {
    map: PathBuf,
    soort: Vergadersoort,
    begin_ms: u64,
    begin: Instant,
    niveaus: Option<Niveaus>,
    /// Is het systeemspoor al [`SYSTEEM_STIL_NA`] stil?
    systeem_stil: bool,
    /// Wat er tijdens de opname gebeurde (een bron viel uit, geen geluid van de computer).
    meldingen: Vec<String>,
}

struct Voorbereiden {
    map: PathBuf,
    soort: Vergadersoort,
    begin_ms: u64,
    duur_ms: Option<u64>,
    /// Wordt een afgebroken opname hersteld?
    herstellen: bool,
}

struct Uitschrijven {
    taak: TaakId,
    map: PathBuf,
    soort: Vergadersoort,
    begin_ms: u64,
    duur_ms: Option<u64>,
    voortgang: Option<u8>,
    /// Uitschrijven of sprekers herkennen (4b-D).
    stap: Stap,
    /// De korte naam, voor in het verslag.
    model: String,
    /// Het model waarmee deze vergadering uitschrijft. Na een modelwissel voor bestanden
    /// verschilt dat van het model in de wachtrij, dus het blijft hier staan.
    model_id: String,
}

/// Een uitgeschreven vergadering. Bewust zonder `Debug`: de tekst mag nooit in een log.
struct Verslag {
    taak: TaakId,
    map: PathBuf,
    gesprek: Gesprek,
    model: String,
    /// De id van het model waarmee het is uitgeschreven, voor in `verslag.json`.
    model_id: String,
    begin_ms: u64,
    /// De naam die de gebruiker het verslag gaf; `None` is de titel die Mondig zelf maakt.
    naam: Option<String>,
    /// De markeringen van persoonsgegevens (V-2): wat de gebruiker weghaalde of zelf markeerde, en
    /// of er bij Kopiëren en opslaan vervangen wordt. Ze staan ook in `verslag.json`.
    persoon: Markeringen,
    /// De notulen (ADR 0026), ook als deze versie ze niet kan lezen. Elke schrijfactie (naam,
    /// verbetering, opnieuw uitschrijven, terugzetten) schrijft ze mee, zodat ze nooit stil
    /// verdwijnen; na opnieuw uitschrijven laat het versiekenmerk zien dat ze bij de vorige versie
    /// horen.
    notulen: Option<OpgeslagenNotulen>,
    /// Maakt Mondig nu notulen van dit verslag? Dan weigert elke wijziging aan het verslag, net als
    /// bij opnieuw uitschrijven, en weigert sluiten.
    notulen_maken: Option<NotulenMaken>,
    /// Mislukte de laatste keer dat het verslag naar schijf ging? Dan staat er een melding in de
    /// stand, probeert de volgende wijziging het opnieuw, en weigert Sluiten (tenzij de gebruiker
    /// toch sluit).
    bewaren_mislukt: bool,
    /// De bytes van `verslag.json` zoals ze het laatst zijn gepland om te schrijven: is er na een
    /// wijziging niets veranderd, dan wordt er niet nog eens geschreven.
    bewaard_bytes: Vec<u8>,
    /// Het volgnummer van de laatst geplande schrijfopdracht ([`Binnen::plan_bewaren`]).
    laatste_volgnr: u64,
    /// Wanneer Mondig dit verslag vanzelf wist (milliseconden sinds 1970). Wordt bij openen, bij
    /// het opruimen en bij een andere bewaartermijn berekend, zodat de stand geen bestanden leest.
    wist_ms: Option<u64>,
    /// Wordt de opname nu als WAV bewaard? Dan leest het mengen de sporen, en mag de map
    /// niet weg.
    audio_bezig: bool,
    /// Hoeveel stukken er nu worden gemengd om af te spelen (V-1). Zolang dat er zijn, wordt de
    /// map niet gewist: op Windows faalt wissen met een open bestand.
    luistert: usize,
    /// Wordt dit verslag opnieuw uitgeschreven (ADR 0023)? Dan blijft het te lezen, te kopiëren
    /// en op te slaan, maar weigert elke wijziging, en sluit het niet.
    opnieuw: Option<Opnieuw>,
    /// De versie van voor het laatste opnieuw uitschrijven, met zijn markeringen, tot het
    /// verslag sluit (besluit 8). Alleen in het geheugen: na een crash is ze weg.
    vorige: Option<Box<Vorige>>,
    /// Wat de melding na het opnieuw uitschrijven zegt.
    na_opnieuw: Option<NaOpnieuw>,
    /// Per beurt: veranderd bij het laatste opnieuw uitschrijven? Zo markeert de interface die
    /// regels even. Leeg na een wijziging die beurten laat schuiven.
    veranderd: Vec<bool>,
    /// Staan de sporen van de opname er nog (bij openen en bij het bijwerken van de lijsten
    /// gekeken, niet bij elke stand)?
    opname_aanwezig: bool,
}

/// Een verslag dat opnieuw wordt uitgeschreven (ADR 0023).
struct Opnieuw {
    /// De taak in de wachtrij (soort vergadering).
    taak: TaakId,
    model_id: String,
    /// De korte naam van het model, voor in het verslag.
    model: String,
    /// Hoe het model in de interface heet.
    rol: Option<Modelrol>,
    voortgang: Option<u8>,
    /// Sinds de wachtrij eraan begon (met het laden van het model).
    begonnen: Option<Instant>,
    /// Het eerste percentage en wanneer dat kwam: vanaf hier telt alleen het uitschrijven zelf,
    /// zonder het laden. Daarmee rekenen de resterende tijd en de gemeten snelheid.
    meetpunt: Option<(Instant, u8)>,
    /// De duur volgens de gemeten snelheid van het model, voor de resterende tijd zolang er nog
    /// geen voortgang is.
    verwacht: Option<Duration>,
}

impl Opnieuw {
    fn stand(&self) -> OpnieuwStand {
        let rest = match (self.voortgang, self.meetpunt) {
            (Some(p), Some((sinds, p0))) => resterend(p, p0, sinds.elapsed()),
            _ => None,
        }
        .or_else(|| {
            self.verwacht
                .map(|v| v.saturating_sub(self.begonnen.map(|b| b.elapsed()).unwrap_or_default()))
        });
        OpnieuwStand {
            rol: self.rol,
            voortgang: self.voortgang,
            rest_ms: rest.map(ms),
        }
    }

    /// Een nieuw percentage: het eerste wordt het meetpunt.
    fn voortgang(&mut self, voortgang: Option<u8>) {
        self.begonnen.get_or_insert_with(Instant::now);
        if let Some(p) = voortgang
            && self.meetpunt.is_none()
        {
            self.meetpunt = Some((Instant::now(), p));
        }
        self.voortgang = voortgang;
    }
}

/// De resterende tijd bij percentage `p`, als het uitschrijven bij `p0` begon te tellen en
/// sindsdien `verstreken` duurde. `None` zolang er sinds het meetpunt niets bij kwam.
fn resterend(p: u8, p0: u8, verstreken: Duration) -> Option<Duration> {
    if p >= 100 {
        return Some(Duration::ZERO);
    }
    (p > p0).then(|| verstreken.mul_f64(f64::from(100 - p) / f64::from(p - p0)))
}

/// Het verslag van voor het opnieuw uitschrijven. De markeringen zitten er niet in: die veranderden
/// niet door het opnieuw uitschrijven, en een markering van daarna blijft zo bij terugzetten.
struct Vorige {
    gesprek: Gesprek,
    model: String,
    model_id: String,
}

/// Wat het opnieuw uitschrijven veranderde, berekend buiten het slot van de stand (punt 8 van
/// de review): per nieuwe beurt of hij veranderde, en hoeveel eigen markeringen wegvielen.
struct Vergelijking {
    veranderd: Vec<bool>,
    verdwenen: usize,
}

impl Vergelijking {
    fn bereken(oud: &Gesprek, persoon: &Markeringen, naam: Option<&str>, nieuw: &Gesprek) -> Self {
        let teksten: Vec<&str> = nieuw
            .beurten
            .iter()
            .flat_map(|b| std::iter::once(b.tekst.as_str()).chain(b.naam.as_deref()))
            .chain(naam)
            .collect();
        Self {
            veranderd: veranderde_beurten(&oud.beurten, &nieuw.beurten),
            verdwenen: persoon.verdwenen(&teksten).len(),
        }
    }
}

/// Wat het opnieuw uitschrijven van het open verslag meekrijgt: de map, het gesprek zoals het nu
/// is (voor de sprekers, en om na afloop buiten het slot te vergelijken), de markeringen en de naam.
struct VoorOpnieuw {
    map: PathBuf,
    oud: Gesprek,
    persoon: Markeringen,
    naam: Option<String>,
}

/// Wat er na het opnieuw uitschrijven veranderde (ADR 0023, punten 2 en 5).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NaOpnieuw {
    /// Hoe het nieuwe model heet: "Opnieuw uitgeschreven met het nauwkeurige model."
    rol: Option<Modelrol>,
    /// Hoeveel regels er veranderden.
    veranderd: usize,
    /// Hoeveel eigen markeringen niet meer in de tekst staan.
    verdwenen: usize,
}

/// Het opnieuw uitschrijven zoals de interface het toont.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpnieuwStand {
    rol: Option<Modelrol>,
    voortgang: Option<u8>,
    /// De resterende tijd in milliseconden, als die te schatten is.
    rest_ms: Option<u64>,
}

/// Een opname die nog in de tijdelijke map staat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpnameInfo {
    /// De mapnaam: zo heet de opname in de commando's.
    id: String,
    soort: Vergadersoort,
    begin_ms: u64,
    /// De duur (bij een afgebroken opname geschat uit de stukken op schijf).
    duur_ms: Option<u64>,
    /// Netjes afgesloten ("Later uitschrijven"); anders afgebroken en eerst te herstellen.
    afgesloten: bool,
    /// Kan Mondig niet nagaan of deze opname nog loopt (het slot is niet te lezen)?
    onzeker: bool,
    /// Wanneer Mondig de opname vanzelf wist (milliseconden sinds 1970): een dag na het markeren.
    /// `None` als Mondig de bewaartermijn niet kent en dus niets wist.
    wist_ms: Option<u64>,
}

/// Een bewaard verslag in de lijst "Eerder opgenomen". Bewust zonder `Debug`: de naam is van de
/// gebruiker en mag nooit in een log.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VerslagInfo {
    /// De mapnaam: zo heet het verslag in de commando's.
    id: String,
    /// De naam die de gebruiker het gaf; zonder naam maakt de interface de titel uit soort en begin.
    naam: Option<String>,
    soort: Vergadersoort,
    begin_ms: u64,
    duur_ms: u64,
    /// Hoeveel sprekers het verslag heeft, als ze herkend zijn.
    sprekers: Option<usize>,
    /// Wanneer Mondig het verslag vanzelf wist (milliseconden sinds 1970); `None` als de
    /// bewaartermijn onbekend is en Mondig dus niets wist.
    wist_ms: Option<u64>,
    /// Staan de sporen er nog, zodat het verslag opnieuw kan worden uitgeschreven (ADR 0023)?
    opname_aanwezig: bool,
}

/// De opnames die nog wachten en de bewaarde verslagen, zoals ze in de tijdelijke map staan.
#[derive(Default)]
struct Lijsten {
    opnames: Vec<OpnameInfo>,
    verslagen: Vec<VerslagInfo>,
}

/// Wat de app over vergaderingen bijhoudt, achter één slot.
#[derive(Default)]
struct Binnen {
    versie: u64,
    fase: Fase,
    opnames: Vec<OpnameInfo>,
    /// De bewaarde verslagen, nieuwste eerst. Het verslag dat nu open is, staat er niet in.
    verslagen: Vec<VerslagInfo>,
    /// Mapnamen die nu worden gewist: ze staan in geen lijst en `ruim_op` slaat ze over tot het
    /// wissen klaar is.
    wordt_gewist: Vec<String>,
    /// Telt de schrijfopdrachten voor `verslag.json`, zodat een oudere nooit een nieuwere overschrijft.
    schrijf_teller: u64,
    fout: Option<Foutmelding>,
    laatste_soort: Vergadersoort,
    /// De bewaartermijn in dagen. `None` is onbekend (de instelling is nog niet gelezen, of
    /// onleesbaar of ongeldig): dan wist Mondig niets. Wordt elk uur opnieuw gelezen.
    bewaar_dagen: Option<u32>,
    /// Een gemeten snelheid (model-id en realtime-factor) van het laatste geslaagde
    /// uitschrijven, die de aanroeper buiten het slot in de store zet (ADR 0023).
    meting: Option<(String, f64)>,
}

/// Een beurt zoals de interface hem toont.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BeurtInfo {
    /// De plek in het verslag: zo heet de beurt in `vergadering_naam`.
    index: usize,
    kant: Kant,
    /// Welke spreker van die kant (vanaf 1), als de sprekers herkend zijn (4b-D). De interface
    /// maakt er "Spreker 1" van in de taal van de gebruiker als `eigen_naam` niet waar is.
    sprekernummer: Option<u8>,
    /// Wie er spreekt ("Jij", "Anderen", "Spreker 1" of een gegeven naam); `None` in de ruimte
    /// zonder herkende sprekers en bij Alleen ik.
    spreker: Option<String>,
    /// Heeft de gebruiker deze beurt een eigen naam gegeven?
    eigen_naam: bool,
    start_ms: u64,
    eind_ms: u64,
    tekst: String,
    /// De persoonsgegevens in de tekst (V-2), in UTF-16-eenheden.
    markeringen: Vec<MarkeringInfo>,
    /// Veranderd bij het laatste opnieuw uitschrijven (ADR 0023): de interface markeert de regel even.
    veranderd: bool,
}

/// De toestand zoals de interface hem toont.
#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[expect(
    clippy::large_enum_variant,
    reason = "er is één stand tegelijk, en die gaat meteen naar de interface; een doos per veld zou niets winnen"
)]
pub(crate) enum StandToestand {
    /// Klaar om op te nemen; de herinnering toont de interface zelf.
    Klaar,
    /// De bronnen worden geopend.
    Starten { soort: Vergadersoort },
    /// Mondig stopt, herstelt of zet de opname in de wachtrij; zo meteen volgt
    /// `uitschrijven` of `klaar`.
    Voorbereiden {
        soort: Vergadersoort,
        begin_ms: u64,
        duur_ms: Option<u64>,
        herstellen: bool,
    },
    /// De opname loopt sinds `begin_ms` (milliseconden sinds 1970).
    NeemtOp {
        soort: Vergadersoort,
        begin_ms: u64,
        duur_ms: u64,
        /// Het laatst gemeten niveau; live komt het via `vergadering-niveau`.
        niveaus: Option<Niveaus>,
        /// De waarschuwing als er al een minuut geen geluid van de computer komt.
        systeem_stil: Option<String>,
        meldingen: Vec<String>,
    },
    /// Uitschrijven, met de taak in de `taak`-events.
    Uitschrijven {
        taak: TaakId,
        soort: Vergadersoort,
        begin_ms: u64,
        duur_ms: Option<u64>,
        voortgang: Option<u8>,
        /// Uitschrijven of, daarna, sprekers herkennen (4b-D): dan toont het scherm dat in
        /// plaats van het percentage, want het rekenen meldt zich niet.
        stap: Stap,
        model: String,
    },
    /// Het verslag.
    Verslag {
        taak: TaakId,
        soort: Vergadersoort,
        begin_ms: u64,
        duur_ms: u64,
        model: String,
        soevereiniteit: Soevereiniteit,
        meldingen: Vec<String>,
        /// Hoeveel uitingen van Jij als echo zijn weggelaten.
        weggelaten: usize,
        beurten: Vec<BeurtInfo>,
        /// Staat de opname er nog, zodat een beurt kan worden afgespeeld (V-1)?
        audio_aanwezig: bool,
        /// De stand van "Persoonsgegevens markeren en vervangen" (V-2).
        persoon: PersoonInfo,
        /// De naam die de gebruiker het verslag gaf; `None`: de interface maakt de titel zelf.
        naam: Option<String>,
        /// Wanneer Mondig het verslag vanzelf wist (milliseconden sinds 1970); `None` als de
        /// bewaartermijn onbekend is.
        wist_ms: Option<u64>,
        /// Kon Mondig de laatste wijzigingen niet naast de opname bewaren? Dan weigert Sluiten,
        /// en biedt de interface "Toch sluiten" aan.
        bewaren_mislukt: bool,
        /// De id van het model waarmee het verslag is uitgeschreven.
        model_id: String,
        /// Staan de sporen van de opname er nog, zodat het verslag opnieuw kan worden
        /// uitgeschreven (ADR 0023)?
        opname_aanwezig: bool,
        /// De rol van het model, voor de gegevensregel (N-B: rol, geen naam). `None` als het geen
        /// spraakmodel uit de catalogus is; dan toont de interface de naam.
        model_rol: Option<Modelrol>,
        /// Hoeveel regels de gebruiker zelf verbeterde.
        verbeterd: usize,
        /// Hoeveel geluid het uitschrijven verwerkt, in milliseconden (de duur keer het aantal
        /// sporen), voor de tijdsschatting in de dialoog.
        geluid_ms: u64,
        /// Het opnieuw uitschrijven dat nu loopt.
        opnieuw: Option<OpnieuwStand>,
        /// Wat er na het laatste opnieuw uitschrijven veranderde, voor de melding.
        na_opnieuw: Option<NaOpnieuw>,
        /// Is "Vorige versie terugzetten" er nog?
        vorige_versie: bool,
        /// De notulen van dit verslag en het maken ervan (ADR 0026).
        notulen: Box<NotulenStand>,
    },
}

impl StandToestand {
    /// Wat het menu van het systeemvak hiervan moet weten: of er een opname loopt (en hoe lang),
    /// of de app met een opname bezig is waardoor een nieuwe niet kan. Een open verslag telt
    /// mee, want `Binnen::claim` weigert dan een nieuwe opname.
    fn systeemvak(&self) -> (crate::systeemvak::OpnameStand, Duration) {
        use crate::systeemvak::OpnameStand;
        match self {
            Self::Klaar => (OpnameStand::Rust, Duration::ZERO),
            Self::NeemtOp { duur_ms, .. } => (OpnameStand::Loopt, Duration::from_millis(*duur_ms)),
            Self::Starten { .. } | Self::Voorbereiden { .. } | Self::Uitschrijven { .. } => {
                (OpnameStand::Bezig, Duration::ZERO)
            }
            // Sluiten wist niets meer (ADR 0022): "Opname starten" sluit het verslag eerst. Dat
            // weigert alleen als bewaren mislukte of opnieuw uitschrijven loopt.
            Self::Verslag {
                bewaren_mislukt,
                opnieuw,
                notulen,
                ..
            } => {
                if *bewaren_mislukt || opnieuw.is_some() || notulen.maken.is_some() {
                    (OpnameStand::VerslagVast, Duration::ZERO)
                } else {
                    (OpnameStand::VerslagOpen, Duration::ZERO)
                }
            }
        }
    }
}

/// De hele stand van het scherm.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VergaderingStand {
    versie: u64,
    toestand: StandToestand,
    /// Opnames die nog uitgeschreven of hersteld kunnen worden, oudste eerst.
    opnames: Vec<OpnameInfo>,
    /// Bewaarde verslagen ("Eerder opgenomen"), nieuwste eerst.
    verslagen: Vec<VerslagInfo>,
    /// De laatste fout (starten, uitschrijven, wissen), tot de volgende handeling.
    fout: Option<Foutmelding>,
    /// De laatst gekozen soort, voor de keuze bij de start.
    laatste_soort: Vergadersoort,
}

fn ms(duur: Duration) -> u64 {
    u64::try_from(duur.as_millis()).unwrap_or(u64::MAX)
}

fn nu_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, ms)
}

impl Binnen {
    /// Schrijft er een vergadering uit met dit model (id)? Het model dat bij de start van het
    /// uitschrijven gold, niet het model dat de wachtrij nu heeft.
    fn schrijft_uit_met(&self, model_id: &str) -> bool {
        match &self.fase {
            Fase::Uitschrijven(u) => u.model_id == model_id,
            Fase::Verslag(v) => v.opnieuw.as_ref().is_some_and(|o| o.model_id == model_id),
            _ => false,
        }
    }

    /// Wat er nodig is om het open verslag opnieuw uit te schrijven: de map en het gesprek zoals
    /// het nu is. Weigert als er geen verslag open is, als het al loopt of als de sporen weg zijn.
    fn voor_opnieuw(&self) -> Result<VoorOpnieuw, Foutmelding> {
        let Fase::Verslag(v) = &self.fase else {
            return Err(geen_verslag());
        };
        if v.opnieuw.is_some() {
            return Err(Foutmelding::nieuw(
                "Mondig schrijft dit verslag al opnieuw uit.",
                Vervolg::Geen,
            ));
        }
        if v.notulen_maken.is_some() {
            return Err(notulen_bezig());
        }
        if !sporen_aanwezig(&v.map) {
            return Err(Foutmelding::nieuw(
                "Opnieuw uitschrijven kan niet meer: de opname is gewist.",
                Vervolg::Geen,
            ));
        }
        Ok(VoorOpnieuw {
            map: v.map.clone(),
            oud: v.gesprek.clone(),
            persoon: v.persoon.clone(),
            naam: v.naam.clone(),
        })
    }

    /// Het opnieuw uitschrijven staat in de wachtrij: vanaf nu weigert het verslag wijzigingen.
    fn begin_opnieuw(&mut self, opnieuw: Opnieuw) {
        if let Fase::Verslag(v) = &mut self.fase {
            v.opnieuw = Some(opnieuw);
            v.na_opnieuw = None;
            self.fout = None;
        }
    }

    /// Stoppen: het opnieuw uitschrijven telt niet meer, het verslag blijft zoals het was. Geeft
    /// de taak om in de wachtrij te annuleren. Een uitslag die daarna nog komt, hoort nergens bij.
    fn stop_opnieuw(&mut self) -> Option<TaakId> {
        match &mut self.fase {
            Fase::Verslag(v) => v.opnieuw.take().map(|o| o.taak),
            _ => None,
        }
    }

    /// Zet de versie van voor het opnieuw uitschrijven terug, met haar model, en plant het
    /// schrijven ervan naar `verslag.json`. De markeringen blijven zoals ze nu zijn.
    fn zet_vorige_terug(&mut self) -> Result<Option<Schrijfopdracht>, Foutmelding> {
        let Fase::Verslag(v) = &mut self.fase else {
            return Err(geen_verslag());
        };
        if v.opnieuw.is_some() {
            return Err(opnieuw_bezig());
        }
        if v.notulen_maken.is_some() {
            return Err(notulen_bezig());
        }
        let Some(vorige) = v.vorige.take() else {
            return Err(Foutmelding::nieuw(
                "Er is geen vorige versie van dit verslag.",
                Vervolg::Geen,
            ));
        };
        v.gesprek = vorige.gesprek;
        v.model = vorige.model;
        v.model_id = vorige.model_id;
        v.veranderd.clear();
        v.na_opnieuw = None;
        v.hercontroleer_notulen();
        Ok(self.plan_bewaren(false))
    }

    /// De gemeten snelheid van het laatste geslaagde uitschrijven, om in de store te zetten.
    fn neem_meting(&mut self) -> Option<(String, f64)> {
        self.meting.take()
    }

    fn stand(&self) -> VergaderingStand {
        let toestand = match &self.fase {
            Fase::Klaar => StandToestand::Klaar,
            Fase::Starten { soort } => StandToestand::Starten { soort: *soort },
            Fase::NeemtOp(o) => StandToestand::NeemtOp {
                soort: o.soort,
                begin_ms: o.begin_ms,
                duur_ms: ms(o.begin.elapsed()),
                niveaus: o.niveaus,
                systeem_stil: o
                    .systeem_stil
                    .then(|| MELDING_SYSTEEM_STIL_LIVE.to_string()),
                meldingen: o.meldingen.clone(),
            },
            Fase::Voorbereiden(v) => StandToestand::Voorbereiden {
                soort: v.soort,
                begin_ms: v.begin_ms,
                duur_ms: v.duur_ms,
                herstellen: v.herstellen,
            },
            Fase::Uitschrijven(u) => StandToestand::Uitschrijven {
                taak: u.taak,
                soort: u.soort,
                begin_ms: u.begin_ms,
                duur_ms: u.duur_ms,
                voortgang: u.voortgang,
                stap: u.stap,
                model: u.model.clone(),
            },
            Fase::Verslag(v) => StandToestand::Verslag {
                taak: v.taak,
                soort: v.gesprek.soort,
                begin_ms: v.begin_ms,
                duur_ms: ms(v.gesprek.duur),
                model: v.model.clone(),
                soevereiniteit: Soevereiniteit::Lokaal,
                meldingen: v.gesprek.meldingen.clone(),
                weggelaten: v.gesprek.weggelaten,
                beurten: beurten_van(&v.gesprek, &v.persoon, &v.veranderd),
                audio_aanwezig: v.map.join(EINDE).is_file(),
                persoon: persoon_info(&v.persoon),
                naam: v.naam.clone(),
                wist_ms: v.wist_ms,
                bewaren_mislukt: v.bewaren_mislukt,
                model_id: v.model_id.clone(),
                opname_aanwezig: v.opname_aanwezig,
                model_rol: catalogus::rol_van(&v.model_id),
                verbeterd: v.gesprek.verbeterde_regels(),
                geluid_ms: ms(v.gesprek.verwerkt_geluid()),
                opnieuw: v.opnieuw.as_ref().map(Opnieuw::stand),
                na_opnieuw: v.na_opnieuw.clone(),
                vorige_versie: v.vorige.is_some(),
                notulen: Box::new(NotulenStand::van(v)),
            },
        };
        VergaderingStand {
            versie: self.versie,
            toestand,
            opnames: self.opnames.clone(),
            verslagen: self.verslagen.clone(),
            fout: self.fout.clone(),
            laatste_soort: self.laatste_soort,
        }
    }

    /// De bewaartermijn waarmee de lijst en het opruimen rekenen; `None` als die onbekend is.
    fn bewaartijd(&self) -> Option<Duration> {
        self.bewaar_dagen.map(vergadering::bewaartijd)
    }

    /// Leest de lijsten met opnames en verslagen opnieuw van schijf, en rekent de wistijd van het
    /// verslag dat open is opnieuw uit. Mappen die nu worden gewist, staan er niet in.
    fn lees_lijsten(&mut self, basis: &Path, nu: u64) {
        let lijsten = zoek_lijsten(basis, self.actieve_map(), self.bewaartijd(), nu);
        self.opnames = lijsten.opnames;
        self.verslagen = lijsten.verslagen;
        let weg = &self.wordt_gewist;
        self.opnames.retain(|o| !weg.contains(&o.id));
        self.verslagen.retain(|v| !weg.contains(&v.id));
        let tijd = self.bewaartijd();
        if let Fase::Verslag(v) = &mut self.fase {
            v.wist_ms = tijd.and_then(|t| vergadering::wistijd_ms(&v.map, t, nu));
            v.opname_aanwezig = sporen_aanwezig(&v.map);
        }
    }

    /// Plant het schrijven van het verslag dat open is naar `verslag.json` (ADR 0022): serialiseert
    /// onder het slot en geeft een opdracht die de aanroeper buiten het slot uitvoert. `None` als
    /// er geen verslag open is, of als er niets veranderde sinds de vorige opdracht. Met `forceer`
    /// of na een mislukte poging wordt er altijd een opdracht gemaakt.
    fn plan_bewaren(&mut self, forceer: bool) -> Option<Schrijfopdracht> {
        let Fase::Verslag(v) = &mut self.fase else {
            return None;
        };
        let bytes = v.bytes()?;
        if !forceer && !v.bewaren_mislukt && bytes == v.bewaard_bytes {
            return None;
        }
        self.schrijf_teller += 1;
        v.laatste_volgnr = self.schrijf_teller;
        v.bewaard_bytes.clone_from(&bytes);
        Some(Schrijfopdracht {
            map: v.map.clone(),
            bytes,
            volgnr: self.schrijf_teller,
            taak: v.taak,
        })
    }

    /// Een opdracht voor een verslag waarvan de laatste poging mislukte; anders `None`.
    fn plan_herstel(&mut self) -> Option<Schrijfopdracht> {
        match &self.fase {
            Fase::Verslag(v) if v.bewaren_mislukt => self.plan_bewaren(true),
            _ => None,
        }
    }

    /// Verwerkt de uitslag van een schrijfopdracht. Een schrijffout breekt het verslag niet: het
    /// blijft open, de stand krijgt een melding in gewone taal, en de volgende wijziging probeert
    /// het opnieuw. Een gelukte poging haalt die melding weg. Alleen de uitslag van de laatste
    /// opdracht van het verslag telt; `Ok(false)` is een opdracht die een nieuwere voorbijging.
    fn na_schrijven(&mut self, opdracht: &Schrijfopdracht, uitslag: std::io::Result<bool>) {
        let Fase::Verslag(v) = &mut self.fase else {
            return;
        };
        if v.taak != opdracht.taak || v.laatste_volgnr != opdracht.volgnr {
            return;
        }
        match uitslag {
            Ok(false) => {}
            Ok(true) => {
                if std::mem::take(&mut v.bewaren_mislukt) {
                    self.fout = None;
                }
            }
            Err(fout) => {
                v.bewaren_mislukt = true;
                self.fout = Some(bewaren_mislukt(Some(fout.kind())));
            }
        }
    }

    /// Schrijft het verslag dat open is meteen, onder het slot van de aanroeper. Alleen voor het
    /// moment dat het uitschrijven klaar is (op de thread van de wachtrij): er is dan nog niets
    /// anders dat het bestand schrijft.
    fn schrijf_nu(&mut self) {
        if let Some(opdracht) = self.plan_bewaren(true) {
            let uitslag = schrijf_bytes_gelogd(&opdracht).map(|()| true);
            self.na_schrijven(&opdracht, uitslag);
        }
    }

    /// De map van de opname waar de app nu mee bezig is, als die er is.
    fn actieve_map(&self) -> Option<&Path> {
        match &self.fase {
            Fase::NeemtOp(o) => Some(&o.map),
            Fase::Voorbereiden(v) => Some(&v.map),
            Fase::Uitschrijven(u) => Some(&u.map),
            Fase::Verslag(v) => Some(&v.map),
            Fase::Klaar | Fase::Starten { .. } => None,
        }
    }

    /// Claimt een nieuwe fase, maar alleen vanuit Klaar. De controle en de overgang gebeuren
    /// in één keer, onder het slot van de aanroeper: twee commando's tegelijk kunnen dus nooit
    /// allebei slagen.
    fn claim(&mut self, nieuw: Fase) -> Result<(), Foutmelding> {
        let bezet = match self.fase {
            Fase::Klaar => None,
            Fase::Starten { .. } | Fase::NeemtOp(_) => Some("Er loopt al een opname."),
            Fase::Voorbereiden(_) => Some("Mondig is nog bezig met de vorige opname. Even geduld."),
            Fase::Uitschrijven(_) => Some(
                "Mondig schrijft nog een vergadering uit. Wacht tot dat klaar is, of kies Later uitschrijven.",
            ),
            Fase::Verslag(_) => Some("Sluit eerst het verslag van de vorige vergadering."),
        };
        if let Some(melding) = bezet {
            return Err(Foutmelding::nieuw(melding, Vervolg::Geen));
        }
        self.fase = nieuw;
        self.fout = None;
        Ok(())
    }

    /// Is de app aan het voorbereiden van precies deze opname? Alleen dan mag die naar de
    /// wachtrij of terug naar Klaar.
    fn bereidt_voor(&self, map: &Path) -> bool {
        matches!(&self.fase, Fase::Voorbereiden(v) if v.map == map)
    }

    /// Zet de fase terug naar Klaar, maar alleen als de app nog deze opname voorbereidde.
    fn klaar_na_voorbereiden(&mut self, map: &Path, fout: Option<Foutmelding>) {
        if self.bereidt_voor(map) {
            self.fase = Fase::Klaar;
            if fout.is_some() {
                self.fout = fout;
            }
        }
    }

    /// Haalt het verslag weg om het te sluiten of te wissen, met de map van zijn opname. Weigert
    /// zolang de opname als WAV wordt bewaard of er geluid wordt klaargezet: dan leest het mengen
    /// de map nog.
    fn neem_verslag(&mut self) -> Result<PathBuf, Foutmelding> {
        match &self.fase {
            Fase::Verslag(v) if v.bezig_met_werk() => Err(bezig_sluiten(v)),
            Fase::Verslag(v) if v.audio_bezig => Err(Foutmelding::nieuw(
                "Mondig bewaart de opname nog. Wacht tot dat klaar is.",
                Vervolg::OpnieuwProberen,
            )),
            Fase::Verslag(v) if v.luistert > 0 => Err(speelt_nog()),
            Fase::Verslag(_) => {
                let Fase::Verslag(v) = std::mem::take(&mut self.fase) else {
                    return Err(geen_verslag());
                };
                Ok(v.map)
            }
            _ => Err(geen_verslag()),
        }
    }

    /// Sluit het verslag zonder iets te wissen (ADR 0022): het staat daarna in de lijst "Eerder
    /// opgenomen". Is de laatste schrijfpoging mislukt (de aanroeper probeerde het net nog eens),
    /// dan weigert dit met een melding, tenzij de gebruiker `toch` sluit. De melding over het niet
    /// bewaarde verslag blijft in dat geval in de stand staan: ze verdwijnt nooit stil. Geeft het
    /// taak-id van het gesloten verslag. Weigert ook onder de voorwaarden van [`Binnen::neem_verslag`].
    fn sluit_verslag(&mut self, toch: bool) -> Result<TaakId, Foutmelding> {
        let (taak, mislukt) = match &self.fase {
            Fase::Verslag(v) if v.bezig_met_werk() => return Err(bezig_sluiten(v)),
            Fase::Verslag(v) => (v.taak, v.bewaren_mislukt),
            _ => return Err(geen_verslag()),
        };
        if mislukt && !toch {
            return Err(Foutmelding::nieuw(
                "Mondig kon je laatste wijzigingen niet bewaren. Kopieer of sla het verslag op, of sluit toch.",
                Vervolg::Geen,
            ));
        }
        self.neem_verslag()?;
        if mislukt {
            if self.fout.is_none() {
                self.fout = Some(bewaren_mislukt(None));
            }
        } else {
            self.fout = None;
        }
        Ok(taak)
    }

    /// Opent een bewaard verslag uit de lijst "Eerder opgenomen" (ADR 0022). Kan alleen vanuit
    /// Klaar of vanuit een open verslag; dat laatste gaat eerst dicht, zonder wissen
    /// ([`Binnen::sluit_verslag`], dus met dezelfde weigering en `toch`), en dan geeft dit zijn
    /// taak-id terug. Is het gevraagde verslag het open verslag, dan gebeurt er niets. Een verslag
    /// dat niet meer te lezen is, gaat opzij en staat daarna (met zijn geluid) onder "Nog uit te
    /// schrijven".
    fn open_verslag(
        &mut self,
        basis: &Path,
        id: &str,
        toch: bool,
    ) -> Result<Option<TaakId>, Foutmelding> {
        let bezig = |melding: &str| Err(Foutmelding::nieuw(melding, Vervolg::Geen));
        match &self.fase {
            Fase::Klaar | Fase::Verslag(_) => {}
            Fase::Starten { .. } | Fase::NeemtOp(_) => {
                return bezig("Er loopt een opname. Open een verslag als je haar hebt gestopt.");
            }
            Fase::Voorbereiden(_) => {
                return bezig("Mondig is nog bezig met de vorige opname. Even geduld.");
            }
            Fase::Uitschrijven(_) => {
                return bezig(
                    "Mondig schrijft nog een vergadering uit. Open een verslag als dat klaar is.",
                );
            }
        }
        if let Fase::Verslag(v) = &self.fase
            && v.map.file_name().is_some_and(|n| n == id)
        {
            return Ok(None);
        }
        let (map, info) = verslag_van(self, basis, id)?;
        let Some(bewaard) = verslagbestand::lees(&map) else {
            verslagbestand::zet_opzij(&map);
            self.lees_lijsten(basis, nu_ms());
            return Err(Foutmelding::nieuw(
                "Dit verslag is niet te lezen. De opname staat nu onder Nog uit te schrijven, dan kun je haar opnieuw uitschrijven.",
                Vervolg::Geen,
            ));
        };
        // Het open verslag gaat dicht, of weigert (niet bewaard, of er wordt nog geluid klaargezet).
        let gesloten = if matches!(self.fase, Fase::Verslag(_)) {
            Some(self.sluit_verslag(toch)?)
        } else {
            self.fout = None;
            None
        };
        let mut verslag = Verslag {
            model: bewaard.model,
            model_id: bewaard.model_id,
            begin_ms: info.begin_ms,
            naam: bewaard.naam,
            persoon: bewaard.markeringen,
            notulen: bewaard.notulen,
            ..Verslag::leeg(TaakId::nieuw(), map, bewaard.gesprek)
        };
        verslag.nummer_notulenpunten();
        // Zo schrijft een wijziging die niets veranderde, niets.
        verslag.bewaard_bytes = verslag.bytes().unwrap_or_default();
        self.fase = Fase::Verslag(Box::new(verslag));
        self.lees_lijsten(basis, nu_ms());
        Ok(gesloten)
    }

    /// Een gebeurtenis van de wachtrij: voortgang, of een fout (dan blijft de opname staan).
    fn bij_gebeurtenis(&mut self, gebeurtenis: &Gebeurtenis) -> bool {
        if let Fase::Verslag(v) = &mut self.fase {
            return bij_opnieuw_gebeurtenis(v, gebeurtenis, &mut self.fout);
        }
        let Fase::Uitschrijven(u) = &mut self.fase else {
            return false;
        };
        if u.taak != gebeurtenis.taak {
            return false;
        }
        match &gebeurtenis.toestand {
            Toestand::Verwerkt { voortgang, stap } => {
                if u.voortgang == *voortgang && u.stap == *stap {
                    return false;
                }
                u.voortgang = *voortgang;
                u.stap = *stap;
                true
            }
            Toestand::Fout { melding, vervolg } => {
                self.fout = Some(Foutmelding::nieuw(melding.clone(), *vervolg));
                self.fase = Fase::Klaar;
                true
            }
            // Geannuleerd buiten "Later uitschrijven" om (bijvoorbeeld via de wachtrij van
            // Bestanden): net als later uitschrijven, de opname blijft staan.
            Toestand::Geannuleerd => {
                self.fase = Fase::Klaar;
                true
            }
            _ => false,
        }
    }

    /// De uitslag van de wachtrij. Alleen voor de taak die nu loopt: na "Later uitschrijven"
    /// of weggooien hoort de uitslag nergens meer bij.
    fn bij_uitslag(&mut self, taak: TaakId, uitslag: Gespreksuitslag) -> bool {
        if matches!(self.fase, Fase::Verslag(_)) {
            return self.bij_opnieuw_uitslag(taak, uitslag, None);
        }
        let Fase::Uitschrijven(u) = &self.fase else {
            return false;
        };
        if u.taak != taak {
            return false;
        }
        match uitslag {
            Gespreksuitslag::Klaar(gesprek) => {
                let Fase::Uitschrijven(u) = std::mem::take(&mut self.fase) else {
                    return false;
                };
                self.fase = Fase::Verslag(Box::new(Verslag {
                    model: u.model,
                    model_id: u.model_id,
                    begin_ms: u.begin_ms,
                    ..Verslag::leeg(taak, u.map, gesprek)
                }));
                self.fout = None;
                // Het verslag staat vanaf nu naast de opname, ook als Mondig meteen afsluit.
                self.schrijf_nu();
            }
            Gespreksuitslag::Fout(fout) => {
                self.fout = Some(Foutmelding::from(fout));
                self.fase = Fase::Klaar;
            }
            // Geannuleerd: zoals "Later uitschrijven", de opname blijft staan.
            Gespreksuitslag::Geannuleerd => self.fase = Fase::Klaar,
        }
        true
    }

    /// De uitslag van het opnieuw uitschrijven van het open verslag (ADR 0023). Alleen voor de
    /// taak die nu loopt: na Stoppen hoort de uitslag nergens meer bij. Klaar: het nieuwe gesprek
    /// komt in het verslag en meteen in `verslag.json`, het oude blijft als vorige versie. Een
    /// fout laat het oude verslag staan, met een melding.
    /// `vergelijking` is buiten het slot berekend; zonder (alleen in tests) gebeurt het hier.
    fn bij_opnieuw_uitslag(
        &mut self,
        taak: TaakId,
        uitslag: Gespreksuitslag,
        vergelijking: Option<Vergelijking>,
    ) -> bool {
        let Fase::Verslag(v) = &mut self.fase else {
            return false;
        };
        if v.opnieuw.as_ref().is_none_or(|o| o.taak != taak) {
            return false;
        }
        let Some(opnieuw) = v.opnieuw.take() else {
            return false;
        };
        match uitslag {
            Gespreksuitslag::Klaar(nieuw) => {
                self.meting = meting(&opnieuw, &nieuw);
                let vergelijking = vergelijking.unwrap_or_else(|| {
                    Vergelijking::bereken(&v.gesprek, &v.persoon, v.naam.as_deref(), &nieuw)
                });
                v.vervang_door(nieuw, opnieuw, vergelijking);
                self.fout = None;
                self.schrijf_nu();
            }
            Gespreksuitslag::Fout(fout) => {
                self.fout = Some(opnieuw_mislukt(&Foutmelding::from(fout).melding));
            }
            Gespreksuitslag::Geannuleerd => {}
        }
        true
    }
}

/// Een gebeurtenis van de wachtrij voor het opnieuw uitschrijven van het open verslag.
fn bij_opnieuw_gebeurtenis(
    v: &mut Verslag,
    gebeurtenis: &Gebeurtenis,
    fout: &mut Option<Foutmelding>,
) -> bool {
    let Some(o) = &mut v.opnieuw else {
        return false;
    };
    if o.taak != gebeurtenis.taak {
        return false;
    }
    match &gebeurtenis.toestand {
        Toestand::Verwerkt { voortgang, .. } => {
            let veranderd = o.voortgang != *voortgang;
            o.voortgang(*voortgang);
            veranderd
        }
        Toestand::Fout { melding, .. } => {
            v.opnieuw = None;
            *fout = Some(opnieuw_mislukt(melding));
            true
        }
        Toestand::Geannuleerd => {
            v.opnieuw = None;
            true
        }
        _ => false,
    }
}

/// De gemeten snelheid van een geslaagd opnieuw uitschrijven (ADR 0023, punt 6): model-id en
/// realtime-factor over het deel na het eerste percentage, dus zonder het laden van het model.
/// Opnieuw uitschrijven draait zonder sprekerstap, dus de meting is van hetzelfde soort werk als
/// de schatting in de dialoog. Gewoon uitschrijven meet Mondig niet: daar zit de sprekerstap in.
fn meting(opnieuw: &Opnieuw, gesprek: &Gesprek) -> Option<(String, f64)> {
    let (sinds, p0) = opnieuw.meetpunt?;
    if p0 >= 100 {
        return None;
    }
    let geluid = gesprek
        .verwerkt_geluid()
        .mul_f64(f64::from(100 - p0) / 100.0);
    let factor = crate::snelheid::factor(sinds.elapsed(), geluid)?;
    Some((opnieuw.model_id.clone(), factor))
}

/// Staan de sporen van een opname er nog, zodat ze opnieuw kunnen worden uitgeschreven?
fn sporen_aanwezig(map: &Path) -> bool {
    map.join(EINDE).is_file() && map.join(MICROFOON).is_dir()
}

/// De melding bij een wijziging tijdens het opnieuw uitschrijven (ADR 0023, punt 7).
fn opnieuw_bezig() -> Foutmelding {
    Foutmelding::nieuw(
        "Mondig schrijft dit verslag opnieuw uit. Verbeteren kan weer als het klaar is.",
        Vervolg::Geen,
    )
}

/// De melding bij sluiten, wissen of een ander verslag openen tijdens het opnieuw uitschrijven.
/// Mondig weigert in plaats van stil te stoppen: zo gaat er niets onverwacht verloren.
fn opnieuw_bezig_sluiten() -> Foutmelding {
    Foutmelding::nieuw(
        "Mondig schrijft dit verslag opnieuw uit. Stop dat eerst, of wacht tot het klaar is.",
        Vervolg::Geen,
    )
}

/// De melding bij een wijziging terwijl Mondig notulen maakt: het verslag moet gelijk blijven,
/// anders passen de bronverwijzingen niet meer.
fn notulen_bezig() -> Foutmelding {
    Foutmelding::nieuw(
        "Mondig maakt notulen van dit verslag. Verbeteren kan weer als het klaar is, of als je stopt.",
        Vervolg::Geen,
    )
}

/// De melding bij sluiten, wissen of een ander verslag openen terwijl Mondig notulen maakt.
fn notulen_bezig_sluiten() -> Foutmelding {
    Foutmelding::nieuw(
        "Mondig maakt notulen van dit verslag. Stop dat eerst, of wacht tot het klaar is.",
        Vervolg::Geen,
    )
}

/// De juiste weigering om te sluiten voor wat het verslag nu doet.
fn bezig_sluiten(v: &Verslag) -> Foutmelding {
    if v.opnieuw.is_some() {
        opnieuw_bezig_sluiten()
    } else {
        notulen_bezig_sluiten()
    }
}

/// De melding als het opnieuw uitschrijven mislukte; het oude verslag staat er nog.
fn opnieuw_mislukt(reden: &str) -> Foutmelding {
    Foutmelding::nieuw(
        format!("Opnieuw uitschrijven is niet gelukt; het verslag is niet veranderd. {reden}"),
        Vervolg::Geen,
    )
}

impl Verslag {
    /// Een verslag met niets dan het gesprek: de rest vult de aanroeper met `..`.
    fn leeg(taak: TaakId, map: PathBuf, gesprek: Gesprek) -> Self {
        Self {
            taak,
            gesprek,
            model: String::new(),
            model_id: String::new(),
            begin_ms: 0,
            naam: None,
            persoon: Markeringen::default(),
            notulen: None,
            notulen_maken: None,
            bewaren_mislukt: false,
            bewaard_bytes: Vec::new(),
            laatste_volgnr: 0,
            wist_ms: None,
            audio_bezig: false,
            luistert: 0,
            opnieuw: None,
            vorige: None,
            na_opnieuw: None,
            veranderd: Vec::new(),
            opname_aanwezig: sporen_aanwezig(&map),
            map,
        }
    }

    /// Zet het opnieuw uitgeschreven gesprek in de plaats van het huidige (ADR 0023): het oude
    /// gesprek wordt de vorige versie, de veranderde regels krijgen hun vlag en de melding telt
    /// ze, met de eigen markeringen die niet meer in de tekst staan (`vergelijking`, berekend
    /// buiten het slot).
    fn vervang_door(&mut self, nieuw: Gesprek, opnieuw: Opnieuw, vergelijking: Vergelijking) {
        let oud = std::mem::replace(&mut self.gesprek, nieuw);
        self.veranderd = vergelijking.veranderd;
        self.vorige = Some(Box::new(Vorige {
            gesprek: oud,
            model: std::mem::replace(&mut self.model, opnieuw.model),
            model_id: std::mem::replace(&mut self.model_id, opnieuw.model_id),
        }));
        self.hercontroleer_notulen();
        self.na_opnieuw = Some(NaOpnieuw {
            rol: opnieuw.rol,
            veranderd: self.veranderd.iter().filter(|v| **v).count(),
            verdwenen: vergelijking.verdwenen,
        });
    }

    /// Na een wijziging aan het gesprek die `aantal` beurten raakte (ADR 0023, punt 4): de vorige
    /// versie en haar melding vervallen, zodat terugzetten nooit stil een latere wijziging
    /// weggooit. Schuiven de beurten (samenvoegen, een andere spreker), dan kloppen de plekken
    /// van de veranderde regels niet meer en gaan die vlaggen weg; bij een tekst of een naam
    /// blijven ze.
    fn na_gesprekswijziging(&mut self, aantal: usize, verschuift: bool) {
        if aantal == 0 {
            return;
        }
        self.vorige = None;
        self.na_opnieuw = None;
        if verschuift {
            self.veranderd.clear();
        }
    }

    /// Doet Mondig nu iets met dit verslag dat niet mag worden onderbroken door sluiten of wissen
    /// (opnieuw uitschrijven of notulen maken)?
    fn bezig_met_werk(&self) -> bool {
        self.opnieuw.is_some() || self.notulen_maken.is_some()
    }

    /// De bytes van `verslag.json` voor dit verslag.
    fn bytes(&self) -> Option<Vec<u8>> {
        verslagbestand::naar_bytes(&Bewaren::nieuw(
            &self.gesprek,
            &self.model_id,
            &self.model,
            self.naam.as_deref(),
            &self.persoon,
            self.notulen.as_ref(),
        ))
        .ok()
    }
}

/// Een opdracht om `verslag.json` te schrijven, gemaakt onder het slot van de stand en uitgevoerd
/// buiten dat slot.
struct Schrijfopdracht {
    map: PathBuf,
    bytes: Vec<u8>,
    /// Hoger is nieuwer; een oudere opdracht overschrijft nooit een nieuwere.
    volgnr: u64,
    taak: TaakId,
}

/// Schrijft de bytes van een opdracht en logt een mislukking (alleen het soort probleem, nooit een
/// pad of tekst).
fn schrijf_bytes_gelogd(opdracht: &Schrijfopdracht) -> std::io::Result<()> {
    verslagbestand::schrijf_bytes(&opdracht.map, &opdracht.bytes).inspect_err(|fout| {
        crate::logboek::mislukt(
            mondig_core::taak::Soort::Vergadering,
            Probleem::uit_io(fout.kind()),
        );
    })
}

/// Voert een opdracht uit, behalve als er voor dezelfde map al een nieuwere is geschreven: `Ok(false)`.
/// De schrijfvolgorde is zo vast, ook als twee commando's tegelijk buiten het slot schrijven.
fn schrijf_opdracht(
    laatste: &Mutex<HashMap<PathBuf, u64>>,
    opdracht: &Schrijfopdracht,
) -> std::io::Result<bool> {
    let mut laatste = slot(laatste);
    if laatste
        .get(&opdracht.map)
        .is_some_and(|&gedaan| gedaan >= opdracht.volgnr)
    {
        return Ok(false);
    }
    schrijf_bytes_gelogd(opdracht)?;
    laatste.insert(opdracht.map.clone(), opdracht.volgnr);
    Ok(true)
}

/// Voert een opdracht uit en verwerkt de uitslag in de stand. Op een blokkerende thread, buiten
/// het slot van de stand tijdens het schrijven.
fn voer_uit(app: &AppHandle, opdracht: &Schrijfopdracht) {
    let staat = app.state::<VergaderingStaat>();
    let uitslag = schrijf_opdracht(&staat.schrijf, opdracht);
    slot(&staat.binnen).na_schrijven(opdracht, uitslag);
}

/// De melding als het verslag niet naar schijf kon. Werkt ook op het scherm Opnemen, als het
/// verslag intussen is gesloten.
fn bewaren_mislukt(soort: Option<std::io::ErrorKind>) -> Foutmelding {
    let melding = if soort == Some(std::io::ErrorKind::StorageFull) {
        "De schijf is vol, dus Mondig kon het verslag niet bewaren. Maak ruimte vrij, of kopieer of sla het op als je het wilt houden."
    } else {
        "Mondig kon het verslag niet naast de opname bewaren. Kopieer of sla het op als je het wilt houden."
    };
    Foutmelding::nieuw(melding, Vervolg::Geen)
}

/// Het taak-id van het verslag dat open is.
fn verslag_taak_in(binnen: &Binnen) -> Result<TaakId, Foutmelding> {
    match &binnen.fase {
        Fase::Verslag(v) => Ok(v.taak),
        _ => Err(geen_verslag()),
    }
}

/// De melding als het verslag sluit terwijl er een stuk geluid wordt klaargezet.
fn speelt_nog() -> Foutmelding {
    Foutmelding::nieuw(
        "Mondig zet nog geluid klaar. Probeer het over een paar seconden opnieuw.",
        Vervolg::OpnieuwProberen,
    )
}

fn geen_verslag() -> Foutmelding {
    Foutmelding::nieuw("Er is geen verslag open.", Vervolg::Geen)
}

fn beurten_van(gesprek: &Gesprek, persoon: &Markeringen, veranderd: &[bool]) -> Vec<BeurtInfo> {
    let sprekers = gesprek.met_sprekers();
    gesprek
        .beurten
        .iter()
        .enumerate()
        .map(|(index, b)| BeurtInfo {
            index,
            kant: b.kant,
            sprekernummer: b.sprekernummer,
            spreker: sprekers.then(|| b.spreker()),
            eigen_naam: b.naam.is_some(),
            start_ms: b.start_ms,
            eind_ms: b.eind_ms,
            tekst: b.tekst.clone(),
            markeringen: markeringen_van(persoon, &b.tekst),
            veranderd: veranderd.get(index).copied().unwrap_or(false),
        })
        .collect()
}

/// Wat de app over vergaderingen onthoudt.
#[derive(Default)]
pub(crate) struct VergaderingStaat {
    binnen: Mutex<Binnen>,
    /// De lopende opname. Apart van [`Binnen`], zodat stoppen (dat even kan duren) de stand
    /// niet blokkeert.
    opname: Mutex<Option<Vergaderopname>>,
    /// Loopt er een opname, of worden de bronnen geopend? Voor de dicteerketen en de
    /// microfoontest: die openen de microfoon dan niet.
    bezig: AtomicBool,
    /// Telt bij elke start en stop op, zodat een oude tikker vanzelf stopt.
    generatie: AtomicU64,
    /// Het systeemvak vroeg om een nieuwe opname met de herinnering vooraf; de interface haalt
    /// dit verzoek op ([`vergadering_start_gevraagd`]).
    start_gevraagd: StartVerzoek,
    /// Per map het volgnummer van de laatst geschreven `verslag.json`: het schrijven zelf gebeurt
    /// onder dit slot en buiten het slot van de stand, in volgorde ([`schrijf_opdracht`]).
    schrijf: Mutex<HashMap<PathBuf, u64>>,
}

impl VergaderingStaat {
    /// Bij het afsluiten van de app: een lopende opname netjes afsluiten, zodat ze daarna als
    /// "nog uit te schrijven" klaarstaat.
    pub(crate) fn stop(&self) {
        self.generatie.fetch_add(1, Ordering::SeqCst);
        let opname = slot(&self.opname).take();
        if let Some(opname) = opname {
            let _ = opname.stop();
        }
        self.bezig.store(false, Ordering::SeqCst);
    }

    /// Het verslag als [`Document`] om te bewaren (voor `bewaren.rs`). `None` als `taak` niet
    /// het verslag van nu is.
    pub(crate) fn document(&self, taak: TaakId) -> Option<Document> {
        let binnen = slot(&self.binnen);
        let Fase::Verslag(v) = &binnen.fase else {
            return None;
        };
        (v.taak == taak).then(|| {
            let mut document =
                Document::van_gesprek(&v.gesprek, &v.model, bewaren::lokaal_moment(v.begin_ms));
            // De naam die de gebruiker gaf, is de titel en geeft de bestandsnaam.
            if let Some(naam) = &v.naam {
                document.titel.clone_from(naam);
            }
            // Met "Persoonsgegevens vervangen" aan krijgen alle exports [BSN] enzovoort, ook in de
            // namen van sprekers. De titel die Mondig zelf maakt, blijft staan.
            for regel in &mut document.regels {
                regel.tekst = v.persoon.voor_uitvoer(&regel.tekst);
            }
            // Een eigen naam van de gebruiker kan een gemarkeerd gegeven bevatten: ook de titel (en
            // dus de bestandsnaam en de metadata) krijgt dan de plaatsvervanger.
            crate::persoon::vervang_velden(&mut document, &v.persoon, v.naam.is_some());
            document
        })
    }
}

/// Loopt er een vergaderopname (of gaat er een beginnen)?
pub(crate) fn loopt(app: &AppHandle) -> bool {
    app.try_state::<VergaderingStaat>()
        .is_some_and(|s| s.bezig.load(Ordering::SeqCst))
}

/// Stuurt de stand naar de interface en past het menu van het systeemvak erop aan.
fn stuur_stand(app: &AppHandle, stand: &VergaderingStand) {
    let _ = app.emit("vergadering-gewijzigd", stand.clone());
    let (opname, duur) = stand.toestand.systeemvak();
    crate::systeemvak::zet_opname(app, opname, &klok_tekst(duur), stand.versie);
}

/// Werkt de stand bij, verhoogt het volgnummer en stuurt de nieuwe stand naar de interface.
fn wijzig(app: &AppHandle, wijziging: impl FnOnce(&mut Binnen)) -> VergaderingStand {
    let staat = app.state::<VergaderingStaat>();
    let stand = {
        let mut binnen = slot(&staat.binnen);
        wijziging(&mut binnen);
        binnen.versie += 1;
        binnen.stand()
    };
    stuur_stand(app, &stand);
    stand
}

// ---------------------------------------------------------------------------
// Opnames in de tijdelijke map
// ---------------------------------------------------------------------------

/// Een geschatte duur van een afgebroken opname: de samples in de stukken van de microfoon
/// (16 kHz, 16 bit, na de kop van 44 bytes).
fn geschatte_duur(map: &Path) -> Option<u64> {
    let lijst = fs::read_dir(map.join(MICROFOON)).ok()?;
    let bytes: u64 = lijst
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "wav"))
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len().saturating_sub(44))
        .sum();
    Some(bytes / 32)
}

/// De opnames in `basis` die nog uitgeschreven of hersteld kunnen worden (oudste eerst) en de
/// bewaarde verslagen (nieuwste eerst). Een map met een leesbaar `verslag.json` is een verslag;
/// een map zonder, of met een onleesbaar of te nieuw `verslag.json`, is een opname die nog wacht.
/// De opname waar de app nu mee bezig is (`actief`), en opnames die nog lopen, staan er niet in.
fn zoek_lijsten(
    basis: &Path,
    actief: Option<&Path>,
    bewaartijd: Option<Duration>,
    nu: u64,
) -> Lijsten {
    let actief = actief.and_then(Path::file_name);
    let wist_ms = |map: &Path| bewaartijd.and_then(|tijd| vergadering::wistijd_ms(map, tijd, nu));
    let id_van = |map: &Path| map.file_name().map(|n| n.to_string_lossy().into_owned());
    let info = |map: &Path, afgesloten: bool, onzeker: bool| -> Option<OpnameInfo> {
        let opzet = vergadering::opzet(map).ok()?;
        let duur_ms = if afgesloten {
            mondig_core::spoor::lees_afsluiting(map)
                .ok()
                .map(|a| a.duur_ms)
        } else {
            geschatte_duur(map)
        };
        Some(OpnameInfo {
            id: id_van(map)?,
            soort: opzet.soort,
            begin_ms: opzet.begin_unix_ms,
            duur_ms,
            afgesloten,
            onzeker,
            wist_ms: wist_ms(map),
        })
    };
    let verslag_info = |map: &Path| -> Option<VerslagInfo> {
        let opzet = vergadering::opzet(map).ok()?;
        // Alleen de samenvatting vooraan in het bestand: de tekst blijft ongelezen.
        let samenvatting = verslagbestand::lees_samenvatting(map)?;
        Some(VerslagInfo {
            id: id_van(map)?,
            naam: samenvatting.naam,
            soort: samenvatting.soort,
            begin_ms: opzet.begin_unix_ms,
            duur_ms: samenvatting.duur_ms,
            sprekers: samenvatting.sprekers,
            wist_ms: wist_ms(map),
            opname_aanwezig: sporen_aanwezig(map),
        })
    };
    let mut lijsten = Lijsten::default();
    let mappen: Vec<PathBuf> = fs::read_dir(basis)
        .map(|mappen| {
            mappen
                .filter_map(Result::ok)
                .map(|e| e.path())
                // Het verslag dat open is, en de opname waar de app mee bezig is, slaan we over.
                .filter(|map| actief.is_none_or(|a| map.file_name() != Some(a)))
                .filter(|map| map.join(OPZET).is_file())
                .collect()
        })
        .unwrap_or_default();
    for map in &mappen {
        if let Some(verslag) = verslag_info(map) {
            lijsten.verslagen.push(verslag);
        } else if map.join(EINDE).is_file()
            && let Some(opname) = info(map, true, false)
        {
            lijsten.opnames.push(opname);
        }
    }
    let verslag_ids: Vec<&str> = lijsten.verslagen.iter().map(|v| v.id.as_str()).collect();
    let onafgemaakt: Vec<OpnameInfo> = vergadering::onafgemaakt_met_stand(basis)
        .iter()
        .filter(|o| {
            o.map
                .file_name()
                .is_none_or(|n| !verslag_ids.contains(&&*n.to_string_lossy()))
        })
        .filter_map(|o| info(&o.map, false, o.stand == Slotstand::Onbekend))
        .collect();
    lijsten.opnames.extend(onafgemaakt);
    lijsten
        .opnames
        .retain(|o| actief.is_none_or(|a| a.to_string_lossy() != o.id));
    lijsten
        .opnames
        .sort_by(|a, b| (a.begin_ms, &a.id).cmp(&(b.begin_ms, &b.id)));
    lijsten
        .verslagen
        .sort_by(|a, b| (b.begin_ms, &b.id).cmp(&(a.begin_ms, &a.id)));
    lijsten
}

/// Leest de lijst opnieuw van schijf.
fn werk_lijst_bij(binnen: &mut Binnen) {
    if let Ok(basis) = vergadering::vergadermap() {
        binnen.lees_lijsten(&basis, nu_ms());
    }
}

/// Zo vaak ruimt Mondig verlopen opnames op, na het opruimen bij het starten.
const OPRUIMEN_ELK: Duration = Duration::from_hours(1);

/// Ruimt de verlopen opnames in `basis` op (ADR 0008, aanvulling 4b-C): een verlopen opname
/// krijgt eerst een markering en wordt een dag later gewist. De lijst wordt opnieuw gelezen
/// als er iets is gewist of gemarkeerd. Is de bewaartermijn onbekend, dan gebeurt er niets. De opname waar de app nu mee bezig is, blijft altijd staan. Dit
/// draait onder het slot van de stand: zo kan een opname niet tussen de controle en het wissen
/// in gebruik raken (uitschrijven, herstellen). Geeft terug of er iets is gewist.
fn ruim_op_in(binnen: &mut Binnen, basis: &Path, nu: u64) -> bool {
    // De opname waar de app mee bezig is, en mappen die nu worden gewist.
    let in_gebruik: Vec<PathBuf> = binnen
        .actieve_map()
        .map(Path::to_path_buf)
        .into_iter()
        .chain(binnen.wordt_gewist.iter().map(PathBuf::from))
        .collect();
    let Some(bewaartijd) = binnen.bewaartijd() else {
        return false;
    };
    let uitslag = vergadering::wis_verlopen(basis, bewaartijd, nu, &in_gebruik);
    if !uitslag.veranderd() {
        return false;
    }
    binnen.lees_lijsten(basis, nu);
    true
}

/// Ruimt op en stuurt de nieuwe stand naar de interface als de lijst veranderde.
fn ruim_op(app: &AppHandle) {
    let Ok(basis) = vergadering::vergadermap() else {
        return;
    };
    // Elk uur opnieuw gelezen: een instelling die eerst onleesbaar was, telt weer mee.
    let dagen = crate::instellingen::lees_bewaar_dagen(app);
    let staat = app.state::<VergaderingStaat>();
    let stand = {
        let mut binnen = slot(&staat.binnen);
        let nu = nu_ms();
        let gewijzigd = binnen.bewaar_dagen != dagen;
        binnen.bewaar_dagen = dagen;
        let veranderd = ruim_op_in(&mut binnen, &basis, nu);
        if !veranderd && !gewijzigd {
            return;
        }
        if !veranderd {
            binnen.lees_lijsten(&basis, nu);
        }
        binnen.versie += 1;
        binnen.stand()
    };
    stuur_stand(app, &stand);
}

/// De bewaartermijn is veranderd in Instellingen: de lijst rekent meteen met de nieuwe tijd,
/// zodat de melding "Wordt morgen automatisch gewist." klopt. Het wissen zelf volgt bij de
/// volgende ronde, hooguit een uur later: wie per ongeluk 1 dag kiest, krijgt zo eerst de
/// melding te zien.
pub(crate) fn bewaartermijn_gewijzigd(app: &AppHandle, dagen: u32) {
    wijzig(app, |b| {
        b.bewaar_dagen = Some(dagen);
        werk_lijst_bij(b);
    });
}

/// Controleert een id uit de interface: het moet een gewone mapnaam zijn en niet de map waar de
/// app nu mee bezig is. De interface kan zo nooit een ander pad aanwijzen.
fn controleer_id(binnen: &Binnen, id: &str, weg: &Foutmelding) -> Result<(), Foutmelding> {
    let gewoon = !id.is_empty()
        && id != "."
        && id != ".."
        && !id.contains(['/', '\\', ':'])
        && Path::new(id).file_name().is_some_and(|n| n == id);
    if !gewoon {
        return Err(weg.clone());
    }
    if binnen
        .actieve_map()
        .and_then(Path::file_name)
        .is_some_and(|a| a == id)
    {
        return Err(Foutmelding::nieuw(
            "Mondig is nu met deze opname bezig.",
            Vervolg::Geen,
        ));
    }
    Ok(())
}

/// De map van een opname uit de lijst. Een id dat geen gewone mapnaam is of niet in de lijst
/// staat, wordt geweigerd: de interface kan zo nooit een ander pad aanwijzen.
fn map_van(binnen: &Binnen, id: &str) -> Result<(PathBuf, OpnameInfo), Foutmelding> {
    let weg = Foutmelding::nieuw("Deze opname staat er niet meer.", Vervolg::Geen);
    controleer_id(binnen, id, &weg)?;
    let info = binnen
        .opnames
        .iter()
        .find(|o| o.id == id)
        .cloned()
        .ok_or(weg)?;
    let basis = vergadering::vergadermap()?;
    Ok((basis.join(id), info))
}

/// De map van een bewaard verslag uit de lijst "Eerder opgenomen", onder `basis`; met dezelfde
/// controles als [`map_van`].
fn verslag_van(
    binnen: &Binnen,
    basis: &Path,
    id: &str,
) -> Result<(PathBuf, VerslagInfo), Foutmelding> {
    let weg = Foutmelding::nieuw("Dit verslag staat er niet meer.", Vervolg::Geen);
    controleer_id(binnen, id, &weg)?;
    let info = binnen
        .verslagen
        .iter()
        .find(|v| v.id == id)
        .cloned()
        .ok_or(weg)?;
    Ok((basis.join(id), info))
}

// ---------------------------------------------------------------------------
// Opname-indicator: taakbalk en systeemvak
// ---------------------------------------------------------------------------

/// Een vlak van `maat` pixels in RGBA met rechtsonder een klein rond stipje (ongeveer 60
/// procent van de breedte) en een zachte rand; de rest is doorzichtig. Windows tekent het
/// overlay-icoon op 16 pixels over de hoek van het taakbalkicoon, dus een vol vlak oogt fors.
#[cfg_attr(
    not(windows),
    allow(
        dead_code,
        reason = "het stipje op de taakbalk bestaat alleen op Windows"
    )
)]
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "kleine afmetingen en dekking tussen 0 en 255"
)]
fn rode_stip(maat: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (maat * maat * 4) as usize];
    let straal = maat as f32 * 0.3;
    let midden = maat as f32 - straal - 0.5;
    for y in 0..maat {
        for x in 0..maat {
            let dx = x as f32 + 0.5 - midden;
            let dy = y as f32 + 0.5 - midden;
            let dekking = (straal - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
            let i = ((y * maat + x) * 4) as usize;
            rgba[i..i + 3].copy_from_slice(&ROOD);
            rgba[i + 3] = (dekking * 255.0).round() as u8;
        }
    }
    rgba
}

/// Het icoon met rechtsonder een rood stipje van ongeveer een derde van de breedte, met een
/// donkere rand zodat het op elke achtergrond opvalt (zoals `.rec.overlay` in het ontwerp).
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "kleine afmetingen en dekking tussen 0 en 255"
)]
fn met_stip(rgba: &[u8], breedte: u32, hoogte: u32) -> Vec<u8> {
    let mut uit = rgba.to_vec();
    let maat = breedte.min(hoogte) as f32;
    let straal = maat * 0.17;
    let rand = (maat * 0.05).max(1.0);
    let midden_x = breedte as f32 - straal - rand;
    let midden_y = hoogte as f32 - straal - rand;
    for y in 0..hoogte {
        for x in 0..breedte {
            let dx = x as f32 + 0.5 - midden_x;
            let dy = y as f32 + 0.5 - midden_y;
            let afstand = (dx * dx + dy * dy).sqrt();
            let buiten = (straal + rand - afstand + 0.5).clamp(0.0, 1.0);
            if buiten <= 0.0 {
                continue;
            }
            let binnen = (straal - afstand + 0.5).clamp(0.0, 1.0);
            let i = ((y * breedte + x) * 4) as usize;
            let Some(px) = uit.get_mut(i..i + 4) else {
                continue;
            };
            // Eerst de donkere rand, dan het rood erover.
            for k in 0..3 {
                let rand_kleur = f32::from(px[k]) * (1.0 - buiten) + 32.0 * buiten;
                let kleur = rand_kleur * (1.0 - binnen) + f32::from(ROOD[k]) * binnen;
                px[k] = kleur.round().clamp(0.0, 255.0) as u8;
            }
            let dekking = f32::from(px[3]).max(buiten * 255.0);
            px[3] = dekking.round().clamp(0.0, 255.0) as u8;
        }
    }
    uit
}

/// De opnametijd als "12:35", boven het uur "1:02:03". Voor de tooltip en het menu van het
/// systeemvak.
pub(crate) fn klok_tekst(duur: Duration) -> String {
    let s = duur.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

/// "Mondig neemt een vergadering op (12:34)", boven het uur "(1:02:03)". Bij een fysieke
/// vergadering "een fysieke vergadering", bij Alleen ik "een notitie".
fn tooltip(soort: Vergadersoort, duur: Duration) -> String {
    let tijd = klok_tekst(duur);
    let wat = match soort {
        Vergadersoort::Online => "een vergadering",
        Vergadersoort::Ruimte => "een fysieke vergadering",
        Vergadersoort::Alleen => "een notitie",
    };
    format!("Mondig neemt {wat} op ({tijd})")
}

/// Zet de opname-indicator aan of uit: het stipje op het taakbalkicoon (alleen Windows) en het
/// icoon en de tooltip in het systeemvak.
fn indicator(app: &AppHandle, opname: Option<(Vergadersoort, Duration)>) {
    #[cfg(windows)]
    if let Some(venster) = app.get_webview_window("main") {
        let stip = opname.map(|_| Image::new_owned(rode_stip(16), 16, 16));
        let _ = venster.set_overlay_icon(stip);
    }
    let Some(tray) = app.tray_by_id("hoofd") else {
        return;
    };
    let standaard = app.default_window_icon().cloned();
    match (opname, standaard) {
        (Some((soort, duur)), Some(icoon)) => {
            let rgba = met_stip(icoon.rgba(), icoon.width(), icoon.height());
            let _ = tray.set_icon(Some(Image::new_owned(rgba, icoon.width(), icoon.height())));
            let _ = tray.set_tooltip(Some(tooltip(soort, duur)));
        }
        (Some((soort, duur)), None) => {
            let _ = tray.set_tooltip(Some(tooltip(soort, duur)));
        }
        (None, icoon) => {
            let _ = tray.set_icon(icoon);
            let _ = tray.set_tooltip(Some("Mondig"));
        }
    }
}

/// Alleen de tooltip bijwerken tijdens de opname.
fn werk_tooltip_bij(app: &AppHandle, soort: Vergadersoort, duur: Duration) {
    if let Some(tray) = app.tray_by_id("hoofd") {
        let _ = tray.set_tooltip(Some(tooltip(soort, duur)));
    }
    crate::systeemvak::zet_tijd(app, &klok_tekst(duur));
}

// ---------------------------------------------------------------------------
// Opnemen
// ---------------------------------------------------------------------------

/// Wat een bron meldt terwijl de opname loopt.
enum Uitval {
    Microfoon(OpnameFout),
    Systeem(OpnameFout),
}

/// De bewaarde soort.
fn lees_soort(app: &AppHandle) -> Vergadersoort {
    soort_uit_instelling(lees_instelling(app, SLEUTEL_SOORT).as_deref())
}

fn schrijf_soort(app: &AppHandle, soort: Vergadersoort) {
    schrijf_instelling(app, SLEUTEL_SOORT, soort_instelling(soort));
}

/// De soort uit de instelling; zonder of met een onbekende waarde online, zoals voorheen.
fn soort_uit_instelling(waarde: Option<&str>) -> Vergadersoort {
    match waarde {
        Some("ruimte") => Vergadersoort::Ruimte,
        Some("alleen") => Vergadersoort::Alleen,
        _ => Vergadersoort::Online,
    }
}

/// De waarde van een soort in de instelling, dezelfde als in `opname.json`.
fn soort_instelling(soort: Vergadersoort) -> &'static str {
    match soort {
        Vergadersoort::Online => "online",
        Vergadersoort::Ruimte => "ruimte",
        Vergadersoort::Alleen => "alleen",
    }
}

/// Opent het geluid van de computer. Zonder eigen keuze en zonder afspeelapparaat (of op een
/// systeem dat het nog niet kan) gaat de opname door met alleen de microfoon en een melding,
/// zoals de CLI; een gekozen apparaat dat niet opent, is een fout.
fn open_systeem(
    afspeelapparaat: Option<&str>,
    meld: mpsc::Sender<Uitval>,
    meldingen: &mut Vec<String>,
) -> Result<Option<Box<dyn Bron>>, Foutmelding> {
    let bij_uitval = Box::new(move |f| {
        let _ = meld.send(Uitval::Systeem(f));
    });
    match SysteemApparatuur.open(afspeelapparaat, bij_uitval) {
        Ok(bron) => Ok(Some(bron)),
        Err(fout)
            if afspeelapparaat.is_none()
                && matches!(
                    fout.soort,
                    FoutSoort::GeenAfspeelapparaat
                        | FoutSoort::SysteemgeluidNietMogelijk
                        | FoutSoort::SysteemgeluidOmrekenenStart
                ) =>
        {
            meldingen.push(Foutmelding::from(fout).melding);
            Ok(None)
        }
        Err(fout) => Err(Foutmelding::from(fout)),
    }
}

/// Opent de bronnen en start de opname. Draait buiten de gebeurtenislus.
fn start_blokkerend(
    app: &AppHandle,
    soort: Vergadersoort,
    microfoon: Option<String>,
    afspeelapparaat: Option<String>,
) -> Result<VergaderingStand, Foutmelding> {
    let staat = app.state::<VergaderingStaat>();
    {
        // Controleren en claimen onder hetzelfde slot: een herstel of tweede start ertussen
        // ziet de fase Starten en weigert.
        let mut binnen = slot(&staat.binnen);
        binnen.claim(Fase::Starten { soort })?;
        binnen.laatste_soort = soort;
        // Eerst bezetten, dan naar het dictaat kijken: het dictaat doet het omgekeerde, zodat
        // minstens een van de twee de ander ziet (zoals proef en dictaat).
        staat.bezig.store(true, Ordering::SeqCst);
    }
    if app
        .state::<DicteerStaat>()
        .dictaat_bezig
        .load(Ordering::SeqCst)
    {
        staat.bezig.store(false, Ordering::SeqCst);
        let fout = Foutmelding::nieuw(
            "Er loopt een dictaat. Start de opname als het dictaat klaar is.",
            Vervolg::OpnieuwProberen,
        );
        wijzig(app, |b| {
            if matches!(b.fase, Fase::Starten { .. }) {
                b.fase = Fase::Klaar;
            }
        });
        return Err(fout);
    }
    schrijf_soort(app, soort);
    wijzig(app, |_| {});
    match open_en_start(app, soort, microfoon.as_deref(), afspeelapparaat.as_deref()) {
        Ok(stand) => Ok(stand),
        Err(fout) => {
            staat.bezig.store(false, Ordering::SeqCst);
            wijzig(app, |b| {
                b.fase = Fase::Klaar;
                b.fout = Some(fout.clone());
            });
            Err(fout)
        }
    }
}

fn open_en_start(
    app: &AppHandle,
    soort: Vergadersoort,
    microfoon: Option<&str>,
    afspeelapparaat: Option<&str>,
) -> Result<VergaderingStand, Foutmelding> {
    // Een lopende microfoontest en de meters stoppen: de opname heeft de microfoon nodig.
    crate::meter::stop_alle(app, crate::meter::Reden::Voorrang);
    let basis = vergadering::vergadermap()?;
    // De gekozen (of bewaarde) microfoon als die er nog is, anders de standaard, zoals een dictaat.
    let gekozen = microfoon
        .map(str::to_string)
        .or_else(|| lees_microfoon(app));
    let microfoon_id = HandyApparatuur
        .microfoons()
        .ok()
        .and_then(|lijst| opgeslagen_microfoon(gekozen.as_deref(), &lijst));
    let (meld, uitvallen) = mpsc::channel::<Uitval>();
    let voor_microfoon = meld.clone();
    let microfoon = HandyApparatuur
        .open(
            microfoon_id.as_deref(),
            Box::new(move |f| {
                let _ = voor_microfoon.send(Uitval::Microfoon(f));
            }),
        )
        .map_err(|f| crate::microfoonrecht::verfijn(Foutmelding::from(f), false))?;
    let mut meldingen = Vec::new();
    // Bij een fysieke vergadering en bij Alleen ik alleen de microfoon.
    let systeem = if soort.met_systeem() {
        open_systeem(afspeelapparaat, meld, &mut meldingen)?
    } else {
        None
    };
    let opname = Vergaderopname::start_als(&basis, soort, microfoon, systeem)?;
    let map = opname.map().to_path_buf();
    let begin_ms = vergadering::opzet(&map).map_or_else(|_| nu_ms(), |o| o.begin_unix_ms);
    let staat = app.state::<VergaderingStaat>();
    let generatie = staat.generatie.fetch_add(1, Ordering::SeqCst) + 1;
    *slot(&staat.opname) = Some(opname);
    indicator(app, Some((soort, Duration::ZERO)));
    let stand = wijzig(app, |b| {
        b.fase = Fase::NeemtOp(Opnemen {
            map,
            soort,
            begin_ms,
            begin: Instant::now(),
            niveaus: None,
            systeem_stil: false,
            meldingen,
        });
        werk_lijst_bij(b);
    });
    let app = app.clone();
    thread::Builder::new()
        .name("mondig-vergadering-tikker".into())
        .spawn(move || tik(&app, soort, generatie, &uitvallen))
        .map_err(|_| Foutmelding::nieuw("De opname kon niet starten.", Vervolg::Geen))?;
    Ok(stand)
}

/// Tien keer per seconde zolang de opname loopt: niveaus naar de interface, uitval en stilte
/// in de stand, de tooltip bijwerken. Stopt als de generatie verandert (stop, weggooien).
fn tik(app: &AppHandle, soort: Vergadersoort, generatie: u64, uitvallen: &mpsc::Receiver<Uitval>) {
    let staat = app.state::<VergaderingStaat>();
    let mut tooltip_op = Instant::now();
    loop {
        thread::sleep(TIK);
        if staat.generatie.load(Ordering::SeqCst) != generatie {
            return;
        }
        let meting = slot(&staat.opname).as_ref().map(|o| {
            (
                o.niveaus(),
                o.systeem_stil_sinds().is_some_and(|d| d >= SYSTEEM_STIL_NA),
                o.schrijffout(),
                o.duur(),
            )
        });
        let Some((niveaus, stil, schrijffout, duur)) = meting else {
            return;
        };
        let _ = app.emit("vergadering-niveau", niveaus);
        let nieuw: Vec<String> = uitvallen
            .try_iter()
            .map(|u| match u {
                Uitval::Microfoon(f) => {
                    crate::microfoonrecht::verfijn(Foutmelding::from(f), false).melding
                }
                Uitval::Systeem(f) => Foutmelding::from(f).melding,
            })
            .collect();
        let veranderd = {
            let mut binnen = slot(&staat.binnen);
            match &mut binnen.fase {
                Fase::NeemtOp(o) => {
                    o.niveaus = Some(niveaus);
                    let veranderd = o.systeem_stil != stil || !nieuw.is_empty();
                    o.systeem_stil = stil;
                    o.meldingen.extend(nieuw);
                    veranderd
                }
                _ => return,
            }
        };
        if veranderd {
            wijzig(app, |_| {});
        }
        if let Some(fout) = schrijffout {
            crate::logboek::mislukt(mondig_core::taak::Soort::Vergadering, None);
            // De schijf is vol: stoppen en bewaren wat er staat; de opname blijft staan.
            stop_opname(app, false);
            wijzig(app, |b| b.fout = Some(fout));
            return;
        }
        if tooltip_op.elapsed() >= TOOLTIP_ELKE {
            tooltip_op = Instant::now();
            werk_tooltip_bij(app, soort, duur);
        }
    }
}

/// Stopt de lopende opname en zet de indicator uit. Met `uitschrijven` gaat de opname in de
/// wachtrij; anders blijft ze staan voor later. Geeft de nieuwe stand.
fn stop_opname(app: &AppHandle, uitschrijven: bool) -> VergaderingStand {
    let staat = app.state::<VergaderingStaat>();
    // Van NeemtOp naar Voorbereiden onder het slot: twee keer stoppen (knop en volle schijf
    // tegelijk) stopt maar een keer, en niets anders kan er tussendoor beginnen.
    let claim = {
        let mut binnen = slot(&staat.binnen);
        if let Fase::NeemtOp(o) = &binnen.fase {
            let v = Voorbereiden {
                map: o.map.clone(),
                soort: o.soort,
                begin_ms: o.begin_ms,
                duur_ms: Some(ms(o.begin.elapsed())),
                herstellen: false,
            };
            let map = v.map.clone();
            binnen.fase = Fase::Voorbereiden(v);
            Some(map)
        } else {
            None
        }
    };
    let Some(map) = claim else {
        return wijzig(app, |_| {});
    };
    staat.generatie.fetch_add(1, Ordering::SeqCst);
    let opname = slot(&staat.opname).take();
    let gestopt = opname.map(Vergaderopname::stop);
    staat.bezig.store(false, Ordering::SeqCst);
    indicator(app, None);
    match gestopt {
        Some(Ok(uitslag)) if uitschrijven => {
            wijzig(app, |b| {
                if let Fase::Voorbereiden(v) = &mut b.fase {
                    v.duur_ms = Some(ms(uitslag.duur));
                }
            });
            zet_in_wachtrij(app, &map)
        }
        // Mislukt het stoppen, dan is wat op schijf staat met herstellen terug te halen.
        Some(Err(fout)) => wijzig(app, |b| {
            b.klaar_na_voorbereiden(&map, Some(fout));
            werk_lijst_bij(b);
        }),
        _ => wijzig(app, |b| {
            b.klaar_na_voorbereiden(&map, None);
            werk_lijst_bij(b);
        }),
    }
}

/// Zet een afgesloten opname in de wachtrij van Bestanden. Alleen vanuit de fase
/// Voorbereiden van deze opname: is die intussen veranderd, dan gebeurt er niets. Lukt het
/// niet (geen model), dan blijft de opname staan en staat de fout in de stand.
fn zet_in_wachtrij(app: &AppHandle, map: &Path) -> VergaderingStand {
    let staat = app.state::<VergaderingStaat>();
    // Een vergadering gaat met het model voor bestanden (de keuze op de pagina Modellen).
    // De wachtrij blijft tijdens het toevoegen vergrendeld, zodat een modelwissel niet tussen
    // het toevoegen en het noteren van het model valt.
    let gelukt = crate::bestanden::met_wachtrij(app, |wachtrij, model_id, model_naam| {
        let voor_uitslag = app.clone();
        // Controle, toevoegen en overgang onder hetzelfde slot; zo vindt ook de eerste
        // gebeurtenis van de wachtrij de taak al.
        let mut binnen = slot(&staat.binnen);
        // Opnieuw kijken onder beide sloten: een snelheidstest die na de eerste controle
        // begon, ziet de vergadering dan als uitschrijvend (zie `schrijft_uit`), en anders
        // weigeren wij hier.
        crate::videokaart::weiger_tijdens_test(app)?;
        if let Fase::Voorbereiden(v) = &binnen.fase
            && v.map == map
        {
            let (soort, begin_ms, duur_ms) = (v.soort, v.begin_ms, v.duur_ms);
            let sprekers = crate::instellingen::lees_sprekers_herkennen(app);
            let toegevoegd = wachtrij.voeg_vergadering_toe_met(
                map.to_path_buf(),
                sprekers,
                move |taak, uitslag| {
                    bij_uitslag(&voor_uitslag, taak, uitslag);
                },
            );
            if let Ok(taak) = toegevoegd {
                binnen.fase = Fase::Uitschrijven(Uitschrijven {
                    taak,
                    map: map.to_path_buf(),
                    soort,
                    begin_ms,
                    duur_ms,
                    voortgang: None,
                    stap: Stap::Uitschrijven,
                    model: model_naam.to_string(),
                    model_id: model_id.to_string(),
                });
                binnen.fout = None;
            } else {
                binnen.fase = Fase::Klaar;
                binnen.fout = Some(Foutmelding::nieuw(
                    "Het uitschrijven is gestopt. Start Mondig opnieuw.",
                    Vervolg::Geen,
                ));
            }
        }
        werk_lijst_bij(&mut binnen);
        binnen.versie += 1;
        Ok(binnen.stand())
    })
    .and_then(|uitkomst| uitkomst);
    let stand = match gelukt {
        Ok(stand) => stand,
        Err(fout) => {
            return wijzig(app, |b| {
                b.klaar_na_voorbereiden(map, Some(fout));
                werk_lijst_bij(b);
            });
        }
    };
    stuur_stand(app, &stand);
    stand
}

/// Wordt er nu een vergadering uitgeschreven met dit model (id)? Voor `bestanden.rs`: een
/// model dat in gebruik is, mag dan niet weg.
pub(crate) fn schrijft_uit(app: &AppHandle, model_id: &str) -> bool {
    app.try_state::<VergaderingStaat>()
        .is_some_and(|staat| slot(&staat.binnen).schrijft_uit_met(model_id))
}

/// Wordt er nu een vergadering uitgeschreven, met welk model ook? Voor het sprekermodel, dat
/// geen spraakmodel is en dus niet bij `schrijft_uit` hoort: het kan niet weg zolang er een
/// vergadering wordt uitgeschreven, want daarbij kunnen de sprekers herkend worden.
pub(crate) fn schrijft_iets_uit(app: &AppHandle) -> bool {
    app.try_state::<VergaderingStaat>()
        .is_some_and(|staat| matches!(slot(&staat.binnen).fase, Fase::Uitschrijven(_)))
}

/// Een gebeurtenis van de wachtrij met soort `vergadering` (via `bestanden.rs`).
pub(crate) fn bij_gebeurtenis(app: &AppHandle, gebeurtenis: &Gebeurtenis) {
    let Some(staat) = app.try_state::<VergaderingStaat>() else {
        return;
    };
    let veranderd = {
        let mut binnen = slot(&staat.binnen);
        let veranderd = binnen.bij_gebeurtenis(gebeurtenis);
        if veranderd && matches!(binnen.fase, Fase::Klaar) {
            werk_lijst_bij(&mut binnen);
        }
        veranderd
    };
    if veranderd {
        wijzig(app, |_| {});
    }
}

/// De uitslag van het uitschrijven (op de thread van de wachtrij).
fn bij_uitslag(app: &AppHandle, taak: TaakId, uitslag: Gespreksuitslag) {
    let staat = app.state::<VergaderingStaat>();
    let veranderd = {
        let mut binnen = slot(&staat.binnen);
        let veranderd = binnen.bij_uitslag(taak, uitslag);
        if veranderd {
            werk_lijst_bij(&mut binnen);
        }
        veranderd
    };
    if veranderd {
        wijzig(app, |_| {});
    }
}

/// De uitslag van het opnieuw uitschrijven (op de thread van de wachtrij). Wat er veranderde,
/// rekent Mondig hier uit, buiten het slot van de stand, met het oude gesprek, de markeringen en
/// de naam van bij de start (die konden tijdens het lopen niet veranderen).
fn bij_opnieuw_uitslag(
    app: &AppHandle,
    taak: TaakId,
    uitslag: Gespreksuitslag,
    (oud, persoon, naam): &(Gesprek, Markeringen, Option<String>),
) {
    let vergelijking = match &uitslag {
        Gespreksuitslag::Klaar(nieuw) => {
            Some(Vergelijking::bereken(oud, persoon, naam.as_deref(), nieuw))
        }
        _ => None,
    };
    let staat = app.state::<VergaderingStaat>();
    let (veranderd, meting) = {
        let mut binnen = slot(&staat.binnen);
        let veranderd = binnen.bij_opnieuw_uitslag(taak, uitslag, vergelijking);
        (veranderd, binnen.neem_meting())
    };
    if let Some((model_id, factor)) = meting {
        crate::snelheid::bewaar(app, &model_id, factor);
    }
    if veranderd {
        wijzig(app, |_| {});
    }
}

/// Wist een opname. Net na het annuleren van het uitschrijven kan het werkproces een stuk nog
/// even open hebben; dan probeert Mondig het een paar seconden opnieuw.
fn wis_met_geduld(map: &Path, geduld: bool) -> Result<(), Foutmelding> {
    let pogingen = if geduld { 30 } else { 1 };
    let mut uitslag = vergadering::wis(map);
    for _ in 1..pogingen {
        if uitslag.is_ok() || !map.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(100));
        uitslag = vergadering::wis(map);
    }
    uitslag
}

/// Annuleert het lopende uitschrijven in de wachtrij, als dat er is.
fn annuleer_uitschrijven(app: &AppHandle, taak: TaakId) {
    if let Some(wachtrij) = app.state::<crate::bestanden::BestandenStaat>().wachtrij() {
        let _ = wachtrij.annuleer(taak);
    }
}

/// Voert `werk` uit buiten de gebeurtenislus: bronnen openen, stoppen en wissen kan even duren.
pub(crate) async fn blokkerend<T: Send + 'static>(
    werk: impl FnOnce() -> Result<T, Foutmelding> + Send + 'static,
) -> Result<T, Foutmelding> {
    tauri::async_runtime::spawn_blocking(werk)
        .await
        .map_err(|_| Foutmelding::nieuw("Dit stopte onverwacht.", Vervolg::OpnieuwProberen))?
}

// ---------------------------------------------------------------------------
// Commando's
// ---------------------------------------------------------------------------

/// De stand zoals hij nu is; de lijst met opnames wordt opnieuw van schijf gelezen.
#[tauri::command]
pub(crate) fn vergadering_stand(app: AppHandle) -> VergaderingStand {
    let soort = lees_soort(&app);
    wijzig(&app, |b| {
        b.laatste_soort = soort;
        werk_lijst_bij(b);
    })
}

/// Een luidspreker of koptelefoon voor de keuze "Geluid van".
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AfspeelInfo {
    id: String,
    naam: String,
    standaard: bool,
}

/// De afspeelapparaten van nu. Op een systeem dat het geluid van de computer nog niet kan
/// opnemen, een fout in gewone taal.
#[tauri::command]
pub(crate) async fn vergadering_afspeelapparaten() -> Result<Vec<AfspeelInfo>, Foutmelding> {
    blokkerend(|| {
        let lijst = SysteemApparatuur
            .afspeelapparaten()
            .map_err(Foutmelding::from)?;
        Ok(lijst
            .into_iter()
            .map(|a| AfspeelInfo {
                id: a.id,
                naam: a.naam,
                standaard: a.standaard,
            })
            .collect())
    })
    .await
}

/// Start een opname. `microfoon` en `afspeelapparaat` zijn ids uit de lijsten; `None` is de
/// bewaarde microfoon (anders de standaard) en het standaardafspeelapparaat, dat een wissel
/// tijdens de opname volgt. Bij `ruimte` telt alleen de microfoon. Het openen kan seconden
/// duren; de stand staat dan op `starten`.
#[tauri::command]
pub(crate) async fn vergadering_start(
    app: AppHandle,
    soort: Vergadersoort,
    microfoon: Option<String>,
    afspeelapparaat: Option<String>,
) -> Result<VergaderingStand, Foutmelding> {
    blokkerend(move || start_blokkerend(&app, soort, microfoon, afspeelapparaat)).await
}

/// Stopt de opname. Met `uitschrijven` gaat ze meteen in de wachtrij; anders blijft ze staan
/// als opname die nog uitgeschreven moet worden.
#[tauri::command]
pub(crate) async fn vergadering_stop(
    app: AppHandle,
    uitschrijven: bool,
) -> Result<VergaderingStand, Foutmelding> {
    blokkerend(move || stop_als_loopt(&app, uitschrijven)).await
}

/// Stopt de opname als er een loopt; anders een melding.
fn stop_als_loopt(app: &AppHandle, uitschrijven: bool) -> Result<VergaderingStand, Foutmelding> {
    if !matches!(
        slot(&app.state::<VergaderingStaat>().binnen).fase,
        Fase::NeemtOp(_)
    ) {
        return Err(Foutmelding::nieuw("Er loopt geen opname.", Vervolg::Geen));
    }
    Ok(stop_opname(app, uitschrijven))
}

// ---------------------------------------------------------------------------
// Vanuit het systeemvak
// ---------------------------------------------------------------------------

/// Vraagt de interface vooraf "Vertel het de anderen"? Bij Alleen ik is er niemand om het aan te
/// vertellen (zelfde regel als `vraagtHerinnering` in de interface, ADR 0008).
fn vraagt_herinnering(soort: Vergadersoort) -> bool {
    soort != Vergadersoort::Alleen
}

/// Zo lang blijft een verzoek van het systeemvak om de vraag "Vertel het de anderen" staan. Wie
/// later pas op Opnemen komt, krijgt geen vraag uit het niets.
const VERZOEK_GELDIG: Duration = Duration::from_secs(30);

/// Het verzoek van het systeemvak: de soort die Rust koos (de vorige) en wanneer het gevraagd is.
#[derive(Default)]
struct StartVerzoek(Mutex<Option<(Vergadersoort, Instant)>>);

impl StartVerzoek {
    fn zet(&self, soort: Vergadersoort, nu: Instant) {
        *slot(&self.0) = Some((soort, nu));
    }

    /// Geeft het verzoek een keer terug en wist het; een verzoek ouder dan [`VERZOEK_GELDIG`]
    /// is vervallen.
    fn neem(&self, nu: Instant) -> Option<Vergadersoort> {
        let (soort, sinds) = slot(&self.0).take()?;
        (nu.saturating_duration_since(sinds) <= VERZOEK_GELDIG).then_some(soort)
    }
}

/// Sluit het open verslag zonder het te wissen (sinds ADR 0022 wist sluiten niets). Voor het
/// commando en voor "Opname starten" in het systeemvak. Weigert als bewaren mislukte (en `toch`
/// onwaar is), als opnieuw uitschrijven loopt of als er geluid wordt klaargezet.
fn sluit_verslag_blokkerend(app: &AppHandle, toch: bool) -> Result<VergaderingStand, Foutmelding> {
    probeer_opnieuw(app);
    let taak = slot(&app.state::<VergaderingStaat>().binnen).sluit_verslag(toch)?;
    app.state::<crate::bestanden::BestandenStaat>()
        .vergeet_los(taak);
    Ok(wijzig(app, werk_lijst_bij))
}

/// "Opname starten" in het menu van het systeemvak. Een open verslag gaat eerst dicht (zonder
/// wissen); lukt dat niet, dan opent het venster op Opnemen met de reden. Daarna de soort van de
/// vorige keer: bij Online en In de ruimte opent het venster op Opnemen met de vraag "Vertel het
/// de anderen" voor precies die soort (zie [`vergadering_start_gevraagd`]); bij Alleen ik begint
/// de opname meteen. Alles op een eigen thread: sluiten en het openen van de bronnen kunnen
/// seconden duren.
pub(crate) fn start_vanuit_systeemvak(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        let verslag_open = matches!(
            slot(&app.state::<VergaderingStaat>().binnen).fase,
            Fase::Verslag(_)
        );
        if verslag_open && let Err(fout) = sluit_verslag_blokkerend(&app, false) {
            wijzig(&app, |b| b.fout = Some(fout));
            crate::toon_opnemen(&app);
            return;
        }
        let soort = lees_soort(&app);
        if vraagt_herinnering(soort) {
            app.state::<VergaderingStaat>()
                .start_gevraagd
                .zet(soort, Instant::now());
            crate::toon_opnemen(&app);
            let _ = app.emit("vergadering-start-gevraagd", ());
            return;
        }
        if let Err(fout) = start_blokkerend(&app, soort, None, None) {
            wijzig(&app, |b| b.fout = Some(fout));
            crate::toon_opnemen(&app);
        }
    });
}

/// "Stoppen en uitschrijven" in het menu van het systeemvak. Op een eigen thread: stoppen kan
/// even duren.
pub(crate) fn stop_vanuit_systeemvak(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        if let Err(fout) = stop_als_loopt(&app, true) {
            wijzig(&app, |b| b.fout = Some(fout));
        }
    });
}

/// Heeft het menu van het systeemvak om de herinnering gevraagd? Geeft de soort waarvoor, een
/// keer, en wist het verzoek; na 30 seconden is het vervallen. De interface zet het scherm op die
/// soort voordat de vraag komt. Ze vraagt dit bij het openen van Opnemen en na het event
/// `vergadering-start-gevraagd`, zodat het ook werkt als het scherm er nog niet stond.
#[tauri::command]
pub(crate) fn vergadering_start_gevraagd(app: AppHandle) -> Option<Vergadersoort> {
    app.state::<VergaderingStaat>()
        .start_gevraagd
        .neem(Instant::now())
}

/// "Later uitschrijven": het uitschrijven stopt en de opname blijft staan.
#[tauri::command]
pub(crate) fn vergadering_later(app: AppHandle) -> VergaderingStand {
    let taak = match &slot(&app.state::<VergaderingStaat>().binnen).fase {
        Fase::Uitschrijven(u) => Some(u.taak),
        _ => None,
    };
    if let Some(taak) = taak {
        annuleer_uitschrijven(&app, taak);
    }
    wijzig(&app, |b| {
        if matches!(b.fase, Fase::Uitschrijven(_)) {
            b.fase = Fase::Klaar;
        }
        werk_lijst_bij(b);
    })
}

/// Wist een opname met al haar audio. Zonder `id` de opname waar de app nu mee bezig is (de
/// lopende opname, het uitschrijven of het verslag); met `id` een opname uit de lijst.
#[tauri::command]
pub(crate) async fn vergadering_weggooien(
    app: AppHandle,
    id: Option<String>,
) -> Result<VergaderingStand, Foutmelding> {
    blokkerend(move || {
        let staat = app.state::<VergaderingStaat>();
        let mut geduld = false;
        let map = if let Some(id) = id {
            map_van(&slot(&staat.binnen), &id)?.0
        } else {
            let (loopt, actief) = {
                let binnen = slot(&staat.binnen);
                match &binnen.fase {
                    Fase::Voorbereiden(_) => {
                        return Err(Foutmelding::nieuw(
                            "Mondig is nog bezig met deze opname. Probeer het zo opnieuw.",
                            Vervolg::OpnieuwProberen,
                        ));
                    }
                    Fase::Verslag(v) if v.audio_bezig => {
                        return Err(Foutmelding::nieuw(
                            "Mondig bewaart de opname nog. Wacht tot dat klaar is.",
                            Vervolg::OpnieuwProberen,
                        ));
                    }
                    Fase::Verslag(v) if v.luistert > 0 => return Err(speelt_nog()),
                    Fase::Verslag(v) if v.bezig_met_werk() => {
                        return Err(bezig_sluiten(v));
                    }
                    _ => {}
                }
                let taak = match &binnen.fase {
                    Fase::Uitschrijven(u) => Some(u.taak),
                    _ => None,
                };
                (
                    matches!(binnen.fase, Fase::NeemtOp(_)),
                    (binnen.actieve_map().map(Path::to_path_buf), taak),
                )
            };
            if loopt {
                stop_opname(&app, false);
            }
            if let Some(taak) = actief.1 {
                annuleer_uitschrijven(&app, taak);
                geduld = true;
            }
            let Some(map) = actief.0 else {
                return Err(Foutmelding::nieuw(
                    "Er is geen opname om weg te gooien.",
                    Vervolg::Geen,
                ));
            };
            wijzig(&app, |b| b.fase = Fase::Klaar);
            map
        };
        let gewist = wis_met_geduld(&map, geduld);
        Ok(wijzig(&app, |b| {
            b.fout = gewist.err();
            werk_lijst_bij(b);
        }))
    })
    .await
}

/// Schrijft een opname uit de lijst uit die netjes is afgesloten ("Later uitschrijven").
#[tauri::command]
pub(crate) async fn vergadering_uitschrijven(
    app: AppHandle,
    id: String,
) -> Result<VergaderingStand, Foutmelding> {
    blokkerend(move || {
        let map = claim_opname(&app, &id, false)?;
        Ok(zet_in_wachtrij(&app, &map))
    })
    .await
}

/// Herstelt een afgebroken opname tot vlak voor het moment dat het misging, en schrijft haar
/// uit.
#[tauri::command]
pub(crate) async fn vergadering_herstel(
    app: AppHandle,
    id: String,
) -> Result<VergaderingStand, Foutmelding> {
    blokkerend(move || {
        let map = claim_opname(&app, &id, true)?;
        match vergadering::herstel(&map) {
            Ok(afsluiting) => {
                wijzig(&app, |b| {
                    if let Fase::Voorbereiden(v) = &mut b.fase
                        && v.map == map
                    {
                        v.duur_ms = Some(afsluiting.duur_ms);
                    }
                });
                Ok(zet_in_wachtrij(&app, &map))
            }
            Err(fout) => {
                wijzig(&app, |b| {
                    b.klaar_na_voorbereiden(&map, Some(fout.clone()));
                    werk_lijst_bij(b);
                });
                Err(fout)
            }
        }
    })
    .await
}

/// Claimt een opname uit de lijst om uit te schrijven (of eerst te herstellen): de controle
/// en de overgang naar Voorbereiden gebeuren onder hetzelfde slot.
fn claim_opname(app: &AppHandle, id: &str, herstellen: bool) -> Result<PathBuf, Foutmelding> {
    let staat = app.state::<VergaderingStaat>();
    let stand = {
        let mut binnen = slot(&staat.binnen);
        let map = claim_in(&mut binnen, id, herstellen)?;
        binnen.versie += 1;
        (map, binnen.stand())
    };
    stuur_stand(app, &stand.1);
    Ok(stand.0)
}

/// Zie [`claim_opname`]; zonder app, om te testen.
fn claim_in(binnen: &mut Binnen, id: &str, herstellen: bool) -> Result<PathBuf, Foutmelding> {
    werk_lijst_bij(binnen);
    claim_uit_lijst(binnen, id, herstellen)
}

/// Claimt de opname `id` uit de lijst zoals die nu in `binnen` staat.
fn claim_uit_lijst(
    binnen: &mut Binnen,
    id: &str,
    herstellen: bool,
) -> Result<PathBuf, Foutmelding> {
    let (map, info) = map_van(binnen, id)?;
    if !herstellen && !info.afgesloten {
        return Err(Foutmelding::nieuw(
            "Deze opname is niet goed afgesloten. Herstel haar eerst.",
            Vervolg::Geen,
        ));
    }
    binnen.claim(Fase::Voorbereiden(Voorbereiden {
        map: map.clone(),
        soort: info.soort,
        begin_ms: info.begin_ms,
        duur_ms: info.duur_ms,
        herstellen,
    }))?;
    Ok(map)
}

/// Geeft de spreker van een beurt een naam: alleen deze beurt (`beurt`), alle beurten van die
/// kant met dezelfde naam (`kant`) of alle beurten van die spreker (`spreker`, 4b-D). Een lege
/// naam zet terug naar "Jij", "Anderen" of "Spreker 1".
#[tauri::command]
pub(crate) async fn vergadering_naam(
    app: AppHandle,
    index: usize,
    naam: String,
    bereik: Naambereik,
) -> Result<VergaderingStand, Foutmelding> {
    wijzig_verslag(app, false, move |gesprek| {
        gesprek.geef_naam(index, &naam, bereik)
    })
    .await
}

/// Verbetert de tekst van een beurt (V-1). Kopiëren en alle exports gebruiken daarna de nieuwe
/// tekst. Een lege tekst laat de oude staan.
#[tauri::command]
pub(crate) async fn vergadering_zet_tekst(
    app: AppHandle,
    index: usize,
    tekst: String,
) -> Result<VergaderingStand, Foutmelding> {
    wijzig_verslag(app, false, move |gesprek| {
        gesprek.zet_tekst(index, &tekst).map(usize::from)
    })
    .await
}

/// Het stuk geluid van `begin_ms` tot `eind_ms` van de opname achter het verslag, als WAV-bytes
/// (V-1): beide sporen gemengd, op de tijdlijn van het verslag. De interface maakt er een blob
/// van en krijgt nooit een pad of bestandsrecht. Is de opname weg, dan een melding in gewone taal.
#[tauri::command]
pub(crate) async fn vergadering_audio_stuk(
    app: AppHandle,
    begin_ms: u64,
    eind_ms: u64,
) -> Result<tauri::ipc::Response, Foutmelding> {
    blokkerend(move || {
        // Zolang het stuk wordt gemengd, weigeren sluiten en weggooien (`luistert`).
        let (taak, map) = match &mut slot(&app.state::<VergaderingStaat>().binnen).fase {
            Fase::Verslag(v) => {
                v.luistert += 1;
                (v.taak, v.map.clone())
            }
            _ => return Err(geen_verslag()),
        };
        let _klaar = LuisterKlaar {
            app: app.clone(),
            taak,
        };
        if !map.join(EINDE).is_file() {
            return Err(Foutmelding::nieuw(
                "De opname is niet meer beschikbaar, dus er is niets af te spelen.",
                Vervolg::Geen,
            ));
        }
        afspelen::fragment_uit_opname(&map, begin_ms, eind_ms)
            .map(tauri::ipc::Response::new)
            .map_err(|f| mengfout(&f))
    })
    .await
}

/// Zet "Persoonsgegevens vervangen" aan of uit voor het verslag dat open is (V-2): Kopiëren en alle
/// exports gebruiken dan `[BSN]`, `[IBAN]` enzovoort in plaats van de gegevens. Er wordt geen sleutel
/// bewaard.
#[tauri::command]
pub(crate) async fn vergadering_persoon_vervangen(
    app: AppHandle,
    aan: bool,
) -> Result<VergaderingStand, Foutmelding> {
    wijzig_persoon(app, move |persoon| {
        persoon.zet_vervangen(aan);
        Ok(())
    })
    .await
}

/// Haalt een markering weg (V-2): een gevonden gegeven wordt in dit verslag overal genegeerd, een
/// zelf gemarkeerd stuk verdwijnt.
#[tauri::command]
pub(crate) async fn vergadering_persoon_weghalen(
    app: AppHandle,
    waarde: String,
) -> Result<VergaderingStand, Foutmelding> {
    wijzig_persoon(app, move |persoon| {
        persoon.haal_weg(&waarde);
        Ok(())
    })
    .await
}

/// Markeert zelf een stuk tekst (V-2), overal in het verslag waar het als los stuk staat.
#[tauri::command]
pub(crate) async fn vergadering_persoon_markeer(
    app: AppHandle,
    waarde: String,
) -> Result<VergaderingStand, Foutmelding> {
    wijzig_persoon(app, move |persoon| {
        persoon.markeer(&waarde).map_err(markeerfout)
    })
    .await
}

/// Past de markeringen van het verslag dat open is aan en stuurt de nieuwe stand. Wat bewaard of
/// gekopieerd was, klopt dan niet meer, maar dat ziet de interface aan de schakelaar zelf.
async fn wijzig_persoon(
    app: AppHandle,
    wijziging: impl FnOnce(&mut Markeringen) -> Result<(), Foutmelding> + Send + 'static,
) -> Result<VergaderingStand, Foutmelding> {
    wijzig_open(app, move |v| wijziging(&mut v.persoon)).await
}

/// Past het verslag dat open is aan, bewaart het naast de opname (ADR 0022) en stuurt de nieuwe
/// stand naar de interface. De wijziging en het serialiseren gebeuren onder het slot van de stand,
/// het schrijven daarbuiten en niet op de hoofddraad. Een mislukte wijziging bewaart niets, en een
/// wijziging die niets veranderde ook niet.
async fn wijzig_open(
    app: AppHandle,
    wijziging: impl FnOnce(&mut Verslag) -> Result<(), Foutmelding> + Send + 'static,
) -> Result<VergaderingStand, Foutmelding> {
    blokkerend(move || {
        let staat = app.state::<VergaderingStaat>();
        let opdracht = wijzig_open_in(&mut slot(&staat.binnen), wijziging)?;
        if let Some(opdracht) = opdracht {
            voer_uit(&app, &opdracht);
        }
        Ok(wijzig(&app, |_| {}))
    })
    .await
}

/// Zie [`wijzig_open`]: het deel onder het slot, zonder app, om te testen. Geeft de opdracht om
/// te schrijven, als er iets veranderde.
fn wijzig_open_in(
    binnen: &mut Binnen,
    wijziging: impl FnOnce(&mut Verslag) -> Result<(), Foutmelding>,
) -> Result<Option<Schrijfopdracht>, Foutmelding> {
    let Fase::Verslag(v) = &mut binnen.fase else {
        return Err(geen_verslag());
    };
    if v.opnieuw.is_some() {
        return Err(opnieuw_bezig());
    }
    if v.notulen_maken.is_some() {
        return Err(notulen_bezig());
    }
    wijziging(v)?;
    Ok(binnen.plan_bewaren(false))
}

/// Geeft een beurt aan een andere spreker (4b-D), bijvoorbeeld als het model twee mensen
/// verwisselde. Zonder `spreker` ("Nieuwe spreker") krijgt de beurt het eerstvolgende vrije
/// nummer. Ook een beurt zonder spreker kan zo een spreker krijgen. De beurt gaat daarna samen
/// met een buur van dezelfde spreker, dus de nummering van de beurten kan veranderen.
#[tauri::command]
pub(crate) async fn vergadering_geef_aan_spreker(
    app: AppHandle,
    index: usize,
    spreker: Option<u8>,
) -> Result<VergaderingStand, Foutmelding> {
    wijzig_verslag(app, true, move |gesprek| match spreker {
        Some(nummer) => gesprek.geef_aan_spreker(index, nummer),
        None => gesprek.geef_aan_nieuwe_spreker(index).map(|_| 1),
    })
    .await
}

/// Voegt twee sprekers samen (4b-D): alle beurten van `van` worden van `naar`.
#[tauri::command]
pub(crate) async fn vergadering_voeg_sprekers_samen(
    app: AppHandle,
    van: u8,
    naar: u8,
) -> Result<VergaderingStand, Foutmelding> {
    wijzig_verslag(app, true, move |gesprek| {
        gesprek.voeg_sprekers_samen(van, naar)
    })
    .await
}

/// Past het gesprek van het verslag dat open is aan en stuurt de nieuwe stand naar de interface.
/// `verschuift`: kan de wijziging beurten samenvoegen of andere beurten raken (zie
/// [`Verslag::na_gesprekswijziging`])?
async fn wijzig_verslag(
    app: AppHandle,
    verschuift: bool,
    wijziging: impl FnOnce(&mut Gesprek) -> Result<usize, Foutmelding> + Send + 'static,
) -> Result<VergaderingStand, Foutmelding> {
    wijzig_open(app, move |v| wijzig_gesprek(v, verschuift, wijziging)).await
}

/// Zie [`wijzig_verslag`]: het deel op het verslag, zonder app, om te testen.
fn wijzig_gesprek(
    v: &mut Verslag,
    verschuift: bool,
    wijziging: impl FnOnce(&mut Gesprek) -> Result<usize, Foutmelding>,
) -> Result<(), Foutmelding> {
    let aantal = wijziging(&mut v.gesprek)?;
    v.na_gesprekswijziging(aantal, verschuift);
    // De bronnen van de notulen kloppen misschien niet meer met de nieuwe tekst of namen.
    if aantal > 0 {
        v.hercontroleer_notulen();
    }
    Ok(())
}

/// Geeft het verslag dat open is een naam (ADR 0022). Die naam is de titel in de lijst "Eerder
/// opgenomen", in het verslag en bij Opslaan en Kopiëren. Een lege naam zet de titel terug die
/// Mondig zelf maakt.
#[tauri::command]
pub(crate) async fn vergadering_verslag_naam(
    app: AppHandle,
    naam: String,
) -> Result<VergaderingStand, Foutmelding> {
    wijzig_open(app, move |v| {
        v.naam = schone_verslagnaam(&naam);
        Ok(())
    })
    .await
}

/// Het taak-id van het verslag dat nu open is.
fn verslag_taak(app: &AppHandle) -> Result<TaakId, Foutmelding> {
    verslag_taak_in(&slot(&app.state::<VergaderingStaat>().binnen))
}

/// Slaat het verslag op, met de namen, via dezelfde route als een transcript (4-C): de map
/// uit de instelling of de mapkiezer, het standaardformaat, en nooit stil overschrijven. Een
/// vraag "bestaat al" beantwoordt de interface met `bestand_bewaar_beslis`.
#[tauri::command]
pub(crate) async fn vergadering_opslaan(
    app: AppHandle,
    formaat: Option<Formaat>,
    andere_plek: bool,
) -> Result<BewaarUitslag, Foutmelding> {
    blokkerend(move || {
        let taak = verslag_taak(&app)?;
        crate::bewaren::bewaar(&app, taak, formaat, andere_plek)
    })
    .await
}

/// Zet het verslag als platte tekst op het klembord, buiten de klembordgeschiedenis.
#[tauri::command]
pub(crate) async fn vergadering_kopieer(app: AppHandle) -> Result<(), Foutmelding> {
    blokkerend(move || {
        let tekst = match &slot(&app.state::<VergaderingStaat>().binnen).fase {
            Fase::Verslag(v) => {
                let verslag = v.gesprek.als_tekst_met(
                    |tekst| v.persoon.voor_uitvoer(tekst),
                    |naam| v.persoon.voor_naam(naam),
                );
                // De notulen komen vóór het verslag, zoals bij Opslaan (ADR 0026 punt 8).
                match &v.notulen {
                    Some(OpgeslagenNotulen::Leesbaar(n)) => {
                        let blok = n.als_blok(&v.gesprek, &v.persoon);
                        format!(
                            "{}

{}

{verslag}",
                            notulen_maken::blok_als_tekst(&blok),
                            blok.verslagkop
                        )
                    }
                    _ => verslag,
                }
            }
            _ => {
                return Err(Foutmelding::nieuw(
                    "Er is geen verslag open.",
                    Vervolg::Geen,
                ));
            }
        };
        if crate::plakken::zet_op_klembord(&tekst) {
            Ok(())
        } else {
            Err(Foutmelding::nieuw(
                "Kopiëren is niet gelukt. Probeer het opnieuw.",
                Vervolg::OpnieuwProberen,
            ))
        }
    })
    .await
}

/// De terugval voor de naam van de WAV in de bewaarkiezer, als er geen verslagnaam te bepalen
/// is: "Vergadering 2026-11-02 14.00.wav", bij Alleen ik "Notitie 2026-11-02 14.00.wav".
fn wav_naam(soort: Vergadersoort, begin_ms: u64) -> String {
    let moment = bewaren::lokaal_moment(begin_ms);
    format!(
        "{} {:04}-{:02}-{:02} {:02}.{:02}.wav",
        soort.titelwoord(),
        moment.year(),
        moment.month(),
        moment.day(),
        moment.hour(),
        moment.minute()
    )
}

fn mengfout(fout: &MengFout) -> Foutmelding {
    crate::logboek::mislukt(
        mondig_core::taak::Soort::Vergadering,
        fout.schrijffout().and_then(|f| Probleem::uit_io(f.kind())),
    );
    match fout.schrijffout().map(std::io::Error::kind) {
        Some(std::io::ErrorKind::StorageFull) => Foutmelding::nieuw(
            "De schijf is vol. Maak ruimte vrij of kies een andere plek.",
            Vervolg::OpnieuwProberen,
        ),
        Some(std::io::ErrorKind::PermissionDenied) => Foutmelding::nieuw(
            "Mondig mag daar niet schrijven. Kies een andere plek.",
            Vervolg::OpnieuwProberen,
        ),
        _ => Foutmelding::nieuw(fout.to_string(), Vervolg::OpnieuwProberen),
    }
}

/// Bewaart de opname van het verslag als één WAV-bestand (beide sporen gemengd) op een plek die
/// de gebruiker kiest. Geeft de bestandsnaam, of `None` als de gebruiker de kiezer sloot. Een
/// bestaand bestand vervangt Mondig alleen als de gebruiker dat in de kiezer bevestigde.
#[tauri::command]
pub(crate) async fn vergadering_audio_bewaren(
    app: AppHandle,
) -> Result<Option<String>, Foutmelding> {
    blokkerend(move || {
        let (taak, map, begin_ms, soort) = {
            let staat = app.state::<VergaderingStaat>();
            let mut binnen = slot(&staat.binnen);
            match &mut binnen.fase {
                Fase::Verslag(v) if v.audio_bezig => {
                    return Err(Foutmelding::nieuw(
                        "Mondig bewaart de opname al.",
                        Vervolg::Geen,
                    ));
                }
                Fase::Verslag(v) => {
                    // Zolang dit loopt, weigeren sluiten en weggooien: het mengen leest de map.
                    v.audio_bezig = true;
                    (v.taak, v.map.clone(), v.begin_ms, v.gesprek.soort)
                }
                _ => return Err(geen_verslag()),
            }
        };
        let _klaar = AudioKlaar {
            app: app.clone(),
            taak,
        };
        // Dezelfde naam als het verslag zou krijgen, in de map waar ook de tekst staat.
        let naam =
            crate::bewaren::wav_naam_voor(&app, taak).unwrap_or_else(|| wav_naam(soort, begin_ms));
        let mut kiezer = app
            .dialog()
            .file()
            .set_file_name(naam)
            .add_filter("Geluid (WAV)", &["wav"]);
        if let Some(venster) = app.get_webview_window("main") {
            kiezer = kiezer.set_parent(&venster);
        }
        if let Some(map) = crate::bewaren::ingestelde_map(&app) {
            kiezer = kiezer.set_directory(map);
        }
        let Some(pad) = kiezer.blocking_save_file().and_then(|p| p.into_path().ok()) else {
            return Ok(None);
        };
        meng_naar_bestand(&map, &pad).map_err(|f| mengfout(&f))?;
        Ok(pad.file_name().map(|n| n.to_string_lossy().into_owned()))
    })
    .await
}

/// Zet de opname als WAV naast een net opgeslagen verslag, als de gebruiker "Opname ook
/// bewaren als geluidsbestand" aanzette (aanvulling op ADR 0008: standaard wordt de audio
/// gewist, dit is een keuze van de gebruiker). `None` als de instelling uit staat of `taak`
/// niet het verslag van nu is; anders de naam van de WAV, of een fout in gewone taal. Het
/// tekstbestand staat er dan al en blijft staan. Zolang het mengen loopt, weigeren sluiten en
/// weggooien (`audio_bezig`); de opname blijft daarna bij het verslag staan tot de bewaartermijn.
pub(crate) fn audio_naast_verslag(
    app: &AppHandle,
    taak: TaakId,
    tekst: &Path,
    vervangen: bool,
) -> Option<Result<String, Foutmelding>> {
    let staat = app.try_state::<VergaderingStaat>()?;
    let map = {
        let mut binnen = slot(&staat.binnen);
        match &mut binnen.fase {
            Fase::Verslag(v) if v.taak == taak => {
                if !crate::bewaren::audio_naast_aan(app) {
                    return None;
                }
                if v.persoon.vervangen() {
                    // Het geluid bevat alles wat er gezegd is: naast een verslag met
                    // plaatsvervangers zou het de vervanging ongedaan maken.
                    return Some(Err(Foutmelding::nieuw(
                        "De opname is niet naast het verslag gezet, omdat persoonsgegevens worden vervangen en het geluid alles bevat. Zet het vervangen uit om de opname mee te bewaren.",
                        Vervolg::Geen,
                    )));
                }
                if v.audio_bezig {
                    return Some(Err(Foutmelding::nieuw(
                        "Mondig bewaart de opname al.",
                        Vervolg::Geen,
                    )));
                }
                v.audio_bezig = true;
                v.map.clone()
            }
            _ => return None,
        }
    };
    let _klaar = AudioKlaar {
        app: app.clone(),
        taak,
    };
    Some(wav_naast(&map, tekst, vervangen))
}

/// Mengt de opname in `opnamemap` naar een WAV naast het tekstbestand `tekst`, met dezelfde
/// basisnaam. Geeft de naam van de WAV. Bestaat die al, dan dezelfde regel als bij het
/// tekstbestand: koos de gebruiker daar Vervangen, dan wordt ook de WAV vervangen; anders
/// krijgt de WAV het eerste vrije volgnummer, want een vraag erbij past hier niet en stil
/// overschrijven doet Mondig nooit. Eerst naar een tijdelijk bestand in dezelfde map, zodat er
/// bij een fout geen half bestand blijft staan.
fn wav_naast(opnamemap: &Path, tekst: &Path, vervangen: bool) -> Result<String, Foutmelding> {
    let map = tekst.parent().unwrap_or_else(|| Path::new("."));
    let naam = crate::bewaren::als_wav(
        &tekst
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    );
    let mut tijdelijk = tempfile::Builder::new()
        .prefix(".mondig-")
        .suffix(".tijdelijk")
        .tempfile_in(map)
        .map_err(|f| crate::bewaren::schrijffout(&f, map, None))?
        .into_temp_path();
    meng_naar_bestand(opnamemap, &tijdelijk).map_err(|f| mengfout(&f))?;
    let doel = map.join(&naam);
    if vervangen {
        return tijdelijk
            .persist(&doel)
            .map(|()| naam)
            .map_err(|f| crate::bewaren::schrijffout(&f.error, map, Some(&doel)));
    }
    let begin = crate::bewaren::eerste_nummer(map, &naam);
    let kandidaten = std::iter::once(naam.clone())
        .chain((begin..begin.saturating_add(100)).map(|n| bewaren::naam::met_volgnummer(&naam, n)));
    for kandidaat in kandidaten {
        match tijdelijk.persist_noclobber(map.join(&kandidaat)) {
            Ok(()) => return Ok(kandidaat),
            Err(fout) if fout.error.kind() == std::io::ErrorKind::AlreadyExists => {
                tijdelijk = fout.path;
            }
            Err(fout) => return Err(crate::bewaren::schrijffout(&fout.error, map, None)),
        }
    }
    Err(Foutmelding::nieuw(
        "Mondig vond geen vrije naam voor de opname.",
        Vervolg::Geen,
    ))
}

/// Haalt de vlag "audio bezig" weer weg als het bewaren afloopt, hoe dan ook.
struct AudioKlaar {
    app: AppHandle,
    taak: TaakId,
}

impl Drop for AudioKlaar {
    fn drop(&mut self) {
        if let Some(staat) = self.app.try_state::<VergaderingStaat>()
            && let Fase::Verslag(v) = &mut slot(&staat.binnen).fase
            && v.taak == self.taak
        {
            v.audio_bezig = false;
        }
    }
}

/// Haalt de teller "er wordt geluid klaargezet" weer omlaag als het mengen afloopt, hoe dan ook.
struct LuisterKlaar {
    app: AppHandle,
    taak: TaakId,
}

impl Drop for LuisterKlaar {
    fn drop(&mut self) {
        if let Some(staat) = self.app.try_state::<VergaderingStaat>()
            && let Fase::Verslag(v) = &mut slot(&staat.binnen).fase
            && v.taak == self.taak
        {
            v.luistert = v.luistert.saturating_sub(1);
        }
    }
}

/// Probeert nog één keer een verslag te schrijven waarvan de laatste poging mislukte, voordat het
/// dicht gaat.
fn probeer_opnieuw(app: &AppHandle) {
    let opdracht = slot(&app.state::<VergaderingStaat>().binnen).plan_herstel();
    if let Some(opdracht) = opdracht {
        voer_uit(app, &opdracht);
    }
}

/// Sluit het verslag (ADR 0022). Er wordt niets gewist: het verslag staat daarna onder "Eerder
/// opgenomen", tot de bewaartermijn om is of de gebruiker het wist. Is het verslag niet bewaard
/// (de schijf was vol), dan probeert Mondig het nog eens en weigert daarna met een melding,
/// tenzij `toch` waar is.
#[tauri::command]
pub(crate) async fn vergadering_sluiten(
    app: AppHandle,
    toch: bool,
) -> Result<VergaderingStand, Foutmelding> {
    blokkerend(move || sluit_verslag_blokkerend(&app, toch)).await
}

/// Opent een bewaard verslag uit de lijst "Eerder opgenomen" (ADR 0022). Een verslag dat al
/// open is, gaat eerst dicht, zonder wissen (en met dezelfde weigering en `toch` als
/// [`vergadering_sluiten`]). Tijdens een opname, het voorbereiden of het uitschrijven weigert
/// Mondig met een melding in gewone taal.
#[tauri::command]
pub(crate) async fn vergadering_verslag_openen(
    app: AppHandle,
    id: String,
    toch: bool,
) -> Result<VergaderingStand, Foutmelding> {
    blokkerend(move || {
        let basis = vergadering::vergadermap()?;
        probeer_opnieuw(&app);
        let staat = app.state::<VergaderingStaat>();
        let gesloten = slot(&staat.binnen).open_verslag(&basis, &id, toch);
        match gesloten {
            Ok(gesloten) => {
                if let Some(taak) = gesloten {
                    app.state::<crate::bestanden::BestandenStaat>()
                        .vergeet_los(taak);
                }
                Ok(wijzig(&app, |_| {}))
            }
            Err(fout) => {
                // Een verslag dat niet te lezen was, is intussen uit de lijst gehaald.
                wijzig(&app, |_| {});
                Err(fout)
            }
        }
    })
    .await
}

/// Wist een verslag met zijn opname: de hele map, na de bevestigvraag in de interface (ADR 0022).
/// Zonder `id` het verslag dat open is; met `id` een verslag uit de lijst "Eerder opgenomen".
/// Weigert zolang het geluid van het open verslag wordt gemengd of afgespeeld. Zolang het wissen
/// duurt, staat de map in `wordt_gewist`: ze komt in geen lijst terug en het opruimen slaat haar over.
#[tauri::command]
pub(crate) async fn vergadering_verslag_wissen(
    app: AppHandle,
    id: Option<String>,
) -> Result<VergaderingStand, Foutmelding> {
    blokkerend(move || {
        let staat = app.state::<VergaderingStaat>();
        let (map, taak) = {
            let mut binnen = slot(&staat.binnen);
            let (map, taak) = if let Some(id) = id {
                let (map, _) = verslag_van(&binnen, &vergadering::vergadermap()?, &id)?;
                (map, None)
            } else {
                let taak = verslag_taak_in(&binnen)?;
                (binnen.neem_verslag()?, Some(taak))
            };
            if let Some(naam) = map.file_name() {
                let naam = naam.to_string_lossy().into_owned();
                binnen.verslagen.retain(|v| v.id != naam);
                binnen.wordt_gewist.push(naam);
            }
            (map, taak)
        };
        if let Some(taak) = taak {
            app.state::<crate::bestanden::BestandenStaat>()
                .vergeet_los(taak);
        }
        let gewist = wis_met_geduld(&map, false);
        Ok(wijzig(&app, |b| {
            if let Some(naam) = map.file_name() {
                let naam = naam.to_string_lossy();
                b.wordt_gewist.retain(|n| *n != *naam);
            }
            b.fout = gewist.err();
            werk_lijst_bij(b);
        }))
    })
    .await
}

// ---------------------------------------------------------------------------
// Opnieuw uitschrijven (ADR 0023)
// ---------------------------------------------------------------------------

/// Een model waarmee het open verslag opnieuw kan worden uitgeschreven.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpnieuwKeuze {
    model_id: String,
    rol: Modelrol,
    /// Staat het model helemaal op deze computer? Anders toont de dialoog de downloadgrootte en
    /// een knop naar Modellen in plaats van te starten.
    aanwezig: bool,
    /// Wat er te downloaden is.
    grootte_bytes: u64,
    /// Hoe lang het voor dit verslag ongeveer duurt, uit de gemeten snelheid van het model op
    /// deze computer; `None` als die er nog niet is.
    duur_ms: Option<u64>,
}

/// Wat de dialoog "Opnieuw uitschrijven" toont.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpnieuwKeuzes {
    /// De rol van het model waarmee het verslag nu is uitgeschreven.
    rol: Option<Modelrol>,
    /// De nauwkeurige modellen, zonder het model van nu; nauwkeurig voor extra nauwkeurig.
    keuzes: Vec<OpnieuwKeuze>,
}

/// De keuzes uit de catalogus voor een verslag dat met `huidig` is uitgeschreven: de modellen
/// met een hogere rol dan `huidig` (bij een onbekend model: nauwkeurig en extra nauwkeurig). Pure functie: de aanwezigheid
/// en de gemeten snelheid komen van de aanroeper.
fn opnieuw_keuzes(
    modellen: &[catalogus::Model],
    huidig: &str,
    geluid: Duration,
    aanwezig: impl Fn(&catalogus::Model) -> bool,
    factor: impl Fn(&str) -> Option<f64>,
) -> OpnieuwKeuzes {
    let huidige_rol = catalogus::modelrol(modellen, huidig);
    let mut keuzes: Vec<OpnieuwKeuze> = modellen
        .iter()
        .filter(|m| m.kiesbaar_voor(Gebruik::Bestanden) && m.id != huidig)
        .filter_map(|m| {
            let rol = catalogus::modelrol(modellen, &m.id)?;
            // Alleen nauwkeuriger dan het model van nu (snel < nauwkeurig < extra nauwkeurig).
            let hoger = huidige_rol.map_or(rol != Modelrol::Snel, |h| rol > h);
            hoger.then(|| OpnieuwKeuze {
                model_id: m.id.clone(),
                rol,
                aanwezig: aanwezig(m),
                grootte_bytes: m.totale_grootte(),
                duur_ms: factor(&m.id)
                    .and_then(|f| crate::snelheid::schat(f, geluid))
                    .map(ms),
            })
        })
        .collect();
    keuzes.sort_by_key(|k| k.rol == Modelrol::ExtraNauwkeurig);
    OpnieuwKeuzes {
        rol: huidige_rol,
        keuzes,
    }
}

/// De modellen waarmee het open verslag opnieuw kan worden uitgeschreven, met per model of het
/// er al is, de downloadgrootte en de verwachte duur.
#[tauri::command]
pub(crate) async fn vergadering_opnieuw_keuzes(
    app: AppHandle,
) -> Result<OpnieuwKeuzes, Foutmelding> {
    blokkerend(move || {
        let (huidig, geluid) = match &slot(&app.state::<VergaderingStaat>().binnen).fase {
            Fase::Verslag(v) => (v.model_id.clone(), v.gesprek.verwerkt_geluid()),
            _ => return Err(geen_verslag()),
        };
        let modellen = catalogus::catalogus().map_err(|_| {
            Foutmelding::nieuw("De lijst met modellen is niet te lezen.", Vervolg::Geen)
        })?;
        let snelheden = crate::snelheid::lees(&app);
        Ok(opnieuw_keuzes(
            &modellen,
            &huidig,
            geluid,
            |m| catalogus::modelmap(&m.id).is_ok_and(|map| download::is_compleet(m, &map)),
            |id| snelheden.get(id).copied(),
        ))
    })
    .await
}

/// Begint het open verslag opnieuw uit te schrijven met model `model` (ADR 0023), via dezelfde
/// wachtrij als het gewone uitschrijven. Is dat een ander model dan dat van de wachtrij, dan
/// krijgt het werk een eigen motor; de wachtrij stopt eerst zijn eigen werkproces. Tijdens het
/// lopen blijft het verslag te lezen, te kopiëren en op te slaan.
#[tauri::command]
pub(crate) async fn vergadering_opnieuw_start(
    app: AppHandle,
    model: String,
) -> Result<VergaderingStand, Foutmelding> {
    blokkerend(move || start_opnieuw(&app, &model)).await
}

fn start_opnieuw(app: &AppHandle, model_id: &str) -> Result<VergaderingStand, Foutmelding> {
    let (model, modelmap) = crate::modellen::zoek_spraakmodel(model_id)?;
    if !model.kiesbaar_voor(Gebruik::Bestanden) {
        return Err(Foutmelding::nieuw(
            "Met dit model kan Mondig geen verslag uitschrijven.",
            Vervolg::Geen,
        ));
    }
    if !download::is_compleet(&model, &modelmap) {
        return Err(Foutmelding::nieuw(
            "Dit model staat nog niet op deze computer. Download het eerst bij Modellen.",
            Vervolg::Geen,
        ));
    }
    let rol = catalogus::catalogus()
        .ok()
        .and_then(|lijst| catalogus::modelrol(&lijst, &model.id));
    let factor = crate::snelheid::lees(app).get(&model.id).copied();
    let staat = app.state::<VergaderingStaat>();
    let stand = crate::bestanden::met_wachtrij(app, |wachtrij, rij_model, _| {
        // Slotvolgorde zoals bij `zet_in_wachtrij`: eerst de rij, dan de stand.
        let mut binnen = slot(&staat.binnen);
        crate::videokaart::weiger_tijdens_test(app)?;
        let VoorOpnieuw {
            map,
            oud,
            persoon,
            naam,
        } = binnen.voor_opnieuw()?;
        let geluid = oud.verwerkt_geluid();
        let vergelijk_met = oud.clone();
        let motor: Option<Box<dyn Bestandsmotor>> = if rij_model == model.id {
            None
        } else {
            Some(Box::new(crate::bestanden::motor_voor(
                app,
                &model,
                modelmap.clone(),
            )?))
        };
        let voor_uitslag = app.clone();
        let taak = wachtrij
            .voeg_opnieuw_toe_met(map, oud, motor, move |taak, uitslag| {
                bij_opnieuw_uitslag(
                    &voor_uitslag,
                    taak,
                    uitslag,
                    &(vergelijk_met, persoon, naam),
                );
            })
            .map_err(|_| {
                Foutmelding::nieuw(
                    "Het uitschrijven is gestopt. Start Mondig opnieuw.",
                    Vervolg::Geen,
                )
            })?;
        binnen.begin_opnieuw(Opnieuw {
            taak,
            model_id: model.id.clone(),
            model: crate::bestanden::korte_naam(&model.naam),
            rol,
            voortgang: None,
            begonnen: None,
            meetpunt: None,
            verwacht: factor.and_then(|f| crate::snelheid::schat(f, geluid)),
        });
        binnen.versie += 1;
        Ok(binnen.stand())
    })
    .and_then(|uitkomst| uitkomst)?;
    stuur_stand(app, &stand);
    Ok(stand)
}

/// Stopt het opnieuw uitschrijven: het verslag blijft zoals het was.
#[tauri::command]
pub(crate) fn vergadering_opnieuw_stoppen(app: AppHandle) -> VergaderingStand {
    let taak = slot(&app.state::<VergaderingStaat>().binnen).stop_opnieuw();
    if let Some(taak) = taak {
        annuleer_uitschrijven(&app, taak);
    }
    wijzig(&app, |_| {})
}

/// Zet de versie van voor het opnieuw uitschrijven terug en bewaart die (besluit 8). Kan tot het
/// verslag sluit.
#[tauri::command]
pub(crate) async fn vergadering_vorige_terugzetten(
    app: AppHandle,
) -> Result<VergaderingStand, Foutmelding> {
    blokkerend(move || {
        let staat = app.state::<VergaderingStaat>();
        let opdracht = slot(&staat.binnen).zet_vorige_terug()?;
        if let Some(opdracht) = opdracht {
            voer_uit(&app, &opdracht);
        }
        Ok(wijzig(&app, |_| {}))
    })
    .await
}

/// Bij het starten van de app: de lijst met opnames die nog wachten, en de bewaarde soort.
pub(crate) fn bij_start(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        let soort = lees_soort(&app);
        let dagen = crate::instellingen::lees_bewaar_dagen(&app);
        wijzig(&app, |b| {
            b.laatste_soort = soort;
            b.bewaar_dagen = dagen;
            werk_lijst_bij(b);
        });
        // Meteen bij het starten en daarna elk uur, buiten het zicht van een opname.
        loop {
            ruim_op(&app);
            thread::sleep(OPRUIMEN_ELK);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use mondig_core::gesprek::{Beurt, Spoorstand};

    /// De standaard bewaartermijn van 7 dagen.
    const WEEK: Duration = Duration::from_hours(7 * 24);

    fn gesprek(soort: Vergadersoort) -> Gesprek {
        Gesprek {
            beurten: vec![
                Beurt {
                    kant: Kant::Jij,
                    sprekernummer: None,
                    start_ms: 0,
                    eind_ms: 1000,
                    tekst: "Hallo.".to_string(),
                    naam: None,
                    verbeterd: false,
                },
                Beurt {
                    kant: Kant::Anderen,
                    sprekernummer: None,
                    start_ms: 2000,
                    eind_ms: 3000,
                    tekst: "Dag.".to_string(),
                    naam: Some("Wethouder".to_string()),
                    verbeterd: false,
                },
            ],
            duur: Duration::from_secs(3),
            meldingen: vec!["let op".to_string()],
            jij: Spoorstand::Opgenomen,
            anderen: Spoorstand::Opgenomen,
            weggelaten: 1,
            soort,
        }
    }

    #[test]
    fn de_soort_in_de_instelling_heen_en_terug() {
        for soort in [
            Vergadersoort::Online,
            Vergadersoort::Ruimte,
            Vergadersoort::Alleen,
        ] {
            assert_eq!(soort_uit_instelling(Some(soort_instelling(soort))), soort);
            // Dezelfde waarde als in opname.json.
            assert_eq!(
                serde_json::to_string(&soort).unwrap(),
                format!("\"{}\"", soort_instelling(soort))
            );
        }
        // Zonder instelling of met een onbekende waarde: online, zoals voorheen.
        assert_eq!(soort_uit_instelling(None), Vergadersoort::Online);
        assert_eq!(soort_uit_instelling(Some("iets")), Vergadersoort::Online);
    }

    #[test]
    fn bij_alleen_ik_en_in_de_ruimte_hebben_beurten_geen_spreker() {
        assert!(
            beurten_van(
                &gesprek(Vergadersoort::Online),
                &Markeringen::default(),
                &[]
            )
            .iter()
            .all(|b| b.spreker.is_some())
        );
        for soort in [Vergadersoort::Ruimte, Vergadersoort::Alleen] {
            assert!(
                beurten_van(&gesprek(soort), &Markeringen::default(), &[])
                    .iter()
                    .all(|b| b.spreker.is_none())
            );
        }
    }

    #[test]
    fn een_bewerkte_beurt_staat_in_de_stand_en_in_elke_export() {
        use mondig_core::bewaren::{Formaat, lokaal_moment, maak};
        let mut g = gesprek(Vergadersoort::Online);
        g.zet_tekst(
            1,
            "Goedemiddag allemaal.
Welkom.",
        )
        .unwrap();
        // Het scherm toont de nieuwe tekst en laat tijden en spreker ongemoeid.
        let beurt = &beurten_van(&g, &Markeringen::default(), &[])[1];
        assert_eq!(
            beurt.tekst,
            "Goedemiddag allemaal.
Welkom."
        );
        assert_eq!((beurt.start_ms, beurt.eind_ms), (2000, 3000));
        assert_eq!(beurt.spreker.as_deref(), Some("Wethouder"));
        // Elke export leest dezelfde tekst; een regeleinde wordt daar een spatie.
        let document = Document::van_gesprek(&g, "Parakeet", lokaal_moment(0));
        for formaat in [
            Formaat::Txt,
            Formaat::Md,
            Formaat::Html,
            Formaat::Srt,
            Formaat::Vtt,
        ] {
            let tekst = String::from_utf8(maak(&document, formaat, true).unwrap()).unwrap();
            assert!(tekst.contains("Goedemiddag allemaal. Welkom."), "{tekst}");
            assert!(!tekst.contains("Dag."), "{tekst}");
        }
        for formaat in [Formaat::Docx, Formaat::Odt, Formaat::Pdf] {
            assert!(!maak(&document, formaat, true).unwrap().is_empty());
        }
        // En Kopiëren.
        assert!(g.als_tekst().contains(
            "Wethouder: Goedemiddag allemaal.
Welkom."
        ));
    }

    #[test]
    fn het_verslag_met_vervangen_aan_geeft_plaatsvervangers_in_documenten_en_stand() {
        use mondig_core::bewaren::{Formaat, maak};
        let mut g = gesprek(Vergadersoort::Online);
        g.zet_tekst(1, "Mijn mail is jan@example.com, bel 06 12345678.")
            .unwrap();
        let staat = VergaderingStaat::default();
        slot(&staat.binnen).fase = Fase::Verslag(Box::new(Verslag {
            model: "Parakeet".to_string(),
            luistert: 0,
            ..Verslag::leeg(TaakId(3), PathBuf::from("vergadering-1"), g)
        }));
        let tekst = |staat: &VergaderingStaat| {
            let document = staat.document(TaakId(3)).unwrap();
            String::from_utf8(maak(&document, Formaat::Txt, true).unwrap()).unwrap()
        };
        assert!(tekst(&staat).contains("jan@example.com"));
        // De stand toont de markeringen (UTF-16) en de schakelaar.
        let json = serde_json::to_value(slot(&staat.binnen).stand()).unwrap();
        assert_eq!(
            json["toestand"]["beurten"][1]["markeringen"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(json["toestand"]["persoon"]["vervangen"], false);
        if let Fase::Verslag(v) = &mut slot(&staat.binnen).fase {
            v.persoon.zet_vervangen(true);
        }
        let uit = tekst(&staat);
        assert!(
            uit.contains("Mijn mail is [E-MAIL], bel [TELEFOON]."),
            "{uit}"
        );
        // De naam van een spreker blijft staan zolang hij niet gemarkeerd is...
        assert!(uit.contains("Wethouder"), "{uit}");
        // ...en wordt vervangen als de gebruiker hem markeert, in de export en bij Kopiëren.
        if let Fase::Verslag(v) = &mut slot(&staat.binnen).fase {
            v.persoon.markeer("Wethouder").unwrap();
        }
        let uit = tekst(&staat);
        assert!(uit.contains("[GEGEVEN]: Mijn mail is [E-MAIL]"), "{uit}");
        assert!(!uit.contains("Wethouder"), "{uit}");
        if let Fase::Verslag(v) = &slot(&staat.binnen).fase {
            let gekopieerd = v
                .gesprek
                .als_tekst_met(|t| v.persoon.voor_uitvoer(t), |n| v.persoon.voor_naam(n));
            assert!(
                gekopieerd.contains("[GEGEVEN]: Mijn mail is [E-MAIL]"),
                "{gekopieerd}"
            );
            assert!(!gekopieerd.contains("Wethouder"), "{gekopieerd}");
        }
    }

    #[test]
    fn sluiten_wacht_tot_een_stuk_geluid_klaar_is() {
        let mut binnen = Binnen {
            fase: Fase::Verslag(Box::new(Verslag {
                model: String::new(),
                luistert: 1,
                ..Verslag::leeg(
                    TaakId(3),
                    PathBuf::from("vergadering-1"),
                    gesprek(Vergadersoort::Online),
                )
            })),
            ..Binnen::default()
        };
        assert!(
            binnen.neem_verslag().is_err(),
            "er wordt nog geluid klaargezet"
        );
        assert!(
            matches!(binnen.fase, Fase::Verslag(_)),
            "het verslag blijft open"
        );
        if let Fase::Verslag(v) = &mut binnen.fase {
            v.luistert = 0;
        }
        assert!(binnen.neem_verslag().is_ok());
    }

    fn uitschrijven(taak: TaakId) -> Binnen {
        Binnen {
            fase: Fase::Uitschrijven(Uitschrijven {
                taak,
                map: PathBuf::from("vergadering-1"),
                soort: Vergadersoort::Online,
                begin_ms: 1_000,
                duur_ms: Some(3_000),
                voortgang: None,
                stap: Stap::Uitschrijven,
                model: "Canary".to_string(),
                model_id: "canary".to_string(),
            }),
            ..Binnen::default()
        }
    }

    #[test]
    fn uitschrijven_houdt_het_model_van_de_start_vast() {
        let binnen = uitschrijven(TaakId(1));
        assert!(binnen.schrijft_uit_met("canary"));
        // Na een modelwissel voor bestanden is dit een ander id: dat model mag wel weg.
        assert!(!binnen.schrijft_uit_met("parakeet"));
        assert!(!Binnen::default().schrijft_uit_met("canary"));
    }

    fn json(stand: &VergaderingStand) -> serde_json::Value {
        serde_json::to_value(stand).unwrap()
    }

    #[test]
    fn klaar_is_de_begintoestand() {
        let stand = Binnen::default().stand();
        let j = json(&stand);
        assert_eq!(j["toestand"]["type"], "klaar");
        assert_eq!(j["laatsteSoort"], "online");
        assert_eq!(j["opnames"], serde_json::json!([]));
        assert!(j["fout"].is_null());
    }

    #[test]
    fn opnemen_toont_soort_begin_en_de_stiltewaarschuwing() {
        let mut binnen = Binnen {
            fase: Fase::NeemtOp(Opnemen {
                map: PathBuf::from("vergadering-1"),
                soort: Vergadersoort::Online,
                begin_ms: 42,
                begin: Instant::now(),
                niveaus: Some(Niveaus {
                    microfoon: 0.5,
                    systeem: Some(0.0),
                }),
                systeem_stil: false,
                meldingen: Vec::new(),
            }),
            ..Binnen::default()
        };
        let j = json(&binnen.stand());
        assert_eq!(j["toestand"]["type"], "neemtOp");
        assert_eq!(j["toestand"]["soort"], "online");
        assert_eq!(j["toestand"]["beginMs"], 42);
        assert_eq!(j["toestand"]["niveaus"]["microfoon"], 0.5);
        assert!(j["toestand"]["systeemStil"].is_null());
        if let Fase::NeemtOp(o) = &mut binnen.fase {
            o.systeem_stil = true;
        }
        let j = json(&binnen.stand());
        assert_eq!(j["toestand"]["systeemStil"], MELDING_SYSTEEM_STIL_LIVE);
        assert_eq!(binnen.actieve_map(), Some(Path::new("vergadering-1")));
    }

    #[test]
    fn uitschrijven_volgt_de_voortgang_van_de_eigen_taak() {
        let taak = TaakId(7);
        let mut binnen = uitschrijven(taak);
        let voortgang = |taak, p| Gebeurtenis {
            taak,
            soort: mondig_core::taak::Soort::Vergadering,
            toestand: Toestand::verwerkt(p),
        };
        assert!(binnen.bij_gebeurtenis(&voortgang(taak, Some(34))));
        // Hetzelfde percentage nog eens: niets nieuws.
        assert!(!binnen.bij_gebeurtenis(&voortgang(taak, Some(34))));
        // Een andere taak telt niet.
        assert!(!binnen.bij_gebeurtenis(&voortgang(TaakId(8), Some(90))));
        let j = json(&binnen.stand());
        assert_eq!(j["toestand"]["type"], "uitschrijven");
        assert_eq!(j["toestand"]["voortgang"], 34);
        assert_eq!(j["toestand"]["taak"], 7);
        assert_eq!(j["toestand"]["model"], "Canary");
    }

    #[test]
    fn klaar_met_uitschrijven_geeft_het_verslag() {
        let taak = TaakId(7);
        let mut binnen = uitschrijven(taak);
        assert!(!binnen.bij_uitslag(TaakId(8), Gespreksuitslag::Geannuleerd));
        assert!(binnen.bij_uitslag(taak, Gespreksuitslag::Klaar(gesprek(Vergadersoort::Online))));
        let j = json(&binnen.stand());
        assert_eq!(j["toestand"]["type"], "verslag");
        assert_eq!(j["toestand"]["duurMs"], 3_000);
        assert_eq!(j["toestand"]["beginMs"], 1_000);
        assert_eq!(j["toestand"]["soevereiniteit"], "lokaal");
        assert_eq!(j["toestand"]["weggelaten"], 1);
        assert_eq!(j["toestand"]["meldingen"][0], "let op");
        let beurten = &j["toestand"]["beurten"];
        assert_eq!(beurten[0]["spreker"], "Jij");
        assert_eq!(beurten[0]["eigenNaam"], false);
        assert_eq!(beurten[1]["spreker"], "Wethouder");
        assert_eq!(beurten[1]["eigenNaam"], true);
        assert_eq!(beurten[1]["kant"], "anderen");
        assert_eq!(beurten[1]["index"], 1);
        // Het verslag is ook een document met de namen.
        let Fase::Verslag(v) = &mut binnen.fase else {
            return;
        };
        v.gesprek.geef_naam(0, "Zaal", Naambereik::Kant).unwrap();
        let doc = Document::van_gesprek(&v.gesprek, &v.model, bewaren::lokaal_moment(0));
        assert_eq!(doc.regels[0].spreker.as_deref(), Some("Zaal"));
    }

    #[test]
    fn in_de_ruimte_staan_er_geen_sprekers_in_de_stand() {
        let taak = TaakId(7);
        let mut binnen = uitschrijven(taak);
        assert!(binnen.bij_uitslag(taak, Gespreksuitslag::Klaar(gesprek(Vergadersoort::Ruimte))));
        let j = json(&binnen.stand());
        assert_eq!(j["toestand"]["soort"], "ruimte");
        assert!(j["toestand"]["beurten"][0]["spreker"].is_null());
    }

    #[test]
    fn een_fout_bij_het_uitschrijven_gaat_terug_naar_klaar_met_de_melding() {
        let taak = TaakId(7);
        let mut binnen = uitschrijven(taak);
        assert!(binnen.bij_gebeurtenis(&Gebeurtenis {
            taak,
            soort: mondig_core::taak::Soort::Vergadering,
            toestand: Toestand::fout("kapot", Vervolg::OpnieuwProberen),
        }));
        let j = json(&binnen.stand());
        assert_eq!(j["toestand"]["type"], "klaar");
        assert_eq!(j["fout"]["melding"], "kapot");
        // Na "Later uitschrijven" hoort een late uitslag nergens meer bij.
        let mut binnen = uitschrijven(taak);
        binnen.fase = Fase::Klaar;
        assert!(!binnen.bij_uitslag(taak, Gespreksuitslag::Klaar(gesprek(Vergadersoort::Online))));
        assert!(matches!(binnen.fase, Fase::Klaar));
    }

    fn schrijf_opname(basis: &Path, naam: &str, begin: u64, soort: &str, einde: bool) -> PathBuf {
        let map = basis.join(naam);
        fs::create_dir_all(map.join(MICROFOON)).unwrap();
        fs::write(
            map.join(OPZET),
            format!(
                "{{\"versie\":1,\"begin_unix_ms\":{begin},\"samplerate\":16000,\"stuk_seconden\":60,\"systeemgeluid\":false,\"soort\":\"{soort}\"}}"
            ),
        )
        .unwrap();
        // Twee seconden microfoon.
        fs::write(
            map.join(MICROFOON).join("00000.wav"),
            vec![0u8; 44 + 64_000],
        )
        .unwrap();
        if einde {
            fs::write(
                map.join(EINDE),
                r#"{"duur_ms":2500,"hersteld":false,"microfoon":{"stukken":1,"samples":32000,"verloren":0,"stil":false,"onderbroken":false},"systeem":null}"#,
            )
            .unwrap();
        }
        map
    }

    #[test]
    fn de_lijst_kent_afgesloten_en_afgebroken_opnames_maar_niet_de_actieve() {
        let basis = tempfile::tempdir().unwrap();
        schrijf_opname(basis.path(), "vergadering-2", 2_000, "ruimte", true);
        schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", false);
        let actief = schrijf_opname(basis.path(), "vergadering-3", 3_000, "online", true);
        // Een vreemde map telt niet mee.
        fs::create_dir_all(basis.path().join("iets-anders")).unwrap();
        let lijst = zoek_lijsten(basis.path(), Some(&actief), Some(WEEK), 5_000).opnames;
        assert_eq!(
            lijst,
            vec![
                OpnameInfo {
                    id: "vergadering-1".to_string(),
                    soort: Vergadersoort::Online,
                    begin_ms: 1_000,
                    duur_ms: Some(2_000),
                    afgesloten: false,
                    onzeker: false,
                    wist_ms: Some(1_000 + ms(WEEK) + DAG),
                },
                OpnameInfo {
                    id: "vergadering-2".to_string(),
                    soort: Vergadersoort::Ruimte,
                    begin_ms: 2_000,
                    duur_ms: Some(2_500),
                    afgesloten: true,
                    onzeker: false,
                    wist_ms: Some(2_000 + ms(WEEK) + DAG),
                },
            ]
        );
        assert_eq!(
            zoek_lijsten(basis.path(), None, Some(WEEK), 5_000)
                .opnames
                .len(),
            3
        );
    }

    const DAG: u64 = 86_400_000;

    fn in_gebruik(fase: Fase) -> Binnen {
        Binnen {
            fase,
            bewaar_dagen: Some(7),
            ..Binnen::default()
        }
    }

    #[test]
    fn opruimen_markeert_eerst_en_wist_een_dag_later() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        let oud = schrijf_opname(basis.path(), "vergadering-1", nu - 9 * DAG, "online", true);
        let jong = schrijf_opname(basis.path(), "vergadering-2", nu - 2 * DAG, "online", true);
        let mut binnen = in_gebruik(Fase::Klaar);
        // De eerste ronde markeert: er verandert iets in de lijst (de wistijd), maar niets is weg.
        assert!(ruim_op_in(&mut binnen, basis.path(), nu));
        assert!(oud.exists() && jong.exists());
        let wist: Vec<(&str, Option<u64>)> = binnen
            .opnames
            .iter()
            .map(|o| (o.id.as_str(), o.wist_ms))
            .collect();
        assert_eq!(
            wist,
            vec![
                ("vergadering-1", Some(nu + DAG)),
                ("vergadering-2", Some(nu + 5 * DAG + DAG)),
            ]
        );
        // Een uur later is er niets nieuws; een dag later gaat de gemarkeerde weg.
        assert!(!ruim_op_in(&mut binnen, basis.path(), nu + 3_600_000));
        assert!(ruim_op_in(&mut binnen, basis.path(), nu + DAG));
        assert!(!oud.exists());
        assert!(jong.exists());
        let ids: Vec<&str> = binnen.opnames.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["vergadering-2"]);
    }

    #[test]
    fn een_opname_in_elke_toestand_van_gebruik_blijft_staan() {
        type MaakFase = fn(PathBuf) -> Fase;
        let nu = 100 * DAG;
        let toestanden: Vec<(&str, MaakFase)> = vec![
            ("neemt op", |map| {
                Fase::NeemtOp(Opnemen {
                    map,
                    soort: Vergadersoort::Online,
                    begin_ms: 0,
                    begin: Instant::now(),
                    niveaus: None,
                    systeem_stil: false,
                    meldingen: Vec::new(),
                })
            }),
            ("voorbereiden", |map| {
                Fase::Voorbereiden(Voorbereiden {
                    map,
                    soort: Vergadersoort::Online,
                    begin_ms: 0,
                    duur_ms: None,
                    herstellen: false,
                })
            }),
            ("uitschrijven", |map| {
                Fase::Uitschrijven(Uitschrijven {
                    taak: TaakId(4),
                    map,
                    soort: Vergadersoort::Online,
                    begin_ms: 0,
                    duur_ms: None,
                    voortgang: None,
                    stap: Stap::Uitschrijven,
                    model: String::new(),
                    model_id: String::new(),
                })
            }),
            ("verslag", |map| {
                Fase::Verslag(Box::new(Verslag {
                    model: String::new(),
                    luistert: 0,
                    ..Verslag::leeg(TaakId(5), map, gesprek(Vergadersoort::Online))
                }))
            }),
        ];
        for (naam, maak) in toestanden {
            let basis = tempfile::tempdir().unwrap();
            let gebruikt = schrijf_opname(basis.path(), "vergadering-1", 0, "online", true);
            let mut binnen = in_gebruik(maak(gebruikt.clone()));
            // Ook jaren later, met alle tijd om te markeren en te wissen, blijft ze staan en
            // krijgt ze geen markering.
            for ronde in [nu, nu + 2 * DAG, nu + 10 * DAG] {
                assert!(!ruim_op_in(&mut binnen, basis.path(), ronde), "{naam}");
            }
            assert!(gebruikt.exists(), "{naam}");
            assert!(!gebruikt.join("verloopt.json").exists(), "{naam}");
            // Is de app er klaar mee, dan geldt gewoon de regel: eerst markeren, dan wissen.
            binnen.fase = Fase::Klaar;
            assert!(ruim_op_in(&mut binnen, basis.path(), nu), "{naam}");
            assert!(ruim_op_in(&mut binnen, basis.path(), nu + DAG), "{naam}");
            assert!(!gebruikt.exists(), "{naam}");
        }
    }

    #[test]
    fn zonder_bekende_bewaartermijn_wordt_er_niets_gedaan() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        let oud = schrijf_opname(basis.path(), "vergadering-1", 0, "online", true);
        // Onbekend (de instelling is onleesbaar): niet markeren, niet wissen, geen wistijd.
        let mut binnen = Binnen::default();
        assert_eq!(binnen.bewaar_dagen, None);
        for ronde in [nu, nu + 5 * DAG] {
            assert!(!ruim_op_in(&mut binnen, basis.path(), ronde));
        }
        assert!(oud.exists() && !oud.join("verloopt.json").exists());
        let lijst = zoek_lijsten(basis.path(), None, None, nu).opnames;
        assert_eq!(lijst[0].wist_ms, None);
        assert_eq!(
            serde_json::to_value(&lijst[0]).unwrap()["wistMs"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn de_wistijd_in_de_lijst_volgt_bewaartermijn_en_markering() {
        let basis = tempfile::tempdir().unwrap();
        schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let lijst = zoek_lijsten(basis.path(), None, Some(Duration::from_hours(24)), 5_000).opnames;
        // Verloopt na een dag, en wordt een dag later gewist.
        assert_eq!(lijst[0].wist_ms, Some(1_000 + 2 * DAG));
        let mut binnen = Binnen::default();
        assert_eq!(binnen.bewaartijd(), None);
        binnen.bewaar_dagen = Some(30);
        assert_eq!(binnen.bewaartijd(), Some(Duration::from_hours(30 * 24)));
        let json = serde_json::to_value(&lijst[0]).unwrap();
        assert_eq!(json["wistMs"], 1_000 + 2 * DAG);
    }

    #[test]
    fn een_opname_heet_alleen_bij_een_mapnaam_uit_de_lijst() {
        let binnen = Binnen {
            opnames: vec![OpnameInfo {
                id: "vergadering-1".to_string(),
                soort: Vergadersoort::Online,
                begin_ms: 1,
                duur_ms: None,
                afgesloten: true,
                onzeker: false,
                wist_ms: None,
            }],
            ..Binnen::default()
        };
        for slecht in [
            "",
            ".",
            "..",
            "../vergadering-1",
            "C:\\x",
            "a/b",
            "vergadering-2",
        ] {
            assert!(map_van(&binnen, slecht).is_err(), "{slecht}");
        }
        if vergadering::vergadermap().is_ok() {
            let (map, info) = map_van(&binnen, "vergadering-1").unwrap();
            assert!(map.ends_with("vergadering-1"));
            assert!(info.afgesloten);
        }
    }

    #[test]
    fn het_stipje_is_rood_en_rond_en_komt_rechtsonder_op_het_icoon() {
        let stip = rode_stip(16);
        assert_eq!(stip.len(), 16 * 16 * 4);
        // Het midden is vol rood, de hoek doorzichtig.
        let midden = ((8 * 16 + 8) * 4) as usize;
        assert_eq!(&stip[midden..midden + 4], &[0xE5, 0x48, 0x4D, 255]);
        assert_eq!(stip[3], 0);

        // Op een wit icoon van 32 bij 32: linksboven blijft wit, rechtsonder wordt rood.
        let wit = vec![255u8; 32 * 32 * 4];
        let uit = met_stip(&wit, 32, 32);
        assert_eq!(&uit[0..4], &[255, 255, 255, 255]);
        let plek = |x: usize, y: usize| (y * 32 + x) * 4;
        let i = plek(23, 23);
        assert_eq!(&uit[i..i + 3], &ROOD);
        // Buiten het stipje rechtsonder blijft het icoon zoals het was.
        let i = plek(31, 0);
        assert_eq!(&uit[i..i + 4], &[255, 255, 255, 255]);
        // Een doorzichtig icoon krijgt een zichtbaar stipje.
        let leeg = vec![0u8; 32 * 32 * 4];
        let uit = met_stip(&leeg, 32, 32);
        assert_eq!(uit[plek(23, 23) + 3], 255);
        // Een te kort buffer geeft geen paniek.
        let _ = met_stip(&[0u8; 8], 32, 32);
    }

    #[test]
    fn de_tooltip_toont_de_duur() {
        assert_eq!(
            tooltip(Vergadersoort::Online, Duration::from_secs(754)),
            "Mondig neemt een vergadering op (12:34)"
        );
        assert_eq!(
            tooltip(Vergadersoort::Online, Duration::from_secs(3723)),
            "Mondig neemt een vergadering op (1:02:03)"
        );
        assert_eq!(
            tooltip(Vergadersoort::Online, Duration::ZERO),
            "Mondig neemt een vergadering op (00:00)"
        );
        assert_eq!(
            tooltip(Vergadersoort::Ruimte, Duration::ZERO),
            "Mondig neemt een fysieke vergadering op (00:00)"
        );
        assert_eq!(
            tooltip(Vergadersoort::Alleen, Duration::from_secs(61)),
            "Mondig neemt een notitie op (01:01)"
        );
    }

    #[test]
    fn de_naam_van_de_wav_heeft_datum_en_tijd() {
        let naam = wav_naam(Vergadersoort::Online, 0);
        assert!(naam.starts_with("Vergadering 19"), "{naam}");
        assert!(wav_naam(Vergadersoort::Ruimte, 0).starts_with("Vergadering 19"));
        assert!(wav_naam(Vergadersoort::Alleen, 0).starts_with("Notitie 19"));
        assert!(
            std::path::Path::new(&naam)
                .extension()
                .is_some_and(|e| e == "wav")
        );
        assert!(!naam.contains(':'));
    }

    fn met_opname(id: &str, afgesloten: bool) -> Binnen {
        Binnen {
            opnames: vec![OpnameInfo {
                id: id.to_string(),
                soort: Vergadersoort::Online,
                begin_ms: 5,
                duur_ms: Some(7_200_000),
                afgesloten,
                onzeker: false,
                wist_ms: None,
            }],
            ..Binnen::default()
        }
    }

    #[test]
    fn starten_en_herstellen_tegelijk_slaagt_maar_een_keer() {
        if vergadering::vergadermap().is_err() {
            return;
        }
        for _ in 0..50 {
            let binnen = std::sync::Arc::new(Mutex::new(met_opname("vergadering-9", false)));
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let start = {
                let (binnen, barrier) = (binnen.clone(), barrier.clone());
                thread::spawn(move || {
                    barrier.wait();
                    slot(&binnen)
                        .claim(Fase::Starten {
                            soort: Vergadersoort::Online,
                        })
                        .is_ok()
                })
            };
            let herstel = {
                let (binnen, barrier) = (binnen.clone(), barrier.clone());
                thread::spawn(move || {
                    barrier.wait();
                    claim_uit_lijst(&mut slot(&binnen), "vergadering-9", true).is_ok()
                })
            };
            let (a, b) = (start.join().unwrap(), herstel.join().unwrap());
            assert!(a ^ b, "precies een van de twee: start {a}, herstel {b}");
        }
    }

    #[test]
    fn tijdens_herstellen_weigert_starten_en_een_tweede_herstel() {
        if vergadering::vergadermap().is_err() {
            return;
        }
        let mut binnen = met_opname("vergadering-9", false);
        let map = claim_uit_lijst(&mut binnen, "vergadering-9", true).unwrap();
        assert!(binnen.bereidt_voor(&map));
        assert!(
            binnen
                .claim(Fase::Starten {
                    soort: Vergadersoort::Online
                })
                .is_err()
        );
        // Nog een keer herstellen of uitschrijven: de opname is nu de actieve.
        assert!(claim_uit_lijst(&mut binnen, "vergadering-9", true).is_err());
        let j = json(&binnen.stand());
        assert_eq!(j["toestand"]["type"], "voorbereiden");
        assert_eq!(j["toestand"]["herstellen"], true);
        assert_eq!(j["toestand"]["duurMs"], 7_200_000);
        // Terug naar Klaar alleen voor deze opname.
        binnen.klaar_na_voorbereiden(Path::new("iets-anders"), None);
        assert!(binnen.bereidt_voor(&map));
        binnen.klaar_na_voorbereiden(&map, None);
        assert!(matches!(binnen.fase, Fase::Klaar));
    }

    #[test]
    fn uitschrijven_van_een_afgebroken_opname_vraagt_eerst_herstel() {
        if vergadering::vergadermap().is_err() {
            return;
        }
        let mut binnen = met_opname("vergadering-9", false);
        assert!(claim_uit_lijst(&mut binnen, "vergadering-9", false).is_err());
        assert!(matches!(binnen.fase, Fase::Klaar));
        let mut binnen = met_opname("vergadering-9", true);
        assert!(claim_uit_lijst(&mut binnen, "vergadering-9", false).is_ok());
    }

    #[test]
    fn de_actieve_opname_is_niet_via_haar_naam_te_pakken() {
        let mut binnen = met_opname("vergadering-1", true);
        binnen.fase = Fase::Voorbereiden(Voorbereiden {
            map: PathBuf::from("x").join("vergadering-1"),
            soort: Vergadersoort::Online,
            begin_ms: 0,
            duur_ms: None,
            herstellen: false,
        });
        assert!(map_van(&binnen, "vergadering-1").is_err());
    }

    /// Een kleine afgesloten opname met alleen een microfoonspoor: 160 samples van 100.
    fn nep_opname(map: &Path) {
        use mondig_core::spoor::stukken::{LOGBOEK, stuknaam};
        let spoor = map.join(MICROFOON);
        fs::create_dir_all(&spoor).unwrap();
        let samples = [100i16; 160];
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
        fs::write(spoor.join(stuknaam(0)), bytes).unwrap();
        fs::write(
            spoor.join(LOGBOEK),
            format!(
                "{{\"stuk\":0,\"bestand\":\"{}\",\"samples\":160,\"begin_ms\":0}}\n",
                stuknaam(0)
            ),
        )
        .unwrap();
        let einde = r#"{"stukken":1,"samples":160,"verloren":0,"stil":false,"onderbroken":false}"#;
        fs::write(
            map.join(EINDE),
            format!(r#"{{"duur_ms":10,"hersteld":false,"microfoon":{einde},"systeem":null}}"#),
        )
        .unwrap();
    }

    /// De bestandsnamen in een map, zonder verborgen (tijdelijke) bestanden, gesorteerd.
    fn namen_in(map: &Path) -> Vec<String> {
        let mut namen: Vec<String> = fs::read_dir(map)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        namen.sort();
        namen
    }

    #[test]
    fn de_opname_komt_als_wav_naast_het_verslag() {
        let opname = tempfile::tempdir().unwrap();
        nep_opname(opname.path());
        let doel = tempfile::tempdir().unwrap();
        let tekst = doel.path().join("2026-11-02 Vergadering 14.00.docx");
        fs::write(&tekst, b"tekst").unwrap();
        let naam = wav_naast(opname.path(), &tekst, false).unwrap();
        assert_eq!(naam, "2026-11-02 Vergadering 14.00.wav");
        let wav = fs::read(doel.path().join(&naam)).unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 320, "de kop en 160 samples");
        assert_eq!(
            namen_in(doel.path()),
            vec![
                "2026-11-02 Vergadering 14.00.docx".to_string(),
                "2026-11-02 Vergadering 14.00.wav".to_string()
            ],
            "geen tijdelijk bestand achtergebleven"
        );
        assert_eq!(fs::read(&tekst).unwrap(), b"tekst", "de tekst blijft");
    }

    #[test]
    fn een_bestaande_wav_wordt_nooit_stil_overschreven() {
        let opname = tempfile::tempdir().unwrap();
        nep_opname(opname.path());
        let doel = tempfile::tempdir().unwrap();
        let tekst = doel.path().join("Notitie.md");
        fs::write(&tekst, b"tekst").unwrap();
        fs::write(doel.path().join("Notitie.wav"), b"oud").unwrap();
        assert_eq!(
            wav_naast(opname.path(), &tekst, false).unwrap(),
            "Notitie (2).wav"
        );
        assert_eq!(fs::read(doel.path().join("Notitie.wav")).unwrap(), b"oud");
        // Koos de gebruiker bij de tekst Vervangen, dan ook bij de WAV.
        assert_eq!(
            wav_naast(opname.path(), &tekst, true).unwrap(),
            "Notitie.wav"
        );
        assert_ne!(fs::read(doel.path().join("Notitie.wav")).unwrap(), b"oud");
    }

    #[test]
    fn een_mislukte_menging_laat_geen_half_bestand_achter() {
        let opname = tempfile::tempdir().unwrap();
        // Geen einde.json: de opname is niet afgesloten.
        let doel = tempfile::tempdir().unwrap();
        let tekst = doel.path().join("Vergadering.docx");
        fs::write(&tekst, b"tekst").unwrap();
        assert!(wav_naast(opname.path(), &tekst, false).is_err());
        assert_eq!(namen_in(doel.path()), vec!["Vergadering.docx".to_string()]);
    }

    #[test]
    fn sluiten_wacht_tot_de_audio_bewaard_is() {
        let taak = TaakId(7);
        let mut binnen = uitschrijven(taak);
        assert!(binnen.bij_uitslag(taak, Gespreksuitslag::Klaar(gesprek(Vergadersoort::Online))));
        if let Fase::Verslag(v) = &mut binnen.fase {
            v.audio_bezig = true;
        }
        assert!(binnen.neem_verslag().is_err());
        assert!(
            matches!(binnen.fase, Fase::Verslag(_)),
            "het verslag blijft"
        );
        if let Fase::Verslag(v) = &mut binnen.fase {
            v.audio_bezig = false;
        }
        assert_eq!(
            binnen.neem_verslag().unwrap(),
            PathBuf::from("vergadering-1")
        );
        assert!(matches!(binnen.fase, Fase::Klaar));
        assert!(binnen.neem_verslag().is_err());
    }

    #[test]
    fn annuleren_buiten_later_om_is_later_uitschrijven() {
        let taak = TaakId(7);
        let mut binnen = uitschrijven(taak);
        assert!(binnen.bij_gebeurtenis(&Gebeurtenis {
            taak,
            soort: mondig_core::taak::Soort::Vergadering,
            toestand: Toestand::Geannuleerd,
        }));
        assert!(matches!(binnen.fase, Fase::Klaar));
        assert!(binnen.fout.is_none());
        // En via de uitslag (als de gebeurtenis er niet was).
        let mut binnen = uitschrijven(taak);
        assert!(binnen.bij_uitslag(taak, Gespreksuitslag::Geannuleerd));
        assert!(matches!(binnen.fase, Fase::Klaar));
    }

    // ---- Verslagen bewaren en Eerder opgenomen (ADR 0022) ----

    /// Wijzigt het open verslag en schrijft meteen, zoals `wijzig_open` maar zonder app.
    fn wijzig_nu(
        binnen: &mut Binnen,
        wijziging: impl FnOnce(&mut Verslag) -> Result<(), Foutmelding>,
    ) -> Result<(), Foutmelding> {
        if let Some(opdracht) = wijzig_open_in(binnen, wijziging)? {
            let uitslag = schrijf_opdracht(&Mutex::new(HashMap::new()), &opdracht);
            binnen.na_schrijven(&opdracht, uitslag);
        }
        Ok(())
    }

    /// Een uitschrijvende vergadering in `map`.
    fn uitschrijven_in(taak: TaakId, map: &Path) -> Binnen {
        let mut binnen = uitschrijven(taak);
        if let Fase::Uitschrijven(u) = &mut binnen.fase {
            u.map = map.to_path_buf();
        }
        binnen.bewaar_dagen = Some(7);
        binnen
    }

    /// Een map met opname en verslag in `basis`, zoals het uitschrijven haar achterlaat.
    fn bewaarde_vergadering(basis: &Path, naam: &str, begin: u64, soort: Vergadersoort) -> PathBuf {
        let map = schrijf_opname(basis, naam, begin, soort_instelling(soort), true);
        verslagbestand::schrijf(
            &map,
            &Bewaren::nieuw(
                &gesprek(soort),
                "canary",
                "Canary",
                None,
                &Markeringen::default(),
                None,
            ),
        )
        .unwrap();
        map
    }

    fn open_verslag_van(binnen: &Binnen) -> &Verslag {
        match &binnen.fase {
            Fase::Verslag(v) => Some(v),
            _ => None,
        }
        .expect("er is een verslag open")
    }

    #[test]
    fn het_verslag_wordt_bewaard_zodra_het_uitschrijven_klaar_is() {
        let basis = tempfile::tempdir().unwrap();
        let map = schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let taak = TaakId(7);
        let mut binnen = uitschrijven_in(taak, &map);
        assert!(!map.join(verslagbestand::BESTAND).exists());
        assert!(binnen.bij_uitslag(taak, Gespreksuitslag::Klaar(gesprek(Vergadersoort::Online))));
        let bewaard = verslagbestand::lees(&map).expect("verslag.json staat er");
        assert!(bewaard.gesprek == gesprek(Vergadersoort::Online));
        assert_eq!(
            (bewaard.model.as_str(), bewaard.model_id.as_str()),
            ("Canary", "canary")
        );
        assert_eq!(bewaard.naam, None);
        assert!(binnen.fout.is_none());
    }

    #[test]
    fn elke_wijziging_gaat_naar_schijf() {
        let basis = tempfile::tempdir().unwrap();
        let map = schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let taak = TaakId(7);
        let mut binnen = uitschrijven_in(taak, &map);
        assert!(binnen.bij_uitslag(taak, Gespreksuitslag::Klaar(gesprek(Vergadersoort::Online))));
        // De tekst van een beurt.
        wijzig_nu(&mut binnen, |v| {
            v.gesprek
                .zet_tekst(1, "Goedemiddag, mijn mail is jan@example.com.")
                .map(|_| ())
        })
        .unwrap();
        // Een naam voor een spreker.
        wijzig_nu(&mut binnen, |v| {
            v.gesprek.geef_naam(0, "Zaal", Naambereik::Kant).map(|_| ())
        })
        .unwrap();
        // Een markering en Vervangen.
        wijzig_nu(&mut binnen, |v| {
            v.persoon.zet_vervangen(true);
            v.persoon.markeer("Wethouder").map_err(markeerfout)
        })
        .unwrap();
        // De naam van het verslag, schoon en begrensd.
        wijzig_nu(&mut binnen, |v| {
            v.naam = schone_verslagnaam("  Overleg   speelveld ");
            Ok(())
        })
        .unwrap();
        let bewaard = verslagbestand::lees(&map).unwrap();
        let open = open_verslag_van(&binnen);
        assert!(bewaard.gesprek == open.gesprek);
        assert!(bewaard.gesprek.beurten[1].tekst.starts_with("Goedemiddag"));
        assert_eq!(bewaard.gesprek.beurten[0].naam.as_deref(), Some("Zaal"));
        assert!(bewaard.markeringen.vervangen());
        assert!(bewaard.markeringen == open.persoon);
        assert_eq!(bewaard.naam.as_deref(), Some("Overleg speelveld"));
        // Een wijziging die mislukt, bewaart niets nieuws.
        let voor = fs::read(map.join(verslagbestand::BESTAND)).unwrap();
        assert!(wijzig_nu(&mut binnen, |v| v.gesprek.zet_tekst(99, "x").map(|_| ())).is_err());
        assert_eq!(fs::read(map.join(verslagbestand::BESTAND)).unwrap(), voor);
        // En zonder open verslag is er niets te wijzigen.
        assert!(wijzig_nu(&mut Binnen::default(), |_| Ok(())).is_err());
    }

    #[test]
    fn een_schrijffout_breekt_het_verslag_niet_en_de_volgende_wijziging_probeert_opnieuw() {
        let basis = tempfile::tempdir().unwrap();
        // De map bestaat niet, dus schrijven mislukt.
        let map = basis.path().join("vergadering-1");
        let taak = TaakId(7);
        let mut binnen = uitschrijven_in(taak, &map);
        assert!(binnen.bij_uitslag(taak, Gespreksuitslag::Klaar(gesprek(Vergadersoort::Online))));
        assert!(open_verslag_van(&binnen).bewaren_mislukt);
        let melding = binnen.fout.clone().expect("een melding in gewone taal");
        assert!(
            melding.melding.contains("Mondig kon het verslag"),
            "{}",
            melding.melding
        );
        // De tekst blijft open en te wijzigen.
        wijzig_nu(&mut binnen, |v| {
            v.gesprek.zet_tekst(0, "Anders.").map(|_| ())
        })
        .unwrap();
        assert!(open_verslag_van(&binnen).bewaren_mislukt);
        let j = json(&binnen.stand());
        assert_eq!(j["toestand"]["beurten"][0]["tekst"], "Anders.");
        assert!(j["fout"]["melding"].is_string());
        // De schijf is er weer: de volgende wijziging lukt en haalt de melding weg.
        fs::create_dir_all(&map).unwrap();
        wijzig_nu(&mut binnen, |v| {
            v.gesprek.zet_tekst(0, "Weer anders.").map(|_| ())
        })
        .unwrap();
        assert!(!open_verslag_van(&binnen).bewaren_mislukt);
        assert!(binnen.fout.is_none());
        let tekst = verslagbestand::lees(&map).unwrap().gesprek.beurten[0]
            .tekst
            .clone();
        assert_eq!(tekst, "Weer anders.");
    }

    #[test]
    fn sluiten_wist_niets_en_het_verslag_staat_dan_in_eerder_opgenomen() {
        let basis = tempfile::tempdir().unwrap();
        let map = schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let taak = TaakId(7);
        let mut binnen = uitschrijven_in(taak, &map);
        assert!(binnen.bij_uitslag(taak, Gespreksuitslag::Klaar(gesprek(Vergadersoort::Online))));
        // Het open verslag staat nog niet in de lijst.
        binnen.lees_lijsten(basis.path(), 5_000);
        assert!(binnen.verslagen.is_empty());
        assert_eq!(binnen.sluit_verslag(false).unwrap(), taak);
        assert!(matches!(binnen.fase, Fase::Klaar));
        assert!(map.exists() && map.join(verslagbestand::BESTAND).is_file());
        assert!(map.join(EINDE).is_file(), "het geluid blijft staan");
        binnen.lees_lijsten(basis.path(), 5_000);
        let ids: Vec<&str> = binnen.verslagen.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, vec!["vergadering-1"]);
        assert!(
            binnen.opnames.is_empty(),
            "het is geen opname meer die wacht"
        );
        // Sluiten zonder verslag kan niet.
        assert!(binnen.sluit_verslag(false).is_err());
    }

    #[test]
    fn sluiten_weigert_zolang_er_geluid_wordt_klaargezet() {
        let basis = tempfile::tempdir().unwrap();
        let map = bewaarde_vergadering(basis.path(), "vergadering-1", 1_000, Vergadersoort::Online);
        let mut binnen = in_gebruik(Fase::Klaar);
        binnen.lees_lijsten(basis.path(), 5_000);
        binnen
            .open_verslag(basis.path(), "vergadering-1", false)
            .unwrap();
        if let Fase::Verslag(v) = &mut binnen.fase {
            v.luistert = 1;
        }
        assert!(binnen.sluit_verslag(false).is_err());
        assert!(matches!(binnen.fase, Fase::Verslag(_)));
        assert!(map.exists());
    }

    #[test]
    fn de_lijst_splitst_verslagen_van_opnames_die_nog_wachten() {
        let basis = tempfile::tempdir().unwrap();
        bewaarde_vergadering(basis.path(), "vergadering-1", 1_000, Vergadersoort::Online);
        bewaarde_vergadering(basis.path(), "vergadering-3", 3_000, Vergadersoort::Alleen);
        let spreker =
            bewaarde_vergadering(basis.path(), "vergadering-2", 2_000, Vergadersoort::Ruimte);
        let mut g = gesprek(Vergadersoort::Ruimte);
        for (b, nummer) in g.beurten.iter_mut().zip([1u8, 2]) {
            b.sprekernummer = Some(nummer);
        }
        verslagbestand::schrijf(
            &spreker,
            &Bewaren::nieuw(
                &g,
                "canary",
                "Canary",
                Some("Teamoverleg"),
                &Markeringen::default(),
                None,
            ),
        )
        .unwrap();
        // Een opname zonder verslag, een afgebroken opname en een actieve map.
        schrijf_opname(basis.path(), "vergadering-4", 4_000, "online", true);
        schrijf_opname(basis.path(), "vergadering-5", 5_000, "online", false);
        let actief =
            bewaarde_vergadering(basis.path(), "vergadering-6", 6_000, Vergadersoort::Online);
        let lijsten = zoek_lijsten(basis.path(), Some(&actief), Some(WEEK), 10_000);
        // Nieuwste eerst.
        let verslagen: Vec<(&str, Option<&str>, Vergadersoort, Option<usize>)> = lijsten
            .verslagen
            .iter()
            .map(|v| (v.id.as_str(), v.naam.as_deref(), v.soort, v.sprekers))
            .collect();
        assert_eq!(
            verslagen,
            vec![
                ("vergadering-3", None, Vergadersoort::Alleen, None),
                (
                    "vergadering-2",
                    Some("Teamoverleg"),
                    Vergadersoort::Ruimte,
                    Some(2)
                ),
                ("vergadering-1", None, Vergadersoort::Online, None),
            ]
        );
        assert_eq!(lijsten.verslagen[0].duur_ms, 3_000);
        assert_eq!(lijsten.verslagen[0].begin_ms, 3_000);
        assert_eq!(lijsten.verslagen[0].wist_ms, Some(3_000 + ms(WEEK) + DAG));
        // De opnames die wachten, oudste eerst.
        let ids: Vec<&str> = lijsten.opnames.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, vec!["vergadering-4", "vergadering-5"]);
        // In de stand heten de velden zoals de interface ze verwacht.
        let j = serde_json::to_value(&lijsten.verslagen[1]).unwrap();
        assert_eq!(j["naam"], "Teamoverleg");
        assert_eq!(j["sprekers"], 2);
        assert_eq!(j["beginMs"], 2_000);
        assert_eq!(j["wistMs"], 2_000 + ms(WEEK) + DAG);
    }

    #[test]
    fn een_onleesbaar_verslag_valt_terug_op_nog_uit_te_schrijven() {
        let basis = tempfile::tempdir().unwrap();
        let kapot =
            bewaarde_vergadering(basis.path(), "vergadering-1", 1_000, Vergadersoort::Online);
        let nieuw =
            bewaarde_vergadering(basis.path(), "vergadering-2", 2_000, Vergadersoort::Online);
        fs::write(kapot.join(verslagbestand::BESTAND), "{ kapot").unwrap();
        // Een verslag van een nieuwere versie dan deze code kent, telt ook niet.
        let json = fs::read_to_string(nieuw.join(verslagbestand::BESTAND)).unwrap();
        fs::write(
            nieuw.join(verslagbestand::BESTAND),
            json.replacen("\"versie\":1", "\"versie\":99", 1),
        )
        .unwrap();
        let lijsten = zoek_lijsten(basis.path(), None, Some(WEEK), 10_000);
        assert!(lijsten.verslagen.is_empty());
        let opnames: Vec<(&str, bool)> = lijsten
            .opnames
            .iter()
            .map(|o| (o.id.as_str(), o.afgesloten))
            .collect();
        assert_eq!(
            opnames,
            vec![("vergadering-1", true), ("vergadering-2", true)]
        );
    }

    #[test]
    fn een_verslag_dat_tussen_lijst_en_openen_kapotging_verdwijnt_uit_de_lijst() {
        let basis = tempfile::tempdir().unwrap();
        let map = bewaarde_vergadering(basis.path(), "vergadering-1", 1_000, Vergadersoort::Online);
        let mut binnen = in_gebruik(Fase::Klaar);
        binnen.lees_lijsten(basis.path(), 5_000);
        assert_eq!(binnen.verslagen.len(), 1);
        fs::write(map.join(verslagbestand::BESTAND), "geen json").unwrap();
        let fout = binnen
            .open_verslag(basis.path(), "vergadering-1", false)
            .unwrap_err();
        assert!(fout.melding.contains("niet te lezen"), "{}", fout.melding);
        assert!(matches!(binnen.fase, Fase::Klaar));
        assert!(binnen.verslagen.is_empty());
        assert_eq!(
            binnen.opnames.len(),
            1,
            "de opname wacht weer op uitschrijven"
        );
        assert!(map.exists());
    }

    #[test]
    fn heropenen_geeft_alles_terug_zoals_het_was() {
        const TEKST: &str = "Mijn mail is jan@example.com en ik ben Jan de Vries.";
        let basis = tempfile::tempdir().unwrap();
        let map = schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let taak = TaakId(7);
        let mut binnen = uitschrijven_in(taak, &map);
        assert!(binnen.bij_uitslag(taak, Gespreksuitslag::Klaar(gesprek(Vergadersoort::Online))));
        wijzig_nu(&mut binnen, |v| {
            v.gesprek.zet_tekst(0, TEKST).map(|_| ())?;
            v.gesprek
                .geef_naam(0, "Zaal", Naambereik::Kant)
                .map(|_| ())?;
            v.persoon.zet_vervangen(true);
            v.persoon.markeer("Jan de Vries").map_err(markeerfout)?;
            v.persoon.haal_weg("wethouder");
            v.naam = schone_verslagnaam("Overleg buurtbijeenkomst");
            Ok(())
        })
        .unwrap();
        let voor = json(&binnen.stand());
        let tekst_voor = open_verslag_van(&binnen).persoon.voor_uitvoer(TEKST);
        assert!(
            tekst_voor.contains("[E-MAIL]") && tekst_voor.contains("[GEGEVEN]"),
            "{tekst_voor}"
        );
        binnen.sluit_verslag(false).unwrap();
        binnen.lees_lijsten(basis.path(), 5_000);
        assert!(
            binnen
                .open_verslag(basis.path(), "vergadering-1", false)
                .unwrap()
                .is_none()
        );
        let na = json(&binnen.stand());
        for sleutel in [
            "beurten",
            "persoon",
            "naam",
            "model",
            "soort",
            "beginMs",
            "duurMs",
            "meldingen",
            "weggelaten",
        ] {
            assert_eq!(
                na["toestand"][sleutel], voor["toestand"][sleutel],
                "{sleutel}"
            );
        }
        assert_eq!(na["toestand"]["naam"], "Overleg buurtbijeenkomst");
        assert_eq!(na["toestand"]["persoon"]["vervangen"], true);
        assert_eq!(na["toestand"]["beurten"][0]["spreker"], "Zaal");
        // Het open verslag staat niet meer in de lijst, en heeft een nieuw taak-id.
        assert!(binnen.verslagen.is_empty());
        let v = open_verslag_van(&binnen);
        assert_ne!(v.taak, taak);
        assert_eq!(v.persoon.voor_uitvoer(TEKST), tekst_voor);
        // De eigen naam is de titel bij Opslaan en Kopiëren.
        let taak_nieuw = v.taak;
        let staat = VergaderingStaat::default();
        *slot(&staat.binnen) = binnen;
        let document = staat.document(taak_nieuw).unwrap();
        assert_eq!(document.titel, "Overleg buurtbijeenkomst");
        assert!(staat.document(TaakId(123_456)).is_none());
    }

    #[test]
    fn openen_weigert_tijdens_een_opname_en_laat_alles_zoals_het_was() {
        let basis = tempfile::tempdir().unwrap();
        bewaarde_vergadering(basis.path(), "vergadering-2", 1_000, Vergadersoort::Online);
        let weigert: Vec<(&str, Fase)> = vec![
            (
                "starten",
                Fase::Starten {
                    soort: Vergadersoort::Online,
                },
            ),
            (
                "neemt op",
                Fase::NeemtOp(Opnemen {
                    map: PathBuf::from("vergadering-9"),
                    soort: Vergadersoort::Online,
                    begin_ms: 0,
                    begin: Instant::now(),
                    niveaus: None,
                    systeem_stil: false,
                    meldingen: Vec::new(),
                }),
            ),
            (
                "voorbereiden",
                Fase::Voorbereiden(Voorbereiden {
                    map: PathBuf::from("vergadering-9"),
                    soort: Vergadersoort::Online,
                    begin_ms: 0,
                    duur_ms: None,
                    herstellen: false,
                }),
            ),
            ("uitschrijven", uitschrijven(TaakId(4)).fase),
        ];
        for (naam, fase) in weigert {
            let mut binnen = in_gebruik(fase);
            binnen.lees_lijsten(basis.path(), 5_000);
            assert_eq!(binnen.verslagen.len(), 1, "{naam}");
            let fout = binnen
                .open_verslag(basis.path(), "vergadering-2", false)
                .unwrap_err();
            assert!(!fout.melding.is_empty(), "{naam}");
            assert!(
                !matches!(binnen.fase, Fase::Klaar | Fase::Verslag(_)),
                "{naam}"
            );
            assert_eq!(binnen.verslagen.len(), 1, "{naam}: de lijst blijft");
        }
    }

    #[test]
    fn openen_controleert_het_id_zoals_bij_opnames() {
        let basis = tempfile::tempdir().unwrap();
        bewaarde_vergadering(basis.path(), "vergadering-1", 1_000, Vergadersoort::Online);
        let mut binnen = in_gebruik(Fase::Klaar);
        binnen.lees_lijsten(basis.path(), 5_000);
        for slecht in [
            "",
            ".",
            "..",
            "../vergadering-1",
            "C:\\x",
            "a/b",
            "vergadering-2",
        ] {
            assert!(
                binnen.open_verslag(basis.path(), slecht, false).is_err(),
                "{slecht}"
            );
            assert!(matches!(binnen.fase, Fase::Klaar), "{slecht}");
        }
        // Een opname die nog wacht, is geen verslag.
        schrijf_opname(basis.path(), "vergadering-2", 2_000, "online", true);
        binnen.lees_lijsten(basis.path(), 5_000);
        assert!(
            binnen
                .open_verslag(basis.path(), "vergadering-2", false)
                .is_err()
        );
        assert!(
            binnen
                .open_verslag(basis.path(), "vergadering-1", false)
                .is_ok()
        );
    }

    #[test]
    fn een_ander_verslag_openen_sluit_het_huidige_zonder_te_wissen() {
        let basis = tempfile::tempdir().unwrap();
        let eerste =
            bewaarde_vergadering(basis.path(), "vergadering-1", 1_000, Vergadersoort::Online);
        let tweede =
            bewaarde_vergadering(basis.path(), "vergadering-2", 2_000, Vergadersoort::Ruimte);
        let mut binnen = in_gebruik(Fase::Klaar);
        binnen.lees_lijsten(basis.path(), 5_000);
        assert!(
            binnen
                .open_verslag(basis.path(), "vergadering-1", false)
                .unwrap()
                .is_none()
        );
        let taak_eerste = open_verslag_van(&binnen).taak;
        // Hetzelfde verslag nog eens: niets te doen.
        assert!(
            binnen
                .open_verslag(basis.path(), "vergadering-1", false)
                .unwrap()
                .is_none()
        );
        assert_eq!(open_verslag_van(&binnen).taak, taak_eerste);
        // Een ander: het huidige gaat dicht en komt terug in de lijst.
        assert_eq!(
            binnen
                .open_verslag(basis.path(), "vergadering-2", false)
                .unwrap(),
            Some(taak_eerste)
        );
        assert_eq!(
            open_verslag_van(&binnen).gesprek.soort,
            Vergadersoort::Ruimte
        );
        let ids: Vec<&str> = binnen.verslagen.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, vec!["vergadering-1"]);
        assert!(eerste.exists() && tweede.exists());
        // Weigert het huidige te sluiten, dan blijft alles zoals het was.
        if let Fase::Verslag(v) = &mut binnen.fase {
            v.audio_bezig = true;
        }
        assert!(
            binnen
                .open_verslag(basis.path(), "vergadering-1", false)
                .is_err()
        );
        assert_eq!(
            open_verslag_van(&binnen).gesprek.soort,
            Vergadersoort::Ruimte
        );
    }

    #[test]
    fn verslag_wissen_haalt_de_hele_map_weg() {
        let basis = tempfile::tempdir().unwrap();
        let in_lijst =
            bewaarde_vergadering(basis.path(), "vergadering-1", 1_000, Vergadersoort::Online);
        let open =
            bewaarde_vergadering(basis.path(), "vergadering-2", 2_000, Vergadersoort::Online);
        let mut binnen = in_gebruik(Fase::Klaar);
        binnen.lees_lijsten(basis.path(), 5_000);
        // Uit de lijst: de map van het verslag, controle op het id inbegrepen.
        assert!(verslag_van(&binnen, basis.path(), "../vergadering-1").is_err());
        let (map, info) = verslag_van(&binnen, basis.path(), "vergadering-1").unwrap();
        assert_eq!(map, in_lijst);
        assert_eq!(info.id, "vergadering-1");
        wis_met_geduld(&map, false).unwrap();
        assert!(
            !in_lijst.exists(),
            "de map met het geluid en het verslag is weg"
        );
        // Het open verslag: dezelfde weg als sluiten, maar dan wissen.
        binnen.lees_lijsten(basis.path(), 5_000);
        binnen
            .open_verslag(basis.path(), "vergadering-2", false)
            .unwrap();
        let map = binnen.neem_verslag().unwrap();
        wis_met_geduld(&map, false).unwrap();
        assert!(!open.exists());
        binnen.lees_lijsten(basis.path(), 5_000);
        assert!(binnen.verslagen.is_empty() && binnen.opnames.is_empty());
        // Het verslag wissen weigert zolang er geluid wordt gemengd.
        let map = bewaarde_vergadering(basis.path(), "vergadering-3", 3_000, Vergadersoort::Online);
        binnen.lees_lijsten(basis.path(), 5_000);
        binnen
            .open_verslag(basis.path(), "vergadering-3", false)
            .unwrap();
        if let Fase::Verslag(v) = &mut binnen.fase {
            v.audio_bezig = true;
        }
        assert!(binnen.neem_verslag().is_err());
        assert!(map.exists());
    }

    #[test]
    fn de_stand_van_het_verslag_meldt_de_naam_en_de_lijst_de_verslagen() {
        let basis = tempfile::tempdir().unwrap();
        bewaarde_vergadering(basis.path(), "vergadering-1", 1_000, Vergadersoort::Online);
        let mut binnen = in_gebruik(Fase::Klaar);
        binnen.lees_lijsten(basis.path(), 5_000);
        let j = json(&binnen.stand());
        assert_eq!(j["verslagen"][0]["id"], "vergadering-1");
        assert!(j["opnames"].as_array().unwrap().is_empty());
        binnen
            .open_verslag(basis.path(), "vergadering-1", false)
            .unwrap();
        let j = json(&binnen.stand());
        assert_eq!(j["toestand"]["type"], "verslag");
        assert!(j["toestand"]["naam"].is_null());
        // De map staat in de echte vergadermap niet, dus de wistijd is hier onbekend.
        assert!(j["toestand"]["wistMs"].is_null() || j["toestand"]["wistMs"].is_u64());
        assert!(
            j["verslagen"].as_array().unwrap().is_empty(),
            "het open verslag staat er niet in"
        );
    }

    // ---- Reparaties na de review ----

    #[test]
    fn een_eigen_naam_met_een_gemarkeerd_gegeven_wordt_overal_vervangen() {
        let staat = VergaderingStaat::default();
        let mut binnen = in_gebruik(Fase::Klaar);
        let mut v = Verslag::leeg(
            TaakId(5),
            PathBuf::from("vergadering-1"),
            gesprek(Vergadersoort::Online),
        );
        v.model = "Parakeet".to_string();
        v.naam = schone_verslagnaam("Overleg met Jan de Vries");
        v.persoon.markeer("Jan de Vries").unwrap();
        v.persoon.zet_vervangen(true);
        binnen.fase = Fase::Verslag(Box::new(v));
        *slot(&staat.binnen) = binnen;
        let document = staat.document(TaakId(5)).unwrap();
        assert_eq!(document.titel, "Overleg met [GEGEVEN]");
        // Daarmee ook de bestandsnaam en de naam van de WAV ernaast.
        let naam = crate::bewaren::standaardnaam(&document, Formaat::Docx);
        assert!(!naam.contains("Jan"), "{naam}");
        assert!(naam.contains("GEGEVEN"), "{naam}");
        assert!(!crate::bewaren::als_wav(&naam).contains("Jan"));
        // Zonder Vervangen blijft de naam zoals de gebruiker hem gaf.
        if let Fase::Verslag(v) = &mut slot(&staat.binnen).fase {
            v.persoon.zet_vervangen(false);
        }
        assert_eq!(
            staat.document(TaakId(5)).unwrap().titel,
            "Overleg met Jan de Vries"
        );
    }

    /// Een open verslag waarvan het schrijven mislukte (de map bestaat niet), en een ander verslag
    /// in de lijst.
    fn mislukt_verslag(basis: &Path) -> Binnen {
        bewaarde_vergadering(basis, "vergadering-2", 2_000, Vergadersoort::Online);
        let map = basis.join("vergadering-1");
        let taak = TaakId(7);
        let mut binnen = uitschrijven_in(taak, &map);
        assert!(binnen.bij_uitslag(taak, Gespreksuitslag::Klaar(gesprek(Vergadersoort::Online))));
        assert!(open_verslag_van(&binnen).bewaren_mislukt);
        binnen.lees_lijsten(basis, 5_000);
        binnen
    }

    #[test]
    fn sluiten_weigert_een_verslag_dat_niet_bewaard_is_tenzij_de_gebruiker_toch_sluit() {
        let basis = tempfile::tempdir().unwrap();
        let mut binnen = mislukt_verslag(basis.path());
        let fout = binnen.sluit_verslag(false).unwrap_err();
        assert!(
            fout.melding.contains("laatste wijzigingen niet bewaren"),
            "{}",
            fout.melding
        );
        assert!(matches!(binnen.fase, Fase::Verslag(_)), "het blijft open");
        assert!(binnen.fout.is_some(), "de waarschuwing blijft");
        assert_eq!(json(&binnen.stand())["toestand"]["bewarenMislukt"], true);
        // Toch sluiten kan, en de waarschuwing verdwijnt dan niet stil.
        binnen.sluit_verslag(true).unwrap();
        assert!(matches!(binnen.fase, Fase::Klaar));
        assert!(binnen.fout.is_some());
    }

    #[test]
    fn een_ander_verslag_openen_weigert_ook_bij_een_niet_bewaard_verslag() {
        let basis = tempfile::tempdir().unwrap();
        let mut binnen = mislukt_verslag(basis.path());
        assert!(
            binnen
                .open_verslag(basis.path(), "vergadering-2", false)
                .is_err()
        );
        assert_eq!(open_verslag_van(&binnen).taak, TaakId(7));
        assert!(binnen.fout.is_some());
        // Toch: het andere verslag opent, en de melding over het eerste blijft staan.
        assert_eq!(
            binnen
                .open_verslag(basis.path(), "vergadering-2", true)
                .unwrap(),
            Some(TaakId(7))
        );
        assert_eq!(
            open_verslag_van(&binnen).gesprek.soort,
            Vergadersoort::Online
        );
        assert!(binnen.fout.is_some(), "de waarschuwing verdwijnt niet stil");
    }

    #[test]
    fn een_nieuwe_poging_voor_het_sluiten_bewaart_het_verslag_en_dan_gaat_het_gewoon_dicht() {
        let basis = tempfile::tempdir().unwrap();
        let mut binnen = mislukt_verslag(basis.path());
        // De schijf is er weer.
        fs::create_dir_all(basis.path().join("vergadering-1")).unwrap();
        let opdracht = binnen.plan_herstel().expect("er is iets te herstellen");
        let uitslag = schrijf_opdracht(&Mutex::new(HashMap::new()), &opdracht);
        binnen.na_schrijven(&opdracht, uitslag);
        assert!(!open_verslag_van(&binnen).bewaren_mislukt);
        assert!(binnen.fout.is_none());
        assert!(
            basis
                .path()
                .join("vergadering-1")
                .join(verslagbestand::BESTAND)
                .is_file()
        );
        binnen.sluit_verslag(false).unwrap();
        assert!(binnen.fout.is_none());
        // Zonder mislukking is er niets te herstellen.
        assert!(binnen.plan_herstel().is_none());
    }

    #[test]
    fn een_wijziging_die_niets_veranderde_schrijft_niets() {
        let basis = tempfile::tempdir().unwrap();
        let map = bewaarde_vergadering(basis.path(), "vergadering-1", 1_000, Vergadersoort::Online);
        let mut binnen = in_gebruik(Fase::Klaar);
        binnen.lees_lijsten(basis.path(), 5_000);
        binnen
            .open_verslag(basis.path(), "vergadering-1", false)
            .unwrap();
        // Dezelfde naam, dezelfde markering: geen opdracht.
        assert!(wijzig_open_in(&mut binnen, |_| Ok(())).unwrap().is_none());
        assert!(
            wijzig_open_in(&mut binnen, |v| {
                v.persoon.zet_vervangen(false);
                Ok(())
            })
            .unwrap()
            .is_none()
        );
        // Een echte wijziging wel, en daarna dezelfde opnieuw niet.
        let opdracht = wijzig_open_in(&mut binnen, |v| {
            v.persoon.zet_vervangen(true);
            Ok(())
        })
        .unwrap()
        .expect("er veranderde iets");
        assert_eq!(opdracht.map, map);
        assert!(
            wijzig_open_in(&mut binnen, |v| {
                v.persoon.zet_vervangen(true);
                Ok(())
            })
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn een_oudere_schrijfopdracht_overschrijft_nooit_een_nieuwere() {
        let basis = tempfile::tempdir().unwrap();
        let map = schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let taak = TaakId(7);
        let mut binnen = uitschrijven_in(taak, &map);
        assert!(binnen.bij_uitslag(taak, Gespreksuitslag::Klaar(gesprek(Vergadersoort::Online))));
        let oud = wijzig_open_in(&mut binnen, |v| {
            v.gesprek.zet_tekst(0, "Eerst.").map(|_| ())
        })
        .unwrap()
        .unwrap();
        let nieuw = wijzig_open_in(&mut binnen, |v| {
            v.gesprek.zet_tekst(0, "Daarna.").map(|_| ())
        })
        .unwrap()
        .unwrap();
        assert!(nieuw.volgnr > oud.volgnr);
        let laatste = Mutex::new(HashMap::new());
        // De nieuwste is het eerst klaar; de oudere komt daarna en wordt overgeslagen.
        assert!(schrijf_opdracht(&laatste, &nieuw).unwrap());
        assert!(!schrijf_opdracht(&laatste, &oud).unwrap());
        binnen.na_schrijven(&oud, Ok(false));
        binnen.na_schrijven(&nieuw, Ok(true));
        assert_eq!(
            verslagbestand::lees(&map).unwrap().gesprek.beurten[0].tekst,
            "Daarna."
        );
        assert!(!open_verslag_van(&binnen).bewaren_mislukt);
        // Andere mappen hebben hun eigen volgorde.
        let andere = Schrijfopdracht {
            map: basis.path().join("andere"),
            bytes: Vec::new(),
            volgnr: 1,
            taak,
        };
        fs::create_dir_all(&andere.map).unwrap();
        assert!(schrijf_opdracht(&laatste, &andere).unwrap());
    }

    #[test]
    fn een_map_die_wordt_gewist_staat_in_geen_lijst_en_het_opruimen_slaat_haar_over() {
        let basis = tempfile::tempdir().unwrap();
        let nu = 100 * DAG;
        let map = bewaarde_vergadering(
            basis.path(),
            "vergadering-1",
            nu - 9 * DAG,
            Vergadersoort::Online,
        );
        let opname = schrijf_opname(basis.path(), "vergadering-2", nu - 9 * DAG, "online", true);
        let mut binnen = in_gebruik(Fase::Klaar);
        binnen.wordt_gewist = vec!["vergadering-1".to_string(), "vergadering-2".to_string()];
        // Een herlezing zet ze niet terug in een lijst.
        binnen.lees_lijsten(basis.path(), nu);
        assert!(binnen.verslagen.is_empty() && binnen.opnames.is_empty());
        // En het opruimen markeert of wist ze niet, ook jaren later.
        for ronde in [nu, nu + 2 * DAG, nu + 10 * DAG] {
            assert!(!ruim_op_in(&mut binnen, basis.path(), ronde));
        }
        assert!(map.exists() && opname.exists());
        assert!(!map.join("verloopt.json").exists());
        // Is het wissen klaar of mislukt, dan telt de map weer mee.
        binnen.wordt_gewist.clear();
        binnen.lees_lijsten(basis.path(), nu);
        assert_eq!(binnen.verslagen.len(), 1);
        assert_eq!(binnen.opnames.len(), 1);
    }

    #[test]
    fn de_wistijd_van_het_open_verslag_komt_bij_openen_en_bij_een_andere_termijn() {
        let basis = tempfile::tempdir().unwrap();
        let begin = nu_ms() - DAG;
        bewaarde_vergadering(basis.path(), "vergadering-1", begin, Vergadersoort::Online);
        let mut binnen = in_gebruik(Fase::Klaar);
        binnen.lees_lijsten(basis.path(), 5_000);
        binnen
            .open_verslag(basis.path(), "vergadering-1", false)
            .unwrap();
        assert_eq!(
            open_verslag_van(&binnen).wist_ms,
            Some(begin + ms(WEEK) + DAG)
        );
        // De stand leest zelf geen bestanden meer: hij geeft wat het verslag onthoudt.
        let j = json(&binnen.stand());
        assert_eq!(j["toestand"]["wistMs"], begin + ms(WEEK) + DAG);
        binnen.bewaar_dagen = Some(30);
        binnen.lees_lijsten(basis.path(), 5_000);
        assert_eq!(
            json(&binnen.stand())["toestand"]["wistMs"],
            begin + 30 * DAG + DAG
        );
        // Een onbekende termijn geeft geen wistijd.
        binnen.bewaar_dagen = None;
        binnen.lees_lijsten(basis.path(), 5_000);
        assert!(json(&binnen.stand())["toestand"]["wistMs"].is_null());
    }

    #[test]
    fn de_lijst_slaat_het_open_verslag_over_en_leest_geen_tekst() {
        let basis = tempfile::tempdir().unwrap();
        let open =
            bewaarde_vergadering(basis.path(), "vergadering-1", 1_000, Vergadersoort::Online);
        bewaarde_vergadering(basis.path(), "vergadering-2", 2_000, Vergadersoort::Online);
        // Het open verslag is niet eens te lezen, en dat maakt de lijst niets uit.
        fs::write(open.join(verslagbestand::BESTAND), "{ kapot").unwrap();
        let lijsten = zoek_lijsten(basis.path(), Some(&open), Some(WEEK), 5_000);
        assert_eq!(lijsten.verslagen.len(), 1);
        assert_eq!(lijsten.verslagen[0].id, "vergadering-2");
        assert!(lijsten.opnames.is_empty());
    }

    // -----------------------------------------------------------------------
    // Opnieuw uitschrijven (ADR 0023)
    // -----------------------------------------------------------------------

    /// Een open, bewaard verslag in `map` met een eigen markering en een verbeterde regel.
    fn open_voor_opnieuw(map: &Path) -> Binnen {
        let taak = TaakId(7);
        let mut binnen = uitschrijven_in(taak, map);
        assert!(binnen.bij_uitslag(taak, Gespreksuitslag::Klaar(gesprek(Vergadersoort::Online))));
        wijzig_nu(&mut binnen, |v| {
            v.persoon.markeer("Wethouder").map_err(markeerfout)?;
            v.gesprek.zet_tekst(0, "Hallo allemaal.").map(|_| ())
        })
        .unwrap();
        binnen
    }

    fn opnieuw(taak: TaakId) -> Opnieuw {
        Opnieuw {
            taak,
            model_id: "whisper".to_string(),
            model: "Whisper".to_string(),
            rol: Some(Modelrol::ExtraNauwkeurig),
            voortgang: None,
            begonnen: None,
            meetpunt: None,
            verwacht: None,
        }
    }

    /// Het gesprek dat het nieuwe model geeft: een regel anders, de naam van de spreker anders
    /// gespeld (de eigen markering "Wethouder" valt dan weg).
    fn nieuw_gesprek() -> Gesprek {
        let mut g = gesprek(Vergadersoort::Online);
        g.beurten[0].tekst = "hallo   Allemaal.".to_string();
        g.beurten[1].tekst = "Dag allemaal.".to_string();
        g.beurten[1].naam = Some("Wethouders".to_string());
        // Een nieuwe uitschrijving heeft ook andere tijden: de opbouw verschilt.
        g.beurten[1].eind_ms += 100;
        g
    }

    #[test]
    fn opnieuw_uitgeschreven_vervangt_het_verslag_en_terugzetten_geeft_precies_het_oude() {
        let basis = tempfile::tempdir().unwrap();
        let map = schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let mut binnen = open_voor_opnieuw(&map);
        let oud_bytes = fs::read(map.join(verslagbestand::BESTAND)).unwrap();
        let oud = open_verslag_van(&binnen).gesprek.clone();
        assert_eq!(oud.verbeterde_regels(), 1);
        let stand = json(&binnen.stand());
        assert_eq!(stand["toestand"]["verbeterd"], 1);
        assert_eq!(stand["toestand"]["opnameAanwezig"], true);
        assert_eq!(stand["toestand"]["modelId"], "canary");

        let mee = binnen.voor_opnieuw().unwrap();
        assert_eq!(mee.map, map);
        assert!(mee.oud == oud);
        assert!(mee.persoon == open_verslag_van(&binnen).persoon);
        binnen.begin_opnieuw(opnieuw(TaakId(20)));
        assert!(binnen.schrijft_uit_met("whisper"));
        let stand = json(&binnen.stand());
        assert_eq!(stand["toestand"]["opnieuw"]["rol"], "extra_nauwkeurig");
        assert!(binnen.voor_opnieuw().is_err(), "niet twee keer tegelijk");

        // Een uitslag van een andere taak hoort er niet bij.
        assert!(!binnen.bij_uitslag(TaakId(99), Gespreksuitslag::Klaar(nieuw_gesprek())));
        assert!(binnen.bij_uitslag(TaakId(20), Gespreksuitslag::Klaar(nieuw_gesprek())));
        let v = open_verslag_van(&binnen);
        assert!(v.opnieuw.is_none());
        assert!(v.gesprek == nieuw_gesprek());
        assert_eq!(v.model_id, "whisper");
        // Het nieuwe verslag staat meteen in verslag.json.
        let bewaard = verslagbestand::lees(&map).unwrap();
        assert!(bewaard.gesprek == nieuw_gesprek());
        assert_eq!(bewaard.model_id, "whisper");
        // De melding: een regel veranderd, de eigen markering staat niet meer in de tekst.
        let stand = json(&binnen.stand());
        let t = &stand["toestand"];
        assert_eq!(t["naOpnieuw"]["veranderd"], 1);
        assert_eq!(t["naOpnieuw"]["verdwenen"], 1);
        assert_eq!(t["naOpnieuw"]["rol"], "extra_nauwkeurig");
        assert_eq!(t["vorigeVersie"], true);
        assert_eq!(t["beurten"][0]["veranderd"], false);
        assert_eq!(t["beurten"][1]["veranderd"], true);
        assert_eq!(t["opnieuw"], serde_json::Value::Null);
        assert_eq!(t["persoon"]["eigen"][0], "wethouder");

        // Terugzetten: precies het oude verslag, ook op schijf.
        let opdracht = binnen.zet_vorige_terug().unwrap();
        if let Some(opdracht) = opdracht {
            let uitslag = schrijf_opdracht(&Mutex::new(HashMap::new()), &opdracht);
            binnen.na_schrijven(&opdracht, uitslag);
        }
        assert!(open_verslag_van(&binnen).gesprek == oud);
        assert_eq!(
            fs::read(map.join(verslagbestand::BESTAND)).unwrap(),
            oud_bytes
        );
        let stand = json(&binnen.stand());
        assert_eq!(stand["toestand"]["vorigeVersie"], false);
        assert_eq!(stand["toestand"]["naOpnieuw"], serde_json::Value::Null);
        assert!(binnen.zet_vorige_terug().is_err(), "maar een vorige versie");
    }

    #[test]
    fn notulen_blijven_bij_naam_wijzigen_opnieuw_uitschrijven_en_terugzetten() {
        use mondig_core::notulen::{self, Herkomst, Invoer, Notulen, Taal};

        let basis = tempfile::tempdir().unwrap();
        let map = schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let mut binnen = open_voor_opnieuw(&map);
        let gesprek = open_verslag_van(&binnen).gesprek.clone();
        let ruw: Notulen = serde_json::from_value(serde_json::json!({
            "samenvatting": "Kort overleg.", "besluiten": [], "actiepunten": [],
        }))
        .unwrap();
        let opgeslagen = OpgeslagenNotulen::Leesbaar(notulen::controleer_notulen(
            ruw,
            &Invoer::van(&gesprek, &Markeringen::default()),
            &gesprek,
            Herkomst {
                aanbieder: "mistral".into(),
                model: "mistral-small-latest".into(),
                moment_ms: 1,
                taal: Taal::Nl,
            },
        ));
        wijzig_nu(&mut binnen, |v| {
            v.notulen = Some(opgeslagen.clone());
            Ok(())
        })
        .unwrap();

        // Een andere naam: de notulen gaan mee naar schijf.
        wijzig_nu(&mut binnen, |v| {
            v.naam = schone_verslagnaam("Overleg speelveld");
            Ok(())
        })
        .unwrap();
        let bewaard = verslagbestand::lees(&map).unwrap();
        assert_eq!(bewaard.naam.as_deref(), Some("Overleg speelveld"));
        let n = bewaard.notulen.expect("notulen na naam wijzigen");
        assert!(n == opgeslagen);
        assert!(!n.leesbaar().unwrap().bij_vorige_versie(&bewaard.gesprek));

        // Opnieuw uitschrijven: de notulen blijven, bij de vorige versie.
        binnen.begin_opnieuw(opnieuw(TaakId(20)));
        assert!(binnen.bij_uitslag(TaakId(20), Gespreksuitslag::Klaar(nieuw_gesprek())));
        let bewaard = verslagbestand::lees(&map).unwrap();
        let n = bewaard.notulen.expect("notulen na opnieuw uitschrijven");
        assert!(n == opgeslagen);
        assert!(n.leesbaar().unwrap().bij_vorige_versie(&bewaard.gesprek));

        // Terugzetten: weer bij de versie waar ze bij horen.
        if let Some(opdracht) = binnen.zet_vorige_terug().unwrap() {
            let uitslag = schrijf_opdracht(&Mutex::new(HashMap::new()), &opdracht);
            binnen.na_schrijven(&opdracht, uitslag);
        }
        let bewaard = verslagbestand::lees(&map).unwrap();
        let n = bewaard.notulen.expect("notulen na terugzetten");
        assert!(!n.leesbaar().unwrap().bij_vorige_versie(&bewaard.gesprek));
    }

    #[test]
    fn stoppen_laat_het_oude_verslag_staan() {
        let basis = tempfile::tempdir().unwrap();
        let map = schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let mut binnen = open_voor_opnieuw(&map);
        let oud_bytes = fs::read(map.join(verslagbestand::BESTAND)).unwrap();
        let oud = open_verslag_van(&binnen).gesprek.clone();
        binnen.begin_opnieuw(opnieuw(TaakId(21)));
        assert_eq!(binnen.stop_opnieuw(), Some(TaakId(21)));
        assert_eq!(binnen.stop_opnieuw(), None);
        // Een uitslag die na Stoppen nog komt, verandert niets.
        assert!(!binnen.bij_uitslag(TaakId(21), Gespreksuitslag::Klaar(nieuw_gesprek())));
        let v = open_verslag_van(&binnen);
        assert!(v.gesprek == oud);
        assert!(v.vorige.is_none());
        assert_eq!(
            fs::read(map.join(verslagbestand::BESTAND)).unwrap(),
            oud_bytes
        );
        // En nu mag er weer gewijzigd en gesloten worden.
        assert!(wijzig_nu(&mut binnen, |_| Ok(())).is_ok());
        assert!(binnen.sluit_verslag(false).is_ok());
    }

    #[test]
    fn tijdens_opnieuw_uitschrijven_weigeren_wijzigen_sluiten_en_wissen() {
        let basis = tempfile::tempdir().unwrap();
        let map = schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let ander =
            bewaarde_vergadering(basis.path(), "vergadering-2", 2_000, Vergadersoort::Online);
        let mut binnen = open_voor_opnieuw(&map);
        binnen.begin_opnieuw(opnieuw(TaakId(22)));
        let fout = wijzig_nu(&mut binnen, |v| {
            v.gesprek.zet_tekst(1, "Iets anders.").map(|_| ())
        })
        .unwrap_err();
        assert!(
            fout.melding.contains("Verbeteren kan weer"),
            "{}",
            fout.melding
        );
        assert!(
            wijzig_nu(&mut binnen, |v| {
                v.persoon.zet_vervangen(true);
                Ok(())
            })
            .is_err()
        );
        assert!(binnen.sluit_verslag(true).is_err());
        assert!(binnen.neem_verslag().is_err());
        let naam = ander.file_name().unwrap().to_string_lossy().into_owned();
        assert!(binnen.open_verslag(basis.path(), &naam, true).is_err());
        assert!(binnen.zet_vorige_terug().is_err());
        // Het verslag is er nog, ongewijzigd, met het opnieuw uitschrijven erbij.
        let v = open_verslag_van(&binnen);
        assert!(v.opnieuw.is_some());
        assert_eq!(v.gesprek.beurten[1].tekst, "Dag.");
    }

    #[test]
    fn de_voortgang_en_een_fout_bij_opnieuw_uitschrijven() {
        let basis = tempfile::tempdir().unwrap();
        let map = schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let mut binnen = open_voor_opnieuw(&map);
        let oud = open_verslag_van(&binnen).gesprek.clone();
        binnen.begin_opnieuw(opnieuw(TaakId(23)));
        let gebeurtenis = |toestand| Gebeurtenis {
            taak: TaakId(23),
            soort: mondig_core::taak::Soort::Vergadering,
            toestand,
        };
        assert!(binnen.bij_gebeurtenis(&gebeurtenis(Toestand::Verwerkt {
            voortgang: Some(34),
            stap: Stap::Uitschrijven,
        })));
        let stand = json(&binnen.stand());
        assert_eq!(stand["toestand"]["opnieuw"]["voortgang"], 34);
        // Het eerste percentage is het meetpunt: nog geen tijd te schatten.
        assert!(stand["toestand"]["opnieuw"]["restMs"].is_null());
        // Een gebeurtenis van een andere taak telt niet.
        assert!(!binnen.bij_gebeurtenis(&Gebeurtenis {
            taak: TaakId(5),
            ..gebeurtenis(Toestand::Geannuleerd)
        }));
        // Een fout laat het oude verslag staan, met een melding.
        assert!(binnen.bij_gebeurtenis(&gebeurtenis(Toestand::Fout {
            melding: "Het model liep vast.".to_string(),
            vervolg: Vervolg::OpnieuwProberen,
        })));
        let v = open_verslag_van(&binnen);
        assert!(v.opnieuw.is_none());
        assert!(v.gesprek == oud);
        let fout = binnen.fout.clone().unwrap();
        assert!(fout.melding.contains("niet veranderd"), "{}", fout.melding);
        // Daarna mag opnieuw, maar niet als de sporen weg zijn.
        assert!(binnen.voor_opnieuw().is_ok());
        fs::remove_dir_all(map.join(MICROFOON)).unwrap();
        assert!(binnen.voor_opnieuw().is_err());
        // De stand leest de schijf niet zelf: pas na het bijwerken van de lijsten.
        assert_eq!(json(&binnen.stand())["toestand"]["opnameAanwezig"], true);
        binnen.lees_lijsten(basis.path(), 0);
        assert_eq!(json(&binnen.stand())["toestand"]["opnameAanwezig"], false);
    }

    #[test]
    fn de_lijst_meldt_of_de_opname_er_nog_is() {
        let basis = tempfile::tempdir().unwrap();
        let met = bewaarde_vergadering(basis.path(), "vergadering-1", 1_000, Vergadersoort::Online);
        let zonder =
            bewaarde_vergadering(basis.path(), "vergadering-2", 2_000, Vergadersoort::Online);
        fs::remove_dir_all(zonder.join(MICROFOON)).unwrap();
        let lijsten = zoek_lijsten(basis.path(), None, None, 0);
        let aanwezig: Vec<(String, bool)> = lijsten
            .verslagen
            .iter()
            .map(|v| (v.id.clone(), v.opname_aanwezig))
            .collect();
        assert_eq!(
            aanwezig,
            vec![
                ("vergadering-2".to_string(), false),
                ("vergadering-1".to_string(), true),
            ]
        );
        assert!(met.exists());
    }

    #[test]
    fn de_resterende_tijd_telt_vanaf_het_eerste_percentage() {
        let minuut = Duration::from_secs(60);
        // Van 10 naar 40 procent in een minuut: nog 60 procent, dus twee minuten.
        assert_eq!(resterend(40, 10, minuut), Some(minuut * 2));
        assert_eq!(resterend(100, 10, minuut), Some(Duration::ZERO));
        assert_eq!(resterend(10, 10, minuut), None);
        // Zonder meetpunt de verwachte duur min wat er al verstreek.
        let o = Opnieuw {
            verwacht: Some(minuut * 5),
            ..opnieuw(TaakId(1))
        };
        assert_eq!(o.stand().rest_ms, Some(300_000));
    }

    #[test]
    fn de_meting_komt_alleen_uit_het_uitschrijven_zelf() {
        let mut o = opnieuw(TaakId(1));
        o.voortgang(None);
        assert!(o.meetpunt.is_none(), "laden telt niet");
        o.voortgang(Some(20));
        o.voortgang(Some(60));
        assert_eq!(o.meetpunt.map(|(_, p)| p), Some(20));
        let g = gesprek(Vergadersoort::Online);
        let (model, factor) = meting(&o, &g).unwrap();
        assert_eq!(model, "whisper");
        assert!(factor >= 0.0);
        // Zonder percentage, of als het eerste percentage al 100 was: geen meting.
        assert!(meting(&opnieuw(TaakId(2)), &g).is_none());
        let mut snel = opnieuw(TaakId(3));
        snel.voortgang(Some(100));
        assert!(meting(&snel, &g).is_none());
        // Gewoon uitschrijven meet niets.
        let mut binnen = uitschrijven(TaakId(4));
        assert!(binnen.bij_uitslag(TaakId(4), Gespreksuitslag::Klaar(g)));
        assert!(binnen.neem_meting().is_none());
    }

    #[test]
    fn na_een_latere_wijziging_vervalt_de_vorige_versie_en_gaat_niets_stil_verloren() {
        let basis = tempfile::tempdir().unwrap();
        let map = schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let mut binnen = open_voor_opnieuw(&map);
        binnen.begin_opnieuw(opnieuw(TaakId(30)));
        assert!(binnen.bij_uitslag(TaakId(30), Gespreksuitslag::Klaar(nieuw_gesprek())));
        // Een markering en de naam van het verslag laten de vorige versie en de vlaggen staan.
        wijzig_nu(&mut binnen, |v| {
            v.persoon.markeer("Allemaal").map_err(markeerfout)?;
            v.naam = schone_verslagnaam("Overleg");
            Ok(())
        })
        .unwrap();
        let t = json(&binnen.stand())["toestand"].clone();
        assert_eq!(t["vorigeVersie"], true);
        assert_eq!(t["beurten"][1]["veranderd"], true);
        // Een verbeterde tekst: de vorige versie vervalt, de vlaggen blijven (niets schuift).
        wijzig_nu(&mut binnen, |v| {
            wijzig_gesprek(v, false, |g| {
                g.zet_tekst(1, "Dag iedereen.").map(usize::from)
            })
        })
        .unwrap();
        let t = json(&binnen.stand())["toestand"].clone();
        assert_eq!(t["vorigeVersie"], false);
        assert_eq!(t["naOpnieuw"], serde_json::Value::Null);
        assert_eq!(t["beurten"][1]["veranderd"], true);
        assert!(
            binnen.zet_vorige_terug().is_err(),
            "de verbetering gaat niet stil verloren"
        );
        assert_eq!(
            open_verslag_van(&binnen).gesprek.beurten[1].tekst,
            "Dag iedereen."
        );
        // Een wijziging die beurten laat schuiven, haalt de vlaggen weg.
        wijzig_nu(&mut binnen, |v| {
            wijzig_gesprek(v, true, |g| g.geef_naam(1, "Piet", Naambereik::Beurt))
        })
        .unwrap();
        assert_eq!(
            json(&binnen.stand())["toestand"]["beurten"][1]["veranderd"],
            false
        );
    }

    #[test]
    fn terugzetten_houdt_een_latere_markering() {
        let basis = tempfile::tempdir().unwrap();
        let map = schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let mut binnen = open_voor_opnieuw(&map);
        binnen.begin_opnieuw(opnieuw(TaakId(31)));
        assert!(binnen.bij_uitslag(TaakId(31), Gespreksuitslag::Klaar(nieuw_gesprek())));
        wijzig_nu(&mut binnen, |v| {
            v.persoon.markeer("Allemaal").map_err(markeerfout)
        })
        .unwrap();
        binnen.zet_vorige_terug().unwrap();
        assert!(
            open_verslag_van(&binnen)
                .persoon
                .eigen()
                .contains(&"allemaal".to_string())
        );
        assert_eq!(open_verslag_van(&binnen).model_id, "canary");
    }

    #[test]
    fn alleen_een_hogere_rol_wordt_aangeboden() {
        let modellen = catalogus::catalogus().unwrap();
        let uur = Duration::from_secs(3_600);
        let rollen = |huidig: &str| -> Vec<Modelrol> {
            opnieuw_keuzes(&modellen, huidig, uur, |_| true, |_| None)
                .keuzes
                .iter()
                .map(|k| k.rol)
                .collect()
        };
        assert_eq!(
            rollen("parakeet-ultra-q8"),
            vec![Modelrol::Nauwkeurig, Modelrol::ExtraNauwkeurig]
        );
        assert_eq!(rollen("canary-1b-v2-q8"), vec![Modelrol::ExtraNauwkeurig]);
        assert!(rollen("whisper-large-v3-turbo-q8").is_empty());
        // Een onbekend model: nauwkeurig en extra nauwkeurig.
        assert_eq!(rollen("weg").len(), 2);
    }

    #[test]
    fn de_gegevensregel_krijgt_de_rol_van_het_model() {
        let basis = tempfile::tempdir().unwrap();
        let map = schrijf_opname(basis.path(), "vergadering-1", 1_000, "online", true);
        let mut binnen = open_voor_opnieuw(&map);
        if let Fase::Verslag(v) = &mut binnen.fase {
            v.model_id = "canary-1b-v2-q8".to_string();
        }
        assert_eq!(json(&binnen.stand())["toestand"]["modelRol"], "nauwkeurig");
        if let Fase::Verslag(v) = &mut binnen.fase {
            v.model_id = "onbekend".to_string();
        }
        assert_eq!(
            json(&binnen.stand())["toestand"]["modelRol"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn de_keuzes_zijn_de_nauwkeurige_modellen_zonder_het_huidige() {
        let modellen = catalogus::catalogus().unwrap();
        let uur = Duration::from_secs(3_600);
        let keuzes = opnieuw_keuzes(
            &modellen,
            "parakeet-ultra-q8",
            uur,
            |m| m.id == "canary-1b-v2-q8",
            |id| (id == "canary-1b-v2-q8").then_some(0.1),
        );
        assert_eq!(keuzes.rol, Some(Modelrol::Snel));
        let rij: Vec<(&str, Modelrol, bool, Option<u64>)> = keuzes
            .keuzes
            .iter()
            .map(|k| (k.model_id.as_str(), k.rol, k.aanwezig, k.duur_ms))
            .collect();
        assert_eq!(
            rij,
            vec![
                ("canary-1b-v2-q8", Modelrol::Nauwkeurig, true, Some(360_000)),
                (
                    "whisper-large-v3-turbo-q8",
                    Modelrol::ExtraNauwkeurig,
                    false,
                    None
                ),
            ]
        );
        assert!(keuzes.keuzes[1].grootte_bytes > 0);
        // Met het nauwkeurige model gemaakt: alleen extra nauwkeurig blijft over.
        let keuzes = opnieuw_keuzes(&modellen, "canary-1b-v2-q8", uur, |_| true, |_| None);
        assert_eq!(keuzes.rol, Some(Modelrol::Nauwkeurig));
        assert_eq!(keuzes.keuzes.len(), 1);
        assert_eq!(keuzes.keuzes[0].rol, Modelrol::ExtraNauwkeurig);
    }

    /// Wat het menu van het systeemvak uit de fase haalt (N-C5): alleen in rust kan een nieuwe
    /// opname beginnen, tijdens een opname staat de status, in alle andere fasen is starten uit.
    #[test]
    fn het_systeemvak_ziet_de_fase_als_rust_loopt_of_bezig() {
        use crate::systeemvak::OpnameStand;
        let stand = |fase: Fase| in_gebruik(fase).stand().toestand.systeemvak().0;
        assert_eq!(stand(Fase::Klaar), OpnameStand::Rust);
        let opname = Fase::NeemtOp(Opnemen {
            map: PathBuf::new(),
            soort: Vergadersoort::Online,
            begin_ms: 0,
            begin: Instant::now(),
            niveaus: None,
            systeem_stil: false,
            meldingen: Vec::new(),
        });
        assert_eq!(stand(opname), OpnameStand::Loopt);
        assert_eq!(
            stand(Fase::Starten {
                soort: Vergadersoort::Ruimte
            }),
            OpnameStand::Bezig
        );
        let voorbereiden = Fase::Voorbereiden(Voorbereiden {
            map: PathBuf::new(),
            soort: Vergadersoort::Online,
            begin_ms: 0,
            duur_ms: None,
            herstellen: false,
        });
        assert_eq!(stand(voorbereiden), OpnameStand::Bezig);
    }

    #[test]
    fn een_open_verslag_en_het_uitschrijven_weigert_starten_tot_het_verslag_dicht_is() {
        use crate::systeemvak::OpnameStand;
        let basis = tempfile::tempdir().unwrap();
        bewaarde_vergadering(basis.path(), "vergadering-1", 1_000, Vergadersoort::Online);
        let mut binnen = in_gebruik(Fase::Klaar);
        binnen.lees_lijsten(basis.path(), 5_000);
        binnen
            .open_verslag(basis.path(), "vergadering-1", false)
            .unwrap();
        // Dezelfde regel als `claim`: met een open verslag weigert de app een nieuwe opname.
        assert!(
            binnen
                .claim(Fase::Starten {
                    soort: Vergadersoort::Online
                })
                .is_err()
        );
        assert_eq!(
            binnen.stand().toestand.systeemvak().0,
            OpnameStand::VerslagOpen
        );
    }

    #[test]
    fn de_herinnering_vooraf_geldt_voor_alles_behalve_alleen_ik() {
        assert!(vraagt_herinnering(Vergadersoort::Online));
        assert!(vraagt_herinnering(Vergadersoort::Ruimte));
        assert!(!vraagt_herinnering(Vergadersoort::Alleen));
    }

    #[test]
    fn de_opnametijd_heeft_uren_boven_het_uur() {
        assert_eq!(klok_tekst(Duration::from_secs(755)), "12:35");
        assert_eq!(klok_tekst(Duration::from_secs(3_723)), "1:02:03");
    }

    #[test]
    fn een_verzoek_van_het_systeemvak_komt_een_keer_en_vervalt_na_30_seconden() {
        let nu = Instant::now();
        let verzoek = StartVerzoek::default();
        assert_eq!(verzoek.neem(nu), None);
        verzoek.zet(Vergadersoort::Ruimte, nu);
        assert_eq!(
            verzoek.neem(nu + Duration::from_secs(5)),
            Some(Vergadersoort::Ruimte)
        );
        // Eenmaal opgehaald is het weg.
        assert_eq!(verzoek.neem(nu + Duration::from_secs(6)), None);
        verzoek.zet(Vergadersoort::Online, nu);
        assert_eq!(verzoek.neem(nu + Duration::from_secs(31)), None);
    }

    #[test]
    fn een_open_verslag_is_te_sluiten_behalve_als_bewaren_mislukte() {
        use crate::systeemvak::OpnameStand;
        let basis = tempfile::tempdir().unwrap();
        bewaarde_vergadering(basis.path(), "vergadering-1", 1_000, Vergadersoort::Online);
        let mut binnen = in_gebruik(Fase::Klaar);
        binnen.lees_lijsten(basis.path(), 5_000);
        binnen
            .open_verslag(basis.path(), "vergadering-1", false)
            .unwrap();
        assert_eq!(
            binnen.stand().toestand.systeemvak().0,
            OpnameStand::VerslagOpen
        );
        // Sluiten zonder wissen lukt en brengt de app terug naar rust.
        binnen.sluit_verslag(false).unwrap();
        assert_eq!(binnen.stand().toestand.systeemvak().0, OpnameStand::Rust);
        // Bewaren mislukt: het menu zegt dat eerst het verslag af moet.
        binnen.lees_lijsten(basis.path(), 5_000);
        binnen
            .open_verslag(basis.path(), "vergadering-1", false)
            .unwrap();
        if let Fase::Verslag(v) = &mut binnen.fase {
            v.bewaren_mislukt = true;
        }
        assert_eq!(
            binnen.stand().toestand.systeemvak().0,
            OpnameStand::VerslagVast
        );
        assert!(binnen.sluit_verslag(false).is_err());
    }
}
