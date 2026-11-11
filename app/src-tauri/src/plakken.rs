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
//! 5. Dicteren overal: komt er binnen [`GRENS_TYPEN`] geen leesbewijs, dan trekt Mondig de
//!    belofte in, zet het oude klembord terug en typt de tekst als Unicode-tekens (zie
//!    "Typen als terugval" hieronder). Lukt ook dat niet, dan staat de tekst op het klembord
//!    met een melding die de reden noemt.
//!
//! Dit bestand bevat de beslissingen als pure functies (met tests) en de koppeling met de
//! app. De Windows-kant staat in `plakken/win32.rs`: één vaste klembordthread die alle
//! opdrachten op volgorde afhandelt. De Mac-kant staat in `plakken/macos.rs` (mijlpaal M1),
//! met het klembord en ⌘V in de crate `mondig-mac`. Op Linux plakt Mondig nog niet: de tekst
//! blijft dan alleen in het venster.
//!
//! Gebeurtenis naar de interface: `plak-uitslag` met `{ taak, uitkomst,
//! vorigKlembordKwijt }`. De tekst zelf gaat nooit naar een log.

// Een deel van deze code is alleen voor Windows (UIPI, klembordformaten); op de Mac en Linux
// is dat ongebruikt.
#![cfg_attr(not(windows), allow(dead_code))]

use std::time::{Duration, Instant};

use mondig_core::taak::TaakId;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

// Overal gecompileerd: de tests lopen de volgorde op Windows na met een nep-klembord.
mod macos;
#[cfg(windows)]
mod win32;

/// Plakken op de Mac, met het echte klembord.
#[cfg(target_os = "macos")]
static MAC: macos::Plakker<mondig_mac::klembord::Systeem> =
    macos::Plakker::nieuw(mondig_mac::klembord::Systeem);

/// Na het laatste leesbewijs blijft de tekst nog zo lang op het klembord: sommige
/// programma's lezen meer dan eens per plakactie (Chromium kijkt eerst en leest dan).
/// Zelfde waarde als Handy (`QUIET_PERIOD`).
pub(crate) const STILTE_NA_LEZEN: Duration = Duration::from_millis(200);

/// Zonder leesbewijs komt het oude klembord zo lang na Ctrl+V terug (plan 3c).
pub(crate) const UITERLIJK_TERUGZETTEN: Duration = Duration::from_secs(8);

/// Komt er zo lang na Ctrl+V geen leesbewijs, dan typt Mondig de tekst (zie "Typen als
/// terugval"). Een programma dat reageert, vraagt de tekst binnen een paar honderd
/// milliseconden op; ook Word met een groot document en een sessie op afstand blijven daar
/// meestal ruim onder. Korter maakt de kans groter dat een traag programma pas na het
/// intrekken leest (het krijgt dan het oude klembord), langer laat de gebruiker wachten.
pub(crate) const GRENS_TYPEN: Duration = Duration::from_millis(1500);

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
    /// Niet plakken: de tekst op het klembord, met deze uitkomst als uitleg (de terugval van
    /// de gebruiker, of een venster met beheerrechten).
    Klembord(PlakUitkomst),
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
        return Plan::Klembord(PlakUitkomst::OpKlembord);
    }
    let Some(voorgrond) = voorgrond else {
        return Plan::Niet(PlakUitkomst::GeenVenster);
    };
    if voorgrond.eigen_proces {
        return Plan::Overslaan;
    }
    if uipi_blokkeert(eigen_integriteit, voorgrond.integriteit) {
        // UIPI houdt plakken en typen allebei tegen: de laatste terugval.
        return Plan::Klembord(PlakUitkomst::Beheerrechten);
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
    /// Mag Mondig typen als er geen leesbewijs komt? Uit in de tests zonder toetsen, en als
    /// het doel niet reageert (dan wacht Mondig tot [`UITERLIJK_TERUGZETTEN`]).
    pub(crate) typen_mag: bool,
    /// Een ander programma dan het doel las de tekst (Ditto, het Office-klembord van een
    /// ander programma): geen leesbewijs, maar de tekst staat daarna wel echt op het klembord.
    pub(crate) andere_lezer: bool,
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
            typen_mag: true,
            andere_lezer: false,
        }
    }

    /// Staat de tekst al echt op het klembord zonder leesbewijs van het doel? Zo als iemand
    /// hem vóór Ctrl+V las, of als een ander programma dan het doel hem las.
    pub(crate) fn al_gerenderd(&self) -> bool {
        self.vroeg_gelezen() || self.andere_lezer
    }

    /// Kan typen nog helpen als het leesbewijs uitblijft? Niet als Ctrl+V al niet verstuurd
    /// kon worden (typen gaat langs dezelfde weg), niet bij een onzeker doel (op Windows houdt
    /// UIPI typen net zo goed tegen, op de Mac beveiligde invoer), en niet als de tekst al
    /// echt op het klembord staat ([`Self::al_gerenderd`]): dan kan het doel hem plakken
    /// zonder dat we het zien, dus typen zou hem dubbel invoegen.
    pub(crate) fn typen_helpt(&self) -> bool {
        self.typen_mag && !self.invoeren_mislukt && !self.doel_onzeker && !self.al_gerenderd()
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
        } else if self.al_gerenderd() {
            // Niets stelligs: misschien plakte het doel de al gerenderde tekst.
            PlakUitkomst::WaarschijnlijkGeplakt
        } else if self.doel_onzeker {
            PlakUitkomst::NietGeplakt
        } else {
            PlakUitkomst::NietGelezen
        }
    }
}

/// Doorwachten, afronden of overstappen op typen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Afweging {
    Wachten,
    Afronden,
    /// Geen leesbewijs binnen [`GRENS_TYPEN`]: de belofte intrekken en de tekst typen.
    Typen,
}

/// Wachten we nog op het doelprogramma, ronden we af, of typen we? Alle grenzen tellen vanaf
/// Ctrl+V. Is er een leesbewijs, dan geldt alleen nog de stilte na de laatste lezing. Typen
/// kan alleen vóór [`UITERLIJK_TERUGZETTEN`]: lukt het intrekken tot dan niet (een ander
/// programma houdt het klembord vast), dan ronden we af zoals altijd.
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
    } else if stand.doel_onzeker && !stand.al_gerenderd() {
        GRENS_ONZEKER_DOEL
    } else {
        UITERLIJK_TERUGZETTEN
    };
    let verstreken = nu.saturating_duration_since(ingevoegd);
    if verstreken >= grens {
        Afweging::Afronden
    } else if stand.typen_helpt() && verstreken >= GRENS_TYPEN {
        Afweging::Typen
    } else {
        Afweging::Wachten
    }
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
    /// De tekst gewoon op het klembord laten staan: het oude klembord is niet bewaard, of de
    /// tekst kwam niet aan en de gebruiker moet hem zelf plakken. Liever het dictaat dan een
    /// leeg klembord of een lege belofte.
    Renderen,
}

/// De ene plek die beslist wat er met een klembord gebeurt dat we hebben aangeraakt: elk
/// pad dat het klembord leegmaakte, zet terug of rendert. `terug` zegt of het oude klembord
/// terug moet: het is bewaard, en de tekst kwam niet aantoonbaar níet aan (dan blijft hij
/// staan, als laatste terugval).
pub(crate) fn afronding(aangeraakt: bool, terug: bool, van_ons: bool) -> Afronding {
    if !aangeraakt || !van_ons {
        Afronding::Niets
    } else if terug {
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
// Typen als terugval
// ---------------------------------------------------------------------------
//
// Sommige programma's lezen het klembord niet na Ctrl+V of ⌘V: een webformulier dat plakken
// blokkeert, een terminal met een eigen plaktoets, een sessie op afstand die de toets anders
// doorgeeft. Komt er binnen [`GRENS_TYPEN`] geen leesbewijs, dan typt Mondig de tekst. Handy
// biedt dat als losse keuze "Direct Input" (handy.computer/docs/paste-methods, geraadpleegd
// 9-11-2026) en waarschuwt dat het tekens verhaspelt op AZERTY en QWERTZ: daar worden letters
// als toetscodes verstuurd. Mondig stuurt de tekens zelf (Windows `KEYEVENTF_UNICODE`, Mac
// `CGEventKeyboardSetUnicodeString`), dus de indeling doet er niet toe.
//
// **Alleen in een tekstveld.** Geen leesbewijs betekent vaak dat er geen tekstveld de focus
// heeft. Getypte letters worden daar sneltoetsen: in Gmail archiveert `e`, in de Verkenner kiest
// een letter een bestand. Mondig typt daarom alleen als het systeem aantoonbaar een bewerkbaar
// tekstveld met de focus meldt (Windows UI Automation, Mac het toegankelijkheidsrecht dat er al
// is); bij twijfel staat de tekst op het klembord met een melding. Reageert het doel niet, dan
// typt Mondig ook niet: dan wacht het tot [`UITERLIJK_TERUGZETTEN`], want een programma dat
// hangt, leest misschien alsnog. Regeleindes en tabs typen we als spatie: Enter verstuurt in
// een chat of formulier, Tab springt naar een ander veld.
//
// **Geen dubbele tekst, geen oud klembord in het doel.** Zodra Mondig besluit te typen, trekt
// het de belofte in en laat het klembord leeg, in één stap waarin geen lezing tussen kan komen
// (Windows: met het klembord open, waardoor geen ander programma kan lezen; Mac: op de
// hoofdthread, waar ook de leesverzoeken binnenkomen). Kwam er vlak daarvoor toch een lezing,
// dan typt Mondig niet en telt het gewoon als plakken. Plakt het doel tijdens het typen alsnog,
// dan krijgt het niets: niet het dictaat (dat zou dubbel staan) en niet het oude klembord (dat
// kan iets gevoeligs zijn). Het oude klembord komt pas na het typen terug, als het klembord dan
// nog van Mondig is; kopieerde de gebruiker intussen zelf, dan wint dat. Stopt het typen
// onderweg, dan komt de rest op het klembord in plaats van het oude. De gewone plakroute blijft
// even snel: dit alles gebeurt alleen als er níets gelezen wordt.

/// Deelt tekst op in blokken om te typen, met hooguit `max_eenheden` UTF-16-eenheden per
/// blok. Een teken buiten het basisvlak (een emoji) is twee eenheden, een surrogaatpaar, en
/// gaat nooit over twee blokken. Een regeleinde (`\r\n`, `\n`, `\r`) en een tab worden een
/// spatie (zie boven); andere stuurtekens typen we niet.
pub(crate) fn typstukken(tekst: &str, max_eenheden: usize) -> Vec<String> {
    // Een surrogaatpaar moet altijd in een blok passen.
    let max = max_eenheden.max(2);
    let mut stukken = Vec::new();
    let mut blok = String::new();
    let mut eenheden = 0;
    let mut tekens = tekst.chars().peekable();
    while let Some(teken) = tekens.next() {
        let teken = match teken {
            '\r' => {
                tekens.next_if_eq(&'\n');
                ' '
            }
            '\n' | '\t' => ' ',
            _ if teken.is_control() => continue,
            _ => teken,
        };
        let lengte = teken.len_utf16();
        if eenheden + lengte > max {
            stukken.push(std::mem::take(&mut blok));
            eenheden = 0;
        }
        blok.push(teken);
        eenheden += lengte;
    }
    if !blok.is_empty() {
        stukken.push(blok);
    }
    stukken
}

/// De tekst van de blokken die nog niet getypt zijn, voor op het klembord.
pub(crate) fn rest_van(stukken: &[String]) -> String {
    stukken.concat()
}

/// Wat Mondig doet als het leesbewijs uitblijft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Typbesluit {
    /// Het doel reageert niet: niet typen, wachten tot de gewone grens.
    Wachten,
    /// Geen tekstveld met de focus, of het is niet vast te stellen: de tekst op het klembord.
    Klembord,
    /// Een bewerkbaar tekstveld heeft de focus: typen.
    Typen,
}

/// Beslist of Mondig typt. `tekstveld` is `None` als het systeem het niet kon zeggen (een
/// fout of een time-out): bij twijfel niet typen.
pub(crate) fn typbesluit(reageert: bool, tekstveld: Option<bool>) -> Typbesluit {
    match (reageert, tekstveld) {
        (false, _) => Typbesluit::Wachten,
        (true, Some(true)) => Typbesluit::Typen,
        (true, _) => Typbesluit::Klembord,
    }
}

/// Het soort element met de focus, zoals UI Automation het noemt (Windows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Uiarol {
    /// `UIA_EditControlTypeId`: een invoerveld.
    Invoer,
    /// `UIA_DocumentControlTypeId`: een document, of een hele webpagina.
    Document,
    /// Al het andere: een lijst, een knop, een venster.
    Anders,
}

/// Wat Windows over het element met de focus zegt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Focusbeeld {
    pub(crate) rol: Uiarol,
    /// Heeft het een `ValuePattern`, met `Some(alleen_lezen)`.
    pub(crate) waarde_alleen_lezen: Option<bool>,
    /// Heeft het een `TextPattern`?
    pub(crate) tekstpatroon: bool,
    /// Heeft de thread van het venster een systeemcaret (`GetGUIThreadInfo`, `hwndCaret`)?
    pub(crate) caret: bool,
}

/// Heeft een bewerkbaar tekstveld de focus (Windows)? Een `ValuePattern` beslist als het er
/// is. Anders telt een invoerveld, en een document alleen met tekstpatroon én caret: een
/// webpagina als die van Gmail is ook een document met tekst, maar zonder caret. Een
/// systeemcaret alleen telt ook, voor oudere programma's zonder UI Automation.
pub(crate) fn is_tekstveld(beeld: Focusbeeld) -> bool {
    if let Some(alleen_lezen) = beeld.waarde_alleen_lezen {
        return !alleen_lezen;
    }
    match beeld.rol {
        Uiarol::Invoer => true,
        Uiarol::Document => beeld.tekstpatroon && beeld.caret,
        Uiarol::Anders => beeld.caret,
    }
}

/// Telt een lezing als leesbewijs? Alleen als het doel zelf las: het proces van het venster dat
/// bij Ctrl+V vooraan stond. Een klembordbeheerder of het klembord van een ander programma is
/// geen bewijs dat het doel plakte. Is een van beide onbekend, dan telt de lezing (de lezer
/// opende het klembord zonder venster, of het doel was niet vast te stellen).
pub(crate) fn lezer_is_doel(lezer: Option<u32>, doel: Option<u32>) -> bool {
    match (lezer, doel) {
        (Some(lezer), Some(doel)) => lezer == doel,
        _ => true,
    }
}

/// Waarom het typen stopte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stopreden {
    /// Er staat een ander venster of programma vooraan dan bij Ctrl+V: verder typen zou de
    /// tekst op de verkeerde plek zetten.
    AnderVenster,
    /// De gebruiker houdt een modifier vast; dan wordt een teken een sneltoets.
    Toetsen,
    /// Op de Mac staat beveiligde invoer aan: getypte tekens komen niet aan.
    Beveiligd,
    /// Het systeem nam de toetsen niet aan.
    Mislukt,
}

/// Hoe het typen afliep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Typafloop {
    /// Alles getypt.
    Klaar,
    /// Gestopt vóór blok `bij`; de rest staat nog niet in het doel.
    Gestopt { bij: usize, reden: Stopreden },
}

/// Typt de blokken een voor een. Vóór elk blok eerst `pauze` (behalve het eerste), zodat het
/// doel de vorige invoer kan verwerken en niets verloren gaat, en dan `stop`: wisselde de
/// gebruiker intussen van venster, of houdt hij een toets vast, dan stopt het typen. `stuur`
/// typt één blok en geeft `false` als het systeem het niet aannam.
pub(crate) fn typ_in_blokken(
    stukken: &[String],
    mut stop: impl FnMut() -> Option<Stopreden>,
    mut stuur: impl FnMut(&str) -> bool,
    mut pauze: impl FnMut(),
) -> Typafloop {
    for (index, stuk) in stukken.iter().enumerate() {
        if index > 0 {
            pauze();
        }
        if let Some(reden) = stop() {
            return Typafloop::Gestopt { bij: index, reden };
        }
        if !stuur(stuk) {
            return Typafloop::Gestopt {
                bij: index,
                reden: Stopreden::Mislukt,
            };
        }
    }
    Typafloop::Klaar
}

/// De uitkomst voor de interface na het typen. Elke stop onderweg (ander venster, een toets,
/// beveiligde invoer) is [`PlakUitkomst::TypenOnderbroken`]: wat al getypt is, staat er, en de
/// rest staat op het klembord. Was er nog niets getypt, dan staat de hele tekst op het
/// klembord en is het [`PlakUitkomst::TypenMislukt`].
pub(crate) fn uitkomst_na_typen(afloop: Typafloop) -> PlakUitkomst {
    match afloop {
        Typafloop::Klaar => PlakUitkomst::Getypt,
        Typafloop::Gestopt {
            reden: Stopreden::Mislukt,
            ..
        }
        | Typafloop::Gestopt { bij: 0, .. } => PlakUitkomst::TypenMislukt,
        Typafloop::Gestopt { .. } => PlakUitkomst::TypenOnderbroken,
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
    /// Ctrl+V niets gelezen: waarschijnlijk hield Windows de toetsen tegen. De tekst staat op
    /// het klembord.
    NietGeplakt,
    /// Het doel las het klembord niet, dus Mondig typte de tekst.
    Getypt,
    /// Mondig typte, maar stopte onderweg (ander venster, een toets ingedrukt, beveiligde
    /// invoer). Wat al getypt was, staat er; de rest staat op het klembord.
    TypenOnderbroken,
    /// Mondig probeerde te typen, maar het systeem nam de toetsen niet aan. De rest staat op
    /// het klembord.
    TypenMislukt,
    /// Er kwam geen leesbewijs en er had geen bewerkbaar tekstveld de focus (of dat was niet
    /// vast te stellen): niet getypt, de tekst staat op het klembord.
    GeenTekstveld,
    /// De terugval staat aan: de tekst staat op het klembord.
    OpKlembord,
    /// Het venster draait met beheerrechten en Mondig niet (UIPI): plakken en typen worden
    /// allebei tegengehouden. De tekst staat op het klembord.
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
    /// Op de Mac: Mondig heeft geen recht op toegankelijkheid, dus macOS zou ⌘V tegenhouden.
    /// De tekst staat op het klembord.
    #[cfg_attr(
        not(target_os = "macos"),
        allow(dead_code, reason = "alleen de Mac kent dit recht")
    )]
    GeenToegang,
    /// Op de Mac: na ⌘V werd niets gelezen terwijl beveiligde invoer aanstond (bijvoorbeeld
    /// een wachtwoordveld); dat is de waarschijnlijke oorzaak. De tekst staat op het klembord.
    #[cfg_attr(
        not(target_os = "macos"),
        allow(dead_code, reason = "alleen de Mac meldt beveiligde invoer")
    )]
    Wachtwoordveld,
}

impl PlakUitkomst {
    /// De naam zoals de interface hem kent (gelijk aan de serde-naam), voor het logboek.
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Geplakt => "geplakt",
            Self::WaarschijnlijkGeplakt => "waarschijnlijk_geplakt",
            Self::NietGelezen => "niet_gelezen",
            Self::NietGeplakt => "niet_geplakt",
            Self::Getypt => "getypt",
            Self::TypenOnderbroken => "typen_onderbroken",
            Self::TypenMislukt => "typen_mislukt",
            Self::GeenTekstveld => "geen_tekstveld",
            Self::OpKlembord => "op_klembord",
            Self::Beheerrechten => "beheerrechten",
            Self::ToetsenIngedrukt => "toetsen_ingedrukt",
            Self::GeenVenster => "geen_venster",
            Self::Mislukt => "mislukt",
            Self::InMondig => "in_mondig",
            Self::GeenToegang => "geen_toegang",
            Self::Wachtwoordveld => "wachtwoordveld",
        }
    }

    /// Langs welke route de tekst bij de gebruiker kwam: `plakken`, `typen`, `klembord` (de
    /// tekst staat op het klembord om zelf te plakken) of `geen` (alleen in Mondig). Bij typen
    /// dat onderweg stopte, staat de rest op het klembord; de uitkomst zegt dat.
    pub(crate) const fn route(self) -> &'static str {
        match self {
            Self::Geplakt | Self::WaarschijnlijkGeplakt | Self::NietGelezen => "plakken",
            Self::Getypt | Self::TypenOnderbroken | Self::TypenMislukt => "typen",
            Self::NietGeplakt
            | Self::GeenTekstveld
            | Self::OpKlembord
            | Self::Beheerrechten
            | Self::GeenToegang
            | Self::Wachtwoordveld => "klembord",
            Self::ToetsenIngedrukt | Self::GeenVenster | Self::Mislukt | Self::InMondig => "geen",
        }
    }
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
    // Per dictaat de route in het logboek, nooit de tekst.
    mondig_core::logboek::log(&mondig_core::logboek::Gebeurtenis::DictaatAfgeleverd {
        route: afloop.uitkomst.route(),
        uitkomst: afloop.uitkomst.code(),
    });
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
    #[cfg(target_os = "macos")]
    {
        let app = app.clone();
        MAC.na_dictaat(tekst, alleen_klembord, move |afloop: Afloop| {
            meld(&app, taak, afloop);
        });
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        // Plakken op Linux volgt later; de tekst staat in het venster. De interface krijgt
        // toch een afloop, zodat niets op "verwerkt" blijft hangen.
        let _ = (tekst, alleen_klembord);
        meld(app, taak, PlakUitkomst::InMondig.into());
    }
}

/// Bij afsluiten: een plakactie die nog loopt, wordt afgerond. Het oude klembord komt
/// terug; lukt dat niet, dan blijft het dictaat op het klembord staan.
pub(crate) fn afsluiten() {
    #[cfg(windows)]
    win32::afsluiten();
    #[cfg(target_os = "macos")]
    MAC.afsluiten();
}

/// Zet tekst blijvend op het klembord, buiten de klembordgeschiedenis en de cloud. Voor de
/// knoppen "Kopiëren" en het menu in de systeembalk.
pub(crate) fn zet_op_klembord(tekst: &str) -> bool {
    #[cfg(windows)]
    {
        win32::zet_blijvend(tekst)
    }
    #[cfg(target_os = "macos")]
    {
        MAC.zet_blijvend(tekst)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
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
        assert_eq!(
            kies_plan(true, None, MIDDEN),
            Plan::Klembord(PlakUitkomst::OpKlembord)
        );
        // Beheerrechten: plakken en typen gaan allebei niet, dus de tekst op het klembord.
        assert_eq!(
            kies_plan(
                false,
                Some(venster(Integriteit::Niveau(INTEGRITEIT_HOOG))),
                MIDDEN
            ),
            Plan::Klembord(PlakUitkomst::Beheerrechten)
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
        // Zonder typen (zoals in de tests zonder toetsen).
        stand.typen_mag = false;
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
            for terug in [false, true] {
                for van_ons in [false, true] {
                    let verwacht = match (aangeraakt, terug, van_ons) {
                        (true, true, true) => Afronding::Terugzetten,
                        (true, false, true) => Afronding::Renderen,
                        _ => Afronding::Niets,
                    };
                    assert_eq!(
                        afronding(aangeraakt, terug, van_ons),
                        verwacht,
                        "aangeraakt {aangeraakt}, terug {terug}, van ons {van_ons}"
                    );
                }
            }
        }
    }

    #[test]
    fn zonder_leesbewijs_na_de_grens_typen() {
        let begin = Instant::now();
        let stand = ingevoegd(begin, false);
        assert_eq!(afweging(&stand, begin + ms(1400)), Afweging::Wachten);
        assert_eq!(afweging(&stand, begin + GRENS_TYPEN), Afweging::Typen);
        // Lukt intrekken tot de uiterste grens niet, dan gewoon afronden.
        assert_eq!(
            afweging(&stand, begin + UITERLIJK_TERUGZETTEN),
            Afweging::Afronden
        );
    }

    #[test]
    fn een_lezing_voor_de_grens_is_gewoon_plakken() {
        let begin = Instant::now();
        let mut stand = ingevoegd(begin, false);
        stand.leesbewijzen.push(begin + ms(1400));
        assert_eq!(afweging(&stand, begin + GRENS_TYPEN), Afweging::Wachten);
        assert_eq!(
            afweging(&stand, begin + ms(1400) + STILTE_NA_LEZEN),
            Afweging::Afronden
        );
    }

    #[test]
    fn niet_typen_als_het_niet_kan_helpen() {
        let begin = Instant::now();
        let na = begin + GRENS_TYPEN;
        // Onzeker doel (UIPI of beveiligde invoer): na een seconde afronden, nooit typen.
        let onzeker = ingevoegd(begin, true);
        assert!(!onzeker.typen_helpt());
        assert_eq!(afweging(&onzeker, na), Afweging::Afronden);
        // Ctrl+V kon niet verstuurd worden.
        let mut mislukt = ingevoegd(begin, false);
        mislukt.invoeren_mislukt = true;
        assert_eq!(afweging(&mislukt, na), Afweging::Afronden);
        // Al vóór Ctrl+V gelezen: de tekst staat echt op het klembord, typen zou hem dubbel
        // zetten.
        let mut vroeg = Transactiestand::nieuw(begin, false);
        vroeg.leesbewijzen.push(begin);
        vroeg.ingevoegd = Some(begin + ms(10));
        assert_eq!(afweging(&vroeg, na + ms(10)), Afweging::Wachten);
        // De gebruiker kopieerde zelf: afronden, niet typen.
        let mut gekopieerd = ingevoegd(begin, false);
        gekopieerd.eigendom_kwijt = true;
        assert_eq!(afweging(&gekopieerd, na), Afweging::Afronden);
    }

    fn blokken(stukken: &[&str]) -> Vec<String> {
        stukken.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn blokken_van_hooguit_de_maat() {
        assert_eq!(
            typstukken("Hallo daar", 4),
            blokken(&["Hall", "o da", "ar"])
        );
        assert!(typstukken("", 4).is_empty());
    }

    #[test]
    fn een_surrogaatpaar_blijft_heel() {
        // De emoji is twee UTF-16-eenheden: hij past niet meer achter "abc" in een blok van 4.
        let stukken = typstukken("abc\u{1F600}d", 4);
        assert_eq!(stukken, blokken(&["abc", "\u{1F600}d"]));
        assert!(stukken.iter().all(|t| t.encode_utf16().count() <= 4));
        // Ook bij een te kleine maat: het paar gaat nooit over twee blokken.
        assert_eq!(
            typstukken("\u{1F600}\u{1F600}", 1),
            blokken(&["\u{1F600}", "\u{1F600}"])
        );
    }

    #[test]
    fn regeleindes_en_tabs_worden_een_spatie() {
        // Enter zou een chatbericht versturen, Tab naar een ander veld springen.
        assert_eq!(
            typstukken("Een\r\nTwee\nDrie\rVier\tvijf\u{7}!", 64),
            blokken(&["Een Twee Drie Vier vijf!"])
        );
        assert_eq!(typstukken("\n\n", 8), blokken(&["  "]));
    }

    #[test]
    fn accenten_en_tekens_van_andere_indelingen_gaan_als_teken() {
        // Wat op AZERTY of QWERTZ op een andere toets zit, gaat gewoon als teken mee.
        let tekst_in = "àéïöüç ß € æøå";
        assert_eq!(rest_van(&typstukken(tekst_in, 4)), tekst_in);
    }

    #[test]
    fn typen_tot_het_einde() {
        let stukken = typstukken("Hallo\nwereld", 3);
        let mut getypt = Vec::new();
        let mut pauzes = 0;
        let afloop = typ_in_blokken(
            &stukken,
            || None,
            |stuk| {
                getypt.push(stuk.to_string());
                true
            },
            || pauzes += 1,
        );
        assert_eq!(afloop, Typafloop::Klaar);
        assert_eq!(getypt, stukken);
        assert_eq!(getypt.concat(), "Hallo wereld");
        // Een pauze tussen elk blok, niet ervoor.
        assert_eq!(pauzes, stukken.len() - 1);
        assert_eq!(uitkomst_na_typen(afloop), PlakUitkomst::Getypt);
    }

    #[test]
    fn stoppen_bij_een_ander_venster() {
        let stukken = typstukken("Een twee drie vier", 4);
        let mut keer = 0;
        let mut getypt = 0;
        let afloop = typ_in_blokken(
            &stukken,
            || {
                keer += 1;
                // Na twee blokken staat er een ander venster vooraan.
                (keer > 2).then_some(Stopreden::AnderVenster)
            },
            |_| {
                getypt += 1;
                true
            },
            || {},
        );
        assert_eq!(
            afloop,
            Typafloop::Gestopt {
                bij: 2,
                reden: Stopreden::AnderVenster
            }
        );
        assert_eq!(getypt, 2);
        // Blokken van vier: "Een ", "twee", dan de rest.
        assert_eq!(rest_van(&stukken[2..]), " drie vier");
        assert_eq!(uitkomst_na_typen(afloop), PlakUitkomst::TypenOnderbroken);
    }

    #[test]
    fn elke_stop_onderweg_is_onderbroken_en_logt_als_typen() {
        let stukken = typstukken("Hallo", 2);
        let afloop = typ_in_blokken(&stukken, || None, |_| false, || {});
        assert_eq!(
            afloop,
            Typafloop::Gestopt {
                bij: 0,
                reden: Stopreden::Mislukt
            }
        );
        assert_eq!(uitkomst_na_typen(afloop), PlakUitkomst::TypenMislukt);
        for reden in [
            Stopreden::AnderVenster,
            Stopreden::Toetsen,
            Stopreden::Beveiligd,
        ] {
            let uitkomst = uitkomst_na_typen(Typafloop::Gestopt { bij: 1, reden });
            assert_eq!(uitkomst, PlakUitkomst::TypenOnderbroken, "{reden:?}");
            assert_eq!(uitkomst.route(), "typen");
            // Nog niets getypt: de hele tekst staat op het klembord.
            assert_eq!(
                uitkomst_na_typen(Typafloop::Gestopt { bij: 0, reden }),
                PlakUitkomst::TypenMislukt
            );
        }
    }

    #[test]
    fn typen_alleen_als_het_doel_reageert_en_er_een_tekstveld_is() {
        assert_eq!(typbesluit(true, Some(true)), Typbesluit::Typen);
        // Geen tekstveld, of niet vast te stellen: de tekst op het klembord.
        assert_eq!(typbesluit(true, Some(false)), Typbesluit::Klembord);
        assert_eq!(typbesluit(true, None), Typbesluit::Klembord);
        // Het doel hangt: wachten tot de gewone grens, nooit typen.
        assert_eq!(typbesluit(false, Some(true)), Typbesluit::Wachten);
        assert_eq!(typbesluit(false, None), Typbesluit::Wachten);
    }

    fn beeld(rol: Uiarol, waarde: Option<bool>, tekst: bool, caret: bool) -> Focusbeeld {
        Focusbeeld {
            rol,
            waarde_alleen_lezen: waarde,
            tekstpatroon: tekst,
            caret,
        }
    }

    #[test]
    fn een_bewerkbaar_veld_is_een_tekstveld() {
        // Kladblok, een zoekvak, een invoerveld in Chrome.
        assert!(is_tekstveld(beeld(Uiarol::Invoer, Some(false), true, true)));
        assert!(is_tekstveld(beeld(Uiarol::Invoer, None, false, false)));
        // Een bewerkbare keuzelijst of een ander element met een schrijfbare waarde.
        assert!(is_tekstveld(beeld(
            Uiarol::Anders,
            Some(false),
            false,
            false
        )));
        // Een document met caret (Word, een rich-text-veld) en een oud programma met caret.
        assert!(is_tekstveld(beeld(Uiarol::Document, None, true, true)));
        assert!(is_tekstveld(beeld(Uiarol::Anders, None, false, true)));
    }

    #[test]
    fn een_webpagina_of_bestandslijst_is_geen_tekstveld() {
        // Gmail of YouTube: een document met tekst, maar alleen-lezen en zonder caret.
        assert!(!is_tekstveld(beeld(
            Uiarol::Document,
            Some(true),
            true,
            false
        )));
        assert!(!is_tekstveld(beeld(Uiarol::Document, None, true, false)));
        // De Verkenner of het bureaublad: een lijst zonder caret.
        assert!(!is_tekstveld(beeld(Uiarol::Anders, None, false, false)));
        // Een alleen-lezen invoerveld.
        assert!(!is_tekstveld(beeld(Uiarol::Invoer, Some(true), true, true)));
    }

    #[test]
    fn alleen_het_doel_levert_een_leesbewijs() {
        assert!(lezer_is_doel(Some(42), Some(42)));
        // Ditto of een ander programma met een klembordvenster.
        assert!(!lezer_is_doel(Some(7), Some(42)));
        // Onbekend: de lezing telt (dan wordt er niet getypt, dus nooit dubbel).
        assert!(lezer_is_doel(None, Some(42)));
        assert!(lezer_is_doel(Some(7), None));
    }

    #[test]
    fn een_andere_lezer_houdt_typen_tegen() {
        let begin = Instant::now();
        let mut stand = ingevoegd(begin, false);
        stand.andere_lezer = true;
        assert!(!stand.typen_helpt());
        assert_eq!(afweging(&stand, begin + GRENS_TYPEN), Afweging::Wachten);
        assert_eq!(stand.uitkomst(), PlakUitkomst::WaarschijnlijkGeplakt);
    }

    #[test]
    fn de_code_is_de_naam_voor_de_interface() {
        for uitkomst in [
            PlakUitkomst::Geplakt,
            PlakUitkomst::WaarschijnlijkGeplakt,
            PlakUitkomst::Getypt,
            PlakUitkomst::TypenOnderbroken,
            PlakUitkomst::TypenMislukt,
            PlakUitkomst::GeenTekstveld,
            PlakUitkomst::Beheerrechten,
            PlakUitkomst::GeenToegang,
            PlakUitkomst::Wachtwoordveld,
            PlakUitkomst::InMondig,
        ] {
            let json = serde_json::to_string(&uitkomst).unwrap_or_default();
            assert_eq!(json, format!("\"{}\"", uitkomst.code()));
        }
        assert_eq!(PlakUitkomst::Getypt.route(), "typen");
        assert_eq!(PlakUitkomst::Geplakt.route(), "plakken");
        assert_eq!(PlakUitkomst::Beheerrechten.route(), "klembord");
        assert_eq!(PlakUitkomst::GeenTekstveld.route(), "klembord");
        assert_eq!(PlakUitkomst::InMondig.route(), "geen");
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
