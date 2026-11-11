//! Notulen maken met een taalmodel (ADR 0026), via de koppeling van ADR 0024.
//!
//! Past het verslag in het budget van het model, dan is het één aanvraag. Anders delen op
//! beurtgrenzen met overlap ([`super::delen`]) en notulen per deel. De punten voegt Mondig zelf
//! samen ([`super::voeg_punten_samen`]); alleen de samenvatting maakt het model uit de
//! samenvattingen van de delen. De controle van de bronnen gebeurt daarna, over het hele verslag.
//!
//! Is een antwoord geen geldig JSON volgens het schema, dan vraagt Mondig het één keer opnieuw met
//! de fout erbij; daarna volgt [`NotulenFout::OngeldigAntwoord`].
//!
//! Er komt geen tekst in fouten of in [`Statistiek`]: alleen aantallen en tijden.

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;

/// Het antwoord op het samenvoegen van de samenvattingen.
#[derive(Deserialize)]
struct Samenvatting {
    samenvatting: String,
}

use super::{
    Herkomst, Invoer, Notulen, Taal, controleer_notulen, delen, json_schema, opdracht,
    samenvatting_schema, samenvoegopdracht, schat_tokens, voeg_punten_samen,
};
use crate::gesprek::Gesprek;
use crate::persoonsgegevens::Markeringen;
use crate::taak::{Foutmelding, Vervolg};
use crate::taalmodel::{Aanvraag, Antwoord, JsonSchema, TaalmodelFout, Verbinding, Voorinstelling};

use super::BewaardeNotulen;

/// Het standaardbudget voor de tekst van het verslag bij Mistral, in geschatte tokens. Mistral Small
/// 4 heeft een context van 256k (docs.mistral.ai, 5-11-2026). Opdelen gaf in de meting van T2 de
/// meeste extra actiepunten (een deel mist de context van de rest), dus zo groot dat een vergadering
/// van een paar uur in één keer gaat, en zo klein dat het antwoord ruim past.
pub const BUDGET_MISTRAL: usize = 60_000;

/// Het standaardbudget bij Ollama. Ollama gebruikt via de OpenAI-compatibele route zijn eigen
/// standaardcontext, die op een computer zonder grote videokaart klein kan zijn (enkele duizenden
/// tokens); wat niet past, laat Ollama stil weg. Daarom voorzichtig.
pub const BUDGET_OLLAMA: usize = 2_000;

/// Hoeveel beurten twee delen standaard overlappen.
pub const OVERLAP_BEURTEN: usize = 3;

/// Zoveel tokens mag een antwoord hooguit hebben.
pub const MAX_UITVOER_TOKENS: u32 = 4_000;

/// Hoe de notulen gemaakt worden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instellingen {
    /// De modelnaam bij de aanbieder.
    pub model: String,
    /// De taal van de notulen.
    pub taal: Taal,
    /// Het budget voor de tekst van het verslag per aanvraag, in geschatte tokens.
    pub budget_tokens: usize,
    /// Hoeveel beurten twee delen overlappen.
    pub overlap_beurten: usize,
    /// Hoeveel tokens een antwoord hooguit mag hebben.
    pub max_uitvoer_tokens: u32,
}

impl Instellingen {
    /// De standaard voor een aanbieder en model.
    #[must_use]
    pub fn voor(voorinstelling: Voorinstelling, model: &str, taal: Taal) -> Self {
        Self {
            model: model.to_string(),
            taal,
            budget_tokens: match voorinstelling {
                Voorinstelling::Mistral => BUDGET_MISTRAL,
                Voorinstelling::OllamaLokaal => BUDGET_OLLAMA,
            },
            overlap_beurten: OVERLAP_BEURTEN,
            max_uitvoer_tokens: MAX_UITVOER_TOKENS,
        }
    }
}

/// Iets dat een taalmodel een vraag kan stellen: in de app een [`Verbinding`], in de tests een
/// nep-aanbieder.
pub trait Vrager {
    /// De aanbieder.
    fn voorinstelling(&self) -> Voorinstelling;
    /// Stelt één vraag.
    ///
    /// # Errors
    ///
    /// [`TaalmodelFout`] in gewone taal.
    fn vraag(&self, aanvraag: &Aanvraag, annuleer: &AtomicBool) -> Result<Antwoord, TaalmodelFout>;
}

impl Vrager for Verbinding {
    fn voorinstelling(&self) -> Voorinstelling {
        Verbinding::voorinstelling(self)
    }

    fn vraag(&self, aanvraag: &Aanvraag, annuleer: &AtomicBool) -> Result<Antwoord, TaalmodelFout> {
        Verbinding::vraag(self, aanvraag, annuleer)
    }
}

/// Getallen over het maken, zonder tekst: voor `mondig-cli notulen` en het logboek.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Statistiek {
    /// Het aantal beurten in het verslag.
    pub beurten: usize,
    /// De geschatte tokens van het hele verslag.
    pub geschatte_tokens: usize,
    /// In hoeveel delen het verslag ging.
    pub delen: usize,
    /// Hoeveel aanvragen er gedaan zijn, inclusief herhalingen en samenvoegen.
    pub aanvragen: usize,
    /// Hoeveel aanvragen opnieuw moesten omdat het antwoord geen geldig JSON was.
    pub herhaald: usize,
    /// Tokens in de vragen, zoals de aanbieder ze meldde.
    pub invoer_tokens: u64,
    /// Tokens in de antwoorden, zoals de aanbieder ze meldde.
    pub uitvoer_tokens: u64,
    /// Hoe lang het duurde.
    pub duur: Duration,
}

/// Wat [`maak`] oplevert.
pub struct Uitkomst {
    /// De gecontroleerde notulen, klaar om te bewaren.
    pub notulen: BewaardeNotulen,
    /// De getallen.
    pub statistiek: Statistiek,
}

/// Wat er mis kan gaan bij het maken van notulen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NotulenFout {
    /// Een fout van het taalmodel of de verbinding.
    #[error(transparent)]
    Taalmodel(#[from] TaalmodelFout),
    /// Het verslag heeft geen tekst.
    #[error("Dit verslag heeft geen tekst, dus er zijn geen notulen van te maken.")]
    GeenTekst,
    /// Het model gaf twee keer geen bruikbare notulen.
    #[error(
        "{0} gaf twee keer geen bruikbare notulen. Probeer het opnieuw of kies een ander model."
    )]
    OngeldigAntwoord(&'static str),
}

impl NotulenFout {
    /// Een vaste code voor het logboek.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Taalmodel(fout) => fout.code(),
            Self::GeenTekst => "geen_tekst",
            Self::OngeldigAntwoord(_) => "ongeldig_antwoord",
        }
    }
}

impl From<NotulenFout> for Foutmelding {
    fn from(fout: NotulenFout) -> Self {
        match fout {
            NotulenFout::Taalmodel(fout) => fout.into(),
            NotulenFout::GeenTekst => Self::nieuw(fout.to_string(), Vervolg::Geen),
            NotulenFout::OngeldigAntwoord(_) => {
                Self::nieuw(fout.to_string(), Vervolg::OpnieuwProberen)
            }
        }
    }
}

/// Maakt notulen van een gesprek. `voortgang` krijgt eerst (0, totaal) en daarna na elke aanvraag (klaar, totaal); het totaal
/// is het aantal delen plus één samenvoegaanvraag als er meer delen zijn.
///
/// # Errors
///
/// [`NotulenFout`] in gewone taal; [`TaalmodelFout::Geannuleerd`] als `annuleer` aan gaat.
pub fn maak(
    vrager: &dyn Vrager,
    gesprek: &Gesprek,
    markeringen: &Markeringen,
    instellingen: &Instellingen,
    annuleer: &AtomicBool,
    voortgang: &mut dyn FnMut(usize, usize),
) -> Result<Uitkomst, NotulenFout> {
    let begin = Instant::now();
    let invoer = Invoer::van(gesprek, markeringen);
    if !invoer.heeft_tekst() {
        return Err(NotulenFout::GeenTekst);
    }
    let schattingen = invoer.schattingen();
    let stukken = delen(
        &schattingen,
        instellingen.budget_tokens,
        instellingen.overlap_beurten,
    );
    let mut statistiek = Statistiek {
        beurten: gesprek.beurten.len(),
        geschatte_tokens: schattingen.iter().sum(),
        delen: stukken.len(),
        ..Statistiek::default()
    };
    let totaal = stukken.len() + usize::from(stukken.len() > 1);
    let naam = vrager.voorinstelling().naam();
    let taal = instellingen.taal;

    // Het totaal is nu al bekend: de eerste melding (0, totaal) laat de interface meteen "deel 1"
    // tonen.
    voortgang(0, totaal);
    let mut per_deel: Vec<Notulen> = Vec::with_capacity(stukken.len());
    for (nummer, stuk) in stukken.iter().enumerate() {
        let kop = if stukken.len() == 1 {
            match taal {
                Taal::Nl => "Verslag:".to_string(),
                Taal::En => "Transcript:".to_string(),
            }
        } else {
            match taal {
                Taal::Nl => format!(
                    "Verslag, deel {} van {} (beurt {} tot en met {}):",
                    nummer + 1,
                    stukken.len(),
                    stuk.start + 1,
                    stuk.end
                ),
                Taal::En => format!(
                    "Transcript, part {} of {} (turns {} to {}):",
                    nummer + 1,
                    stukken.len(),
                    stuk.start + 1,
                    stuk.end
                ),
            }
        };
        let gebruiker = format!("{kop}\n\n{}", invoer.regels[stuk.clone()].join("\n"));
        let aanvraag = aanvraag(
            instellingen,
            &opdracht(taal, invoer.labels),
            gebruiker,
            "notulen",
            json_schema(),
        );
        per_deel.push(vraag_json(
            vrager,
            &aanvraag,
            taal,
            naam,
            annuleer,
            &mut statistiek,
        )?);
        voortgang(nummer + 1, totaal);
    }

    let notulen = if per_deel.len() == 1 {
        per_deel.pop().unwrap_or_else(|| unreachable!("één deel"))
    } else {
        let samen = voeg_delen_samen(
            vrager,
            per_deel,
            instellingen,
            naam,
            annuleer,
            &mut statistiek,
        )?;
        voortgang(totaal, totaal);
        samen
    };

    let herkomst = Herkomst {
        aanbieder: vrager.voorinstelling().code().to_string(),
        model: instellingen.model.clone(),
        moment_ms: nu_ms(),
        taal,
    };
    statistiek.duur = begin.elapsed();
    Ok(Uitkomst {
        notulen: controleer_notulen(notulen, &invoer, gesprek, herkomst),
        statistiek,
    })
}

/// Voegt de notulen van de delen samen: de punten in Rust ([`voeg_punten_samen`]), de samenvatting
/// via het model uit de samenvattingen van de delen, zoveel als in het budget past.
fn voeg_delen_samen(
    vrager: &dyn Vrager,
    per_deel: Vec<Notulen>,
    instellingen: &Instellingen,
    naam: &'static str,
    annuleer: &AtomicBool,
    statistiek: &mut Statistiek,
) -> Result<Notulen, NotulenFout> {
    let taal = instellingen.taal;
    // De punten voegt Mondig zelf samen, zodat het model er geen kan laten vallen; alleen de
    // samenvatting komt van het model, uit de samenvattingen van de delen (binnen het budget).
    let mut samenvattingen = Vec::new();
    let mut gebruikt = 0;
    for (nummer, deel) in per_deel.iter().enumerate() {
        let tokens = schat_tokens(&deel.samenvatting);
        if gebruikt + tokens > instellingen.budget_tokens {
            break;
        }
        gebruikt += tokens;
        samenvattingen.push(json!({ "deel": nummer + 1, "samenvatting": deel.samenvatting }));
    }
    let gebruiker = json!({ "delen": samenvattingen }).to_string();
    let vraag = aanvraag(
        instellingen,
        samenvoegopdracht(taal),
        gebruiker,
        "samenvatting",
        samenvatting_schema(),
    );
    let samenvatting: Samenvatting = vraag_json(vrager, &vraag, taal, naam, annuleer, statistiek)?;
    let mut samen = voeg_punten_samen(per_deel);
    samen.samenvatting = samenvatting.samenvatting;
    Ok(samen)
}

/// Milliseconden sinds 1970.
fn nu_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Een aanvraag met het schema, voorspelbaar (temperatuur 0) en zonder nadenken vooraf.
fn aanvraag(
    instellingen: &Instellingen,
    systeem: &str,
    gebruiker: String,
    schemanaam: &str,
    schema: serde_json::Value,
) -> Aanvraag {
    Aanvraag {
        model: instellingen.model.clone(),
        systeem: systeem.to_string(),
        gebruiker,
        temperatuur: 0.0,
        schema: Some(JsonSchema {
            naam: schemanaam.into(),
            schema,
        }),
        max_tokens: Some(instellingen.max_uitvoer_tokens),
        redeneren: false,
    }
}

/// Vraagt notulen; is het antwoord geen geldig JSON volgens het schema, dan één keer opnieuw met
/// de fout erbij.
fn vraag_json<T: DeserializeOwned>(
    vrager: &dyn Vrager,
    aanvraag: &Aanvraag,
    taal: Taal,
    naam: &'static str,
    annuleer: &AtomicBool,
    statistiek: &mut Statistiek,
) -> Result<T, NotulenFout> {
    let mut huidig = aanvraag.clone();
    for poging in 0..2 {
        let antwoord = vrager.vraag(&huidig, annuleer)?;
        statistiek.aanvragen += 1;
        if let Some(gebruik) = antwoord.gebruik {
            statistiek.invoer_tokens += u64::from(gebruik.invoer);
            statistiek.uitvoer_tokens += u64::from(gebruik.uitvoer);
        }
        match lees_json::<T>(&antwoord) {
            Ok(notulen) => return Ok(notulen),
            Err(fout) if poging == 0 => {
                statistiek.herhaald += 1;
                let melding = match taal {
                    Taal::Nl => format!(
                        "Je vorige antwoord was geen geldig JSON volgens het schema ({fout}). Geef alleen het JSON-object, zonder andere tekst."
                    ),
                    Taal::En => format!(
                        "Your previous answer was not valid JSON following the schema ({fout}). Give only the JSON object, without any other text."
                    ),
                };
                huidig.gebruiker = format!("{}\n\n{melding}", aanvraag.gebruiker);
            }
            Err(_) => {}
        }
    }
    Err(NotulenFout::OngeldigAntwoord(naam))
}

/// Leest notulen uit een antwoord. Een Markdown-codeblok eromheen mag; een afgekapt antwoord
/// niet. De fout is een korte beschrijving zonder tekst van het antwoord.
fn lees_json<T: DeserializeOwned>(antwoord: &Antwoord) -> Result<T, String> {
    if antwoord.afgekapt {
        return Err("het antwoord was te lang en is afgebroken; houd het korter".into());
    }
    let tekst = zonder_codeblok(antwoord.tekst.trim());
    serde_json::from_str::<T>(tekst).map_err(|fout| {
        // Alleen de soort fout en de plek, nooit tekst uit het antwoord.
        format!(
            "{:?} bij regel {}, kolom {}",
            fout.classify(),
            fout.line(),
            fout.column()
        )
    })
}

/// Haalt een Markdown-codeblok weg als het model dat om de JSON zette.
pub(crate) fn zonder_codeblok(tekst: &str) -> &str {
    let Some(binnen) = tekst.strip_prefix("```") else {
        return tekst;
    };
    let binnen = binnen.strip_prefix("json").unwrap_or(binnen);
    binnen.strip_suffix("```").unwrap_or(binnen).trim()
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::taalmodel::Tokengebruik;

    /// Een nep-aanbieder die vaste antwoorden geeft en de aanvragen bewaart.
    pub(crate) struct NepVrager {
        pub(crate) antwoorden: Mutex<Vec<Result<Antwoord, TaalmodelFout>>>,
        pub(crate) gezien: Mutex<Vec<Aanvraag>>,
    }

    impl NepVrager {
        pub(crate) fn met(teksten: &[&str]) -> Self {
            Self {
                antwoorden: Mutex::new(
                    teksten
                        .iter()
                        .rev()
                        .map(|t| {
                            Ok(Antwoord {
                                tekst: (*t).to_string(),
                                gebruik: Some(Tokengebruik {
                                    invoer: 100,
                                    uitvoer: 10,
                                }),
                                afgekapt: false,
                            })
                        })
                        .collect(),
                ),
                gezien: Mutex::new(Vec::new()),
            }
        }

        pub(crate) fn gezien(&self) -> Vec<Aanvraag> {
            self.gezien.lock().unwrap().clone()
        }
    }

    impl Vrager for NepVrager {
        fn voorinstelling(&self) -> Voorinstelling {
            Voorinstelling::Mistral
        }

        fn vraag(
            &self,
            aanvraag: &Aanvraag,
            _annuleer: &AtomicBool,
        ) -> Result<Antwoord, TaalmodelFout> {
            self.gezien.lock().unwrap().push(aanvraag.clone());
            self.antwoorden
                .lock()
                .unwrap()
                .pop()
                .unwrap_or(Err(TaalmodelFout::Onverwacht("Mistral")))
        }
    }

    #[test]
    fn een_codeblok_om_de_json_mag() {
        assert_eq!(zonder_codeblok("```json\n{}\n```"), "{}");
        assert_eq!(zonder_codeblok("```\n{}\n```"), "{}");
        assert_eq!(zonder_codeblok("{}"), "{}");
    }

    #[test]
    fn de_fout_bij_ongeldig_json_bevat_geen_tekst_van_het_antwoord() {
        let antwoord = Antwoord {
            tekst: "{\"samenvatting\": \"Geheim van Jansen\"".into(),
            gebruik: None,
            afgekapt: false,
        };
        let fout = lees_json::<Notulen>(&antwoord).err().unwrap();
        assert!(!fout.contains("Jansen"), "{fout}");
    }
}
