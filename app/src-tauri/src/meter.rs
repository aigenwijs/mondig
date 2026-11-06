//! Niveaumeters zonder opname (fase UX-C): het niveau van de gekozen microfoon en van het
//! geluid dat het gekozen afspeelapparaat afspeelt, voor de meters bij de apparaatkeuze.
//! Er wordt niets bewaard: de bron geeft alleen een piek door (`Niveaumeting` in
//! `mondig-core`), op dezelfde schaal als de meters tijdens een vergaderopname.
//!
//! Commando's en events:
//!
//! - `meter_start { bron, apparaat }`: `bron` is `"microfoon"` of `"systeem"`, `apparaat`
//!   een id uit `microfoon_lijst` of `vergadering_afspeelapparaten`, of `null` voor de
//!   standaard. Een eerdere meter van dezelfde bron stopt. Antwoordt als de bron open is.
//! - `meter_hartslag { bron }`: de interface bevestigt dat de meter nog in beeld is. Geeft
//!   `false` als er (niet meer) een meter loopt. Zonder bevestiging binnen
//!   [`HARTSLAG_TERMIJN`] stopt de app de meter zelf, zodat de microfoon niet open blijft
//!   als de interface vergeet te stoppen of het venster verborgen is.
//! - `meter_stop { bron }`: stopt de meter van die bron, of allebei bij `null`.
//! - event `meter-niveau` `{ bron, niveau }`: twintig keer per seconde, 0 tot 1. Speelt de
//!   computer niets af, dan is het niveau van het systeemgeluid 0.
//! - event `meter-einde` `{ bron, reden, fout }`: de app stopte de meter zelf. `reden` is
//!   `"fout"` (de bron viel uit, `fout` zegt waarom), `"hartslag"` (geen bevestiging),
//!   `"voorrang"` (een dictaat of vergaderopname begon) of `"venster"` (het venster sloot).
//!   Na `meter_stop` komt dit event niet.
//!
//! Voorrang: een dictaat of vergaderopname wint. Loopt er een, dan weigert `meter_start`;
//! begint er een, dan stoppen alle meters (`stop_alle`). Dezelfde volgorde als bij de
//! microfoontest: eerst de generatie claimen, dan kijken, zodat minstens een van beide de
//! ander ziet.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use mondig_core::opname::handy::HandyApparatuur;
use mondig_core::opname::niveau::Niveaumeting;
use mondig_core::opname::systeem::SysteemApparatuur;
use mondig_core::opname::{Apparatuur, opgeslagen_microfoon};
use mondig_core::taak::{Foutmelding, Vervolg};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::dicteerketen::{DicteerStaat, slot};

/// Zo vaak gaat het niveau naar de interface: twintig keer per seconde.
pub(crate) const TIK: Duration = Duration::from_millis(50);

/// Zo lang mag de interface de meter niet bevestigen voordat de app hem zelf stopt.
pub(crate) const HARTSLAG_TERMIJN: Duration = Duration::from_secs(5);

/// Wat een meter meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Bron {
    /// De gekozen microfoon.
    Microfoon,
    /// Het geluid dat het gekozen afspeelapparaat afspeelt.
    Systeem,
}

impl Bron {
    const ALLE: [Self; 2] = [Self::Microfoon, Self::Systeem];

    fn index(self) -> usize {
        match self {
            Self::Microfoon => 0,
            Self::Systeem => 1,
        }
    }
}

/// Waarom de app een meter zelf stopte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Reden {
    /// De bron viel uit.
    Fout,
    /// De interface bevestigde de meter niet op tijd.
    Hartslag,
    /// Een dictaat of vergaderopname begon.
    Voorrang,
    /// Het venster sloot of werd verborgen.
    Venster,
}

/// Inhoud van het event `meter-niveau`.
#[derive(Debug, Clone, Copy, Serialize)]
struct NiveauEvent {
    bron: Bron,
    niveau: f32,
}

/// Inhoud van het event `meter-einde`.
#[derive(Debug, Clone, Serialize)]
struct EindeEvent {
    bron: Bron,
    reden: Reden,
    fout: Option<Foutmelding>,
}

/// Wanneer de interface de meter voor het laatst bevestigde.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Hartslag {
    laatst: Instant,
    termijn: Duration,
}

impl Hartslag {
    pub(crate) fn nieuw(nu: Instant, termijn: Duration) -> Self {
        Self {
            laatst: nu,
            termijn,
        }
    }

    pub(crate) fn bevestig(&mut self, nu: Instant) {
        self.laatst = self.laatst.max(nu);
    }

    /// Is de termijn verstreken sinds de laatste bevestiging?
    pub(crate) fn verlopen(&self, nu: Instant) -> bool {
        nu.saturating_duration_since(self.laatst) > self.termijn
    }
}

/// Een lopende meter met zijn hartslag.
struct Lopend<T> {
    meting: T,
    hartslag: Hartslag,
}

/// Wat de tikker moet doen.
pub(crate) enum Tik<T> {
    /// De meter is ingehaald of gestopt: de tikker stopt zonder iets te melden.
    Klaar,
    /// De hartslag is verlopen: de meting is uit de plek genomen en moet stoppen.
    Verlopen(T),
    /// Het niveau van nu.
    Niveau(f32),
}

/// De plek van een meter per bron, met een teller waarmee een meter die nog opent kan zien
/// dat hij intussen is ingehaald (gestopt, nieuwe start, begin van een dictaat). Los van
/// Tauri en de bron, zodat de regels zonder hardware te testen zijn.
pub(crate) struct Plek<T> {
    lopend: Mutex<Option<Lopend<T>>>,
    generatie: AtomicU64,
}

impl<T> Default for Plek<T> {
    fn default() -> Self {
        Self {
            lopend: Mutex::new(None),
            generatie: AtomicU64::new(0),
        }
    }
}

impl<T> Plek<T> {
    /// Claimt de plek voor een nieuwe meter; een eerdere meter (of een die nog opent) is
    /// daarmee ingehaald. Geeft de generatie van de nieuwe meter en de vorige meting, die de
    /// aanroeper moet stoppen.
    pub(crate) fn claim(&self) -> (u64, Option<T>) {
        let generatie = self.generatie.fetch_add(1, Ordering::SeqCst) + 1;
        let vorige = slot(&self.lopend).take().map(|l| l.meting);
        (generatie, vorige)
    }

    /// Is `generatie` nog de huidige?
    pub(crate) fn is_huidig(&self, generatie: u64) -> bool {
        self.generatie.load(Ordering::SeqCst) == generatie
    }

    /// Zet een geopende meting op de plek, als die intussen niet is ingehaald. Anders komt
    /// de meting terug, om te stoppen.
    pub(crate) fn plaats(&self, generatie: u64, meting: T, nu: Instant) -> Result<(), T> {
        let mut lopend = slot(&self.lopend);
        if !self.is_huidig(generatie) {
            return Err(meting);
        }
        *lopend = Some(Lopend {
            meting,
            hartslag: Hartslag::nieuw(nu, HARTSLAG_TERMIJN),
        });
        Ok(())
    }

    /// Haalt de meter weg (stop, voorrang, uitval); ook een meter die nog opent is daarmee
    /// ingehaald.
    pub(crate) fn neem(&self) -> Option<T> {
        self.generatie.fetch_add(1, Ordering::SeqCst);
        slot(&self.lopend).take().map(|l| l.meting)
    }

    /// Haalt de meter van `generatie` weg, alleen als die nog de huidige is (uitval van een
    /// bron die al vervangen is, stopt de nieuwe niet).
    pub(crate) fn neem_als(&self, generatie: u64) -> Option<T> {
        let mut lopend = slot(&self.lopend);
        if !self.is_huidig(generatie) {
            return None;
        }
        self.generatie.fetch_add(1, Ordering::SeqCst);
        lopend.take().map(|l| l.meting)
    }

    /// Bevestigt de hartslag. Geeft `false` als er geen meter loopt.
    pub(crate) fn bevestig(&self, nu: Instant) -> bool {
        match slot(&self.lopend).as_mut() {
            Some(lopend) => {
                lopend.hartslag.bevestig(nu);
                true
            }
            None => false,
        }
    }

    /// Eén tik van de meter van `generatie`: het niveau, of stoppen.
    pub(crate) fn tik(&self, generatie: u64, nu: Instant, meet: impl FnOnce(&T) -> f32) -> Tik<T> {
        let mut lopend = slot(&self.lopend);
        if !self.is_huidig(generatie) {
            return Tik::Klaar;
        }
        let Some(huidig) = lopend.as_ref() else {
            return Tik::Klaar;
        };
        if huidig.hartslag.verlopen(nu) {
            self.generatie.fetch_add(1, Ordering::SeqCst);
            return lopend
                .take()
                .map_or(Tik::Klaar, |l| Tik::Verlopen(l.meting));
        }
        Tik::Niveau(meet(&huidig.meting).clamp(0.0, 1.0))
    }
}

/// De meters van de app, een per bron.
#[derive(Default)]
pub(crate) struct MeterStaat {
    plekken: [Plek<Niveaumeting>; 2],
}

impl MeterStaat {
    fn plek(&self, bron: Bron) -> &Plek<Niveaumeting> {
        &self.plekken[bron.index()]
    }
}

/// Mag een meter nu starten? Een dictaat of vergaderopname gaat voor.
pub(crate) fn mag_starten(dictaat_bezig: bool, vergadering_loopt: bool) -> Result<(), Foutmelding> {
    if dictaat_bezig {
        return Err(Foutmelding::nieuw(
            "Er loopt een dictaat. De meter doet het weer als het dictaat klaar is.",
            Vervolg::OpnieuwProberen,
        ));
    }
    if vergadering_loopt {
        return Err(Foutmelding::nieuw(
            "Er loopt een opname. De meter doet het weer als de opname gestopt is.",
            Vervolg::Geen,
        ));
    }
    Ok(())
}

/// Start de meter van `bron` op `apparaat` (`None`: de standaard) en stopt een eerdere
/// meter van dezelfde bron. Het openen kan bij een Bluetooth-headset seconden duren; het
/// antwoord komt als de bron open is.
#[tauri::command]
pub(crate) async fn meter_start(
    app: AppHandle,
    bron: Bron,
    apparaat: Option<String>,
) -> Result<(), Foutmelding> {
    tauri::async_runtime::spawn_blocking(move || start(&app, bron, apparaat.as_deref()))
        .await
        .map_err(|_| Foutmelding::nieuw("De meter stopte onverwacht.", Vervolg::OpnieuwProberen))?
}

fn start(app: &AppHandle, bron: Bron, apparaat: Option<&str>) -> Result<(), Foutmelding> {
    let staat = app.state::<MeterStaat>();
    let plek = staat.plek(bron);
    // Eerst claimen, dan kijken: een dictaat of vergadering die daarna begint, verhoogt de
    // generatie (`stop_alle`), en dan gooit `plaats` de meting weg.
    let (generatie, vorige) = plek.claim();
    drop(vorige);
    mag_starten(
        app.state::<DicteerStaat>()
            .dictaat_bezig
            .load(Ordering::SeqCst),
        crate::vergadering::loopt(app),
    )?;

    let voor_uitval = app.clone();
    let bij_uitval = move |fout: Foutmelding| {
        // De bron meldt dit op zijn eigen thread; stoppen mag daar niet.
        thread::spawn(move || {
            let staat = voor_uitval.state::<MeterStaat>();
            if let Some(meting) = staat.plek(bron).neem_als(generatie) {
                drop(meting);
                let fout = match bron {
                    Bron::Microfoon => crate::microfoonrecht::verfijn(fout, false),
                    Bron::Systeem => fout,
                };
                meld_einde(&voor_uitval, bron, Reden::Fout, Some(fout));
            }
        });
    };
    let meting = match bron {
        Bron::Microfoon => {
            // Een microfoon die er niet meer is, valt terug op de standaard: de meter hoort
            // te meten wat een dictaat zou gebruiken.
            let gekozen = match HandyApparatuur.microfoons() {
                Ok(lijst) => opgeslagen_microfoon(apparaat, &lijst),
                Err(_) => apparaat.map(str::to_string),
            };
            Niveaumeting::microfoon(&HandyApparatuur, gekozen.as_deref(), bij_uitval)
                .map_err(|fout| crate::microfoonrecht::verfijn(fout, false))?
        }
        Bron::Systeem => Niveaumeting::systeem(&SysteemApparatuur, apparaat, bij_uitval)?,
    };
    if let Err(ingehaald) = plek.plaats(generatie, meting, Instant::now()) {
        // Ingehaald terwijl de bron opende: weggooien, de nieuwe eigenaar bepaalt.
        drop(ingehaald);
        return Ok(());
    }
    let voor_tikker = app.clone();
    thread::Builder::new()
        .name("mondig-meter".into())
        .spawn(move || tikker(&voor_tikker, bron, generatie))
        .map_err(|_| {
            drop(plek.neem());
            Foutmelding::nieuw("De meter kon niet starten.", Vervolg::OpnieuwProberen)
        })?;
    Ok(())
}

/// Twintig keer per seconde het niveau naar de interface, tot de meter stopt of de hartslag
/// verloopt.
fn tikker(app: &AppHandle, bron: Bron, generatie: u64) {
    loop {
        thread::sleep(TIK);
        let staat = app.state::<MeterStaat>();
        match staat
            .plek(bron)
            .tik(generatie, Instant::now(), Niveaumeting::neem_niveau)
        {
            Tik::Klaar => return,
            Tik::Verlopen(meting) => {
                drop(meting);
                meld_einde(app, bron, Reden::Hartslag, None);
                return;
            }
            Tik::Niveau(niveau) => {
                let _ = app.emit("meter-niveau", NiveauEvent { bron, niveau });
            }
        }
    }
}

fn meld_einde(app: &AppHandle, bron: Bron, reden: Reden, fout: Option<Foutmelding>) {
    let _ = app.emit("meter-einde", EindeEvent { bron, reden, fout });
}

/// De interface bevestigt dat de meter van `bron` nog in beeld is. Geeft `false` als er geen
/// meter loopt (gestopt door de app, of nooit gestart).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri geeft een commando de argumenten per waarde mee; een verwijzing kan niet"
)]
#[tauri::command]
pub(crate) fn meter_hartslag(app: AppHandle, bron: Bron) -> bool {
    app.state::<MeterStaat>()
        .plek(bron)
        .bevestig(Instant::now())
}

/// Stopt de meter van `bron`, of allebei bij `None` (het scherm verdwijnt of wordt verborgen).
#[tauri::command]
pub(crate) async fn meter_stop(app: AppHandle, bron: Option<Bron>) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let staat = app.state::<MeterStaat>();
        for b in Bron::ALLE {
            if bron.is_none_or(|gekozen| gekozen == b) {
                drop(staat.plek(b).neem());
            }
        }
    })
    .await;
}

/// Stopt alle meters namens de app (dictaat of vergadering begint, venster sluit) en meldt
/// dat aan de interface.
pub(crate) fn stop_alle(app: &AppHandle, reden: Reden) {
    let Some(staat) = app.try_state::<MeterStaat>() else {
        return;
    };
    for bron in Bron::ALLE {
        if let Some(meting) = staat.plek(bron).neem() {
            drop(meting);
            meld_einde(app, bron, reden, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use mondig_core::opname::niveau::niveau_van_rms;

    use super::*;

    fn meet_vast(_: &&str) -> f32 {
        0.5
    }

    #[test]
    fn de_hartslag_verloopt_pas_na_de_termijn() {
        let begin = Instant::now();
        let mut hartslag = Hartslag::nieuw(begin, HARTSLAG_TERMIJN);
        assert!(!hartslag.verlopen(begin + HARTSLAG_TERMIJN));
        assert!(hartslag.verlopen(begin + HARTSLAG_TERMIJN + Duration::from_millis(1)));
        // Een bevestiging schuift de termijn op; een oudere tijd zet hem niet terug.
        hartslag.bevestig(begin + Duration::from_secs(4));
        hartslag.bevestig(begin + Duration::from_secs(1));
        assert!(!hartslag.verlopen(begin + Duration::from_secs(9)));
        assert!(hartslag.verlopen(begin + Duration::from_millis(9_001)));
    }

    #[test]
    fn zonder_hartslag_stopt_de_tikker_de_meter_zelf() {
        let plek = Plek::<&str>::default();
        let begin = Instant::now();
        let (generatie, _) = plek.claim();
        plek.plaats(generatie, "microfoon", begin).unwrap();
        assert!(matches!(
            plek.tik(generatie, begin + Duration::from_secs(1), meet_vast),
            Tik::Niveau(n) if (n - 0.5).abs() < f32::EPSILON
        ));
        assert!(matches!(
            plek.tik(generatie, begin + Duration::from_secs(6), meet_vast),
            Tik::Verlopen("microfoon")
        ));
        // Daarna is de plek leeg en de tikker klaar; een hartslag komt te laat.
        assert!(matches!(
            plek.tik(generatie, begin + Duration::from_secs(6), meet_vast),
            Tik::Klaar
        ));
        assert!(!plek.bevestig(begin + Duration::from_secs(7)));
    }

    #[test]
    fn met_hartslag_blijft_de_meter_lopen() {
        let plek = Plek::<&str>::default();
        let begin = Instant::now();
        let (generatie, _) = plek.claim();
        plek.plaats(generatie, "systeem", begin).unwrap();
        for s in 1..=20 {
            let nu = begin + Duration::from_secs(s);
            assert!(plek.bevestig(nu));
            assert!(matches!(plek.tik(generatie, nu, meet_vast), Tik::Niveau(_)));
        }
    }

    #[test]
    fn het_niveau_blijft_binnen_nul_en_een() {
        let plek = Plek::<&str>::default();
        let begin = Instant::now();
        let (generatie, _) = plek.claim();
        plek.plaats(generatie, "x", begin).unwrap();
        assert!(
            matches!(plek.tik(generatie, begin, |_| 3.0), Tik::Niveau(n) if (n - 1.0).abs() < f32::EPSILON)
        );
        assert!(
            matches!(plek.tik(generatie, begin, |_| -1.0), Tik::Niveau(n) if n.abs() < f32::EPSILON)
        );
    }

    #[test]
    fn een_dictaat_of_vergadering_gaat_voor() {
        assert!(mag_starten(false, false).is_ok());
        assert!(
            mag_starten(true, false)
                .unwrap_err()
                .melding
                .contains("dictaat")
        );
        assert!(
            mag_starten(false, true)
                .unwrap_err()
                .melding
                .contains("opname")
        );
    }

    #[test]
    fn een_meter_die_nog_opent_en_wordt_ingehaald_gaat_weg() {
        // Zoals een dictaat dat begint terwijl de microfoon van de meter nog opent: `stop_alle`
        // neemt de plek, de meting die daarna klaar is wordt teruggegeven om te stoppen.
        let plek = Plek::<&str>::default();
        let (generatie, _) = plek.claim();
        assert!(plek.neem().is_none());
        assert_eq!(plek.plaats(generatie, "laat", Instant::now()), Err("laat"));
        assert!(matches!(
            plek.tik(generatie, Instant::now(), meet_vast),
            Tik::Klaar
        ));
    }

    #[test]
    fn voorrang_stopt_een_lopende_meter_en_de_tikker() {
        let plek = Plek::<&str>::default();
        let (generatie, _) = plek.claim();
        plek.plaats(generatie, "loopt", Instant::now()).unwrap();
        assert_eq!(plek.neem(), Some("loopt"));
        assert!(matches!(
            plek.tik(generatie, Instant::now(), meet_vast),
            Tik::Klaar
        ));
    }

    #[test]
    fn een_nieuwe_start_vervangt_de_vorige_en_oude_uitval_raakt_de_nieuwe_niet() {
        let plek = Plek::<&str>::default();
        let (oud, _) = plek.claim();
        plek.plaats(oud, "oud", Instant::now()).unwrap();
        let (nieuw, vorige) = plek.claim();
        assert_eq!(vorige, Some("oud"));
        plek.plaats(nieuw, "nieuw", Instant::now()).unwrap();
        // De oude tikker stopt, uitval van de oude bron laat de nieuwe staan.
        assert!(matches!(
            plek.tik(oud, Instant::now(), meet_vast),
            Tik::Klaar
        ));
        assert_eq!(plek.neem_als(oud), None);
        assert_eq!(plek.neem_als(nieuw), Some("nieuw"));
    }

    #[test]
    fn de_schaal_is_die_van_de_meters_tijdens_de_opname() {
        // Piek van -30 dBFS is de helft van de meter, -60 dBFS en stiller is 0, vol is 1:
        // op tien streepjes is -30 dBFS vijf streepjes.
        assert!((niveau_van_rms(0.031_622_78) - 0.5).abs() < 0.001);
        assert!(niveau_van_rms(0.001).abs() < f32::EPSILON);
        assert!((niveau_van_rms(1.0) - 1.0).abs() < f32::EPSILON);
        // Het testgeluid (zie geluid.rs) komt rond zes streepjes uit.
        let testgeluid = niveau_van_rms(crate::geluid::uitsturing(crate::geluid::STANDAARD_VOLUME));
        assert!((0.6..0.7).contains(&testgeluid), "{testgeluid}");
    }

    #[test]
    fn de_events_hebben_de_vorm_voor_de_interface() {
        let niveau = serde_json::to_value(NiveauEvent {
            bron: Bron::Systeem,
            niveau: 0.25,
        })
        .unwrap();
        assert_eq!(
            niveau,
            serde_json::json!({ "bron": "systeem", "niveau": 0.25 })
        );
        let einde = serde_json::to_value(EindeEvent {
            bron: Bron::Microfoon,
            reden: Reden::Hartslag,
            fout: None,
        })
        .unwrap();
        assert_eq!(
            einde,
            serde_json::json!({ "bron": "microfoon", "reden": "hartslag", "fout": null })
        );
        let bron: Bron = serde_json::from_str("\"microfoon\"").unwrap();
        assert_eq!(bron, Bron::Microfoon);
    }
}
