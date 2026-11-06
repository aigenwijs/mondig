//! Persoonsgegevens met een vaste vorm markeren en vervangen (stroom V, V-2, hoofdplan 4-E1).
//!
//! Alles gebeurt lokaal en met vaste regels, zonder taalmodel: BSN (elfproef), IBAN (mod-97),
//! telefoon (Nederlands vast en mobiel, ook met +31 of 0031), e-mail, postcode met
//! huisnummer en straat met huisnummer (adres), datums en kentekens (sidecodes 1 tot en met 13).
//! Namen, organisaties en plaatsen horen hier niet bij (4-E2): die markeert de gebruiker zelf
//! met [`Markeringen::markeer`].
//!
//! Een BSN of IBAN telt alleen als de controle klopt; een reeks van negen cijfers die de
//! elfproef niet haalt, is dus een gewoon getal. Cijfers die de spraakherkenning in losse
//! groepen schrijft ("123 45 678 2", "1 2 3 4 5 6 7 8 2") tellen mee. Bedragen (met euroteken
//! of "euro"), percentages, decimalen en punten als duizendtalscheiding (123.456.789) worden
//! niet als BSN gezien.
//!
//! De opzet van de soorten volgt Presidio (Microsoft, MIT): een herkenner per soort met een
//! controle erbij, en een voorrang als twee herkenners hetzelfde stuk willen hebben. De
//! Nederlandse regels zijn hier zelf geschreven.
//!
//! Vervangen gaat zonder sleutel: [`Markeringen::vervang`] zet `[BSN]` enzovoort in de plaats
//! en bewaart nergens wat er stond. Een lijst "PERSOON 1 = ..." zou zelf een persoonsgegeven
//! zijn. Gepseudonimiseerde tekst blijft overigens een persoonsgegeven (AVG, overweging 26):
//! de gebruiker leest altijd zelf na. Bewust zonder `Debug`: de tekst komt nooit in een log.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Wat voor gegeven er gevonden is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Soort {
    /// Burgerservicenummer (negen cijfers, elfproef).
    Bsn,
    /// Rekeningnummer (mod-97).
    Iban,
    /// Telefoonnummer.
    Telefoon,
    /// E-mailadres.
    Email,
    /// Postcode met huisnummer, of straat met huisnummer.
    Adres,
    /// Datum met dag, maand en jaar.
    Datum,
    /// Nederlands kenteken.
    Kenteken,
    /// Een stuk tekst dat de gebruiker zelf markeerde.
    Handmatig,
}

impl Soort {
    /// Wat er bij vervangen voor in de plaats komt.
    #[must_use]
    pub fn plaatsvervanger(self) -> &'static str {
        match self {
            Self::Bsn => "[BSN]",
            Self::Iban => "[IBAN]",
            Self::Telefoon => "[TELEFOON]",
            Self::Email => "[E-MAIL]",
            Self::Adres => "[ADRES]",
            Self::Datum => "[DATUM]",
            Self::Kenteken => "[KENTEKEN]",
            Self::Handmatig => "[GEGEVEN]",
        }
    }
}

/// Een gevonden stuk tekst, als bereik in bytes van de tekst (`van` tot `tot`, `tot` erbuiten).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vondst {
    /// Wat het is.
    pub soort: Soort,
    /// Begin in bytes.
    pub van: usize,
    /// Einde in bytes.
    pub tot: usize,
}

/// De plek in UTF-16-eenheden (zoals de webview telt) van een bytepositie in `tekst`.
#[must_use]
pub fn utf16_plek(tekst: &str, byte: usize) -> usize {
    tekst
        .get(..byte)
        .map_or(0, |deel| deel.encode_utf16().count())
}

/// De bytepositie in `tekst` van een plek in UTF-16-eenheden, of `None` als die midden in een
/// teken valt of buiten de tekst ligt.
#[must_use]
pub fn byte_plek(tekst: &str, utf16: usize) -> Option<usize> {
    let mut eenheden = 0;
    for (byte, teken) in tekst.char_indices() {
        if eenheden == utf16 {
            return Some(byte);
        }
        eenheden += teken.len_utf16();
        if eenheden > utf16 {
            return None;
        }
    }
    (eenheden == utf16).then_some(tekst.len())
}

// ---------------------------------------------------------------------------
// Herkenners
// ---------------------------------------------------------------------------

/// Een kandidaat in tekens (niet bytes): voorrang, soort, begin, einde.
type Kandidaat = (u8, Soort, usize, usize);

fn is_cijfer(c: char) -> bool {
    c.is_ascii_digit()
}

fn is_scheiding(c: char) -> bool {
    matches!(c, ' ' | '-' | '.')
}

/// Staat op `i` het begin van een woord (het teken ervoor is geen letter of cijfer)?
fn begin_woord(t: &[char], i: usize) -> bool {
    i == 0 || !t[i - 1].is_alphanumeric()
}

/// Is `j` het einde van een woord (het teken erna is geen letter of cijfer)?
fn einde_woord(t: &[char], j: usize) -> bool {
    j >= t.len() || !t[j].is_alphanumeric()
}

fn cijfer_waarde(c: char) -> u32 {
    c.to_digit(10).unwrap_or(0)
}

/// De elfproef voor een BSN: negen cijfers, gewogen met 9 tot 2 en -1, deelbaar door 11. Een
/// reeks van louter hetzelfde cijfer (en 000000000) telt niet.
#[must_use]
pub fn elfproef(cijfers: &str) -> bool {
    let waarden: Vec<u32> = cijfers.chars().map(cijfer_waarde).collect();
    if waarden.len() != 9 || !cijfers.chars().all(is_cijfer) {
        return false;
    }
    if waarden.iter().all(|w| *w == waarden[0]) {
        return false;
    }
    let som: i64 = waarden
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let gewicht = if i == 8 {
                -1
            } else {
                9 - i64::try_from(i).unwrap_or(0)
            };
            gewicht * i64::from(*w)
        })
        .sum();
    som % 11 == 0
}

/// De lengte van een IBAN per land, voor de landen die in Nederlandse gesprekken voorkomen.
fn iban_lengte(land: &str) -> Option<usize> {
    Some(match land {
        "NL" | "DK" | "FI" => 18,
        "BE" => 16,
        "DE" | "GB" | "IE" => 22,
        "FR" | "IT" | "GR" => 27,
        "ES" | "SE" => 24,
        "LU" | "AT" => 20,
        "CH" => 21,
        "PT" => 25,
        "NO" => 15,
        "PL" => 28,
        _ => return None,
    })
}

/// Mod-97 op een IBAN zonder spaties: de eerste vier tekens naar achteren, letters als 10 tot 35,
/// de rest bij deling door 97 moet 1 zijn.
#[must_use]
pub fn iban_klopt(compact: &str) -> bool {
    if compact.len() < 5 || !compact.is_ascii() {
        return false;
    }
    let (kop, staart) = compact.split_at(4);
    let mut rest = 0u32;
    for c in staart.chars().chain(kop.chars()) {
        let Some(waarde) = c.to_digit(36) else {
            return false;
        };
        rest = if waarde >= 10 {
            (rest * 100 + waarde) % 97
        } else {
            (rest * 10 + waarde) % 97
        };
    }
    rest == 1
}

fn vind_iban(t: &[char], uit: &mut Vec<Kandidaat>) {
    for i in 0..t.len().saturating_sub(3) {
        if !begin_woord(t, i) || !t[i].is_ascii_alphabetic() || !t[i + 1].is_ascii_alphabetic() {
            continue;
        }
        // Na het land mag één spatie staan ("NL 91 ABNA ...").
        let na_land = if t[i + 2] == ' ' { i + 3 } else { i + 2 };
        if na_land + 1 >= t.len() || !is_cijfer(t[na_land]) || !is_cijfer(t[na_land + 1]) {
            continue;
        }
        let land: String = t[i..i + 2].iter().map(char::to_ascii_uppercase).collect();
        let Some(lengte) = iban_lengte(&land) else {
            continue;
        };
        let mut compact = String::new();
        let mut j = i;
        while j < t.len() && compact.len() < lengte {
            let c = t[j];
            if c.is_ascii_alphanumeric() {
                compact.push(c.to_ascii_uppercase());
            } else if c == ' ' && j + 1 < t.len() && t[j + 1].is_ascii_alphanumeric() {
                // Eén spatie tussen groepen van vier mag.
            } else {
                break;
            }
            j += 1;
        }
        if compact.len() == lengte && einde_woord(t, j) && iban_klopt(&compact) {
            uit.push((0, Soort::Iban, i, j));
        }
    }
}

fn vind_email(t: &[char], uit: &mut Vec<Kandidaat>) {
    let lokaal = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-');
    let domein = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-');
    for (k, c) in t.iter().enumerate() {
        if *c != '@' {
            continue;
        }
        let mut van = k;
        while van > 0 && lokaal(t[van - 1]) {
            van -= 1;
        }
        while van < k && matches!(t[van], '.' | '-') {
            van += 1;
        }
        let mut tot = k + 1;
        while tot < t.len() && domein(t[tot]) {
            tot += 1;
        }
        while tot > k + 1 && matches!(t[tot - 1], '.' | '-') {
            tot -= 1;
        }
        let naam: String = t[k + 1..tot].iter().collect();
        let labels: Vec<&str> = naam.split('.').collect();
        let geldig = van < k
            && labels.len() >= 2
            && labels.iter().all(|l| !l.is_empty())
            && labels
                .last()
                .is_some_and(|l| l.len() >= 2 && l.chars().all(|c| c.is_ascii_alphabetic()));
        if geldig {
            uit.push((1, Soort::Email, van, tot));
        }
    }
}

fn vind_telefoon(t: &[char], uit: &mut Vec<Kandidaat>) {
    for i in 0..t.len() {
        let plus = t[i] == '+';
        if !(plus || t[i] == '0' || t[i] == '(') || !begin_woord(t, i) {
            continue;
        }
        // Een nummer begint niet midden in een rij cijfers ("12 0612345678").
        if i >= 2 && is_scheiding(t[i - 1]) && is_cijfer(t[i - 2]) {
            continue;
        }
        let mut cijfers = String::new();
        let mut einden: Vec<usize> = Vec::new();
        let mut j = i + usize::from(plus);
        let mut haakjes = 0;
        while j < t.len() && cijfers.len() < 14 {
            let c = t[j];
            if is_cijfer(c) {
                cijfers.push(c);
                einden.push(j + 1);
            } else if (c == '(' || c == ')') && haakjes < 2 {
                haakjes += 1;
            } else if is_scheiding(c) && j + 1 < t.len() && (is_cijfer(t[j + 1]) || t[j + 1] == '(')
            {
                // Eén scheidingsteken tussen groepen.
            } else {
                break;
            }
            j += 1;
        }
        // Het voorvoegsel (+31 of 0031), en dan negen cijfers na de nul van het landelijke nummer.
        let voorvoegsel = if plus {
            if !cijfers.starts_with("31") {
                continue;
            }
            2
        } else if cijfers.starts_with("0031") {
            4
        } else {
            0
        };
        let rest = &cijfers[voorvoegsel..];
        // Verbruikt aantal cijfers en het landelijke nummer (tien cijfers, met de nul).
        let (verbruikt, nationaal) = if voorvoegsel == 0 {
            if !rest.starts_with('0') || rest.len() < 10 {
                continue;
            }
            (10, rest[..10].to_string())
        } else if rest.starts_with('0') {
            if rest.len() < 10 {
                continue;
            }
            (voorvoegsel + 10, rest[..10].to_string())
        } else {
            if rest.len() < 9 {
                continue;
            }
            (voorvoegsel + 9, format!("0{}", &rest[..9]))
        };
        if !('1'..='9').contains(&nationaal.chars().nth(1).unwrap_or('0')) {
            continue;
        }
        let eind = einden[verbruikt - 1];
        // Een streepje of punt met een cijfer erna betekent dat het nummer verder loopt, zoals bij
        // "03-11-2026 14.00": dat is een datum met een tijd, geen telefoonnummer.
        let loopt_door =
            eind + 1 < t.len() && matches!(t[eind], '-' | '.') && is_cijfer(t[eind + 1]);
        if !einde_woord(t, eind) || loopt_door {
            continue;
        }
        uit.push((3, Soort::Telefoon, i, eind));
    }
}

/// De lettersoort en het aantal van een stuk van een kenteken: `L` letters, `C` cijfers.
const KENTEKENS: [&[(char, usize)]; 13] = [
    &[('L', 2), ('C', 2), ('C', 2)],
    &[('C', 2), ('C', 2), ('L', 2)],
    &[('C', 2), ('L', 2), ('C', 2)],
    &[('L', 2), ('C', 2), ('L', 2)],
    &[('L', 2), ('L', 2), ('C', 2)],
    &[('C', 2), ('L', 2), ('L', 2)],
    &[('C', 2), ('L', 3), ('C', 1)],
    &[('C', 1), ('L', 3), ('C', 2)],
    &[('L', 2), ('C', 3), ('L', 1)],
    &[('L', 1), ('C', 3), ('L', 2)],
    &[('L', 3), ('C', 2), ('L', 1)],
    &[('L', 1), ('C', 2), ('L', 3)],
    &[('C', 1), ('L', 2), ('C', 3)],
];

/// Letters op een Nederlands kenteken: hoofdletters zonder klinkers.
fn kentekenletter(c: char) -> bool {
    c.is_ascii_uppercase() && !matches!(c, 'A' | 'E' | 'I' | 'O' | 'U')
}

fn vind_kenteken(t: &[char], uit: &mut Vec<Kandidaat>) {
    for i in 0..t.len() {
        if !begin_woord(t, i) || (i > 0 && matches!(t[i - 1], '-')) {
            continue;
        }
        // Met streepjes mag het ook in kleine letters; met spaties of zonder scheiding alleen in
        // hoofdletters, anders lijkt gewone tekst te vaak op een kenteken.
        'scheiding: for (scheiding, kleine_letters) in
            [(Some('-'), true), (Some(' '), false), (None, false)]
        {
            'patroon: for patroon in KENTEKENS {
                let mut j = i;
                for (nr, (soort, aantal)) in patroon.iter().enumerate() {
                    if nr > 0
                        && let Some(teken) = scheiding
                    {
                        if j >= t.len() || t[j] != teken {
                            continue 'patroon;
                        }
                        j += 1;
                    }
                    for _ in 0..*aantal {
                        let goed = j < t.len()
                            && if *soort == 'L' {
                                let letter = if kleine_letters {
                                    t[j].to_ascii_uppercase()
                                } else {
                                    t[j]
                                };
                                kentekenletter(letter)
                            } else {
                                is_cijfer(t[j])
                            };
                        if !goed {
                            continue 'patroon;
                        }
                        j += 1;
                    }
                }
                let loopt_door = j < t.len() && t[j] == '-';
                if einde_woord(t, j) && !loopt_door {
                    uit.push((4, Soort::Kenteken, i, j));
                    break 'scheiding;
                }
            }
        }
    }
}

/// Leest `min` tot en met `max` cijfers vanaf `j`; geeft het eind en de waarde.
fn lees_cijfers(t: &[char], j: usize, min: usize, max: usize) -> Option<(usize, u32)> {
    let mut eind = j;
    let mut waarde = 0u32;
    while eind < t.len() && eind - j < max && is_cijfer(t[eind]) {
        waarde = waarde * 10 + cijfer_waarde(t[eind]);
        eind += 1;
    }
    (eind - j >= min && !(eind < t.len() && is_cijfer(t[eind]))).then_some((eind, waarde))
}

/// Een huisnummer vanaf `j`: 1 tot 5 cijfers, met eventueel een letter of een toevoeging
/// ("12", "12a", "12-3", "12 b"). Geeft het eind.
fn lees_huisnummer(t: &[char], j: usize) -> Option<usize> {
    let (mut eind, _) = lees_cijfers(t, j, 1, 5)?;
    if eind < t.len() && t[eind].is_alphabetic() && einde_woord(t, eind + 1) {
        eind += 1;
    } else if eind + 1 < t.len() && t[eind] == '-' && t[eind + 1].is_alphanumeric() {
        let mut k = eind + 1;
        while k < t.len() && t[k].is_alphanumeric() && k - eind <= 4 {
            k += 1;
        }
        if einde_woord(t, k) {
            eind = k;
        }
    }
    einde_woord(t, eind).then_some(eind)
}

/// Achtervoegsels van straatnamen. Bewust zonder "markt", "ring", "park", "pad" en "hof": die staan
/// ook in gewone woorden ("woningmarkt", "vergadering", "leerpad").
const STRAATDELEN: [&str; 12] = [
    "straat",
    "laan",
    "weg",
    "plein",
    "gracht",
    "singel",
    "kade",
    "steeg",
    "dijk",
    "dreef",
    "baan",
    "boulevard",
];

/// Staat er in de 30 tekens voor `i` (zonder letop hoofdletters) een van de `woorden`?
fn context_voor(t: &[char], i: usize, woorden: &[&str]) -> bool {
    let voor: String = t[i.saturating_sub(30)..i]
        .iter()
        .collect::<String>()
        .to_lowercase();
    woorden.iter().any(|w| voor.contains(w))
}

/// Tweeletterwoorden die geen postcodeletters zijn: "1999 en 2024" is geen postcode.
const KORTE_WOORDEN: [&str; 17] = [
    "en", "of", "op", "in", "is", "er", "ze", "we", "je", "ik", "de", "te", "nu", "om", "na", "ja",
    "of",
];

fn vind_adres(t: &[char], uit: &mut Vec<Kandidaat>) {
    for i in 0..t.len() {
        // Postcode: 1234 AB, met huisnummer ("1234 AB 12"), of alleen na het woord "postcode".
        if begin_woord(t, i)
            && ('1'..='9').contains(&t[i])
            && let Some((mut j, _)) = lees_cijfers(t, i, 4, 4)
        {
            if j < t.len() && t[j] == ' ' {
                j += 1;
            }
            if j + 1 < t.len()
                && t[j].is_ascii_alphabetic()
                && t[j + 1].is_ascii_alphabetic()
                && einde_woord(t, j + 2)
            {
                let letters: String = t[j..j + 2].iter().map(char::to_ascii_uppercase).collect();
                let hoofdletters = t[j].is_ascii_uppercase() && t[j + 1].is_ascii_uppercase();
                let korte = KORTE_WOORDEN.contains(&letters.to_lowercase().as_str());
                if !matches!(letters.as_str(), "SA" | "SD" | "SS") && (hoofdletters || !korte) {
                    let na = j + 2;
                    let begin = if na < t.len() && t[na] == ' ' {
                        na + 1
                    } else {
                        na
                    };
                    // Een huisnummer van hooguit vier cijfers, en geen jaartal na kleine letters.
                    let huisnummer = lees_huisnummer(t, begin).filter(|eind| {
                        let cijfers = t[begin..*eind]
                            .iter()
                            .take_while(|c| is_cijfer(**c))
                            .count();
                        cijfers <= 4 && (hoofdletters || cijfers <= 3)
                    });
                    if let Some(eind) = huisnummer {
                        uit.push((5, Soort::Adres, i, eind));
                    } else if context_voor(t, i, &["postcode"]) {
                        uit.push((5, Soort::Adres, i, j + 2));
                    }
                }
            }
        }
        // Straat met huisnummer: Kerkstraat 12.
        if begin_woord(t, i) && t[i].is_uppercase() {
            let mut j = i;
            while j < t.len() && t[j].is_alphabetic() {
                j += 1;
            }
            let woord: String = t[i..j].iter().collect::<String>().to_lowercase();
            let is_straat = STRAATDELEN
                .iter()
                .any(|d| woord.chars().count() >= d.len() + 3 && woord.ends_with(d));
            if is_straat
                && j + 1 < t.len()
                && t[j] == ' '
                && let Some(eind) = lees_huisnummer(t, j + 1)
            {
                // Hooguit vier cijfers en geen jaartal ("Kerkstraat 2024" is geen adres).
                let cijfers: String = t[j + 1..eind]
                    .iter()
                    .take_while(|c| is_cijfer(**c))
                    .collect();
                let nummer: u32 = cijfers.parse().unwrap_or(u32::MAX);
                if cijfers.len() <= 4 && !geldig_jaar(nummer) {
                    uit.push((5, Soort::Adres, i, eind));
                }
            }
        }
    }
}

const MAANDEN: [&str; 12] = [
    "januari",
    "februari",
    "maart",
    "april",
    "mei",
    "juni",
    "juli",
    "augustus",
    "september",
    "oktober",
    "november",
    "december",
];

fn geldig_jaar(jaar: u32) -> bool {
    (1850..=2100).contains(&jaar)
}

const MAAND_AFKORTINGEN: [(&str, u32); 12] = [
    ("jan", 1),
    ("feb", 2),
    ("mrt", 3),
    ("apr", 4),
    ("mei", 5),
    ("jun", 6),
    ("jul", 7),
    ("aug", 8),
    ("sept", 9),
    ("sep", 9),
    ("okt", 10),
    ("nov", 11),
];

/// Woorden vlak voor een datum die zeggen dat het om een datum gaat, voor de vormen die anders te
/// vaak op iets anders lijken ("3 11 1980", "3 nov. 2026").
const DATUMWOORDEN: [&str; 6] = [
    "geboren",
    "geb.",
    "geboortedatum",
    "datum",
    "overleden",
    "d.d.",
];

/// Staat er na `eind` een scheidingsteken met een cijfer erna? Dan loopt het getal door.
fn loopt_getal_door(t: &[char], eind: usize) -> bool {
    eind + 1 < t.len() && matches!(t[eind], '-' | '/' | '.') && is_cijfer(t[eind + 1])
}

fn vind_datum(t: &[char], uit: &mut Vec<Kandidaat>) {
    for i in 0..t.len() {
        if !begin_woord(t, i) || !is_cijfer(t[i]) {
            continue;
        }
        if i >= 2 && is_scheiding(t[i - 1]) && is_cijfer(t[i - 2]) {
            continue;
        }
        let context = context_voor(t, i, &DATUMWOORDEN);
        if let Some((j, dag)) = lees_cijfers(t, i, 1, 2) {
            // 12-03-1985, 12/3/85, 12.03.1985, en met spaties ("3 11 1980", met context).
            if j < t.len() && matches!(t[j], '-' | '/' | '.' | ' ') {
                let teken = t[j];
                if let Some((k, maand)) = lees_cijfers(t, j + 1, 1, 2)
                    && k < t.len()
                    && t[k] == teken
                    && let Some((eind, jaar)) = lees_cijfers(t, k + 1, 2, 4)
                {
                    let lengte = eind - (k + 1);
                    let jaar_goed = if teken == ' ' {
                        lengte == 4 && geldig_jaar(jaar) && context
                    } else if lengte == 4 {
                        geldig_jaar(jaar)
                    } else {
                        lengte == 2 && teken != '.'
                    };
                    if (1..=31).contains(&dag)
                        && (1..=12).contains(&maand)
                        && jaar_goed
                        && einde_woord(t, eind)
                        && !loopt_getal_door(t, eind)
                    {
                        uit.push((2, Soort::Datum, i, eind));
                        continue;
                    }
                }
            }
            // 12 maart 1985 (zonder context), 3 nov. 2026 (met context).
            if (1..=31).contains(&dag) && j < t.len() && t[j] == ' ' {
                let rest: String = t[j + 1..t.len().min(j + 14)].iter().collect();
                let rest_klein = rest.to_lowercase();
                let volledig = MAANDEN
                    .iter()
                    .find(|m| rest_klein.starts_with(**m))
                    .map(|m| (m.chars().count(), true));
                let afkorting = MAAND_AFKORTINGEN
                    .iter()
                    .find(|(m, _)| rest_klein.starts_with(m))
                    .map(|(m, _)| (m.len(), false));
                if let Some((lengte, is_volledig)) = volledig.or(afkorting) {
                    let mut k = j + 1 + lengte;
                    if k < t.len() && t[k] == '.' {
                        k += 1;
                    }
                    if k < t.len()
                        && t[k] == ' '
                        && let Some((eind, jaar)) = lees_cijfers(t, k + 1, 4, 4)
                        && geldig_jaar(jaar)
                        && einde_woord(t, eind)
                        && (is_volledig || context)
                    {
                        uit.push((2, Soort::Datum, i, eind));
                        continue;
                    }
                }
            }
        }
        // 2024-03-12.
        if let Some((j, jaar)) = lees_cijfers(t, i, 4, 4)
            && geldig_jaar(jaar)
            && j < t.len()
            && t[j] == '-'
            && let Some((k, maand)) = lees_cijfers(t, j + 1, 2, 2)
            && k < t.len()
            && t[k] == '-'
            && let Some((eind, dag)) = lees_cijfers(t, k + 1, 2, 2)
            && (1..=12).contains(&maand)
            && (1..=31).contains(&dag)
            && einde_woord(t, eind)
        {
            uit.push((2, Soort::Datum, i, eind));
        }
    }
}

/// Staat er vlak voor `i` een geldteken of een eenheid, of vlak na `eind` "euro" of een procent?
fn lijkt_op_bedrag(t: &[char], i: usize, eind: usize) -> bool {
    let voor: String = t[i.saturating_sub(8)..i]
        .iter()
        .collect::<String>()
        .to_lowercase();
    let voor = voor.trim_end();
    let na: String = t[eind..t.len().min(eind + 9)]
        .iter()
        .collect::<String>()
        .to_lowercase();
    let na = na.trim_start();
    voor.ends_with('€')
        || voor.ends_with('$')
        || voor.ends_with("euro")
        || voor.ends_with("eur")
        || na.starts_with('€')
        || na.starts_with('%')
        || na.starts_with("euro")
        || na.starts_with("eur ")
        || na.starts_with("procent")
        || na.starts_with("km")
}

/// Woorden vlak voor een nummer die zeggen dat het een BSN is. Alleen dan telt een nummer dat in
/// losse groepen staat ("123 456 782", "123.456.782"); negen aaneengesloten cijfers die de
/// elfproef halen, tellen ook zonder.
const BSNWOORDEN: [&str; 4] = ["bsn", "burgerservice", "sofi", "burger service"];

fn vind_bsn(t: &[char], uit: &mut Vec<Kandidaat>) {
    let mut i = 0;
    while i < t.len() {
        if !is_cijfer(t[i]) {
            i += 1;
            continue;
        }
        // Een rij cijfergroepen, gescheiden door één spatie, streepje of punt.
        let begin = i;
        let mut j = i;
        let mut cijfers = String::new();
        loop {
            while j < t.len() && is_cijfer(t[j]) {
                cijfers.push(t[j]);
                j += 1;
            }
            if j + 1 < t.len() && is_scheiding(t[j]) && is_cijfer(t[j + 1]) {
                j += 1;
            } else {
                break;
            }
        }
        // `j` staat achter de rij; `eind` is het einde van het laatste cijfer.
        let eind = j;
        let in_groepen = eind - begin != cijfers.len();
        let voor_scheiding = begin >= 2 && is_scheiding(t[begin - 1]) && is_cijfer(t[begin - 2]);
        let decimaal_ervoor = begin >= 2 && t[begin - 1] == ',' && is_cijfer(t[begin - 2]);
        let decimaal_erna = eind + 1 < t.len() && t[eind] == ',' && is_cijfer(t[eind + 1]);
        let plus_ervoor = begin >= 1 && matches!(t[begin - 1], '+' | '(');
        // In groepen of met punten alleen met een BSN-woord ervoor; aaneengesloten ook zonder.
        let context_goed = !in_groepen || context_voor(t, begin, &BSNWOORDEN);
        if cijfers.len() == 9
            && context_goed
            && begin_woord(t, begin)
            && einde_woord(t, eind)
            && !voor_scheiding
            && !decimaal_ervoor
            && !decimaal_erna
            && !plus_ervoor
            && !lijkt_op_bedrag(t, begin, eind)
            && elfproef(&cijfers)
        {
            uit.push((6, Soort::Bsn, begin, eind));
        }
        i = j.max(i + 1);
    }
}

/// Alle gegevens met een vaste vorm in `tekst`, op volgorde van begin, zonder overlap. Als twee
/// herkenners hetzelfde stuk willen, wint de eerste van: IBAN, e-mail, telefoon, kenteken, adres,
/// datum, BSN.
#[must_use]
pub fn vind(tekst: &str) -> Vec<Vondst> {
    let t: Vec<char> = tekst.chars().collect();
    let mut kandidaten: Vec<Kandidaat> = Vec::new();
    vind_iban(&t, &mut kandidaten);
    vind_email(&t, &mut kandidaten);
    vind_telefoon(&t, &mut kandidaten);
    vind_kenteken(&t, &mut kandidaten);
    vind_adres(&t, &mut kandidaten);
    vind_datum(&t, &mut kandidaten);
    vind_bsn(&t, &mut kandidaten);
    // Voorrang, dan het langste stuk, dan het eerste.
    kandidaten.sort_by_key(|(voorrang, _, van, tot)| (*voorrang, usize::MAX - (tot - van), *van));
    let mut gekozen: Vec<(Soort, usize, usize)> = Vec::new();
    for (_, soort, van, tot) in kandidaten {
        if gekozen.iter().all(|(_, v, e)| tot <= *v || van >= *e) {
            gekozen.push((soort, van, tot));
        }
    }
    gekozen.sort_by_key(|(_, van, _)| *van);
    let mut bytes = Vec::with_capacity(t.len() + 1);
    let mut positie = 0;
    for c in &t {
        bytes.push(positie);
        positie += c.len_utf8();
    }
    bytes.push(positie);
    gekozen
        .into_iter()
        .map(|(soort, van, tot)| Vondst {
            soort,
            van: bytes[van],
            tot: bytes[tot],
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Markeringen van één verslag
// ---------------------------------------------------------------------------

/// Wat de gebruiker met de markeringen van één verslag heeft gedaan: welke gevonden stukken
/// hij weghaalde, welke stukken tekst hij zelf markeerde, en of er bij Kopiëren en opslaan
/// vervangen wordt. De stukken staan als tekst (zonder hoofdletters), niet als plek: zo klopt
/// het ook als een regel wijzigt of beurten samengaan, en wordt hetzelfde nummer overal
/// hetzelfde behandeld. Ze gaan mee in `verslag.json` naast de opname ([`crate::verslag`], ADR
/// 0022): anders valt een heropend verslag stil terug op "niet vervangen". Bewust zonder `Debug`.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Markeringen {
    vervangen: bool,
    genegeerd: BTreeSet<String>,
    eigen: BTreeSet<String>,
}

/// De kortste en langste tekst die de gebruiker zelf kan markeren, in tekens.
const EIGEN_MIN: usize = 2;
const EIGEN_MAX: usize = 100;

fn klein(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn sleutel(tekst: &str) -> String {
    tekst
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .map(klein)
        .collect()
}

/// De tekens van een tekst met hun plek in bytes, en dezelfde tekens zonder hoofdletters.
type Tekens = (Vec<(usize, char)>, Vec<char>);

/// De plekken (in bytes) waar het eigen stuk `eigen` (al zonder hoofdletters) in `tekst` staat
/// als los woord of los stuk. `t` en `klein_t` zijn de tekens van `tekst` met hun plek, en
/// dezelfde tekens zonder hoofdletters.
fn zoek_eigen<'a>(
    eigen: &str,
    tekst: &'a str,
    t: &'a [(usize, char)],
    klein_t: &'a [char],
) -> impl Iterator<Item = (usize, usize)> + 'a {
    let naald: Vec<char> = eigen.chars().collect();
    let mut i = 0;
    std::iter::from_fn(move || {
        while !naald.is_empty() && i + naald.len() <= klein_t.len() {
            let eind = i + naald.len();
            let past = klein_t[i..eind] == naald[..];
            let los = (i == 0 || !t[i - 1].1.is_alphanumeric())
                && (eind == t.len() || !t[eind].1.is_alphanumeric());
            if past && los {
                let plek = (t[i].0, t.get(eind).map_or(tekst.len(), |(b, _)| *b));
                i = eind;
                return Some(plek);
            }
            i += 1;
        }
        None
    })
}

impl Markeringen {
    /// Herstelt de regels van een ingelezen bestand (`verslag.json`): elk stuk staat genormaliseerd
    /// (zonder hoofdletters, witruimte als één spatie), een eigen markering is tussen
    /// `EIGEN_MIN` en `EIGEN_MAX` tekens, en een leeg stuk valt weg. Een met de hand bewerkt
    /// bestand kan zo geen markering krijgen die [`Markeringen::markeer`] had geweigerd.
    pub(crate) fn herstel(&mut self) {
        self.genegeerd = std::mem::take(&mut self.genegeerd)
            .iter()
            .map(|s| sleutel(s))
            .filter(|s| !s.is_empty())
            .collect();
        self.eigen = std::mem::take(&mut self.eigen)
            .iter()
            .map(|s| sleutel(s))
            .filter(|s| (EIGEN_MIN..=EIGEN_MAX).contains(&s.chars().count()))
            .collect();
    }

    /// Wordt er bij Kopiëren en opslaan vervangen?
    #[must_use]
    pub fn vervangen(&self) -> bool {
        self.vervangen
    }

    /// Zet het vervangen aan of uit.
    pub fn zet_vervangen(&mut self, aan: bool) {
        self.vervangen = aan;
    }

    /// De stukken die de gebruiker zelf markeerde, voor de lijst op het scherm.
    #[must_use]
    pub fn eigen(&self) -> Vec<String> {
        self.eigen.iter().cloned().collect()
    }

    /// Haalt de markering van dit stuk weg: het stuk wordt overal in dit verslag genegeerd, ook als
    /// het zowel gevonden als zelf gemarkeerd was. Geeft `false` bij een leeg stuk.
    pub fn haal_weg(&mut self, waarde: &str) -> bool {
        let sleutel = sleutel(waarde);
        if sleutel.is_empty() {
            return false;
        }
        self.eigen.remove(&sleutel);
        self.genegeerd.insert(sleutel);
        true
    }

    /// Markeert zelf een stuk tekst: overal waar het als los woord of los stuk voorkomt (zonder
    /// letop hoofdletters). Een eerder weggehaalde markering van dit stuk komt terug.
    ///
    /// # Errors
    ///
    /// Geeft een melding in gewone taal als het stuk te kort of te lang is, of een regeleinde
    /// bevat.
    pub fn markeer(&mut self, waarde: &str) -> Result<(), &'static str> {
        let sleutel = sleutel(waarde);
        let lengte = sleutel.chars().count();
        if waarde.contains('\n') {
            return Err("Markeer een stuk tekst binnen één regel.");
        }
        if lengte < EIGEN_MIN {
            return Err("Markeer minstens twee tekens.");
        }
        if lengte > EIGEN_MAX {
            return Err("Dit stuk is te lang om te markeren.");
        }
        self.genegeerd.remove(&sleutel);
        self.eigen.insert(sleutel);
        Ok(())
    }

    /// De markeringen in `tekst`: de gevonden gegevens (zonder de weggehaalde) en de stukken die
    /// de gebruiker zelf markeerde. Op volgorde van begin, zonder overlap: overlappende
    /// markeringen vormen samen één markering ("Jan" en "Jan de Vries" geven "Jan de Vries"; een
    /// eigen stuk "Kerkstraat 12, 1234 AB Utrecht" neemt het gevonden "Kerkstraat 12" in zich op).
    /// De soort is die van de langste markering in de groep.
    #[must_use]
    pub fn vondsten(&self, tekst: &str) -> Vec<Vondst> {
        let mut lijst: Vec<Vondst> = vind(tekst)
            .into_iter()
            .filter(|v| !self.genegeerd.contains(&sleutel(&tekst[v.van..v.tot])))
            .collect();
        if self.eigen.is_empty() {
            return lijst;
        }
        let t: Vec<(usize, char)> = tekst.char_indices().collect();
        let klein_t: Vec<char> = t.iter().map(|(_, c)| klein(*c)).collect();
        for eigen in &self.eigen {
            lijst.extend(
                zoek_eigen(eigen, tekst, &t, &klein_t).map(|(van, tot)| Vondst {
                    soort: Soort::Handmatig,
                    van,
                    tot,
                }),
            );
        }
        // Overlappende markeringen samenvoegen; de langste bepaalt de soort.
        lijst.sort_by_key(|v| (v.van, std::cmp::Reverse(v.tot)));
        let mut samen: Vec<Vondst> = Vec::new();
        let mut langste = 0;
        for v in lijst {
            match samen.last_mut() {
                Some(laatste) if v.van < laatste.tot => {
                    laatste.tot = laatste.tot.max(v.tot);
                    if v.tot - v.van > langste {
                        langste = v.tot - v.van;
                        laatste.soort = v.soort;
                    }
                }
                _ => {
                    langste = v.tot - v.van;
                    samen.push(v);
                }
            }
        }
        samen
    }

    /// De stukken die de gebruiker zelf markeerde en die in geen van deze teksten meer als los
    /// stuk voorkomen (ADR 0023, punt 2): na opnieuw uitschrijven kan het model een gemarkeerde
    /// naam anders spellen, en dan zou die onvervangen meegaan. De aanroeper noemt het aantal,
    /// en de stukken staan nog in [`Markeringen::eigen`], zodat de gebruiker opnieuw kan markeren.
    #[must_use]
    pub fn verdwenen(&self, teksten: &[&str]) -> Vec<String> {
        let voorbereid: Vec<Tekens> = teksten
            .iter()
            .map(|tekst| {
                let t: Vec<(usize, char)> = tekst.char_indices().collect();
                let klein_t = t.iter().map(|(_, c)| klein(*c)).collect();
                (t, klein_t)
            })
            .collect();
        self.eigen
            .iter()
            .filter(|eigen| {
                !teksten
                    .iter()
                    .zip(&voorbereid)
                    .any(|(tekst, (t, klein_t))| {
                        zoek_eigen(eigen, tekst, t, klein_t).next().is_some()
                    })
            })
            .cloned()
            .collect()
    }

    /// De markeringen in een reeks teksten die achter elkaar horen (de stukken van één spreker in
    /// een transcript): een nummer dat over de grens van twee stukken loopt, wordt toch gevonden.
    /// Geeft per tekst zijn deel van de markeringen, met plekken in die tekst.
    #[must_use]
    pub fn vondsten_over(&self, teksten: &[&str]) -> Vec<Vec<Vondst>> {
        self.verdeel(teksten)
            .into_iter()
            .map(|per_tekst| per_tekst.into_iter().map(|(v, _)| v).collect())
            .collect()
    }

    /// Zoals [`Markeringen::voor_uitvoer`], maar over een reeks teksten die achter elkaar horen. Een
    /// markering die over twee teksten loopt, krijgt de plaatsvervanger in de eerste; het deel in de
    /// volgende tekst vervalt.
    #[must_use]
    pub fn uitvoer_over(&self, teksten: &[&str]) -> Vec<String> {
        if !self.vervangen {
            return teksten.iter().map(|t| (*t).to_string()).collect();
        }
        self.verdeel(teksten)
            .into_iter()
            .zip(teksten)
            .map(|(delen, tekst)| {
                let mut uit = String::with_capacity(tekst.len());
                let mut positie = 0;
                for (v, eerste) in delen {
                    uit.push_str(&tekst[positie..v.van]);
                    if eerste {
                        uit.push_str(v.soort.plaatsvervanger());
                    }
                    positie = v.tot;
                }
                uit.push_str(&tekst[positie..]);
                uit
            })
            .collect()
    }

    /// Zoekt over de aaneengeplakte teksten (met een spatie ertussen) en verdeelt de markeringen
    /// weer over de teksten. Het bool is waar voor het eerste deel van een markering.
    fn verdeel(&self, teksten: &[&str]) -> Vec<Vec<(Vondst, bool)>> {
        let mut samen = String::new();
        let mut grenzen: Vec<(usize, usize)> = Vec::new();
        for (nr, tekst) in teksten.iter().enumerate() {
            if nr > 0 {
                samen.push(' ');
            }
            grenzen.push((samen.len(), samen.len() + tekst.len()));
            samen.push_str(tekst);
        }
        let mut uit: Vec<Vec<(Vondst, bool)>> = vec![Vec::new(); teksten.len()];
        for v in self.vondsten(&samen) {
            let mut eerste = true;
            for (nr, (a, b)) in grenzen.iter().enumerate() {
                let van = v.van.max(*a);
                let tot = v.tot.min(*b);
                if van < tot {
                    uit[nr].push((
                        Vondst {
                            soort: v.soort,
                            van: van - a,
                            tot: tot - a,
                        },
                        eerste,
                    ));
                    eerste = false;
                }
            }
        }
        uit
    }

    /// De tekst met `[BSN]` enzovoort in de plaats van elke markering, ongeacht de schakelaar.
    #[must_use]
    pub fn vervang_altijd(&self, tekst: &str) -> String {
        let mut uit = String::with_capacity(tekst.len());
        let mut positie = 0;
        for v in self.vondsten(tekst) {
            uit.push_str(&tekst[positie..v.van]);
            uit.push_str(v.soort.plaatsvervanger());
            positie = v.tot;
        }
        uit.push_str(&tekst[positie..]);
        uit
    }

    /// Een naam of titel voor Kopiëren en opslaan (een sprekernaam, een bestandsnaam, een label):
    /// zoals [`Markeringen::voor_uitvoer`], maar datums blijven staan, want die zijn in een naam
    /// meestal gewoon een datum ("2026-11-03 Interview").
    #[must_use]
    pub fn voor_naam(&self, tekst: &str) -> String {
        if !self.vervangen {
            return tekst.to_string();
        }
        let mut uit = String::with_capacity(tekst.len());
        let mut positie = 0;
        for v in self
            .vondsten(tekst)
            .into_iter()
            .filter(|v| v.soort != Soort::Datum)
        {
            uit.push_str(&tekst[positie..v.van]);
            uit.push_str(v.soort.plaatsvervanger());
            positie = v.tot;
        }
        uit.push_str(&tekst[positie..]);
        uit
    }

    /// De tekst voor Kopiëren en opslaan: vervangen als de schakelaar aan staat, anders ongewijzigd.
    #[must_use]
    pub fn voor_uitvoer(&self, tekst: &str) -> String {
        if self.vervangen {
            self.vervang_altijd(tekst)
        } else {
            tekst.to_string()
        }
    }
}

#[cfg(test)]
mod tests;
