//! Notulen maken van het verslag dat open is (T3, ADR 0026), als lange klus op een eigen draad.
//!
//! Het loopt zoals opnieuw uitschrijven (ADR 0023): zolang Mondig notulen maakt, staat het in het
//! verslag ([`NotulenMaken`]) en weigert elke wijziging aan het verslag, want dan passen de
//! bronverwijzingen niet meer. De interface krijgt de voortgang per deel en kan Stoppen. Klaar:
//! de notulen komen in het verslag en meteen in `verslag.json`.
//!
//! Er komt nooit tekst, een prompt, een antwoord of een sleutel in het logboek of in een
//! foutmelding; dat is de kern al zo gebouwd (`mondig_core::notulen::maken`).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

use mondig_core::bewaren::{Notulenblok, Notulenregel};
use mondig_core::gesprek::Gesprek;
use mondig_core::notulen::maken::{self, Instellingen, NotulenFout};
use mondig_core::notulen::{
    BewaardeNotulen, Bron, Controle, Gecontroleerd, Invoer, OpgeslagenNotulen, Taal, tijdcode,
};
use mondig_core::persoonsgegevens::Markeringen;
use mondig_core::sleutel;
use mondig_core::taak::{Foutmelding, Vervolg};
use mondig_core::taalmodel::{TaalmodelFout, Verbinding, Voorinstelling};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::{
    Binnen, Fase, Schrijfopdracht, VergaderingStaat, VergaderingStand, Verslag, blokkerend,
    geen_verslag, slot, voer_uit, wijzig,
};

/// Telt de keren dat het maken begint, zodat een uitslag van een gestopt maken nergens bij hoort.
static VOLGENDE: AtomicU64 = AtomicU64::new(1);

/// Wat de gebruiker bevestigde voor hij Notulen maken koos: waar de tekst heen gaat (`mistral` of
/// `ollama`) en of persoonsgegevens vervangen worden. Rust vergelijkt het met de stand van nu.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Bevestiging {
    pub(crate) aanbieder: String,
    pub(crate) vervangen: bool,
}

/// Mondig maakt nu notulen van dit verslag.
pub(super) struct NotulenMaken {
    id: u64,
    annuleer: Arc<AtomicBool>,
    klaar: usize,
    totaal: usize,
}

/// De voortgang zoals de interface hem toont.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotulenVoortgang {
    /// Hoeveel delen klaar zijn.
    klaar: usize,
    /// Hoeveel delen het worden (met het samenvoegen erbij); 0 als dat nog niet bekend is.
    totaal: usize,
}

/// Waar een punt gezegd is, voor de interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BronInfo {
    /// Staat het citaat echt in het verslag (ADR 0026 punt 3)?
    gevonden: bool,
    /// De beurt waarnaar de bronknop springt (vanaf 0, zoals `BeurtInfo::index`). Alleen als de
    /// bron gevonden is en de notulen bij de huidige versie van het verslag horen.
    beurt: Option<usize>,
    /// De tijd van die beurt in milliseconden.
    ms: Option<u64>,
    /// Het citaat dat het model gaf, voor de uitleg bij de bronknop. Het is alleen om te lezen en
    /// staat er al vervangen in als "Persoonsgegevens vervangen" aan staat.
    citaat: String,
}

/// Een besluit voor de interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BesluitInfo {
    /// Het vaste nummer van het punt (ook na weghalen van een ander punt).
    id: u32,
    /// De echte tekst: daarmee begint het bewerken.
    tekst: String,
    /// De tekst zoals hij op het scherm staat: met "Persoonsgegevens vervangen" aan met
    /// `[BSN]` enzovoort, net als in Kopiëren en Opslaan.
    tekst_getoond: String,
    bron: BronInfo,
}

/// Een actiepunt voor de interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActiepuntInfo {
    /// Het vaste nummer van het punt.
    id: u32,
    wat: String,
    wie: String,
    wanneer: String,
    /// Zoals `BesluitInfo::tekst_getoond`: wat, wie en wanneer zoals ze op het scherm staan.
    wat_getoond: String,
    wie_getoond: String,
    wanneer_getoond: String,
    wie_niet_teruggevonden: bool,
    wanneer_niet_teruggevonden: bool,
    bron: BronInfo,
}

/// De notulen voor de interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotulenInfo {
    samenvatting: String,
    /// Zoals `BesluitInfo::tekst_getoond`.
    samenvatting_getoond: String,
    besluiten: Vec<BesluitInfo>,
    actiepunten: Vec<ActiepuntInfo>,
    /// `mistral` of `ollama`.
    aanbieder: String,
    model: String,
    moment_ms: u64,
    taal: Taal,
    /// Gemaakt bij een andere versie van het verslag: de bronnen zijn dan niet klikbaar.
    vorige_versie: bool,
    /// Hoeveel bronnen gevonden en hoeveel niet teruggevonden zijn.
    gevonden: usize,
    niet_gevonden: usize,
}

/// De notulen en het maken ervan in de stand van het verslag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotulenStand {
    /// De notulen, als deze versie ze kan lezen.
    pub(super) info: Option<NotulenInfo>,
    /// Staan er notulen in het verslag die deze versie niet kan lezen? Ze blijven bewaard.
    onleesbaar: bool,
    /// De voortgang zolang Mondig notulen maakt.
    pub(super) maken: Option<NotulenVoortgang>,
}

impl NotulenStand {
    pub(super) fn van(v: &Verslag) -> Self {
        let info = match &v.notulen {
            Some(OpgeslagenNotulen::Leesbaar(n)) => Some(notulen_info(n, &v.gesprek, &v.persoon)),
            _ => None,
        };
        Self {
            info,
            onleesbaar: matches!(v.notulen, Some(OpgeslagenNotulen::Onleesbaar(_))),
            maken: v.notulen_maken.as_ref().map(|m| NotulenVoortgang {
                klaar: m.klaar,
                totaal: m.totaal,
            }),
        }
    }
}

fn bron_info(
    bron: &Bron,
    controle: Controle,
    gesprek: &Gesprek,
    persoon: &Markeringen,
    vorige: bool,
) -> BronInfo {
    let beurt = match controle {
        Controle::Gevonden { beurt } if !vorige && beurt >= 1 && beurt <= gesprek.beurten.len() => {
            Some(beurt - 1)
        }
        _ => None,
    };
    BronInfo {
        gevonden: controle.gevonden(),
        beurt,
        ms: beurt
            .and_then(|i| gesprek.beurten.get(i))
            .map(|b| b.start_ms),
        citaat: persoon.voor_uitvoer(&bron.citaat),
    }
}

/// De notulen zoals de interface ze toont. Met "Persoonsgegevens vervangen" aan krijgt het scherm
/// dezelfde vervangingen als Kopiëren en Opslaan (`BewaardeNotulen::als_blok`), net als het
/// verslag; de echte tekst blijft erbij, want bewerken begint daarmee.
pub(super) fn notulen_info(
    n: &BewaardeNotulen,
    gesprek: &Gesprek,
    persoon: &Markeringen,
) -> NotulenInfo {
    let vorige = n.bij_vorige_versie(gesprek);
    let (gevonden, niet_gevonden) = n.telling();
    NotulenInfo {
        samenvatting: n.samenvatting.clone(),
        samenvatting_getoond: persoon.voor_uitvoer(&n.samenvatting),
        besluiten: n
            .besluiten
            .iter()
            .map(|b| BesluitInfo {
                id: b.id,
                tekst: b.punt.tekst.clone(),
                tekst_getoond: persoon.voor_uitvoer(&b.punt.tekst),
                bron: bron_info(&b.punt.bron, b.controle, gesprek, persoon, vorige),
            })
            .collect(),
        actiepunten: n
            .actiepunten
            .iter()
            .map(|a| ActiepuntInfo {
                id: a.id,
                wat: a.punt.wat.clone(),
                wie: a.punt.wie.clone(),
                wanneer: a.punt.wanneer.clone(),
                wat_getoond: persoon.voor_uitvoer(&a.punt.wat),
                wie_getoond: persoon.voor_naam(&a.punt.wie),
                wanneer_getoond: persoon.voor_uitvoer(&a.punt.wanneer),
                wie_niet_teruggevonden: a.wie_niet_teruggevonden,
                wanneer_niet_teruggevonden: a.wanneer_niet_teruggevonden,
                bron: bron_info(&a.punt.bron, a.controle, gesprek, persoon, vorige),
            })
            .collect(),
        aanbieder: n.aanbieder.clone(),
        model: n.model.clone(),
        moment_ms: n.moment_ms,
        taal: n.taal,
        vorige_versie: vorige,
        gevonden,
        niet_gevonden,
    }
}

impl Binnen {
    /// Begint het maken van notulen voor het open verslag: weigert zonder verslag, als het
    /// verslag opnieuw wordt uitgeschreven of als er al notulen worden gemaakt. Geeft het nummer
    /// van dit maken, het gesprek en de markeringen zoals ze nu zijn.
    pub(super) fn begin_notulen(
        &mut self,
        annuleer: Arc<AtomicBool>,
        bevestigd: &Bevestiging,
        aanbieder: &str,
        overschrijf: bool,
    ) -> Result<(u64, Gesprek, Markeringen), Foutmelding> {
        let Fase::Verslag(v) = &mut self.fase else {
            return Err(geen_verslag());
        };
        if v.opnieuw.is_some() {
            return Err(Foutmelding::nieuw(
                "Mondig schrijft dit verslag opnieuw uit. Maak daarna de notulen.",
                Vervolg::Geen,
            ));
        }
        if v.notulen_maken.is_some() {
            return Err(Foutmelding::nieuw(
                "Mondig maakt al notulen van dit verslag.",
                Vervolg::Geen,
            ));
        }
        // De gebruiker bevestigde waar de tekst heen gaat en of persoonsgegevens vervangen worden;
        // is een van beide intussen anders (andere instelling, de schakelaar omgezet), dan telt
        // die bevestiging niet meer en vraagt de interface opnieuw.
        if bevestigd.aanbieder != aanbieder || bevestigd.vervangen != v.persoon.vervangen() {
            return Err(Foutmelding::nieuw(
                "Waar de tekst heen gaat of het vervangen van persoonsgegevens is veranderd. Bevestig het opnieuw.",
                Vervolg::OpnieuwProberen,
            ));
        }
        // Notulen die deze versie niet kan lezen (van een nieuwere versie van Mondig) worden alleen
        // overschreven als de gebruiker dat uitdrukkelijk vroeg.
        if matches!(v.notulen, Some(OpgeslagenNotulen::Onleesbaar(_))) && !overschrijf {
            return Err(Foutmelding::nieuw(
                "Er staan notulen van een nieuwere versie van Mondig. Bevestig dat je ze wilt overschrijven.",
                Vervolg::Geen,
            ));
        }
        let id = VOLGENDE.fetch_add(1, Ordering::Relaxed);
        v.notulen_maken = Some(NotulenMaken {
            id,
            annuleer,
            klaar: 0,
            totaal: 0,
        });
        self.fout = None;
        Ok((id, v.gesprek.clone(), v.persoon.clone()))
    }

    /// Een nieuwe voortgang van het maken met nummer `id`. `false` als dat maken niet meer loopt
    /// of er niets veranderde.
    pub(super) fn voortgang_notulen(&mut self, id: u64, klaar: usize, totaal: usize) -> bool {
        let Fase::Verslag(v) = &mut self.fase else {
            return false;
        };
        match &mut v.notulen_maken {
            Some(m) if m.id == id && (m.klaar, m.totaal) != (klaar, totaal) => {
                m.klaar = klaar;
                m.totaal = totaal;
                true
            }
            _ => false,
        }
    }

    /// Het maken met nummer `id` is klaar. Gelukt: de notulen komen in het verslag, en de
    /// opdracht om ze naar `verslag.json` te schrijven. Mislukt: een melding in de stand, het
    /// verslag blijft zoals het was. Gestopt (of het verslag is er niet meer): niets. Geeft ook
    /// of de stand veranderde.
    pub(super) fn klaar_notulen(
        &mut self,
        id: u64,
        uitslag: Result<BewaardeNotulen, NotulenFout>,
    ) -> (bool, Option<Schrijfopdracht>) {
        let Fase::Verslag(v) = &mut self.fase else {
            return (false, None);
        };
        if v.notulen_maken.as_ref().is_none_or(|m| m.id != id) {
            return (false, None);
        }
        v.notulen_maken = None;
        match uitslag {
            Ok(notulen) => {
                v.notulen = Some(OpgeslagenNotulen::Leesbaar(notulen));
                // Nieuwe notulen: "Ongedaan maken" van een naam zou ze anders terugzetten. Bij
                // mislukken of stoppen blijft het ongedaan maken staan.
                v.stap_terug = None;
                self.fout = None;
                (true, self.plan_bewaren(false))
            }
            Err(NotulenFout::Taalmodel(TaalmodelFout::Geannuleerd)) => (true, None),
            Err(fout) => {
                self.fout = Some(Foutmelding::from(fout));
                (true, None)
            }
        }
    }

    /// Stoppen: het maken telt niet meer, het verslag blijft zoals het was. Geeft `true` als er
    /// iets liep. De aanvraag naar het taalmodel breekt af zodra de vlag aan gaat.
    pub(super) fn stop_notulen(&mut self) -> bool {
        match &mut self.fase {
            Fase::Verslag(v) => v.notulen_maken.take().is_some_and(|m| {
                m.annuleer.store(true, Ordering::Relaxed);
                true
            }),
            _ => false,
        }
    }
}

/// Het deel van het maken dat op de eigen draad loopt.
fn draai(
    app: &AppHandle,
    id: u64,
    verbinding: &Verbinding,
    gesprek: &Gesprek,
    markeringen: &Markeringen,
    instellingen: &Instellingen,
    annuleer: &AtomicBool,
) {
    let naam = verbinding.voorinstelling().naam();
    let uitslag = catch_unwind(AssertUnwindSafe(|| {
        let mut voortgang = |klaar: usize, totaal: usize| {
            let staat = app.state::<VergaderingStaat>();
            let veranderd = slot(&staat.binnen).voortgang_notulen(id, klaar, totaal);
            if veranderd {
                wijzig(app, |_| {});
            }
        };
        maken::maak(
            verbinding,
            gesprek,
            markeringen,
            instellingen,
            annuleer,
            &mut voortgang,
        )
        .map(|uitkomst| uitkomst.notulen)
    }))
    // Een paniek mag het verslag niet voor altijd vastzetten.
    .unwrap_or(Err(NotulenFout::Taalmodel(TaalmodelFout::Onverwacht(naam))));
    let staat = app.state::<VergaderingStaat>();
    let (veranderd, opdracht) = slot(&staat.binnen).klaar_notulen(id, uitslag);
    if let Some(opdracht) = opdracht {
        voer_uit(app, &opdracht);
    }
    if veranderd {
        wijzig(app, |_| {});
    }
}

/// Begint met maken; zie [`vergadering_notulen_maken`].
fn start(
    app: &AppHandle,
    taal: &str,
    bevestigd: &Bevestiging,
    overschrijf: bool,
) -> Result<VergaderingStand, Foutmelding> {
    let gekozen = crate::notulen::lees(app);
    let sleutel_staat = gekozen.aanbieder == Some(Voorinstelling::Mistral)
        && sleutel::bestaat(Voorinstelling::Mistral).unwrap_or(false);
    let (voorinstelling, model) = crate::notulen::bepaal_start(&gekozen, sleutel_staat)?;
    let verbinding = Verbinding::voor(voorinstelling).map_err(Foutmelding::from)?;
    let taal = if taal == "en" { Taal::En } else { Taal::Nl };
    let instellingen = Instellingen::voor(voorinstelling, &model, taal);

    let annuleer = Arc::new(AtomicBool::new(false));
    let staat = app.state::<VergaderingStaat>();
    let (id, gesprek, markeringen) = slot(&staat.binnen).begin_notulen(
        Arc::clone(&annuleer),
        bevestigd,
        voorinstelling.code(),
        overschrijf,
    )?;
    // Het aantal delen meldt de kern zelf als (0, totaal) voor de eerste aanvraag.
    let stand = wijzig(app, |_| {});

    let app = app.clone();
    thread::spawn(move || {
        draai(
            &app,
            id,
            &verbinding,
            &gesprek,
            &markeringen,
            &instellingen,
            &annuleer,
        );
    });
    Ok(stand)
}

/// Maakt notulen van het verslag dat open is, in de taal van de interface (`nl` of `en`). Weigert
/// met een melding in gewone taal zonder instelling, zonder vinkje over training of zonder sleutel
/// (bij Mistral). Het maken loopt op een eigen draad; de voortgang staat in de stand van het
/// verslag.
#[tauri::command]
pub(crate) async fn vergadering_notulen_maken(
    app: AppHandle,
    taal: String,
    bevestigd: Bevestiging,
    overschrijf: bool,
) -> Result<VergaderingStand, Foutmelding> {
    blokkerend(move || start(&app, &taal, &bevestigd, overschrijf)).await
}

/// Stopt het maken van notulen: het verslag blijft zoals het was.
#[tauri::command]
pub(crate) fn vergadering_notulen_stoppen(app: AppHandle) -> VergaderingStand {
    slot(&app.state::<VergaderingStaat>().binnen).stop_notulen();
    wijzig(&app, |_| {})
}

/// Een besluit of een actiepunt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PuntSoort {
    Besluit,
    Actiepunt,
}

/// De notulen van het verslag, als ze er zijn en te lezen.
fn leesbare(v: &mut Verslag) -> Result<&mut BewaardeNotulen, Foutmelding> {
    match &mut v.notulen {
        Some(OpgeslagenNotulen::Leesbaar(n)) => Ok(n),
        _ => Err(Foutmelding::nieuw(
            "Dit verslag heeft geen notulen die Mondig kan aanpassen.",
            Vervolg::Geen,
        )),
    }
}

fn geen_punt() -> Foutmelding {
    Foutmelding::nieuw("Dit punt staat niet meer in de notulen.", Vervolg::Geen)
}

/// Zo lang mag de tekst van een punt hooguit zijn, en zo lang wie of wanneer.
const MAX_TEKST: usize = 1000;
const MAX_VELD: usize = 200;

/// Haalt besturingstekens weg, laat witruimte één spatie worden en kapt af.
fn schoon(tekst: &str, max: usize) -> String {
    tekst
        .chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

/// Past een punt aan. Het punt houdt zijn bron en het label "Niet teruggevonden" blijft staan
/// als dat er was (ADR 0026 punt 7). Alleen bij `wie` en `wanneer` vervalt de markering van dat
/// veld als de gebruiker het veld zelf veranderde: de nieuwe waarde is zijn eigen keuze.
pub(super) fn pas_punt_aan(
    v: &mut Verslag,
    soort: PuntSoort,
    id: u32,
    tekst: &str,
    wie: &str,
    wanneer: &str,
) -> Result<(), Foutmelding> {
    let notulen = leesbare(v)?;
    let tekst = schoon(tekst, MAX_TEKST);
    if tekst.is_empty() {
        return Err(Foutmelding::nieuw(
            "Een punt kan niet leeg zijn. Haal het weg als je het niet wilt houden.",
            Vervolg::Geen,
        ));
    }
    match soort {
        PuntSoort::Besluit => {
            let punt: &mut Gecontroleerd<_> = notulen
                .besluiten
                .iter_mut()
                .find(|p| p.id == id)
                .ok_or_else(geen_punt)?;
            punt.punt.tekst = tekst;
        }
        PuntSoort::Actiepunt => {
            let punt = notulen
                .actiepunten
                .iter_mut()
                .find(|p| p.id == id)
                .ok_or_else(geen_punt)?;
            let (wie, wanneer) = (schoon(wie, MAX_VELD), schoon(wanneer, MAX_VELD));
            // Opgeschoond tegen opgeschoond: een wie met een spatie erachter van het model is
            // hetzelfde als zonder.
            if wie != schoon(&punt.punt.wie, MAX_VELD) {
                punt.wie_niet_teruggevonden = false;
            }
            if wanneer != schoon(&punt.punt.wanneer, MAX_VELD) {
                punt.wanneer_niet_teruggevonden = false;
            }
            punt.punt.wat = tekst;
            punt.punt.wie = wie;
            punt.punt.wanneer = wanneer;
        }
    }
    Ok(())
}

/// Haalt een punt weg.
pub(super) fn haal_punt_weg(v: &mut Verslag, soort: PuntSoort, id: u32) -> Result<(), Foutmelding> {
    let notulen = leesbare(v)?;
    match soort {
        PuntSoort::Besluit => {
            let plek = notulen.besluiten.iter().position(|p| p.id == id);
            notulen.besluiten.remove(plek.ok_or_else(geen_punt)?);
        }
        PuntSoort::Actiepunt => {
            let plek = notulen.actiepunten.iter().position(|p| p.id == id);
            notulen.actiepunten.remove(plek.ok_or_else(geen_punt)?);
        }
    }
    Ok(())
}

impl Verslag {
    /// Na een verbeterde regel of een andere naam: de bronnen van de notulen opnieuw controleren
    /// tegen de tekst van nu, zodat een citaat dat de gebruiker wegverbeterde "niet teruggevonden"
    /// wordt. Alleen als de notulen bij de huidige opbouw van het verslag horen.
    pub(super) fn hercontroleer_notulen(&mut self) {
        if let Some(OpgeslagenNotulen::Leesbaar(n)) = &mut self.notulen
            && !n.bij_vorige_versie(&self.gesprek)
        {
            n.hercontroleer(&Invoer::van(&self.gesprek, &self.persoon));
        }
    }

    /// Geeft punten zonder nummer (uit een bestand van voor de nummers) een nummer.
    pub(super) fn nummer_notulenpunten(&mut self) {
        if let Some(OpgeslagenNotulen::Leesbaar(n)) = &mut self.notulen {
            n.nummer_punten();
        }
    }
}

/// Past een besluit of actiepunt aan. Bij een besluit zijn `wie` en `wanneer` niet van belang.
#[tauri::command]
pub(crate) async fn vergadering_notulen_punt_aanpassen(
    app: AppHandle,
    soort: PuntSoort,
    id: u32,
    tekst: String,
    wie: Option<String>,
    wanneer: Option<String>,
) -> Result<VergaderingStand, Foutmelding> {
    super::wijzig_open(app, move |v| {
        pas_punt_aan(
            v,
            soort,
            id,
            &tekst,
            wie.as_deref().unwrap_or_default(),
            wanneer.as_deref().unwrap_or_default(),
        )
    })
    .await
}

/// Haalt een besluit of actiepunt uit de notulen.
#[tauri::command]
pub(crate) async fn vergadering_notulen_punt_weghalen(
    app: AppHandle,
    soort: PuntSoort,
    id: u32,
) -> Result<VergaderingStand, Foutmelding> {
    super::wijzig_open(app, move |v| haal_punt_weg(v, soort, id)).await
}

/// De notulen als platte tekst, voor het klembord: de kop, de samenvatting, de besluiten en de
/// actiepunten, met bij elk punt de tijd van de bron.
pub(crate) fn blok_als_tekst(blok: &Notulenblok) -> String {
    let mut regels: Vec<String> = Vec::new();
    for regel in &blok.regels {
        match regel {
            Notulenregel::Kop(tekst) | Notulenregel::Alinea(tekst) => regels.push(tekst.clone()),
            Notulenregel::Tussenkop(tekst) => {
                regels.push(String::new());
                regels.push(tekst.clone());
            }
            Notulenregel::Punt { tijd_ms, tekst } => {
                let voor = tijd_ms.map_or_else(|| "\u{2022}".to_string(), tijdcode);
                regels.push(format!("{voor}  {tekst}"));
            }
        }
    }
    regels.join("\n")
}

impl VergaderingStaat {
    /// De notulen van het verslag van nu, opgemaakt voor de export en het klembord (ADR 0026
    /// punt 8). `None` zonder verslag, of zonder notulen die deze versie kan lezen. Bij `taak`
    /// moet het het verslag van die taak zijn.
    pub(crate) fn notulenblok(
        &self,
        taak: Option<mondig_core::taak::TaakId>,
    ) -> Option<Notulenblok> {
        let binnen = slot(&self.binnen);
        let Fase::Verslag(v) = &binnen.fase else {
            return None;
        };
        if taak.is_some_and(|t| t != v.taak) {
            return None;
        }
        match &v.notulen {
            Some(OpgeslagenNotulen::Leesbaar(n)) => Some(n.als_blok(&v.gesprek, &v.persoon)),
            _ => None,
        }
    }
}

/// Zet de notulen van het open verslag op het klembord, buiten de klembordgeschiedenis. Alleen de
/// notulen, niet het verslag.
#[tauri::command]
pub(crate) async fn vergadering_notulen_kopieer(app: AppHandle) -> Result<(), Foutmelding> {
    blokkerend(move || {
        let blok = app
            .state::<VergaderingStaat>()
            .notulenblok(None)
            .ok_or_else(|| {
                Foutmelding::nieuw("Dit verslag heeft nog geen notulen.", Vervolg::Geen)
            })?;
        if crate::plakken::zet_op_klembord(&blok_als_tekst(&blok)) {
            Ok(())
        } else {
            Err(Foutmelding::nieuw(
                "Kopiëren is niet gelukt. Probeer het opnieuw.",
                Vervolg::OpnieuwProberen,
            ))
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mondig_core::gesprek::{Beurt, Kant, Spoorstand, Vergadersoort};
    use mondig_core::notulen::{Actiepunt, Besluit, Herkomst, Notulen};
    use mondig_core::taak::TaakId;

    use super::*;

    fn gesprek() -> Gesprek {
        let beurt = |kant, start_ms, eind_ms, tekst: &str| Beurt {
            kant,
            sprekernummer: None,
            start_ms,
            eind_ms,
            tekst: tekst.to_string(),
            naam: None,
            verbeterd: false,
        };
        Gesprek {
            beurten: vec![
                beurt(
                    Kant::Jij,
                    0,
                    5_000,
                    "We besluiten de bijeenkomst op donderdag te houden.",
                ),
                beurt(
                    Kant::Anderen,
                    9_000,
                    14_000,
                    "Ik stuur Piet de uitnodiging voor vrijdag door.",
                ),
            ],
            duur: Duration::from_secs(14),
            meldingen: Vec::new(),
            jij: Spoorstand::Opgenomen,
            anderen: Spoorstand::Opgenomen,
            weggelaten: 0,
            soort: Vergadersoort::Online,
        }
    }

    fn punt(tekst: &str, beurt: i64, citaat: &str) -> (String, Bron) {
        (
            tekst.into(),
            Bron {
                beurt,
                citaat: citaat.into(),
            },
        )
    }

    /// Notulen met een gevonden besluit, een niet teruggevonden besluit en een actiepunt met een
    /// markering bij `wie`.
    fn notulen(gesprek: &Gesprek) -> BewaardeNotulen {
        let (t1, b1) = punt(
            "De bijeenkomst is op donderdag.",
            1,
            "We besluiten de bijeenkomst op donderdag te houden.",
        );
        let (t2, b2) = punt(
            "Er komt een nieuwe aula.",
            2,
            "Dit staat nergens in het verslag.",
        );
        let n = Notulen {
            samenvatting: "Over de bijeenkomst.".into(),
            besluiten: vec![
                Besluit {
                    tekst: t1,
                    bron: b1,
                },
                Besluit {
                    tekst: t2,
                    bron: b2,
                },
            ],
            actiepunten: vec![Actiepunt {
                wat: "De uitnodiging doorsturen".into(),
                wie: "Anna".into(),
                wanneer: "vrijdag".into(),
                bron: Bron {
                    beurt: 2,
                    citaat: "Ik stuur Piet de uitnodiging voor vrijdag door.".into(),
                },
            }],
        };
        let invoer = Invoer::van(gesprek, &Markeringen::default());
        mondig_core::notulen::controleer_notulen(
            n,
            &invoer,
            gesprek,
            Herkomst {
                aanbieder: "mistral".into(),
                model: "mistral-large-latest".into(),
                moment_ms: 1,
                taal: Taal::Nl,
            },
        )
    }

    fn verslag_met_notulen() -> Verslag {
        let g = gesprek();
        let n = notulen(&g);
        let mut v = Verslag::leeg(TaakId(1), std::path::PathBuf::from("map"), g);
        v.notulen = Some(OpgeslagenNotulen::Leesbaar(n));
        v
    }

    fn verslag_in(binnen: &Binnen) -> &Verslag {
        match &binnen.fase {
            Fase::Verslag(v) => Some(v),
            _ => None,
        }
        .expect("er is een verslag open")
    }

    fn leesbaar_in(v: &Verslag) -> &BewaardeNotulen {
        match &v.notulen {
            Some(OpgeslagenNotulen::Leesbaar(n)) => Some(n),
            _ => None,
        }
        .expect("er zijn leesbare notulen")
    }

    fn begin(binnen: &mut Binnen) -> Result<(u64, Gesprek, Markeringen), Foutmelding> {
        binnen.begin_notulen(
            Arc::new(AtomicBool::new(false)),
            &Bevestiging {
                aanbieder: "mistral".into(),
                vervangen: false,
            },
            "mistral",
            false,
        )
    }

    fn binnen_met(v: Verslag) -> Binnen {
        Binnen {
            fase: Fase::Verslag(Box::new(v)),
            ..Binnen::default()
        }
    }

    #[test]
    fn de_stand_toont_de_bron_van_een_gevonden_punt_en_markeert_de_rest() {
        let v = verslag_met_notulen();
        let stand = NotulenStand::van(&v);
        let info = stand.info.expect("notulen");
        assert!(!info.vorige_versie);
        assert!(info.besluiten[0].bron.gevonden);
        assert_eq!(info.besluiten[0].bron.beurt, Some(0));
        assert_eq!(info.besluiten[0].bron.ms, Some(0));
        assert!(!info.besluiten[1].bron.gevonden);
        assert_eq!(info.besluiten[1].bron.beurt, None);
        assert_eq!(info.actiepunten[0].bron.beurt, Some(1));
        assert!(info.actiepunten[0].wie_niet_teruggevonden);
        assert_eq!((info.gevonden, info.niet_gevonden), (2, 1));
    }

    #[test]
    fn met_vervangen_aan_toont_het_scherm_hetzelfde_als_kopieren_en_opslaan() {
        let mut v = verslag_met_notulen();
        // Een BSN in de samenvatting en in het citaat, een eigen markering in een besluit, in het
        // wat van een actiepunt en in de wie.
        if let Some(OpgeslagenNotulen::Leesbaar(n)) = &mut v.notulen {
            n.samenvatting = "Over de bijeenkomst, BSN 111222333.".into();
            n.besluiten[0].punt.bron.citaat = "Mijn BSN is 111222333.".into();
        }
        v.persoon.markeer("donderdag").unwrap();
        v.persoon.markeer("uitnodiging").unwrap();
        v.persoon.markeer("Anna").unwrap();

        // Vervangen uit: het scherm toont de echte tekst.
        let uit = NotulenStand::van(&v).info.expect("notulen");
        assert_eq!(uit.samenvatting_getoond, uit.samenvatting);
        assert_eq!(uit.besluiten[0].tekst_getoond, uit.besluiten[0].tekst);
        assert_eq!(uit.actiepunten[0].wie_getoond, "Anna");

        // Vervangen aan: op het scherm staat nergens meer het echte gegeven.
        v.persoon.zet_vervangen(true);
        let info = NotulenStand::van(&v).info.expect("notulen");
        assert_eq!(info.samenvatting_getoond, "Over de bijeenkomst, BSN [BSN].");
        assert_eq!(
            info.besluiten[0].tekst_getoond,
            "De bijeenkomst is op [GEGEVEN]."
        );
        assert_eq!(info.actiepunten[0].wat_getoond, "De [GEGEVEN] doorsturen");
        assert_eq!(info.actiepunten[0].wie_getoond, "[GEGEVEN]");
        assert_eq!(info.actiepunten[0].wanneer_getoond, "vrijdag");
        assert_eq!(info.besluiten[0].bron.citaat, "Mijn BSN is [BSN].");
        // De echte tekst blijft in de stand: bewerken begint daarmee.
        assert_eq!(info.besluiten[0].tekst, "De bijeenkomst is op donderdag.");
        assert_eq!(info.actiepunten[0].wie, "Anna");
        // Wat het scherm toont, staat ook zo in Kopiëren en Opslaan.
        let n = leesbaar_in(&v);
        let kopie = blok_als_tekst(&n.als_blok(&v.gesprek, &v.persoon));
        for getoond in [
            info.samenvatting_getoond.as_str(),
            info.besluiten[0].tekst_getoond.as_str(),
            info.actiepunten[0].wat_getoond.as_str(),
        ] {
            assert!(kopie.contains(getoond), "{getoond} ontbreekt in {kopie}");
        }
        assert!(kopie.contains("[GEGEVEN]"), "{kopie}");
        assert!(
            !kopie.contains("111222333") && !kopie.contains("Anna"),
            "{kopie}"
        );
        // En het is te serialiseren zonder het echte BSN in de getoonde velden.
        let json = serde_json::to_value(&info).unwrap();
        assert!(
            !json["samenvattingGetoond"]
                .to_string()
                .contains("111222333")
        );
        assert!(
            !json["besluiten"][0]["bron"]["citaat"]
                .to_string()
                .contains("111222333")
        );
    }

    #[test]
    fn na_een_andere_versie_van_het_verslag_zijn_de_bronnen_niet_klikbaar() {
        let mut v = verslag_met_notulen();
        v.gesprek.beurten[0].eind_ms += 500;
        let info = NotulenStand::van(&v).info.expect("notulen");
        assert!(info.vorige_versie);
        assert!(info.besluiten[0].bron.gevonden, "de controle zelf blijft");
        assert_eq!(info.besluiten[0].bron.beurt, None);
        assert_eq!(info.besluiten[0].bron.ms, None);
    }

    #[test]
    fn een_punt_aanpassen_bewaart_het_in_verslag_json_en_houdt_zijn_bron() {
        let mut binnen = binnen_met(verslag_met_notulen());
        let opdracht = super::super::wijzig_open_in(&mut binnen, |v| {
            pas_punt_aan(
                v,
                PuntSoort::Besluit,
                2,
                "Er komt een grotere aula.",
                "",
                "",
            )
        })
        .expect("aanpassen")
        .expect("er moet geschreven worden");
        let json = String::from_utf8(opdracht.bytes).expect("json");
        assert!(json.contains("Er komt een grotere aula."), "{json}");
        let v = verslag_in(&binnen);
        let n = leesbaar_in(v);
        // De bron en het label "niet teruggevonden" blijven.
        assert_eq!(n.besluiten[1].punt.bron.beurt, 2);
        assert_eq!(n.besluiten[1].controle, Controle::NietTeruggevonden);
    }

    #[test]
    fn een_actiepunt_aanpassen_laat_de_markering_vervallen_als_het_veld_verandert() {
        let mut v = verslag_met_notulen();
        pas_punt_aan(
            &mut v,
            PuntSoort::Actiepunt,
            1,
            "Doorsturen",
            "Anna",
            "maandag",
        )
        .expect("aanpassen");
        let n = leesbaar_in(&v);
        let a = &n.actiepunten[0];
        assert_eq!(a.punt.wanneer, "maandag");
        // `wie` bleef gelijk, dus die markering blijft; `wanneer` veranderde.
        assert!(a.wie_niet_teruggevonden);
        assert!(!a.wanneer_niet_teruggevonden);
    }

    #[test]
    fn een_lege_tekst_of_een_onbekend_punt_wordt_geweigerd() {
        let mut v = verslag_met_notulen();
        assert!(pas_punt_aan(&mut v, PuntSoort::Besluit, 1, "   ", "", "").is_err());
        assert!(pas_punt_aan(&mut v, PuntSoort::Besluit, 9, "x y", "", "").is_err());
        assert!(haal_punt_weg(&mut v, PuntSoort::Actiepunt, 3).is_err());
    }

    #[test]
    fn een_punt_weghalen_bewaart_het() {
        let mut binnen = binnen_met(verslag_met_notulen());
        let opdracht =
            super::super::wijzig_open_in(&mut binnen, |v| haal_punt_weg(v, PuntSoort::Besluit, 1))
                .expect("weghalen")
                .expect("er moet geschreven worden");
        assert!(
            !String::from_utf8(opdracht.bytes)
                .expect("json")
                .contains("De bijeenkomst is op donderdag.")
        );
        let v = verslag_in(&binnen);
        let n = leesbaar_in(v);
        assert_eq!(n.besluiten.len(), 1);
    }

    #[test]
    fn tijdens_het_maken_weigeren_de_wijzigcommandos_en_sluiten() {
        let mut binnen = binnen_met(verslag_met_notulen());
        let (id, _, _) = begin(&mut binnen).expect("begin");
        let fout = super::super::wijzig_open_in(&mut binnen, |v| {
            pas_punt_aan(v, PuntSoort::Besluit, 1, "Anders", "", "")
        })
        .err()
        .expect("weigeren");
        assert!(fout.melding.contains("maakt notulen"), "{}", fout.melding);
        // Ook de naam en de verbeteringen lopen via wijzig_open_in: dezelfde weigering.
        assert!(
            super::super::wijzig_open_in(&mut binnen, |v| {
                v.naamvelden.onderwerp = Some("x".into());
                Ok(())
            })
            .is_err()
        );
        assert!(binnen.sluit_verslag(false).is_err());
        assert!(binnen.neem_verslag().is_err());
        assert!(binnen.voor_opnieuw().is_err());
        // Een tweede keer beginnen kan niet.
        assert!(begin(&mut binnen).is_err());
        // Na Stoppen kan alles weer, en een late uitslag hoort nergens bij.
        assert!(binnen.stop_notulen());
        let (veranderd, opdracht) = binnen.klaar_notulen(id, Err(NotulenFout::GeenTekst));
        assert!(!veranderd && opdracht.is_none());
        assert!(binnen.fout.is_none());
        assert!(super::super::wijzig_open_in(&mut binnen, |_| Ok(())).is_ok());
    }

    #[test]
    fn klaar_zet_de_notulen_in_het_verslag_en_plant_het_schrijven() {
        let g = gesprek();
        let nieuw = notulen(&g);
        let mut v = Verslag::leeg(TaakId(1), std::path::PathBuf::from("map"), g);
        v.bewaard_bytes = v.bytes().unwrap_or_default();
        let mut binnen = binnen_met(v);
        let (id, _, _) = begin(&mut binnen).expect("begin");
        assert!(binnen.voortgang_notulen(id, 1, 3));
        let (veranderd, opdracht) = binnen.klaar_notulen(id, Ok(nieuw));
        assert!(veranderd);
        let json = String::from_utf8(opdracht.expect("schrijven").bytes).expect("json");
        assert!(json.contains("De bijeenkomst is op donderdag."), "{json}");
        let v = verslag_in(&binnen);
        assert!(v.notulen_maken.is_none());
        assert!(NotulenStand::van(v).info.is_some());
    }

    #[test]
    fn ongedaan_maken_blijft_tot_er_echt_nieuwe_notulen_staan() {
        let g = gesprek();
        let nieuw = notulen(&g);
        let mut v = Verslag::leeg(TaakId(1), std::path::PathBuf::from("map"), g);
        super::super::wijzig_sprekers(&mut v, |g| super::super::noem(g, 1, "Noor")).unwrap();
        assert!(v.stap_terug.is_some());
        let mut binnen = binnen_met(v);
        // Mislukt of gestopt: de stap blijft.
        let (id, _, _) = begin(&mut binnen).expect("begin");
        binnen.klaar_notulen(id, Err(NotulenFout::GeenTekst));
        assert!(verslag_in(&binnen).stap_terug.is_some());
        begin(&mut binnen).expect("begin");
        assert!(binnen.stop_notulen());
        assert!(verslag_in(&binnen).stap_terug.is_some());
        // Gelukt: ongedaan maken zou de nieuwe notulen weggooien, dus de stap vervalt.
        let (id, _, _) = begin(&mut binnen).expect("begin");
        binnen.klaar_notulen(id, Ok(nieuw));
        assert!(verslag_in(&binnen).stap_terug.is_none());
    }

    #[test]
    fn mislukt_geeft_een_melding_en_laat_oude_notulen_staan() {
        let mut binnen = binnen_met(verslag_met_notulen());
        let (id, _, _) = begin(&mut binnen).expect("begin");
        let (veranderd, opdracht) = binnen.klaar_notulen(id, Err(NotulenFout::GeenTekst));
        assert!(veranderd && opdracht.is_none());
        assert!(binnen.fout.is_some());
        let v = verslag_in(&binnen);
        assert!(v.notulen_maken.is_none());
        assert!(NotulenStand::van(v).info.is_some(), "oude notulen blijven");
    }

    #[test]
    fn de_tekst_voor_het_klembord_heeft_de_notulen_met_tijden() {
        let v = verslag_met_notulen();
        let n = leesbaar_in(&v);
        let tekst = blok_als_tekst(&n.als_blok(&v.gesprek, &v.persoon));
        assert!(tekst.starts_with("Notulen\n"), "{tekst}");
        assert!(
            tekst.contains("00:00  De bijeenkomst is op donderdag."),
            "{tekst}"
        );
        assert!(
            tekst.contains("Niet teruggevonden in het verslag"),
            "{tekst}"
        );
    }

    #[test]
    fn een_punt_wordt_gevonden_op_nummer_ook_na_weghalen_van_een_ander() {
        let mut v = verslag_met_notulen();
        haal_punt_weg(&mut v, PuntSoort::Besluit, 1).expect("weghalen");
        // Het tweede besluit heet nog steeds 2; een commando met een oud nummer raakt niets anders.
        pas_punt_aan(&mut v, PuntSoort::Besluit, 2, "Nieuwe tekst.", "", "").expect("aanpassen");
        assert!(haal_punt_weg(&mut v, PuntSoort::Besluit, 1).is_err());
        let n = leesbaar_in(&v);
        assert_eq!(n.besluiten.len(), 1);
        assert_eq!(n.besluiten[0].punt.tekst, "Nieuwe tekst.");
    }

    #[test]
    fn een_wie_met_een_spatie_van_het_model_verliest_zijn_markering_niet() {
        let mut v = verslag_met_notulen();
        if let Some(OpgeslagenNotulen::Leesbaar(n)) = &mut v.notulen {
            n.actiepunten[0].punt.wie = "Anna ".into();
        }
        // De gebruiker verandert niets aan `wie` in het formulier ("Anna" is "Anna ").
        pas_punt_aan(
            &mut v,
            PuntSoort::Actiepunt,
            1,
            "Doorsturen",
            "Anna",
            "vrijdag",
        )
        .expect("aanpassen");
        assert!(leesbaar_in(&v).actiepunten[0].wie_niet_teruggevonden);
    }

    #[test]
    fn maken_met_een_verouderde_bevestiging_wordt_geweigerd() {
        let mut binnen = binnen_met(verslag_met_notulen());
        let ander_aanbieder = Bevestiging {
            aanbieder: "ollama".into(),
            vervangen: false,
        };
        let fout = binnen
            .begin_notulen(
                Arc::new(AtomicBool::new(false)),
                &ander_aanbieder,
                "mistral",
                false,
            )
            .err()
            .expect("weigeren");
        assert!(
            fout.melding.contains("Bevestig het opnieuw"),
            "{}",
            fout.melding
        );
        // Vervangen stond aan bij de bevestiging, maar staat nu uit (of andersom).
        let vervangen = Bevestiging {
            aanbieder: "mistral".into(),
            vervangen: true,
        };
        assert!(
            binnen
                .begin_notulen(
                    Arc::new(AtomicBool::new(false)),
                    &vervangen,
                    "mistral",
                    false
                )
                .is_err()
        );
        assert!(verslag_in(&binnen).notulen_maken.is_none());
        assert!(begin(&mut binnen).is_ok());
    }

    #[test]
    fn onleesbare_notulen_worden_alleen_met_een_uitdrukkelijke_vlag_overschreven() {
        let mut v = verslag_met_notulen();
        v.notulen = Some(OpgeslagenNotulen::Onleesbaar(
            serde_json::json!({"nieuw": 1}),
        ));
        let mut binnen = binnen_met(v);
        let fout = begin(&mut binnen).err().expect("weigeren");
        assert!(fout.melding.contains("nieuwere versie"), "{}", fout.melding);
        assert!(matches!(
            verslag_in(&binnen).notulen,
            Some(OpgeslagenNotulen::Onleesbaar(_))
        ));
        let bevestiging = Bevestiging {
            aanbieder: "mistral".into(),
            vervangen: false,
        };
        assert!(
            binnen
                .begin_notulen(
                    Arc::new(AtomicBool::new(false)),
                    &bevestiging,
                    "mistral",
                    true
                )
                .is_ok()
        );
    }

    #[test]
    fn een_tekstverbetering_laat_de_notulen_staan_en_controleert_de_bron_opnieuw() {
        let mut v = verslag_met_notulen();
        let voor = NotulenStand::van(&v).info.expect("notulen");
        assert!(voor.besluiten[0].bron.gevonden);
        v.gesprek.beurten[0].tekst = "Iets heel anders gezegd.".into();
        v.hercontroleer_notulen();
        let na = NotulenStand::van(&v).info.expect("notulen");
        // Geen "vorige versie": de opbouw is gelijk. Wel niet meer teruggevonden.
        assert!(!na.vorige_versie);
        assert!(!na.besluiten[0].bron.gevonden);
        assert_eq!(na.besluiten[0].bron.beurt, None);
    }
}
