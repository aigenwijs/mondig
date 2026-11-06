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
pub(crate) fn status_tekst(tijd: &str) -> String {
    format!("Opname loopt \u{b7} {tijd}")
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
pub(crate) fn regels(stand: MenuStand, tijd: &str) -> Vec<Regel> {
    let mut lijst = vec![item(ID_OPENEN, "Mondig openen", true), Regel::Scheiding];
    match stand.opname {
        OpnameStand::Rust | OpnameStand::VerslagOpen => {
            lijst.push(item(ID_OPNAME_START, "Opname starten", true));
        }
        OpnameStand::Bezig => lijst.push(item(ID_OPNAME_START, "Opname starten", false)),
        OpnameStand::VerslagVast => lijst.push(item(
            ID_OPNAME_START,
            "Opname starten (eerst het verslag afmaken)",
            false,
        )),
        OpnameStand::Loopt => {
            lijst.push(Regel::Item {
                id: ID_OPNAME_STATUS,
                tekst: status_tekst(tijd),
                aan: false,
            });
            lijst.push(item(ID_OPNAME_STOP, "Stoppen en uitschrijven", true));
        }
    }
    lijst.push(item(
        ID_KOPIEER,
        "Laatste dictaat kopiëren",
        stand.kopieer_aan,
    ));
    lijst.push(Regel::Scheiding);
    lijst.push(item(ID_MAP, "Map openen", true));
    lijst.push(item(ID_INSTELLINGEN, "Instellingen", true));
    lijst.push(item(ID_OVER, "Over Mondig", true));
    lijst.push(Regel::Scheiding);
    lijst.push(item(ID_AFSLUITEN, "Afsluiten", true));
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
) -> tauri::Result<(Menu<Wry>, Option<MenuItem<Wry>>)> {
    let menu = Menu::new(app)?;
    let mut status = None;
    for regel in regels(stand, tijd) {
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

/// Een kopie van wat het menu nodig heeft, onder het slot genomen.
fn momentopname(staat: &TrayStaat) -> (MenuStand, String, u64) {
    let binnen = slot(staat);
    (
        binnen.boek.stand,
        binnen.boek.tijd.clone(),
        binnen.boek.opbouw,
    )
}

/// Het eerste menu, bij het bouwen van het systeemvakicoon.
pub(crate) fn maak_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let staat = app.state::<TrayStaat>();
    let (stand, tijd, opbouw) = momentopname(&staat);
    let (menu, status) = bouw(app, stand, &tijd)?;
    let mut binnen = slot(&staat);
    if binnen.boek.opbouw == opbouw {
        binnen.status = status;
    }
    drop(binnen);
    Ok(menu)
}

/// Bouwt het menu opnieuw en zet het, zonder dat het slot vastligt tijdens de menuaanroepen.
fn ververs(app: &AppHandle, staat: &TrayStaat) {
    let (stand, tijd, opbouw) = momentopname(staat);
    let Some(tray) = app.tray_by_id("hoofd") else {
        return;
    };
    let Ok((menu, status)) = bouw(app, stand, &tijd) else {
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
    let status = {
        let mut binnen = slot(&staat);
        binnen.boek.tijd = tijd.to_string();
        binnen.status.clone()
    };
    if let Some(status) = status {
        let _ = status.set_text(status_tekst(tijd));
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
        let lijst = regels(stand(OpnameStand::Rust, false), "00:00");
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
        let lijst = regels(stand(OpnameStand::Loopt, true), "12:35");
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
        let lijst = regels(stand(OpnameStand::Bezig, false), "00:00");
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
        assert_eq!(status_tekst("1:02:03"), "Opname loopt \u{b7} 1:02:03");
    }
}
