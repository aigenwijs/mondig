//! De sneltoets voor dicteren, zonder echte toetsen (sprint 3b, ADR 0012).
//!
//! Alles wat te beslissen valt staat hier als gewone functies en een kleine
//! toestandsmachine, zodat het te testen is zonder toetsenbord of venster. De Tauri-schil
//! vertaalt alleen toetsgebeurtenissen naar deze aanroepen en voert de [`Actie`] uit.
//!
//! - **Houden of tikken** (zoals het houden-of-wisselen van Handy): een druk van minstens
//!   [`HOUD_DREMPEL`] is houden, dus opnemen tot loslaten. Korter is tikken: de opname
//!   loopt door tot de volgende druk.
//! - **Terugvallijst**: lukt de gewenste toets niet te registreren (bijvoorbeeld omdat een
//!   ander programma hem al heeft), dan de volgende uit [`TERUGVAL_SNELTOETSEN`].
//!
//! - **Twee toetsen** (ADR 0027 punt 1): naast de gewone toets kan er een tweede zijn voor een
//!   opgeschoond dictaat. Er loopt hooguit één dictaat tegelijk; zie [`Toets`].
//!
//! Windows stuurt bij een ingedrukte toets herhaalde "ingedrukt"-gebeurtenissen en, per
//! herhaling, ook een "losgelaten"-gebeurtenis zodra de toets los is. De machine houdt
//! daarom bij of de toets nu vast is en negeert herhalingen.

use std::time::{Duration, Instant};

use serde::Serialize;

/// De sneltoets die Mondig standaard probeert (besluit, 1-11-2026).
pub const STANDAARD_SNELTOETS: &str = "Ctrl+Space";

/// De standaard voor de tweede sneltoets, die een opgeschoond dictaat geeft (ADR 0027 punt 1).
/// Daarvoor is er geen terugvallijst: is hij bezet, of dezelfde als de gewone toets, dan staat
/// hij uit tot de gebruiker een andere kiest. De gewone toets gaat altijd voor.
pub const STANDAARD_OPSCHOONTOETS: &str = "Ctrl+Shift+Space";

/// Wat Mondig probeert als de gewenste toets niet te registreren is, in volgorde.
pub const TERUGVAL_SNELTOETSEN: [&str; 2] = ["Ctrl+Alt+Space", "Ctrl+Shift+Space"];

/// Een druk van minstens zoveel is houden (opnemen tot loslaten); korter is tikken.
pub const HOUD_DREMPEL: Duration = Duration::from_millis(300);

/// Na loslaten loopt de opname nog even door, zodat het laatste woord niet wegvalt. Een
/// eigen keuze: Handy loopt standaard niet door (plan, logboek 1-11-2026).
pub const NALOOPTIJD: Duration = Duration::from_millis(300);

/// Welke toets het uiteindelijk werd.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registratie {
    /// De geregistreerde toets, in de schrijfwijze van de plugin (bijvoorbeeld `Ctrl+Space`).
    pub toets: String,
    /// Is dit een terugvaloptie in plaats van de gewenste toets?
    pub is_terugval: bool,
}

/// Probeert de gewenste toets en daarna de terugvallijst te registreren. `probeer`
/// registreert één toets en geeft een foutmelding bij mislukken. Een toets die al
/// geprobeerd is wordt niet nog eens geprobeerd.
///
/// # Errors
///
/// Geeft per geprobeerde toets de foutmelding als geen enkele lukte.
pub fn registreer_met_terugval(
    gewenst: &str,
    mut probeer: impl FnMut(&str) -> Result<(), String>,
) -> Result<Registratie, Vec<(String, String)>> {
    let mut geprobeerd: Vec<&str> = Vec::new();
    let mut fouten = Vec::new();
    for (volgorde, kandidaat) in std::iter::once(gewenst)
        .chain(TERUGVAL_SNELTOETSEN)
        .enumerate()
    {
        if geprobeerd.contains(&kandidaat) {
            continue;
        }
        geprobeerd.push(kandidaat);
        match probeer(kandidaat) {
            Ok(()) => {
                return Ok(Registratie {
                    toets: kandidaat.to_string(),
                    is_terugval: volgorde > 0,
                });
            }
            Err(melding) => fouten.push((kandidaat.to_string(), melding)),
        }
    }
    Err(fouten)
}

/// Registreert de opgeslagen sneltoets bij de start. De toets gaat eerst door
/// [`normaliseer_sneltoets`] en [`controleer_sneltoets`]. Mag hij niet (bijvoorbeeld Pause uit
/// een oudere versie, of een gereserveerde combinatie), dan wordt hij niet geregistreerd en
/// begint de keten bij [`STANDAARD_SNELTOETS`], met `is_terugval` aan zodat het startscherm
/// meldt dat de gewenste toets niet gebruikt kon worden. Een waarschuwing (F1 tot en met F12)
/// is goed: de gebruiker heeft die toets eerder bevestigd.
///
/// # Errors
///
/// Geeft per geprobeerde toets de foutmelding als geen enkele lukte.
pub fn registreer_opgeslagen(
    opgeslagen: &str,
    probeer: impl FnMut(&str) -> Result<(), String>,
) -> Result<Registratie, Vec<(String, String)>> {
    let genormaliseerd = normaliseer_sneltoets(opgeslagen);
    let ongeldig = matches!(controleer_sneltoets(&genormaliseerd), Oordeel::Fout { .. });
    let basis = if ongeldig {
        STANDAARD_SNELTOETS
    } else {
        genormaliseerd.as_str()
    };
    registreer_met_terugval(basis, probeer).map(|registratie| Registratie {
        is_terugval: registratie.is_terugval || ongeldig,
        ..registratie
    })
}

/// De toets in gewone taal voor de gebruiker: `Ctrl+Space` wordt `Ctrl+Spatie`, `Ctrl+KeyA`
/// wordt `Ctrl+A`.
#[must_use]
pub fn toets_label(toets: &str) -> String {
    toets
        .split('+')
        .map(|deel| deel_label(deel.trim()))
        .collect::<Vec<_>>()
        .join("+")
}

fn deel_label(deel: &str) -> String {
    let klein = deel.to_lowercase();
    match klein.as_str() {
        "space" => return "Spatie".to_string(),
        "esc" | "escape" => return "Esc".to_string(),
        "ctrl" | "control" => return "Ctrl".to_string(),
        "alt" | "option" => return "Alt".to_string(),
        "shift" => return "Shift".to_string(),
        "scrolllock" => return "Scroll Lock".to_string(),
        "pageup" => return "Page Up".to_string(),
        "pagedown" => return "Page Down".to_string(),
        "arrowup" | "up" => return "Pijl omhoog".to_string(),
        "arrowdown" | "down" => return "Pijl omlaag".to_string(),
        "arrowleft" | "left" => return "Pijl links".to_string(),
        "arrowright" | "right" => return "Pijl rechts".to_string(),
        _ => {}
    }
    if let Some(letter) = klein.strip_prefix("key")
        && letter.len() == 1
    {
        return letter.to_uppercase();
    }
    if let Some(cijfer) = klein.strip_prefix("digit")
        && cijfer.len() == 1
    {
        return cijfer.to_string();
    }
    if let Some(cijfer) = klein.strip_prefix("numpad")
        && cijfer.len() == 1
    {
        return format!("Num {cijfer}");
    }
    deel.to_string()
}

/// Waarom een sneltoets niet kan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SneltoetsFout {
    /// Er is niets opgegeven, of er staat een leeg stuk in.
    Leeg,
    /// De Windows-toets (of Cmd) is gereserveerd door het besturingssysteem.
    WindowsToets,
    /// Een toets zonder Ctrl of Alt die ook voor typen of navigeren dient (letters, cijfers,
    /// Spatie, pijltjes, Delete, Home, End, Page Up/Down, Num-cijfers). Globaal weggekaapt
    /// zou typen of navigeren in andere programma's niet meer werken.
    LosseToets,
    /// Een toets met alleen Shift erbij (zonder Ctrl of Alt).
    ShiftAlleen,
    /// Alleen toetsen als Ctrl en Shift, geen echte toets erbij.
    GeenHoofdtoets,
    /// Meer dan één echte toets (Ctrl+A+B).
    MeerdereHoofdtoetsen,
    /// Esc is voor annuleren.
    EscNietToegestaan,
    /// Een combinatie die andere programma's of Windows zelf nodig hebben (Ctrl+C, Alt+F4).
    Gereserveerd,
    /// Een toets die de sneltoetsplugin niet kent (Enter, Tab, Backspace, mediatoetsen).
    Onbekend,
    /// Pause: Windows meldt bij Pause geen echte loslaat-gebeurtenis (indrukken en loslaten
    /// komen tegelijk), dus houden-om-te-praten werkt niet. Met Ctrl wordt het `VK_CANCEL`
    /// (Break), dat nooit als sneltoets afgaat.
    WerktNiet,
}

/// Soort waarschuwing: de sneltoets mag, maar pas na bevestiging door de gebruiker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WaarschuwingSoort {
    /// F1 tot en met F12 zonder Ctrl of Alt: andere programma's gebruiken ze (F5 vernieuwt,
    /// F11 is volledig scherm).
    Functietoets,
}

/// Het oordeel over een sneltoets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "uitkomst", rename_all = "camelCase")]
pub enum Oordeel {
    /// De sneltoets mag zonder meer.
    Goed,
    /// De sneltoets mag na bevestiging. `toets` is de gewone toets (`F5`).
    Waarschuwing {
        /// Het soort waarschuwing.
        soort: WaarschuwingSoort,
        /// De toets waar de waarschuwing over gaat.
        toets: String,
    },
    /// De sneltoets mag niet.
    Fout {
        /// Waarom niet.
        fout: SneltoetsFout,
    },
}

/// Combinaties die andere programma's of Windows zelf nodig hebben, in kleine letters en de
/// volgorde Ctrl, Alt, Shift, gewone toets.
const GERESERVEERD: [&str; 18] = [
    "ctrl+keyc",
    "ctrl+keyv",
    "ctrl+keyx",
    "ctrl+keyz",
    "ctrl+keyy",
    "ctrl+keya",
    "ctrl+keys",
    "ctrl+keyp",
    "ctrl+keyf",
    "ctrl+shift+keyz",
    "ctrl+shift+keys",
    "ctrl+shift+keyv",
    "alt+f4",
    "alt+tab",
    "ctrl+alt+delete",
    "ctrl+shift+escape",
    // Plakken en kopieren via Insert, ook in Windows zelf.
    "shift+insert",
    "ctrl+insert",
];

/// Het nummer van een functietoets (`f5` is 5), of `None` als het er geen is.
fn functienummer(hoofd: &str) -> Option<u8> {
    let cijfers = hoofd.strip_prefix('f')?;
    if cijfers.starts_with('0') || !cijfers.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    cijfers.parse::<u8>().ok().filter(|n| (1..=24).contains(n))
}

/// Is dit `voorvoegsel` plus precies één teken uit de reeks?
fn is_enkel(hoofd: &str, voorvoegsel: &str, reeks: fn(&u8) -> bool) -> bool {
    hoofd
        .strip_prefix(voorvoegsel)
        .is_some_and(|rest| rest.len() == 1 && rest.bytes().all(|b| reeks(&b)))
}

/// Kent de plugin deze gewone toets (kleine letters)? Dezelfde namen als
/// `KeyboardEvent.code`, zonder Esc, Enter, Tab en Backspace: Esc annuleert het vangen, de
/// rest hoort bij typen.
fn is_bekende_hoofdtoets(hoofd: &str) -> bool {
    functienummer(hoofd).is_some()
        || is_enkel(hoofd, "key", u8::is_ascii_lowercase)
        || is_enkel(hoofd, "digit", u8::is_ascii_digit)
        || is_enkel(hoofd, "numpad", u8::is_ascii_digit)
        || matches!(
            hoofd,
            "space"
                | "backquote"
                | "backslash"
                | "bracketleft"
                | "bracketright"
                | "comma"
                | "equal"
                | "minus"
                | "period"
                | "quote"
                | "semicolon"
                | "slash"
                | "insert"
                | "delete"
                | "home"
                | "end"
                | "pageup"
                | "pagedown"
                | "scrolllock"
                | "arrowup"
                | "arrowdown"
                | "arrowleft"
                | "arrowright"
        )
}

/// De canonieke schrijfwijze van een gewone toets (`KeyboardEvent.code`) voor een alias die
/// de plugin ook accepteert (`a`, `up`, `num5`, `esc`), of `None` als het geen alias of
/// bekende naam is. Bron: `parse_key` in global-hotkey 0.8.0 (`hotkey.rs`).
fn canonieke_hoofdtoets(klein: &str) -> Option<String> {
    let mut tekens = klein.chars();
    if let (Some(enig), None) = (tekens.next(), tekens.next()) {
        match enig {
            'a'..='z' => return Some(format!("Key{}", enig.to_ascii_uppercase())),
            '0'..='9' => return Some(format!("Digit{enig}")),
            '`' => return Some("Backquote".into()),
            '\\' => return Some("Backslash".into()),
            '[' => return Some("BracketLeft".into()),
            ']' => return Some("BracketRight".into()),
            ',' => return Some("Comma".into()),
            '=' => return Some("Equal".into()),
            '-' => return Some("Minus".into()),
            '.' => return Some("Period".into()),
            '\'' => return Some("Quote".into()),
            ';' => return Some("Semicolon".into()),
            '/' => return Some("Slash".into()),
            _ => {}
        }
    }
    if let Some(nummer) = functienummer(klein) {
        return Some(format!("F{nummer}"));
    }
    if let Some(cijfer) = klein
        .strip_prefix("num")
        .filter(|rest| rest.len() == 1 && rest.bytes().all(|b| b.is_ascii_digit()))
    {
        return Some(format!("Numpad{cijfer}"));
    }
    if let Some(letter) = klein
        .strip_prefix("key")
        .filter(|r| r.len() == 1 && r.bytes().all(|b| b.is_ascii_lowercase()))
    {
        return Some(format!("Key{}", letter.to_ascii_uppercase()));
    }
    for voorvoegsel in ["digit", "numpad"] {
        if let Some(cijfer) = klein
            .strip_prefix(voorvoegsel)
            .filter(|rest| rest.len() == 1 && rest.bytes().all(|b| b.is_ascii_digit()))
        {
            let naam = if voorvoegsel == "digit" {
                "Digit"
            } else {
                "Numpad"
            };
            return Some(format!("{naam}{cijfer}"));
        }
    }
    let naam = match klein {
        "space" => "Space",
        "backquote" => "Backquote",
        "backslash" => "Backslash",
        "bracketleft" => "BracketLeft",
        "bracketright" => "BracketRight",
        "comma" => "Comma",
        "equal" => "Equal",
        "minus" => "Minus",
        "period" => "Period",
        "quote" => "Quote",
        "semicolon" => "Semicolon",
        "slash" => "Slash",
        "insert" => "Insert",
        "delete" => "Delete",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "scrolllock" => "ScrollLock",
        "arrowup" | "up" => "ArrowUp",
        "arrowdown" | "down" => "ArrowDown",
        "arrowleft" | "left" => "ArrowLeft",
        "arrowright" | "right" => "ArrowRight",
        "esc" | "escape" => "Escape",
        "pause" | "pausebreak" => "Pause",
        _ => return None,
    };
    Some(naam.to_string())
}

/// Zet een sneltoets in de vaste schrijfwijze: modifiers als `Ctrl`, `Alt`, `Shift` (met
/// `Super` voorop), de gewone toets als `KeyboardEvent.code` (`Ctrl+Up` wordt
/// `Ctrl+ArrowUp`, `ctrl+a` wordt `Ctrl+KeyA`). Wat niet te herkennen is blijft zoals het
/// was, zodat [`controleer_sneltoets`] het afkeurt. Elke opgeslagen of nieuwe toets gaat hier
/// eerst langs, zodat een geldige alias niet verloren gaat.
#[must_use]
pub fn normaliseer_sneltoets(toets: &str) -> String {
    let (mut windows, mut ctrl, mut alt, mut shift) = (false, false, false, false);
    let mut rest: Vec<String> = Vec::new();
    for deel in toets.split('+') {
        let schoon = deel.trim();
        match schoon.to_lowercase().as_str() {
            "super" | "cmd" | "command" | "meta" | "win" | "windows" => windows = true,
            "ctrl" | "control" | "cmdorctrl" | "cmdorcontrol" | "commandorcontrol"
            | "commandorctrl" => ctrl = true,
            "alt" | "option" => alt = true,
            "shift" => shift = true,
            klein => rest.push(canonieke_hoofdtoets(klein).unwrap_or_else(|| schoon.to_string())),
        }
    }
    let mut delen: Vec<String> = Vec::new();
    for (aan, naam) in [
        (windows, "Super"),
        (ctrl, "Ctrl"),
        (alt, "Alt"),
        (shift, "Shift"),
    ] {
        if aan {
            delen.push(naam.to_string());
        }
    }
    delen.extend(rest);
    delen.join("+")
}

/// Controleert een sneltoets in de schrijfwijze van de plugin (`Ctrl+Alt+Space`) tegen de
/// regels van Mondig. Dit is de enige plek met die regels: de interface vraagt het oordeel
/// via een Tauri-commando, en het laden uit de instellingen gaat er ook langs.
///
/// Met Ctrl of Alt mag elke toets die de plugin kent, behalve gereserveerde combinaties en
/// Pause. Zonder: Insert, Scroll Lock en F13 tot en met F24 zonder meer, F1 tot en met F12
/// met een waarschuwing, ook met Shift erbij. Al het andere mag niet (met Shift is dat
/// [`SneltoetsFout::ShiftAlleen`]). Aliassen van de plugin (`Ctrl+Up`) tellen als de echte
/// naam, zie [`normaliseer_sneltoets`].
#[must_use]
pub fn controleer_sneltoets(toets: &str) -> Oordeel {
    let toets = normaliseer_sneltoets(toets);
    let fout = |fout| Oordeel::Fout { fout };
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut hoofden: Vec<String> = Vec::new();
    for deel in toets.split('+') {
        let klein = deel.trim().to_lowercase();
        match klein.as_str() {
            "" => return fout(SneltoetsFout::Leeg),
            "super" | "cmd" | "command" | "meta" | "win" | "windows" => {
                return fout(SneltoetsFout::WindowsToets);
            }
            "ctrl" | "control" | "cmdorctrl" | "cmdorcontrol" | "commandorcontrol"
            | "commandorctrl" => ctrl = true,
            "alt" | "option" => alt = true,
            "shift" => shift = true,
            "esc" => hoofden.push("escape".to_string()),
            _ => hoofden.push(klein),
        }
    }
    let hoofd = match hoofden.as_slice() {
        [] => return fout(SneltoetsFout::GeenHoofdtoets),
        [enkel] => enkel.as_str(),
        _ => return fout(SneltoetsFout::MeerdereHoofdtoetsen),
    };

    // Gereserveerd gaat voor Esc en onbekend, zodat Alt+Tab en Ctrl+Shift+Esc de duidelijke
    // melding krijgen.
    let sleutel = format!(
        "{}{}{}{hoofd}",
        if ctrl { "ctrl+" } else { "" },
        if alt { "alt+" } else { "" },
        if shift { "shift+" } else { "" },
    );
    if GERESERVEERD.contains(&sleutel.as_str()) {
        return fout(SneltoetsFout::Gereserveerd);
    }
    if hoofd == "escape" {
        return fout(SneltoetsFout::EscNietToegestaan);
    }
    if matches!(hoofd, "pause" | "pausebreak") {
        return fout(SneltoetsFout::WerktNiet);
    }
    if !is_bekende_hoofdtoets(hoofd) {
        return fout(SneltoetsFout::Onbekend);
    }
    if ctrl || alt {
        return Oordeel::Goed;
    }
    // Shift erbij verandert niets aan een toets die los mag; alleen wat gereserveerd is
    // (Shift+Insert) is hierboven al geweigerd.
    match functienummer(hoofd) {
        Some(nummer) if nummer >= 13 => Oordeel::Goed,
        Some(_) => Oordeel::Waarschuwing {
            soort: WaarschuwingSoort::Functietoets,
            toets: hoofd.to_uppercase(),
        },
        None if matches!(hoofd, "insert" | "scrolllock") => Oordeel::Goed,
        None if shift => fout(SneltoetsFout::ShiftAlleen),
        None => fout(SneltoetsFout::LosseToets),
    }
}

/// Wat de machine de schil laat doen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actie {
    /// Niets te doen.
    Geen,
    /// Begin met opnemen.
    Start,
    /// Stop met opnemen en schrijf uit.
    Stop,
}

/// Waar het dictaat zich bevindt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fase {
    /// Er loopt niets; een druk begint een dictaat.
    Rust,
    /// Er wordt opgenomen. `wisselen` is waar na een tik: de opname loopt door tot de
    /// volgende druk.
    Opnemen {
        /// Zit de opname in wisselmodus (na een tik)?
        wisselen: bool,
    },
    /// De opname is gestopt en wordt uitgeschreven. Een nieuwe druk wordt genegeerd:
    /// het werkproces schrijft één dictaat tegelijk uit (ADR 0012), en een tweede
    /// opname erdoorheen zou het scherm laten springen.
    Verwerkt,
}

/// Welke sneltoets: de gewone, of de tweede voor een opgeschoond dictaat (ADR 0027 punt 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toets {
    /// De gewone dicteertoets.
    Gewoon,
    /// De tweede sneltoets: dit dictaat wordt opgeschoond.
    Opschonen,
}

impl Toets {
    const fn plek(self) -> usize {
        match self {
            Self::Gewoon => 0,
            Self::Opschonen => 1,
        }
    }
}

/// De toestandsmachine van de dicteertoetsen. Er loopt hooguit één dictaat tegelijk, over beide
/// toetsen heen: de toets die het dictaat begon, stopt het ook. Een druk op de andere toets
/// tijdens een dictaat doet niets.
#[derive(Debug, Clone, Copy)]
pub struct Toetsbesturing {
    fase: Fase,
    /// Is de toets nu vast, per toets? Herhaalde gebeurtenissen van het besturingssysteem
    /// tellen niet mee.
    vast: [bool; 2],
    /// Wanneer de toets voor deze opname werd ingedrukt.
    druk_begin: Option<Instant>,
    /// De toets waarmee het huidige (of laatste) dictaat begon.
    toets: Toets,
}

impl Default for Toetsbesturing {
    fn default() -> Self {
        Self::nieuw()
    }
}

impl Toetsbesturing {
    /// Een machine in rust.
    #[must_use]
    pub fn nieuw() -> Self {
        Self {
            fase: Fase::Rust,
            vast: [false; 2],
            druk_begin: None,
            toets: Toets::Gewoon,
        }
    }

    /// Waar het dictaat nu is.
    #[must_use]
    pub fn fase(&self) -> Fase {
        self.fase
    }

    /// De toets waarmee het huidige of laatste dictaat begon.
    #[must_use]
    pub fn toets(&self) -> Toets {
        self.toets
    }

    /// De gewone toets gaat in; zie [`Self::ingedrukt_met`].
    pub fn ingedrukt(&mut self, nu: Instant) -> Actie {
        self.ingedrukt_met(Toets::Gewoon, nu)
    }

    /// De gewone toets gaat los; zie [`Self::losgelaten_met`].
    pub fn losgelaten(&mut self, nu: Instant) -> Actie {
        self.losgelaten_met(Toets::Gewoon, nu)
    }

    /// Een toets gaat in. Herhalingen terwijl die toets vast is doen niets, net als de andere
    /// toets tijdens een dictaat.
    pub fn ingedrukt_met(&mut self, toets: Toets, nu: Instant) -> Actie {
        if self.vast[toets.plek()] {
            return Actie::Geen;
        }
        self.vast[toets.plek()] = true;
        match self.fase {
            Fase::Rust => {
                self.fase = Fase::Opnemen { wisselen: false };
                self.druk_begin = Some(nu);
                self.toets = toets;
                Actie::Start
            }
            // Tweede druk na een tik, met dezelfde toets: stoppen.
            Fase::Opnemen { wisselen: true } if toets == self.toets => {
                self.fase = Fase::Verwerkt;
                Actie::Stop
            }
            Fase::Opnemen { .. } | Fase::Verwerkt => Actie::Geen,
        }
    }

    /// Een toets gaat los. Na een lange druk stopt de opname; na een korte begint de
    /// wisselmodus. Alleen de toets die het dictaat begon telt.
    pub fn losgelaten_met(&mut self, toets: Toets, nu: Instant) -> Actie {
        if !self.vast[toets.plek()] {
            return Actie::Geen;
        }
        self.vast[toets.plek()] = false;
        if toets != self.toets || self.fase != (Fase::Opnemen { wisselen: false }) {
            return Actie::Geen;
        }
        let duur = self
            .druk_begin
            .map_or(Duration::ZERO, |begin| nu.saturating_duration_since(begin));
        self.druk_begin = None;
        if duur >= HOUD_DREMPEL {
            self.fase = Fase::Verwerkt;
            Actie::Stop
        } else {
            self.fase = Fase::Opnemen { wisselen: true };
            Actie::Geen
        }
    }

    /// De opname is van buitenaf gestopt (tien minuten vol, of de microfoon viel weg).
    /// Geeft `true` als er een opname liep en die nu naar "verwerkt" gaat.
    pub fn stop_van_buitenaf(&mut self) -> bool {
        if matches!(self.fase, Fase::Opnemen { .. }) {
            self.fase = Fase::Verwerkt;
            true
        } else {
            false
        }
    }

    /// Esc of de knop "Annuleren" tijdens het opnemen. Geeft `true` als er een opname liep
    /// die nu weggegooid moet worden.
    pub fn annuleer(&mut self) -> bool {
        if matches!(self.fase, Fase::Opnemen { .. }) {
            self.terug_naar_rust();
            true
        } else {
            false
        }
    }

    /// Het dictaat is afgelopen (klaar, mislukt of geannuleerd) of kon niet beginnen:
    /// terug naar rust. Is een toets dan nog vast, dan blijft hij genegeerd tot hij los is.
    pub fn terug_naar_rust(&mut self) {
        self.fase = Fase::Rust;
        self.druk_begin = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn na(begin: Instant, ms: u64) -> Instant {
        begin + Duration::from_millis(ms)
    }

    #[test]
    fn houden_neemt_op_tot_loslaten() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        assert_eq!(b.ingedrukt(t0), Actie::Start);
        assert_eq!(b.fase(), Fase::Opnemen { wisselen: false });
        assert_eq!(b.losgelaten(na(t0, 1200)), Actie::Stop);
        assert_eq!(b.fase(), Fase::Verwerkt);
    }

    #[test]
    fn precies_op_de_drempel_is_houden() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        b.ingedrukt(t0);
        assert_eq!(b.losgelaten(t0 + HOUD_DREMPEL), Actie::Stop);
    }

    #[test]
    fn net_onder_de_drempel_is_tikken() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        b.ingedrukt(t0);
        assert_eq!(b.losgelaten(na(t0, 299)), Actie::Geen);
        assert_eq!(b.fase(), Fase::Opnemen { wisselen: true });
    }

    #[test]
    fn tikken_loopt_door_tot_de_volgende_druk() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        b.ingedrukt(t0);
        b.losgelaten(na(t0, 120));
        // Lang praten zonder iets in te drukken verandert niets.
        assert_eq!(b.fase(), Fase::Opnemen { wisselen: true });
        assert_eq!(b.ingedrukt(na(t0, 9000)), Actie::Stop);
        assert_eq!(b.fase(), Fase::Verwerkt);
        // Het loslaten van die tweede druk doet niets meer.
        assert_eq!(b.losgelaten(na(t0, 9100)), Actie::Geen);
    }

    #[test]
    fn herhaalde_gebeurtenissen_van_windows_worden_genegeerd() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        assert_eq!(b.ingedrukt(t0), Actie::Start);
        // Toetsherhaling: veel "ingedrukt" terwijl de toets vast is.
        for ms in [500, 530, 560, 590] {
            assert_eq!(b.ingedrukt(na(t0, ms)), Actie::Geen);
        }
        // Eén stop, ook als er meerdere "losgelaten" komen (één per herhaling).
        assert_eq!(b.losgelaten(na(t0, 2000)), Actie::Stop);
        assert_eq!(b.losgelaten(na(t0, 2001)), Actie::Geen);
        assert_eq!(b.losgelaten(na(t0, 2002)), Actie::Geen);
    }

    #[test]
    fn druk_tijdens_verwerken_wordt_genegeerd() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        b.ingedrukt(t0);
        b.losgelaten(na(t0, 1000));
        assert_eq!(b.fase(), Fase::Verwerkt);
        assert_eq!(b.ingedrukt(na(t0, 1500)), Actie::Geen);
        assert_eq!(b.losgelaten(na(t0, 1900)), Actie::Geen);
        assert_eq!(b.fase(), Fase::Verwerkt);
        // Pas als het dictaat afgelopen is, begint een nieuwe druk weer een opname.
        b.terug_naar_rust();
        assert_eq!(b.ingedrukt(na(t0, 3000)), Actie::Start);
    }

    #[test]
    fn annuleren_gooit_alleen_een_lopende_opname_weg() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        assert!(!b.annuleer());
        b.ingedrukt(t0);
        assert!(b.annuleer());
        assert_eq!(b.fase(), Fase::Rust);
        // De toets is nog vast: herhalingen starten geen nieuwe opname.
        assert_eq!(b.ingedrukt(na(t0, 400)), Actie::Geen);
        assert_eq!(b.losgelaten(na(t0, 900)), Actie::Geen);
        assert_eq!(b.ingedrukt(na(t0, 2000)), Actie::Start);
    }

    #[test]
    fn annuleren_werkt_ook_na_een_tik() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        b.ingedrukt(t0);
        b.losgelaten(na(t0, 100));
        assert!(b.annuleer());
        assert_eq!(b.fase(), Fase::Rust);
    }

    #[test]
    fn annuleren_tijdens_verwerken_hoort_niet_bij_de_toets() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        b.ingedrukt(t0);
        b.losgelaten(na(t0, 900));
        assert!(!b.annuleer());
        assert_eq!(b.fase(), Fase::Verwerkt);
    }

    #[test]
    fn opname_die_vol_is_gaat_naar_verwerken() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        assert!(!b.stop_van_buitenaf());
        b.ingedrukt(t0);
        b.losgelaten(na(t0, 100));
        assert!(b.stop_van_buitenaf());
        assert_eq!(b.fase(), Fase::Verwerkt);
        assert!(!b.stop_van_buitenaf());
    }

    #[test]
    fn mislukte_start_laat_de_machine_in_rust_zonder_herstart_door_herhaling() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        assert_eq!(b.ingedrukt(t0), Actie::Start);
        b.terug_naar_rust();
        // De gebruiker houdt de toets nog vast: geen nieuwe poging.
        assert_eq!(b.ingedrukt(na(t0, 600)), Actie::Geen);
        assert_eq!(b.losgelaten(na(t0, 1500)), Actie::Geen);
        assert_eq!(b.fase(), Fase::Rust);
    }

    #[test]
    fn de_tweede_toets_begint_een_dictaat_en_onthoudt_dat() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        assert_eq!(b.ingedrukt_met(Toets::Opschonen, t0), Actie::Start);
        assert_eq!(b.toets(), Toets::Opschonen);
        assert_eq!(
            b.losgelaten_met(Toets::Opschonen, na(t0, 1000)),
            Actie::Stop
        );
        assert_eq!(b.toets(), Toets::Opschonen);
        b.terug_naar_rust();
        assert_eq!(b.ingedrukt(na(t0, 2000)), Actie::Start);
        assert_eq!(b.toets(), Toets::Gewoon);
    }

    #[test]
    fn de_andere_toets_doet_niets_tijdens_een_dictaat() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        assert_eq!(b.ingedrukt(t0), Actie::Start);
        // Houden met de gewone toets: de tweede toets start of stopt niets.
        assert_eq!(b.ingedrukt_met(Toets::Opschonen, na(t0, 400)), Actie::Geen);
        assert_eq!(b.losgelaten_met(Toets::Opschonen, na(t0, 900)), Actie::Geen);
        assert_eq!(b.fase(), Fase::Opnemen { wisselen: false });
        assert_eq!(b.losgelaten(na(t0, 1200)), Actie::Stop);
        // Tijdens het verwerken ook niet.
        assert_eq!(b.ingedrukt_met(Toets::Opschonen, na(t0, 1500)), Actie::Geen);
        assert_eq!(b.fase(), Fase::Verwerkt);
        assert_eq!(b.toets(), Toets::Gewoon);
    }

    #[test]
    fn tikken_met_de_tweede_toets_stopt_alleen_met_de_tweede_toets() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        b.ingedrukt_met(Toets::Opschonen, t0);
        assert_eq!(b.losgelaten_met(Toets::Opschonen, na(t0, 100)), Actie::Geen);
        assert_eq!(b.fase(), Fase::Opnemen { wisselen: true });
        // De gewone toets stopt dit dictaat niet.
        assert_eq!(b.ingedrukt(na(t0, 3000)), Actie::Geen);
        assert_eq!(b.losgelaten(na(t0, 3100)), Actie::Geen);
        assert_eq!(b.fase(), Fase::Opnemen { wisselen: true });
        assert_eq!(b.ingedrukt_met(Toets::Opschonen, na(t0, 5000)), Actie::Stop);
        assert_eq!(b.toets(), Toets::Opschonen);
    }

    #[test]
    fn de_andere_toets_die_nog_vast_is_start_na_rust_geen_dictaat_door_herhaling() {
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        b.ingedrukt(t0);
        // De tweede toets gaat in tijdens het dictaat en blijft vast.
        b.ingedrukt_met(Toets::Opschonen, na(t0, 200));
        b.losgelaten(na(t0, 800));
        b.terug_naar_rust();
        // Herhalingen van de tweede toets tellen niet; pas na loslaten weer.
        assert_eq!(b.ingedrukt_met(Toets::Opschonen, na(t0, 900)), Actie::Geen);
        assert_eq!(
            b.losgelaten_met(Toets::Opschonen, na(t0, 1000)),
            Actie::Geen
        );
        assert_eq!(
            b.ingedrukt_met(Toets::Opschonen, na(t0, 1100)),
            Actie::Start
        );
    }

    /// Een nepregistratie waarbij alleen de genoemde toetsen vrij zijn.
    fn probeer_met(vrij: &'static [&'static str]) -> impl FnMut(&str) -> Result<(), String> {
        move |toets| {
            if vrij.contains(&toets) {
                Ok(())
            } else {
                Err("AlreadyRegistered".to_string())
            }
        }
    }

    #[test]
    fn gewenste_toets_die_lukt_is_geen_terugval() {
        let uitkomst =
            registreer_met_terugval(STANDAARD_SNELTOETS, probeer_met(&["Ctrl+Space"])).unwrap();
        assert_eq!(uitkomst.toets, "Ctrl+Space");
        assert!(!uitkomst.is_terugval);
    }

    #[test]
    fn terugvallijst_gaat_in_volgorde() {
        let eerste = registreer_met_terugval(
            STANDAARD_SNELTOETS,
            probeer_met(&["Ctrl+Alt+Space", "Ctrl+Shift+Space"]),
        )
        .unwrap();
        assert_eq!(eerste.toets, "Ctrl+Alt+Space");
        assert!(eerste.is_terugval);

        let tweede =
            registreer_met_terugval(STANDAARD_SNELTOETS, probeer_met(&["Ctrl+Shift+Space"]))
                .unwrap();
        assert_eq!(tweede.toets, "Ctrl+Shift+Space");
        assert!(tweede.is_terugval);
    }

    #[test]
    fn als_niets_lukt_komen_alle_fouten_terug() {
        let fouten =
            registreer_met_terugval(STANDAARD_SNELTOETS, |_| Err("bezet".to_string())).unwrap_err();
        let toetsen: Vec<&str> = fouten.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(
            toetsen,
            ["Ctrl+Space", "Ctrl+Alt+Space", "Ctrl+Shift+Space"]
        );
    }

    #[test]
    fn een_eigen_toets_valt_terug_op_dezelfde_lijst_zonder_dubbelen() {
        let mut geprobeerd = Vec::new();
        let uitkomst = registreer_met_terugval("Ctrl+Alt+Space", |toets| {
            geprobeerd.push(toets.to_string());
            if toets == "Ctrl+Shift+Space" {
                Ok(())
            } else {
                Err("bezet".to_string())
            }
        })
        .unwrap();
        assert_eq!(uitkomst.toets, "Ctrl+Shift+Space");
        // Ctrl+Alt+Space staat zowel als wens als in de terugvallijst: één poging.
        assert_eq!(geprobeerd, ["Ctrl+Alt+Space", "Ctrl+Shift+Space"]);
    }

    #[test]
    fn label_is_gewone_taal() {
        assert_eq!(toets_label("Ctrl+Space"), "Ctrl+Spatie");
        assert_eq!(toets_label("Ctrl+Alt+Space"), "Ctrl+Alt+Spatie");
        assert_eq!(toets_label("Escape"), "Esc");
        assert_eq!(toets_label("Control+KeyD"), "Ctrl+D");
        assert_eq!(toets_label("Ctrl+Shift+Digit5"), "Ctrl+Shift+5");
        assert_eq!(toets_label("Alt+ArrowUp"), "Alt+Pijl omhoog");
        assert_eq!(toets_label("Ctrl+Numpad7"), "Ctrl+Num 7");
        assert_eq!(toets_label("Ctrl+F9"), "Ctrl+F9");
        assert_eq!(toets_label("ScrollLock"), "Scroll Lock");
    }

    fn is_fout(toets: &str, verwacht: SneltoetsFout) {
        assert_eq!(
            controleer_sneltoets(toets),
            Oordeel::Fout { fout: verwacht },
            "{toets}"
        );
    }

    #[test]
    fn goede_sneltoetsen_worden_goedgekeurd() {
        for toets in [
            "Ctrl+Space",
            "Ctrl+Alt+Space",
            "Alt+KeyD",
            "Ctrl+Shift+KeyD",
            "Alt+Shift+F9",
            "control+space",
            "Ctrl+F5",
            "Ctrl+Delete",
            "Alt+Insert",
            "Ctrl+ScrollLock",
            // Losse toetsen die niet in de weg zitten.
            "Insert",
            "ScrollLock",
            "F13",
            "F24",
        ] {
            assert_eq!(controleer_sneltoets(toets), Oordeel::Goed, "{toets}");
        }
    }

    #[test]
    fn functietoetsen_f1_tot_f12_vragen_bevestiging() {
        for (toets, naam) in [("F5", "F5"), ("f9", "F9"), ("F1", "F1"), ("F12", "F12")] {
            assert_eq!(
                controleer_sneltoets(toets),
                Oordeel::Waarschuwing {
                    soort: WaarschuwingSoort::Functietoets,
                    toets: naam.to_string()
                },
                "{toets}"
            );
        }
    }

    #[test]
    fn slechte_sneltoetsen_krijgen_hun_eigen_reden() {
        is_fout("", SneltoetsFout::Leeg);
        is_fout("Ctrl+", SneltoetsFout::Leeg);
        is_fout("Super+Ctrl+Space", SneltoetsFout::WindowsToets);
        is_fout("Super+Insert", SneltoetsFout::WindowsToets);
        is_fout("KeyA", SneltoetsFout::LosseToets);
        is_fout("Shift+KeyA", SneltoetsFout::ShiftAlleen);
        is_fout("Ctrl+Shift", SneltoetsFout::GeenHoofdtoets);
        is_fout("Ctrl+KeyA+KeyB", SneltoetsFout::MeerdereHoofdtoetsen);
        is_fout("Ctrl+Escape", SneltoetsFout::EscNietToegestaan);
        is_fout("Escape", SneltoetsFout::EscNietToegestaan);
        is_fout("Ctrl+Enter", SneltoetsFout::Onbekend);
        is_fout("Ctrl+Tab", SneltoetsFout::Onbekend);
        is_fout("Ctrl+Nonsens", SneltoetsFout::Onbekend);
    }

    #[test]
    fn shift_verandert_het_oordeel_van_een_losse_toets_niet() {
        assert_eq!(controleer_sneltoets("Shift+F13"), Oordeel::Goed);
        assert_eq!(controleer_sneltoets("Shift+ScrollLock"), Oordeel::Goed);
        assert_eq!(
            controleer_sneltoets("Shift+F5"),
            Oordeel::Waarschuwing {
                soort: WaarschuwingSoort::Functietoets,
                toets: "F5".to_string()
            }
        );
        // Wat gereserveerd is blijft geweigerd, en een typtoets met Shift is ShiftAlleen.
        is_fout("Shift+Insert", SneltoetsFout::Gereserveerd);
        is_fout("Shift+KeyA", SneltoetsFout::ShiftAlleen);
        is_fout("Shift+Space", SneltoetsFout::ShiftAlleen);
    }

    #[test]
    fn aliassen_van_de_plugin_worden_genormaliseerd() {
        for (alias, canoniek) in [
            ("Ctrl+Up", "Ctrl+ArrowUp"),
            ("ctrl+a", "Ctrl+KeyA"),
            ("Control+5", "Ctrl+Digit5"),
            ("alt+shift+num3", "Alt+Shift+Numpad3"),
            ("Ctrl+Esc", "Ctrl+Escape"),
            ("Shift+Ctrl+f9", "Ctrl+Shift+F9"),
            ("CmdOrCtrl+,", "Ctrl+Comma"),
            ("Ctrl+Space", "Ctrl+Space"),
            ("Super+Ctrl+Space", "Super+Ctrl+Space"),
            ("insert", "Insert"),
        ] {
            assert_eq!(normaliseer_sneltoets(alias), canoniek, "{alias}");
        }
        // Onbekend blijft staan, zodat de controle het afkeurt.
        assert_eq!(normaliseer_sneltoets("Ctrl+Nonsens"), "Ctrl+Nonsens");
        assert_eq!(normaliseer_sneltoets("Ctrl+"), "Ctrl+");
        // Geldige aliassen worden goed gekeurd in plaats van onbekend.
        assert_eq!(controleer_sneltoets("Ctrl+Up"), Oordeel::Goed);
        assert_eq!(
            controleer_sneltoets("Ctrl+a+b"),
            Oordeel::Fout {
                fout: SneltoetsFout::MeerdereHoofdtoetsen
            }
        );
        // De gereserveerde combinaties gelden ook in aliasvorm.
        is_fout("ctrl+c", SneltoetsFout::Gereserveerd);
        is_fout("Alt+Tab", SneltoetsFout::Gereserveerd);
    }

    /// Registreert alles wat vrij is en onthoudt wat er geprobeerd is.
    fn registreer_opgeslagen_met(opgeslagen: &str) -> (Registratie, Vec<String>) {
        let mut geprobeerd = Vec::new();
        let uitkomst = registreer_opgeslagen(opgeslagen, |toets| {
            geprobeerd.push(toets.to_string());
            Ok(())
        })
        .unwrap();
        (uitkomst, geprobeerd)
    }

    #[test]
    fn opgeslagen_verboden_toets_valt_terug_op_de_standaard_als_terugval() {
        for verboden in [
            "Pause",
            "Ctrl+Pause",
            "Ctrl+KeyC",
            "Shift+Insert",
            "Super+Space",
            "KeyA",
        ] {
            let (uitkomst, geprobeerd) = registreer_opgeslagen_met(verboden);
            assert_eq!(uitkomst.toets, STANDAARD_SNELTOETS, "{verboden}");
            assert!(uitkomst.is_terugval, "{verboden}");
            // De verboden toets is nooit aan de plugin aangeboden.
            assert_eq!(geprobeerd, [STANDAARD_SNELTOETS], "{verboden}");
        }
    }

    #[test]
    fn opgeslagen_verboden_toets_gaat_door_de_terugvallijst_als_de_standaard_bezet_is() {
        let uitkomst = registreer_opgeslagen("Pause", |toets| {
            if toets == "Ctrl+Alt+Space" {
                Ok(())
            } else {
                Err("bezet".to_string())
            }
        })
        .unwrap();
        assert_eq!(uitkomst.toets, "Ctrl+Alt+Space");
        assert!(uitkomst.is_terugval);
    }

    #[test]
    fn opgeslagen_goede_toets_en_waarschuwing_worden_gewoon_geregistreerd() {
        let (goed, _) = registreer_opgeslagen_met("Insert");
        assert_eq!(goed.toets, "Insert");
        assert!(!goed.is_terugval);
        // F5 vraagt bevestiging bij het kiezen, niet bij het laden.
        let (waarschuwing, _) = registreer_opgeslagen_met("F5");
        assert_eq!(waarschuwing.toets, "F5");
        assert!(!waarschuwing.is_terugval);
        // Een alias wordt genormaliseerd en blijft behouden.
        let (alias, geprobeerd) = registreer_opgeslagen_met("Ctrl+Up");
        assert_eq!(alias.toets, "Ctrl+ArrowUp");
        assert!(!alias.is_terugval);
        assert_eq!(geprobeerd, ["Ctrl+ArrowUp"]);
    }

    #[test]
    fn pause_is_nooit_een_sneltoets() {
        for toets in [
            "Pause",
            "Ctrl+Pause",
            "Alt+Pause",
            "Ctrl+Shift+Pause",
            "PauseBreak",
        ] {
            is_fout(toets, SneltoetsFout::WerktNiet);
        }
    }

    #[test]
    fn gereserveerde_combinaties_worden_geweigerd() {
        for toets in [
            "Ctrl+KeyC",
            "Shift+Ctrl+KeyZ",
            "Control+KeyV",
            "Alt+F4",
            "Alt+Tab",
            "Ctrl+Alt+Delete",
            "Ctrl+Shift+Escape",
            "Shift+Insert",
            "Ctrl+Insert",
        ] {
            is_fout(toets, SneltoetsFout::Gereserveerd);
        }
        // Vergelijkbare combinaties mogen wel.
        assert_eq!(controleer_sneltoets("Ctrl+Alt+KeyC"), Oordeel::Goed);
        assert_eq!(controleer_sneltoets("Ctrl+Shift+KeyD"), Oordeel::Goed);
        assert_eq!(controleer_sneltoets("Alt+F9"), Oordeel::Goed);
    }

    #[test]
    fn losse_toetsen_die_typen_of_navigeren_blokkeren_worden_geweigerd() {
        for toets in [
            "Digit1", "Space", "ArrowUp", "Delete", "Home", "End", "PageUp", "PageDown", "Numpad1",
        ] {
            is_fout(toets, SneltoetsFout::LosseToets);
        }
        for toets in ["F0", "F25"] {
            is_fout(toets, SneltoetsFout::Onbekend);
        }
    }

    #[test]
    fn oordeel_serialiseert_voor_de_interface() {
        let goed = serde_json::to_value(controleer_sneltoets("Insert")).unwrap();
        assert_eq!(goed, serde_json::json!({ "uitkomst": "goed" }));
        let waarschuwing = serde_json::to_value(controleer_sneltoets("F5")).unwrap();
        assert_eq!(
            waarschuwing,
            serde_json::json!({ "uitkomst": "waarschuwing", "soort": "functietoets", "toets": "F5" })
        );
        let slecht = serde_json::to_value(controleer_sneltoets("Pause")).unwrap();
        assert_eq!(
            slecht,
            serde_json::json!({ "uitkomst": "fout", "fout": "werktNiet" })
        );
    }

    #[test]
    fn herhaalde_indrukken_van_een_losse_toets_starten_geen_tweede_opname() {
        // Houden van Insert geeft herhaalde keydown-gebeurtenissen; precies als bij Ctrl+Spatie.
        let t0 = Instant::now();
        let mut b = Toetsbesturing::nieuw();
        assert_eq!(b.ingedrukt(t0), Actie::Start);
        for ms in [500, 533, 566, 600, 633] {
            assert_eq!(b.ingedrukt(na(t0, ms)), Actie::Geen);
        }
        assert_eq!(b.fase(), Fase::Opnemen { wisselen: false });
        assert_eq!(b.losgelaten(na(t0, 1500)), Actie::Stop);
    }
}
