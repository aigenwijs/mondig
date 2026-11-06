//! Een vergadering als gesprek: de twee sporen samen in beurten "Jij" en "Anderen", in
//! tijdsvolgorde (ADR 0008, sprint 4b-B).
//!
//! Elk spoor wordt per uiting uitgeschreven ([`LokaleEngine::transcribeer_spoor`]), met
//! tijden op de gezamenlijke tijdlijn van de opname. [`beurten`] vlecht die twee rijen in
//! elkaar; dat is een pure functie, zodat de regels zonder werkproces te testen zijn.
//! [`schrijf_gesprek_uit`] doet het hele stuk voor een opnamemap.
//!
//! Een vergadering "in de ruimte" ([`Vergadersoort::Ruimte`], sprint 4b-C) heeft alleen de
//! microfoon: dan worden de beurten alinea's (dezelfde regel van hooguit [`MAX_PAUZE`]
//! pauze), zonder sprekers, zonder echoregel en zonder de melding dat het geluid van de
//! computer ontbrak. Alleen ik ([`Vergadersoort::Alleen`], fase UX-D) gaat net zo. In een
//! online verslag kan de gebruiker een beurt een naam geven ([`Gesprek::geef_naam`]).
//!
//! Sprekerherkenning (4b-D, ADR 0018): staat het sprekermodel klaar, dan splitst
//! [`schrijf_gesprek_uit`] online het spoor "Anderen" en in de ruimte het microfoonspoor in
//! sprekers ([`sprekers::koppel`]). Een beurt heeft dan een [`Beurt::sprekernummer`] en heet
//! "Spreker 1", "Spreker 2", ... tot de gebruiker hem een naam geeft (per spreker met
//! [`Naambereik::Spreker`]). [`Gesprek::geef_aan_spreker`] en [`Gesprek::voeg_sprekers_samen`]
//! herstellen wat het model verkeerd deed. Zonder sprekermodel, met de instelling uit, of met
//! maar één gevonden spreker blijft alles zoals het was.

pub mod echo;
pub mod sprekers;

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::aanbieder::{AanbiederFout, LokaleEngine, Uitgeschreven, spoorfout};
use crate::engine::protocol::{Segment, SprekerSegment};
use crate::engine::supervisor::{EngineFout, Voortgang};
use crate::filter::FilterInstellingen;
use crate::spoor::{self, MICROFOON, SYSTEEM, Spoorlezer};
use crate::taak::{Foutmelding, Stap, Vervolg};

pub use crate::spoor::Vergadersoort;

use self::echo::{Ontdubbelen, ontdubbel};
use self::sprekers::{
    Toegewezen, koppel, vul_aan_met_dichtstbijzijnde, zonder_nummers_bij_een_spreker,
};
use crate::bewerken::schone_bewerking;

/// Zoveel loopt de microfoon achter op het geluid van de computer: gemeten 89 tot 116 ms
/// (ADR 0013). Dit gaat van de tijden van "Jij" af voordat de beurten op volgorde gaan.
pub const MICROFOON_VERTRAGING: Duration = Duration::from_millis(100);

/// Een pauze van hooguit zo lang tussen twee uitingen van dezelfde kant houdt ze in één beurt.
pub const MAX_PAUZE: Duration = Duration::from_secs(3);

/// Wie er spreekt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kant {
    /// Het microfoonspoor: de gebruiker zelf.
    Jij,
    /// Het systeemspoor: wat de computer afspeelde, de anderen in de vergadering.
    Anderen,
}

impl Kant {
    /// De naam zoals de gebruiker hem ziet.
    #[must_use]
    pub fn naam(self) -> &'static str {
        match self {
            Self::Jij => "Jij",
            Self::Anderen => "Anderen",
        }
    }
}

/// Hoeveel sprekers een kant hooguit kan hebben. Sortformer vindt er 4; de gebruiker kan er
/// meer maken als het model twee mensen samennam ([`Gesprek::geef_aan_spreker`]).
pub const MAX_SPREKERS: u8 = 8;

/// De naam van een spreker zonder eigen naam: "Spreker 1". Nederlands, zoals "Jij" en
/// "Anderen"; de interface vertaalt met het nummer (ADR 0007).
#[must_use]
pub fn sprekernaam(nummer: u8) -> String {
    format!("Spreker {nummer}")
}

/// Eén beurt in het gesprek: een of meer uitingen van dezelfde kant (en spreker) achter elkaar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Beurt {
    /// Wie er spreekt.
    pub kant: Kant,
    /// Welke spreker van die kant, als de sprekers herkend zijn (vanaf 1). `None`: zonder
    /// sprekerherkenning, of er was geen spreker in de buurt.
    pub sprekernummer: Option<u8>,
    /// Begin op de tijdlijn van de opname, in milliseconden.
    pub start_ms: u64,
    /// Eind op de tijdlijn, in milliseconden.
    pub eind_ms: u64,
    /// De tekst, uitingen gescheiden door een spatie.
    pub tekst: String,
    /// Een naam die de gebruiker de spreker gaf ("Wethouder De Vries", of "Zaal" voor "Jij"
    /// bij een hybride vergadering). `None`: de naam van de kant.
    pub naam: Option<String>,
    /// Heeft de gebruiker de tekst van deze beurt zelf verbeterd ([`Gesprek::zet_tekst`],
    /// ADR 0023)? Opnieuw uitschrijven schrijft zo'n regel opnieuw uit; de dialoog telt ze
    /// vooraf. Ontbreekt in een `verslag.json` van voor deze stap, en is dan `false`.
    #[serde(default)]
    pub verbeterd: bool,
}

impl Beurt {
    /// Wie er spreekt zoals de gebruiker het ziet: de gegeven naam, anders "Spreker 1" (als de
    /// sprekers herkend zijn), anders de kant.
    #[must_use]
    pub fn spreker(&self) -> String {
        self.naam.clone().unwrap_or_else(|| self.standaardnaam())
    }

    /// De naam zonder eigen naam: "Spreker 1", of de naam van de kant.
    #[must_use]
    pub fn standaardnaam(&self) -> String {
        self.sprekernummer
            .map_or_else(|| self.kant.naam().to_string(), sprekernaam)
    }
}

/// Zet de uitingen van beide sporen om in beurten.
///
/// Eerst gaat [`MICROFOON_VERTRAGING`] van de tijden van "Jij" af (niet onder nul). Dan
/// gaan alle uitingen op volgorde van begin; bij een gelijk begin komt "Anderen" eerst.
/// Opeenvolgende uitingen van dezelfde kant worden één beurt als de pauze ertussen hooguit
/// [`MAX_PAUZE`] is. Uitingen zonder tekst tellen niet mee.
#[must_use]
pub fn beurten(jij: &[Segment], anderen: &[Segment]) -> Vec<Beurt> {
    beurten_met_sprekers(jij, &zonder_sprekers(anderen))
}

/// Als [`beurten`], maar "Anderen" is gekoppeld aan sprekers ([`sprekers::koppel`]): een beurt
/// gaat alleen door bij dezelfde spreker.
#[must_use]
pub fn beurten_met_sprekers(jij: &[Segment], anderen: &[Toegewezen]) -> Vec<Beurt> {
    vlecht(&zonder_sprekers(jij), anderen, MICROFOON_VERTRAGING)
}

/// De alinea's van een vergadering in de ruimte: de uitingen van de microfoon, samengevoegd
/// bij een pauze van hooguit [`MAX_PAUZE`], zoals [`beurten`] maar zonder verschuiving (er is
/// geen tweede spoor om op te wachten).
#[must_use]
pub fn alineas(jij: &[Segment]) -> Vec<Beurt> {
    alineas_met_sprekers(&zonder_sprekers(jij))
}

/// Als [`alineas`], met sprekers ([`sprekers::koppel`]): een vergadering in de ruimte waarvan
/// het microfoonspoor in sprekers is gesplitst. Een alinea gaat alleen door bij dezelfde spreker.
#[must_use]
pub fn alineas_met_sprekers(jij: &[Toegewezen]) -> Vec<Beurt> {
    vlecht(jij, &[], Duration::ZERO)
}

/// Uitingen zonder spreker, voor de route zonder sprekerherkenning.
fn zonder_sprekers(segmenten: &[Segment]) -> Vec<Toegewezen> {
    segmenten
        .iter()
        .map(|s| Toegewezen {
            segment: s.clone(),
            spreker: None,
        })
        .collect()
}

/// Zie [`beurten`]; `vertraging` gaat van de tijden van "Jij" af.
fn vlecht(jij: &[Toegewezen], anderen: &[Toegewezen], vertraging: Duration) -> Vec<Beurt> {
    voeg_samen(los_op_volgorde(jij, anderen, vertraging))
}

/// De uitingen van [`vlecht`] als losse beurten op volgorde, nog niet samengevoegd.
fn los_op_volgorde(jij: &[Toegewezen], anderen: &[Toegewezen], vertraging: Duration) -> Vec<Beurt> {
    let vertraging = u64::try_from(vertraging.as_millis()).unwrap_or(u64::MAX);
    let uiting = |kant: Kant, t: &Toegewezen, vertraging: u64| Beurt {
        kant,
        sprekernummer: t.spreker,
        start_ms: t.segment.start_ms.saturating_sub(vertraging),
        eind_ms: t.segment.eind_ms.saturating_sub(vertraging),
        tekst: t.segment.tekst.trim().to_string(),
        naam: None,
        verbeterd: false,
    };
    let mut uitingen: Vec<Beurt> = jij
        .iter()
        .map(|t| uiting(Kant::Jij, t, vertraging))
        .chain(anderen.iter().map(|t| uiting(Kant::Anderen, t, 0)))
        .filter(|u| !u.tekst.is_empty())
        .collect();
    // Stabiel sorteren: binnen een kant blijft de volgorde van het spoor.
    uitingen.sort_by_key(|u| (u.start_ms, u.kant == Kant::Jij));
    uitingen
}

/// Mogen deze twee beurten (na elkaar) één beurt worden? Dezelfde kant, dezelfde spreker,
/// dezelfde naam, en hooguit [`MAX_PAUZE`] ertussen.
fn hoort_bij(vorige: &Beurt, volgende: &Beurt) -> bool {
    let max_pauze = u64::try_from(MAX_PAUZE.as_millis()).unwrap_or(u64::MAX);
    vorige.kant == volgende.kant
        && vorige.sprekernummer == volgende.sprekernummer
        && vorige.naam == volgende.naam
        && volgende.start_ms.saturating_sub(vorige.eind_ms) <= max_pauze
}

/// Voegt opvolgende beurten samen volgens [`hoort_bij`], met een spatie tussen de teksten.
fn voeg_samen(beurten: Vec<Beurt>) -> Vec<Beurt> {
    let mut uit: Vec<Beurt> = Vec::with_capacity(beurten.len());
    for beurt in beurten {
        match uit.last_mut() {
            Some(vorige) if hoort_bij(vorige, &beurt) => {
                vorige.start_ms = vorige.start_ms.min(beurt.start_ms);
                vorige.eind_ms = vorige.eind_ms.max(beurt.eind_ms);
                vorige.tekst.push(' ');
                vorige.tekst.push_str(&beurt.tekst);
                // Een samengevoegde beurt met een verbeterd deel is verbeterd.
                vorige.verbeterd |= beurt.verbeterd;
            }
            _ => uit.push(beurt),
        }
    }
    uit
}

/// Wat er met een spoor van de opname was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spoorstand {
    /// Het spoor is er en liep de hele opname door.
    Opgenomen,
    /// Het spoor is er niet (bijvoorbeeld een opname zonder geluid van de computer).
    Ontbrak,
    /// De bron viel tijdens de opname uit; wat tot dan opgenomen was, staat erin.
    Uitgevallen,
    /// Het spoor is er, maar het uitschrijven mislukte; het andere spoor staat er wel in.
    Mislukt,
}

/// De melding als het systeemspoor ontbrak.
pub const MELDING_ZONDER_SYSTEEM: &str =
    "Het geluid van de computer is niet opgenomen. In het verslag staat alleen wat jij zei.";

/// De melding als het systeemspoor tijdens de opname uitviel.
pub const MELDING_SYSTEEM_UITGEVALLEN: &str = "Het geluid van de computer viel tijdens de opname weg. Vanaf dat moment staat alleen wat jij zei in het verslag.";

/// De melding als de microfoon tijdens de opname uitviel.
pub const MELDING_MICROFOON_UITGEVALLEN: &str = "De microfoon viel tijdens de opname weg. Vanaf dat moment staat alleen wat de anderen zeiden in het verslag.";

/// De melding als de microfoon uitviel bij een vergadering in de ruimte (er is geen ander spoor).
pub const MELDING_MICROFOON_UITGEVALLEN_RUIMTE: &str =
    "De microfoon viel tijdens de opname weg. Het verslag loopt tot dat moment.";

/// De melding als het uitschrijven van het systeemspoor mislukte; de fout volgt erna.
pub const MELDING_SYSTEEM_MISLUKT: &str = "Wat de anderen zeiden kon niet worden uitgeschreven, dus in het verslag staat alleen wat jij zei.";

/// De melding als het uitschrijven van het microfoonspoor mislukte; de fout volgt erna.
pub const MELDING_MICROFOON_MISLUKT: &str = "Wat jij zei kon niet worden uitgeschreven, dus in het verslag staat alleen wat de anderen zeiden.";

/// De melding als er uitingen van "Jij" zijn weggelaten omdat ze echo van "Anderen" waren.
pub const MELDING_ECHO_WEGGELATEN: &str = "Een deel van wat de anderen zeiden kwam ook via je microfoon binnen. Die dubbele tekst is weggelaten. Met een koptelefoon wordt het verslag het best.";

/// De melding als het herkennen van de sprekers mislukte; de fout volgt erna. Het verslag is
/// er wel, zoals zonder sprekerherkenning.
pub const MELDING_SPREKERS_MISLUKT: &str =
    "De sprekers konden niet worden herkend, dus het verslag staat er zonder sprekers.";

/// De melding als de opname na een onderbreking is hersteld.
pub const MELDING_HERSTELD: &str = "De opname werd onderbroken en is daarna hersteld. Het laatste stukje voor de onderbreking kan ontbreken.";

/// Een uitgeschreven vergadering. Bewust zonder `Debug`, zodat de tekst nooit in een log
/// komt (zoals [`crate::aanbieder::Uitgeschreven`]). Wel te bewaren naast de opname
/// ([`crate::verslag`], ADR 0022).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gesprek {
    /// De beurten in tijdsvolgorde.
    pub beurten: Vec<Beurt>,
    /// Lengte van de opname (uit `einde.json`).
    pub duur: Duration,
    /// Wat de gebruiker moet weten, in gewone taal, zoals [`spoor::MELDING_STIL_SYSTEEM`].
    pub meldingen: Vec<String>,
    /// Het microfoonspoor ("Jij").
    pub jij: Spoorstand,
    /// Het systeemspoor ("Anderen").
    pub anderen: Spoorstand,
    /// Hoeveel uitingen van "Jij" er zijn weggelaten omdat ze echo van "Anderen" waren.
    pub weggelaten: usize,
    /// Online of in de ruimte, uit `opname.json`.
    pub soort: Vergadersoort,
}

/// Voor welke beurten een naam geldt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Naambereik {
    /// Alleen deze beurt.
    Beurt,
    /// Alle beurten van deze kant die nu dezelfde naam hebben als deze beurt. Een beurt
    /// die eerder apart een naam kreeg, houdt die.
    Kant,
    /// Alle beurten van deze spreker (zelfde kant en sprekernummer) die nu dezelfde naam
    /// hebben als deze beurt (4b-D). Een beurt die eerder apart een naam kreeg, houdt die.
    /// Zonder sprekernummer werkt dit als [`Naambereik::Kant`] voor de beurten zonder nummer.
    Spreker,
}

/// Zoveel tekens mag een naam hebben.
pub const MAX_NAAM: usize = 60;

/// Zoveel tekens mag de naam van een heel verslag hebben (ADR 0022): langer dan de naam van
/// een spreker, want het is een titel.
pub const MAX_VERSLAGNAAM: usize = 100;

/// Maakt een naam schoon en kapt hem af op `max` tekens: witruimte wordt één spatie, andere
/// tekens die geen tekst zijn vallen weg. Leeg geeft `None`.
fn schoon_tot(naam: &str, max: usize) -> Option<String> {
    let zichtbaar: String = naam
        .chars()
        .filter(|c| c.is_whitespace() || !c.is_control())
        .collect();
    let schoon: String = zichtbaar
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect();
    let schoon = schoon.trim_end();
    (!schoon.is_empty()).then(|| schoon.to_string())
}

/// Een naam zoals de gebruiker hem typte, schoongemaakt: witruimte wordt één spatie, andere
/// tekens die geen tekst zijn vallen weg, hooguit [`MAX_NAAM`] tekens. Leeg geeft `None`.
fn schone_naam(naam: &str) -> Option<String> {
    schoon_tot(naam, MAX_NAAM)
}

/// De naam die de gebruiker een verslag gaf, schoongemaakt zoals [`schone_naam`] maar hooguit
/// [`MAX_VERSLAGNAAM`] tekens. Leeg geeft `None`: dan maakt Mondig de titel zelf.
#[must_use]
pub fn schone_verslagnaam(naam: &str) -> Option<String> {
    schoon_tot(naam, MAX_VERSLAGNAAM)
}

impl Gesprek {
    /// Geeft de spreker van beurt `index` een naam, voor alleen die beurt of voor alle beurten
    /// van die kant met dezelfde naam (zie [`Naambereik`]). Een lege naam, of de naam van de
    /// kant zelf ("Jij", "Anderen"), zet de naam terug. Geeft hoeveel beurten er veranderden.
    ///
    /// # Errors
    ///
    /// Geeft een [`Foutmelding`] bij een vergadering in de ruimte (daar zijn geen sprekers) of
    /// als de beurt er niet is.
    pub fn geef_naam(
        &mut self,
        index: usize,
        naam: &str,
        bereik: Naambereik,
    ) -> Result<usize, Foutmelding> {
        if !self.met_sprekers() {
            return Err(geen_sprekers());
        }
        let beurt = self.beurten.get(index).ok_or_else(geen_beurt)?;
        let kant = beurt.kant;
        let nummer = beurt.sprekernummer;
        let huidig = beurt.spreker();
        // De standaardnaam ("Anderen", "Spreker 2") intypen zet de naam terug.
        let nieuw = schone_naam(naam).filter(|n| *n != beurt.standaardnaam());
        let mut veranderd = 0;
        for (i, b) in self.beurten.iter_mut().enumerate() {
            let hoort_erbij = match bereik {
                Naambereik::Beurt => i == index,
                Naambereik::Kant => b.kant == kant && b.spreker() == huidig,
                Naambereik::Spreker => {
                    b.kant == kant && b.sprekernummer == nummer && b.spreker() == huidig
                }
            };
            if hoort_erbij && b.naam != nieuw {
                b.naam.clone_from(&nieuw);
                veranderd += 1;
            }
        }
        Ok(veranderd)
    }

    /// Vervangt de tekst van beurt `index` door `tekst` (V-1: een regel verbeteren). Kopiëren en
    /// alle exports lezen daarna de nieuwe tekst. De tekst wordt schoongemaakt
    /// ([`crate::bewerken::schone_bewerking`]); een lege tekst laat de oude staan. Geeft `true`
    /// als de tekst veranderde. Tijden en spreker blijven zoals ze zijn.
    ///
    /// # Errors
    ///
    /// Geeft een [`Foutmelding`] als de beurt er niet is.
    pub fn zet_tekst(&mut self, index: usize, tekst: &str) -> Result<bool, Foutmelding> {
        let beurt = self.beurten.get_mut(index).ok_or_else(geen_beurt)?;
        let Some(schoon) = schone_bewerking(tekst) else {
            return Ok(false);
        };
        if beurt.tekst == schoon {
            return Ok(false);
        }
        beurt.tekst = schoon;
        beurt.verbeterd = true;
        Ok(true)
    }

    /// Hoeveel beurten de gebruiker zelf verbeterde (ADR 0023). Voor de dialoog "Opnieuw
    /// uitschrijven": die regels worden opnieuw uitgeschreven. Een verslag van voor ADR 0023
    /// telt 0; dat is te weinig, maar nooit te veel.
    #[must_use]
    pub fn verbeterde_regels(&self) -> usize {
        self.beurten.iter().filter(|b| b.verbeterd).count()
    }

    /// Hoeveel geluid het uitschrijven van dit gesprek verwerkt: de duur keer het aantal
    /// sporen (online met het geluid van de computer twee, anders één). Voor de gemeten
    /// snelheid per model (ADR 0023, punt 6).
    #[must_use]
    pub fn verwerkt_geluid(&self) -> Duration {
        let sporen = if self.soort.met_systeem() && self.anderen != Spoorstand::Ontbrak {
            2
        } else {
            1
        };
        self.duur * sporen
    }

    /// Staan er sprekers in het verslag? Online altijd ("Jij", "Anderen" of een naam), en ook
    /// in de ruimte als de sprekers herkend zijn (4b-D). Alleen dan tonen verslag en export
    /// wie er sprak.
    #[must_use]
    pub fn met_sprekers(&self) -> bool {
        self.soort.met_sprekers() || self.beurten.iter().any(|b| b.sprekernummer.is_some())
    }

    /// De kant waarvan de sprekers herkend worden: online "Anderen", in de ruimte de
    /// microfoon ("Jij"). Alleen ik heeft geen sprekers.
    #[must_use]
    pub fn sprekerkant(&self) -> Option<Kant> {
        match self.soort {
            Vergadersoort::Online => Some(Kant::Anderen),
            Vergadersoort::Ruimte => Some(Kant::Jij),
            Vergadersoort::Alleen => None,
        }
    }

    /// De sprekernummers die in het verslag voorkomen, oplopend.
    #[must_use]
    pub fn sprekernummers(&self) -> Vec<u8> {
        let mut nummers: Vec<u8> = self
            .beurten
            .iter()
            .filter_map(|b| b.sprekernummer)
            .collect();
        nummers.sort_unstable();
        nummers.dedup();
        nummers
    }

    /// Geeft beurt `index` aan spreker `nummer` (4b-D), bijvoorbeeld als het model twee mensen
    /// verwisselde, of een nieuw nummer als het model twee mensen samennam. De beurt krijgt de
    /// naam van die spreker (van de eerste beurt met dat nummer), of geen naam als die spreker
    /// nog geen naam heeft. Daarna gaat de beurt samen met een buur van dezelfde spreker (zoals
    /// [`beurten`] dat doet). Geeft hoeveel beurten er veranderden (0 of 1).
    ///
    /// # Errors
    ///
    /// Geeft een [`Foutmelding`] als de beurt er niet is, als deze kant geen sprekers heeft
    /// (online "Jij", of Alleen ik) of als het nummer niet tussen 1 en [`MAX_SPREKERS`] ligt.
    pub fn geef_aan_spreker(&mut self, index: usize, nummer: u8) -> Result<usize, Foutmelding> {
        let beurt = self.beurten.get(index).ok_or_else(geen_beurt)?;
        if self.sprekerkant() != Some(beurt.kant) {
            return Err(geen_sprekers());
        }
        if !(1..=MAX_SPREKERS).contains(&nummer) {
            return Err(Foutmelding::nieuw(
                format!("Kies een spreker van 1 tot en met {MAX_SPREKERS}."),
                Vervolg::Geen,
            ));
        }
        if beurt.sprekernummer == Some(nummer) {
            return Ok(0);
        }
        let kant = beurt.kant;
        // In de ruimte zonder sprekers (geen sprekermodel, of maar één spreker gevonden) zijn
        // alle alinea's van één persoon. Krijgt er een een spreker, dan worden de andere
        // Spreker 1, anders heten ze ineens "Jij".
        if self.soort == Vergadersoort::Ruimte && self.sprekernummers().is_empty() {
            for (i, b) in self.beurten.iter_mut().enumerate() {
                if i != index && b.kant == kant {
                    b.sprekernummer = Some(1);
                }
            }
        }
        let naam = self.naam_van_spreker(kant, nummer);
        if let Some(beurt) = self.beurten.get_mut(index) {
            beurt.sprekernummer = Some(nummer);
            beurt.naam = naam;
        }
        self.voeg_buren_samen();
        Ok(1)
    }

    /// Voegt spreker `van` samen met spreker `naar` (4b-D): alle beurten van `van` worden van
    /// `naar` en krijgen de naam van `naar` (of geen naam als die er nog geen heeft). Naast
    /// elkaar liggende beurten gaan daarna samen. Geeft hoeveel beurten er veranderden.
    ///
    /// # Errors
    ///
    /// Geeft een [`Foutmelding`] als dit verslag geen sprekers heeft, of als `van` en `naar`
    /// gelijk zijn of niet tussen 1 en [`MAX_SPREKERS`] liggen.
    pub fn voeg_sprekers_samen(&mut self, van: u8, naar: u8) -> Result<usize, Foutmelding> {
        let kant = self.sprekerkant().ok_or_else(geen_sprekers)?;
        if van == naar || ![van, naar].iter().all(|n| (1..=MAX_SPREKERS).contains(n)) {
            return Err(Foutmelding::nieuw(
                "Kies twee verschillende sprekers.",
                Vervolg::Geen,
            ));
        }
        // De naam van `naar`; heeft die er geen, dan die van `van` ("Piet" samenvoegen in een
        // naamloze spreker houdt "Piet").
        let naam_van = self.naam_van_spreker(kant, van);
        let naam_naar = self.naam_van_spreker(kant, naar);
        let doelnaam = naam_naar.clone().or_else(|| naam_van.clone());
        let mut veranderd = 0;
        for b in &mut self.beurten {
            let eigen = match b.sprekernummer {
                Some(n) if b.kant == kant && n == van => &naam_van,
                Some(n) if b.kant == kant && n == naar => &naam_naar,
                _ => continue,
            };
            // Een beurt die apart een naam kreeg (anders dan zijn spreker), houdt die.
            let apart = b.naam.is_some() && b.naam != *eigen;
            let nieuwe_naam = if apart {
                b.naam.clone()
            } else {
                doelnaam.clone()
            };
            if b.sprekernummer != Some(naar) || b.naam != nieuwe_naam {
                b.sprekernummer = Some(naar);
                b.naam = nieuwe_naam;
                veranderd += 1;
            }
        }
        self.voeg_buren_samen();
        Ok(veranderd)
    }

    /// Geeft beurt `index` aan een nieuwe spreker: het laagste nummer dat nog niet in het
    /// verslag staat (zoals "Nieuwe spreker" in Vibe). Voor wie het model bij een ander
    /// indeelde, bijvoorbeeld iemand die laat instroomde (D-0). Werkt ook voor een beurt zonder
    /// spreker. Geeft het nieuwe nummer.
    ///
    /// # Errors
    ///
    /// Als [`Gesprek::geef_aan_spreker`], en als alle [`MAX_SPREKERS`] nummers bezet zijn.
    pub fn geef_aan_nieuwe_spreker(&mut self, index: usize) -> Result<u8, Foutmelding> {
        let mut bezet = self.sprekernummers();
        // In de ruimte zonder sprekers worden de andere alinea's Spreker 1 (zie
        // `geef_aan_spreker`), dus de nieuwe spreker is 2.
        if self.soort == Vergadersoort::Ruimte && bezet.is_empty() {
            bezet.push(1);
        }
        let nummer = (1..=MAX_SPREKERS)
            .find(|n| !bezet.contains(n))
            .ok_or_else(|| {
                Foutmelding::nieuw(
                    format!("Er zijn al {MAX_SPREKERS} sprekers; voeg er eerst twee samen."),
                    Vervolg::Geen,
                )
            })?;
        self.geef_aan_spreker(index, nummer)?;
        Ok(nummer)
    }

    /// De naam die spreker `nummer` van `kant` nu heeft: de naam die het vaakst bij zijn
    /// beurten staat ("geen naam" telt mee). Een beurt die apart een naam kreeg, bepaalt de
    /// naam van de spreker dus niet. Bij gelijke stand de naam die het eerst voorkomt.
    fn naam_van_spreker(&self, kant: Kant, nummer: u8) -> Option<String> {
        let mut tellingen: Vec<(Option<&str>, usize)> = Vec::new();
        for b in self
            .beurten
            .iter()
            .filter(|b| b.kant == kant && b.sprekernummer == Some(nummer))
        {
            let naam = b.naam.as_deref();
            match tellingen.iter_mut().find(|(n, _)| *n == naam) {
                Some((_, aantal)) => *aantal += 1,
                None => tellingen.push((naam, 1)),
            }
        }
        let meeste = tellingen.iter().map(|(_, aantal)| *aantal).max()?;
        tellingen
            .into_iter()
            .find(|(_, aantal)| *aantal == meeste)
            .and_then(|(naam, _)| naam.map(str::to_string))
    }

    /// Voegt naast elkaar liggende beurten van dezelfde spreker samen, met dezelfde regel als
    /// [`beurten`] (hooguit [`MAX_PAUZE`] ertussen).
    fn voeg_buren_samen(&mut self) {
        self.beurten = voeg_samen(std::mem::take(&mut self.beurten));
    }

    /// Het verslag als platte tekst, om te kopiëren: per beurt een alinea, met de spreker
    /// ervoor ("Jij: ...") als er sprekers zijn, anders zonder.
    #[must_use]
    pub fn als_tekst(&self) -> String {
        self.als_tekst_met(str::to_string, str::to_string)
    }

    /// Zoals [`Gesprek::als_tekst`], maar elke tekst gaat eerst door `bewerk` en elke naam van een
    /// spreker door `bewerk_naam` (bijvoorbeeld het vervangen van persoonsgegevens, V-2).
    #[must_use]
    pub fn als_tekst_met(
        &self,
        bewerk: impl Fn(&str) -> String,
        bewerk_naam: impl Fn(&str) -> String,
    ) -> String {
        let met_sprekers = self.met_sprekers();
        self.beurten
            .iter()
            .map(|b| {
                if met_sprekers {
                    format!("{}: {}", bewerk_naam(&b.spreker()), bewerk(&b.tekst))
                } else {
                    bewerk(&b.tekst)
                }
            })
            .collect::<Vec<_>>()
            .join(
                "

",
            )
    }
}

fn geen_sprekers() -> Foutmelding {
    Foutmelding::nieuw("In dit verslag staan geen sprekers.", Vervolg::Geen)
}

fn geen_beurt() -> Foutmelding {
    Foutmelding::nieuw(
        "Deze beurt staat niet (meer) in het verslag.",
        Vervolg::Geen,
    )
}

/// Schrijft een afgesloten vergaderopname uit als gesprek: beide sporen via
/// [`LokaleEngine::transcribeer_spoor`], daarna samengevoegd met [`beurten`]. Zonder
/// systeemspoor komt er alleen "Jij" in.
///
/// De [`Voortgang`] loopt over beide sporen samen: eerst de microfoon, dan het systeemspoor,
/// met als totaal hun lengte samen. `annuleer` breekt het lopende spoor af; dan komt er
/// [`crate::engine::supervisor::EngineFout::Geannuleerd`] en geen half gesprek. `filter`
/// geldt per spoor, zoals bij een bestand.
///
/// Met `ontdubbelen` en een systeemspoor vallen de uitingen van "Jij" weg die volgens de
/// echoregel van [`echo::tekst_echo`] op "Anderen" lijken (de microfoon hoorde de luidspreker);
/// [`Gesprek::weggelaten`] telt ze en [`MELDING_ECHO_WEGGELATEN`] zegt het. `None` laat alles staan.
///
/// Faalt het uitschrijven van één spoor, dan komt het gesprek er toch, met alleen het andere
/// spoor, [`Spoorstand::Mislukt`] en een melding met de fout: een uur vergaderen gaat niet
/// verloren omdat één spoor niet lukt.
///
/// Met `sprekers` en een sprekermodel naast het spraakmodel ([`LokaleEngine::heeft_sprekermodel`])
/// worden daarna de sprekers herkend: online op "Anderen", in de ruimte op de microfoon (4b-D).
/// Mislukt dat, dan komt het verslag zonder sprekers met [`MELDING_SPREKERS_MISLUKT`]; een
/// annulering breekt ook hier het hele gesprek af. Bij Alleen ik gebeurt het niet.
///
/// # Errors
///
/// Geeft een [`AanbiederFout`] als de opname niet is afgesloten, bij een annulering, of als
/// geen enkel spoor lukt (dan de fout van de microfoon); ook als de stiltedetectie niet
/// klaarstaat, want die geldt voor beide sporen.
pub fn schrijf_gesprek_uit(
    engine: &mut LokaleEngine,
    opnamemap: &Path,
    filter: Option<&FilterInstellingen>,
    ontdubbelen: Option<&Ontdubbelen>,
    sprekers: bool,
    voortgang: &mut dyn FnMut(Voortgang),
    annuleer: &AtomicBool,
) -> Result<Gesprek, AanbiederFout> {
    schrijf_uit_met(
        engine,
        opnamemap,
        filter,
        ontdubbelen,
        Sprekerbron::Model(sprekers),
        voortgang,
        annuleer,
    )
}

/// Schrijft een vergadering opnieuw uit met het model van `engine` (ADR 0023): de sporen gaan
/// opnieuw door het spraakmodel, maar het sprekermodel draait niet. Wie wanneer sprak komt uit
/// het huidige verslag `oud`, met de verbeteringen van de gebruiker erin; zie
/// [`beurten_opnieuw`] voor de regels. Voortgang, annuleren, filter, echo en de meldingen over
/// de sporen gaan zoals bij [`schrijf_gesprek_uit`].
///
/// # Errors
///
/// Als [`schrijf_gesprek_uit`]. Bij een fout of annulering is er geen half gesprek: de
/// aanroeper houdt het oude.
pub fn schrijf_gesprek_opnieuw_uit(
    engine: &mut LokaleEngine,
    opnamemap: &Path,
    filter: Option<&FilterInstellingen>,
    ontdubbelen: Option<&Ontdubbelen>,
    oud: &Gesprek,
    voortgang: &mut dyn FnMut(Voortgang),
    annuleer: &AtomicBool,
) -> Result<Gesprek, AanbiederFout> {
    schrijf_uit_met(
        engine,
        opnamemap,
        filter,
        ontdubbelen,
        Sprekerbron::Verslag(oud),
        voortgang,
        annuleer,
    )
}

/// De lengte van een spoor, alleen voor de voortgang: een spoor dat niet te lezen is, faalt
/// straks bij het uitschrijven en valt dan terug op het andere spoor.
fn spoorlengte(map: &Path) -> Duration {
    Spoorlezer::open(map).map_or(Duration::ZERO, |lezer| {
        Duration::from_millis(lezer.tijdlijn().stroom_ms())
    })
}

/// Waar de sprekers van een gesprek vandaan komen.
#[derive(Clone, Copy)]
enum Sprekerbron<'a> {
    /// Het sprekermodel, als het waar is en het model klaarstaat.
    Model(bool),
    /// Het huidige verslag, bij opnieuw uitschrijven (ADR 0023).
    Verslag(&'a Gesprek),
}

/// Het gedeelde werk van [`schrijf_gesprek_uit`] en [`schrijf_gesprek_opnieuw_uit`].
fn schrijf_uit_met(
    engine: &mut LokaleEngine,
    opnamemap: &Path,
    filter: Option<&FilterInstellingen>,
    ontdubbelen: Option<&Ontdubbelen>,
    bron: Sprekerbron<'_>,
    voortgang: &mut dyn FnMut(Voortgang),
    annuleer: &AtomicBool,
) -> Result<Gesprek, AanbiederFout> {
    let sprekers = matches!(bron, Sprekerbron::Model(true));
    let afsluiting = spoor::lees_afsluiting(opnamemap).map_err(|f| spoorfout(&f))?;
    let soort = spoor::lees_soort(opnamemap);
    // In de ruimte en bij Alleen ik: alleen de microfoon, alinea's zonder sprekers.
    let ruimte = !soort.met_systeem();
    let microfoon = opnamemap.join(MICROFOON);
    let systeem = opnamemap.join(SYSTEEM);
    // In de ruimte telt alleen de microfoon, ook als er toch een systeemspoor zou staan.
    let met_systeem = !ruimte && afsluiting.systeem.is_some() && systeem.is_dir();
    let lengte_microfoon = spoorlengte(&microfoon);
    let lengte_systeem = if met_systeem {
        spoorlengte(&systeem)
    } else {
        Duration::ZERO
    };
    let totaal = Some(lengte_microfoon + lengte_systeem);

    let jij = engine.transcribeer_spoor(
        &microfoon,
        filter,
        &mut |stand: Voortgang| {
            voortgang(Voortgang {
                klaar: stand.klaar,
                totaal,
                stap: Stap::Uitschrijven,
            });
        },
        annuleer,
    );
    let jij = doorgaan_na(jij)?;
    let anderen = if met_systeem {
        let anderen = engine.transcribeer_spoor(
            &systeem,
            filter,
            &mut |stand: Voortgang| {
                voortgang(Voortgang {
                    klaar: lengte_microfoon + stand.klaar,
                    totaal,
                    stap: Stap::Uitschrijven,
                });
            },
            annuleer,
        );
        Some(doorgaan_na(anderen)?)
    } else {
        None
    };
    // Geen enkel spoor gelukt: dan is er geen gesprek, en de fout van de microfoon zegt het meest.
    let (jij, anderen) = match (jij, anderen) {
        (Err(fout), None | Some(Err(_))) => return Err(fout),
        uitslag => uitslag,
    };

    let (jij_stand, anderen_stand, mut meldingen) =
        standen(&afsluiting, &jij, anderen.as_ref(), ruimte);
    // Welk spoor in sprekers gesplitst wordt: online "Anderen", in de ruimte de microfoon. Een
    // spoor zonder tekst hoeft het sprekermodel niet te horen.
    let sprekerspoor = match soort {
        _ if !sprekers => None,
        Vergadersoort::Online if !segmenten(anderen.as_ref()).is_empty() => Some(&systeem),
        Vergadersoort::Ruimte if !segmenten(Some(&jij)).is_empty() => Some(&microfoon),
        _ => None,
    };
    let gevonden = match sprekerspoor {
        Some(map) => herken_sprekers(engine, map, voortgang, annuleer, &mut meldingen)?,
        None => None,
    };
    if ruimte {
        // Geen tweede spoor: geen echo, en er ontbrak niets.
        return Ok(Gesprek {
            beurten: beurten_uit(
                bron,
                true,
                (segmenten(Some(&jij)), &[]),
                gevonden.as_deref(),
            ),
            weggelaten: 0,
            duur: Duration::from_millis(afsluiting.duur_ms),
            meldingen,
            jij: jij_stand,
            anderen: Spoorstand::Ontbrak,
            soort,
        });
    }
    if anderen_stand == Spoorstand::Ontbrak {
        meldingen.push(MELDING_ZONDER_SYSTEEM.to_string());
    }
    meldingen.extend(afsluiting.melding_stil_systeem());

    let (jij_segmenten, weggelaten) = zonder_echo(
        segmenten(Some(&jij)),
        segmenten(anderen.as_ref()),
        ontdubbelen,
        &mut meldingen,
    );

    Ok(Gesprek {
        beurten: beurten_uit(
            bron,
            false,
            (&jij_segmenten, segmenten(anderen.as_ref())),
            gevonden.as_deref(),
        ),
        soort,
        weggelaten,
        duur: Duration::from_millis(afsluiting.duur_ms),
        meldingen,
        jij: jij_stand,
        anderen: anderen_stand,
    })
}

/// De beurten uit de uitingen: bij opnieuw uitschrijven met de sprekers van het oude verslag,
/// anders met de sprekers die het sprekermodel vond, als alinea's (`ruimte`, ook bij Alleen ik)
/// of als online beurten.
fn beurten_uit(
    bron: Sprekerbron<'_>,
    ruimte: bool,
    (jij, anderen): (&[Segment], &[Segment]),
    gevonden: Option<&[SprekerSegment]>,
) -> Vec<Beurt> {
    match bron {
        Sprekerbron::Verslag(oud) => beurten_opnieuw(oud, jij, anderen),
        Sprekerbron::Model(_) if ruimte => ruimte_beurten(jij, gevonden),
        Sprekerbron::Model(_) => online_beurten(jij, anderen, gevonden),
    }
}

/// De beurten van een online vergadering, met de sprekers van "Anderen" als die gevonden zijn.
fn online_beurten(
    jij: &[Segment],
    anderen: &[Segment],
    gevonden: Option<&[SprekerSegment]>,
) -> Vec<Beurt> {
    let Some(gevonden) = gevonden else {
        return beurten(jij, anderen);
    };
    let mut delen = koppel(anderen, gevonden);
    zonder_nummers_bij_een_spreker(&mut delen);
    beurten_met_sprekers(jij, &delen)
}

/// De alinea's van een vergadering in de ruimte, met sprekers als die gevonden zijn.
fn ruimte_beurten(jij: &[Segment], gevonden: Option<&[SprekerSegment]>) -> Vec<Beurt> {
    let Some(gevonden) = gevonden else {
        return alineas(jij);
    };
    let mut delen = koppel(jij, gevonden);
    zonder_nummers_bij_een_spreker(&mut delen);
    // In de ruimte is iedereen een spreker; "geen spreker" heeft hier geen naam (online heet
    // dat "Anderen"). Daarom de dichtstbijzijnde spreker.
    vul_aan_met_dichtstbijzijnde(&mut delen);
    alineas_met_sprekers(&delen)
}

/// De beurten van een opnieuw uitgeschreven gesprek (ADR 0023, punt 1): de nieuwe uitingen
/// `jij` en `anderen` (zoals het spraakmodel ze gaf, na het weglaten van echo), met wie wanneer
/// sprak uit het huidige verslag `oud`. Pure functie, zonder werkproces te testen.
///
/// - De beurten van `oud` met een sprekernummer worden tijdvakken per spreker
///   ([`SprekerSegment`]) op de kant van de sprekers (online "Anderen", in de ruimte de
///   microfoon), en de nieuwe uitingen gaan door dezelfde [`koppel`] als na het sprekermodel.
///   Een uiting die over twee oude sprekers loopt, wordt zo op de woorden gesplitst. Ook de
///   verbeteringen van de gebruiker aan de sprekers gaan zo mee. Een verslag zonder sprekers
///   blijft zonder.
/// - Een uiting zonder oude spreker in de buurt krijgt die van het dichtstbijzijnde oude
///   tijdvak, online en in de ruimte. Nummers worden nooit weggehaald: ook een
///   enkele spreker blijft staan zoals de gebruiker hem had.
/// - Namen: elke nieuwe uiting krijgt de naam van de oude beurt van dezelfde kant en spreker
///   waarmee hij het meest overlapt in tijd. Overlapt er geen, dan de naam die die spreker
///   het vaakst heeft. Zo gaan namen per spreker en per beurt allebei mee.
/// - Daarna gaan opvolgende uitingen samen tot beurten, met de gewone regel.
#[must_use]
pub fn beurten_opnieuw(oud: &Gesprek, jij: &[Segment], anderen: &[Segment]) -> Vec<Beurt> {
    let mut uitingen = if oud.soort.met_systeem() {
        let vakken = tijdvakken(&oud.beurten, Kant::Anderen);
        let anderen = if vakken.is_empty() {
            zonder_sprekers(anderen)
        } else {
            // Een uiting in een gat (verder dan [`sprekers::MAX_AFSTAND`] van elke oude beurt)
            // krijgt de dichtstbijzijnde spreker, en daarmee zijn naam: het oude verslag had
            // sprekers, dus "Anderen" zonder nummer zou er ineens een nieuwe spreker lijken.
            let mut delen = koppel(anderen, &vakken);
            vul_aan_uit_vakken(&mut delen, &vakken);
            delen
        };
        los_op_volgorde(&zonder_sprekers(jij), &anderen, MICROFOON_VERTRAGING)
    } else {
        let vakken = tijdvakken(&oud.beurten, Kant::Jij);
        let jij = if vakken.is_empty() {
            zonder_sprekers(jij)
        } else {
            let mut delen = koppel(jij, &vakken);
            vul_aan_uit_vakken(&mut delen, &vakken);
            delen
        };
        los_op_volgorde(&jij, &[], Duration::ZERO)
    };
    neem_namen_over(&oud.beurten, &mut uitingen);
    voeg_samen(uitingen)
}

/// Geeft elk deel zonder spreker de spreker van het dichtstbijzijnde oude tijdvak, hoe ver ook
/// (bij gelijke afstand het eerste). Zo valt een uiting in een gat nooit buiten de sprekers.
fn vul_aan_uit_vakken(delen: &mut [Toegewezen], vakken: &[SprekerSegment]) {
    for deel in delen.iter_mut().filter(|d| d.spreker.is_none()) {
        let (start, eind) = (deel.segment.start_ms, deel.segment.eind_ms);
        deel.spreker = vakken
            .iter()
            .min_by_key(|v| {
                let afstand = v
                    .start_ms
                    .saturating_sub(eind)
                    .max(start.saturating_sub(v.eind_ms));
                (afstand, v.start_ms)
            })
            .map(|v| v.spreker);
    }
}

/// De beurten van `kant` met een sprekernummer als tijdvakken per spreker.
fn tijdvakken(beurten: &[Beurt], kant: Kant) -> Vec<SprekerSegment> {
    beurten
        .iter()
        .filter(|b| b.kant == kant)
        .filter_map(|b| {
            b.sprekernummer.map(|spreker| SprekerSegment {
                spreker,
                start_ms: b.start_ms,
                eind_ms: b.eind_ms,
            })
        })
        .collect()
}

/// De overlap in tijd van twee beurten, in milliseconden.
fn overlap_ms(a: &Beurt, b: &Beurt) -> u64 {
    a.eind_ms
        .min(b.eind_ms)
        .saturating_sub(a.start_ms.max(b.start_ms))
}

/// Geeft elke nieuwe uiting de naam van de oude beurt van dezelfde kant en spreker met de meeste
/// overlap; zonder overlap de naam die die spreker het vaakst had (zie [`beurten_opnieuw`]).
fn neem_namen_over(oud: &[Beurt], nieuw: &mut [Beurt]) {
    for uiting in nieuw {
        let zelfde = |b: &&Beurt| b.kant == uiting.kant && b.sprekernummer == uiting.sprekernummer;
        let mut beste: Option<(u64, &Beurt)> = None;
        for b in oud.iter().filter(zelfde) {
            let gedeeld = overlap_ms(b, uiting);
            // Bij gelijke overlap de eerste oude beurt, zodat de uitkomst vastligt.
            if gedeeld > 0 && beste.is_none_or(|(meeste, _)| gedeeld > meeste) {
                beste = Some((gedeeld, b));
            }
        }
        uiting.naam = match beste {
            Some((_, b)) => b.naam.clone(),
            None => meeste_naam(oud.iter().filter(zelfde)),
        };
    }
}

/// De naam die het vaakst voorkomt bij deze beurten ("geen naam" telt mee); bij gelijke stand de
/// naam die het eerst voorkomt.
fn meeste_naam<'a>(beurten: impl Iterator<Item = &'a Beurt>) -> Option<String> {
    let mut tellingen: Vec<(Option<&str>, usize)> = Vec::new();
    for b in beurten {
        let naam = b.naam.as_deref();
        match tellingen.iter_mut().find(|(n, _)| *n == naam) {
            Some((_, aantal)) => *aantal += 1,
            None => tellingen.push((naam, 1)),
        }
    }
    let meeste = tellingen.iter().map(|(_, aantal)| *aantal).max()?;
    tellingen
        .into_iter()
        .find(|(_, aantal)| *aantal == meeste)
        .and_then(|(naam, _)| naam.map(str::to_string))
}

/// Welke beurten van het nieuwe gesprek veranderd zijn ten opzichte van het oude (ADR 0023,
/// punt 5), op volgorde van `nieuw`. Per kant worden de oude en nieuwe beurten die elkaar in tijd
/// (ook via elkaar) overlappen een tijdvak. In elk tijdvak gaan de woorden van de oude beurten
/// achter elkaar, en die van de nieuwe ook, zonder verschil in hoofdletters en witruimte. Zijn die
/// gelijk, dan is geen nieuwe beurt in dat tijdvak veranderd, ook niet als de woorden anders over
/// de beurten verdeeld zijn; anders zijn ze allemaal veranderd. Een weggevallen woord telt dus,
/// en "dag" is iets anders dan "goedemiddag". Een nieuwe beurt waar eerst niets stond, is
/// veranderd. Eén keer sorteren en dan één keer door de tijd: niet kwadratisch.
#[must_use]
pub fn veranderde_beurten(oud: &[Beurt], nieuw: &[Beurt]) -> Vec<bool> {
    let mut veranderd = vec![true; nieuw.len()];
    for kant in [Kant::Jij, Kant::Anderen] {
        // (begin, eind, nieuw?, plek), op volgorde van begin.
        let mut vakken: Vec<(u64, u64, bool, usize)> = oud
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kant == kant)
            .map(|(i, b)| (b.start_ms, b.eind_ms, false, i))
            .chain(
                nieuw
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.kant == kant)
                    .map(|(i, b)| (b.start_ms, b.eind_ms, true, i)),
            )
            .collect();
        vakken.sort_unstable();
        let mut groep: Vec<(bool, usize)> = Vec::new();
        let mut groep_eind = 0;
        for (start, eind, is_nieuw, plek) in vakken {
            if !groep.is_empty() && start >= groep_eind {
                beoordeel(oud, nieuw, &groep, &mut veranderd);
                groep.clear();
            }
            groep_eind = if groep.is_empty() {
                eind
            } else {
                groep_eind.max(eind)
            };
            groep.push((is_nieuw, plek));
        }
        beoordeel(oud, nieuw, &groep, &mut veranderd);
    }
    veranderd
}

/// Zet de nieuwe beurten van een tijdvak op onveranderd als hun woorden samen gelijk zijn aan
/// die van de oude beurten in dat tijdvak (zie [`veranderde_beurten`]).
fn beoordeel(oud: &[Beurt], nieuw: &[Beurt], groep: &[(bool, usize)], veranderd: &mut [bool]) {
    let woorden = |is_nieuw: bool| -> Vec<String> {
        let mut plekken: Vec<usize> = groep
            .iter()
            .filter(|(n, _)| *n == is_nieuw)
            .map(|(_, i)| *i)
            .collect();
        // Op volgorde in het gesprek, niet op begin: zo staan de woorden zoals ze gelezen worden.
        plekken.sort_unstable();
        let bron = if is_nieuw { nieuw } else { oud };
        plekken
            .iter()
            .flat_map(|&i| bron[i].tekst.split_whitespace())
            .map(str::to_lowercase)
            .collect()
    };
    let oude = woorden(false);
    if oude.is_empty() {
        return;
    }
    if oude == woorden(true) {
        for (_, i) in groep.iter().filter(|(n, _)| *n) {
            veranderd[*i] = false;
        }
    }
}

/// Herkent de sprekers op een spoor als het sprekermodel klaarstaat. `Ok(None)` zonder
/// sprekermodel, of als het mislukte (dan staat [`MELDING_SPREKERS_MISLUKT`] met de fout in
/// `meldingen`). Alleen een annulering is een fout: dan is er geen gesprek.
///
/// De voortgang krijgt [`Stap::Sprekers`], zodat het scherm "Sprekers herkennen" kan tonen: het
/// percentage is dat van het lezen van het spoor; het rekenen daarna (op de processor zo'n
/// 2,5 minuut per uur) meldt zich niet, en de stap blijft dan staan.
fn herken_sprekers(
    engine: &mut LokaleEngine,
    map: &Path,
    voortgang: &mut dyn FnMut(Voortgang),
    annuleer: &AtomicBool,
    meldingen: &mut Vec<String>,
) -> Result<Option<Vec<SprekerSegment>>, AanbiederFout> {
    herken_met_stap(engine, voortgang, meldingen, |engine, voortgang| {
        engine.herken_sprekers(map, voortgang, annuleer)
    })
}

/// De gedeelde kant van [`herken_sprekers`] en [`voeg_sprekers_toe`]: de voortgang krijgt
/// [`Stap::Sprekers`] en een mislukking wordt een melding in plaats van een fout.
fn herken_met_stap(
    engine: &mut LokaleEngine,
    voortgang: &mut dyn FnMut(Voortgang),
    meldingen: &mut Vec<String>,
    herken: impl FnOnce(
        &mut LokaleEngine,
        &mut dyn FnMut(Voortgang),
    ) -> Result<Option<Vec<SprekerSegment>>, AanbiederFout>,
) -> Result<Option<Vec<SprekerSegment>>, AanbiederFout> {
    if !engine.heeft_sprekermodel() {
        return Ok(None);
    }
    // Meteen melden, ook voordat het werkproces het spoor leest.
    voortgang(Voortgang {
        klaar: Duration::ZERO,
        totaal: Some(Duration::from_secs(1)),
        stap: Stap::Sprekers,
    });
    let mut met_stap = |stand: Voortgang| {
        voortgang(Voortgang {
            stap: Stap::Sprekers,
            ..stand
        });
    };
    match herken(engine, &mut met_stap) {
        Ok(gevonden) => Ok(gevonden),
        Err(AanbiederFout::Engine(EngineFout::Geannuleerd)) => {
            Err(AanbiederFout::Engine(EngineFout::Geannuleerd))
        }
        Err(fout) => {
            meldingen.push(format!("{MELDING_SPREKERS_MISLUKT} ({fout})"));
            Ok(None)
        }
    }
}

/// Herkent de sprekers in een uitgeschreven bestand en zet ze in `uitgeschreven.sprekers` (4b-D,
/// fase D-2). Doet niets zonder sprekermodel (zie [`LokaleEngine::heeft_sprekermodel`]) of
/// zonder tekst. Elke uiting krijgt de spreker met de meeste overlap ([`koppel`]); wisselt de
/// spreker midden in een uiting, dan wordt die gesplitst. Is er maar één spreker, dan blijft het
/// transcript zoals het was ([`zonder_nummers_bij_een_spreker`]); een uiting zonder spreker in de
/// buurt krijgt de dichtstbijzijnde ([`vul_aan_met_dichtstbijzijnde`]), want in een bestand met
/// sprekers heeft "geen spreker" geen naam.
///
/// Mislukt het herkennen, dan staat de reden in `uitgeschreven.sprekerfout` en blijft het
/// transcript zoals het was. Alleen een annulering is een fout.
///
/// # Errors
///
/// Geeft [`EngineFout::Geannuleerd`] na een annulering.
pub fn voeg_sprekers_toe(
    engine: &mut LokaleEngine,
    bestand: &Path,
    uitgeschreven: &mut Uitgeschreven,
    voortgang: &mut dyn FnMut(Voortgang),
    annuleer: &AtomicBool,
) -> Result<(), AanbiederFout> {
    let segmenten = &uitgeschreven.gefilterd.transcript.segmenten;
    if segmenten.iter().all(|s| s.tekst.trim().is_empty()) {
        return Ok(());
    }
    let mut meldingen = Vec::new();
    let gevonden = herken_met_stap(engine, voortgang, &mut meldingen, |engine, voortgang| {
        engine.herken_sprekers_bestand(bestand, voortgang, annuleer)
    })?;
    uitgeschreven.sprekerfout = meldingen.into_iter().next();
    if let Some(gevonden) = gevonden {
        let mut delen = koppel(segmenten, &gevonden);
        zonder_nummers_bij_een_spreker(&mut delen);
        if delen.iter().any(|d| d.spreker.is_some()) {
            vul_aan_met_dichtstbijzijnde(&mut delen);
            uitgeschreven.sprekers = Some(delen);
        }
    }
    Ok(())
}

/// Wat er met elk spoor was, en de meldingen daarover (mislukt, uitgevallen, hersteld).
fn standen(
    afsluiting: &spoor::Afsluiting,
    jij: &Result<Uitgeschreven, AanbiederFout>,
    anderen: Option<&Result<Uitgeschreven, AanbiederFout>>,
    ruimte: bool,
) -> (Spoorstand, Spoorstand, Vec<String>) {
    let stand = |onderbroken: bool| {
        if onderbroken {
            Spoorstand::Uitgevallen
        } else {
            Spoorstand::Opgenomen
        }
    };
    let mut meldingen = Vec::new();
    let jij_stand = if let Err(fout) = jij {
        meldingen.push(format!("{MELDING_MICROFOON_MISLUKT} ({fout})"));
        Spoorstand::Mislukt
    } else {
        stand(afsluiting.microfoon.onderbroken)
    };
    let anderen_stand = match (&afsluiting.systeem, anderen) {
        (_, Some(Err(fout))) => {
            meldingen.push(format!("{MELDING_SYSTEEM_MISLUKT} ({fout})"));
            Spoorstand::Mislukt
        }
        (Some(s), Some(Ok(_))) => stand(s.onderbroken),
        _ => Spoorstand::Ontbrak,
    };
    if afsluiting.hersteld {
        // Na een herstel staan alle sporen als onderbroken; één melding zegt genoeg.
        meldingen.push(MELDING_HERSTELD.to_string());
    } else {
        if jij_stand == Spoorstand::Uitgevallen {
            meldingen.push(if ruimte {
                MELDING_MICROFOON_UITGEVALLEN_RUIMTE.to_string()
            } else {
                MELDING_MICROFOON_UITGEVALLEN.to_string()
            });
        }
        if anderen_stand == Spoorstand::Uitgevallen {
            meldingen.push(MELDING_SYSTEEM_UITGEVALLEN.to_string());
        }
    }
    (jij_stand, anderen_stand, meldingen)
}

/// Vanaf dit aandeel weggelaten uitingen van "Jij" met woorden (procent) komt de melding over
/// echo. Ook met een koptelefoon kan er een uiting wegvallen (dubbele tekst die beide kanten
/// zeggen); dan is het advies om een koptelefoon te gebruiken verwarrend. Op AMI `ES2002a`:
/// koptelefoon ongeveer 1 procent, luidspreker ongeveer 38 procent.
pub const ECHO_MELDING_VANAF: usize = 20;

/// Zijn er zoveel uitingen weggelaten dat de gebruiker echo heeft (zie [`ECHO_MELDING_VANAF`])?
fn veel_echo(weggelaten: usize, uitingen: usize) -> bool {
    weggelaten > 0 && weggelaten * 100 >= uitingen * ECHO_MELDING_VANAF
}

/// Laat de echo uit "Jij" weg (als `ontdubbelen` er is) en zet de melding erbij als dat veel
/// was. Geeft de overgebleven uitingen en hoeveel er wegvielen.
fn zonder_echo(
    jij: &[Segment],
    anderen: &[Segment],
    ontdubbelen: Option<&Ontdubbelen>,
    meldingen: &mut Vec<String>,
) -> (Vec<Segment>, usize) {
    let Some(regel) = ontdubbelen else {
        return (jij.to_vec(), 0);
    };
    let uit = ontdubbel(jij, anderen, regel);
    if veel_echo(uit.weggelaten, uit.met_woorden) {
        meldingen.push(MELDING_ECHO_WEGGELATEN.to_string());
    }
    (uit.over, uit.weggelaten)
}

/// De segmenten van een spoor dat lukte; leeg als het ontbrak of mislukte.
fn segmenten(uit: Option<&Result<Uitgeschreven, AanbiederFout>>) -> &[Segment] {
    match uit {
        Some(Ok(uit)) => &uit.gefilterd.transcript.segmenten,
        _ => &[],
    }
}

/// Een annulering breekt het gesprek af; elke andere fout van een spoor gaat mee in de
/// uitslag, zodat het andere spoor er nog in kan.
fn doorgaan_na(
    uitslag: Result<Uitgeschreven, AanbiederFout>,
) -> Result<Result<Uitgeschreven, AanbiederFout>, AanbiederFout> {
    match uitslag {
        Err(AanbiederFout::Engine(EngineFout::Geannuleerd)) => {
            Err(AanbiederFout::Engine(EngineFout::Geannuleerd))
        }
        uitslag => Ok(uitslag),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn de_naam_van_een_verslag_is_schoon_en_begrensd() {
        assert_eq!(
            schone_verslagnaam("  Overleg \t speelveld\n ").as_deref(),
            Some("Overleg speelveld")
        );
        assert_eq!(schone_verslagnaam("   \u{7}  "), None);
        assert_eq!(schone_verslagnaam(""), None);
        let lang = "a".repeat(250);
        let naam = schone_verslagnaam(&lang).unwrap();
        assert_eq!(naam.chars().count(), MAX_VERSLAGNAAM);
        // Een spreker houdt de kortere grens.
        assert_eq!(schone_naam(&lang).unwrap().chars().count(), MAX_NAAM);
    }

    fn seg(start_ms: u64, eind_ms: u64, tekst: &str) -> Segment {
        Segment::nieuw(start_ms, eind_ms, tekst)
    }

    fn kort(beurten: &[Beurt]) -> Vec<(Kant, u64, u64, &str)> {
        beurten
            .iter()
            .map(|b| (b.kant, b.start_ms, b.eind_ms, b.tekst.as_str()))
            .collect()
    }

    #[test]
    fn de_microfoon_schuift_honderd_milliseconden_naar_voren() {
        let b = beurten(&[seg(1100, 2100, "hallo"), seg(50, 80, "kort")], &[]);
        assert_eq!(
            kort(&b),
            vec![(Kant::Jij, 0, 2000, "kort hallo")],
            "niet onder nul, en de pauze van 1 s houdt ze samen"
        );
        // Het systeemspoor schuift niet.
        let b = beurten(&[], &[seg(1100, 2100, "hoi")]);
        assert_eq!(kort(&b), vec![(Kant::Anderen, 1100, 2100, "hoi")]);
    }

    #[test]
    fn beurten_staan_op_volgorde_van_begin_en_bij_gelijk_begin_eerst_anderen() {
        let b = beurten(
            &[seg(5100, 6000, "jij twee"), seg(1100, 2000, "jij een")],
            &[
                seg(1000, 1500, "anderen een"),
                seg(3000, 4000, "anderen twee"),
            ],
        );
        assert_eq!(
            kort(&b),
            vec![
                (Kant::Anderen, 1000, 1500, "anderen een"),
                (Kant::Jij, 1000, 1900, "jij een"),
                (Kant::Anderen, 3000, 4000, "anderen twee"),
                (Kant::Jij, 5000, 5900, "jij twee"),
            ]
        );
    }

    #[test]
    fn een_pauze_van_drie_seconden_houdt_de_beurt_bij_elkaar() {
        let b = beurten(
            &[],
            &[
                seg(0, 1000, "een"),
                // Precies 3 s pauze: zelfde beurt.
                seg(4000, 5000, "twee"),
                // 3,001 s pauze: nieuwe beurt.
                seg(8001, 9000, "drie"),
                // Overlap telt als geen pauze.
                seg(8500, 9500, "vier"),
            ],
        );
        assert_eq!(
            kort(&b),
            vec![
                (Kant::Anderen, 0, 5000, "een twee"),
                (Kant::Anderen, 8001, 9500, "drie vier"),
            ]
        );
    }

    #[test]
    fn een_beurt_van_de_ander_ertussen_breekt_de_beurt() {
        let b = beurten(
            &[seg(100, 1100, "ja"), seg(2100, 3100, "precies")],
            &[seg(1500, 1800, "toch?")],
        );
        assert_eq!(
            kort(&b),
            vec![
                (Kant::Jij, 0, 1000, "ja"),
                (Kant::Anderen, 1500, 1800, "toch?"),
                (Kant::Jij, 2000, 3000, "precies"),
            ]
        );
    }

    #[test]
    fn de_echomelding_komt_pas_bij_veel_weggelaten_uitingen() {
        assert!(!veel_echo(0, 0));
        assert!(!veel_echo(0, 100));
        assert!(!veel_echo(12, 131));
        assert!(veel_echo(20, 100));
        assert!(veel_echo(94, 202));
        assert!(veel_echo(1, 1));
    }

    #[test]
    fn uitingen_zonder_tekst_tellen_niet_mee() {
        let b = beurten(
            &[seg(100, 1100, "een"), seg(1200, 1300, "  ")],
            &[seg(1150, 1250, "")],
        );
        assert_eq!(kort(&b), vec![(Kant::Jij, 0, 1000, "een")]);
        assert!(beurten(&[], &[]).is_empty());
        assert_eq!(Kant::Jij.naam(), "Jij");
        assert_eq!(Kant::Anderen.naam(), "Anderen");
    }

    #[test]
    fn in_de_ruimte_worden_het_alineas_zonder_verschuiving() {
        let b = alineas(&[
            seg(0, 1000, "een"),
            seg(3500, 4000, "twee"),
            seg(8000, 9000, "drie"),
        ]);
        assert_eq!(
            kort(&b),
            vec![
                (Kant::Jij, 0, 4000, "een twee"),
                (Kant::Jij, 8000, 9000, "drie"),
            ]
        );
    }

    fn gesprek(soort: Vergadersoort) -> Gesprek {
        Gesprek {
            beurten: beurten(
                &[seg(100, 1100, "hallo"), seg(10_100, 11_100, "ja")],
                &[
                    seg(5000, 6000, "goedemiddag"),
                    seg(15_000, 16_000, "tot ziens"),
                    seg(20_000, 21_000, "nog iets"),
                ],
            ),
            duur: Duration::from_secs(21),
            meldingen: Vec::new(),
            jij: Spoorstand::Opgenomen,
            anderen: Spoorstand::Opgenomen,
            weggelaten: 0,
            soort,
        }
    }

    fn sprekers(g: &Gesprek) -> Vec<String> {
        g.beurten.iter().map(Beurt::spreker).collect()
    }

    #[test]
    fn een_beurt_bewerken_vervangt_de_tekst_en_laat_de_rest_staan() {
        let mut g = gesprek(Vergadersoort::Online);
        let voor = g.beurten[1].clone();
        assert!(g.zet_tekst(1, "  Goedemiddag allemaal.  ").unwrap());
        assert_eq!(g.beurten[1].tekst, "Goedemiddag allemaal.");
        assert_eq!(
            (
                g.beurten[1].start_ms,
                g.beurten[1].eind_ms,
                g.beurten[1].kant
            ),
            (voor.start_ms, voor.eind_ms, voor.kant),
            "tijden en kant blijven"
        );
        // Dezelfde tekst nog eens, en een lege tekst: niets verandert.
        assert!(!g.zet_tekst(1, "Goedemiddag allemaal.").unwrap());
        assert!(
            !g.zet_tekst(
                1, " 
 "
            )
            .unwrap()
        );
        assert_eq!(g.beurten[1].tekst, "Goedemiddag allemaal.");
        assert!(g.zet_tekst(99, "x").is_err());
    }

    #[test]
    fn een_bewerkte_beurt_staat_in_het_gekopieerde_verslag() {
        let mut g = gesprek(Vergadersoort::Online);
        g.zet_tekst(0, "Hallo, goedemorgen.").unwrap();
        assert!(g.als_tekst().starts_with(
            "Jij: Hallo, goedemorgen.

"
        ));
    }

    #[test]
    fn een_bewerking_blijft_bij_een_andere_spreker_of_samenvoegen() {
        let mut g = met_sprekers();
        g.zet_tekst(1, "Goedemiddag, dames en heren.").unwrap();
        g.voeg_sprekers_samen(2, 1).unwrap();
        assert!(
            g.beurten
                .iter()
                .any(|b| b.tekst.starts_with("Goedemiddag, dames en heren."))
        );
    }

    #[test]
    fn een_naam_voor_alleen_deze_beurt() {
        let mut g = gesprek(Vergadersoort::Online);
        assert_eq!(
            sprekers(&g),
            vec!["Jij", "Anderen", "Jij", "Anderen", "Anderen"]
        );
        let n = g
            .geef_naam(1, "  Wethouder\tDe   Vries ", Naambereik::Beurt)
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            sprekers(&g),
            vec!["Jij", "Wethouder De Vries", "Jij", "Anderen", "Anderen"]
        );
    }

    #[test]
    fn een_naam_voor_de_hele_kant_laat_een_aparte_naam_staan() {
        let mut g = gesprek(Vergadersoort::Online);
        g.geef_naam(1, "Wethouder De Vries", Naambereik::Beurt)
            .unwrap();
        // Alle beurten van Anderen die nog "Anderen" heten.
        let n = g.geef_naam(3, "Raad", Naambereik::Kant).unwrap();
        assert_eq!(n, 2);
        assert_eq!(
            sprekers(&g),
            vec!["Jij", "Wethouder De Vries", "Jij", "Raad", "Raad"]
        );
        // Hybride: Jij wordt de zaal, voor alle beurten van Jij.
        assert_eq!(g.geef_naam(0, "Zaal", Naambereik::Kant).unwrap(), 2);
        assert_eq!(
            sprekers(&g),
            vec!["Zaal", "Wethouder De Vries", "Zaal", "Raad", "Raad"]
        );
        // Een andere kant wordt nooit geraakt, ook niet bij dezelfde naam.
        assert_eq!(g.geef_naam(0, "Raad", Naambereik::Kant).unwrap(), 2);
        assert_eq!(g.beurten[0].kant, Kant::Jij);
        assert_eq!(g.geef_naam(3, "Anderen", Naambereik::Kant).unwrap(), 2);
        assert_eq!(
            sprekers(&g),
            vec!["Raad", "Wethouder De Vries", "Raad", "Anderen", "Anderen"]
        );
    }

    #[test]
    fn een_lege_naam_of_de_naam_van_de_kant_zet_terug() {
        let mut g = gesprek(Vergadersoort::Online);
        g.geef_naam(1, "Raad", Naambereik::Kant).unwrap();
        assert_eq!(g.geef_naam(1, "   ", Naambereik::Kant).unwrap(), 3);
        assert!(g.beurten.iter().all(|b| b.naam.is_none()));
        g.geef_naam(0, "Zaal", Naambereik::Beurt).unwrap();
        assert_eq!(g.geef_naam(0, "Jij", Naambereik::Beurt).unwrap(), 1);
        assert_eq!(g.beurten[0].naam, None);
        // Niets veranderd: nul.
        assert_eq!(g.geef_naam(0, "", Naambereik::Beurt).unwrap(), 0);
    }

    #[test]
    fn een_naam_is_begrensd_en_zonder_stuurtekens() {
        let mut g = gesprek(Vergadersoort::Online);
        let lang = "a".repeat(100);
        g.geef_naam(0, &lang, Naambereik::Beurt).unwrap();
        assert_eq!(g.beurten[0].spreker().chars().count(), MAX_NAAM);
        g.geef_naam(0, "Piet\u{7}\nJansen", Naambereik::Beurt)
            .unwrap();
        assert_eq!(g.beurten[0].spreker(), "Piet Jansen");
    }

    #[test]
    fn in_de_ruimte_of_een_onbekende_beurt_geeft_een_melding() {
        let mut g = gesprek(Vergadersoort::Ruimte);
        assert!(g.geef_naam(0, "Piet", Naambereik::Beurt).is_err());
        let mut g = gesprek(Vergadersoort::Alleen);
        assert!(g.geef_naam(0, "Piet", Naambereik::Beurt).is_err());
        let mut g = gesprek(Vergadersoort::Online);
        assert!(g.geef_naam(99, "Piet", Naambereik::Beurt).is_err());
    }

    #[test]
    fn het_verslag_als_tekst_met_en_zonder_sprekers() {
        let mut g = gesprek(Vergadersoort::Online);
        g.geef_naam(1, "Wethouder", Naambereik::Beurt).unwrap();
        assert!(
            g.als_tekst()
                .starts_with("Jij: hallo\n\nWethouder: goedemiddag\n\nJij: ja")
        );
        let g = gesprek(Vergadersoort::Ruimte);
        assert!(g.als_tekst().starts_with("hallo\n\ngoedemiddag"));
        let g = gesprek(Vergadersoort::Alleen);
        assert!(g.als_tekst().starts_with("hallo\n\ngoedemiddag"));
    }

    // ---------- sprekers (4b-D) ----------

    fn deel(spreker: Option<u8>, start_ms: u64, eind_ms: u64, tekst: &str) -> Toegewezen {
        Toegewezen {
            segment: seg(start_ms, eind_ms, tekst),
            spreker,
        }
    }

    /// Een online gesprek waarin "Anderen" drie sprekers heeft:
    /// 0 Jij, 1 Spreker 1, 2 Spreker 2, 3 Jij, 4 Spreker 1, 5 Spreker 3, 6 Spreker 1.
    fn met_sprekers() -> Gesprek {
        Gesprek {
            beurten: beurten_met_sprekers(
                &[seg(100, 1100, "hallo"), seg(10_100, 11_100, "ja")],
                &[
                    deel(Some(1), 2000, 3000, "goedemiddag"),
                    deel(Some(2), 3200, 4000, "dag"),
                    deel(Some(1), 12_000, 13_000, "dus"),
                    deel(Some(3), 13_100, 14_000, "nee"),
                    deel(Some(1), 14_200, 15_000, "toch wel"),
                ],
            ),
            duur: Duration::from_secs(15),
            meldingen: Vec::new(),
            jij: Spoorstand::Opgenomen,
            anderen: Spoorstand::Opgenomen,
            weggelaten: 0,
            soort: Vergadersoort::Online,
        }
    }

    #[test]
    fn een_beurt_gaat_alleen_door_bij_dezelfde_spreker() {
        let g = met_sprekers();
        assert_eq!(
            sprekers(&g),
            vec![
                "Jij",
                "Spreker 1",
                "Spreker 2",
                "Jij",
                "Spreker 1",
                "Spreker 3",
                "Spreker 1"
            ]
        );
        assert_eq!(g.sprekernummers(), vec![1, 2, 3]);
        // Zonder spreker heet een beurt van Anderen gewoon "Anderen".
        let b = beurten_met_sprekers(&[], &[deel(None, 0, 1000, "wie?")]);
        assert_eq!(b[0].spreker(), "Anderen");
        // Dezelfde spreker binnen de pauze: één beurt.
        let b = beurten_met_sprekers(
            &[],
            &[
                deel(Some(2), 0, 1000, "een"),
                deel(Some(2), 2000, 3000, "twee"),
            ],
        );
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].tekst, "een twee");
        assert_eq!(b[0].sprekernummer, Some(2));
    }

    #[test]
    fn een_naam_per_spreker_geldt_voor_al_zijn_beurten() {
        let mut g = met_sprekers();
        let n = g.geef_naam(4, "Piet", Naambereik::Spreker).unwrap();
        assert_eq!(n, 3);
        assert_eq!(
            sprekers(&g),
            vec![
                "Jij",
                "Piet",
                "Spreker 2",
                "Jij",
                "Piet",
                "Spreker 3",
                "Piet"
            ]
        );
        // Een beurt die apart een naam kreeg, houdt die.
        let mut g = met_sprekers();
        g.geef_naam(6, "Voorzitter", Naambereik::Beurt).unwrap();
        assert_eq!(g.geef_naam(1, "Piet", Naambereik::Spreker).unwrap(), 2);
        assert_eq!(g.beurten[6].spreker(), "Voorzitter");
        // "Spreker 1" intypen zet terug.
        assert_eq!(g.geef_naam(1, "Spreker 1", Naambereik::Spreker).unwrap(), 2);
        assert_eq!(g.beurten[1].naam, None);
        // Naambereik Kant raakt alleen beurten met dezelfde naam, dus niet de andere sprekers.
        let mut g = met_sprekers();
        assert_eq!(g.geef_naam(2, "Raad", Naambereik::Kant).unwrap(), 1);
    }

    #[test]
    fn een_beurt_aan_een_andere_spreker_geven_voegt_hem_bij_zijn_buren() {
        let mut g = met_sprekers();
        g.geef_naam(1, "Piet", Naambereik::Spreker).unwrap();
        // "dag" (Spreker 2) was eigenlijk Piet: hij gaat samen met de beurt ervoor.
        assert_eq!(g.geef_aan_spreker(2, 1).unwrap(), 1);
        assert_eq!(
            sprekers(&g),
            vec!["Jij", "Piet", "Jij", "Piet", "Spreker 3", "Piet"]
        );
        assert_eq!(g.beurten[1].tekst, "goedemiddag dag");
        assert_eq!((g.beurten[1].start_ms, g.beurten[1].eind_ms), (2000, 4000));
        // Nog eens dezelfde spreker: niets.
        assert_eq!(g.geef_aan_spreker(1, 1).unwrap(), 0);
        // Een nieuwe spreker voor wie het model bij een ander indeelde.
        assert_eq!(g.geef_aan_spreker(4, 5).unwrap(), 1);
        assert_eq!(g.beurten[4].spreker(), "Spreker 5");
        assert_eq!(g.beurten[4].naam, None);
        // Jij heeft online geen sprekers; buiten 1 tot 8 kan niet; een onbekende beurt ook niet.
        assert!(g.geef_aan_spreker(0, 2).is_err());
        assert!(g.geef_aan_spreker(1, 0).is_err());
        assert!(g.geef_aan_spreker(1, MAX_SPREKERS + 1).is_err());
        assert!(g.geef_aan_spreker(99, 1).is_err());
    }

    #[test]
    fn een_beurt_naar_een_nieuwe_spreker_of_zonder_spreker_naar_een_spreker() {
        let mut g = met_sprekers();
        // Het eerstvolgende vrije nummer: 1, 2 en 3 zijn bezet.
        assert_eq!(g.geef_aan_nieuwe_spreker(5).unwrap(), 4);
        assert_eq!(g.beurten[5].spreker(), "Spreker 4");
        // Een beurt zonder spreker (de koppeling vond niemand) krijgt er een.
        let mut g = Gesprek {
            beurten: beurten_met_sprekers(
                &[],
                &[
                    deel(Some(1), 0, 1000, "een"),
                    deel(None, 5000, 6000, "wie?"),
                ],
            ),
            ..met_sprekers()
        };
        assert_eq!(g.beurten[1].spreker(), "Anderen");
        assert_eq!(g.geef_aan_spreker(1, 1).unwrap(), 1);
        assert_eq!(g.beurten[1].spreker(), "Spreker 1");
        let mut g = Gesprek {
            beurten: beurten_met_sprekers(&[], &[deel(None, 0, 1000, "wie?")]),
            ..met_sprekers()
        };
        assert_eq!(g.geef_aan_nieuwe_spreker(0).unwrap(), 1);
        // Online Jij kan geen spreker krijgen.
        let mut g = met_sprekers();
        assert!(g.geef_aan_nieuwe_spreker(0).is_err());
    }

    #[test]
    fn in_de_ruimte_zonder_sprekers_worden_de_andere_alineas_spreker_1() {
        let mut g = Gesprek {
            beurten: alineas(&[
                seg(0, 1000, "een"),
                seg(5000, 6000, "twee"),
                seg(10_000, 11_000, "drie"),
            ]),
            ..gesprek(Vergadersoort::Ruimte)
        };
        assert!(!g.met_sprekers());
        assert_eq!(g.geef_aan_nieuwe_spreker(1).unwrap(), 2);
        assert_eq!(sprekers(&g), vec!["Spreker 1", "Spreker 2", "Spreker 1"]);
        assert_eq!(
            g.als_tekst(),
            "Spreker 1: een\n\nSpreker 2: twee\n\nSpreker 1: drie"
        );
        // Online blijven de andere beurten "Anderen": daar heeft dat een naam.
        let mut g = Gesprek {
            beurten: beurten(&[], &[seg(0, 1000, "een"), seg(5000, 6000, "twee")]),
            ..gesprek(Vergadersoort::Online)
        };
        assert_eq!(g.geef_aan_nieuwe_spreker(1).unwrap(), 1);
        assert_eq!(sprekers(&g), vec!["Anderen", "Spreker 1"]);
    }

    #[test]
    fn samenvoegen_houdt_de_naam_van_de_bron_als_het_doel_er_geen_heeft() {
        let mut g = met_sprekers();
        g.geef_naam(2, "Piet", Naambereik::Spreker).unwrap();
        // Spreker 2 ("Piet") in de naamloze Spreker 1: alles heet daarna Piet.
        g.voeg_sprekers_samen(2, 1).unwrap();
        assert_eq!(
            sprekers(&g),
            vec!["Jij", "Piet", "Jij", "Piet", "Spreker 3", "Piet"]
        );
        assert_eq!(g.beurten[1].tekst, "goedemiddag dag");
    }

    #[test]
    fn de_naam_van_een_spreker_is_de_naam_die_het_vaakst_voorkomt() {
        let mut g = met_sprekers();
        // Spreker 1 heet Piet, maar zijn eerste beurt kreeg apart "Voorzitter".
        g.geef_naam(4, "Piet", Naambereik::Spreker).unwrap();
        g.geef_naam(1, "Voorzitter", Naambereik::Beurt).unwrap();
        assert_eq!(
            g.naam_van_spreker(Kant::Anderen, 1).as_deref(),
            Some("Piet")
        );
        // Een beurt die naar Spreker 1 gaat, krijgt "Piet", niet "Voorzitter".
        g.geef_aan_spreker(5, 1).unwrap();
        assert_eq!(g.beurten[4].spreker(), "Piet");
        assert_eq!(g.beurten[4].tekst, "dus nee toch wel");
        // En bij samenvoegen houdt de apart genoemde beurt zijn naam.
        g.voeg_sprekers_samen(1, 2).unwrap();
        assert_eq!(g.beurten[1].spreker(), "Voorzitter");
    }

    #[test]
    fn twee_sprekers_samenvoegen() {
        let mut g = met_sprekers();
        g.geef_naam(1, "Piet", Naambereik::Spreker).unwrap();
        // Spreker 3 is ook Piet: zijn beurt krijgt de naam en gaat samen met de buren.
        assert_eq!(g.voeg_sprekers_samen(3, 1).unwrap(), 1);
        assert_eq!(
            sprekers(&g),
            vec!["Jij", "Piet", "Spreker 2", "Jij", "Piet"]
        );
        assert_eq!(g.beurten[4].tekst, "dus nee toch wel");
        assert_eq!(g.sprekernummers(), vec![1, 2]);
        // Een spreker zonder beurten: niets veranderd.
        assert_eq!(g.voeg_sprekers_samen(4, 1).unwrap(), 0);
        assert!(g.voeg_sprekers_samen(1, 1).is_err());
        assert!(g.voeg_sprekers_samen(0, 1).is_err());
        // Alleen ik heeft geen sprekers.
        let mut alleen = Gesprek {
            soort: Vergadersoort::Alleen,
            ..met_sprekers()
        };
        assert!(alleen.voeg_sprekers_samen(2, 1).is_err());
    }

    #[test]
    fn in_de_ruimte_met_sprekers_zijn_er_wel_sprekers() {
        let mut g = Gesprek {
            beurten: alineas_met_sprekers(&[
                deel(Some(1), 0, 1000, "een"),
                deel(Some(2), 1500, 2000, "twee"),
                deel(Some(2), 2500, 3000, "drie"),
            ]),
            soort: Vergadersoort::Ruimte,
            ..gesprek(Vergadersoort::Ruimte)
        };
        assert!(g.met_sprekers());
        assert_eq!(g.sprekerkant(), Some(Kant::Jij));
        assert_eq!(sprekers(&g), vec!["Spreker 1", "Spreker 2"]);
        assert_eq!(g.als_tekst(), "Spreker 1: een\n\nSpreker 2: twee drie");
        g.geef_naam(1, "Anna", Naambereik::Spreker).unwrap();
        assert_eq!(g.als_tekst(), "Spreker 1: een\n\nAnna: twee drie");
        // Spreker 1 aan Anna geven: alles één alinea, met haar naam.
        assert_eq!(g.geef_aan_spreker(0, 2).unwrap(), 1);
        assert_eq!(g.als_tekst(), "Anna: een twee drie");
        // Zonder nummers blijft het zoals het was: alinea's zonder sprekers.
        let g = gesprek(Vergadersoort::Ruimte);
        assert!(!g.met_sprekers());
    }

    // Opnieuw uitschrijven (ADR 0023)

    fn met_woorden(start_ms: u64, eind_ms: u64, woorden: &[(u64, u64, &str)]) -> Segment {
        Segment {
            woorden: woorden
                .iter()
                .map(|(s, e, t)| crate::engine::protocol::Woord {
                    start_ms: *s,
                    eind_ms: *e,
                    tekst: (*t).to_string(),
                })
                .collect(),
            ..seg(
                start_ms,
                eind_ms,
                &woorden.iter().map(|w| w.2).collect::<Vec<_>>().join(" "),
            )
        }
    }

    fn rij(g: &[Beurt]) -> Vec<(Kant, Option<u8>, String, String)> {
        g.iter()
            .map(|b| (b.kant, b.sprekernummer, b.spreker(), b.tekst.clone()))
            .collect()
    }

    #[test]
    fn opnieuw_houdt_sprekers_namen_en_de_verbeteringen_aan_sprekers() {
        let mut oud = met_sprekers();
        // Namen per spreker, per beurt en per kant, en een verbeterde spreker.
        oud.geef_naam(2, "Piet", Naambereik::Spreker).unwrap();
        oud.geef_naam(1, "Voorzitter", Naambereik::Beurt).unwrap();
        oud.geef_naam(0, "Zaal", Naambereik::Kant).unwrap();
        // "nee" (Spreker 3) was eigenlijk Spreker 1: hij gaat samen met zijn buren.
        let nee = oud.beurten.iter().position(|b| b.tekst == "nee").unwrap();
        oud.geef_aan_spreker(nee, 1).unwrap();
        let nieuw = beurten_opnieuw(
            &oud,
            &[seg(100, 1100, "Hallo!"), seg(10_100, 11_100, "Ja.")],
            &[
                seg(2000, 3000, "Goedemiddag."),
                seg(3200, 4000, "Dag."),
                seg(12_000, 13_000, "Dus"),
                seg(13_100, 14_000, "nee,"),
                seg(14_200, 15_000, "toch wel."),
            ],
        );
        let s = |t: &str| t.to_string();
        assert_eq!(
            rij(&nieuw),
            vec![
                (Kant::Jij, None, s("Zaal"), s("Hallo!")),
                (Kant::Anderen, Some(1), s("Voorzitter"), s("Goedemiddag.")),
                (Kant::Anderen, Some(2), s("Piet"), s("Dag.")),
                (Kant::Jij, None, s("Zaal"), s("Ja.")),
                (
                    Kant::Anderen,
                    Some(1),
                    s("Spreker 1"),
                    s("Dus nee, toch wel.")
                ),
            ]
        );
        assert!(nieuw.iter().all(|b| !b.verbeterd));
    }

    #[test]
    fn opnieuw_splitst_een_uiting_over_twee_oude_sprekers() {
        let oud = met_sprekers();
        // Het nieuwe model maakt van "goedemiddag" en "dag" een uiting, met woordtijden.
        let nieuw = beurten_opnieuw(
            &oud,
            &[],
            &[met_woorden(
                2000,
                4000,
                &[(2000, 2900, "Goedemiddag."), (3200, 3900, "Dag.")],
            )],
        );
        assert_eq!(
            nieuw
                .iter()
                .map(|b| (b.sprekernummer, b.tekst.as_str()))
                .collect::<Vec<_>>(),
            vec![(Some(1), "Goedemiddag."), (Some(2), "Dag.")]
        );
    }

    #[test]
    fn opnieuw_zonder_sprekers_en_in_de_ruimte() {
        // Online zonder sprekermodel: "Anderen" blijft zonder nummers.
        let oud = gesprek(Vergadersoort::Online);
        let nieuw = beurten_opnieuw(&oud, &[], &[seg(5000, 6000, "Goedemiddag.")]);
        assert_eq!(nieuw[0].sprekernummer, None);
        assert_eq!(nieuw[0].spreker(), "Anderen");

        // In de ruimte met twee sprekers: een uiting zonder oude spreker in de buurt krijgt de
        // dichtstbijzijnde, met de naam van die spreker.
        let mut ruimte = Gesprek {
            beurten: alineas_met_sprekers(&[
                deel(Some(1), 0, 1000, "een"),
                deel(Some(2), 1500, 2000, "twee"),
            ]),
            ..gesprek(Vergadersoort::Ruimte)
        };
        ruimte.geef_naam(1, "Anna", Naambereik::Spreker).unwrap();
        let nieuw = beurten_opnieuw(
            &ruimte,
            &[
                seg(0, 1000, "Een."),
                seg(1500, 2000, "Twee."),
                seg(9000, 9500, "Later."),
            ],
            &[],
        );
        assert_eq!(
            nieuw
                .iter()
                .map(|b| (b.sprekernummer, b.spreker(), b.tekst.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (Some(1), "Spreker 1".to_string(), "Een."),
                (Some(2), "Anna".to_string(), "Twee."),
                (Some(2), "Anna".to_string(), "Later."),
            ]
        );

        // Alleen ik: alinea's zonder sprekers.
        let alleen = gesprek(Vergadersoort::Alleen);
        let nieuw = beurten_opnieuw(&alleen, &[seg(0, 900, "Notitie.")], &[]);
        assert_eq!(nieuw[0].sprekernummer, None);
        assert_eq!(nieuw[0].naam, None);
    }

    #[test]
    fn een_verbeterde_regel_wordt_onthouden_en_geteld() {
        let mut g = gesprek(Vergadersoort::Online);
        assert_eq!(g.verbeterde_regels(), 0);
        // Dezelfde tekst is geen verbetering.
        let zelfde = g.beurten[0].tekst.clone();
        assert!(!g.zet_tekst(0, &zelfde).unwrap());
        assert_eq!(g.verbeterde_regels(), 0);
        assert!(g.zet_tekst(0, "Hallo allemaal.").unwrap());
        assert!(g.zet_tekst(2, "Goedemiddag allemaal.").unwrap());
        assert!(g.beurten[0].verbeterd);
        assert_eq!(g.verbeterde_regels(), 2);
        // Samengaan met een buur houdt de verbetering.
        let mut s = met_sprekers();
        s.zet_tekst(5, "Nee.").unwrap();
        s.geef_aan_spreker(5, 1).unwrap();
        assert_eq!(s.verbeterde_regels(), 1);
    }

    #[test]
    fn veranderde_regels_tellen_zonder_hoofdletters_en_witruimte() {
        let oud = met_sprekers();
        let nieuw = beurten_opnieuw(
            &oud,
            &[seg(100, 1100, "HALLO"), seg(10_100, 11_100, "jawel")],
            &[
                seg(2000, 3000, "goedemiddag"),
                seg(3200, 4000, "  dag "),
                seg(12_000, 13_000, "dus"),
                seg(13_100, 14_000, "nee"),
                seg(14_200, 15_000, "toch niet"),
            ],
        );
        let veranderd = veranderde_beurten(&oud.beurten, &nieuw);
        assert_eq!(nieuw.len(), veranderd.len());
        let regels: Vec<(&str, bool)> = nieuw
            .iter()
            .zip(&veranderd)
            .map(|(b, v)| (b.tekst.as_str(), *v))
            .collect();
        assert_eq!(
            regels,
            vec![
                ("HALLO", false),
                ("goedemiddag", false),
                ("dag", false),
                ("jawel", true),
                ("dus", false),
                ("nee", false),
                ("toch niet", true),
            ]
        );
        assert_eq!(veranderd.iter().filter(|v| **v).count(), 2);

        // Een andere splitsing van dezelfde tekst is geen verandering; een uiting waar eerst
        // niets stond wel.
        let een = Beurt {
            kant: Kant::Anderen,
            sprekernummer: None,
            start_ms: 0,
            eind_ms: 4000,
            tekst: "Een twee drie vier".to_string(),
            naam: None,
            verbeterd: false,
        };
        let nieuw = vec![
            Beurt {
                eind_ms: 1500,
                tekst: "een twee".to_string(),
                ..een.clone()
            },
            Beurt {
                start_ms: 2000,
                tekst: "drie vier".to_string(),
                ..een.clone()
            },
            Beurt {
                start_ms: 20_000,
                eind_ms: 21_000,
                tekst: "vijf".to_string(),
                ..een.clone()
            },
        ];
        assert_eq!(veranderde_beurten(&[een], &nieuw), vec![false, false, true]);
    }

    #[test]
    fn het_verwerkte_geluid_telt_de_sporen() {
        let online = gesprek(Vergadersoort::Online);
        assert_eq!(online.verwerkt_geluid(), Duration::from_secs(42));
        let zonder = Gesprek {
            anderen: Spoorstand::Ontbrak,
            ..gesprek(Vergadersoort::Online)
        };
        assert_eq!(zonder.verwerkt_geluid(), Duration::from_secs(21));
        assert_eq!(
            gesprek(Vergadersoort::Ruimte).verwerkt_geluid(),
            Duration::from_secs(21)
        );
    }

    fn anderen(start_ms: u64, eind_ms: u64, tekst: &str) -> Beurt {
        Beurt {
            kant: Kant::Anderen,
            sprekernummer: None,
            start_ms,
            eind_ms,
            tekst: tekst.to_string(),
            naam: None,
            verbeterd: false,
        }
    }

    #[test]
    fn een_weggevallen_woord_of_een_deel_van_een_woord_is_een_verandering() {
        let oud = vec![anderen(0, 4000, "Goedemiddag allemaal en welkom")];
        // Een woord minder.
        let minder = vec![anderen(0, 4000, "goedemiddag allemaal en")];
        assert_eq!(veranderde_beurten(&oud, &minder), vec![true]);
        // "dag" staat in "goedemiddag", maar is een ander woord; ook als het in twee stukken komt.
        let oud = vec![anderen(0, 4000, "goedemiddag")];
        let stukken = vec![anderen(0, 1000, "goede"), anderen(2000, 4000, "dag")];
        assert_eq!(veranderde_beurten(&oud, &stukken), vec![true, true]);
        // Dezelfde woorden anders verdeeld, over twee oude beurten heen: geen verandering.
        let oud = vec![
            anderen(0, 2000, "een twee drie"),
            anderen(1500, 4000, "vier"),
        ];
        let anders = vec![
            anderen(0, 1000, "Een"),
            anderen(1200, 4000, "twee  drie vier"),
        ];
        assert_eq!(veranderde_beurten(&oud, &anders), vec![false, false]);
        // Ander tijdvak, andere kant: los beoordeeld.
        let jij = Beurt {
            kant: Kant::Jij,
            ..anderen(0, 4000, "een twee drie vier")
        };
        assert_eq!(veranderde_beurten(&oud, &[jij]), vec![true]);
    }

    #[test]
    fn online_krijgt_een_uiting_in_een_gat_de_dichtstbijzijnde_spreker_met_zijn_naam() {
        let mut oud = met_sprekers();
        oud.geef_naam(2, "Piet", Naambereik::Spreker).unwrap();
        // Tussen 4000 (einde van Piet) en 12000 sprak niemand van Anderen; de nieuwe uiting om
        // 6000 ligt meer dan een seconde van elke oude beurt.
        let nieuw = beurten_opnieuw(&oud, &[], &[seg(6000, 6500, "Nog iets.")]);
        assert_eq!(nieuw.len(), 1);
        assert_eq!(nieuw[0].sprekernummer, Some(2));
        assert_eq!(nieuw[0].spreker(), "Piet");
        // Zonder sprekers in het oude verslag blijft het "Anderen".
        let zonder = gesprek(Vergadersoort::Online);
        let nieuw = beurten_opnieuw(&zonder, &[], &[seg(30_000, 31_000, "Iets.")]);
        assert_eq!(nieuw[0].sprekernummer, None);
    }
}
