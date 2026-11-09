//! Het logboek voor support (S-2a, ADR 0003): een lokaal bestand met gebeurtenissen en fouten,
//! zonder tekst van de gebruiker.
//!
//! **Privacy door de typen.** Een logregel wordt gebouwd uit een [`Gebeurtenis`]. Die kent
//! alleen getallen, vaste codes (`&'static str`, dus een letterlijke tekst in de code) en
//! modelnamen uit de catalogus ([`ModelNaam`]). Er is geen variant en geen veld waar een
//! dictaat, transcript, bestandsnaam, spreker of foutmelding in past. Wie een nieuwe
//! gebeurtenis toevoegt, kan er dus geen tekst van de gebruiker in kwijt. De test
//! `elke_regel_bestaat_alleen_uit_codes_en_getallen` bewaakt de vorm van de regel.
//!
//! **Begrensd.** Het logboek is één bestand dat bij [`GRENS_BYTES`] wordt hernoemd naar
//! een oud bestand (het vorige oude bestand verdwijnt). Samen nemen ze dus nooit meer dan
//! twee keer de grens in.
//!
//! **Niets gaat vanzelf weg.** De gebruiker kopieert het logboek zelf naar een mail of een
//! melding; deze module doet geen netwerkverkeer.
//!
//! Vibe schrijft logs met `tracing` naar een dagelijks bestand en ruimt oude bestanden op
//! (`desktop/src-tauri/src/logging.rs`, `cleaner.rs`, MIT); dat patroon is hier overgenomen, de
//! code is eigen (ADR 0006). Waarom geen `tracing`: dat accepteert elke tekst als veld, en
//! rotatie op grootte (in plaats van op dag) heeft nog geen eigen crate nodig.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::probleem::Probleem;
use crate::taak::Soort;

/// Zo groot mag één logbestand worden voor het wordt omgedraaid.
pub const GRENS_BYTES: u64 = 256 * 1024;

const HUIDIG: &str = "mondig.log";
const OUD: &str = "mondig.oud.log";

/// Hoe ernstig een regel is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Niveau {
    /// Een gewone gebeurtenis.
    Info,
    /// Er ging iets mis.
    Fout,
}

impl Niveau {
    const fn code(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Fout => "fout",
        }
    }
}

/// Een modelnaam uit de catalogus. Een id dat daar niet in staat wordt "onbekend": zo kan er
/// geen vrije tekst via een modelnaam in het logboek komen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelNaam(String);

impl ModelNaam {
    /// De naam voor dit id, of "onbekend" als de catalogus het niet kent.
    #[must_use]
    pub fn nieuw(id: &str) -> Self {
        let bekend = crate::model::catalogus()
            .is_ok_and(|catalogus| catalogus.iter().any(|model| model.id == id));
        Self(if bekend { id } else { "onbekend" }.to_string())
    }
}

/// De naam van een type op het sleepbord van de Mac, zoals `public.file-url` (ADR 0030). Alleen
/// de naam, nooit de inhoud. Zo'n naam komt van een app, niet van de gebruiker, maar hij wordt
/// toch teruggebracht tot het alfabet van het logboek: kleine letters, cijfers, punt, min en
/// underscore (de rest wordt `_`), hooguit [`SleepbordType::LENGTE`] tekens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SleepbordType(String);

impl SleepbordType {
    /// Zo lang mag een naam in het logboek zijn.
    pub const LENGTE: usize = 80;
    /// Zoveel namen komen er hooguit in één regel.
    pub const AANTAL: usize = 20;

    /// De naam in het alfabet van het logboek; leeg wordt `leeg`.
    #[must_use]
    pub fn nieuw(naam: &str) -> Self {
        let schoon: String = naam
            .chars()
            .take(Self::LENGTE)
            .map(|c| {
                let c = c.to_ascii_lowercase();
                if c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        Self(if schoon.is_empty() {
            "leeg".to_string()
        } else {
            schoon
        })
    }

    /// De eerste [`SleepbordType::AANTAL`] namen.
    #[must_use]
    pub fn lijst<S: AsRef<str>>(namen: &[S]) -> Vec<Self> {
        namen
            .iter()
            .take(Self::AANTAL)
            .map(|naam| Self::nieuw(naam.as_ref()))
            .collect()
    }
}

/// Een waarde in een logregel: een getal of een vaste code.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Waarde {
    Getal(u64),
    Code(&'static str),
    Model(ModelNaam),
    Type(SleepbordType),
}

/// Waar een taak om draait, voor `TaakMislukt`. Zonder de naam of inhoud van de taak.
fn taakcode(soort: Soort) -> &'static str {
    match soort {
        Soort::Dictaat => "dictaat",
        Soort::Bestand => "bestand",
        Soort::Vergadering => "vergadering",
        Soort::Download => "download",
    }
}

/// Alles wat het logboek kent. Bewust zonder tekstvelden (zie de moduletekst).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gebeurtenis {
    /// Mondig is gestart.
    AppGestart {
        /// Het versienummer van de app.
        versie: &'static str,
        /// Het besturingssysteem (`windows`, `macos`, `linux`).
        besturingssysteem: &'static str,
    },
    /// De computer is uitgelezen voor de hardwarecheck.
    ComputerGelezen {
        /// Werkgeheugen in MB, als te meten.
        geheugen_mb: Option<u64>,
        /// Aantal processorkernen.
        kernen: Option<u32>,
        /// Vrije schijfruimte in MB.
        vrij_mb: Option<u64>,
        /// Aantal gevonden videokaarten, als zoeken lukte.
        videokaarten: Option<u32>,
    },
    /// De eerste-start-wizard is bij deze stap aangekomen.
    WizardStap {
        /// Een vaste code, bijvoorbeeld `advies`.
        stap: &'static str,
    },
    /// Een download begint.
    DownloadGestart {
        /// Het model.
        model: ModelNaam,
    },
    /// Een download is klaar.
    DownloadKlaar {
        /// Het model.
        model: ModelNaam,
    },
    /// Een taak (dictaat, bestand, vergadering of download) mislukte. Alleen de soort van de
    /// taak en het soort probleem; nooit de melding, want die kan een bestandsnaam bevatten.
    TaakMislukt {
        /// Wat voor taak.
        taak: Soort,
        /// Het soort probleem als Mondig dat kent.
        probleem: Option<Probleem>,
    },
    /// Een motor viel terug van de videokaart op de processor.
    VideokaartTerugval,
    /// De snelheidstest is klaar.
    Snelheidstest {
        /// `loont`, `loont_niet` of `videokaart_faalde`.
        oordeel: &'static str,
        /// Hoeveel keer sneller, in tienden (95 is 9,5 keer).
        keer_sneller_tienden: Option<u64>,
    },
    /// De gebruiker zette de videokaart aan of uit.
    VideokaartKeuze {
        /// Staat hij nu aan?
        aan: bool,
    },
    /// Alle instellingen zijn teruggezet (de modellen bleven staan).
    InstellingenTerugGezet,
    /// De gebruiker kopieerde het logboek.
    LogboekGekopieerd,
    /// Een dictaat is opgeschoond, of het bleef letterlijk (ADR 0027 punt 8): de uitkomst, de
    /// reden en de duur, zoals `opschonen::Uitkomst::logregel`, nooit tekst.
    Opschonen {
        /// `opgeschoond` of `letterlijk`.
        uitkomst: &'static str,
        /// Bij letterlijk: `te_laat`, `afgekeurd`, `afgekapt`, `aanbieder`, `gestopt` of
        /// `geen_tekst`.
        reden: Option<&'static str>,
        /// Bij afgekeurd de regel, bij een fout van de aanbieder de soort fout.
        detail: Option<&'static str>,
        /// Hoe lang het duurde, in milliseconden.
        duur_ms: u64,
    },
    /// Er is iets op het venster losgelaten zonder bestandspad (ADR 0030): hoeveel beloftes de
    /// Mac-haak ophaalt en welke typen er op het sleepbord stonden, zodat een tester ziet wat
    /// een app als Dictafoon meestuurt.
    SlepenZonderPaden {
        /// Het aantal bestandsbeloftes (0 als er niets op te halen viel).
        beloftes: u64,
        /// De typenamen, zonder inhoud.
        typen: Vec<SleepbordType>,
    },
    /// De haak voor bestandsbeloftes op de Mac staat er niet (ADR 0030); slepen vanuit de
    /// Finder werkt dan nog wel.
    SleephaakNiet {
        /// Een vaste code, bijvoorbeeld `andere_klasse`.
        reden: &'static str,
    },
    /// Een dictaat is afgeleverd in een ander programma, of niet: langs welke route en met welke
    /// uitkomst. Nooit de tekst zelf.
    DictaatAfgeleverd {
        /// `plakken`, `typen`, `klembord` of `geen`.
        route: &'static str,
        /// De uitkomst zoals de interface hem kent, bijvoorbeeld `getypt`.
        uitkomst: &'static str,
    },
}

impl Gebeurtenis {
    /// De vaste code van de gebeurtenis en haar velden.
    fn onderdelen(&self) -> (&'static str, Vec<(&'static str, Waarde)>) {
        let getal = |naam, waarde: Option<u64>| waarde.map(|w| (naam, Waarde::Getal(w)));
        match self {
            Self::AppGestart {
                versie,
                besturingssysteem,
            } => (
                "app_gestart",
                vec![
                    ("versie", Waarde::Code(versie)),
                    ("os", Waarde::Code(besturingssysteem)),
                ],
            ),
            Self::ComputerGelezen {
                geheugen_mb,
                kernen,
                vrij_mb,
                videokaarten,
            } => (
                "computer_gelezen",
                [
                    getal("geheugen_mb", *geheugen_mb),
                    getal("kernen", kernen.map(u64::from)),
                    getal("vrij_mb", *vrij_mb),
                    getal("videokaarten", videokaarten.map(u64::from)),
                ]
                .into_iter()
                .flatten()
                .collect(),
            ),
            Self::WizardStap { stap } => ("wizard_stap", vec![("stap", Waarde::Code(stap))]),
            Self::DownloadGestart { model } => (
                "download_gestart",
                vec![("model", Waarde::Model(model.clone()))],
            ),
            Self::DownloadKlaar { model } => (
                "download_klaar",
                vec![("model", Waarde::Model(model.clone()))],
            ),
            Self::TaakMislukt { taak, probleem } => {
                let mut velden = vec![("taak", Waarde::Code(taakcode(*taak)))];
                if let Some(probleem) = probleem {
                    velden.push(("probleem", Waarde::Code(probleem.code())));
                }
                ("taak_mislukt", velden)
            }
            Self::VideokaartTerugval => ("videokaart_terugval", Vec::new()),
            Self::Snelheidstest {
                oordeel,
                keer_sneller_tienden,
            } => (
                "snelheidstest",
                [
                    Some(("oordeel", Waarde::Code(oordeel))),
                    getal("keer_sneller_tienden", *keer_sneller_tienden),
                ]
                .into_iter()
                .flatten()
                .collect(),
            ),
            Self::VideokaartKeuze { aan } => (
                "videokaart_keuze",
                vec![("stand", Waarde::Code(if *aan { "aan" } else { "uit" }))],
            ),
            Self::InstellingenTerugGezet => ("instellingen_terug_gezet", Vec::new()),
            Self::LogboekGekopieerd => ("logboek_gekopieerd", Vec::new()),
            Self::Opschonen {
                uitkomst,
                reden,
                detail,
                duur_ms,
            } => (
                "opschonen",
                [
                    Some(("uitkomst", Waarde::Code(uitkomst))),
                    reden.map(|r| ("reden", Waarde::Code(r))),
                    detail.map(|d| ("detail", Waarde::Code(d))),
                    Some(("duur_ms", Waarde::Getal(*duur_ms))),
                ]
                .into_iter()
                .flatten()
                .collect(),
            ),
            Self::SlepenZonderPaden { beloftes, typen } => (
                "slepen_zonder_paden",
                std::iter::once(("beloftes", Waarde::Getal(*beloftes)))
                    .chain(typen.iter().map(|t| ("type", Waarde::Type(t.clone()))))
                    .collect(),
            ),
            Self::SleephaakNiet { reden } => {
                ("sleephaak_niet", vec![("reden", Waarde::Code(reden))])
            }
            Self::DictaatAfgeleverd { route, uitkomst } => (
                "dictaat_afgeleverd",
                vec![
                    ("route", Waarde::Code(route)),
                    ("uitkomst", Waarde::Code(uitkomst)),
                ],
            ),
        }
    }

    /// Het niveau van de gebeurtenis.
    #[must_use]
    pub fn niveau(&self) -> Niveau {
        match self {
            Self::TaakMislukt { .. } | Self::VideokaartTerugval | Self::SleephaakNiet { .. } => {
                Niveau::Fout
            }
            Self::Snelheidstest { oordeel, .. } if *oordeel == "videokaart_faalde" => Niveau::Fout,
            _ => Niveau::Info,
        }
    }

    /// De logregel zonder tijd en niveau: de code met `naam=waarde`-paren.
    #[must_use]
    pub fn tekst(&self) -> String {
        let (code, velden) = self.onderdelen();
        let mut regel = code.to_string();
        for (naam, waarde) in velden {
            regel.push(' ');
            regel.push_str(naam);
            regel.push('=');
            match waarde {
                Waarde::Getal(n) => regel.push_str(&n.to_string()),
                Waarde::Code(c) => regel.push_str(c),
                Waarde::Model(m) => regel.push_str(&m.0),
                Waarde::Type(t) => regel.push_str(&t.0),
            }
        }
        regel
    }
}

/// Het tijdstip als `2026-11-02T15:04:05Z` (UTC), uit seconden sinds 1970. Eigen rekensom
/// (de burgerlijke kalender uit dagen, van Howard Hinnant) om geen tijdbibliotheek in te
/// hoeven zetten voor één regel.
#[must_use]
pub fn tijdstip(seconden: u64) -> String {
    let dagen = i64::try_from(seconden / 86_400).unwrap_or(i64::MAX / 2);
    let rest = seconden % 86_400;
    let z = dagen + 719_468;
    let tijdperk = z.div_euclid(146_097);
    let dag_in_tijdperk = z.rem_euclid(146_097);
    let jaar_in_tijdperk = (dag_in_tijdperk - dag_in_tijdperk / 1_460 + dag_in_tijdperk / 36_524
        - dag_in_tijdperk / 146_096)
        / 365;
    let dag_in_jaar =
        dag_in_tijdperk - (365 * jaar_in_tijdperk + jaar_in_tijdperk / 4 - jaar_in_tijdperk / 100);
    let mp = (5 * dag_in_jaar + 2) / 153;
    let dag = dag_in_jaar - (153 * mp + 2) / 5 + 1;
    let maand = if mp < 10 { mp + 3 } else { mp - 9 };
    let jaar = jaar_in_tijdperk + tijdperk * 400 + i64::from(maand <= 2);
    format!(
        "{jaar:04}-{maand:02}-{dag:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Het logboek in een map.
#[derive(Debug)]
pub struct Logboek {
    map: PathBuf,
    grens: u64,
    /// Houdt schrijven, omdraaien en lezen uit elkaar.
    slot: Mutex<()>,
}

impl Logboek {
    /// Een logboek in `map` (wordt gemaakt bij het eerste schrijven), dat een bestand omdraait
    /// zodra het `grens` bytes zou overschrijden.
    #[must_use]
    pub fn nieuw(map: PathBuf, grens: u64) -> Self {
        Self {
            map,
            grens,
            slot: Mutex::new(()),
        }
    }

    /// De map met de logbestanden, voor "Logmap openen".
    #[must_use]
    pub fn map(&self) -> &Path {
        &self.map
    }

    /// Schrijft een regel met dit tijdstip (seconden sinds 1970). Een fout bij het schrijven
    /// (schijf vol, map weg) wordt genegeerd: het logboek mag Mondig nooit in de weg zitten.
    pub fn schrijf_op(&self, seconden: u64, gebeurtenis: &Gebeurtenis) {
        let regel = format!(
            "{} {} {}\n",
            tijdstip(seconden),
            gebeurtenis.niveau().code(),
            gebeurtenis.tekst()
        );
        let _slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = self.voeg_toe(&regel);
    }

    fn voeg_toe(&self, regel: &str) -> io::Result<()> {
        fs::create_dir_all(&self.map)?;
        let huidig = self.map.join(HUIDIG);
        let grootte = fs::metadata(&huidig).map_or(0, |m| m.len());
        if grootte > 0 && grootte.saturating_add(regel.len() as u64) > self.grens {
            // Het oude bestand verdwijnt; hernoemen vervangt het op Windows niet vanzelf.
            let oud = self.map.join(OUD);
            let _ = fs::remove_file(&oud);
            fs::rename(&huidig, &oud)?;
        }
        let mut bestand = OpenOptions::new().create(true).append(true).open(huidig)?;
        bestand.write_all(regel.as_bytes())
    }

    /// De hele inhoud, het oude bestand eerst: wat de gebruiker kopieert voor support.
    #[must_use]
    pub fn lees(&self) -> String {
        let _slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        let stuk = |naam: &str| fs::read_to_string(self.map.join(naam)).unwrap_or_default();
        let mut tekst = stuk(OUD);
        tekst.push_str(&stuk(HUIDIG));
        tekst
    }
}

static LOGBOEK: OnceLock<Logboek> = OnceLock::new();

/// Zet het logboek van dit proces op. De tweede aanroep doet niets.
pub fn start(map: PathBuf) {
    let _ = LOGBOEK.set(Logboek::nieuw(map, GRENS_BYTES));
}

/// Het logboek van dit proces, als [`start`] liep.
#[must_use]
pub fn logboek() -> Option<&'static Logboek> {
    LOGBOEK.get()
}

/// Schrijft een gebeurtenis in het logboek van dit proces. Zonder [`start`] (in tests, in de
/// opdrachtregel) gebeurt er niets.
pub fn log(gebeurtenis: &Gebeurtenis) {
    if let Some(logboek) = logboek() {
        let nu = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        logboek.schrijf_op(nu, gebeurtenis);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NU: u64 = 1_793_631_845;

    fn voorbeelden() -> Vec<Gebeurtenis> {
        let model = ModelNaam::nieuw(crate::model::STANDAARD_MODEL);
        vec![
            Gebeurtenis::AppGestart {
                versie: "0.3.0",
                besturingssysteem: "windows",
            },
            Gebeurtenis::ComputerGelezen {
                geheugen_mb: Some(16_000),
                kernen: Some(8),
                vrij_mb: None,
                videokaarten: Some(1),
            },
            Gebeurtenis::WizardStap { stap: "advies" },
            Gebeurtenis::DownloadGestart {
                model: model.clone(),
            },
            Gebeurtenis::DownloadKlaar { model },
            Gebeurtenis::TaakMislukt {
                taak: Soort::Bestand,
                probleem: Some(Probleem::SchijfVol),
            },
            Gebeurtenis::TaakMislukt {
                taak: Soort::Dictaat,
                probleem: None,
            },
            Gebeurtenis::VideokaartTerugval,
            Gebeurtenis::Snelheidstest {
                oordeel: "loont",
                keer_sneller_tienden: Some(95),
            },
            Gebeurtenis::VideokaartKeuze { aan: true },
            Gebeurtenis::InstellingenTerugGezet,
            Gebeurtenis::LogboekGekopieerd,
            Gebeurtenis::Opschonen {
                uitkomst: "letterlijk",
                reden: Some("afgekeurd"),
                detail: Some("getal_erbij"),
                duur_ms: 420,
            },
            Gebeurtenis::SlepenZonderPaden {
                beloftes: 1,
                typen: SleepbordType::lijst(&[
                    "com.apple.NSFilePromiseItemMetaData",
                    "Dit is wat iemand dicteerde!",
                    "",
                ]),
            },
            Gebeurtenis::SleephaakNiet {
                reden: "andere_klasse",
            },
            Gebeurtenis::DictaatAfgeleverd {
                route: "typen",
                uitkomst: "getypt",
            },
        ]
    }

    #[test]
    fn een_sleepbordtype_past_in_het_alfabet_van_het_logboek() {
        assert_eq!(
            Gebeurtenis::SlepenZonderPaden {
                beloftes: 0,
                typen: SleepbordType::lijst(&[
                    "public.file-url",
                    "NSFilenamesPboardType",
                    "a b=c",
                    ""
                ]),
            }
            .tekst(),
            concat!(
                "slepen_zonder_paden beloftes=0 type=public.file-url ",
                "type=nsfilenamespboardtype type=a_b_c type=leeg"
            )
        );
        // Begrensd: hooguit 80 tekens per naam en 20 namen per regel.
        let lang = "x".repeat(500);
        assert_eq!(SleepbordType::nieuw(&lang).0.len(), SleepbordType::LENGTE);
        let veel: Vec<String> = (0..50).map(|i| format!("type{i}")).collect();
        assert_eq!(SleepbordType::lijst(&veel).len(), SleepbordType::AANTAL);
    }

    #[test]
    fn het_tijdstip_klopt_op_bekende_momenten() {
        assert_eq!(tijdstip(0), "1970-01-01T00:00:00Z");
        assert_eq!(tijdstip(NU), "2026-11-02T15:04:05Z");
        // Een schrikkeldag aan het eind van de dag.
        assert_eq!(tijdstip(951_868_799), "2000-02-29T23:59:59Z");
        assert_eq!(tijdstip(951_868_800), "2000-03-01T00:00:00Z");
    }

    #[test]
    fn elke_regel_bestaat_alleen_uit_codes_en_getallen() {
        // Het teken-alfabet van een regel: kleine letters, cijfers, underscore, punt, min,
        // een spatie tussen de delen en `=` tussen naam en waarde. Geen hoofdletters en geen
        // leestekens, dus geen zin die in een veld past.
        let toegestaan = |c: char| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-' | ' ' | '=')
        };
        for gebeurtenis in voorbeelden() {
            let tekst = gebeurtenis.tekst();
            assert!(tekst.chars().all(toegestaan), "ongeldig teken in: {tekst}");
            assert!(!tekst.is_empty());
        }
    }

    #[test]
    fn een_modelnaam_buiten_de_catalogus_wordt_onbekend() {
        // Een poging om tekst via een modelnaam te loggen komt er niet door.
        let naam = ModelNaam::nieuw("Dit is wat iemand dicteerde.");
        assert_eq!(
            Gebeurtenis::DownloadKlaar { model: naam }.tekst(),
            "download_klaar model=onbekend"
        );
    }

    #[test]
    fn een_mislukte_taak_logt_de_soort_en_het_probleem() {
        let g = Gebeurtenis::TaakMislukt {
            taak: Soort::Bestand,
            probleem: Some(Probleem::SchijfVol),
        };
        assert_eq!(g.tekst(), "taak_mislukt taak=bestand probleem=schijf_vol");
        assert_eq!(g.niveau(), Niveau::Fout);
    }

    #[test]
    fn niet_te_meten_waarden_ontbreken_in_de_regel() {
        let g = Gebeurtenis::ComputerGelezen {
            geheugen_mb: None,
            kernen: Some(4),
            vrij_mb: None,
            videokaarten: None,
        };
        assert_eq!(g.tekst(), "computer_gelezen kernen=4");
    }

    #[test]
    fn een_regel_krijgt_tijd_niveau_en_code() {
        let map = tempfile::tempdir().unwrap();
        let logboek = Logboek::nieuw(map.path().join("logs"), GRENS_BYTES);
        logboek.schrijf_op(NU, &Gebeurtenis::InstellingenTerugGezet);
        logboek.schrijf_op(NU + 1, &Gebeurtenis::VideokaartTerugval);
        assert_eq!(
            logboek.lees(),
            "2026-11-02T15:04:05Z info instellingen_terug_gezet\n\
             2026-11-02T15:04:06Z fout videokaart_terugval\n"
        );
    }

    #[test]
    fn het_logboek_draait_om_en_blijft_begrensd() {
        let map = tempfile::tempdir().unwrap();
        let logboek = Logboek::nieuw(map.path().to_path_buf(), 200);
        for n in 0..100 {
            logboek.schrijf_op(NU + n, &Gebeurtenis::LogboekGekopieerd);
        }
        let grootte = |naam: &str| fs::metadata(map.path().join(naam)).map_or(0, |m| m.len());
        assert!(grootte(HUIDIG) <= 200, "huidig: {}", grootte(HUIDIG));
        assert!(grootte(OUD) <= 200, "oud: {}", grootte(OUD));
        assert!(grootte(OUD) > 0, "er moet een oud bestand zijn");
        // Er zijn nooit meer dan twee bestanden.
        assert_eq!(fs::read_dir(map.path()).unwrap().count(), 2);
        // De nieuwste regel staat achteraan, de volgorde blijft oplopend.
        let tekst = logboek.lees();
        assert!(tekst.ends_with("2026-11-02T15:05:44Z info logboek_gekopieerd\n"));
        let tijden: Vec<&str> = tekst.lines().map(|l| &l[..20]).collect();
        assert!(tijden.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn een_logboek_in_een_map_die_niet_kan_wordt_stil_overgeslagen() {
        let map = tempfile::tempdir().unwrap();
        // Een bestand waar de map hoort te staan.
        let pad = map.path().join("geen-map");
        fs::write(&pad, b"x").unwrap();
        let logboek = Logboek::nieuw(pad, GRENS_BYTES);
        logboek.schrijf_op(NU, &Gebeurtenis::LogboekGekopieerd);
        assert_eq!(logboek.lees(), "");
    }

    #[test]
    fn zonder_start_doet_log_niets() {
        // Het proces-logboek is in tests niet gestart (alleen `Logboek::nieuw` wordt gebruikt).
        log(&Gebeurtenis::LogboekGekopieerd);
    }
}
