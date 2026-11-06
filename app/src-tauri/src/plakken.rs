//! Plakken na een dictaat (sprint 3c, ADR 0012 "Tekst nooit kwijt"). Het patroon is
//! Handy's "reliable paste" (`src-tauri/src/paste_tx`, MIT, commit `7bfb2e2`):
//!
//! 1. Wachten tot de modifiers van de sneltoets los zijn, met een grens. Anders wordt
//!    Ctrl+V bijvoorbeeld Ctrl+Alt+V.
//! 2. Het oude klembord bewaren en de tekst met delayed rendering op het klembord zetten,
//!    samen met de drie formaten die hem buiten de klembordgeschiedenis (Win+V) en de
//!    cloud houden.
//! 3. Ctrl+V sturen naar het venster met de focus.
//! 4. Het oude klembord terugzetten zodra het doelprogramma de tekst na Ctrl+V heeft
//!    opgevraagd (het leesbewijs, `WM_RENDERFORMAT`) en het daarna even stil bleef, of
//!    uiterlijk na 8 s. Niet als de gebruiker intussen zelf iets kopieerde.
//!
//! Dit bestand bevat de beslissingen als pure functies (met tests) en de koppeling met de
//! app. De Windows-kant staat in `plakken/win32.rs`: één vaste klembordthread die alle
//! opdrachten op volgorde afhandelt. Op macOS en Linux plakt Mondig nog niet (sprint 3e):
//! de tekst blijft dan alleen in het venster.
//!
//! Gebeurtenis naar de interface: `plak-uitslag` met `{ taak, uitkomst,
//! vorigKlembordKwijt }`. De tekst zelf gaat nooit naar een log.

// Plakken op macOS en Linux volgt in sprint 3e; tot dan gebruikt alleen Windows deze code.
#![cfg_attr(not(windows), allow(dead_code))]

use std::time::{Duration, Instant};

use mondig_core::taak::TaakId;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[cfg(windows)]
mod win32;

/// Na het laatste leesbewijs blijft de tekst nog zo lang op het klembord: sommige
/// programma's lezen meer dan eens per plakactie (Chromium kijkt eerst en leest dan).
/// Zelfde waarde als Handy (`QUIET_PERIOD`).
pub(crate) const STILTE_NA_LEZEN: Duration = Duration::from_millis(200);

/// Zonder leesbewijs komt het oude klembord zo lang na Ctrl+V terug (plan 3c).
pub(crate) const UITERLIJK_TERUGZETTEN: Duration = Duration::from_secs(8);

/// Kon Ctrl+V niet verstuurd worden, dan komt er geen leesbewijs en zetten we snel terug.
pub(crate) const TERUGZETTEN_ZONDER_INVOER: Duration = Duration::from_millis(500);

/// Was het token van het doelprogramma niet te lezen, dan plakken we toch en wachten we zo
/// lang op het leesbewijs. Komt het niet, dan hield Windows de toetsen waarschijnlijk
/// tegen (UIPI) en meldt Mondig dat meteen, in plaats van na acht seconden.
pub(crate) const GRENS_ONZEKER_DOEL: Duration = Duration::from_secs(1);

/// Lukt terugzetten niet (een ander programma houdt het klembord vast), dan probeert de
/// klembordthread het om de zoveel tijd opnieuw.
pub(crate) const HERPOGING_STAP: Duration = Duration::from_millis(500);

/// Zo lang blijft de klembordthread het terugzetten opnieuw proberen. Daarna laat hij het
/// dictaat op het klembord staan, als laatste redmiddel.
pub(crate) const HERPOGING_GRENS: Duration = Duration::from_secs(30);

/// Zo lang wachten we hooguit tot de gebruiker Ctrl, Alt, Shift en Windows heeft losgelaten.
/// In de houdmodus zijn ze meestal al los (loslaten stopt de opname en het uitschrijven
/// duurt een halve seconde); in de tikmodus kan iemand ze nog even vasthouden.
pub(crate) const MODIFIERS_GRENS: Duration = Duration::from_millis(1500);

/// Om de zoveel tijd kijken we opnieuw of de modifiers los zijn.
pub(crate) const MODIFIERS_STAP: Duration = Duration::from_millis(10);

/// Het integriteitsniveau "hoog" van Windows (`SECURITY_MANDATORY_HIGH_RID`): een
/// programma dat met beheerrechten draait.
pub(crate) const INTEGRITEIT_HOOG: u32 = 0x3000;

// ---------------------------------------------------------------------------
// De tekst
// ---------------------------------------------------------------------------

/// De tekst zoals hij geplakt wordt: met een spatie erachter, zodat twee dictaten niet aan
/// elkaar plakken. Niet als hij al op een spatie of regeleinde eindigt, en niet bij lege
/// tekst.
pub(crate) fn met_spatie(tekst: &str) -> String {
    match tekst.chars().last() {
        None => String::new(),
        Some(laatste) if laatste.is_whitespace() => tekst.to_string(),
        Some(_) => format!("{tekst} "),
    }
}

// ---------------------------------------------------------------------------
// Wachten op losse modifiers
// ---------------------------------------------------------------------------

/// Wat te doen terwijl we wachten tot de modifiers los zijn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Modifierwacht {
    /// Alles is los: plakken.
    Plakken,
    /// Er is nog iets ingedrukt en de grens is niet bereikt.
    Wachten,
    /// Er is na de grens nog iets ingedrukt: niet plakken.
    Opgeven,
}

/// Beslist na `verstreken` wachttijd, met `ingedrukt` voor "is er nu een modifier ingedrukt".
pub(crate) fn modifierwacht(ingedrukt: bool, verstreken: Duration) -> Modifierwacht {
    if !ingedrukt {
        Modifierwacht::Plakken
    } else if verstreken >= MODIFIERS_GRENS {
        Modifierwacht::Opgeven
    } else {
        Modifierwacht::Wachten
    }
}

/// Wacht tot `ingedrukt` onwaar geeft, hooguit [`MODIFIERS_GRENS`]. Geeft `true` als alles
/// los is. `ingedrukt` wordt meteen gevraagd, dus zonder modifiers wacht dit niet.
pub(crate) fn wacht_op_losse_modifiers(mut ingedrukt: impl FnMut() -> bool) -> bool {
    let begin = Instant::now();
    loop {
        match modifierwacht(ingedrukt(), begin.elapsed()) {
            Modifierwacht::Plakken => return true,
            Modifierwacht::Opgeven => return false,
            Modifierwacht::Wachten => std::thread::sleep(MODIFIERS_STAP),
        }
    }
}

// ---------------------------------------------------------------------------
// Waar plakken we in?
// ---------------------------------------------------------------------------

/// Het integriteitsniveau van een programma, zoals Windows het in het token zet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Integriteit {
    /// Het niveau zelf (`SECURITY_MANDATORY_*_RID`).
    Niveau(u32),
    /// Het token was niet te lezen. Vanuit een gewoon programma gebeurt dat vaak bij een
    /// programma met beheerrechten, maar ook bij andere beschermde programma's. Of UIPI
    /// de toetsen tegenhoudt, blijkt dan pas uit het leesbewijs.
    Onleesbaar,
}

/// Houdt UIPI de toetsaanslag aantoonbaar tegen? Windows laat een programma alleen invoer
/// sturen naar programma's op hetzelfde of een lager integriteitsniveau. `SendInput` meldt
/// dat niet, dus we kijken het vooraf na. Alleen een leesbaar, hoger niveau telt; draait
/// Mondig zelf met beheerrechten, dan houdt niets het tegen.
pub(crate) fn uipi_blokkeert(eigen: u32, doel: Integriteit) -> bool {
    eigen < INTEGRITEIT_HOOG && matches!(doel, Integriteit::Niveau(niveau) if niveau > eigen)
}

/// Is onbekend of UIPI de toetsen tegenhoudt? Dan plakken we en laat het leesbewijs het
/// uitwijzen (zie [`GRENS_ONZEKER_DOEL`]).
pub(crate) fn doel_onzeker(eigen: u32, doel: Integriteit) -> bool {
    eigen < INTEGRITEIT_HOOG && doel == Integriteit::Onleesbaar
}

/// Het venster met de focus, zover het voor plakken telt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Voorgrond {
    /// Is het een venster van Mondig zelf?
    pub(crate) eigen_proces: bool,
    /// Het integriteitsniveau van het programma achter het venster.
    pub(crate) integriteit: Integriteit,
}

/// Hoe het dictaat bij de gebruiker komt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Plan {
    /// Plakken in het venster met de focus. Met `doel_onzeker` was het niveau van het
    /// doelprogramma niet te lezen.
    Plakken { doel_onzeker: bool },
    /// De terugval: de tekst op het klembord zetten en niet plakken.
    AlleenKlembord,
    /// Niets doen: het venster van Mondig heeft de focus en toont de tekst al.
    Overslaan,
    /// Niet plakken, wel melden.
    Niet(PlakUitkomst),
}

/// Kiest wat er met een gelukt dictaat gebeurt.
pub(crate) fn kies_plan(
    alleen_klembord: bool,
    voorgrond: Option<Voorgrond>,
    eigen_integriteit: u32,
) -> Plan {
    if alleen_klembord {
        return Plan::AlleenKlembord;
    }
    let Some(voorgrond) = voorgrond else {
        return Plan::Niet(PlakUitkomst::GeenVenster);
    };
    if voorgrond.eigen_proces {
        return Plan::Overslaan;
    }
    if uipi_blokkeert(eigen_integriteit, voorgrond.integriteit) {
        return Plan::Niet(PlakUitkomst::Beheerrechten);
    }
    Plan::Plakken {
        doel_onzeker: doel_onzeker(eigen_integriteit, voorgrond.integriteit),
    }
}

// ---------------------------------------------------------------------------
// Het oude klembord bewaren
// ---------------------------------------------------------------------------

/// Nummers van standaardformaten die Windows zelf uit een ander formaat afleidt
/// (learn.microsoft.com/windows/win32/dataxchg/clipboard-formats, "Synthesized Clipboard
/// Formats").
pub(crate) mod formaat {
    pub(crate) const TEKST: u32 = 1;
    pub(crate) const BITMAP: u32 = 2;
    pub(crate) const METAFILE: u32 = 3;
    pub(crate) const OEM_TEKST: u32 = 7;
    pub(crate) const DIB: u32 = 8;
    pub(crate) const UNICODE_TEKST: u32 = 13;
    pub(crate) const ENH_METAFILE: u32 = 14;
    pub(crate) const HDROP: u32 = 15;
    pub(crate) const LANDINSTELLING: u32 = 16;
    pub(crate) const DIBV5: u32 = 17;
}

/// Welke formaten van het klembord we bewaren, in de volgorde van het klembord. Wat
/// Windows zelf afleidt, slaan we over: het opvragen ervan kost tijd (een DIBV5 uit een
/// grote schermafdruk wordt dan eerst omgezet) en Windows maakt het na het terugzetten
/// weer aan. Van tekst houden we Unicode, van DIB en DIBV5 het eerste, de bitmap alleen
/// zonder DIB en de oude metafile alleen zonder EMF.
pub(crate) fn te_bewaren(formaten: &[u32]) -> Vec<u32> {
    use formaat::{
        BITMAP, DIB, DIBV5, ENH_METAFILE, LANDINSTELLING, METAFILE, OEM_TEKST, TEKST, UNICODE_TEKST,
    };
    let heeft_unicode = formaten.contains(&UNICODE_TEKST);
    let heeft_emf = formaten.contains(&ENH_METAFILE);
    let eerste_dib = formaten.iter().copied().find(|f| *f == DIB || *f == DIBV5);
    formaten
        .iter()
        .copied()
        .filter(|f| match *f {
            TEKST | OEM_TEKST | LANDINSTELLING => !heeft_unicode,
            DIB | DIBV5 => Some(*f) == eerste_dib,
            BITMAP => eerste_dib.is_none(),
            METAFILE => !heeft_emf,
            _ => true,
        })
        .collect()
}

/// Geregistreerde formaten die de gebruiker zelf plakt: opmaak, webopmaak en afbeeldingen.
/// "PNG" zetten onder meer Chrome, Office en het Knipprogramma; "image/png" zet Firefox.
const MERKBARE_NAMEN: [&str; 4] = ["Rich Text Format", "HTML Format", "PNG", "image/png"];

/// Merkt de gebruiker het als dit formaat van het oude klembord niet terugkomt? Ja voor
/// tekst, opmaak, afbeeldingen en gekopieerde bestanden; nee voor de interne formaten van
/// programma's als Word en Excel (koppelingen, objectbeschrijvingen, eigen opmaak), want
/// plakken werkt daarna gewoon met de tekst of de opmaak. `naam` is de naam van een
/// geregistreerd formaat, als die er is.
///
/// Van tekst en afbeeldingen bewaart [`te_bewaren`] maar één vorm: de oude tekstformaten
/// en de bitmap alleen als er geen nieuwere vorm is. Komen die niet terug, dan is de tekst
/// of de afbeelding weg, dus ook die tellen mee.
pub(crate) fn merkbaar(formaat: u32, naam: Option<&str>) -> bool {
    use formaat::{BITMAP, DIB, DIBV5, HDROP, OEM_TEKST, TEKST, UNICODE_TEKST};
    match formaat {
        TEKST | OEM_TEKST | UNICODE_TEKST | BITMAP | DIB | DIBV5 | HDROP => true,
        _ => naam.is_some_and(|naam| {
            MERKBARE_NAMEN
                .iter()
                .any(|merkbaar| merkbaar.eq_ignore_ascii_case(naam))
        }),
    }
}

// ---------------------------------------------------------------------------
// Een plakactie: wachten en afronden
// ---------------------------------------------------------------------------

/// Wat er gebeurde sinds de tekst op het klembord kwam. Alles op de klembordthread.
#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "losse feiten uit verschillende berichten; geen toestandsmachine"
)]
pub(crate) struct Transactiestand {
    /// Wanneer de tekst (als belofte) op het klembord kwam.
    pub(crate) gepubliceerd: Instant,
    /// Wanneer Ctrl+V verstuurd werd. Alle grenzen tellen vanaf hier.
    pub(crate) ingevoegd: Option<Instant>,
    /// Ctrl+V kon niet verstuurd worden.
    pub(crate) invoeren_mislukt: bool,
    /// Wanneer een programma de tekst opvroeg.
    pub(crate) leesbewijzen: Vec<Instant>,
    /// Een ander programma nam het klembord over (de gebruiker kopieerde iets).
    pub(crate) eigendom_kwijt: bool,
    /// Een volgende opdracht of afsluiten rondt deze transactie nu af.
    pub(crate) afgebroken: bool,
    /// Was het niveau van het doelprogramma onbekend? Dan geldt [`GRENS_ONZEKER_DOEL`].
    pub(crate) doel_onzeker: bool,
}

impl Transactiestand {
    pub(crate) fn nieuw(nu: Instant, doel_onzeker: bool) -> Self {
        Self {
            gepubliceerd: nu,
            ingevoegd: None,
            invoeren_mislukt: false,
            leesbewijzen: Vec::new(),
            eigendom_kwijt: false,
            afgebroken: false,
            doel_onzeker,
        }
    }

    /// Het laatste leesbewijs na Ctrl+V, als dat er is.
    pub(crate) fn laatste_leesbewijs(&self) -> Option<Instant> {
        let ingevoegd = self.ingevoegd?;
        self.leesbewijzen
            .iter()
            .copied()
            .rev()
            .find(|moment| *moment >= ingevoegd)
    }

    /// Las een ander programma de tekst al vóór Ctrl+V (een klembordbeheerder, de
    /// gasttools van een virtuele machine)? Dan is de belofte al gerenderd en kan het doel
    /// geen eigen leesbewijs meer geven. Een vroege lezing is geen bewijs.
    pub(crate) fn vroeg_gelezen(&self) -> bool {
        match self.ingevoegd {
            Some(ingevoegd) => self.leesbewijzen.iter().any(|moment| *moment < ingevoegd),
            None => !self.leesbewijzen.is_empty(),
        }
    }

    /// De uitkomst voor de interface bij het afronden.
    pub(crate) fn uitkomst(&self) -> PlakUitkomst {
        if self.invoeren_mislukt {
            PlakUitkomst::Mislukt
        } else if self.laatste_leesbewijs().is_some() {
            PlakUitkomst::Geplakt
        } else if self.vroeg_gelezen() {
            // Niets stelligs: misschien plakte het doel de al gerenderde tekst.
            PlakUitkomst::WaarschijnlijkGeplakt
        } else if self.doel_onzeker {
            PlakUitkomst::NietGeplakt
        } else {
            PlakUitkomst::NietGelezen
        }
    }
}

/// Doorwachten of afronden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Afweging {
    Wachten,
    Afronden,
}

/// Wachten we nog op het doelprogramma, of ronden we af? Alle grenzen tellen vanaf
/// Ctrl+V. Is er een leesbewijs, dan geldt alleen nog de stilte na de laatste lezing.
pub(crate) fn afweging(stand: &Transactiestand, nu: Instant) -> Afweging {
    let klaar = |ja: bool| {
        if ja {
            Afweging::Afronden
        } else {
            Afweging::Wachten
        }
    };
    if stand.eigendom_kwijt || stand.afgebroken {
        return Afweging::Afronden;
    }
    let Some(ingevoegd) = stand.ingevoegd else {
        // Hoort niet voor te komen (Ctrl+V volgt direct op publiceren); dan toch begrensd.
        return klaar(nu.saturating_duration_since(stand.gepubliceerd) >= UITERLIJK_TERUGZETTEN);
    };
    if let Some(laatste) = stand.laatste_leesbewijs() {
        return klaar(nu.saturating_duration_since(laatste) >= STILTE_NA_LEZEN);
    }
    let grens = if stand.invoeren_mislukt {
        TERUGZETTEN_ZONDER_INVOER
    } else if stand.doel_onzeker && !stand.vroeg_gelezen() {
        GRENS_ONZEKER_DOEL
    } else {
        UITERLIJK_TERUGZETTEN
    };
    klaar(nu.saturating_duration_since(ingevoegd) >= grens)
}

/// Is het klembord nog van ons? Geen ander programma nam het over en het volgnummer
/// (`GetClipboardSequenceNumber`) is nog dat van onze laatste wijziging.
pub(crate) fn nog_van_ons(eigendom_kwijt: bool, volgnummer_nu: u32, volgnummer_eigen: u32) -> bool {
    !eigendom_kwijt && volgnummer_nu == volgnummer_eigen
}

/// Wat er bij het afronden met het klembord gebeurt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Afronding {
    /// Niets: we hebben het klembord nooit leeggemaakt, of de gebruiker kopieerde intussen
    /// zelf iets (dat wint).
    Niets,
    /// Het oude klembord terugzetten. Lukt dat niet, dan [`Afronding::Renderen`].
    Terugzetten,
    /// De tekst gewoon op het klembord laten staan: het oude klembord is niet bewaard.
    /// Liever het dictaat dan een leeg klembord of een lege belofte.
    Renderen,
}

/// De ene plek die beslist wat er met een klembord gebeurt dat we hebben aangeraakt: elk
/// pad dat het klembord leegmaakte, zet terug of rendert.
pub(crate) fn afronding(aangeraakt: bool, bewaard: bool, van_ons: bool) -> Afronding {
    if !aangeraakt || !van_ons {
        Afronding::Niets
    } else if bewaard {
        Afronding::Terugzetten
    } else {
        Afronding::Renderen
    }
}

/// Wat er gebeurt bij een volgende poging om het oude klembord terug te zetten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Herpogingstap {
    /// Nog eens proberen.
    Opnieuw,
    /// De gebruiker kopieerde intussen zelf iets: dat wint, het oude klembord vervalt.
    GebruikerWint,
    /// Na [`HERPOGING_GRENS`]: het dictaat op het klembord laten staan.
    LaatsteRedmiddel,
}

/// Beslist per poging, met `van_ons` uit [`nog_van_ons`] en de tijd sinds de eerste
/// mislukte poging.
pub(crate) fn herpogingstap(van_ons: bool, verstreken: Duration) -> Herpogingstap {
    if !van_ons {
        Herpogingstap::GebruikerWint
    } else if verstreken >= HERPOGING_GRENS {
        Herpogingstap::LaatsteRedmiddel
    } else {
        Herpogingstap::Opnieuw
    }
}

/// Wat er met het klembord van voor het plakken gebeurde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VorigKlembord {
    /// Bewaard en teruggezet, of nooit aangeraakt, of de gebruiker kopieerde zelf.
    Bewaard,
    /// Tekst, opmaak, een afbeelding of bestanden konden niet bewaard of teruggezet worden
    /// (te groot, onleesbaar of buiten het tijdbudget). Interne formaten van programma's
    /// tellen niet mee (zie [`merkbaar`]).
    Onvolledig,
    /// Het kon niet bewaard of niet teruggezet worden; het dictaat staat er nu.
    Kwijt,
}

/// Wat de gebruiker te horen krijgt na een geslaagd of onvolledig terugzetten.
pub(crate) fn vorig_na_terugzetten(
    bewaard_volledig: bool,
    teruggezet_volledig: bool,
) -> VorigKlembord {
    if bewaard_volledig && teruggezet_volledig {
        VorigKlembord::Bewaard
    } else {
        VorigKlembord::Onvolledig
    }
}

// ---------------------------------------------------------------------------
// Uitkomst en koppeling met de app
// ---------------------------------------------------------------------------

/// Hoe het plakken afliep, voor de melding in de interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PlakUitkomst {
    /// Geplakt, en het doelprogramma heeft de tekst na Ctrl+V opgevraagd.
    Geplakt,
    /// Een ander programma las de tekst al vóór Ctrl+V; of het doel plakte, is niet te
    /// zien.
    WaarschijnlijkGeplakt,
    /// Ctrl+V is verstuurd, maar er kwam geen leesbewijs.
    NietGelezen,
    /// Het niveau van het doelprogramma was onbekend en er kwam binnen een seconde na
    /// Ctrl+V niets gelezen: waarschijnlijk hield Windows de toetsen tegen.
    NietGeplakt,
    /// De terugval staat aan: de tekst staat op het klembord.
    OpKlembord,
    /// Het venster draait met beheerrechten en Mondig niet (UIPI).
    Beheerrechten,
    /// De gebruiker hield na de grens nog een modifier vast.
    ToetsenIngedrukt,
    /// Er was geen venster met de focus.
    GeenVenster,
    /// Het klembord was bezet of Windows weigerde iets.
    Mislukt,
    /// Niet geplakt omdat Mondig zelf (het venster of de pil) vooraan stond, of omdat
    /// plakken op dit systeem nog niet kan: de tekst staat in het venster van Mondig.
    InMondig,
}

/// Een uitkomst plus wat er met het vorige klembord gebeurde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Afloop {
    pub(crate) uitkomst: PlakUitkomst,
    pub(crate) vorig_klembord: VorigKlembord,
}

impl From<PlakUitkomst> for Afloop {
    fn from(uitkomst: PlakUitkomst) -> Self {
        Self {
            uitkomst,
            vorig_klembord: VorigKlembord::Bewaard,
        }
    }
}

/// Inhoud van het event `plak-uitslag`. Kan voor dezelfde taak een tweede keer komen, als
/// het terugzetten pas later definitief mislukt.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlakUitslag {
    taak: TaakId,
    uitkomst: PlakUitkomst,
    vorig_klembord: VorigKlembord,
}

fn meld(app: &AppHandle, taak: TaakId, afloop: Afloop) {
    let _ = app.emit(
        "plak-uitslag",
        PlakUitslag {
            taak,
            uitkomst: afloop.uitkomst,
            vorig_klembord: afloop.vorig_klembord,
        },
    );
}

/// Brengt de tekst van een gelukt dictaat bij de gebruiker: plakken, of met de terugval op
/// het klembord. Blokkeert hooguit tot de modifiers los zijn; de klembordthread doet de
/// rest. De uitkomst gaat als `plak-uitslag` naar de interface.
pub(crate) fn na_dictaat(app: &AppHandle, taak: TaakId, tekst: &str) {
    let alleen_klembord = crate::instellingen::lees_alleen_klembord(app);
    #[cfg(windows)]
    {
        let app = app.clone();
        win32::na_dictaat(tekst, alleen_klembord, move |afloop: Afloop| {
            meld(&app, taak, afloop);
        });
    }
    #[cfg(not(windows))]
    {
        // Plakken op macOS en Linux volgt in sprint 3e; de tekst staat in het venster. De
        // interface krijgt toch een afloop, zodat niets op "verwerkt" blijft hangen.
        let _ = (tekst, alleen_klembord);
        meld(app, taak, PlakUitkomst::InMondig.into());
    }
}

/// Bij afsluiten: een plakactie die nog loopt, wordt afgerond. Het oude klembord komt
/// terug; lukt dat niet, dan blijft het dictaat op het klembord staan.
pub(crate) fn afsluiten() {
    #[cfg(windows)]
    win32::afsluiten();
}

/// Zet tekst blijvend op het klembord, buiten de klembordgeschiedenis en de cloud. Voor de
/// knoppen "Kopiëren" en het menu in de systeembalk.
pub(crate) fn zet_op_klembord(tekst: &str) -> bool {
    #[cfg(windows)]
    {
        win32::zet_blijvend(tekst)
    }
    #[cfg(not(windows))]
    {
        let _ = tekst;
        false
    }
}

/// Kopieert tekst naar het klembord, buiten de geschiedenis. Geeft `false` als dat niet
/// lukte (de interface meldt dat dan in gewone taal).
#[tauri::command]
pub(crate) async fn klembord_zet(tekst: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || zet_op_klembord(&tekst))
        .await
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatie_achter_gewone_tekst() {
        assert_eq!(met_spatie("Hallo daar."), "Hallo daar. ");
    }

    #[test]
    fn geen_spatie_na_regeleinde_of_spatie() {
        assert_eq!(met_spatie("Regel\n"), "Regel\n");
        assert_eq!(met_spatie("Regel\r\n"), "Regel\r\n");
        assert_eq!(met_spatie("Al een spatie "), "Al een spatie ");
    }

    #[test]
    fn lege_tekst_blijft_leeg() {
        assert_eq!(met_spatie(""), "");
    }

    #[test]
    fn spatie_na_niet_ascii() {
        assert_eq!(met_spatie("Café"), "Café ");
    }

    #[test]
    fn modifiers_los_plakt_meteen() {
        assert_eq!(modifierwacht(false, Duration::ZERO), Modifierwacht::Plakken);
        assert_eq!(
            modifierwacht(false, MODIFIERS_GRENS * 2),
            Modifierwacht::Plakken
        );
    }

    #[test]
    fn modifier_ingedrukt_wacht_tot_de_grens() {
        assert_eq!(
            modifierwacht(true, Duration::from_millis(100)),
            Modifierwacht::Wachten
        );
        assert_eq!(modifierwacht(true, MODIFIERS_GRENS), Modifierwacht::Opgeven);
    }

    #[test]
    fn wachten_stopt_zodra_modifiers_los_zijn() {
        let mut keer = 0;
        let los = wacht_op_losse_modifiers(|| {
            keer += 1;
            keer < 3
        });
        assert!(los);
        assert_eq!(keer, 3);
    }

    const MIDDEN: u32 = 0x2000;
    const LAAG: u32 = 0x1000;
    const SYSTEEM: u32 = 0x4000;

    #[test]
    fn uipi_houdt_alleen_aantoonbaar_hoger_tegen() {
        assert!(uipi_blokkeert(
            MIDDEN,
            Integriteit::Niveau(INTEGRITEIT_HOOG)
        ));
        assert!(uipi_blokkeert(MIDDEN, Integriteit::Niveau(SYSTEEM)));
        assert!(!uipi_blokkeert(MIDDEN, Integriteit::Onleesbaar));
        assert!(!uipi_blokkeert(MIDDEN, Integriteit::Niveau(MIDDEN)));
        assert!(!uipi_blokkeert(MIDDEN, Integriteit::Niveau(LAAG)));
        assert!(!uipi_blokkeert(
            INTEGRITEIT_HOOG,
            Integriteit::Niveau(SYSTEEM)
        ));
    }

    #[test]
    fn onleesbaar_doel_is_onzeker() {
        assert!(doel_onzeker(MIDDEN, Integriteit::Onleesbaar));
        assert!(!doel_onzeker(MIDDEN, Integriteit::Niveau(INTEGRITEIT_HOOG)));
        assert!(!doel_onzeker(INTEGRITEIT_HOOG, Integriteit::Onleesbaar));
    }

    fn venster(integriteit: Integriteit) -> Voorgrond {
        Voorgrond {
            eigen_proces: false,
            integriteit,
        }
    }

    #[test]
    fn plannen() {
        assert_eq!(
            kies_plan(false, Some(venster(Integriteit::Niveau(MIDDEN))), MIDDEN),
            Plan::Plakken {
                doel_onzeker: false
            }
        );
        assert_eq!(
            kies_plan(false, Some(venster(Integriteit::Onleesbaar)), MIDDEN),
            Plan::Plakken { doel_onzeker: true }
        );
        assert_eq!(kies_plan(true, None, MIDDEN), Plan::AlleenKlembord);
        assert_eq!(
            kies_plan(
                false,
                Some(venster(Integriteit::Niveau(INTEGRITEIT_HOOG))),
                MIDDEN
            ),
            Plan::Niet(PlakUitkomst::Beheerrechten)
        );
        assert_eq!(
            kies_plan(false, None, MIDDEN),
            Plan::Niet(PlakUitkomst::GeenVenster)
        );
        let eigen = Voorgrond {
            eigen_proces: true,
            integriteit: Integriteit::Niveau(MIDDEN),
        };
        assert_eq!(kies_plan(false, Some(eigen), MIDDEN), Plan::Overslaan);
    }

    /// Een stand met Ctrl+V op `begin`.
    fn ingevoegd(begin: Instant, doel_onzeker: bool) -> Transactiestand {
        let mut stand = Transactiestand::nieuw(begin, doel_onzeker);
        stand.ingevoegd = Some(begin);
        stand
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn zonder_leesbewijs_acht_seconden_na_ctrl_v() {
        let begin = Instant::now();
        let mut stand = Transactiestand::nieuw(begin, false);
        // Publiceren kan even duren (bewaren); de grens telt pas vanaf Ctrl+V.
        stand.ingevoegd = Some(begin + ms(400));
        assert_eq!(
            afweging(&stand, begin + UITERLIJK_TERUGZETTEN),
            Afweging::Wachten
        );
        assert_eq!(
            afweging(&stand, begin + ms(400) + UITERLIJK_TERUGZETTEN),
            Afweging::Afronden
        );
        assert_eq!(stand.uitkomst(), PlakUitkomst::NietGelezen);
    }

    #[test]
    fn na_leesbewijs_alleen_de_stilte() {
        let begin = Instant::now();
        let mut stand = ingevoegd(begin, false);
        stand.leesbewijzen.push(begin + ms(50));
        stand.leesbewijzen.push(begin + ms(200));
        assert_eq!(afweging(&stand, begin + ms(300)), Afweging::Wachten);
        assert_eq!(
            afweging(&stand, begin + ms(200) + STILTE_NA_LEZEN),
            Afweging::Afronden
        );
        assert_eq!(stand.uitkomst(), PlakUitkomst::Geplakt);
    }

    #[test]
    fn leesbewijs_gaat_voor_de_korte_grenzen() {
        // Onzeker doel en een late, lange reeks lezingen: zolang er gelezen wordt, geldt
        // niet de grens van een seconde.
        let begin = Instant::now();
        let mut stand = ingevoegd(begin, true);
        stand.leesbewijzen.push(begin + ms(900));
        stand.leesbewijzen.push(begin + ms(1050));
        assert_eq!(afweging(&stand, begin + ms(1200)), Afweging::Wachten);
        assert_eq!(stand.uitkomst(), PlakUitkomst::Geplakt);
    }

    #[test]
    fn onzeker_doel_zonder_lezing_meldt_na_een_seconde() {
        let begin = Instant::now();
        let stand = ingevoegd(begin, true);
        assert_eq!(afweging(&stand, begin + ms(900)), Afweging::Wachten);
        assert_eq!(
            afweging(&stand, begin + GRENS_ONZEKER_DOEL),
            Afweging::Afronden
        );
        assert_eq!(stand.uitkomst(), PlakUitkomst::NietGeplakt);
    }

    #[test]
    fn vroege_lezing_is_geen_bewijs() {
        let begin = Instant::now();
        let mut stand = Transactiestand::nieuw(begin, false);
        stand.leesbewijzen.push(begin + ms(2));
        stand.ingevoegd = Some(begin + ms(10));
        assert!(stand.vroeg_gelezen());
        assert!(stand.laatste_leesbewijs().is_none());
        // Geen korte grens: wachten tot acht seconden of tot de gebruiker kopieert.
        assert_eq!(afweging(&stand, begin + ms(5000)), Afweging::Wachten);
        assert_eq!(
            afweging(&stand, begin + ms(10) + UITERLIJK_TERUGZETTEN),
            Afweging::Afronden
        );
        assert_eq!(stand.uitkomst(), PlakUitkomst::WaarschijnlijkGeplakt);
    }

    #[test]
    fn vroege_lezing_bij_onzeker_doel_meldt_niet_stellig() {
        let begin = Instant::now();
        let mut stand = Transactiestand::nieuw(begin, true);
        stand.leesbewijzen.push(begin + ms(2));
        stand.ingevoegd = Some(begin + ms(10));
        // Niet de grens van een seconde: er is gelezen, dus "niet geplakt" klopt misschien niet.
        assert_eq!(afweging(&stand, begin + ms(2000)), Afweging::Wachten);
        assert_eq!(stand.uitkomst(), PlakUitkomst::WaarschijnlijkGeplakt);
    }

    #[test]
    fn mislukte_invoer_zet_snel_terug() {
        let begin = Instant::now();
        let mut stand = ingevoegd(begin, false);
        stand.invoeren_mislukt = true;
        assert_eq!(
            afweging(&stand, begin + TERUGZETTEN_ZONDER_INVOER),
            Afweging::Afronden
        );
        assert_eq!(stand.uitkomst(), PlakUitkomst::Mislukt);
    }

    #[test]
    fn kopieren_of_afbreken_rondt_meteen_af() {
        let begin = Instant::now();
        let mut stand = ingevoegd(begin, false);
        stand.eigendom_kwijt = true;
        assert_eq!(afweging(&stand, begin), Afweging::Afronden);
        let mut stand = ingevoegd(begin, false);
        stand.afgebroken = true;
        assert_eq!(afweging(&stand, begin), Afweging::Afronden);
    }

    #[test]
    fn zonder_ctrl_v_toch_begrensd() {
        let begin = Instant::now();
        let stand = Transactiestand::nieuw(begin, false);
        assert_eq!(afweging(&stand, begin + ms(100)), Afweging::Wachten);
        assert_eq!(
            afweging(&stand, begin + UITERLIJK_TERUGZETTEN),
            Afweging::Afronden
        );
    }

    #[test]
    fn nog_van_ons_alleen_zonder_wijziging() {
        assert!(nog_van_ons(false, 41, 41));
        assert!(!nog_van_ons(false, 42, 41));
        assert!(!nog_van_ons(true, 41, 41));
    }

    /// Alle paden: een aangeraakt klembord dat nog van ons is, wordt teruggezet of
    /// gerenderd; in alle andere gevallen blijft het klembord zoals het is.
    #[test]
    fn afronding_alle_paden() {
        for aangeraakt in [false, true] {
            for bewaard in [false, true] {
                for van_ons in [false, true] {
                    let verwacht = match (aangeraakt, bewaard, van_ons) {
                        (true, true, true) => Afronding::Terugzetten,
                        (true, false, true) => Afronding::Renderen,
                        _ => Afronding::Niets,
                    };
                    assert_eq!(
                        afronding(aangeraakt, bewaard, van_ons),
                        verwacht,
                        "aangeraakt {aangeraakt}, bewaard {bewaard}, van ons {van_ons}"
                    );
                }
            }
        }
    }

    #[test]
    fn afgeleide_tekstformaten_overslaan() {
        use formaat::{LANDINSTELLING, OEM_TEKST, TEKST, UNICODE_TEKST};
        const RTF: u32 = 0xC0A0;
        assert_eq!(
            te_bewaren(&[RTF, UNICODE_TEKST, LANDINSTELLING, TEKST, OEM_TEKST]),
            vec![RTF, UNICODE_TEKST]
        );
        assert_eq!(te_bewaren(&[TEKST, OEM_TEKST]), vec![TEKST, OEM_TEKST]);
    }

    #[test]
    fn afgeleide_afbeeldingen_overslaan() {
        use formaat::{BITMAP, DIB, DIBV5, ENH_METAFILE, METAFILE};
        assert_eq!(te_bewaren(&[DIB, BITMAP, DIBV5]), vec![DIB]);
        assert_eq!(te_bewaren(&[DIBV5, DIB, BITMAP]), vec![DIBV5]);
        assert_eq!(te_bewaren(&[BITMAP]), vec![BITMAP]);
        assert_eq!(te_bewaren(&[ENH_METAFILE, METAFILE]), vec![ENH_METAFILE]);
    }

    #[test]
    fn tekst_opmaak_afbeelding_en_bestanden_zijn_merkbaar() {
        use formaat::{BITMAP, DIB, DIBV5, HDROP, OEM_TEKST, TEKST, UNICODE_TEKST};
        for formaat in [UNICODE_TEKST, TEKST, OEM_TEKST, DIB, DIBV5, BITMAP, HDROP] {
            assert!(merkbaar(formaat, None), "formaat {formaat}");
        }
        for naam in [
            "Rich Text Format",
            "HTML Format",
            "PNG",
            "image/png",
            "html format",
        ] {
            assert!(merkbaar(0xC0A0, Some(naam)), "{naam}");
        }
    }

    #[test]
    fn interne_formaten_van_office_zijn_niet_merkbaar() {
        use formaat::{ENH_METAFILE, LANDINSTELLING, METAFILE};
        for naam in [
            "Object Descriptor",
            "Link Source Descriptor",
            "Embed Source",
            "Biff12",
            "XML Spreadsheet",
            "Art::GVML ClipFormat",
            "Hyperlink",
        ] {
            assert!(!merkbaar(0xC0B0, Some(naam)), "{naam}");
        }
        // Een geregistreerd formaat zonder bekende naam, en wat Windows zelf afleidt.
        assert!(!merkbaar(0xC0C0, None));
        assert!(!merkbaar(ENH_METAFILE, None));
        assert!(!merkbaar(METAFILE, None));
        assert!(!merkbaar(LANDINSTELLING, None));
    }

    #[test]
    fn herpoging_tot_de_grens() {
        assert_eq!(herpogingstap(true, ms(500)), Herpogingstap::Opnieuw);
        assert_eq!(herpogingstap(true, ms(29_999)), Herpogingstap::Opnieuw);
        assert_eq!(
            herpogingstap(true, HERPOGING_GRENS),
            Herpogingstap::LaatsteRedmiddel
        );
    }

    #[test]
    fn herpoging_stopt_als_de_gebruiker_kopieert() {
        assert_eq!(herpogingstap(false, ms(10)), Herpogingstap::GebruikerWint);
        assert_eq!(
            herpogingstap(false, HERPOGING_GRENS * 2),
            Herpogingstap::GebruikerWint
        );
    }

    #[test]
    fn vorig_klembord_na_terugzetten() {
        assert_eq!(vorig_na_terugzetten(true, true), VorigKlembord::Bewaard);
        assert_eq!(vorig_na_terugzetten(false, true), VorigKlembord::Onvolledig);
        assert_eq!(vorig_na_terugzetten(true, false), VorigKlembord::Onvolledig);
    }
}
