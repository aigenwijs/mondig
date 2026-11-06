//! Hulpmiddelen voor het werkproces: de berichtenlus over stdin en stdout.
//!
//! Het echte werkproces (`mondig-engine`) en het nep-werkproces voor tests
//! (`nep-engine`) gebruiken dezelfde lus en verschillen alleen in wat een [`Werker`]
//! doet met een opdracht.
//!
//! Stdin wordt op een eigen draad gelezen, zodat een [`Verzoek::Annuleer`] aankomt
//! terwijl een bestand loopt. De werker ziet dat via [`Annulering`]: tussen twee stukken
//! met [`Annulering::is_geannuleerd`], en midden in een stuk via het [`CancelToken`] dat
//! transcribe.cpp tussen twee rekenstappen bekijkt.

use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};

use mondig_core::engine::protocol::{
    Antwoord, Apparaat, FoutSoort, PROTOCOL_VERSIE, Verzoek, naar_regel, samples_uit_base64,
    uit_regel,
};
use mondig_core::filter::{FilterInstellingen, Meting};
pub use transcribe_cpp::CancelToken;

/// Wat de leesdraad en de werker over annuleren delen.
#[derive(Debug, Default)]
struct AnnuleerStaat {
    /// Het nummer van het laatste bestandsverzoek dat de leesdraad doorgaf (0: nog geen).
    laatste: AtomicU64,
    /// Het nummer van het bestandsverzoek dat geannuleerd is (0: geen).
    geannuleerd: AtomicU64,
    /// Breekt een lopende berekening van transcribe.cpp af.
    token: CancelToken,
}

/// Of het bestand (of spoor) dat nu loopt geannuleerd is. Elk bestands- en spoorverzoek
/// krijgt een eigen nummer, zodat een late annulering voor een vorig bestand het volgende niet raakt.
#[derive(Debug, Clone)]
pub struct Annulering {
    staat: Arc<AnnuleerStaat>,
    nummer: u64,
}

impl Annulering {
    /// Een annulering die nooit afgaat, voor een werker buiten de berichtenlus (tests).
    #[must_use]
    pub fn nooit() -> Self {
        Self {
            staat: Arc::default(),
            nummer: 0,
        }
    }

    /// Is dit bestand geannuleerd?
    #[must_use]
    pub fn is_geannuleerd(&self) -> bool {
        self.nummer != 0 && self.staat.geannuleerd.load(Ordering::SeqCst) == self.nummer
    }

    /// Het token om in de sessie van transcribe.cpp te zetten.
    #[must_use]
    pub fn token(&self) -> &CancelToken {
        &self.staat.token
    }
}

/// Wat de leesdraad aan de werker doorgeeft.
enum Bericht {
    /// Een verzoek, met het nummer als het een bestand is (anders 0).
    Verzoek(Verzoek, u64),
    /// Een regel die geen geldig verzoek is.
    Onleesbaar(String),
    /// Stdin gaf een fout.
    Fout(io::Error),
}

/// Leest stdin tot het einde. Een annulering wordt hier meteen afgehandeld (ze krijgt
/// geen eigen antwoord); de rest gaat in volgorde naar de werker. Bij `Stop` of het
/// einde van stdin wordt een lopend bestand ook afgebroken, zodat het proces snel stopt.
fn lees_verzoeken(staat: &AnnuleerStaat, zender: &Sender<Bericht>) {
    let annuleer_lopend = || {
        staat
            .geannuleerd
            .store(staat.laatste.load(Ordering::SeqCst), Ordering::SeqCst);
        staat.token.cancel();
    };
    for regel in io::stdin().lock().lines() {
        let regel = match regel {
            Ok(regel) => regel,
            Err(fout) => {
                annuleer_lopend();
                let _ = zender.send(Bericht::Fout(fout));
                return;
            }
        };
        if regel.trim().is_empty() {
            continue;
        }
        let bericht = match uit_regel::<Verzoek>(&regel) {
            Ok(Verzoek::Annuleer) => {
                annuleer_lopend();
                continue;
            }
            Ok(
                verzoek @ (Verzoek::Transcribeer { .. }
                | Verzoek::TranscribeerSpoor { .. }
                | Verzoek::HerkenSprekers { .. }
                | Verzoek::HerkenSprekersBestand { .. }),
            ) => {
                let nummer = staat.laatste.fetch_add(1, Ordering::SeqCst) + 1;
                Bericht::Verzoek(verzoek, nummer)
            }
            Ok(Verzoek::Stop) => {
                annuleer_lopend();
                Bericht::Verzoek(Verzoek::Stop, 0)
            }
            Ok(verzoek) => Bericht::Verzoek(verzoek, 0),
            Err(fout) => Bericht::Onleesbaar(fout.to_string()),
        };
        if zender.send(bericht).is_err() {
            return;
        }
    }
    // Stdin is dicht: de supervisor is weg of wil dat we stoppen.
    annuleer_lopend();
}

/// De bouwcode van dit werkproces: versie plus commit, bijvoorbeeld `0.3.0-dev+afd2907`
/// (ADR 0012, aanvulling 2-11-2026). De app vergelijkt haar eigen code hiermee.
pub const BOUWCODE: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("MONDIG_BOUW_COMMIT"));

/// Wat een werkproces met opdrachten doet.
pub trait Werker {
    /// Laadt een model en antwoordt met [`Antwoord::Geladen`] of [`Antwoord::Fout`].
    fn laad(
        &mut self,
        model_id: &str,
        modelmap: &Path,
        apparaat: Apparaat,
        taal: Option<&str>,
    ) -> Antwoord;

    /// Schrijft een audiobestand uit, in stukken met na elk stuk een
    /// [`Antwoord::Voortgang`] (via [`stuur`]), en antwoordt met [`Antwoord::Resultaat`],
    /// [`Antwoord::Fout`] of, als `annulering` afgaat, [`Antwoord::Geannuleerd`].
    fn transcribeer(&mut self, pad: &Path, annulering: &Annulering) -> Antwoord;

    /// Schrijft een spoor van een vergaderopname uit, per uiting (zie
    /// [`Verzoek::TranscribeerSpoor`]), met een [`Antwoord::Voortgang`] na elk venster (via
    /// [`stuur`]). Antwoordt als [`Werker::transcribeer`].
    fn transcribeer_spoor(&mut self, map: &Path, annulering: &Annulering) -> Antwoord;

    /// Herkent de sprekers op een spoor van een vergaderopname (zie
    /// [`Verzoek::HerkenSprekers`]) en antwoordt met [`Antwoord::Sprekers`], [`Antwoord::Fout`]
    /// of, als `annulering` afgaat, [`Antwoord::Geannuleerd`].
    fn herken_sprekers(&mut self, map: &Path, annulering: &Annulering) -> Antwoord;

    /// Herkent de sprekers in een audiobestand (zie [`Verzoek::HerkenSprekersBestand`]);
    /// antwoordt als [`Werker::herken_sprekers`].
    fn herken_sprekers_bestand(&mut self, pad: &Path, annulering: &Annulering) -> Antwoord;

    /// Schrijft geluid uit het geheugen uit (16 kHz mono) en antwoordt met
    /// [`Antwoord::Resultaat`] of [`Antwoord::Fout`]. Met `knip_stilte` wordt stilte aan
    /// begin en eind eerst weggeknipt, als het werkproces daarvoor een model heeft.
    fn transcribeer_geluid(&mut self, samples: &[f32], knip_stilte: bool) -> Antwoord;

    /// Antwoordt met de videokaarten die dit proces ziet ([`Antwoord::Apparaten`]), of met
    /// [`Antwoord::Fout`] als dat niet meer kan (het proces is al met alleen de processor
    /// gestart).
    fn apparaten(&mut self) -> Antwoord;
}

/// Het antwoord bij digitale stilte, of `None` als er wel iets te horen is. Bij stilte
/// slaat het werkproces het model over: er valt niets te horen en het model zou alleen
/// iets kunnen verzinnen. De drempel is de standaard van het filter; de app past haar
/// eigen instellingen daarna nog toe op de meegestuurde meting.
#[must_use]
pub fn antwoord_bij_stilte(duur_ms: u64, meting: &Meting) -> Option<Antwoord> {
    meting
        .is_stil(&FilterInstellingen::standaard())
        .then(|| Antwoord::Resultaat {
            waarschuwing: None,
            segmenten: Vec::new(),
            duur_ms,
            meting: meting.clone(),
        })
}

/// Schrijft één bericht als regel naar stdout en spoelt direct door.
///
/// Elk bericht begint met een regeleinde. Native bibliotheken (ggml)
/// delen onze stdout en kunnen tekst zonder regeleinde schrijven; zonder dit regeleinde
/// zou zo'n fragment vóór het JSON-bericht plakken. Een leeg regeltje meer is voor de
/// supervisor onschuldig, want die slaat regels over die geen bericht zijn. Alternatief
/// was stdout bij de start naar stderr omleiden en voor het protocol een eigen kopie
/// bewaren, maar dat kan alleen met `unsafe` en een extra afhankelijkheid.
///
/// # Errors
///
/// Geeft de schrijffout als stdout dicht is (de supervisor is weg).
pub fn stuur(antwoord: &Antwoord) -> io::Result<()> {
    let regel = naar_regel(antwoord).map_err(io::Error::other)?;
    let mut uit = io::stdout().lock();
    write!(uit, "\n{regel}\n")?;
    uit.flush()
}

/// Meldt zich met [`PROTOCOL_VERSIE`] en voert opdrachten uit tot `Stop` of tot stdin sluit.
///
/// # Errors
///
/// Geeft een I/O-fout als stdin of stdout stuk gaat.
pub fn draai<W: Werker>(werker: &mut W) -> io::Result<()> {
    draai_met_hallo(werker, PROTOCOL_VERSIE, Some(BOUWCODE))
}

/// Als [`draai`], maar met een eigen versienummer in het eerste bericht (voor tests).
///
/// # Errors
///
/// Geeft een I/O-fout als stdin of stdout stuk gaat.
pub fn draai_met_versie<W: Werker>(werker: &mut W, versie: u32) -> io::Result<()> {
    draai_met_hallo(werker, versie, Some(BOUWCODE))
}

/// Als [`draai`], maar met een eigen versienummer en bouwcode in het eerste bericht (voor
/// tests). `None` bootst een ouder werkproces na dat nog geen bouwcode meldde.
///
/// # Errors
///
/// Geeft een I/O-fout als stdin of stdout stuk gaat.
pub fn draai_met_hallo<W: Werker>(
    werker: &mut W,
    versie: u32,
    bouw: Option<&str>,
) -> io::Result<()> {
    stuur(&Antwoord::Hallo {
        versie,
        bouw: bouw.map(str::to_string),
    })?;
    let staat = Arc::new(AnnuleerStaat::default());
    let (zender, ontvanger) = mpsc::channel();
    let voor_lezer = Arc::clone(&staat);
    // De draad eindigt met stdin; bij `Stop` eindigt het proces en daarmee de draad.
    std::thread::spawn(move || lees_verzoeken(&voor_lezer, &zender));
    for bericht in ontvanger {
        let (verzoek, nummer) = match bericht {
            Bericht::Verzoek(verzoek, nummer) => (verzoek, nummer),
            Bericht::Onleesbaar(fout) => {
                stuur(&Antwoord::Fout {
                    soort: FoutSoort::Verzoek,
                    melding: format!("Het verzoek is niet te lezen: {fout}"),
                })?;
                continue;
            }
            Bericht::Fout(fout) => return Err(fout),
        };
        // Elke nieuwe berekening begint zonder afgebroken token: een annulering die
        // binnenkwam terwijl er niets liep, mag de volgende opdracht niet raken. Een
        // annulering voor juist deze opdracht ziet de werker via het nummer.
        staat.token.reset();
        let antwoord = match verzoek {
            Verzoek::Laad {
                model_id,
                modelmap,
                apparaat,
                taal,
            } => werker.laad(&model_id, &modelmap, apparaat, taal.as_deref()),
            Verzoek::Transcribeer { pad } => {
                let annulering = Annulering {
                    staat: Arc::clone(&staat),
                    nummer,
                };
                werker.transcribeer(&pad, &annulering)
            }
            Verzoek::TranscribeerSpoor { map } => {
                let annulering = Annulering {
                    staat: Arc::clone(&staat),
                    nummer,
                };
                werker.transcribeer_spoor(&map, &annulering)
            }
            Verzoek::HerkenSprekers { map } => {
                let annulering = Annulering {
                    staat: Arc::clone(&staat),
                    nummer,
                };
                werker.herken_sprekers(&map, &annulering)
            }
            Verzoek::HerkenSprekersBestand { pad } => {
                let annulering = Annulering {
                    staat: Arc::clone(&staat),
                    nummer,
                };
                werker.herken_sprekers_bestand(&pad, &annulering)
            }
            // Geluid dat niet te decoderen is, is een kapot verzoek en geen kapot
            // audiobestand: de afzender stuurde iets dat het protocol niet toestaat, en een
            // nieuwe poging op de processor verandert daar niets aan (`FoutSoort::Verzoek`).
            Verzoek::TranscribeerGeluid {
                samples,
                knip_stilte,
            } => match samples_uit_base64(&samples) {
                Ok(samples) => werker.transcribeer_geluid(&samples, knip_stilte),
                Err(fout) => Antwoord::Fout {
                    soort: FoutSoort::Verzoek,
                    melding: format!("Het geluid in het verzoek is niet te lezen: {fout}."),
                },
            },
            Verzoek::Apparaten => werker.apparaten(),
            Verzoek::Stop => return Ok(()),
            // De leesdraad handelt annuleren zelf af; hier komt het niet.
            Verzoek::Annuleer => continue,
        };
        stuur(&antwoord)?;
    }
    // Stdin is gesloten: de supervisor is weg of wil dat we stoppen.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stilte_krijgt_een_antwoord_zonder_model() {
        let stil = Meting::van_samples(&vec![0.0; 16_000]);
        let Some(Antwoord::Resultaat {
            segmenten,
            duur_ms,
            meting,
            ..
        }) = antwoord_bij_stilte(1000, &stil)
        else {
            unreachable!("stilte hoort een resultaat te geven");
        };
        assert!(segmenten.is_empty());
        assert_eq!(duur_ms, 1000);
        assert_eq!(meting, stil);
    }

    #[test]
    fn annulering_geldt_alleen_voor_haar_eigen_bestand() {
        let staat = Arc::new(AnnuleerStaat::default());
        let eerste = Annulering {
            staat: Arc::clone(&staat),
            nummer: 1,
        };
        let tweede = Annulering {
            staat: Arc::clone(&staat),
            nummer: 2,
        };
        assert!(!eerste.is_geannuleerd());
        staat.geannuleerd.store(1, Ordering::SeqCst);
        assert!(eerste.is_geannuleerd());
        assert!(!tweede.is_geannuleerd());
        assert!(!Annulering::nooit().is_geannuleerd());
    }

    #[test]
    fn geluid_krijgt_geen_voorgekookt_antwoord() {
        let luid = Meting::van_samples(&vec![0.2; 16_000]);
        assert!(antwoord_bij_stilte(1000, &luid).is_none());
    }
}
