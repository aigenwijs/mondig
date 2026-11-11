//! Notulen van een verslag, met bronverwijzing en controle (ADR 0026).
//!
//! Een taalmodel maakt van de beurten van een verslag een samenvatting, besluiten en
//! actiepunten, als JSON volgens een vast schema ([`json_schema`]). Elk besluit en actiepunt heeft
//! een [`Bron`]: het nummer van de beurt en een letterlijk citaat daaruit. Mondig controleert elke
//! bron zelf ([`controleer`]) en bewaart de notulen in `verslag.json` ([`BewaardeNotulen`]).
//!
//! Dit deel is puur en zonder netwerk: het schema, de invoer voor het model, het opdelen van lange
//! verslagen, de controle, het kenmerk van de versie van het gesprek en de opmaak voor de export.
//! Het maken zelf (de aanvragen aan het taalmodel) staat in [`maken`], achter de feature
//! `taalmodel`. Zo kan ook een build zonder die feature `verslag.json` met notulen lezen en
//! schrijven zonder ze kwijt te raken.
//!
//! **Nummering.** Beurten zijn voor het model en in het schema genummerd vanaf 1, zoals de
//! gebruiker ze zou tellen; beurt 1 is `gesprek.beurten[0]`.
//!
//! **Privacy.** Alle typen met tekst hebben bewust geen `Debug`.

#[cfg(feature = "taalmodel")]
pub mod maken;
#[cfg(test)]
mod tests;

use std::ops::Range;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::gesprek::{Gesprek, Vergadersoort};
use crate::persoonsgegevens::Markeringen;

/// De taal van de notulen: die van de interface (ADR 0026 punt 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Taal {
    /// Nederlands.
    #[default]
    Nl,
    /// Engels.
    En,
}

/// Waar een punt in het verslag gezegd is, volgens het model.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bron {
    /// Het nummer van de beurt, vanaf 1. Als `i64` gelezen, zodat een nummer onder 1 alleen dat
    /// punt "niet teruggevonden" maakt en niet het hele antwoord ongeldig.
    pub beurt: i64,
    /// Een letterlijk stuk tekst uit die beurt.
    pub citaat: String,
}

/// Een besluit.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Besluit {
    /// Wat er besloten is.
    pub tekst: String,
    /// Waar het gezegd is.
    pub bron: Bron,
}

/// Een actiepunt.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actiepunt {
    /// Wat er moet gebeuren.
    pub wat: String,
    /// Wie het doet; leeg als dat niet gezegd is.
    pub wie: String,
    /// Wanneer het klaar moet zijn; leeg als dat niet gezegd is.
    pub wanneer: String,
    /// Waar het gezegd is.
    pub bron: Bron,
}

/// Notulen zoals het model ze geeft (het schema van ADR 0026 punt 1).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notulen {
    /// Hooguit vijf zinnen.
    pub samenvatting: String,
    /// De besluiten.
    pub besluiten: Vec<Besluit>,
    /// De actiepunten.
    pub actiepunten: Vec<Actiepunt>,
}

/// De uitkomst van de controle van een bron (ADR 0026 punt 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "uitkomst", rename_all = "snake_case")]
pub enum Controle {
    /// Het citaat staat in deze beurt (vanaf 1): de genoemde beurt of die ervoor of erna.
    Gevonden {
        /// De beurt waarin het citaat echt staat.
        beurt: usize,
    },
    /// De beurt bestaat niet, of het citaat staat er niet in (ook niet ervoor of erna).
    NietTeruggevonden,
}

impl Controle {
    /// Is de bron gevonden?
    #[must_use]
    pub fn gevonden(self) -> bool {
        matches!(self, Self::Gevonden { .. })
    }
}

/// Een besluit of actiepunt met de uitkomst van de controle. De gebruiker kan het punt later
/// aanpassen; de controle blijft dan staan (ADR 0026 punt 7).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gecontroleerd<T> {
    /// Het punt zoals het model het gaf (of zoals de gebruiker het aanpaste).
    #[serde(flatten)]
    pub punt: T,
    /// Een vast nummer van dit punt binnen zijn lijst (vanaf 1). Het verandert niet als er een
    /// punt wordt weggehaald, zodat een commando van de interface nooit het verkeerde punt raakt.
    /// Oudere bestanden zonder nummer krijgen 0; de app geeft ze dan een nummer.
    #[serde(default)]
    pub id: u32,
    /// De uitkomst van de controle.
    pub controle: Controle,
    /// Een actiepunt: staat `wie` niet in de bronbeurt (of ervoor of erna) en is het ook niet de
    /// spreker daarvan? Dan blijft het staan, met deze markering.
    #[serde(default, skip_serializing_if = "is_onwaar")]
    pub wie_niet_teruggevonden: bool,
    /// Een actiepunt: staat `wanneer` niet in de bronbeurt (of ervoor of erna)?
    #[serde(default, skip_serializing_if = "is_onwaar")]
    pub wanneer_niet_teruggevonden: bool,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_onwaar(waarde: &bool) -> bool {
    !*waarde
}

/// Notulen zoals ze in `verslag.json` staan (ADR 0026 punt 5).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BewaardeNotulen {
    /// Hooguit vijf zinnen.
    pub samenvatting: String,
    /// De besluiten met hun controle.
    pub besluiten: Vec<Gecontroleerd<Besluit>>,
    /// De actiepunten met hun controle.
    pub actiepunten: Vec<Gecontroleerd<Actiepunt>>,
    /// De vaste code van de aanbieder (`mistral` of `ollama`).
    pub aanbieder: String,
    /// De modelnaam bij de aanbieder.
    pub model: String,
    /// Wanneer de notulen gemaakt zijn, in milliseconden sinds 1970.
    pub moment_ms: u64,
    /// Het kenmerk van het gesprek waarvan ze gemaakt zijn ([`kenmerk`]).
    pub gesprek_kenmerk: String,
    /// De taal van de notulen.
    #[serde(default)]
    pub taal: Taal,
}

impl BewaardeNotulen {
    /// Zijn de notulen gemaakt bij een andere versie van het gesprek, bijvoorbeeld voor het
    /// opnieuw uitschrijven (ADR 0026 punt 6)? Dan zijn de bronnen niet meer klikbaar.
    #[must_use]
    pub fn bij_vorige_versie(&self, gesprek: &Gesprek) -> bool {
        self.gesprek_kenmerk != kenmerk(gesprek)
    }

    /// Controleert alle bronnen en velden opnieuw tegen de teksten in `invoer`, bijvoorbeeld na
    /// een verbeterde regel: een citaat dat de gebruiker wegverbeterde, wordt zo "niet
    /// teruggevonden". De tekst van een punt verandert niet. Alleen zinvol zolang de notulen bij
    /// de huidige opbouw van het gesprek horen ([`BewaardeNotulen::bij_vorige_versie`]).
    pub fn hercontroleer(&mut self, invoer: &Invoer) {
        for b in &mut self.besluiten {
            b.controle = controleer(&b.punt.bron, &invoer.teksten);
        }
        for a in &mut self.actiepunten {
            a.controle = controleer(&a.punt.bron, &invoer.teksten);
            let (wie, wanneer) = controleer_velden(&a.punt, a.controle, invoer);
            a.wie_niet_teruggevonden = wie;
            a.wanneer_niet_teruggevonden = wanneer;
        }
    }

    /// Laat de notulen meegaan met een wijziging aan de sprekers van het gesprek, van `oud` naar
    /// `nieuw` (een naam, samenvoegen, een beurt aan een andere spreker). Gingen er beurten samen
    /// terwijl de tekst gelijk bleef ([`crate::gesprek::beurtkaart`]), dan wijzen de bronnen naar
    /// de nieuwe plek en horen de notulen bij de nieuwe opbouw, dus niet bij een vorige versie.
    /// Een spreker die nu anders heet ([`crate::gesprek::naamwijzigingen`]), heet in `wie` ook
    /// zo: "Spreker 1" wordt "Noor". Daarna hoort [`BewaardeNotulen::hercontroleer`].
    ///
    /// Notulen die al bij een vorige versie hoorden, blijven zoals ze zijn.
    pub fn volg(&mut self, oud: &Gesprek, nieuw: &Gesprek) {
        if self.bij_vorige_versie(oud) {
            return;
        }
        let Some(kaart) = crate::gesprek::beurtkaart(oud, nieuw) else {
            return;
        };
        let nieuwe_plek = |beurt: usize| kaart.get(beurt.wrapping_sub(1)).map(|p| p + 1);
        let verplaats = |bron: &mut Bron, controle: &mut Controle| {
            // De beurt waarin het citaat stond, anders de beurt die het model noemde.
            let oude_beurt = match *controle {
                Controle::Gevonden { beurt } => Some(beurt),
                Controle::NietTeruggevonden => bestaande_beurt(bron.beurt, kaart.len()),
            };
            if let Some(beurt) = oude_beurt.and_then(nieuwe_plek) {
                bron.beurt = i64::try_from(beurt).unwrap_or(i64::MAX);
                if controle.gevonden() {
                    *controle = Controle::Gevonden { beurt };
                }
            }
        };
        for b in &mut self.besluiten {
            verplaats(&mut b.punt.bron, &mut b.controle);
        }
        for a in &mut self.actiepunten {
            verplaats(&mut a.punt.bron, &mut a.controle);
        }
        self.gesprek_kenmerk = kenmerk(nieuw);
        let namen = crate::gesprek::naamwijzigingen(oud, nieuw);
        for a in &mut self.actiepunten {
            let wie = normaliseer(&a.punt.wie);
            if let Some((_, naar)) = namen.iter().find(|(van, _)| normaliseer(van) == wie) {
                a.punt.wie.clone_from(naar);
            }
        }
    }

    /// Geeft elk punt zonder nummer (0) een nummer dat nog niet bestaat. Geeft `true` als er iets
    /// veranderde.
    pub fn nummer_punten(&mut self) -> bool {
        fn nummer<T>(lijst: &mut [Gecontroleerd<T>]) -> bool {
            let eerste = lijst.iter().map(|p| p.id).max().unwrap_or(0) + 1;
            let mut veranderd = false;
            for (id, punt) in (eerste..).zip(lijst.iter_mut().filter(|p| p.id == 0)) {
                punt.id = id;
                veranderd = true;
            }
            veranderd
        }
        let a = nummer(&mut self.besluiten);
        let b = nummer(&mut self.actiepunten);
        a || b
    }

    /// Alle controles, eerst van de besluiten en dan van de actiepunten.
    pub fn controles(&self) -> impl Iterator<Item = Controle> + '_ {
        self.besluiten
            .iter()
            .map(|b| b.controle)
            .chain(self.actiepunten.iter().map(|a| a.controle))
    }

    /// Hoeveel actiepunten een markering bij `wie` of `wanneer` hebben (elk punt één keer).
    #[must_use]
    pub fn veldmarkeringen(&self) -> usize {
        self.actiepunten
            .iter()
            .filter(|a| a.wie_niet_teruggevonden || a.wanneer_niet_teruggevonden)
            .count()
    }

    /// Hoeveel bronnen gevonden en hoeveel niet teruggevonden zijn.
    #[must_use]
    pub fn telling(&self) -> (usize, usize) {
        let gevonden = self.controles().filter(|c| c.gevonden()).count();
        (gevonden, self.controles().count() - gevonden)
    }

    /// De notulen opgemaakt voor de export, vóór het verslag (ADR 0026 punt 8): in de taal van de
    /// notulen, met bij elk punt de tijd van de bron en met de persoonsgegevens vervangen zoals in
    /// het verslag. Bij een vorige versie van het gesprek staan er geen tijden bij, want die
    /// kloppen dan niet meer.
    #[cfg(feature = "bewaren")]
    #[must_use]
    pub fn als_blok(
        &self,
        gesprek: &Gesprek,
        markeringen: &Markeringen,
    ) -> crate::bewaren::Notulenblok {
        use crate::bewaren::{Notulenblok, Notulenregel};

        let l = Labels::voor(self.taal);
        let vorige = self.bij_vorige_versie(gesprek);
        let tijd = |controle: Controle| match controle {
            Controle::Gevonden { beurt } if !vorige => gesprek
                .beurten
                .get(beurt.wrapping_sub(1))
                .map(|b| b.start_ms),
            _ => None,
        };
        let niet_gevonden = |controle: Controle, tekst: String| {
            if controle.gevonden() {
                tekst
            } else {
                format!("{tekst} [{}]", l.niet_teruggevonden)
            }
        };
        let uit = |tekst: &str| markeringen.voor_uitvoer(tekst);

        let mut regels = vec![Notulenregel::Kop(l.notulen.to_string())];
        if vorige {
            regels.push(Notulenregel::Alinea(l.vorige_versie.to_string()));
        }
        regels.push(Notulenregel::Alinea(format!(
            "{} ({}, {}). {}",
            l.gemaakt_door,
            aanbiedernaam(&self.aanbieder),
            self.model,
            l.lees_na
        )));
        regels.push(Notulenregel::Tussenkop(l.samenvatting.to_string()));
        regels.push(Notulenregel::Alinea(l.samenvatting_door_model.to_string()));
        regels.push(Notulenregel::Alinea(uit(&self.samenvatting)));

        regels.push(Notulenregel::Tussenkop(l.besluiten.to_string()));
        if self.besluiten.is_empty() {
            regels.push(Notulenregel::Alinea(l.geen_besluiten.to_string()));
        }
        for b in &self.besluiten {
            regels.push(Notulenregel::Punt {
                tijd_ms: tijd(b.controle),
                tekst: niet_gevonden(b.controle, uit(&b.punt.tekst)),
            });
        }

        regels.push(Notulenregel::Tussenkop(l.actiepunten.to_string()));
        if self.actiepunten.is_empty() {
            regels.push(Notulenregel::Alinea(l.geen_actiepunten.to_string()));
        }
        for a in &self.actiepunten {
            let mut erbij = Vec::new();
            let veld = |niet: bool| {
                if niet {
                    format!(" ({})", l.veld_niet_gevonden)
                } else {
                    String::new()
                }
            };
            if !a.punt.wie.trim().is_empty() {
                erbij.push(format!(
                    "{}: {}{}",
                    l.wie,
                    markeringen.voor_naam(&a.punt.wie),
                    veld(a.wie_niet_teruggevonden)
                ));
            }
            if !a.punt.wanneer.trim().is_empty() {
                erbij.push(format!(
                    "{}: {}{}",
                    l.wanneer,
                    uit(&a.punt.wanneer),
                    veld(a.wanneer_niet_teruggevonden)
                ));
            }
            let mut tekst = uit(&a.punt.wat);
            if !erbij.is_empty() {
                tekst = format!("{tekst} ({})", erbij.join("; "));
            }
            regels.push(Notulenregel::Punt {
                tijd_ms: tijd(a.controle),
                tekst: niet_gevonden(a.controle, tekst),
            });
        }
        Notulenblok {
            regels,
            verslagkop: l.verslag.to_string(),
            taal: self.taal,
            verwerking: verwerking_buiten_computer(&self.aanbieder),
        }
    }
}

/// De vaste teksten van de notulen in een taal.
#[cfg_attr(not(feature = "bewaren"), allow(dead_code))]
struct Labels {
    notulen: &'static str,
    samenvatting: &'static str,
    besluiten: &'static str,
    actiepunten: &'static str,
    verslag: &'static str,
    wie: &'static str,
    wanneer: &'static str,
    niet_teruggevonden: &'static str,
    vorige_versie: &'static str,
    gemaakt_door: &'static str,
    lees_na: &'static str,
    geen_besluiten: &'static str,
    geen_actiepunten: &'static str,
    samenvatting_door_model: &'static str,
    veld_niet_gevonden: &'static str,
}

impl Labels {
    #[cfg_attr(not(feature = "bewaren"), allow(dead_code))]
    const fn voor(taal: Taal) -> Self {
        match taal {
            Taal::Nl => Self {
                notulen: "Notulen",
                samenvatting: "Samenvatting",
                besluiten: "Besluiten",
                actiepunten: "Actiepunten",
                verslag: "Verslag",
                wie: "wie",
                wanneer: "wanneer",
                niet_teruggevonden: "Niet teruggevonden in het verslag",
                vorige_versie: "Gemaakt bij de vorige versie van het verslag. De tijden kloppen mogelijk niet meer.",
                gemaakt_door: "Gemaakt door een taalmodel",
                lees_na: "Lees de notulen na.",
                geen_besluiten: "Geen besluiten.",
                geen_actiepunten: "Geen actiepunten.",
                samenvatting_door_model: "Samenvatting gemaakt door een taalmodel; lees het verslag na.",
                veld_niet_gevonden: "niet in het verslag gevonden",
            },
            Taal::En => Self {
                notulen: "Minutes",
                samenvatting: "Summary",
                besluiten: "Decisions",
                actiepunten: "Action items",
                verslag: "Transcript",
                wie: "who",
                wanneer: "when",
                niet_teruggevonden: "Not found in the transcript",
                vorige_versie: "Made from the previous version of the transcript. The times may no longer match.",
                gemaakt_door: "Made by a language model",
                lees_na: "Check the minutes.",
                geen_besluiten: "No decisions.",
                geen_actiepunten: "No action items.",
                samenvatting_door_model: "Summary made by a language model; check the transcript.",
                veld_niet_gevonden: "not found in the transcript",
            },
        }
    }
}

/// Door wie de notulen zijn gemaakt als dat buiten deze computer gebeurde, met het
/// soevereiniteitslabel, voor de gegevens in het document. `None` bij Ollama (op deze computer).
/// Een onbekende code blijft zichtbaar, zodat een export nooit stilletjes "lokaal" zegt.
#[cfg_attr(not(feature = "bewaren"), allow(dead_code))]
fn verwerking_buiten_computer(code: &str) -> Option<String> {
    match code {
        "ollama" => None,
        "mistral" => Some("Mistral (Europees)".to_string()),
        andere => Some(andere.to_string()),
    }
}

/// De naam van een aanbieder bij zijn vaste code, voor in de export.
#[cfg_attr(not(feature = "bewaren"), allow(dead_code))]
fn aanbiedernaam(code: &str) -> &str {
    match code {
        "mistral" => "Mistral",
        "ollama" => "Ollama",
        andere => andere,
    }
}

/// Het veld `notulen` van `verslag.json` zoals het op schijf staat. Notulen die deze versie niet
/// kan lezen (beschadigd, of van een nieuwere opbouw) blijven als ruwe JSON bewaard en gaan
/// ongewijzigd terug naar schijf; ze worden alleen niet getoond. Zo verdwijnen notulen nooit stil.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpgeslagenNotulen {
    /// Notulen die deze versie kan lezen.
    Leesbaar(BewaardeNotulen),
    /// Notulen die deze versie niet kan lezen, precies zoals ze er stonden.
    Onleesbaar(Value),
}

impl OpgeslagenNotulen {
    /// De notulen als ze te lezen zijn.
    #[must_use]
    pub fn leesbaar(&self) -> Option<&BewaardeNotulen> {
        match self {
            Self::Leesbaar(notulen) => Some(notulen),
            Self::Onleesbaar(_) => None,
        }
    }
}

/// Het JSON-schema van het samenvoegen van de samenvattingen.
#[must_use]
pub fn samenvatting_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "samenvatting": { "type": "string" } },
        "required": ["samenvatting"],
        "additionalProperties": false,
    })
}

/// Hetzelfde punt uit twee delen? Ja als de bron binnen één beurt ligt en het ene genormaliseerde
/// citaat het andere bevat (of gelijk is).
fn zelfde_bron(a: &Bron, b: &Bron) -> bool {
    if a.beurt.abs_diff(b.beurt) > 1 {
        return false;
    }
    let (x, y) = (normaliseer(&a.citaat), normaliseer(&b.citaat));
    if x.is_empty() || y.is_empty() {
        return false;
    }
    let (x, y) = (format!(" {x} "), format!(" {y} "));
    x.contains(&y) || y.contains(&x)
}

/// Voegt de punten van de delen samen, in volgorde: alles wordt overgenomen, alleen een punt met
/// dezelfde bron als een eerder punt (door de overlap van de delen) valt weg. De samenvatting is
/// leeg; die komt van het model.
#[must_use]
pub fn voeg_punten_samen(delen: Vec<Notulen>) -> Notulen {
    let mut samen = Notulen {
        samenvatting: String::new(),
        besluiten: Vec::new(),
        actiepunten: Vec::new(),
    };
    for deel in delen {
        for besluit in deel.besluiten {
            if !samen
                .besluiten
                .iter()
                .any(|b| zelfde_bron(&b.bron, &besluit.bron))
            {
                samen.besluiten.push(besluit);
            }
        }
        for actie in deel.actiepunten {
            if !samen
                .actiepunten
                .iter()
                .any(|a| zelfde_bron(&a.bron, &actie.bron))
            {
                samen.actiepunten.push(actie);
            }
        }
    }
    samen
}

/// Het JSON-schema van het antwoord (`response_format`), streng: alle velden verplicht en geen
/// andere velden, zoals Mistral en Ollama dat vragen.
#[must_use]
pub fn json_schema() -> Value {
    let bron = json!({
        "type": "object",
        "properties": {
            "beurt": { "type": "integer", "minimum": 1 },
            "citaat": { "type": "string" },
        },
        "required": ["beurt", "citaat"],
        "additionalProperties": false,
    });
    json!({
        "type": "object",
        "properties": {
            "samenvatting": { "type": "string" },
            "besluiten": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "tekst": { "type": "string" },
                        "bron": bron,
                    },
                    "required": ["tekst", "bron"],
                    "additionalProperties": false,
                },
            },
            "actiepunten": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "wat": { "type": "string" },
                        "wie": { "type": "string" },
                        "wanneer": { "type": "string" },
                        "bron": bron,
                    },
                    "required": ["wat", "wie", "wanneer", "bron"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["samenvatting", "besluiten", "actiepunten"],
        "additionalProperties": false,
    })
}

/// Welke regels van het verslag een sprekerlabel hebben voor het model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sprekerlabels {
    /// Elke regel heeft een spreker.
    Alle,
    /// Een deel van de regels heeft een spreker (in de ruimte waar niet elke alinea is
    /// aangewezen).
    Deels,
    /// Geen enkele regel heeft een spreker.
    Geen,
}

/// De opdracht aan het model (systeembericht) voor notulen van een verslag of een deel ervan.
/// Zonder sprekerlabels staat er geen naam voor de tekst en vraagt de opdracht alleen een "wie" als
/// de tekst zelf een naam noemt; bij `Deels` geldt dat voor de regels zonder label.
#[must_use]
pub fn opdracht(taal: Taal, labels: Sprekerlabels) -> String {
    match (taal, labels) {
        (Taal::Nl, Sprekerlabels::Alle) => OPDRACHT_NL.to_string(),
        (Taal::En, Sprekerlabels::Alle) => OPDRACHT_EN.to_string(),
        (Taal::Nl, Sprekerlabels::Geen) => OPDRACHT_ZONDER_SPREKERS_NL.to_string(),
        (Taal::En, Sprekerlabels::Geen) => OPDRACHT_ZONDER_SPREKERS_EN.to_string(),
        (Taal::Nl, Sprekerlabels::Deels) => format!(
            "{OPDRACHT_NL}
{DEELS_NL}"
        ),
        (Taal::En, Sprekerlabels::Deels) => format!(
            "{OPDRACHT_EN}
{DEELS_EN}"
        ),
    }
}

const DEELS_NL: &str = "8. Bij een regel zonder spreker (de vorm \"[nummer] tijd: tekst\") is niet bekend wie er spreekt: vul wie dan alleen in als de tekst zelf een naam of rol noemt, anders leeg, en nooit \"ik\", \"jij\" of \"de spreker\".";
const DEELS_EN: &str = "8. For a line without a speaker (in the form \"[number] time: text\") it is not known who is speaking: fill in wie only if the text itself mentions a name or role, otherwise leave it empty, and never \"I\", \"you\" or \"the speaker\".";

/// De opdracht om de samenvattingen van de delen samen te voegen tot één samenvatting. De punten
/// voegt Mondig zelf samen ([`voeg_punten_samen`]), zodat het model er geen kan laten vallen.
#[must_use]
pub fn samenvoegopdracht(taal: Taal) -> &'static str {
    match taal {
        Taal::Nl => SAMENVOEGEN_NL,
        Taal::En => SAMENVOEGEN_EN,
    }
}

const OPDRACHT_NL: &str = "\
Je maakt notulen van een vergadering. Je krijgt het verslag als genummerde beurten in de vorm \"[nummer] tijd spreker: tekst\". Het verslag is automatisch uitgeschreven en kan fouten bevatten.

Regels:
1. Gebruik alleen wat in het verslag staat. Voeg niets toe en vul niets aan uit eigen kennis.
2. samenvatting: hooguit vijf zinnen over waar de vergadering over ging en wat eruit kwam.
3. besluiten: alleen wat de deelnemers echt hebben besloten of afgesproken, zoals \"dan doen we het zo\", \"akkoord\" of \"afgesproken\". Een voorstel, idee, wens of twijfel is geen besluit, ook niet als niemand bezwaar maakt. Zinnen als \"we zouden misschien\", \"eventueel\", \"moeten we niet eens\" of \"dat kunnen we overwegen\" zijn geen besluit. Bij twijfel: geen besluit. Elk besluit staat er één keer: stemmen meerdere deelnemers in met hetzelfde besluit, dan is dat één besluit met als bron de beurt waarin het besluit wordt uitgesproken. Een instemming als \"akkoord\" of \"ja\" is nooit een apart besluit.
4. actiepunten: een concrete taak die iemand echt op zich neemt of krijgt. Een aanbod of mogelijkheid zoals \"ik kan wel\", \"we zouden kunnen\" of \"misschien later\" is geen actiepunt als niemand de taak daarna aanneemt. wie: de naam of rol zoals die in het verslag staat; zegt een spreker \"ik doe dat\" of \"ik pak dat op\", dan is wie de naam van die spreker. Is niet gezegd wie het doet, laat wie leeg. wanneer: alleen een moment of termijn die in het verslag genoemd wordt; anders leeg. Nooit raden.
5. bron bij elk besluit en actiepunt: beurt is het nummer van de beurt waarin het besluit of de taak wordt uitgesproken. citaat is een letterlijk stuk uit die beurt van vijf tot vijftien woorden (minstens vier), precies zoals het er staat: niet herschrijven, niet inkorten met puntjes, geen woorden toevoegen. Zet het nummer, de tijd en de naam van de spreker niet in het citaat, alleen wat er gezegd is.
6. Staan er geen besluiten of actiepunten in het verslag, geef dan een lege lijst.
7. Schrijf kort en zakelijk in het Nederlands. Geef alleen JSON volgens het schema.";

const OPDRACHT_EN: &str = "\
You write the minutes of a meeting. You get the transcript as numbered turns in the form \"[number] time speaker: text\". The transcript was made automatically and may contain errors.

Rules:
1. Use only what is in the transcript. Do not add anything or fill in from your own knowledge.
2. samenvatting (summary): at most five sentences on what the meeting was about and what came out of it.
3. besluiten (decisions): only what the participants actually decided or agreed, such as \"then we will do it that way\", \"agreed\" or \"settled\". A proposal, idea, wish or doubt is not a decision, even if nobody objects. Phrases like \"maybe we could\", \"possibly\", \"shouldn't we\" or \"we could consider that\" are not decisions. When in doubt: no decision. List each decision once: if several participants agree to the same decision, that is one decision, with as source the turn in which it is stated. An agreement such as \"agreed\" or \"yes\" is never a separate decision.
4. actiepunten (action items): a concrete task that someone actually takes on or is given. An offer or possibility such as \"I could\", \"we might\" or \"maybe later\" is not an action item if nobody then takes the task on. wie (who): the name or role as it appears in the transcript; if a speaker says \"I will do that\", wie is that speaker's name. If nobody was named, leave wie empty. wanneer (when): only a moment or deadline mentioned in the transcript; otherwise empty. Never guess.
5. bron (source) for every decision and action item: beurt is the number of the turn in which the decision or task is stated. citaat is a literal quote from that turn of five to fifteen words (at least four), exactly as written: do not rewrite, do not shorten with dots, do not add words. Do not put the number, the time or the speaker name in the quote, only what was said. Keep the quote in the language of the transcript.
6. If there are no decisions or action items, give an empty list.
7. Write briefly and factually in English. Give only JSON following the schema.";

// Zonder sprekers weet het verslag niet wie er spreekt (een opname in de ruimte zonder
// sprekerherkenning, of Alleen ik): alleen kop en regel 4 verschillen van de opdracht met sprekers.
const OPDRACHT_ZONDER_SPREKERS_NL: &str = "\
Je maakt notulen van een vergadering. Je krijgt het verslag als genummerde beurten in de vorm \"[nummer] tijd: tekst\". Het verslag is automatisch uitgeschreven en kan fouten bevatten. Er staat niet bij wie er spreekt.

Regels:
1. Gebruik alleen wat in het verslag staat. Voeg niets toe en vul niets aan uit eigen kennis.
2. samenvatting: hooguit vijf zinnen over waar de vergadering over ging en wat eruit kwam.
3. besluiten: alleen wat de deelnemers echt hebben besloten of afgesproken, zoals \"dan doen we het zo\", \"akkoord\" of \"afgesproken\". Een voorstel, idee, wens of twijfel is geen besluit, ook niet als niemand bezwaar maakt. Zinnen als \"we zouden misschien\", \"eventueel\", \"moeten we niet eens\" of \"dat kunnen we overwegen\" zijn geen besluit. Bij twijfel: geen besluit. Elk besluit staat er één keer: stemmen meerdere deelnemers in met hetzelfde besluit, dan is dat één besluit met als bron de beurt waarin het besluit wordt uitgesproken. Een instemming als \"akkoord\" of \"ja\" is nooit een apart besluit.
4. actiepunten: een concrete taak die iemand echt op zich neemt of krijgt. Een aanbod of mogelijkheid zoals \"ik kan wel\", \"we zouden kunnen\" of \"misschien later\" is geen actiepunt als niemand de taak daarna aanneemt. wie: alleen een naam of rol die in de tekst zelf bij de taak wordt genoemd, zoals \"Piet pakt dat op\" of \"de projectleider regelt het\". Het verslag zegt niet wie er spreekt: bij \"ik doe dat\" of \"ik pak dat op\" is wie dus leeg, en vul nooit \"ik\", \"jij\" of \"de spreker\" in. Wordt er geen naam of rol genoemd, laat wie leeg. wanneer: alleen een moment of termijn die in het verslag genoemd wordt; anders leeg. Nooit raden.
5. bron bij elk besluit en actiepunt: beurt is het nummer van de beurt waarin het besluit of de taak wordt uitgesproken. citaat is een letterlijk stuk uit die beurt van vijf tot vijftien woorden (minstens vier), precies zoals het er staat: niet herschrijven, niet inkorten met puntjes, geen woorden toevoegen. Zet het nummer en de tijd niet in het citaat, alleen wat er gezegd is.
6. Staan er geen besluiten of actiepunten in het verslag, geef dan een lege lijst.
7. Schrijf kort en zakelijk in het Nederlands. Geef alleen JSON volgens het schema.";

const OPDRACHT_ZONDER_SPREKERS_EN: &str = "\
You write the minutes of a meeting. You get the transcript as numbered turns in the form \"[number] time: text\". The transcript was made automatically and may contain errors. It does not say who is speaking.

Rules:
1. Use only what is in the transcript. Do not add anything or fill in from your own knowledge.
2. samenvatting (summary): at most five sentences on what the meeting was about and what came out of it.
3. besluiten (decisions): only what the participants actually decided or agreed, such as \"then we will do it that way\", \"agreed\" or \"settled\". A proposal, idea, wish or doubt is not a decision, even if nobody objects. Phrases like \"maybe we could\", \"possibly\", \"shouldn't we\" or \"we could consider that\" are not decisions. When in doubt: no decision. List each decision once: if several participants agree to the same decision, that is one decision, with as source the turn in which it is stated. An agreement such as \"agreed\" or \"yes\" is never a separate decision.
4. actiepunten (action items): a concrete task that someone actually takes on or is given. An offer or possibility such as \"I could\", \"we might\" or \"maybe later\" is not an action item if nobody then takes the task on. wie (who): only a name or role that the text itself mentions with the task, such as \"Piet will pick that up\" or \"the project lead will arrange it\". The transcript does not say who is speaking: for \"I will do that\" wie is therefore empty, and never fill in \"I\", \"you\" or \"the speaker\". If no name or role is mentioned, leave wie empty. wanneer (when): only a moment or deadline mentioned in the transcript; otherwise empty. Never guess.
5. bron (source) for every decision and action item: beurt is the number of the turn in which the decision or task is stated. citaat is a literal quote from that turn of five to fifteen words (at least four), exactly as written: do not rewrite, do not shorten with dots, do not add words. Do not put the number or the time in the quote, only what was said. Keep the quote in the language of the transcript.
6. If there are no decisions or action items, give an empty list.
7. Write briefly and factually in English. Give only JSON following the schema.";

const SAMENVOEGEN_NL: &str = "\
Je krijgt de samenvattingen van opeenvolgende delen van één vergadering, als JSON. Maak er één samenvatting van van hooguit vijf zinnen over de hele vergadering, alleen op basis van deze samenvattingen. Voeg niets toe. Schrijf in het Nederlands. Geef alleen JSON volgens het schema.";

const SAMENVOEGEN_EN: &str = "\
You get the summaries of consecutive parts of one meeting, as JSON. Make one summary of at most five sentences about the whole meeting, based only on these summaries. Do not add anything. Write in English. Give only JSON following the schema.";

/// De invoer voor het model: per beurt een regel "[nummer] tijd spreker: tekst", en de tekst
/// zoals die naar het model gaat (voor de controle).
pub struct Invoer {
    /// Per beurt de regel voor het model.
    pub regels: Vec<String>,
    /// Per beurt de tekst zoals het model hem kreeg.
    pub teksten: Vec<String>,
    /// Per beurt de spreker zoals het model hem kreeg.
    pub sprekers: Vec<String>,
    /// Welke regels een sprekerlabel kregen. Een regel zonder label heeft hierboven een lege
    /// spreker.
    pub labels: Sprekerlabels,
}

impl Invoer {
    /// De invoer bij een gesprek. Staat "Persoonsgegevens vervangen" aan, dan krijgt het model de
    /// vervangen tekst en sprekernamen (ADR 0024 punt 4); anders de tekst zoals hij is. Een regel
    /// zonder spreker ([`Gesprek::label`], in de ruimte een alinea die niemand aanwees) heeft
    /// alleen het nummer, de tijd en de tekst: anders heet hij "Jij" en wordt "Jij" de eigenaar
    /// van elk actiepunt. Bij Alleen ik is "Jij" wel de schrijver: elke regel houdt zijn label.
    #[must_use]
    pub fn van(gesprek: &Gesprek, markeringen: &Markeringen) -> Self {
        let alleen = gesprek.soort == Vergadersoort::Alleen;
        let mut regels = Vec::with_capacity(gesprek.beurten.len());
        let mut teksten = Vec::with_capacity(gesprek.beurten.len());
        let mut sprekers = Vec::with_capacity(gesprek.beurten.len());
        for (index, beurt) in gesprek.beurten.iter().enumerate() {
            let tekst = op_een_regel(&markeringen.voor_uitvoer(&beurt.tekst));
            let tijd = tijdcode(beurt.start_ms);
            let nummer = index + 1;
            let label = if alleen {
                Some(beurt.spreker())
            } else if gesprek.met_sprekers() {
                gesprek.label(beurt)
            } else {
                None
            };
            if let Some(naam) = label {
                let spreker = op_een_regel(&markeringen.voor_naam(&naam));
                regels.push(format!("[{nummer}] {tijd} {spreker}: {tekst}"));
                sprekers.push(spreker);
            } else {
                regels.push(format!("[{nummer}] {tijd}: {tekst}"));
                sprekers.push(String::new());
            }
            teksten.push(tekst);
        }
        let met = sprekers.iter().filter(|s| !s.is_empty()).count();
        let labels = if met == sprekers.len() {
            Sprekerlabels::Alle
        } else if met == 0 {
            Sprekerlabels::Geen
        } else {
            Sprekerlabels::Deels
        };
        Self {
            regels,
            teksten,
            sprekers,
            labels,
        }
    }

    /// Heeft het gesprek tekst?
    #[must_use]
    pub fn heeft_tekst(&self) -> bool {
        self.teksten.iter().any(|t| !t.trim().is_empty())
    }

    /// De geschatte tokens per regel ([`schat_tokens`], plus één voor het regeleinde).
    #[must_use]
    pub fn schattingen(&self) -> Vec<usize> {
        self.regels.iter().map(|r| schat_tokens(r) + 1).collect()
    }
}

/// Witruimte (ook regeleinden) wordt één spatie, zodat een beurt één regel is.
fn op_een_regel(tekst: &str) -> String {
    tekst.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// "00:07", "12:40", boven het uur "1:02:40", zoals het scherm. De enige opmaak van een tijd voor
/// verslagen en notulen (ook voor de export en de opdrachtregel).
#[must_use]
pub fn tijdcode(ms: u64) -> String {
    let totaal = ms / 1000;
    let (uren, minuten, seconden) = (totaal / 3600, (totaal % 3600) / 60, totaal % 60);
    if uren > 0 {
        format!("{uren}:{minuten:02}:{seconden:02}")
    } else {
        format!("{minuten:02}:{seconden:02}")
    }
}

/// Een ruwe, voorzichtige schatting van het aantal tokens: één per drie tekens. Nederlandse tekst
/// is bij de gangbare tokenizers eerder vier tekens per token, dus dit schat te hoog en houdt
/// marge.
#[must_use]
pub fn schat_tokens(tekst: &str) -> usize {
    tekst.chars().count().div_ceil(3)
}

/// Deelt beurten op in delen van hooguit `budget` geschatte tokens, op beurtgrenzen, met `overlap`
/// beurten overlap tussen twee delen (ADR 0026 punt 4). Een beurt die alleen al groter is dan het
/// budget is een deel op zich. Past alles in het budget, dan is er één deel. De overlap is hooguit
/// de helft van een deel, en elk deel begint later dan het vorige, dus het opdelen eindigt altijd.
#[must_use]
pub fn delen(schattingen: &[usize], budget: usize, overlap: usize) -> Vec<Range<usize>> {
    let aantal = schattingen.len();
    let mut uit = Vec::new();
    let mut begin = 0;
    while begin < aantal {
        let mut eind = begin;
        let mut som = 0;
        while eind < aantal && (eind == begin || som + schattingen[eind] <= budget) {
            som += schattingen[eind];
            eind += 1;
        }
        uit.push(begin..eind);
        if eind == aantal {
            break;
        }
        // De overlap is hooguit de helft van het deel, zodat elk volgend deel minstens een half
        // deel verder begint.
        let stuk = eind - begin;
        begin = (eind - overlap.min(stuk / 2)).max(begin + 1);
    }
    uit
}

/// Een kenmerk van de opbouw van het gesprek: FNV-1a (64 bit) over het aantal beurten en van elke
/// beurt de tijden, de kant en het sprekernummer. Niet de tekst en niet de namen: een kleine
/// tekstverbetering of een andere naam verandert het niet, want de beurten blijven op hun plek en
/// de bronnen blijven kloppen (de app controleert ze dan opnieuw tegen de nieuwe tekst). Opnieuw
/// uitschrijven, samenvoegen of een andere spreker verandert de opbouw wel.
#[must_use]
pub fn kenmerk(gesprek: &Gesprek) -> String {
    const BEGIN: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIEM: u64 = 0x0100_0000_01b3;
    let mut hash = BEGIN;
    let mut voeg_toe = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIEM);
        }
    };
    voeg_toe(&(gesprek.beurten.len() as u64).to_le_bytes());
    for beurt in &gesprek.beurten {
        voeg_toe(&beurt.start_ms.to_le_bytes());
        voeg_toe(&beurt.eind_ms.to_le_bytes());
        voeg_toe(&[
            u8::from(beurt.kant == crate::gesprek::Kant::Anderen),
            beurt.sprekernummer.unwrap_or(0),
        ]);
    }
    format!("fnv2:{hash:016x}")
}

/// Tekst om te vergelijken: kleine letters, leestekens worden spatie, witruimte één spatie
/// (ADR 0026 punt 3).
#[must_use]
pub fn normaliseer(tekst: &str) -> String {
    let mut uit = String::with_capacity(tekst.len());
    for teken in tekst.chars() {
        if teken.is_alphanumeric() {
            uit.extend(teken.to_lowercase());
        } else {
            uit.push(' ');
        }
    }
    op_een_regel(&uit)
}

/// Controleert een bron (ADR 0026 punt 3): bestaat de beurt, en staat het citaat (zonder verschil
/// in hoofdletters, witruimte en leestekens, als hele woorden) in die beurt of de beurt ervoor of
/// erna? `teksten` zijn de teksten zoals het model ze kreeg, beurt 1 eerst.
#[must_use]
pub fn controleer(bron: &Bron, teksten: &[String]) -> Controle {
    let citaat = normaliseer(&bron.citaat);
    let Some(genoemd) = bestaande_beurt(bron.beurt, teksten.len()) else {
        return Controle::NietTeruggevonden;
    };
    if citaat.split(' ').count() < MIN_CITAATWOORDEN || citaat.is_empty() {
        return Controle::NietTeruggevonden;
    }
    let gezocht = format!(" {citaat} ");
    for beurt in rond(genoemd, teksten.len()) {
        if format!(" {} ", normaliseer(&teksten[beurt - 1])).contains(&gezocht) {
            return Controle::Gevonden { beurt };
        }
    }
    Controle::NietTeruggevonden
}

/// Zoveel woorden (na normaliseren) moet een citaat minstens hebben; een korter citaat ("ja,
/// akkoord") staat in veel beurten en zegt dus niets.
pub const MIN_CITAATWOORDEN: usize = 4;

/// Het nummer als beurt die bestaat (vanaf 1), of `None`.
fn bestaande_beurt(beurt: i64, aantal: usize) -> Option<usize> {
    usize::try_from(beurt)
        .ok()
        .filter(|b| (1..=aantal).contains(b))
}

/// De beurt zelf, dan die ervoor, dan die erna, voor zover ze bestaan.
fn rond(beurt: usize, aantal: usize) -> impl Iterator<Item = usize> {
    [Some(beurt), beurt.checked_sub(1), Some(beurt + 1)]
        .into_iter()
        .flatten()
        .filter(move |b| (1..=aantal).contains(b))
}

/// Staat `gezocht` (genormaliseerd, als hele woorden) in `tekst`?
fn bevat(tekst: &str, gezocht: &str) -> bool {
    let gezocht = normaliseer(gezocht);
    !gezocht.is_empty() && format!(" {} ", normaliseer(tekst)).contains(&format!(" {gezocht} "))
}

/// Is `wie` een aanduiding als "ik", "Jij" of "de spreker" in plaats van een naam of rol?
fn is_voornaamwoord(wie: &str) -> bool {
    const WOORDEN: [&str; 13] = [
        "ik",
        "jij",
        "je",
        "u",
        "wij",
        "we",
        "de spreker",
        "i",
        "you",
        "me",
        "myself",
        "the speaker",
        "speaker",
    ];
    let wie = normaliseer(wie);
    WOORDEN.contains(&wie.as_str())
}

/// Controleert `wie` en `wanneer` van een actiepunt (true = niet teruggevonden). `wie` klopt als
/// het in de bronbeurt of de beurt ervoor of erna staat, of als het de spreker van de bronbeurt is
/// (wie zegt "ik pak het op", is de eigenaar). `wanneer` klopt als het daar staat. Een leeg veld
/// is altijd goed. De bronbeurt is waar het citaat gevonden is, anders wat het model noemde.
fn controleer_velden(punt: &Actiepunt, controle: Controle, invoer: &Invoer) -> (bool, bool) {
    let aantal = invoer.teksten.len();
    let bronbeurt = match controle {
        Controle::Gevonden { beurt } => Some(beurt),
        Controle::NietTeruggevonden => bestaande_beurt(punt.bron.beurt, aantal),
    };
    let staat_rond = |veld: &str| {
        bronbeurt.is_some_and(|b| rond(b, aantal).any(|r| bevat(&invoer.teksten[r - 1], veld)))
    };
    let wie = punt.wie.trim();
    // Bij een regel zonder spreker is "ik" of "Jij" geen eigenaar: er is niemand die zo heet. Dat het
    // woord in de tekst staat ("ik pak het op") maakt het geen naam.
    let zonder_label = bronbeurt.is_none_or(|b| invoer.sprekers[b - 1].is_empty());
    let alleen_voornaamwoord = zonder_label && is_voornaamwoord(wie);
    let wie_ok = wie.is_empty()
        || (!alleen_voornaamwoord
            && (staat_rond(wie)
                || bronbeurt.is_some_and(|b| {
                    let spreker = &invoer.sprekers[b - 1];
                    bevat(spreker, wie) || bevat(wie, spreker)
                })));
    let wanneer = punt.wanneer.trim();
    let wanneer_ok = wanneer.is_empty() || staat_rond(wanneer);
    (!wie_ok, !wanneer_ok)
}

/// De gegevens die bij bewaarde notulen horen, naast de inhoud.
pub struct Herkomst {
    /// De vaste code van de aanbieder.
    pub aanbieder: String,
    /// De modelnaam.
    pub model: String,
    /// Wanneer, in milliseconden sinds 1970.
    pub moment_ms: u64,
    /// De taal.
    pub taal: Taal,
}

/// Het vaste nummer van het punt op plek `i` (vanaf 1).
fn nummer_van(i: usize) -> u32 {
    u32::try_from(i + 1).unwrap_or(u32::MAX)
}

/// Controleert elk punt en maakt de notulen om te bewaren. Mondig verandert de tekst van een punt
/// nooit; een punt zonder teruggevonden bron blijft staan.
#[must_use]
pub fn controleer_notulen(
    notulen: Notulen,
    invoer: &Invoer,
    gesprek: &Gesprek,
    herkomst: Herkomst,
) -> BewaardeNotulen {
    let teksten = &invoer.teksten;
    BewaardeNotulen {
        samenvatting: notulen.samenvatting,
        besluiten: notulen
            .besluiten
            .into_iter()
            .enumerate()
            .map(|(i, punt)| Gecontroleerd {
                id: nummer_van(i),
                controle: controleer(&punt.bron, teksten),
                punt,
                wie_niet_teruggevonden: false,
                wanneer_niet_teruggevonden: false,
            })
            .collect(),
        actiepunten: notulen
            .actiepunten
            .into_iter()
            .enumerate()
            .map(|(i, punt)| {
                let controle = controleer(&punt.bron, teksten);
                let (wie, wanneer) = controleer_velden(&punt, controle, invoer);
                Gecontroleerd {
                    id: nummer_van(i),
                    controle,
                    punt,
                    wie_niet_teruggevonden: wie,
                    wanneer_niet_teruggevonden: wanneer,
                }
            })
            .collect(),
        aanbieder: herkomst.aanbieder,
        model: herkomst.model,
        moment_ms: herkomst.moment_ms,
        gesprek_kenmerk: kenmerk(gesprek),
        taal: herkomst.taal,
    }
}
