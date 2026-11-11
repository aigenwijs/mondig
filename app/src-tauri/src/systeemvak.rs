//! Het menu van het systeemvak (N-C5, ontwerp ux-naar-9 sectie 6).
//!
//! Welke regels er staan en of ze aan staan, bepaalt [`regels`]: een pure functie van de stand
//! (`MenuStand`) en de opnametijd, zodat het zonder venster te testen is. Dit bestand bouwt er
//! daarna het echte menu van en werkt dat bij:
//!
//! - rust: "Opname starten" (aan);
//! - tijdens een opname: bovenaan een uitgeschakelde statusregel "Opname loopt · 12:35" en
//!   daaronder "Stoppen en uitschrijven";
//! - tijdens het openen van de bronnen, het voorbereiden en het uitschrijven: "Opname starten"
//!   uitgeschakeld, want de app weigert dan een nieuwe opname;
//! - bij een open verslag: "Opname starten" sluit het verslag eerst (zonder wissen). Lukt dat niet
//!   (bewaren mislukt, opnieuw uitschrijven loopt), dan staat hij uit met de reden in de tekst.
//!
//! Een andere stand bouwt het menu opnieuw; alleen de tijd in de statusregel wordt ter plekke
//! aangepast (elke paar seconden, zoals de tooltip).

use std::sync::Mutex;

use serde::Deserialize;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Manager, Wry};

/// Waar de opnamefunctie van de app nu staat, voor het menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum OpnameStand {
    /// Er gebeurt niets: een nieuwe opname kan beginnen.
    #[default]
    Rust,
    /// De opname loopt.
    Loopt,
    /// De app is met een opname bezig (bronnen openen, voorbereiden, uitschrijven): een nieuwe
    /// opname kan niet.
    Bezig,
    /// Een verslag staat open en is te sluiten (zonder wissen): "Opname starten" sluit het eerst.
    VerslagOpen,
    /// Een verslag staat open maar sluiten lukt nu niet (bewaren mislukt of opnieuw uitschrijven
    /// loopt): "Opname starten" blijft uit, met de reden in de tekst.
    VerslagVast,
}

/// Alles waarvan de opbouw van het menu afhangt (de tijd niet: die wordt ter plekke bijgewerkt).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct MenuStand {
    pub opname: OpnameStand,
    /// Is er een laatste dictaat om te kopiëren?
    pub kopieer_aan: bool,
}

/// De teksten van het menu in de taal van de app. Rust kent die taal niet (`taal.rs`), dus de
/// interface geeft ze door, bij het starten en bij een taalwissel, net als de sprekernamen
/// (`vergadering_zet_labels`). Zo staan alle vertalingen op één plek (`nl.ts` en `en.ts`). Tot de
/// interface ze doorgeeft, geldt Nederlands.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Labels {
    pub openen: String,
    pub opname_starten: String,
    /// "Opname starten (eerst het verslag afmaken)": de reden dat starten nu niet kan.
    pub opname_starten_eerst_afmaken: String,
    /// Het begin van de statusregel: "Opname loopt", gevolgd door " \u{b7} 12:35".
    pub opname_loopt: String,
    pub stoppen: String,
    pub kopieer: String,
    pub map: String,
    pub instellingen: String,
    pub over: String,
    pub afsluiten: String,
    /// De tooltip van het pictogram tijdens een opname, per soort. `{tijd}` wordt de opnametijd:
    /// "Mondig neemt een vergadering op ({tijd})".
    pub tip_online: String,
    pub tip_ruimte: String,
    pub tip_alleen: String,
}

impl Default for Labels {
    fn default() -> Self {
        let s = str::to_string;
        Self {
            openen: s("Mondig openen"),
            opname_starten: s("Opname starten"),
            opname_starten_eerst_afmaken: s("Opname starten (eerst het verslag afmaken)"),
            opname_loopt: s("Opname loopt"),
            stoppen: s("Stoppen en uitschrijven"),
            kopieer: s("Laatste dictaat kopiëren"),
            map: s("Map openen"),
            instellingen: s("Instellingen"),
            over: s("Over Mondig"),
            afsluiten: s("Afsluiten"),
            tip_online: s("Mondig neemt een vergadering op ({tijd})"),
            tip_ruimte: s("Mondig neemt een vergadering in de ruimte op ({tijd})"),
            tip_alleen: s("Mondig neemt een notitie op ({tijd})"),
        }
    }
}

/// Eén regel van het menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Regel {
    Item {
        id: &'static str,
        tekst: String,
        aan: bool,
    },
    Scheiding,
}

pub(crate) const ID_OPENEN: &str = "openen";
pub(crate) const ID_OPNAME_START: &str = "opname_start";
pub(crate) const ID_OPNAME_STOP: &str = "opname_stop";
pub(crate) const ID_OPNAME_STATUS: &str = "opname_status";
pub(crate) const ID_KOPIEER: &str = "kopieer";
pub(crate) const ID_MAP: &str = "map";
pub(crate) const ID_INSTELLINGEN: &str = "instellingen";
pub(crate) const ID_OVER: &str = "over";
pub(crate) const ID_AFSLUITEN: &str = "afsluiten";

/// De tekst van de statusregel tijdens een opname: "Opname loopt · 12:35".
pub(crate) fn status_tekst(labels: &Labels, tijd: &str) -> String {
    format!("{} \u{b7} {tijd}", labels.opname_loopt)
}

fn item(id: &'static str, tekst: &str, aan: bool) -> Regel {
    Regel::Item {
        id,
        tekst: tekst.to_string(),
        aan,
    }
}

/// De regels van het menu voor deze stand. `tijd` is de verstreken opnametijd ("12:35"), alleen
/// van belang tijdens een opname.
pub(crate) fn regels(stand: MenuStand, tijd: &str, labels: &Labels) -> Vec<Regel> {
    let mut lijst = vec![item(ID_OPENEN, &labels.openen, true), Regel::Scheiding];
    match stand.opname {
        OpnameStand::Rust | OpnameStand::VerslagOpen => {
            lijst.push(item(ID_OPNAME_START, &labels.opname_starten, true));
        }
        OpnameStand::Bezig => lijst.push(item(ID_OPNAME_START, &labels.opname_starten, false)),
        OpnameStand::VerslagVast => lijst.push(item(
            ID_OPNAME_START,
            &labels.opname_starten_eerst_afmaken,
            false,
        )),
        OpnameStand::Loopt => {
            lijst.push(Regel::Item {
                id: ID_OPNAME_STATUS,
                tekst: status_tekst(labels, tijd),
                aan: false,
            });
            lijst.push(item(ID_OPNAME_STOP, &labels.stoppen, true));
        }
    }
    lijst.push(item(ID_KOPIEER, &labels.kopieer, stand.kopieer_aan));
    lijst.push(Regel::Scheiding);
    lijst.push(item(ID_MAP, &labels.map, true));
    lijst.push(item(ID_INSTELLINGEN, &labels.instellingen, true));
    lijst.push(item(ID_OVER, &labels.over, true));
    lijst.push(Regel::Scheiding);
    lijst.push(item(ID_AFSLUITEN, &labels.afsluiten, true));
    lijst
}

/// Wat het systeemvak nu toont, als Tauri-staat.
///
/// Regel voor dit slot: nooit een menuaanroep van Tauri (`MenuItem::with_id`, `set_menu`,
/// `set_text`) terwijl het slot vastligt. Zulke aanroepen wachten op de hoofddraad, en een
/// synchroon commando op de hoofddraad (bijvoorbeeld `dictaat_verwijder`, via `werk_tray_bij`) wil
/// op zijn beurt dit slot: dan hangt de app. Onder het slot alleen de stand aanpassen en een
/// kopie nemen; het menu bouwen en zetten gebeurt erna.
#[derive(Default)]
pub(crate) struct TrayStaat(Mutex<Binnen>);

#[derive(Default)]
struct Binnen {
    boek: Boek,
    /// De statusregel, zolang die in het menu staat.
    status: Option<MenuItem<Wry>>,
}

/// De boekhouding zonder Tauri-typen, zodat de tests het uitvoerbare bestand niet aan menucode
/// koppelen (die op Windows een manifest voor de gewone testprogramma's mist).
#[derive(Default)]
struct Boek {
    stand: MenuStand,
    tijd: String,
    labels: Labels,
    /// De versie van de vergaderstand waar `stand.opname` bij hoort: een oudere melding, die na
    /// het loslaten van het slot van de vergadering te laat aankomt, wordt genegeerd.
    versie: u64,
    /// Telt op bij elke andere opbouw van het menu: een bouwer die achterliep zet zijn menu niet.
    opbouw: u64,
}

impl Boek {
    /// Past de opnamestand aan. Geeft `true` als het menu opnieuw gebouwd moet worden. Een oudere
    /// versie verandert niets.
    fn zet_opname(&mut self, opname: OpnameStand, tijd: &str, versie: u64) -> bool {
        if versie < self.versie {
            return false;
        }
        self.versie = versie;
        self.tijd = tijd.to_string();
        if self.stand.opname == opname {
            return false;
        }
        self.stand.opname = opname;
        self.opbouw += 1;
        true
    }

    /// Zet de teksten in een andere taal; `true` als het menu opnieuw gebouwd moet worden.
    fn zet_labels(&mut self, labels: Labels) -> bool {
        if self.labels == labels {
            return false;
        }
        self.labels = labels;
        self.opbouw += 1;
        true
    }

    /// Past "Laatste dictaat kopiëren" aan; `true` als het menu opnieuw gebouwd moet worden.
    fn zet_kopieer(&mut self, aan: bool) -> bool {
        if self.stand.kopieer_aan == aan {
            return false;
        }
        self.stand.kopieer_aan = aan;
        self.opbouw += 1;
        true
    }
}

fn slot(staat: &TrayStaat) -> std::sync::MutexGuard<'_, Binnen> {
    staat
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Bouwt het menu voor een stand. Raakt het slot niet.
fn bouw(
    app: &AppHandle,
    stand: MenuStand,
    tijd: &str,
    labels: &Labels,
) -> tauri::Result<(Menu<Wry>, Option<MenuItem<Wry>>)> {
    let menu = Menu::new(app)?;
    let mut status = None;
    for regel in regels(stand, tijd, labels) {
        match regel {
            Regel::Scheiding => menu.append(&PredefinedMenuItem::separator(app)?)?,
            Regel::Item { id, tekst, aan } => {
                let nieuw = MenuItem::with_id(app, id, tekst, aan, None::<&str>)?;
                menu.append(&nieuw as &dyn IsMenuItem<Wry>)?;
                if id == ID_OPNAME_STATUS {
                    status = Some(nieuw);
                }
            }
        }
    }
    Ok((menu, status))
}

/// Eén tekst in de taal van de app, voor wat buiten het menu staat (de tooltip tijdens een
/// opname). `kies` wijst de tekst aan; alleen die wordt gekopieerd, zodat de tikker niet elke
/// paar seconden alle teksten onder het slot kloont. Zonder staat (de tray bestaat nog niet) geldt
/// Nederlands.
pub(crate) fn label(app: &AppHandle, kies: impl FnOnce(&Labels) -> &String) -> String {
    match app.try_state::<TrayStaat>() {
        Some(staat) => kies(&slot(&staat).boek.labels).clone(),
        None => kies(&Labels::default()).clone(),
    }
}

/// Een kopie van wat het menu nodig heeft, onder het slot genomen.
fn momentopname(staat: &TrayStaat) -> (MenuStand, String, Labels, u64) {
    let binnen = slot(staat);
    (
        binnen.boek.stand,
        binnen.boek.tijd.clone(),
        binnen.boek.labels.clone(),
        binnen.boek.opbouw,
    )
}

/// Het eerste menu, bij het bouwen van het systeemvakicoon.
pub(crate) fn maak_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let staat = app.state::<TrayStaat>();
    let (stand, tijd, labels, opbouw) = momentopname(&staat);
    let (menu, status) = bouw(app, stand, &tijd, &labels)?;
    let mut binnen = slot(&staat);
    if binnen.boek.opbouw == opbouw {
        binnen.status = status;
    }
    drop(binnen);
    Ok(menu)
}

/// Bouwt het menu opnieuw en zet het, zonder dat het slot vastligt tijdens de menuaanroepen.
fn ververs(app: &AppHandle, staat: &TrayStaat) {
    let (stand, tijd, labels, opbouw) = momentopname(staat);
    let Some(tray) = app.tray_by_id("hoofd") else {
        return;
    };
    let Ok((menu, status)) = bouw(app, stand, &tijd, &labels) else {
        return;
    };
    {
        let mut binnen = slot(staat);
        if binnen.boek.opbouw != opbouw {
            // Intussen is er een nieuwere stand; die bouwt zijn eigen menu.
            return;
        }
        binnen.status = status;
    }
    let _ = tray.set_menu(Some(menu));
}

/// De opnamestand is veranderd, met de versie van de vergaderstand waar ze bij hoort.
pub(crate) fn zet_opname(app: &AppHandle, opname: OpnameStand, tijd: &str, versie: u64) {
    let Some(staat) = app.try_state::<TrayStaat>() else {
        return;
    };
    let nodig = slot(&staat).boek.zet_opname(opname, tijd, versie);
    if nodig {
        ververs(app, &staat);
    }
}

/// "Laatste dictaat kopiëren" aan of uit.
pub(crate) fn zet_kopieer(app: &AppHandle, aan: bool) {
    let Some(staat) = app.try_state::<TrayStaat>() else {
        return;
    };
    let nodig = slot(&staat).boek.zet_kopieer(aan);
    if nodig {
        ververs(app, &staat);
    }
}

/// Werkt de tijd in de statusregel bij, zonder het menu opnieuw te bouwen.
pub(crate) fn zet_tijd(app: &AppHandle, tijd: &str) {
    let Some(staat) = app.try_state::<TrayStaat>() else {
        return;
    };
    let (status, tekst) = {
        let mut binnen = slot(&staat);
        binnen.boek.tijd = tijd.to_string();
        (
            binnen.status.clone(),
            status_tekst(&binnen.boek.labels, tijd),
        )
    };
    if let Some(status) = status {
        let _ = status.set_text(tekst);
    }
}

/// Geeft het menu de teksten in de taal van de app. De interface roept dit aan bij het starten
/// en als de taal wisselt; een wissel bouwt het menu opnieuw.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de AppHandle per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn systeemvak_zet_labels(app: AppHandle, labels: Labels) {
    let Some(staat) = app.try_state::<TrayStaat>() else {
        return;
    };
    let nodig = slot(&staat).boek.zet_labels(labels);
    if nodig {
        ververs(&app, &staat);
    }
}

#[cfg(test)]
mod tests_stand {
    use super::*;

    #[test]
    fn een_open_verslag_laat_starten_toe_of_zegt_waarom_niet() {
        let open = regels(
            MenuStand {
                opname: OpnameStand::VerslagOpen,
                kopieer_aan: false,
            },
            "00:00",
            &Labels::default(),
        );
        assert!(open.contains(&Regel::Item {
            id: ID_OPNAME_START,
            tekst: "Opname starten".to_string(),
            aan: true
        }));
        let vast = regels(
            MenuStand {
                opname: OpnameStand::VerslagVast,
                kopieer_aan: false,
            },
            "00:00",
            &Labels::default(),
        );
        assert!(vast.contains(&Regel::Item {
            id: ID_OPNAME_START,
            tekst: "Opname starten (eerst het verslag afmaken)".to_string(),
            aan: false
        }));
    }

    #[test]
    fn een_oudere_stand_verandert_niets_en_een_nieuwe_bouwt_het_menu_opnieuw() {
        let mut binnen = Boek::default();
        assert!(binnen.zet_opname(OpnameStand::Loopt, "00:01", 5));
        // Te laat aangekomen: versie 4 hoort bij een eerdere stand.
        assert!(!binnen.zet_opname(OpnameStand::Rust, "00:00", 4));
        assert_eq!(binnen.stand.opname, OpnameStand::Loopt);
        assert_eq!(binnen.tijd, "00:01");
        // Dezelfde stand met een nieuwere versie: alleen de tijd, geen nieuw menu.
        assert!(!binnen.zet_opname(OpnameStand::Loopt, "00:09", 6));
        assert_eq!(binnen.tijd, "00:09");
        assert!(binnen.zet_opname(OpnameStand::Bezig, "00:00", 7));
        assert!(binnen.zet_kopieer(true));
        assert!(!binnen.zet_kopieer(true));
        assert_eq!(binnen.opbouw, 3);
    }

    fn engels() -> Labels {
        Labels {
            openen: "Open Mondig".to_string(),
            opname_starten: "Start recording".to_string(),
            opname_starten_eerst_afmaken: "Start recording (finish the report first)".to_string(),
            opname_loopt: "Recording".to_string(),
            stoppen: "Stop and transcribe".to_string(),
            kopieer: "Copy last dictation".to_string(),
            map: "Open folder".to_string(),
            instellingen: "Settings".to_string(),
            over: "About Mondig".to_string(),
            afsluiten: "Quit".to_string(),
            tip_online: "Mondig is recording a meeting ({tijd})".to_string(),
            tip_ruimte: "Mondig is recording an in-room meeting ({tijd})".to_string(),
            tip_alleen: "Mondig is recording a note ({tijd})".to_string(),
        }
    }

    #[test]
    fn een_taalwissel_bouwt_het_menu_opnieuw_en_dezelfde_taal_niet() {
        let mut binnen = Boek::default();
        // Nederlands is de standaard, dus dezelfde teksten doorgeven bouwt niets opnieuw.
        assert!(!binnen.zet_labels(Labels::default()));
        assert!(binnen.zet_labels(engels()));
        assert!(!binnen.zet_labels(engels()));
        assert_eq!(binnen.opbouw, 1);
        assert_eq!(
            status_tekst(&binnen.labels, "0:05"),
            "Recording \u{b7} 0:05"
        );
    }

    #[test]
    fn het_menu_volgt_de_doorgegeven_teksten() {
        let lijst = regels(
            MenuStand {
                opname: OpnameStand::Loopt,
                kopieer_aan: true,
            },
            "12:35",
            &engels(),
        );
        let teksten: Vec<String> = lijst
            .iter()
            .filter_map(|r| match r {
                Regel::Item { tekst, .. } => Some(tekst.clone()),
                Regel::Scheiding => None,
            })
            .collect();
        assert_eq!(
            teksten,
            [
                "Open Mondig",
                "Recording \u{b7} 12:35",
                "Stop and transcribe",
                "Copy last dictation",
                "Open folder",
                "Settings",
                "About Mondig",
                "Quit"
            ]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(lijst: &[Regel]) -> Vec<&'static str> {
        lijst
            .iter()
            .map(|r| match r {
                Regel::Item { id, .. } => *id,
                Regel::Scheiding => "-",
            })
            .collect()
    }

    fn aan(lijst: &[Regel], gezocht: &str) -> bool {
        lijst
            .iter()
            .find_map(|r| match r {
                Regel::Item { id, aan, .. } if *id == gezocht => Some(*aan),
                _ => None,
            })
            .expect(gezocht)
    }

    fn tekst(lijst: &[Regel], gezocht: &str) -> String {
        lijst
            .iter()
            .find_map(|r| match r {
                Regel::Item { id, tekst, .. } if *id == gezocht => Some(tekst.clone()),
                _ => None,
            })
            .expect(gezocht)
    }

    fn stand(opname: OpnameStand, kopieer_aan: bool) -> MenuStand {
        MenuStand {
            opname,
            kopieer_aan,
        }
    }

    #[test]
    fn in_rust_staat_opname_starten_onder_mondig_openen() {
        let lijst = regels(stand(OpnameStand::Rust, false), "00:00", &Labels::default());
        assert_eq!(
            ids(&lijst),
            [
                "openen",
                "-",
                "opname_start",
                "kopieer",
                "-",
                "map",
                "instellingen",
                "over",
                "-",
                "afsluiten"
            ]
        );
        assert!(aan(&lijst, ID_OPNAME_START));
        assert_eq!(tekst(&lijst, ID_OPNAME_START), "Opname starten");
        // Zonder laatste dictaat is kopiëren uit.
        assert!(!aan(&lijst, ID_KOPIEER));
    }

    #[test]
    fn tijdens_een_opname_staat_de_status_bovenaan_en_is_ze_uitgeschakeld() {
        let lijst = regels(stand(OpnameStand::Loopt, true), "12:35", &Labels::default());
        assert_eq!(
            ids(&lijst),
            [
                "openen",
                "-",
                "opname_status",
                "opname_stop",
                "kopieer",
                "-",
                "map",
                "instellingen",
                "over",
                "-",
                "afsluiten"
            ]
        );
        assert_eq!(tekst(&lijst, ID_OPNAME_STATUS), "Opname loopt \u{b7} 12:35");
        assert!(!aan(&lijst, ID_OPNAME_STATUS));
        assert_eq!(tekst(&lijst, ID_OPNAME_STOP), "Stoppen en uitschrijven");
        assert!(aan(&lijst, ID_OPNAME_STOP));
        // Er staat dan geen tweede manier om te starten.
        assert!(!ids(&lijst).contains(&ID_OPNAME_START));
        assert!(aan(&lijst, ID_KOPIEER));
    }

    #[test]
    fn tijdens_het_uitschrijven_of_voorbereiden_is_opname_starten_uitgeschakeld() {
        let lijst = regels(
            stand(OpnameStand::Bezig, false),
            "00:00",
            &Labels::default(),
        );
        assert!(!aan(&lijst, ID_OPNAME_START));
        assert_eq!(tekst(&lijst, ID_OPNAME_START), "Opname starten");
        assert!(!ids(&lijst).contains(&ID_OPNAME_STOP));
        assert!(!ids(&lijst).contains(&ID_OPNAME_STATUS));
        // De rest van het menu blijft bruikbaar.
        for id in [ID_OPENEN, ID_MAP, ID_INSTELLINGEN, ID_OVER, ID_AFSLUITEN] {
            assert!(aan(&lijst, id), "{id}");
        }
    }

    #[test]
    fn de_tijd_boven_het_uur_heeft_uren() {
        assert_eq!(
            status_tekst(&Labels::default(), "1:02:03"),
            "Opname loopt \u{b7} 1:02:03"
        );
    }
}
