//! Taalmodellen als aanbieder (ADR 0024): één koppeling voor OpenAI-compatibele chat
//! completions, met twee vaste voorinstellingen.
//!
//! - [`Voorinstelling::Mistral`]: `https://api.mistral.ai/v1`, label Europees, eigen sleutel.
//! - [`Voorinstelling::OllamaLokaal`]: `http://localhost:11434/v1`, label Lokaal, geen sleutel.
//!
//! Er is geen vrij adres: het label hoort bij de voorinstelling, niet bij wat iemand typt. Het
//! netwerk gaat alleen naar die twee adressen; omleidingen worden niet gevolgd, zodat de
//! sleutel nooit bij een andere server terechtkomt.
//!
//! **Privacy.** Een [`Aanvraag`] en een [`Antwoord`] bevatten tekst van de gebruiker. Hun
//! `Debug` toont alleen lengtes en getallen. Een [`TaalmodelFout`] bevat nooit de sleutel, de
//! Authorization-header, de prompt of het antwoord, ook niet als de aanbieder die in zijn
//! foutmelding terugstuurt: de fout neemt alleen de statuscode over.
//!
//! De sleutel komt uit [`crate::sleutel::sleutel_voor`].

use std::fmt;
use std::io::ErrorKind;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::sleutel::{self, Sleutel, SleutelFout};
use crate::soevereiniteit::Soevereiniteit;
use crate::taak::{Foutmelding, Vervolg};

/// Een vaste aanbieder van taalmodellen. Het adres en het label horen erbij en zijn niet aan
/// te passen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Voorinstelling {
    /// Mistral AI in Europa, met een eigen sleutel van de gebruiker.
    Mistral,
    /// Ollama op deze computer, zonder sleutel.
    OllamaLokaal,
}

impl Voorinstelling {
    /// Alle voorinstellingen, in de volgorde van de interface.
    pub const ALLE: [Self; 2] = [Self::Mistral, Self::OllamaLokaal];

    /// Het basisadres van de OpenAI-compatibele API.
    #[must_use]
    pub fn basisadres(self) -> &'static str {
        match self {
            Self::Mistral => "https://api.mistral.ai/v1",
            Self::OllamaLokaal => "http://localhost:11434/v1",
        }
    }

    /// Waar de tekst wordt verwerkt.
    #[must_use]
    pub fn soevereiniteit(self) -> Soevereiniteit {
        match self {
            Self::Mistral => Soevereiniteit::Europees,
            Self::OllamaLokaal => Soevereiniteit::Lokaal,
        }
    }

    /// Naam in gewone taal.
    #[must_use]
    pub fn naam(self) -> &'static str {
        match self {
            Self::Mistral => "Mistral",
            Self::OllamaLokaal => "Ollama",
        }
    }

    /// Vaste code voor het logboek en de opdrachtregel.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Mistral => "mistral",
            Self::OllamaLokaal => "ollama",
        }
    }

    /// Heeft deze aanbieder een sleutel nodig?
    #[must_use]
    pub fn sleutel_nodig(self) -> bool {
        self.sleutelnaam().is_some()
    }

    /// De naam van de sleutel in de sleutelbos, of `None` zonder sleutel (ADR 0025).
    #[must_use]
    pub fn sleutelnaam(self) -> Option<&'static str> {
        match self {
            Self::Mistral => Some("taalmodel-mistral"),
            Self::OllamaLokaal => None,
        }
    }
}

/// Tijdslimieten van een aanvraag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opties {
    /// Hoe lang een verbinding maken mag duren.
    pub verbinden: Duration,
    /// Hoe lang de hele aanvraag, inclusief het antwoord, hooguit mag duren.
    pub totaal: Duration,
}

impl Default for Opties {
    /// Ruim genoeg voor een lang antwoord van een lokaal model op de processor.
    fn default() -> Self {
        Self {
            verbinden: Duration::from_secs(10),
            totaal: Duration::from_mins(5),
        }
    }
}

/// Het JSON-schema waaraan het antwoord moet voldoen (`response_format` met `json_schema`).
#[derive(Clone, PartialEq)]
pub struct JsonSchema {
    /// Korte naam van het schema, zoals de aanbieder die vraagt (letters, cijfers, `_`, `-`).
    pub naam: String,
    /// Het schema zelf.
    pub schema: Value,
}

/// Eén vraag aan een taalmodel. `Debug` toont alleen lengtes, nooit de tekst.
#[derive(Clone, PartialEq)]
pub struct Aanvraag {
    /// De modelnaam bij de aanbieder, bijvoorbeeld `mistral-small-latest`.
    pub model: String,
    /// De opdracht aan het model (systeembericht). Leeg betekent: geen systeembericht.
    pub systeem: String,
    /// De tekst van de gebruiker.
    pub gebruiker: String,
    /// Hoe vrij het model antwoordt (0 is het meest voorspelbaar).
    pub temperatuur: f32,
    /// Een schema voor het antwoord, of `None` voor vrije tekst.
    pub schema: Option<JsonSchema>,
    /// Hoeveel tokens het antwoord hooguit mag hebben, of `None` voor de grens van de aanbieder.
    pub max_tokens: Option<u32>,
    /// Mag een redeneermodel eerst nadenken voor het antwoordt? Bij Ollama zet `false` dat uit
    /// (`reasoning_effort: none`), anders gaan de tokens en de tijd op aan nadenken. Bij
    /// Mistral hoort nadenken bij het model; daar stuurt Mondig niets mee.
    pub redeneren: bool,
}

impl fmt::Debug for Aanvraag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Aanvraag")
            .field("model", &self.model)
            .field("systeem_tekens", &self.systeem.chars().count())
            .field("gebruiker_tekens", &self.gebruiker.chars().count())
            .field("temperatuur", &self.temperatuur)
            .field("schema", &self.schema.as_ref().map(|s| &s.naam))
            .field("max_tokens", &self.max_tokens)
            .field("redeneren", &self.redeneren)
            .finish()
    }
}

impl Aanvraag {
    /// De body van de aanvraag voor `/chat/completions` bij deze aanbieder.
    fn body(&self, voorinstelling: Voorinstelling) -> Value {
        let mut berichten = Vec::new();
        if !self.systeem.is_empty() {
            berichten.push(json!({ "role": "system", "content": self.systeem }));
        }
        berichten.push(json!({ "role": "user", "content": self.gebruiker }));
        let mut body = json!({
            "model": self.model,
            "messages": berichten,
            "temperature": self.temperatuur,
            "stream": false,
        });
        if let Some(max) = self.max_tokens {
            body["max_tokens"] = json!(max);
        }
        // Alleen bij Ollama: Mistral kent dit veld niet bij elk model.
        if !self.redeneren && voorinstelling == Voorinstelling::OllamaLokaal {
            body["reasoning_effort"] = json!("none");
        }
        if let Some(schema) = &self.schema {
            body["response_format"] = json!({
                "type": "json_schema",
                "json_schema": { "name": schema.naam, "schema": schema.schema, "strict": true },
            });
        }
        body
    }
}

/// Hoeveel tokens een aanvraag kostte, zoals de aanbieder het meldt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tokengebruik {
    /// Tokens in de vraag (systeem en gebruiker samen).
    pub invoer: u32,
    /// Tokens in het antwoord.
    pub uitvoer: u32,
}

/// Het antwoord van een taalmodel. `Debug` toont alleen de lengte, nooit de tekst.
#[derive(Clone, PartialEq)]
pub struct Antwoord {
    /// De tekst van het eerste bericht (bij een schema: de JSON als tekst).
    pub tekst: String,
    /// Het tokengebruik, als de aanbieder dat meldt.
    pub gebruik: Option<Tokengebruik>,
    /// Het model stopte omdat `max_tokens` op was; de tekst is dan onvolledig.
    pub afgekapt: bool,
}

impl fmt::Debug for Antwoord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Antwoord")
            .field("tekens", &self.tekst.chars().count())
            .field("gebruik", &self.gebruik)
            .field("afgekapt", &self.afgekapt)
            .finish()
    }
}

/// Wat er mis kan gaan bij een taalmodel. Elke melding is gewone taal en bevat alleen de naam
/// van de aanbieder en hooguit een statuscode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TaalmodelFout {
    /// Er is nog geen sleutel ingevuld.
    #[error("Er is nog geen sleutel voor {0} ingevuld. Vul hem in bij Instellingen.")]
    SleutelOntbreekt(&'static str),
    /// De sleutel of de sleutelbos gaf een fout.
    #[error(transparent)]
    Sleutel(#[from] SleutelFout),
    /// De aanbieder accepteert de sleutel niet (401 of 403).
    #[error("{0} accepteert de sleutel niet. Controleer de sleutel bij Instellingen.")]
    SleutelOngeldig(&'static str),
    /// Geen tegoed meer of te veel aanvragen (402 of 429).
    #[error("Het tegoed of de limiet bij {0} is bereikt. Probeer het later opnieuw.")]
    Limiet(&'static str),
    /// Het gevraagde model bestaat niet bij de aanbieder.
    #[error("Dit taalmodel bestaat niet bij {0}. Kies een ander model.")]
    ModelOntbreekt(&'static str),
    /// De aanbieder kent `GET /models` niet (404).
    #[error("{}", geen_modellenlijst(*.0))]
    GeenModellenlijst(Voorinstelling),
    /// Het certificaat van de aanbieder is niet te controleren, vaak door een eigen certificaat
    /// van de organisatie op een proxy.
    #[error(
        "De verbinding met {0} is niet te controleren. Werkt je organisatie met een eigen certificaat? Vraag het je ICT."
    )]
    Certificaat(&'static str),
    /// De proxy uit de omgeving werkt niet of laat de verbinding niet toe.
    #[error("De proxy van deze computer laat geen verbinding met {0} toe. Vraag het je ICT.")]
    Proxy(&'static str),
    /// Ollama draait niet op deze computer (verbinding geweigerd).
    #[error("Ollama draait niet op deze computer. Start Ollama en probeer het opnieuw.")]
    OllamaDraaitNiet,
    /// De aanbieder is niet te bereiken.
    #[error("{0} is niet bereikbaar. Controleer de internetverbinding en probeer het opnieuw.")]
    Onbereikbaar(&'static str),
    /// De aanbieder gaf niet op tijd antwoord.
    #[error("{0} gaf niet op tijd antwoord. Probeer het opnieuw.")]
    TijdOverschreden(&'static str),
    /// Het antwoord was niet te lezen.
    #[error("Het antwoord van {0} was niet te lezen. Probeer het opnieuw.")]
    AntwoordOnleesbaar(&'static str),
    /// De aanbieder wees de aanvraag af om een andere reden.
    #[error("{naam} wees de aanvraag af (foutcode {status}).")]
    Afgewezen {
        /// De aanbieder.
        naam: &'static str,
        /// De HTTP-statuscode.
        status: u16,
    },
    /// De aanbieder heeft een storing (5xx).
    #[error("{naam} heeft een storing (foutcode {status}). Probeer het later opnieuw.")]
    Storing {
        /// De aanbieder.
        naam: &'static str,
        /// De HTTP-statuscode.
        status: u16,
    },
    /// Een fout in de verbinding die in geen van de andere soorten past.
    #[error("Er ging iets onverwachts mis in de verbinding met {0}. Probeer het opnieuw.")]
    Onverwacht(&'static str),
    /// De gebruiker stopte de aanvraag.
    #[error("Gestopt.")]
    Geannuleerd,
}

/// De melding als de aanbieder geen lijst met modellen geeft.
fn geen_modellenlijst(voorinstelling: Voorinstelling) -> &'static str {
    match voorinstelling {
        Voorinstelling::Mistral => "Deze aanbieder kan de lijst met modellen niet geven.",
        Voorinstelling::OllamaLokaal => {
            "Ollama kan de lijst met modellen niet geven. Werk Ollama bij."
        }
    }
}

impl TaalmodelFout {
    /// Een vaste code voor het logboek (ADR 0024 punt 5: alleen de soort fout).
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::SleutelOntbreekt(_) => "sleutel_ontbreekt",
            Self::Sleutel(fout) => fout.code(),
            Self::SleutelOngeldig(_) => "sleutel_ongeldig",
            Self::Limiet(_) => "limiet",
            Self::ModelOntbreekt(_) => "model_ontbreekt",
            Self::GeenModellenlijst(_) => "geen_modellenlijst",
            Self::Certificaat(_) => "certificaat",
            Self::Proxy(_) => "proxy",
            Self::Onverwacht(_) => "onverwacht",
            Self::OllamaDraaitNiet => "ollama_draait_niet",
            Self::Onbereikbaar(_) => "onbereikbaar",
            Self::TijdOverschreden(_) => "tijd_overschreden",
            Self::AntwoordOnleesbaar(_) => "antwoord_onleesbaar",
            Self::Afgewezen { .. } => "afgewezen",
            Self::Storing { .. } => "storing",
            Self::Geannuleerd => "geannuleerd",
        }
    }

    /// Wat de gebruiker nu kan doen.
    #[must_use]
    pub fn vervolg(self) -> Vervolg {
        match self {
            Self::Sleutel(fout) => fout.foutmelding().vervolg,
            Self::ModelOntbreekt(_) => Vervolg::AnderModelKiezen,
            Self::Limiet(_)
            | Self::OllamaDraaitNiet
            | Self::Onbereikbaar(_)
            | Self::TijdOverschreden(_)
            | Self::AntwoordOnleesbaar(_)
            | Self::Onverwacht(_)
            | Self::Storing { .. } => Vervolg::OpnieuwProberen,
            // De sleutel invullen of herstellen gebeurt in Instellingen; dat zegt de melding.
            Self::SleutelOntbreekt(_)
            | Self::SleutelOngeldig(_)
            | Self::Afgewezen { .. }
            | Self::GeenModellenlijst(_)
            | Self::Certificaat(_)
            | Self::Proxy(_)
            | Self::Geannuleerd => Vervolg::Geen,
        }
    }
}

impl From<TaalmodelFout> for Foutmelding {
    fn from(fout: TaalmodelFout) -> Self {
        Self::nieuw(fout.to_string(), fout.vervolg())
    }
}

/// Een verbinding met één aanbieder: het vaste adres, de sleutel, de tijdslimieten en één
/// `ureq::Agent` die verbindingen openhoudt (keep-alive), zodat opeenvolgende aanvragen niet elk
/// een nieuwe TLS-verbinding opzetten. Klonen delen die agent. `Debug` toont de sleutel niet.
#[derive(Clone)]
pub struct Verbinding {
    voorinstelling: Voorinstelling,
    basisadres: String,
    sleutel: Option<Sleutel>,
    opties: Opties,
    /// Wordt bij annuleren afgesloten en vervangen door een nieuwe (zie [`Verbinding::stuur`]).
    agent: Arc<Mutex<ureq::Agent>>,
}

impl fmt::Debug for Verbinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Verbinding")
            .field("voorinstelling", &self.voorinstelling)
            .field("basisadres", &self.basisadres)
            .field("sleutel", &self.sleutel.as_ref().map(|_| "verborgen"))
            .field("opties", &self.opties)
            .finish_non_exhaustive()
    }
}

impl Verbinding {
    /// Een verbinding met deze voorinstelling, met de sleutel uit [`sleutel::sleutel_voor`].
    ///
    /// # Errors
    ///
    /// [`TaalmodelFout::SleutelOntbreekt`] als de aanbieder een sleutel nodig heeft en er geen
    /// is, of [`TaalmodelFout::Sleutelbos`] als de sleutelbos faalt.
    pub fn voor(voorinstelling: Voorinstelling) -> Result<Self, TaalmodelFout> {
        let sleutel = if voorinstelling.sleutel_nodig() {
            Some(
                sleutel::sleutel_voor(voorinstelling)?
                    .ok_or(TaalmodelFout::SleutelOntbreekt(voorinstelling.naam()))?,
            )
        } else {
            None
        };
        Ok(Self::nieuw(
            voorinstelling,
            voorinstelling.basisadres(),
            sleutel,
        ))
    }

    fn nieuw(voorinstelling: Voorinstelling, adres: &str, sleutel: Option<Sleutel>) -> Self {
        let opties = Opties::default();
        let agent = bouw_agent(adres, opties);
        Self {
            voorinstelling,
            basisadres: adres.to_string(),
            sleutel,
            opties,
            agent: Arc::new(Mutex::new(agent)),
        }
    }

    /// Een verbinding met een ander adres, alleen voor de nep-server in de tests (ook die van
    /// [`crate::notulen`]).
    #[cfg(test)]
    pub(crate) fn op_adres(
        voorinstelling: Voorinstelling,
        adres: &str,
        sleutel: Option<Sleutel>,
    ) -> Self {
        Self::nieuw(voorinstelling, adres, sleutel)
    }

    /// Dezelfde verbinding met andere tijdslimieten (en dus een eigen, nieuwe agent).
    #[must_use]
    pub fn met_opties(mut self, opties: Opties) -> Self {
        self.opties = opties;
        self.agent = Arc::new(Mutex::new(bouw_agent(&self.basisadres, opties)));
        self
    }

    /// De voorinstelling van deze verbinding.
    #[must_use]
    pub fn voorinstelling(&self) -> Voorinstelling {
        self.voorinstelling
    }

    /// De namen van de modellen die de aanbieder heeft (`GET /models`), gesorteerd.
    ///
    /// # Errors
    ///
    /// [`TaalmodelFout`] in gewone taal.
    pub fn modellen(&self, annuleer: &AtomicBool) -> Result<Vec<String>, TaalmodelFout> {
        #[derive(Deserialize)]
        struct Lijst {
            data: Vec<Item>,
        }
        #[derive(Deserialize)]
        struct Item {
            id: String,
        }
        let tekst = self
            .stuur("models", None, annuleer)
            .map_err(|fout| match fout {
                // Bij de lijst gaat een 404 niet over een model maar over de aanbieder zelf.
                TaalmodelFout::ModelOntbreekt(_) => {
                    TaalmodelFout::GeenModellenlijst(self.voorinstelling)
                }
                fout => fout,
            })?;
        let lijst: Lijst = serde_json::from_str(&tekst)
            .map_err(|_| TaalmodelFout::AntwoordOnleesbaar(self.voorinstelling.naam()))?;
        let mut namen: Vec<String> = lijst.data.into_iter().map(|i| i.id).collect();
        namen.sort();
        namen.dedup();
        Ok(namen)
    }

    /// Stelt één vraag en wacht op het antwoord. Stopt met [`TaalmodelFout::Geannuleerd`] zodra
    /// `annuleer` aan gaat, en met [`TaalmodelFout::TijdOverschreden`] na [`Opties::totaal`].
    /// Annuleren breekt ook de andere lopende aanvragen van deze verbinding (en haar klonen) af.
    ///
    /// # Errors
    ///
    /// [`TaalmodelFout`] in gewone taal.
    pub fn vraag(
        &self,
        aanvraag: &Aanvraag,
        annuleer: &AtomicBool,
    ) -> Result<Antwoord, TaalmodelFout> {
        let body = aanvraag.body(self.voorinstelling).to_string().into_bytes();
        let tekst = self.stuur("chat/completions", Some(body), annuleer)?;
        lees_antwoord(&tekst).ok_or(TaalmodelFout::AntwoordOnleesbaar(
            self.voorinstelling.naam(),
        ))
    }

    /// De gedeelde agent.
    fn agent(&self) -> ureq::Agent {
        self.agent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Sluit alle verbindingen van de agent, ook die van aanvragen die nog lopen, en zet een
    /// nieuwe agent klaar voor de volgende aanvraag.
    ///
    /// `Agent::shutdown` (ureq 3.5, "Add `Agent::shutdown()` to interrupt requests in flight", PR
    /// #1211; docs.rs/ureq/3.5.0, `Agent::shutdown`) sluit de sockets, zodat een aanvraag die op
    /// het antwoord wacht meteen een fout krijgt. Een DNS-opzoeking of TCP-verbinding die nog
    /// loopt, stopt niet; die is begrensd door [`Opties::verbinden`]. Een afgesloten agent weigert
    /// elke nieuwe aanvraag, daarom komt er een nieuwe.
    fn sluit_af(&self) {
        let nieuw = bouw_agent(&self.basisadres, self.opties);
        let oud = std::mem::replace(
            &mut *self.agent.lock().unwrap_or_else(PoisonError::into_inner),
            nieuw,
        );
        oud.shutdown();
    }

    /// Stuurt een aanvraag in een eigen draad, zodat annuleren meteen werkt: bij annuleren sluit
    /// [`Verbinding::sluit_af`] de verbinding, waardoor ook de draad direct stopt. De
    /// Authorization-header leeft alleen in die draad en wordt daarna gewist. Geeft de body bij
    /// een 2xx-antwoord.
    fn stuur(
        &self,
        pad: &str,
        body: Option<Vec<u8>>,
        annuleer: &AtomicBool,
    ) -> Result<String, TaalmodelFout> {
        let naam = self.voorinstelling.naam();
        if annuleer.load(Ordering::Relaxed) {
            return Err(TaalmodelFout::Geannuleerd);
        }
        let agent = self.agent();
        let adres = format!("{}/{pad}", self.basisadres);
        let autorisatie = self
            .sleutel
            .as_ref()
            .map(|s| Zeroizing::new(format!("Bearer {}", s.waarde())));
        let (zender, ontvanger) = mpsc::channel();
        thread::spawn(move || {
            let uitkomst = voer_uit(
                &agent,
                &adres,
                autorisatie.as_ref().map(|a| a.as_str()),
                body,
            );
            drop(autorisatie);
            let _ = zender.send(uitkomst);
        });

        // Een vangnet boven de tijdslimiet van de agent, voor het geval de draad niet stopt.
        let uiterlijk = Instant::now() + self.opties.totaal + Duration::from_secs(5);
        let uitkomst = loop {
            if annuleer.load(Ordering::Relaxed) {
                self.sluit_af();
                return Err(TaalmodelFout::Geannuleerd);
            }
            match ontvanger.recv_timeout(Duration::from_millis(50)) {
                Ok(uitkomst) => break uitkomst,
                Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() < uiterlijk => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    self.sluit_af();
                    return Err(TaalmodelFout::TijdOverschreden(naam));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(TaalmodelFout::Onverwacht(naam));
                }
            }
        };
        match uitkomst {
            Ok((status, tekst)) if (200..300).contains(&status) => Ok(tekst),
            Ok((status, tekst)) => Err(fout_uit_status(naam, status, &tekst)),
            Err(fout) => Err(fout_uit_verbinding(self.voorinstelling, &fout)),
        }
    }
}

/// Is het adres op deze computer?
fn is_lokaal(adres: &str) -> bool {
    let zonder_schema = adres
        .strip_prefix("http://")
        .or_else(|| adres.strip_prefix("https://"))
        .unwrap_or(adres);
    zonder_schema.starts_with("localhost:")
        || zonder_schema.starts_with("127.0.0.1:")
        || zonder_schema.starts_with("[::1]:")
}

/// Bouwt de agent voor een adres.
fn bouw_agent(adres: &str, opties: Opties) -> ureq::Agent {
    // Proxy (besluit, 5-11-2026, ADR 0024): naar Mistral mag de proxy uit de
    // omgeving, want gemeenten en andere organisaties werken vaak achter een eigen proxy. ureq
    // 3.5 leest daarvoor alleen ALL_PROXY, HTTPS_PROXY en HTTP_PROXY, met NO_PROXY
    // (`Proxy::try_from_env`); de proxy-instelling van Windows zelf (Internetopties, PAC) kent
    // het niet. Naar localhost nooit via een proxy: de tekst moet op deze computer blijven.
    let proxy = if is_lokaal(adres) {
        None
    } else {
        ureq::Proxy::try_from_env()
    };
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .user_agent("mondig")
        // Nooit een omleiding volgen: de sleutel gaat alleen naar het vaste adres.
        .max_redirects(0)
        .proxy(proxy)
        .timeout_connect(Some(opties.verbinden))
        .timeout_global(Some(opties.totaal))
        // Certificaten controleren met de opslag van het besturingssysteem (op Windows de
        // certificaatopslag van Windows), zodat een eigen basiscertificaat van de organisatie op
        // een proxy met TLS-inspectie werkt (ureq 3.5, `RootCerts::PlatformVerifier`).
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into()
}

/// De eigenlijke HTTP-aanvraag: statuscode en body. De foutsoort komt terug, nooit als tekst.
fn voer_uit(
    agent: &ureq::Agent,
    adres: &str,
    autorisatie: Option<&str>,
    body: Option<Vec<u8>>,
) -> Result<(u16, String), ureq::Error> {
    let mut antwoord = if let Some(body) = body {
        let mut verzoek = agent
            .post(adres)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");
        if let Some(waarde) = autorisatie {
            verzoek = verzoek.header("Authorization", waarde);
        }
        verzoek.send(&body[..])?
    } else {
        let mut verzoek = agent.get(adres).header("Accept", "application/json");
        if let Some(waarde) = autorisatie {
            verzoek = verzoek.header("Authorization", waarde);
        }
        verzoek.call()?
    };
    let status = antwoord.status().as_u16();
    let tekst = antwoord.body_mut().read_to_string()?;
    Ok((status, tekst))
}

/// Vertaalt een foutcode van de aanbieder. De body wordt alleen bekeken om "model bestaat
/// niet" te herkennen; er komt niets van in de fout.
fn fout_uit_status(naam: &'static str, status: u16, body: &str) -> TaalmodelFout {
    match status {
        401 | 403 => TaalmodelFout::SleutelOngeldig(naam),
        402 | 429 => TaalmodelFout::Limiet(naam),
        404 => TaalmodelFout::ModelOntbreekt(naam),
        400 | 422 if gaat_over_onbekend_model(body) => TaalmodelFout::ModelOntbreekt(naam),
        500..=599 => TaalmodelFout::Storing { naam, status },
        _ => TaalmodelFout::Afgewezen { naam, status },
    }
}

/// Zegt de foutmelding van de aanbieder dat het model niet bestaat? Mistral meldt dat met een
/// 400 en het type `invalid_model`; anderen met een tekst over het model.
fn gaat_over_onbekend_model(body: &str) -> bool {
    let Ok(waarde) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    // Zowel `{ "type": .. }` als `{ "error": { "type": .. } }`.
    let fout = waarde
        .get("error")
        .filter(|e| e.is_object())
        .unwrap_or(&waarde);
    let tekst = |veld: &str| {
        fout.get(veld)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase()
    };
    let (soort, code, melding) = (tekst("type"), tekst("code"), tekst("message"));
    soort == "invalid_model"
        || code == "invalid_model"
        || code == "model_not_found"
        || (melding.contains("model")
            && [
                "not found",
                "invalid model",
                "does not exist",
                "unknown model",
            ]
            .iter()
            .any(|t| melding.contains(t)))
}

/// Vertaalt een verbindingsfout naar een soort, zonder de tekst van de fout over te nemen.
/// "Onbereikbaar" alleen bij echte verbindings- en DNS-fouten; verbinding geweigerd op deze
/// computer betekent bij Ollama dat Ollama niet draait.
fn fout_uit_verbinding(voorinstelling: Voorinstelling, fout: &ureq::Error) -> TaalmodelFout {
    let naam = voorinstelling.naam();
    match fout {
        ureq::Error::Timeout(_) => TaalmodelFout::TijdOverschreden(naam),
        ureq::Error::Io(io) => fout_uit_io(voorinstelling, io),
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => {
            TaalmodelFout::Onbereikbaar(naam)
        }
        ureq::Error::Tls(_)
        | ureq::Error::Pem(_)
        | ureq::Error::Rustls(_)
        | ureq::Error::TlsRequired => TaalmodelFout::Certificaat(naam),
        ureq::Error::InvalidProxyUrl | ureq::Error::ConnectProxyFailed(_) => {
            TaalmodelFout::Proxy(naam)
        }
        // Een header die niet te versturen is: alleen de Authorization-header is niet vast, dus
        // het ligt aan een teken in de sleutel.
        ureq::Error::Http(_) => TaalmodelFout::Sleutel(SleutelFout::OnzichtbaarTeken),
        ureq::Error::Protocol(_)
        | ureq::Error::BodyExceedsLimit(_)
        | ureq::Error::LargeResponseHeader(..) => TaalmodelFout::AntwoordOnleesbaar(naam),
        _ => TaalmodelFout::Onverwacht(naam),
    }
}

/// Vertaalt een io-fout. Een fout in de TLS-handdruk komt van ureq als io-fout van de soort
/// `InvalidData` met een `rustls::Error` erin (`complete_io` in ureq 3.5, `tls/rustls.rs`).
fn fout_uit_io(voorinstelling: Voorinstelling, io: &std::io::Error) -> TaalmodelFout {
    let naam = voorinstelling.naam();
    let is_tls = io
        .get_ref()
        .is_some_and(|binnen| binnen.downcast_ref::<rustls::Error>().is_some());
    match io.kind() {
        _ if is_tls => TaalmodelFout::Certificaat(naam),
        ErrorKind::TimedOut => TaalmodelFout::TijdOverschreden(naam),
        ErrorKind::ConnectionRefused if voorinstelling == Voorinstelling::OllamaLokaal => {
            TaalmodelFout::OllamaDraaitNiet
        }
        ErrorKind::ConnectionRefused
        | ErrorKind::ConnectionReset
        | ErrorKind::ConnectionAborted
        | ErrorKind::NotConnected
        | ErrorKind::HostUnreachable
        | ErrorKind::NetworkUnreachable
        | ErrorKind::NetworkDown
        | ErrorKind::AddrNotAvailable
        | ErrorKind::BrokenPipe
        | ErrorKind::UnexpectedEof => TaalmodelFout::Onbereikbaar(naam),
        ErrorKind::InvalidData => TaalmodelFout::AntwoordOnleesbaar(naam),
        _ => TaalmodelFout::Onverwacht(naam),
    }
}

/// Leest het antwoord van `/chat/completions`: de tekst van het eerste bericht en het
/// tokengebruik. `None` als het antwoord niet die vorm heeft of leeg is. Een leeg antwoord dat
/// is afgekapt (alle tokens gingen op aan nadenken) komt wel terug, als afgekapt.
fn lees_antwoord(tekst: &str) -> Option<Antwoord> {
    #[derive(Deserialize)]
    struct Chat {
        choices: Vec<Keuze>,
        usage: Option<Gebruik>,
    }
    #[derive(Deserialize)]
    struct Keuze {
        message: Bericht,
        finish_reason: Option<String>,
    }
    #[derive(Deserialize)]
    struct Bericht {
        content: Option<Value>,
    }
    #[derive(Deserialize)]
    struct Gebruik {
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
    }

    let chat: Chat = serde_json::from_str(tekst).ok()?;
    let keuze = chat.choices.into_iter().next()?;
    // Meestal is `content` tekst; sommige modellen van Mistral geven een lijst stukken, waarvan
    // alleen de stukken van het type `text` het antwoord zijn.
    let afgekapt = keuze.finish_reason.as_deref() == Some("length");
    let tekst = match keuze.message.content.unwrap_or(Value::Null) {
        Value::String(tekst) => tekst,
        Value::Null if afgekapt => String::new(),
        Value::Array(stukken) => stukken
            .iter()
            .filter(|s| s.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|s| s.get("text").and_then(Value::as_str))
            .collect(),
        _ => return None,
    };
    if tekst.trim().is_empty() && !afgekapt {
        return None;
    }
    let gebruik = chat.usage.and_then(|g| {
        Some(Tokengebruik {
            invoer: g.prompt_tokens?,
            uitvoer: g.completion_tokens?,
        })
    });
    Some(Antwoord {
        tekst,
        gebruik,
        afgekapt,
    })
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    use super::*;

    const GEHEIM: &str = "sk-heel-geheim-42";
    const PROMPT: &str = "Vertrouwelijke vergadertekst over Jansen";

    /// Wat de nep-server bij elk verzoek terugstuurt.
    #[derive(Clone)]
    enum Gedrag {
        /// Een antwoord met deze status en body.
        Antwoord(u16, String),
        /// Zo lang wachten voor er een antwoord komt.
        Traag(Duration),
    }

    /// Wat de nep-server van een verzoek zag.
    #[derive(Clone, Debug)]
    struct Gezien {
        pad: String,
        autorisatie: Option<String>,
        body: String,
        /// Het adres van de client: een andere poort is een nieuwe TCP-verbinding.
        bron: Option<std::net::SocketAddr>,
    }

    /// Kleine HTTP-server op 127.0.0.1 voor de tests; geen echt netwerk.
    struct NepServer {
        adres: String,
        gezien: Arc<Mutex<Vec<Gezien>>>,
        stop: Arc<AtomicBool>,
        draad: Option<JoinHandle<()>>,
    }

    impl NepServer {
        fn start(gedrag: Gedrag) -> Self {
            let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
            let adres = format!("http://{}/v1", server.server_addr().to_ip().unwrap());
            let gezien = Arc::new(Mutex::new(Vec::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let (g, s) = (Arc::clone(&gezien), Arc::clone(&stop));
            let draad = thread::spawn(move || {
                while !s.load(Ordering::Relaxed) {
                    let Ok(Some(mut verzoek)) = server.recv_timeout(Duration::from_millis(20))
                    else {
                        continue;
                    };
                    let autorisatie = verzoek
                        .headers()
                        .iter()
                        .find(|h| h.field.equiv("Authorization"))
                        .map(|h| h.value.as_str().to_string());
                    let mut body = String::new();
                    let _ = std::io::Read::read_to_string(verzoek.as_reader(), &mut body);
                    g.lock().unwrap().push(Gezien {
                        pad: verzoek.url().to_string(),
                        autorisatie,
                        body,
                        bron: verzoek.remote_addr().copied(),
                    });
                    match &gedrag {
                        Gedrag::Antwoord(status, inhoud) => {
                            let _ = verzoek.respond(
                                tiny_http::Response::from_string(inhoud.clone())
                                    .with_status_code(*status),
                            );
                        }
                        Gedrag::Traag(duur) => {
                            thread::sleep(*duur);
                            let _ = verzoek.respond(tiny_http::Response::from_string("{}"));
                        }
                    }
                }
            });
            Self {
                adres,
                gezien,
                stop,
                draad: Some(draad),
            }
        }

        fn gezien(&self) -> Vec<Gezien> {
            self.gezien.lock().unwrap().clone()
        }

        /// Hoeveel verschillende TCP-verbindingen de server zag.
        fn verbindingen(&self) -> usize {
            let mut bronnen: Vec<_> = self.gezien().into_iter().map(|g| g.bron).collect();
            bronnen.sort();
            bronnen.dedup();
            bronnen.len()
        }
    }

    impl Drop for NepServer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(draad) = self.draad.take() {
                let _ = draad.join();
            }
        }
    }

    #[test]
    fn verbindingsfouten_krijgen_hun_eigen_soort() {
        use std::io;
        let m = Voorinstelling::Mistral;
        let tls = io::Error::new(
            io::ErrorKind::InvalidData,
            rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer),
        );
        let gevallen = [
            (ureq::Error::Io(tls), TaalmodelFout::Certificaat("Mistral")),
            (ureq::Error::Tls("x"), TaalmodelFout::Certificaat("Mistral")),
            (
                ureq::Error::InvalidProxyUrl,
                TaalmodelFout::Proxy("Mistral"),
            ),
            (
                ureq::Error::HostNotFound,
                TaalmodelFout::Onbereikbaar("Mistral"),
            ),
            (
                ureq::Error::Io(io::Error::from(io::ErrorKind::ConnectionReset)),
                TaalmodelFout::Onbereikbaar("Mistral"),
            ),
            (
                ureq::Error::Io(io::Error::from(io::ErrorKind::InvalidData)),
                TaalmodelFout::AntwoordOnleesbaar("Mistral"),
            ),
            (
                ureq::Error::BodyExceedsLimit(1),
                TaalmodelFout::AntwoordOnleesbaar("Mistral"),
            ),
            (
                ureq::Error::Io(io::Error::from(io::ErrorKind::PermissionDenied)),
                TaalmodelFout::Onverwacht("Mistral"),
            ),
            (
                ureq::Error::RedirectFailed,
                TaalmodelFout::Onverwacht("Mistral"),
            ),
        ];
        for (fout, verwacht) in gevallen {
            assert_eq!(fout_uit_verbinding(m, &fout), verwacht);
        }
        assert!(
            TaalmodelFout::Certificaat("Mistral")
                .to_string()
                .contains("Vraag het je ICT")
        );
    }

    #[test]
    fn een_sleutel_met_een_raar_teken_geeft_een_sleutelfout() {
        // Sleutel::nieuw laat dit niet toe; hier komt hij toch binnen, zoals uit een oude
        // sleutelbos. ureq weigert de header en Mondig zegt dat het aan de sleutel ligt.
        let server = NepServer::start(Gedrag::Antwoord(200, chat("klaar", "stop")));
        let verbinding = Verbinding::op_adres(
            Voorinstelling::Mistral,
            &server.adres,
            Some(Sleutel::ongecontroleerd("abc\ndef")),
        );
        let fout = verbinding.vraag(&aanvraag(false), &nooit()).unwrap_err();
        assert_eq!(fout, TaalmodelFout::Sleutel(SleutelFout::OnzichtbaarTeken));
        assert!(server.gezien().is_empty());
    }

    #[test]
    fn geen_modellenlijst_bij_een_404() {
        let server = NepServer::start(Gedrag::Antwoord(404, "{}".into()));
        let fout = mistral(&server).modellen(&nooit()).unwrap_err();
        assert_eq!(
            fout,
            TaalmodelFout::GeenModellenlijst(Voorinstelling::Mistral)
        );
        assert_eq!(
            fout.to_string(),
            "Deze aanbieder kan de lijst met modellen niet geven."
        );
        let ollama = Verbinding::op_adres(Voorinstelling::OllamaLokaal, &server.adres, None);
        let fout = ollama.modellen(&nooit()).unwrap_err();
        assert!(fout.to_string().contains("Werk Ollama bij"));
    }

    #[test]
    fn de_agent_wordt_hergebruikt_en_na_annuleren_vervangen() {
        let server = NepServer::start(Gedrag::Antwoord(200, chat("klaar", "stop")));
        let verbinding = mistral(&server);
        verbinding.vraag(&aanvraag(false), &nooit()).unwrap();
        verbinding.vraag(&aanvraag(false), &nooit()).unwrap();
        // Twee aanvragen, één TCP-verbinding: keep-alive werkt.
        assert_eq!(server.verbindingen(), 1);
        verbinding.sluit_af();
        verbinding.vraag(&aanvraag(false), &nooit()).unwrap();
        assert_eq!(server.verbindingen(), 2);
    }

    fn mistral(server: &NepServer) -> Verbinding {
        Verbinding::op_adres(
            Voorinstelling::Mistral,
            &server.adres,
            Some(Sleutel::nieuw(GEHEIM).unwrap()),
        )
    }

    fn aanvraag(schema: bool) -> Aanvraag {
        Aanvraag {
            model: "mistral-small-latest".into(),
            systeem: "Je maakt notulen.".into(),
            gebruiker: PROMPT.into(),
            temperatuur: 0.0,
            schema: schema.then(|| JsonSchema {
                naam: "test".into(),
                schema: json!({
                    "type": "object",
                    "properties": { "antwoord": { "type": "string" } },
                    "required": ["antwoord"],
                    "additionalProperties": false,
                }),
            }),
            max_tokens: Some(50),
            redeneren: false,
        }
    }

    fn chat(inhoud: &str, finish: &str) -> String {
        json!({
            "id": "x",
            "choices": [{ "index": 0, "message": { "role": "assistant", "content": inhoud }, "finish_reason": finish }],
            "usage": { "prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15 },
        })
        .to_string()
    }

    fn nooit() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn gelukt_zonder_schema_met_sleutel_en_gebruik() {
        let server = NepServer::start(Gedrag::Antwoord(200, chat("klaar", "stop")));
        let antwoord = mistral(&server).vraag(&aanvraag(false), &nooit()).unwrap();
        assert_eq!(antwoord.tekst, "klaar");
        assert_eq!(
            antwoord.gebruik,
            Some(Tokengebruik {
                invoer: 12,
                uitvoer: 3
            })
        );
        assert!(!antwoord.afgekapt);

        let gezien = server.gezien();
        assert_eq!(gezien.len(), 1);
        assert_eq!(gezien[0].pad, "/v1/chat/completions");
        assert_eq!(
            gezien[0].autorisatie.as_deref(),
            Some(format!("Bearer {GEHEIM}").as_str())
        );
        let body: Value = serde_json::from_str(&gezien[0].body).unwrap();
        assert_eq!(body["model"], "mistral-small-latest");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], PROMPT);
        assert_eq!(body["max_tokens"], 50);
        assert!(body.get("response_format").is_none());
        // Mistral krijgt geen veld voor nadenken.
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn gelukt_met_schema_stuurt_response_format_mee() {
        let server = NepServer::start(Gedrag::Antwoord(
            200,
            chat(r#"{"antwoord":"klaar"}"#, "stop"),
        ));
        let antwoord = mistral(&server).vraag(&aanvraag(true), &nooit()).unwrap();
        let json: Value = serde_json::from_str(&antwoord.tekst).unwrap();
        assert_eq!(json["antwoord"], "klaar");

        let body: Value = serde_json::from_str(&server.gezien()[0].body).unwrap();
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["name"], "test");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(
            body["response_format"]["json_schema"]["schema"]["required"][0],
            "antwoord"
        );
    }

    #[test]
    fn ollama_stuurt_geen_sleutel_mee() {
        let server = NepServer::start(Gedrag::Antwoord(200, chat("klaar", "stop")));
        let verbinding = Verbinding::op_adres(Voorinstelling::OllamaLokaal, &server.adres, None);
        verbinding.vraag(&aanvraag(false), &nooit()).unwrap();
        let gezien = server.gezien();
        assert_eq!(gezien[0].autorisatie, None);
        let body: Value = serde_json::from_str(&gezien[0].body).unwrap();
        assert_eq!(body["reasoning_effort"], "none");
    }

    #[test]
    fn een_leeg_afgekapt_antwoord_is_geen_onleesbaar_antwoord() {
        // Een redeneermodel dat alle tokens aan nadenken gaf (gezien bij qwen3 via Ollama).
        let body = json!({
            "choices": [{ "message": { "role": "assistant", "content": "", "reasoning": "Okay" }, "finish_reason": "length" }],
            "usage": { "prompt_tokens": 19, "completion_tokens": 20 },
        })
        .to_string();
        let antwoord = lees_antwoord(&body).unwrap();
        assert!(antwoord.afgekapt);
        assert!(antwoord.tekst.is_empty());
    }

    #[test]
    fn een_afgekapt_antwoord_wordt_gemeld() {
        let server = NepServer::start(Gedrag::Antwoord(200, chat("kla", "length")));
        let antwoord = mistral(&server).vraag(&aanvraag(false), &nooit()).unwrap();
        assert!(antwoord.afgekapt);
    }

    #[test]
    fn inhoud_als_lijst_van_stukken_wordt_tekst() {
        let body = json!({
            "choices": [{ "message": { "content": [
                { "type": "thinking", "thinking": [{ "type": "text", "text": "hmm" }] },
                { "type": "text", "text": "kl" },
                { "type": "text", "text": "aar" },
            ] }, "finish_reason": "stop" }],
        })
        .to_string();
        let antwoord = lees_antwoord(&body).unwrap();
        assert_eq!(antwoord.tekst, "klaar");
        assert_eq!(antwoord.gebruik, None);
    }

    #[test]
    fn foutcodes_worden_gewone_taal() {
        let gevallen = [
            (401, "{}", TaalmodelFout::SleutelOngeldig("Mistral")),
            (403, "{}", TaalmodelFout::SleutelOngeldig("Mistral")),
            (402, "{}", TaalmodelFout::Limiet("Mistral")),
            (429, "{}", TaalmodelFout::Limiet("Mistral")),
            (404, "{}", TaalmodelFout::ModelOntbreekt("Mistral")),
            (
                400,
                r#"{"object":"error","message":"Invalid model: x","type":"invalid_model","code":"1500"}"#,
                TaalmodelFout::ModelOntbreekt("Mistral"),
            ),
            (
                400,
                r#"{"message":"bad"}"#,
                TaalmodelFout::Afgewezen {
                    naam: "Mistral",
                    status: 400,
                },
            ),
            (
                503,
                "",
                TaalmodelFout::Storing {
                    naam: "Mistral",
                    status: 503,
                },
            ),
            (
                302,
                "",
                TaalmodelFout::Afgewezen {
                    naam: "Mistral",
                    status: 302,
                },
            ),
        ];
        for (status, body, verwacht) in gevallen {
            let server = NepServer::start(Gedrag::Antwoord(status, body.into()));
            let fout = mistral(&server)
                .vraag(&aanvraag(false), &nooit())
                .unwrap_err();
            assert_eq!(fout, verwacht, "status {status}");
        }
    }

    #[test]
    fn ollama_meldt_een_ontbrekend_model_in_de_tekst() {
        let body = r#"{"error":{"message":"model \"qwen9\" not found, try pulling it first","type":"api_error"}}"#;
        assert!(gaat_over_onbekend_model(body));
        assert!(!gaat_over_onbekend_model(
            r#"{"error":{"message":"bad request"}}"#
        ));
        assert!(!gaat_over_onbekend_model("geen json"));
    }

    #[test]
    fn de_vervolgactie_past_bij_de_fout() {
        assert_eq!(
            TaalmodelFout::Limiet("Mistral").vervolg(),
            Vervolg::OpnieuwProberen
        );
        assert_eq!(
            TaalmodelFout::ModelOntbreekt("Mistral").vervolg(),
            Vervolg::AnderModelKiezen
        );
        assert_eq!(
            TaalmodelFout::OllamaDraaitNiet.vervolg(),
            Vervolg::OpnieuwProberen
        );
        let melding = Foutmelding::from(TaalmodelFout::OllamaDraaitNiet);
        assert!(melding.melding.contains("Start Ollama"));
        assert_eq!(
            TaalmodelFout::SleutelOngeldig("Mistral").code(),
            "sleutel_ongeldig"
        );
    }

    #[test]
    fn een_onleesbaar_antwoord_is_een_eigen_fout() {
        for body in ["geen json", r#"{"choices":[]}"#, &chat("   ", "stop")] {
            let server = NepServer::start(Gedrag::Antwoord(200, body.to_string()));
            let fout = mistral(&server)
                .vraag(&aanvraag(false), &nooit())
                .unwrap_err();
            assert_eq!(fout, TaalmodelFout::AntwoordOnleesbaar("Mistral"));
        }
    }

    #[test]
    fn een_trage_aanbieder_geeft_een_time_out() {
        let server = NepServer::start(Gedrag::Traag(Duration::from_secs(2)));
        let verbinding = mistral(&server).met_opties(Opties {
            verbinden: Duration::from_secs(1),
            totaal: Duration::from_millis(300),
        });
        let begin = Instant::now();
        let fout = verbinding.vraag(&aanvraag(false), &nooit()).unwrap_err();
        assert_eq!(fout, TaalmodelFout::TijdOverschreden("Mistral"));
        assert!(begin.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn annuleren_stopt_meteen() {
        let server = NepServer::start(Gedrag::Traag(Duration::from_secs(2)));
        let annuleer = Arc::new(AtomicBool::new(false));
        let vlag = Arc::clone(&annuleer);
        let zetter = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            vlag.store(true, Ordering::Relaxed);
        });
        let begin = Instant::now();
        let fout = mistral(&server)
            .vraag(&aanvraag(false), &annuleer)
            .unwrap_err();
        zetter.join().unwrap();
        assert_eq!(fout, TaalmodelFout::Geannuleerd);
        assert!(begin.elapsed() < Duration::from_secs(1));
    }

    /// Een adres waar niemand luistert: een poort die net vrij is gegeven.
    fn dicht_adres() -> String {
        let luisteraar = TcpListener::bind("127.0.0.1:0").unwrap();
        let adres = format!("http://{}/v1", luisteraar.local_addr().unwrap());
        drop(luisteraar);
        adres
    }

    #[test]
    fn verbinding_geweigerd_bij_ollama_zegt_start_ollama() {
        let verbinding = Verbinding::op_adres(Voorinstelling::OllamaLokaal, &dicht_adres(), None);
        let fout = verbinding.vraag(&aanvraag(false), &nooit()).unwrap_err();
        assert_eq!(fout, TaalmodelFout::OllamaDraaitNiet);
    }

    #[test]
    fn verbinding_geweigerd_bij_mistral_is_onbereikbaar() {
        let verbinding = Verbinding::op_adres(
            Voorinstelling::Mistral,
            &dicht_adres(),
            Some(Sleutel::nieuw(GEHEIM).unwrap()),
        );
        let fout = verbinding.vraag(&aanvraag(false), &nooit()).unwrap_err();
        assert_eq!(fout, TaalmodelFout::Onbereikbaar("Mistral"));
    }

    #[test]
    fn modellen_worden_gesorteerd_opgehaald() {
        let body = json!({ "object": "list", "data": [
            { "id": "mistral-small-latest", "object": "model" },
            { "id": "codestral-latest", "object": "model" },
        ] })
        .to_string();
        let server = NepServer::start(Gedrag::Antwoord(200, body));
        let modellen = mistral(&server).modellen(&nooit()).unwrap();
        assert_eq!(modellen, ["codestral-latest", "mistral-small-latest"]);
        let gezien = server.gezien();
        assert_eq!(gezien[0].pad, "/v1/models");
        assert!(gezien[0].autorisatie.is_some());
    }

    #[test]
    fn sleutel_en_tekst_staan_nergens_in_fouten_of_debug() {
        // De aanbieder stuurt de sleutel en de prompt terug in zijn foutmelding.
        let echo = format!(r#"{{"message":"Bearer {GEHEIM} mag niet: {PROMPT}"}}"#);
        let server = NepServer::start(Gedrag::Antwoord(401, echo.clone()));
        let verbinding = mistral(&server);
        let fout = verbinding.vraag(&aanvraag(false), &nooit()).unwrap_err();

        let antwoord = Antwoord {
            tekst: PROMPT.into(),
            gebruik: None,
            afgekapt: false,
        };
        let uitvoer = [
            fout.to_string(),
            format!("{fout:?}"),
            Foutmelding::from(fout).melding,
            format!("{verbinding:?}"),
            format!("{:?}", aanvraag(true)),
            format!("{antwoord:?}"),
            TaalmodelFout::Afgewezen {
                naam: "Mistral",
                status: 400,
            }
            .to_string(),
        ];
        for tekst in &uitvoer {
            assert!(!tekst.contains(GEHEIM), "sleutel in: {tekst}");
            assert!(!tekst.contains("Bearer"), "header in: {tekst}");
            assert!(!tekst.contains("Jansen"), "prompt in: {tekst}");
        }
    }

    #[test]
    fn voorinstellingen_hebben_vaste_adressen_en_labels() {
        assert_eq!(
            Voorinstelling::Mistral.basisadres(),
            "https://api.mistral.ai/v1"
        );
        assert_eq!(
            Voorinstelling::Mistral.soevereiniteit(),
            Soevereiniteit::Europees
        );
        assert_eq!(
            Voorinstelling::Mistral.sleutelnaam(),
            Some("taalmodel-mistral")
        );
        assert_eq!(
            Voorinstelling::OllamaLokaal.basisadres(),
            "http://localhost:11434/v1"
        );
        assert_eq!(
            Voorinstelling::OllamaLokaal.soevereiniteit(),
            Soevereiniteit::Lokaal
        );
        assert!(!Voorinstelling::OllamaLokaal.sleutel_nodig());
    }

    #[test]
    fn zonder_sleutel_geen_verbinding_met_ollama_wel() {
        // Ollama heeft geen sleutelbos nodig, dus dit raakt de echte sleutelbos niet.
        let verbinding = Verbinding::voor(Voorinstelling::OllamaLokaal).unwrap();
        assert_eq!(verbinding.voorinstelling(), Voorinstelling::OllamaLokaal);
        assert!(is_lokaal(&verbinding.basisadres));
        assert!(!is_lokaal(Voorinstelling::Mistral.basisadres()));
    }
}
