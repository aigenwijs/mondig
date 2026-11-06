//! De controle van een opgeschoond dictaat door Mondig zelf (ADR 0027 punt 3).
//!
//! Puur en zonder netwerk. [`controleer`] legt het dictaat naast het antwoord van het model en
//! keurt het af bij de eerste regel die faalt, in deze volgorde:
//!
//! 1. Een leeg antwoord ([`Regel::Leeg`]).
//! 2. Te veel of te weinig woorden ([`Regel::TeVeelWoorden`], [`Regel::TeWeinigWoorden`]). Dat
//!    vangt een model dat een gedicteerde vraag beantwoordt ("wat is de hoofdstad van Frankrijk"
//!    wordt "Parijs.") of een opdracht uitvoert (een hele mail). Klankwoorden als "ehm" tellen
//!    in het dictaat niet mee, want die horen eruit.
//! 3. Een getal, bedrag of datum dat erbij komt of ontbreekt ([`Regel::GetalErbij`],
//!    [`Regel::GetalWeg`]). Getalwoorden worden eerst getallen: "twee" en "2" zijn hetzelfde,
//!    "vijf euro vijftig" en "€5,50" ook, "tweeduizend" en "2.000" ook, "anderhalf miljoen" en
//!    "1,5 miljoen" ook. Het losse woord "een" is een lidwoord, behalve direct voor een eenheid
//!    ("een uur") of na "punt" ("punt een").
//! 4. Een maand of weekdag die erbij komt of ontbreekt ([`Regel::DatumErbij`],
//!    [`Regel::DatumWeg`]): "drie maart" wordt geen "3 mei".
//! 5. Een nieuwe naam ([`Regel::NieuweNaam`]): een woord met een hoofdletter dat niet aan het
//!    begin van een zin staat en niet in het dictaat stond (zonder op hoofdletters of accenten
//!    te letten).
//!
//! Bij getallen, maanden en dagen mag iets uit het dictaat weg als de spreker zich vlak erna
//! verbetert met iets van dezelfde soort dat wel in het antwoord staat: "drie nee vier",
//! "dinsdag nee woensdag". "Om drie uur, nee, dat kan niet eerder" laat de drie dus staan.
//!
//! Getal-, maand-, dag-, klank- en verbeterwoorden hangen af van de [`Taal`]: Nederlands of
//! Engels. In het Nederlands is de komma het decimaalteken en de punt het duizendtal
//! ("2.000", "12,50"), in het Engels andersom.
//!
//! De grenzen zijn constanten, zodat ze na de meting op `eval/opschonen` aan te passen zijn.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::notulen::Taal;

/// Hoeveel procent van het aantal woorden in het dictaat (zonder klankwoorden) het antwoord
/// hooguit mag hebben: meer dan 30 procent erbij is afkeuren.
pub const MAX_WOORDEN_PROCENT: usize = 130;

/// Hoeveel procent van het aantal woorden in het dictaat (zonder klankwoorden) het antwoord
/// minstens moet hebben: minder dan de helft is afkeuren.
pub const MIN_WOORDEN_PROCENT: usize = 50;

/// Zoveel woorden speling bovenop de procenten, zodat een heel kort dictaat ("ja prima" wordt
/// "Ja, prima.", "dinsdag nee woensdag" wordt "Woensdag.") niet op één woord afgekeurd wordt.
pub const MARGE_WOORDEN: usize = 1;

/// Zoveel woorden mogen er tussen een getal (of maand, of dag) en het verbeterwoord staan, en
/// tussen het verbeterwoord en de verbetering: "om drie uur nee om vier uur".
pub const VERBETER_AFSTAND: usize = 3;

/// De woordenlijsten van één taal.
struct Lijsten {
    /// Klanken die in het dictaat niet meetellen bij het aantal woorden.
    klank: &'static [&'static str],
    /// Woorden waarmee een spreker zichzelf verbetert.
    verbeter: &'static [&'static str],
    /// Munten: "vijf euro vijftig" is 5,50.
    munt: &'static [&'static str],
    /// Het woord voor het decimaalteken: "drie komma vijf" is 3,5.
    komma: &'static str,
    /// Na deze woorden is "een" het getal 1 ("een uur").
    eenheid: &'static [&'static str],
    /// Voor deze woorden is "een" het getal 1 ("punt een").
    nummering: &'static [&'static str],
    /// Bij Engels: "and" tussen getalwoorden ("one hundred and five").
    en: Option<&'static str>,
    /// De maanden.
    maanden: &'static [&'static str],
    /// De weekdagen.
    dagen: &'static [&'static str],
    /// Het decimaalteken en het duizendtalteken in cijfers.
    decimaal: char,
    duizendtal: char,
}

const NL: Lijsten = Lijsten {
    klank: &[
        "eh", "ehm", "ehh", "ehhm", "ehmm", "euh", "uh", "uhm", "uhh", "um", "hm", "hmm", "mmm",
    ],
    verbeter: &["nee", "sorry", "bedoel", "correctie", "pardon"],
    munt: &["euro", "dollar", "pond"],
    komma: "komma",
    eenheid: &[
        "uur", "euro", "keer", "procent", "kwart", "week", "dag", "maand", "jaar", "minuut",
        "seconde",
    ],
    nummering: &["punt", "nummer", "stap", "fase", "hoofdstuk"],
    en: None,
    maanden: &[
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
    ],
    dagen: &[
        "maandag",
        "dinsdag",
        "woensdag",
        "donderdag",
        "vrijdag",
        "zaterdag",
        "zondag",
    ],
    decimaal: ',',
    duizendtal: '.',
};

const EN: Lijsten = Lijsten {
    klank: &[
        "uh", "um", "er", "erm", "uhm", "hm", "hmm", "ah", "mm", "mmm",
    ],
    verbeter: &["no", "sorry", "mean", "correction"],
    munt: &["euro", "euros", "dollar", "dollars", "pound", "pounds"],
    komma: "point",
    eenheid: &[],
    nummering: &[],
    en: Some("and"),
    maanden: &[
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ],
    dagen: &[
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
    ],
    decimaal: '.',
    duizendtal: ',',
};

fn lijsten(taal: Taal) -> &'static Lijsten {
    match taal {
        Taal::Nl => &NL,
        Taal::En => &EN,
    }
}

/// Waarom Mondig een antwoord afkeurt. De code komt in het logboek, nooit de tekst.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Regel {
    /// Het antwoord is leeg.
    Leeg,
    /// Het antwoord was geen geldig JSON volgens het schema.
    Onleesbaar,
    /// Het antwoord heeft te veel woorden.
    TeVeelWoorden,
    /// Het antwoord heeft te weinig woorden.
    TeWeinigWoorden,
    /// Er staat een getal, bedrag of datum in dat niet in het dictaat stond.
    GetalErbij,
    /// Er ontbreekt een getal, bedrag of datum uit het dictaat.
    GetalWeg,
    /// Er staat een maand of dag in die niet in het dictaat stond.
    DatumErbij,
    /// Er ontbreekt een maand of dag uit het dictaat.
    DatumWeg,
    /// Er staat een naam in die niet in het dictaat stond.
    NieuweNaam,
}

impl Regel {
    /// Alle regels, in de volgorde van de controle (voor tellingen).
    pub const ALLE: [Self; 9] = [
        Self::Leeg,
        Self::Onleesbaar,
        Self::TeVeelWoorden,
        Self::TeWeinigWoorden,
        Self::GetalErbij,
        Self::GetalWeg,
        Self::DatumErbij,
        Self::DatumWeg,
        Self::NieuweNaam,
    ];

    /// Een vaste code voor het logboek en de meting.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Leeg => "leeg",
            Self::Onleesbaar => "onleesbaar",
            Self::TeVeelWoorden => "te_veel_woorden",
            Self::TeWeinigWoorden => "te_weinig_woorden",
            Self::GetalErbij => "getal_erbij",
            Self::GetalWeg => "getal_weg",
            Self::DatumErbij => "datum_erbij",
            Self::DatumWeg => "datum_weg",
            Self::NieuweNaam => "nieuwe_naam",
        }
    }
}

/// Keurt het antwoord van het model goed (`Ok`) of af met de eerste regel die faalt. `taal`
/// bepaalt welke getal-, maand-, dag-, klank- en verbeterwoorden Mondig herkent.
///
/// # Errors
///
/// De [`Regel`] waarop het antwoord is afgekeurd.
pub fn controleer(dictaat: &str, antwoord: &str, taal: Taal) -> Result<(), Regel> {
    let lijsten = lijsten(taal);
    if aantal_woorden(antwoord) == 0 {
        return Err(Regel::Leeg);
    }
    controleer_woorden(dictaat, antwoord, lijsten)?;
    controleer_getallen(dictaat, antwoord, taal)?;
    controleer_datums(dictaat, antwoord, lijsten)?;
    controleer_namen(dictaat, antwoord)
}

/// Tekst om te vergelijken op hele woorden: kleine letters zonder accenten, leestekens worden
/// een spatie, `€` en `%` worden een eigen woord, witruimte wordt één spatie. Voor de kenmerken
/// van de testset in `mondig-cli opschonen meet`.
#[must_use]
pub fn vergelijkbaar(tekst: &str) -> String {
    let mut uit = String::with_capacity(tekst.len());
    for teken in tekst
        .chars()
        .flat_map(char::to_lowercase)
        .map(zonder_accent)
    {
        match teken {
            '€' | '%' => {
                uit.push(' ');
                uit.push(teken);
                uit.push(' ');
            }
            t if t.is_alphanumeric() => uit.push(t),
            _ => uit.push(' '),
        }
    }
    uit.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// De woorden van een tekst: stukken tussen witruimte met minstens één letter of cijfer.
fn woorden(tekst: &str) -> impl Iterator<Item = &str> {
    tekst
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
}

fn aantal_woorden(tekst: &str) -> usize {
    woorden(tekst).count()
}

/// Het aantal woorden van het dictaat zonder klankwoorden.
fn woorden_zonder_klank(tekst: &str, lijsten: &Lijsten) -> usize {
    woorden(tekst)
        .filter(|w| !lijsten.klank.contains(&kern(w).as_str()))
        .count()
}

fn controleer_woorden(dictaat: &str, antwoord: &str, lijsten: &Lijsten) -> Result<(), Regel> {
    let invoer = woorden_zonder_klank(dictaat, lijsten);
    let uitvoer = aantal_woorden(antwoord);
    // In honderdsten, zodat er geen kommagetallen nodig zijn.
    if uitvoer * 100 > invoer * MAX_WOORDEN_PROCENT + MARGE_WOORDEN * 100 {
        return Err(Regel::TeVeelWoorden);
    }
    if (uitvoer + MARGE_WOORDEN) * 100 < invoer * MIN_WOORDEN_PROCENT {
        return Err(Regel::TeWeinigWoorden);
    }
    Ok(())
}

/// Een woord in kleine letters, zonder leestekens aan het begin en eind (accenten blijven).
fn klein(woord: &str) -> String {
    woord
        .trim_matches(|c: char| !c.is_alphanumeric())
        .chars()
        .flat_map(char::to_lowercase)
        .collect()
}

/// Een woord in kleine letters, zonder accenten en zonder leestekens aan het begin en eind.
fn kern(woord: &str) -> String {
    klein(woord).chars().map(zonder_accent).collect()
}

/// Een letter zonder accent; de gangbare letters met accent in het Nederlands, Engels, Frans en
/// Duits.
fn zonder_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        'ý' | 'ÿ' => 'y',
        c => c,
    }
}

// ---- Getallen ----
//
// Getallen zijn in duizendsten (`u128`), zodat "12,50" en "1,5 miljoen" zonder kommagetallen
// te vergelijken zijn.

/// Eén getal in duizendsten.
const EEN: u128 = 1_000;

/// Een getalwoord: een gewoon getal (in duizendsten), of een vermenigvuldiger die op zichzelf
/// kan staan ("honderd", "duizend", "miljoen"; als geheel getal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Getalwoord {
    Getal(u128),
    Keer(u128),
}

const EENHEDEN: [(&str, u64); 9] = [
    ("een", 1),
    ("twee", 2),
    ("drie", 3),
    ("vier", 4),
    ("vijf", 5),
    ("zes", 6),
    ("zeven", 7),
    ("acht", 8),
    ("negen", 9),
];

const TIENERS: [(&str, u64); 10] = [
    ("tien", 10),
    ("elf", 11),
    ("twaalf", 12),
    ("dertien", 13),
    ("veertien", 14),
    ("vijftien", 15),
    ("zestien", 16),
    ("zeventien", 17),
    ("achttien", 18),
    ("negentien", 19),
];

const TIENTALLEN: [(&str, u64); 8] = [
    ("twintig", 20),
    ("dertig", 30),
    ("veertig", 40),
    ("vijftig", 50),
    ("zestig", 60),
    ("zeventig", 70),
    ("tachtig", 80),
    ("negentig", 90),
];

const EN_ONDER_TWINTIG: [(&str, u64); 19] = [
    ("one", 1),
    ("two", 2),
    ("three", 3),
    ("four", 4),
    ("five", 5),
    ("six", 6),
    ("seven", 7),
    ("eight", 8),
    ("nine", 9),
    ("ten", 10),
    ("eleven", 11),
    ("twelve", 12),
    ("thirteen", 13),
    ("fourteen", 14),
    ("fifteen", 15),
    ("sixteen", 16),
    ("seventeen", 17),
    ("eighteen", 18),
    ("nineteen", 19),
];

const EN_TIENTALLEN: [(&str, u64); 8] = [
    ("twenty", 20),
    ("thirty", 30),
    ("forty", 40),
    ("fifty", 50),
    ("sixty", 60),
    ("seventy", 70),
    ("eighty", 80),
    ("ninety", 90),
];

/// Engelse rangtelwoorden die niet op "-th" na het telwoord eindigen.
const EN_RANG: [(&str, u64); 7] = [
    ("first", 1),
    ("second", 2),
    ("third", 3),
    ("fifth", 5),
    ("eighth", 8),
    ("ninth", 9),
    ("twelfth", 12),
];

fn uit_tabel(tabel: &[(&str, u64)], woord: &str) -> Option<u64> {
    tabel.iter().find(|(w, _)| *w == woord).map(|(_, g)| *g)
}

/// Een getal onder de honderd als één woord: "zeven", "twaalf", "dertig", "zesendertig" (ook
/// "tweeëntwintig", na [`kern`] "tweeentwintig").
fn onder_honderd(woord: &str) -> Option<u64> {
    if let Some(g) = uit_tabel(&EENHEDEN, woord)
        .or_else(|| uit_tabel(&TIENERS, woord))
        .or_else(|| uit_tabel(&TIENTALLEN, woord))
    {
        return Some(g);
    }
    TIENTALLEN.iter().find_map(|(tiental, waarde)| {
        let voor = woord.strip_suffix(tiental)?.strip_suffix("en")?;
        Some(waarde + uit_tabel(&EENHEDEN, voor)?)
    })
}

/// Een getal als één woord, tot de miljoenen: "vierhonderdvijftig", "tweeduizend",
/// "twaalfhonderd", "negentienhonderdvijfentachtig". Een leeg deel voor "honderd" of "duizend"
/// is één, een leeg deel erna nul.
fn samengesteld(woord: &str) -> Option<u64> {
    for (deel, keer) in [("miljoen", 1_000_000), ("duizend", 1_000), ("honderd", 100)] {
        if let Some((voor, na)) = woord.split_once(deel) {
            let voor = if voor.is_empty() {
                1
            } else {
                samengesteld(voor)?
            };
            let na = if na.is_empty() { 0 } else { samengesteld(na)? };
            return voor.checked_mul(keer)?.checked_add(na);
        }
    }
    onder_honderd(woord)
}

/// Herkent een Nederlands getalwoord, ook een rangtelwoord ("derde", "achtste",
/// "eenendertigste"), "anderhalf" en "tweeënhalf". `klein` is het woord met accenten, `kern`
/// zonder. Het losse woord "een" telt hier niet (dat is meestal een lidwoord), "één" wel.
fn getalwoord_nl(klein: &str, kern: &str) -> Option<Getalwoord> {
    if matches!(klein, "één" | "eén") {
        return Some(Getalwoord::Getal(EEN));
    }
    match kern {
        "een" | "nul" => return None,
        "honderd" => return Some(Getalwoord::Keer(100)),
        "duizend" => return Some(Getalwoord::Keer(1_000)),
        "miljoen" => return Some(Getalwoord::Keer(1_000_000)),
        "miljard" => return Some(Getalwoord::Keer(1_000_000_000)),
        "anderhalf" | "anderhalve" => return Some(Getalwoord::Getal(EEN + EEN / 2)),
        "eerste" => return Some(Getalwoord::Getal(EEN)),
        "derde" => return Some(Getalwoord::Getal(3 * EEN)),
        _ => {}
    }
    let geheel = |g: u64| Some(Getalwoord::Getal(u128::from(g) * EEN));
    if let Some(g) = samengesteld(kern) {
        return geheel(g);
    }
    if let Some(stam) = kern.strip_suffix("enhalf") {
        return samengesteld(stam).map(|g| Getalwoord::Getal(u128::from(g) * EEN + EEN / 2));
    }
    let stam = kern
        .strip_suffix("ste")
        .or_else(|| kern.strip_suffix("de"))?;
    samengesteld(stam).and_then(geheel)
}

/// Een Engels getal onder de honderd, ook met streepje ("twenty-one") en als rangtelwoord
/// ("twenty-first", "fourth", "thirtieth").
fn en_onder_honderd(woord: &str) -> Option<u64> {
    if let Some((tiental, eenheid)) = woord.split_once('-') {
        return Some(uit_tabel(&EN_TIENTALLEN, tiental)? + en_eenheid(eenheid)?);
    }
    en_eenheid(woord).or_else(|| uit_tabel(&EN_TIENTALLEN, woord))
}

/// Een Engels getal onder de twintig of een tiental, ook als rangtelwoord.
fn en_eenheid(woord: &str) -> Option<u64> {
    if let Some(g) = uit_tabel(&EN_ONDER_TWINTIG, woord).or_else(|| uit_tabel(&EN_RANG, woord)) {
        return Some(g);
    }
    if let Some(stam) = woord.strip_suffix("ieth") {
        return uit_tabel(&EN_TIENTALLEN, &format!("{stam}y"));
    }
    let stam = woord.strip_suffix("th")?;
    uit_tabel(&EN_ONDER_TWINTIG, stam).or_else(|| uit_tabel(&EN_TIENTALLEN, stam))
}

/// Herkent een Engels getalwoord.
fn getalwoord_en(kern: &str) -> Option<Getalwoord> {
    match kern {
        "hundred" | "hundredth" => Some(Getalwoord::Keer(100)),
        "thousand" | "thousandth" => Some(Getalwoord::Keer(1_000)),
        "million" => Some(Getalwoord::Keer(1_000_000)),
        "billion" => Some(Getalwoord::Keer(1_000_000_000)),
        _ => en_onder_honderd(kern).map(|g| Getalwoord::Getal(u128::from(g) * EEN)),
    }
}

fn getalwoord(klein: &str, kern: &str, taal: Taal) -> Option<Getalwoord> {
    match taal {
        Taal::Nl => getalwoord_nl(klein, kern),
        Taal::En => getalwoord_en(kern),
    }
}

/// De stukken met cijfers in een woord: reeksen van cijfers, punten en komma's die met een cijfer
/// beginnen en eindigen ("€12,50." geeft "12,50"; "14:30" geeft "14" en "30").
fn cijferstukken(woord: &str) -> Vec<&str> {
    woord
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .map(|s| s.trim_matches(|c: char| !c.is_ascii_digit()))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Een geheel getal uit cijfers, in duizendsten.
fn cijfers(tekst: &str) -> Option<u128> {
    tekst.parse::<u128>().ok()?.checked_mul(EEN)
}

/// Leest een stuk met cijfers als getallen in duizendsten. Het duizendtalteken telt alleen als
/// er steeds precies drie cijfers na komen ("2.000", "10.000"); het decimaalteken geeft
/// hooguit drie decimalen ("12,50" is 12,5). Wat niet past ("1,2,3"), worden losse getallen.
fn lees_cijferstuk(stuk: &str, lijsten: &Lijsten) -> Vec<u128> {
    let los = || {
        stuk.split(['.', ','])
            .filter_map(cijfers)
            .collect::<Vec<_>>()
    };
    let delen: Vec<&str> = stuk.split(lijsten.decimaal).collect();
    let (geheel, decimalen) = match delen.as_slice() {
        [geheel] => (*geheel, None),
        [geheel, decimalen] if !decimalen.contains(lijsten.duizendtal) => {
            (*geheel, Some(*decimalen))
        }
        _ => return los(),
    };
    let groepen: Vec<&str> = geheel.split(lijsten.duizendtal).collect();
    let geheel = if groepen.len() == 1 {
        groepen[0].to_string()
    } else if (1..=3).contains(&groepen[0].len()) && groepen[1..].iter().all(|g| g.len() == 3) {
        groepen.concat()
    } else if groepen.len() == 2 && decimalen.is_none() {
        // "1.5" in het Nederlands: dan is de punt toch een decimaalteken.
        return lees_cijferstuk(
            &stuk.replace(lijsten.duizendtal, &lijsten.decimaal.to_string()),
            lijsten,
        );
    } else {
        return los();
    };
    let Some(mut waarde) = cijfers(&geheel) else {
        return Vec::new();
    };
    if let Some(decimalen) = decimalen {
        let drie: String = decimalen.chars().chain("000".chars()).take(3).collect();
        waarde += drie.parse::<u128>().unwrap_or(0);
    }
    vec![waarde]
}

/// Een getal in de tekst, met de plek (het nummer van het woord) waar het begint en eindigt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Gevonden<T> {
    begin: usize,
    eind: usize,
    waarde: T,
}

/// Een getal dat nog groeit: "drie duizend vijf honderd" is `totaal` 3000 plus `huidig` 500.
#[derive(Debug, Clone, Copy)]
struct Bouw {
    begin: usize,
    eind: usize,
    totaal: u128,
    huidig: u128,
}

impl Bouw {
    fn waarde(self) -> u128 {
        self.totaal + self.huidig
    }

    /// Mag het getalwoord `g` erbij opgeteld worden? Alleen bij een rond getal met ruimte:
    /// "honderd vijftig" en "tweeduizend zesentwintig" wel, "twee drie" en "twintig vijf" niet.
    fn heeft_ruimte_voor(self, g: u128) -> bool {
        let basis = if self.huidig > 0 {
            self.huidig
        } else {
            self.totaal
        };
        if !basis.is_multiple_of(EEN) || !g.is_multiple_of(EEN) {
            return false;
        }
        let ruimte = ruimte(basis / EEN);
        ruimte >= 100 && g / EEN < ruimte
    }
}

/// De grootste macht van tien waardoor `x` deelbaar is (1 voor 26, 100 voor 500).
fn ruimte(mut x: u128) -> u128 {
    let mut macht = 1;
    while x > 0 && x.is_multiple_of(10) {
        x /= 10;
        macht *= 10;
    }
    macht
}

/// Wat het volgende getalwoord aan het vorige getal vastplakt.
#[derive(Debug, Clone, Copy)]
enum Plak {
    /// Na "vijf euro": "vijftig" zijn centen.
    Munt(usize),
    /// Na "drie komma": "vijf" zijn decimalen.
    Komma(usize),
    /// Na "twee komma vijf": "miljoen" vermenigvuldigt.
    Keer(usize),
}

/// Leest de getallen uit een tekst.
struct Lezer<'a> {
    lijsten: &'a Lijsten,
    uit: Vec<Gevonden<u128>>,
    bouw: Option<Bouw>,
    plak: Option<Plak>,
}

impl Lezer<'_> {
    fn sluit(&mut self) {
        if let Some(bouw) = self.bouw.take() {
            self.uit.push(Gevonden {
                begin: bouw.begin,
                eind: bouw.eind,
                waarde: bouw.waarde(),
            });
        }
    }

    /// Het laatst gevonden getal, als het eindigde op het woord vlak voor `plek`.
    fn net_ervoor(&self, plek: usize) -> Option<usize> {
        let laatste = self.uit.len().checked_sub(1)?;
        (self.uit[laatste].eind + 1 == plek).then_some(laatste)
    }

    fn getal(&mut self, plek: usize, g: u128, uit_cijfers: bool) {
        match (self.plak.take(), self.bouw) {
            (Some(Plak::Munt(i)), None)
                if !uit_cijfers && g.is_multiple_of(EEN) && g < 100 * EEN =>
            {
                self.uit[i].waarde += g / 100;
                self.uit[i].eind = plek;
                self.plak = Some(Plak::Keer(i));
            }
            (Some(Plak::Komma(i)), None)
                if !uit_cijfers && g.is_multiple_of(EEN) && g < EEN * EEN =>
            {
                let n = g / EEN;
                let deler = if n < 10 {
                    10
                } else if n < 100 {
                    100
                } else {
                    1_000
                };
                self.uit[i].waarde += n * EEN / deler;
                self.uit[i].eind = plek;
                self.plak = Some(Plak::Keer(i));
            }
            (_, Some(mut bouw)) if !uit_cijfers && bouw.heeft_ruimte_voor(g) => {
                if bouw.huidig > 0 {
                    bouw.huidig += g;
                } else {
                    bouw.huidig = g;
                }
                bouw.eind = plek;
                self.bouw = Some(bouw);
            }
            _ => {
                self.sluit();
                self.bouw = Some(Bouw {
                    begin: plek,
                    eind: plek,
                    totaal: 0,
                    huidig: g,
                });
            }
        }
    }

    fn keer(&mut self, plek: usize, keer: u128) {
        if let Some(mut bouw) = self.bouw {
            let basis = if bouw.huidig == 0 { EEN } else { bouw.huidig };
            if keer >= 1_000 {
                bouw.totaal += basis.saturating_mul(keer);
                bouw.huidig = 0;
            } else if bouw.huidig >= keer * EEN {
                // "tweeduizend honderd": optellen.
                bouw.huidig += keer * EEN;
            } else {
                bouw.huidig = basis.saturating_mul(keer);
            }
            bouw.eind = plek;
            self.bouw = Some(bouw);
            self.plak = None;
            return;
        }
        if let Some(Plak::Keer(i)) = self.plak.take() {
            self.uit[i].waarde = self.uit[i].waarde.saturating_mul(keer);
            self.uit[i].eind = plek;
            return;
        }
        let (totaal, huidig) = if keer >= 1_000 {
            (keer * EEN, 0)
        } else {
            (0, keer * EEN)
        };
        self.bouw = Some(Bouw {
            begin: plek,
            eind: plek,
            totaal,
            huidig,
        });
    }
}

/// De getallen in een tekst, in duizendsten, met hun plek. Opeenvolgende getalwoorden worden
/// één getal: "twee honderd" is 200, "drie duizend vijf honderd" 3500, "tweeduizend
/// zesentwintig" 2026, "5 miljoen" 5000000. Nul telt niet mee: "14:00" en "veertien uur" zijn
/// dan hetzelfde.
fn getallen(tekst: &str, taal: Taal) -> Vec<Gevonden<u128>> {
    let lijsten = lijsten(taal);
    let lijst: Vec<&str> = woorden(tekst).collect();
    let kernen: Vec<String> = lijst.iter().map(|w| kern(w)).collect();
    let mut lezer = Lezer {
        lijsten,
        uit: Vec::new(),
        bouw: None,
        plak: None,
    };
    for (plek, woord) in lijst.iter().enumerate() {
        let k = kernen[plek].as_str();
        let stukken = cijferstukken(woord);
        let soort = match stukken.as_slice() {
            [] => getalwoord(&klein(woord), k, taal).or_else(|| {
                // "een uur", "punt een": dan is "een" het getal 1.
                let na = kernen.get(plek + 1).map(String::as_str);
                let voor = plek.checked_sub(1).map(|p| kernen[p].as_str());
                (k == "een"
                    && (na.is_some_and(|n| lezer.lijsten.eenheid.contains(&n))
                        || voor.is_some_and(|v| lezer.lijsten.nummering.contains(&v))))
                .then_some(Getalwoord::Getal(EEN))
            }),
            [stuk] => match lees_cijferstuk(stuk, lijsten).as_slice() {
                [g] => {
                    lezer.getal(plek, *g, true);
                    continue;
                }
                losse => {
                    lezer.sluit();
                    lezer.plak = None;
                    let losse = losse.to_vec();
                    lezer.uit.extend(losse.into_iter().map(|waarde| Gevonden {
                        begin: plek,
                        eind: plek,
                        waarde,
                    }));
                    continue;
                }
            },
            _ => None,
        };
        match soort {
            Some(Getalwoord::Getal(g)) => lezer.getal(plek, g, false),
            Some(Getalwoord::Keer(keer)) => lezer.keer(plek, keer),
            None => {
                // "one hundred and five": "and" breekt het getal niet als er nog ruimte is.
                if lijsten.en == Some(k) && lezer.bouw.is_some_and(|b| b.heeft_ruimte_voor(EEN)) {
                    continue;
                }
                lezer.sluit();
                lezer.plak = if lijsten.munt.contains(&k) {
                    lezer.net_ervoor(plek).map(Plak::Munt)
                } else if lijsten.komma == k {
                    lezer.net_ervoor(plek).map(Plak::Komma)
                } else {
                    None
                };
                if !stukken.is_empty() {
                    let losse: Vec<u128> = stukken
                        .iter()
                        .filter_map(|s| cijfers(s.split(['.', ',']).next().unwrap_or(s)))
                        .collect();
                    lezer.uit.extend(losse.into_iter().map(|waarde| Gevonden {
                        begin: plek,
                        eind: plek,
                        waarde,
                    }));
                }
            }
        }
    }
    lezer.sluit();
    lezer.uit.retain(|g| g.waarde != 0);
    lezer.uit
}

/// De plekken van verbeterwoorden ("nee", "sorry").
fn verbeterplekken(tekst: &str, lijsten: &Lijsten) -> Vec<usize> {
    woorden(tekst)
        .enumerate()
        .filter(|(_, w)| lijsten.verbeter.contains(&kern(w).as_str()))
        .map(|(plek, _)| plek)
        .collect()
}

/// Mag dit stuk uit het dictaat wegvallen? Alleen als de spreker zich vlak erna verbetert met
/// iets anders van dezelfde soort dat wel in het antwoord staat ("drie nee vier").
fn verbeterd<T: PartialEq>(
    eigen: &Gevonden<T>,
    alle: &[Gevonden<T>],
    verbeter: &[usize],
    in_antwoord: impl Fn(&T) -> bool,
) -> bool {
    verbeter.iter().any(|&q| {
        q > eigen.eind
            && q - eigen.eind <= VERBETER_AFSTAND
            && alle.iter().any(|ander| {
                ander.begin > q
                    && ander.begin - q <= VERBETER_AFSTAND
                    && ander.waarde != eigen.waarde
                    && in_antwoord(&ander.waarde)
            })
    })
}

/// Vergelijkt wat er in het dictaat en het antwoord staat (getallen, of maanden en dagen).
fn vergelijk<T: Ord + Clone>(
    invoer: &[Gevonden<T>],
    uitvoer: &[Gevonden<T>],
    verbeter: &[usize],
    erbij: Regel,
    weg: Regel,
) -> Result<(), Regel> {
    let in_dictaat: BTreeSet<&T> = invoer.iter().map(|g| &g.waarde).collect();
    let in_antwoord: BTreeSet<&T> = uitvoer.iter().map(|g| &g.waarde).collect();
    if !in_antwoord.is_subset(&in_dictaat) {
        return Err(erbij);
    }
    let staat_in_antwoord = |w: &T| in_antwoord.contains(w);
    let mag_weg = |g: &Gevonden<T>| {
        staat_in_antwoord(&g.waarde) || verbeterd(g, invoer, verbeter, staat_in_antwoord)
    };
    if !invoer.iter().all(mag_weg) {
        return Err(weg);
    }
    Ok(())
}

fn controleer_getallen(dictaat: &str, antwoord: &str, taal: Taal) -> Result<(), Regel> {
    vergelijk(
        &getallen(dictaat, taal),
        &getallen(antwoord, taal),
        &verbeterplekken(dictaat, lijsten(taal)),
        Regel::GetalErbij,
        Regel::GetalWeg,
    )
}

// ---- Maanden en dagen ----

/// De maanden en weekdagen in een tekst, met hun plek.
fn datums(tekst: &str, lijsten: &Lijsten) -> Vec<Gevonden<String>> {
    woorden(tekst)
        .enumerate()
        .map(|(plek, w)| (plek, kern(w)))
        .filter(|(_, k)| {
            lijsten.maanden.contains(&k.as_str()) || lijsten.dagen.contains(&k.as_str())
        })
        .map(|(plek, waarde)| Gevonden {
            begin: plek,
            eind: plek,
            waarde,
        })
        .collect()
}

fn controleer_datums(dictaat: &str, antwoord: &str, lijsten: &Lijsten) -> Result<(), Regel> {
    vergelijk(
        &datums(dictaat, lijsten),
        &datums(antwoord, lijsten),
        &verbeterplekken(dictaat, lijsten),
        Regel::DatumErbij,
        Regel::DatumWeg,
    )
}

// ---- Namen ----

/// De delen van een woord om te vergelijken: de kern, en de stukken tussen streepjes
/// ("noord-holland" is ook "noord" en "holland"). Een bezits-s ("Piets", "Piet's") gaat eraf.
fn delen(woord: &str) -> Vec<String> {
    let k = kern(woord);
    let k = k
        .strip_suffix("'s")
        .or_else(|| k.strip_suffix("\u{2019}s"))
        .map_or(k.clone(), str::to_string);
    let mut uit: Vec<String> = k
        .split(['-', '\'', '\u{2019}'])
        .filter(|d| !d.is_empty())
        .map(str::to_string)
        .collect();
    uit.push(k);
    uit
}

/// Eindigt dit woord een zin?
fn sluit_zin(woord: &str) -> bool {
    woord
        .trim_end_matches(['"', '\'', ')', ']', '\u{201d}', '\u{2019}'])
        .ends_with(['.', '!', '?'])
}

fn controleer_namen(dictaat: &str, antwoord: &str) -> Result<(), Regel> {
    let bekend: BTreeSet<String> = woorden(dictaat).flat_map(delen).collect();
    for regel in antwoord.lines() {
        // Elke regel begint een zin (een opsomming of nieuwe alinea).
        let mut begin_zin = true;
        for woord in regel.split_whitespace() {
            let schoon = woord.trim_start_matches(|c: char| !c.is_alphanumeric());
            let hoofdletter = schoon.chars().next().is_some_and(char::is_uppercase);
            let met_cijfer = schoon.chars().any(|c| c.is_ascii_digit());
            if hoofdletter && !begin_zin && !met_cijfer {
                let delen = delen(schoon);
                // Het woord zelf, of elk stuk tussen streepjes, stond in het dictaat.
                let heel = delen.last().is_some_and(|k| bekend.contains(k));
                let stukken =
                    delen.len() > 1 && delen[..delen.len() - 1].iter().all(|d| bekend.contains(d));
                if !heel && !stukken {
                    return Err(Regel::NieuweNaam);
                }
            }
            if schoon.chars().any(char::is_alphanumeric) || sluit_zin(woord) {
                begin_zin = sluit_zin(woord);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// De controle in het Nederlands, zoals de meeste tests hem gebruiken.
    fn controleer(dictaat: &str, antwoord: &str) -> Result<(), Regel> {
        super::controleer(dictaat, antwoord, Taal::Nl)
    }

    fn engels(dictaat: &str, antwoord: &str) -> Result<(), Regel> {
        super::controleer(dictaat, antwoord, Taal::En)
    }

    /// Een getal in duizendsten als tekst: "3", "5,5".
    fn toon(g: u128) -> String {
        if g.is_multiple_of(EEN) {
            (g / EEN).to_string()
        } else {
            let decimalen = format!("{:03}", g % EEN);
            format!("{},{}", g / EEN, decimalen.trim_end_matches('0'))
        }
    }

    fn getallen_van(tekst: &str) -> Vec<String> {
        getallen(tekst, Taal::Nl)
            .into_iter()
            .map(|g| toon(g.waarde))
            .collect()
    }

    #[test]
    fn getalwoorden_worden_getallen() {
        for (woord, getal) in [
            ("zeven", 7),
            ("twaalf", 12),
            ("twintig", 20),
            ("drieentwintig", 23),
            ("zesendertig", 36),
            ("vierhonderdvijftig", 450),
            ("honderd", 100),
            ("tweeduizend", 2_000),
            ("negentienhonderdvijfentachtig", 1_985),
            ("vijfenzestig", 65),
        ] {
            let g = getalwoord(woord, woord, Taal::Nl).map(|g| match g {
                Getalwoord::Getal(g) => g / EEN,
                Getalwoord::Keer(g) => g,
            });
            assert_eq!(g, Some(getal), "{woord}");
        }
        for woord in [
            "een", "nul", "beste", "laatste", "tiener", "en", "eend", "vierkant",
        ] {
            assert_eq!(getalwoord(woord, woord, Taal::Nl), None, "{woord}");
        }
    }

    #[test]
    fn rangtelwoorden_tellen_mee() {
        assert_eq!(getallen_van("de derde keer"), ["3"]);
        assert_eq!(getallen_van("de eenendertigste"), ["31"]);
        assert_eq!(getallen_van("op de 31ste"), ["31"]);
        assert_eq!(getallen_van("de achtste"), ["8"]);
        assert_eq!(getallen_van("de vierde"), ["4"]);
    }

    #[test]
    fn opeenvolgende_getalwoorden_worden_een_getal() {
        assert_eq!(getallen_van("tweeduizend zesentwintig"), ["2026"]);
        assert_eq!(getallen_van("twee honderd mensen"), ["200"]);
        assert_eq!(getallen_van("5 miljoen"), ["5000000"]);
        assert_eq!(getallen_van("een miljoen"), ["1000000"]);
        assert_eq!(getallen_van("twee of drie"), ["2", "3"]);
        assert_eq!(getallen_van("vijf euro vijftig"), ["5,5"]);
        assert_eq!(getallen_van("€5,50"), ["5,5"]);
        assert_eq!(getallen_van("om 14:00 uur"), ["14"]);
        assert_eq!(getallen_van("06 12"), ["6", "12"]);
    }

    #[test]
    fn een_leeg_antwoord_wordt_afgekeurd() {
        assert_eq!(controleer("ja prima", ""), Err(Regel::Leeg));
        assert_eq!(controleer("ja prima", "  . , "), Err(Regel::Leeg));
    }

    #[test]
    fn een_netjes_opgeschoond_dictaat_komt_erdoor() {
        assert_eq!(
            controleer(
                "ehm ik moet nog eh even naar de de supermarkt voor melk en brood",
                "Ik moet nog even naar de supermarkt voor melk en brood."
            ),
            Ok(())
        );
        assert_eq!(controleer("ja prima", "Ja, prima."), Ok(()));
        assert_eq!(controleer("oké", "Oké."), Ok(()));
        assert_eq!(
            controleer("dinsdag nee woensdag", "Woensdag."),
            Ok(()),
            "een kort dictaat met een verbetering"
        );
    }

    #[test]
    fn een_beantwoorde_vraag_wordt_afgekeurd() {
        let vraag = "ehm wat is de hoofdstad van frankrijk";
        assert_eq!(controleer(vraag, "Parijs."), Err(Regel::TeWeinigWoorden));
        assert_eq!(
            controleer(vraag, "De hoofdstad van Frankrijk is Parijs."),
            Err(Regel::NieuweNaam)
        );
        assert_eq!(
            controleer(vraag, "Wat is de hoofdstad van Frankrijk?"),
            Ok(())
        );
        assert_eq!(
            controleer(
                "hoeveel is twaalf keer twaalf",
                "Twaalf keer twaalf is 144."
            ),
            Err(Regel::GetalErbij)
        );
    }

    #[test]
    fn een_uitgevoerde_opdracht_wordt_afgekeurd() {
        let opdracht = "schrijf een mail aan piet dat de offerte pas volgende week komt";
        let mail = "Beste Piet, hierbij laat ik je weten dat de offerte helaas pas volgende week komt. \
                    Mocht je vragen hebben, laat het me dan gerust weten. Met vriendelijke groet.";
        assert_eq!(controleer(opdracht, mail), Err(Regel::TeVeelWoorden));
        assert_eq!(
            controleer(
                opdracht,
                "Schrijf een mail aan Piet dat de offerte pas volgende week komt."
            ),
            Ok(())
        );
    }

    #[test]
    fn de_woordgrenzen_tellen_klankwoorden_niet_mee() {
        // Tien woorden zonder klanken: 14 mag (130 procent plus één), 15 niet.
        let dictaat = "ehm een twee drie vier vijf zes zeven acht negen tien";
        assert_eq!(woorden_zonder_klank(dictaat, &NL), 10);
        let antwoord = |n: usize| vec!["woord"; n].join(" ");
        assert_eq!(controleer_woorden(dictaat, &antwoord(14), &NL), Ok(()));
        assert_eq!(
            controleer_woorden(dictaat, &antwoord(15), &NL),
            Err(Regel::TeVeelWoorden)
        );
        // De helft min één (4) mag, 3 niet.
        assert_eq!(controleer_woorden(dictaat, &antwoord(4), &NL), Ok(()));
        assert_eq!(
            controleer_woorden(dictaat, &antwoord(3), &NL),
            Err(Regel::TeWeinigWoorden)
        );
    }

    #[test]
    fn getallen_in_cijfers_mogen() {
        for (dictaat, antwoord) in [
            ("we zijn met twee mensen", "We zijn met 2 mensen."),
            ("het kost vijf euro", "Het kost €5."),
            (
                "het kaartje kost twaalf euro vijftig",
                "Het kaartje kost €12,50.",
            ),
            (
                "het loopt tot tweeduizend achtentwintig",
                "Het loopt tot 2028.",
            ),
            ("de deadline is drie november", "De deadline is 3 november."),
            ("dat is de derde keer", "Dat is de 3e keer."),
            ("de opkomst was vijfenzestig procent", "De opkomst was 65%."),
            ("we beginnen om veertien uur", "We beginnen om 14:00 uur."),
            ("het duurt een uur", "Het duurt 1 uur."),
            ("het duurt 1 uur", "Het duurt een uur."),
            ("punt een de begroting", "1. De begroting"),
            (
                "we hebben drieëntwintig aanmeldingen",
                "We hebben 23 aanmeldingen.",
            ),
        ] {
            assert_eq!(controleer(dictaat, antwoord), Ok(()), "{dictaat}");
        }
    }

    #[test]
    fn een_getal_erbij_of_weg_wordt_afgekeurd() {
        assert_eq!(
            controleer("de factuur is vierhonderd euro", "De factuur is €450."),
            Err(Regel::GetalErbij)
        );
        assert_eq!(
            controleer(
                "we komen op drie november om tien uur",
                "We komen op 3 november."
            ),
            Err(Regel::GetalWeg)
        );
        assert_eq!(
            controleer(
                "we komen morgen naar het kantoor in de stad",
                "We komen morgen, 6 november, naar het kantoor in de stad."
            ),
            Err(Regel::GetalErbij)
        );
    }

    #[test]
    fn een_verbeterd_getal_mag_weg() {
        assert_eq!(
            controleer("zet er drie nee vier stoelen bij", "Zet er 4 stoelen bij."),
            Ok(())
        );
        assert_eq!(
            controleer(
                "we beginnen om drie uur sorry om vier uur",
                "We beginnen om vier uur."
            ),
            Ok(())
        );
        // Het verbeterde getal is wel weg, maar de verbetering zelf mag niet weg.
        assert_eq!(
            controleer("zet er drie nee vier stoelen bij", "Zet er stoelen bij."),
            Err(Regel::GetalWeg)
        );
    }

    #[test]
    fn namen_uit_het_dictaat_mogen_een_hoofdletter_krijgen() {
        assert_eq!(
            controleer(
                "ik heb gebeld met marieke de jong van de gemeente voorbeeldstad",
                "Ik heb gebeld met Marieke de Jong van de gemeente Voorbeeldstad."
            ),
            Ok(())
        );
        assert_eq!(
            controleer(
                "we rijden naar noord holland",
                "We rijden naar Noord-Holland."
            ),
            Ok(())
        );
        assert_eq!(
            controleer("dat is piets laptop", "Dat is Piets laptop."),
            Ok(())
        );
        assert_eq!(
            controleer("ik werk bij aigenwijs", "Ik werk bij Aigenwijs."),
            Ok(())
        );
        // Accenten tellen niet: "australië" en "Australie".
        assert_eq!(
            controleer(
                "wat is de hoofdstad van australië",
                "Wat is de hoofdstad van Australie?"
            ),
            Ok(())
        );
    }

    #[test]
    fn een_nieuwe_naam_wordt_afgekeurd_maar_niet_aan_het_begin_van_een_zin() {
        assert_eq!(
            controleer(
                "stuur het stuk naar de wethouder",
                "Stuur het stuk naar wethouder Jansen."
            ),
            Err(Regel::NieuweNaam)
        );
        // Een nieuw woord aan het begin van een zin is geen naam.
        assert_eq!(
            controleer(
                "het is af ehm en dan moet het naar de drukker",
                "Het is af. Daarna moet het naar de drukker."
            ),
            Ok(())
        );
        // Ook na een regelovergang begint een zin.
        assert_eq!(
            controleer("boodschappen melk brood", "Boodschappen:\nMelk\nBrood"),
            Ok(())
        );
    }

    #[test]
    fn elke_regel_heeft_een_eigen_code() {
        let codes: BTreeSet<&str> = Regel::ALLE.iter().map(|r| r.code()).collect();
        assert_eq!(codes.len(), Regel::ALLE.len());
    }

    // ---- Review O1: deze tests faalden eerst ----

    #[test]
    fn duizendtallen_en_decimalen_met_punt_en_komma() {
        for (dictaat, antwoord) in [
            ("het kost tweeduizend euro", "Het kost €2.000."),
            (
                "we verwachten tienduizend bezoekers",
                "We verwachten 10.000 bezoekers.",
            ),
            ("dat is vijftienhonderd euro", "Dat is €1.500."),
            (
                "het budget is anderhalf miljoen",
                "Het budget is 1,5 miljoen.",
            ),
            (
                "het budget is twee komma vijf miljoen",
                "Het budget is 2,5 miljoen.",
            ),
            (
                "het kaartje kost twaalf euro vijftig",
                "Het kaartje kost €12,50.",
            ),
        ] {
            assert_eq!(controleer(dictaat, antwoord), Ok(()), "{dictaat}");
        }
        assert_eq!(
            controleer("het kost tweeduizend euro", "Het kost €2.500."),
            Err(Regel::GetalErbij)
        );
    }

    #[test]
    fn losse_getalwoorden_worden_goed_opgeteld() {
        assert_eq!(getallen_van("duizend twee honderd"), getallen_van("1200"));
        assert_eq!(
            getallen_van("drie duizend vijf honderd"),
            getallen_van("3500")
        );
        assert_eq!(getallen_van("twaalfhonderd"), getallen_van("1200"));
        assert_eq!(getallen_van("twaalf honderd"), getallen_van("1200"));
    }

    #[test]
    fn een_als_lidwoord_staat_geen_1_toe() {
        assert_eq!(
            controleer(
                "we hebben een afspraak in december",
                "We hebben op 1 december een afspraak."
            ),
            Err(Regel::GetalErbij)
        );
        assert_eq!(controleer("het duurt een uur", "Het duurt 1 uur."), Ok(()));
        assert_eq!(controleer("het duurt één dag", "Het duurt 1 dag."), Ok(()));
    }

    #[test]
    fn een_verbetering_zonder_nieuw_getal_laat_het_getal_niet_vallen() {
        assert_eq!(
            controleer(
                "om drie uur nee dat kan niet eerder",
                "Om drie uur? Nee, dat kan niet eerder."
            ),
            Ok(())
        );
        assert_eq!(
            controleer(
                "om drie uur nee dat kan niet eerder",
                "Nee, dat kan niet eerder."
            ),
            Err(Regel::GetalWeg)
        );
    }

    #[test]
    fn maanden_en_dagen_mogen_niet_veranderen() {
        assert_eq!(
            controleer("de deadline is drie maart", "De deadline is 3 mei."),
            Err(Regel::DatumErbij)
        );
        assert_eq!(
            controleer(
                "we zien elkaar dinsdag op kantoor",
                "We zien elkaar donderdag op kantoor."
            ),
            Err(Regel::DatumErbij)
        );
        assert_eq!(
            controleer(
                "we zien elkaar dinsdag op kantoor",
                "We zien elkaar op kantoor."
            ),
            Err(Regel::DatumWeg)
        );
        assert_eq!(
            controleer(
                "we zien elkaar dinsdag nee woensdag",
                "We zien elkaar woensdag."
            ),
            Ok(())
        );
    }

    #[test]
    fn engelse_getallen_datums_en_verbeteringen() {
        for (dictaat, antwoord) in [
            (
                "we meet on the twenty-first of march at three",
                "We meet on the 21st of March at 3.",
            ),
            ("one hundred and five people", "105 people."),
            ("it costs two thousand euros", "It costs €2,000."),
            ("it costs two point five million", "It costs 2.5 million."),
            ("the third time", "The 3rd time."),
            ("at three no four o'clock", "At four o'clock."),
            ("see you tuesday no wednesday", "See you Wednesday."),
        ] {
            assert_eq!(engels(dictaat, antwoord), Ok(()), "{dictaat}");
        }
        assert_eq!(
            engels("see you on tuesday", "See you on Thursday."),
            Err(Regel::DatumErbij)
        );
        assert_eq!(
            engels(
                "we will start the meeting at three o'clock",
                "We will start the meeting at 3 o'clock, at 15:00."
            ),
            Err(Regel::GetalErbij)
        );
        // In het Nederlands zijn Engelse getalwoorden geen getallen.
        assert_eq!(
            controleer("at three o'clock", "At 3 o'clock."),
            Err(Regel::GetalErbij)
        );
    }

    #[test]
    fn vergelijkbaar_maakt_hele_woorden() {
        assert_eq!(vergelijkbaar("Het kost €12,50!"), "het kost € 12 50");
        assert_eq!(vergelijkbaar("Oké, 65%"), "oke 65 %");
        assert_eq!(vergelijkbaar("  Australië  "), "australie");
    }
}
