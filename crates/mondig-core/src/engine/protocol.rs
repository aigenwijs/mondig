//! Berichten tussen de supervisor en het werkproces.
//!
//! Elk bericht is één regel JSON op stdin (verzoek) of stdout (antwoord). Het veld
//! `type` zegt welk bericht het is. Het werkproces begint elk antwoord met een
//! regeleinde, zodat uitvoer van native bibliotheken zonder regeleinde op stdout
//! nooit voor een bericht komt te staan; de supervisor slaat regels over die geen
//! bericht zijn. Nieuwe berichten komen erbij als extra variant en `PROTOCOL_VERSIE`
//! gaat dan omhoog.
//!
//! Geluid uit het geheugen (versie 3, ADR 0012) gaat als base64 van 32-bit floats in
//! little-endian volgorde (16 kHz, mono) in dezelfde JSON-regel; zie [`samples_naar_base64`].
//! Versie 4 voegt daar de optie `knip_stilte` aan toe: het werkproces knipt stilte aan
//! begin en eind weg met Silero, als dat model naast het spraakmodel staat.
//!
//! Versie 5 (fase 4) schrijft een bestand in stukken uit: het werkproces stuurt na elk
//! stuk een [`Antwoord::Voortgang`] en pas daarna het resultaat. [`Verzoek::Annuleer`]
//! breekt een lopend bestand af; het werkproces antwoordt dan met
//! [`Antwoord::Geannuleerd`] en blijft draaien voor de volgende opdracht.
//!
//! Versie 6 geeft het resultaat een optionele `waarschuwing` mee: wat bij het lezen van
//! een bestand opviel, zoals een afgebroken bestand ([`Leeswaarschuwing`]).
//!
//! Versie 7 (sprint 4b-B) schrijft een spoor van een vergaderopname uit per uiting
//! ([`Verzoek::TranscribeerSpoor`]): één segment per spraakgebied van Silero, met tijden in
//! de stroom van het spoor. Voortgang en annuleren werken als bij een bestand.
//!
//! Versie 8 (2-11-2026) vraagt het werkproces welke videokaarten het ziet
//! ([`Verzoek::Apparaten`], antwoord [`Antwoord::Apparaten`]). Het werkproces registreert
//! daarvoor de videokaart-backend; de supervisor stopt het proces daarna, zodat een volgend
//! model weer met het masker van zijn eigen apparaat start (ADR 0011).
//!
//! Versie 9 (4b-D, fase D-1) herkent sprekers op een spoor van een vergaderopname
//! ([`Verzoek::HerkenSprekers`], antwoord [`Antwoord::Sprekers`]) en geeft een [`Segment`] de
//! tijden per woord mee ([`Segment::woorden`]). Dat veld ontbreekt in de JSON als het leeg
//! is, en een bericht zonder het veld leest als "geen woordtijden".
//!
//! Versie 10 (4b-D, fase D-2) herkent sprekers ook in een gewoon audiobestand
//! ([`Verzoek::HerkenSprekersBestand`]), met hetzelfde antwoord, dezelfde voortgang en hetzelfde
//! annuleren als bij een spoor.

use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

use crate::audio::Leeswaarschuwing;
use crate::filter::Meting;

/// Versie van dit protocol. Het werkproces meldt haar in [`Antwoord::Hallo`]; de
/// supervisor weigert een proces met een andere versie.
pub const PROTOCOL_VERSIE: u32 = 10;

/// Waarop de engine rekent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Apparaat {
    /// Alleen de processor. Altijd beschikbaar.
    Cpu,
    /// De videokaart, als het werkproces daarvoor gebouwd is (Vulkan op Windows en Linux, Metal op de Mac).
    Gpu,
}

/// Wat voor videokaart het werkproces ziet (de indeling van ggml).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KaartSoort {
    /// Een losse videokaart met eigen geheugen.
    Los,
    /// Een videochip in de processor, die het werkgeheugen deelt. Vaak niet sneller dan de
    /// processor zelf (ADR 0011).
    Ingebouwd,
}

/// Een videokaart die het werkproces kan gebruiken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Videokaart {
    /// De naam zoals de driver hem meldt, bijvoorbeeld `NVIDIA GeForce RTX 4090`.
    pub naam: String,
    /// Los of ingebouwd.
    pub soort: KaartSoort,
}

/// Wat de supervisor aan het werkproces vraagt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Verzoek {
    /// Laad een model. Het vorige model (als er een was) wordt losgelaten.
    Laad {
        /// Id uit de modelcatalogus.
        model_id: String,
        /// Map waarin de bestanden van het model staan.
        modelmap: PathBuf,
        /// Op welk apparaat het model moet draaien.
        apparaat: Apparaat,
        /// Taalcode (ISO 639-1) voor modellen die een taal nodig hebben.
        taal: Option<String>,
    },
    /// Schrijf een audiobestand uit met het geladen model, in stukken. Na elk stuk komt
    /// een [`Antwoord::Voortgang`], aan het eind een [`Antwoord::Resultaat`] (of
    /// [`Antwoord::Fout`], of [`Antwoord::Geannuleerd`] na [`Verzoek::Annuleer`]).
    Transcribeer {
        /// Pad naar het audiobestand.
        pad: PathBuf,
    },
    /// Schrijf geluid uit het geheugen uit met het geladen model, zonder bestand op
    /// schijf (een dictaat komt nooit op schijf, ADR 0012).
    TranscribeerGeluid {
        /// De samples als base64 van 32-bit floats (little-endian), 16 kHz mono. Maak
        /// en lees dit met [`samples_naar_base64`] en [`samples_uit_base64`].
        samples: String,
        /// Knip stilte aan begin en eind weg voordat het model het geluid krijgt. Het
        /// werkproces gebruikt daarvoor Silero uit de map naast het spraakmodel; staat
        /// dat er niet, dan wordt er gewoon uitgeschreven zonder knippen. Geen spraak
        /// gevonden geeft een resultaat zonder segmenten.
        #[serde(default)]
        knip_stilte: bool,
    },
    /// Schrijf een spoor van een vergaderopname uit, per uiting (sprint 4b-B). Het
    /// werkproces leest de stukken van het spoor als één stroom, laat Silero de
    /// spraakgebieden vinden en schrijft elk gebied apart uit: één [`Segment`] per gebied,
    /// met begin en eind in milliseconden van de stroom (zonder de gaten tussen de ankers),
    /// net als de meting. Zo meet het filter in de app op dezelfde plek als de tekst; de app
    /// rekent de tijden daarna om naar de tijdlijn van de opname. Na elk venster komt een
    /// [`Antwoord::Voortgang`] (in milliseconden van het spoor zelf), aan het eind een
    /// [`Antwoord::Resultaat`]. Annuleren werkt als bij [`Verzoek::Transcribeer`]. Zonder
    /// Silero naast het spraakmodel komt er een [`Antwoord::Fout`].
    TranscribeerSpoor {
        /// De map van het spoor (`<opnamemap>/microfoon` of `<opnamemap>/systeem`).
        map: PathBuf,
    },
    /// Breek het bestand of spoor af dat nu wordt uitgeschreven. Krijgt zelf geen antwoord:
    /// het lopende [`Verzoek::Transcribeer`], [`Verzoek::TranscribeerSpoor`],
    /// [`Verzoek::HerkenSprekers`] of [`Verzoek::HerkenSprekersBestand`] eindigt met
    /// [`Antwoord::Geannuleerd`]. Loopt er niets, dan gebeurt er niets.
    Annuleer,
    /// Herken de sprekers op een spoor van een vergaderopname (versie 9). Het werkproces leest
    /// het spoor als dezelfde stroom als [`Verzoek::TranscribeerSpoor`] en draait Sortformer
    /// erop, uit de map naast het spraakmodel. Het spraakmodel moet geladen zijn: Sortformer
    /// draait op hetzelfde apparaat en wordt na afloop weer losgelaten. Antwoord:
    /// [`Antwoord::Sprekers`] met tijden in milliseconden van de stroom. Tijdens het lezen
    /// komt er een [`Antwoord::Voortgang`]; annuleren werkt als bij een spoor. Staat het
    /// sprekermodel er niet, dan komt er een [`Antwoord::Fout`] van de soort
    /// [`FoutSoort::ModelOntbreekt`].
    HerkenSprekers {
        /// De map van het spoor (`<opnamemap>/microfoon` of `<opnamemap>/systeem`).
        map: PathBuf,
    },
    /// Herken de sprekers in een audiobestand (versie 10). Als [`Verzoek::HerkenSprekers`], maar
    /// het werkproces decodeert het bestand zelf (16 kHz mono, in tijden van het bestand) in
    /// plaats van een spoor te lezen. Antwoord: [`Antwoord::Sprekers`]; tijdens het lezen komt er
    /// een [`Antwoord::Voortgang`] en annuleren werkt als bij een spoor.
    HerkenSprekersBestand {
        /// Pad naar het audiobestand.
        pad: PathBuf,
    },
    /// Welke videokaarten zie je? Antwoord: [`Antwoord::Apparaten`]. Alleen als eerste
    /// opdracht aan een vers werkproces: het registreert daarvoor de videokaart-backend, en
    /// dat kan maar één keer per proces (versie 8).
    Apparaten,
    /// Stop het werkproces netjes.
    Stop,
}

/// Wat het werkproces terugstuurt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Antwoord {
    /// Eerste bericht na het opstarten: het proces is klaar voor opdrachten.
    Hallo {
        /// Protocolversie van het werkproces.
        versie: u32,
        /// Bouwcode van het werkproces (versie plus commit, ADR 0012), zodat de app kan
        /// zien of werkproces en app uit dezelfde uitgave komen. Een ouder werkproces stuurt
        /// dit veld niet mee en leest hier als `None`; de app ziet dat als "niet bijgewerkt".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bouw: Option<String>,
    },
    /// Het model is geladen.
    Geladen,
    /// De uitkomst van een transcriptie.
    Resultaat {
        /// De tekst in stukken met tijdcodes.
        segmenten: Vec<Segment>,
        /// Duur van de audio in milliseconden.
        duur_ms: u64,
        /// Meting van de audio voor het filter tegen verzonnen tekst, zodat de app de
        /// audio zelf niet hoeft te decoderen.
        meting: Meting,
        /// Wat bij het lezen van het bestand opviel. Ontbreekt als er niets was.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        waarschuwing: Option<Leeswaarschuwing>,
    },
    /// Tussenstand bij het uitschrijven van een bestand, na elk stuk.
    Voortgang {
        /// Tot hoever het bestand is uitgeschreven, in milliseconden vanaf het begin.
        klaar_ms: u64,
        /// De duur van het hele bestand volgens de container, als die bekend is.
        totaal_ms: Option<u64>,
    },
    /// Het bestand is op verzoek afgebroken; het werkproces draait nog.
    Geannuleerd,
    /// De sprekers op een spoor (antwoord op [`Verzoek::HerkenSprekers`]). Segmenten van
    /// verschillende sprekers mogen elkaar overlappen.
    Sprekers {
        /// Wie wanneer sprak, op volgorde van begin.
        segmenten: Vec<SprekerSegment>,
    },
    /// De videokaarten die het werkproces ziet; leeg als er geen bruikbare is.
    Apparaten {
        /// In de volgorde van ggml; de eerste is de kaart die een model krijgt.
        videokaarten: Vec<Videokaart>,
    },
    /// De opdracht is mislukt, maar het werkproces draait nog.
    Fout {
        /// Soort fout, zodat de supervisor kan bepalen of een nieuwe poging zin heeft.
        soort: FoutSoort,
        /// Melding in gewone taal. Bevat nooit transcripttekst.
        melding: String,
    },
}

/// Een stuk tekst met begin en eind in de audio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    /// Begin in milliseconden vanaf het begin van de audio.
    pub start_ms: u64,
    /// Eind in milliseconden.
    pub eind_ms: u64,
    /// De tekst.
    pub tekst: String,
    /// De woorden met hun tijden, in dezelfde tijdbasis als het segment (versie 9). Leeg als
    /// het model geen woordtijden geeft (Canary, ADR 0011), bij een ouder werkproces, of als
    /// het filter de tekst veranderde. Zo leest een oud bericht nog, en een nieuw bericht
    /// zonder woorden ziet eruit als een oud.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub woorden: Vec<Woord>,
}

impl Segment {
    /// Een segment zonder woordtijden.
    #[must_use]
    pub fn nieuw(start_ms: u64, eind_ms: u64, tekst: impl Into<String>) -> Self {
        Self {
            start_ms,
            eind_ms,
            tekst: tekst.into(),
            woorden: Vec::new(),
        }
    }
}

/// Eén woord met begin en eind, in milliseconden.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Woord {
    /// Begin in milliseconden.
    pub start_ms: u64,
    /// Eind in milliseconden.
    pub eind_ms: u64,
    /// Het woord zoals het model het schreef, met leestekens.
    pub tekst: String,
}

/// Wie er sprak tussen twee tijden, volgens het sprekermodel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SprekerSegment {
    /// Het nummer van de spreker, vanaf 1. Sortformer kent er hooguit 4.
    pub spreker: u8,
    /// Begin in milliseconden.
    pub start_ms: u64,
    /// Eind in milliseconden.
    pub eind_ms: u64,
}

/// Soort fout die het werkproces meldt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoutSoort {
    /// Het model kon niet geladen worden (geen geheugen, driverfout).
    ModelLaden,
    /// Het model of een bestand ervan ontbreekt. Een andere videokaart-instelling
    /// helpt daar niet tegen, dus geen nieuwe poging op de processor.
    ModelOntbreekt,
    /// Er is nog geen model geladen.
    NietGeladen,
    /// Het audiobestand is niet te lezen.
    Audio,
    /// De engine faalde tijdens het uitschrijven.
    Transcriberen,
    /// Het verzoek was niet te begrijpen.
    Verzoek,
    /// Het sprekermodel faalde, ook na een poging op de processor (versie 9). Het spraakmodel
    /// werkt nog, dus geen nieuwe poging op de processor: het verslag komt zonder sprekers.
    Sprekermodel,
}

impl FoutSoort {
    /// Kan een nieuwe poging op de processor zin hebben? Alleen bij fouten van de
    /// engine zelf, niet bij een kapot audiobestand of een verkeerd verzoek.
    #[must_use]
    pub fn kan_aan_apparaat_liggen(self) -> bool {
        matches!(self, Self::ModelLaden | Self::Transcriberen)
    }
}

/// Wat er mis kan gaan bij het terugzetten van samples uit een verzoek.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SamplesFout {
    /// De tekst is geen geldige base64.
    #[error("het geluid in het verzoek is geen geldige base64")]
    OngeldigeBase64,
    /// Het aantal bytes is geen veelvoud van 4, dus het zijn geen hele 32-bit getallen.
    #[error("het geluid in het verzoek heeft een lengte die niet bij 32-bit getallen past")]
    OngeldigeLengte,
}

/// Zet samples (16 kHz mono) om naar base64 van 32-bit floats in little-endian volgorde.
#[must_use]
pub fn samples_naar_base64(samples: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    STANDARD.encode(bytes)
}

/// Zet base64 van 32-bit floats (little-endian) terug naar samples.
///
/// # Errors
///
/// Geeft [`SamplesFout`] bij ongeldige base64 of bij een lengte die geen veelvoud van
/// vier bytes is.
pub fn samples_uit_base64(tekst: &str) -> Result<Vec<f32>, SamplesFout> {
    let bytes = STANDARD
        .decode(tekst)
        .map_err(|_| SamplesFout::OngeldigeBase64)?;
    if bytes.len() % 4 != 0 {
        return Err(SamplesFout::OngeldigeLengte);
    }
    let (stukken, _) = bytes.as_chunks::<4>();
    Ok(stukken
        .iter()
        .map(|stuk| f32::from_le_bytes(*stuk))
        .collect())
}

/// Zet een bericht om naar één regel JSON (zonder regeleinde).
///
/// # Errors
///
/// Geeft de serde-fout als het bericht niet te serialiseren is. Met deze typen
/// gebeurt dat in de praktijk niet.
pub fn naar_regel<T: Serialize>(bericht: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(bericht)
}

/// Leest één regel JSON als bericht.
///
/// # Errors
///
/// Geeft de serde-fout als de regel geen geldig bericht van dit type is.
pub fn uit_regel<'a, T: Deserialize<'a>>(regel: &'a str) -> Result<T, serde_json::Error> {
    serde_json::from_str(regel)
}

/// Zoekt een bericht in een regel waar native uitvoer zonder regeleinde voor kan staan
/// (bijvoorbeeld `voortgang 40%{"type":"hallo","versie":4}`). Probeert elke plek waar
/// `{"type":` begint; tekst voor of na het bericht wordt genegeerd. Geeft `None` als
/// de regel geen bericht van dit type bevat.
#[must_use]
pub fn zoek_bericht<T: for<'de> Deserialize<'de>>(regel: &str) -> Option<T> {
    regel.match_indices(r#"{"type":"#).find_map(|(begin, _)| {
        serde_json::Deserializer::from_str(&regel[begin..])
            .into_iter::<T>()
            .next()
            .and_then(Result::ok)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heen_en_terug<T>(bericht: &T)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let regel = naar_regel(bericht).unwrap();
        assert!(
            !regel.contains('\n'),
            "een bericht moet op één regel passen"
        );
        let terug: T = uit_regel(&regel).unwrap();
        assert_eq!(&terug, bericht);
    }

    #[test]
    fn verzoeken_overleven_serde() {
        heen_en_terug(&Verzoek::Laad {
            model_id: "whisper-large-v3-turbo-q8".into(),
            modelmap: PathBuf::from(r"C:\modellen\whisper-large-v3-turbo-q8"),
            apparaat: Apparaat::Gpu,
            taal: Some("nl".into()),
        });
        heen_en_terug(&Verzoek::Laad {
            model_id: "x".into(),
            modelmap: PathBuf::from("/m"),
            apparaat: Apparaat::Cpu,
            taal: None,
        });
        heen_en_terug(&Verzoek::Transcribeer {
            pad: PathBuf::from("opname met spaties.wav"),
        });
        heen_en_terug(&Verzoek::TranscribeerGeluid {
            samples: samples_naar_base64(&[0.0, -0.5, 0.25]),
            knip_stilte: true,
        });
        heen_en_terug(&Verzoek::TranscribeerSpoor {
            map: PathBuf::from(r"C:\tijdelijk\vergadering-1\systeem"),
        });
        heen_en_terug(&Verzoek::HerkenSprekers {
            map: PathBuf::from(r"C:\tijdelijk\vergadering-1\systeem"),
        });
        heen_en_terug(&Verzoek::HerkenSprekersBestand {
            pad: PathBuf::from("opname met spaties.wav"),
        });
        heen_en_terug(&Verzoek::Annuleer);
        heen_en_terug(&Verzoek::Apparaten);
        heen_en_terug(&Verzoek::Stop);
    }

    #[test]
    fn antwoorden_overleven_serde() {
        heen_en_terug(&Antwoord::Hallo {
            versie: PROTOCOL_VERSIE,
            bouw: None,
        });
        heen_en_terug(&Antwoord::Hallo {
            versie: PROTOCOL_VERSIE,
            bouw: Some("0.3.0-dev+afd2907".into()),
        });
        heen_en_terug(&Antwoord::Geladen);
        heen_en_terug(&Antwoord::Resultaat {
            segmenten: vec![
                Segment::nieuw(0, 2500, "Dit is een \"test\"\nmet een regeleinde."),
                Segment {
                    start_ms: 2500,
                    eind_ms: 3000,
                    tekst: "Ja hoor.".into(),
                    woorden: vec![
                        Woord {
                            start_ms: 2500,
                            eind_ms: 2700,
                            tekst: "Ja".into(),
                        },
                        Woord {
                            start_ms: 2700,
                            eind_ms: 3000,
                            tekst: "hoor.".into(),
                        },
                    ],
                },
            ],
            duur_ms: 3000,
            meting: Meting {
                rms: 0.0123,
                frame_rms: vec![0.0, 0.25, 0.031_415_9],
            },
            waarschuwing: None,
        });
        heen_en_terug(&Antwoord::Resultaat {
            segmenten: Vec::new(),
            duur_ms: 4000,
            meting: Meting {
                rms: 0.1,
                frame_rms: Vec::new(),
            },
            waarschuwing: Some(Leeswaarschuwing::Afgebroken { tot_ms: 4000 }),
        });
        heen_en_terug(&Antwoord::Voortgang {
            klaar_ms: 60_000,
            totaal_ms: Some(3_600_000),
        });
        heen_en_terug(&Antwoord::Voortgang {
            klaar_ms: 1,
            totaal_ms: None,
        });
        heen_en_terug(&Antwoord::Geannuleerd);
        heen_en_terug(&Antwoord::Sprekers {
            segmenten: vec![
                SprekerSegment {
                    spreker: 1,
                    start_ms: 0,
                    eind_ms: 1200,
                },
                SprekerSegment {
                    spreker: 2,
                    start_ms: 900,
                    eind_ms: 3000,
                },
            ],
        });
        heen_en_terug(&Antwoord::Sprekers {
            segmenten: Vec::new(),
        });
        heen_en_terug(&Antwoord::Apparaten {
            videokaarten: vec![
                Videokaart {
                    naam: "NVIDIA GeForce RTX 4090".into(),
                    soort: KaartSoort::Los,
                },
                Videokaart {
                    naam: "AMD Radeon(TM) Graphics".into(),
                    soort: KaartSoort::Ingebouwd,
                },
            ],
        });
        heen_en_terug(&Antwoord::Apparaten {
            videokaarten: Vec::new(),
        });
        heen_en_terug(&Antwoord::Fout {
            soort: FoutSoort::ModelLaden,
            melding: "Geen geheugen.".into(),
        });
    }

    #[test]
    fn formaat_is_stabiel() {
        assert_eq!(naar_regel(&Verzoek::Stop).unwrap(), r#"{"type":"stop"}"#);
        assert_eq!(
            naar_regel(&Antwoord::Hallo {
                versie: 4,
                bouw: None
            })
            .unwrap(),
            r#"{"type":"hallo","versie":4}"#
        );
        assert_eq!(
            naar_regel(&Antwoord::Hallo {
                versie: 4,
                bouw: Some("0.3.0-dev+afd2907".into())
            })
            .unwrap(),
            r#"{"type":"hallo","versie":4,"bouw":"0.3.0-dev+afd2907"}"#
        );
        // Een hallo van een ouder werkproces (zonder bouwcode) blijft leesbaar.
        assert_eq!(
            uit_regel::<Antwoord>(r#"{"type":"hallo","versie":4}"#).unwrap(),
            Antwoord::Hallo {
                versie: 4,
                bouw: None
            }
        );
        assert_eq!(PROTOCOL_VERSIE, 10);
        assert_eq!(
            naar_regel(&Verzoek::HerkenSprekers {
                map: PathBuf::from("opname/systeem"),
            })
            .unwrap(),
            r#"{"type":"herken_sprekers","map":"opname/systeem"}"#
        );
        assert_eq!(
            naar_regel(&Verzoek::HerkenSprekersBestand {
                pad: PathBuf::from("gesprek.wav"),
            })
            .unwrap(),
            r#"{"type":"herken_sprekers_bestand","pad":"gesprek.wav"}"#
        );
        assert_eq!(
            naar_regel(&Antwoord::Sprekers {
                segmenten: vec![SprekerSegment {
                    spreker: 3,
                    start_ms: 10,
                    eind_ms: 20,
                }],
            })
            .unwrap(),
            r#"{"type":"sprekers","segmenten":[{"spreker":3,"start_ms":10,"eind_ms":20}]}"#
        );
        assert_eq!(
            naar_regel(&Verzoek::Apparaten).unwrap(),
            r#"{"type":"apparaten"}"#
        );
        assert_eq!(
            naar_regel(&Antwoord::Apparaten {
                videokaarten: vec![Videokaart {
                    naam: "Kaart".into(),
                    soort: KaartSoort::Ingebouwd,
                }],
            })
            .unwrap(),
            r#"{"type":"apparaten","videokaarten":[{"naam":"Kaart","soort":"ingebouwd"}]}"#
        );
        assert_eq!(
            naar_regel(&Verzoek::TranscribeerSpoor {
                map: PathBuf::from("opname/microfoon"),
            })
            .unwrap(),
            r#"{"type":"transcribeer_spoor","map":"opname/microfoon"}"#
        );
        assert_eq!(
            naar_regel(&Verzoek::Annuleer).unwrap(),
            r#"{"type":"annuleer"}"#
        );
        assert_eq!(
            naar_regel(&Antwoord::Voortgang {
                klaar_ms: 60_000,
                totaal_ms: Some(120_000)
            })
            .unwrap(),
            r#"{"type":"voortgang","klaar_ms":60000,"totaal_ms":120000}"#
        );
        // 1.0f32 is 00 00 80 3F in little-endian, en -2.0f32 is 00 00 00 C0.
        assert_eq!(
            naar_regel(&Verzoek::TranscribeerGeluid {
                samples: samples_naar_base64(&[1.0, -2.0]),
                knip_stilte: false,
            })
            .unwrap(),
            r#"{"type":"transcribeer_geluid","samples":"AACAPwAAAMA=","knip_stilte":false}"#
        );
        // Een verzoek zonder de optie (van een oudere afzender) knipt niet.
        assert_eq!(
            uit_regel::<Verzoek>(r#"{"type":"transcribeer_geluid","samples":"AACAPwAAAMA="}"#)
                .unwrap(),
            Verzoek::TranscribeerGeluid {
                samples: "AACAPwAAAMA=".into(),
                knip_stilte: false,
            }
        );
    }

    #[test]
    fn een_segment_zonder_woorden_blijft_het_oude_formaat() {
        /// Zoals een oudere lezer een segment kent: zonder woorden.
        #[derive(Deserialize)]
        struct OudSegment {
            tekst: String,
        }
        // Een nieuw segment zonder woorden schrijft het veld niet: een ouder werkproces of
        // een oudere app leest het zoals altijd.
        assert_eq!(
            naar_regel(&Segment::nieuw(0, 10, "hoi")).unwrap(),
            r#"{"start_ms":0,"eind_ms":10,"tekst":"hoi"}"#
        );
        // Een resultaat van een ouder werkproces (zonder woorden) leest nog, als "geen woordtijden".
        let oud = r#"{"type":"resultaat","segmenten":[{"start_ms":0,"eind_ms":10,"tekst":"hoi"}],"duur_ms":10,"meting":{"rms":0.1,"frame_rms":[]}}"#;
        let Antwoord::Resultaat { segmenten, .. } = uit_regel::<Antwoord>(oud).unwrap() else {
            unreachable!("een resultaat hoort een resultaat te blijven");
        };
        assert_eq!(segmenten, vec![Segment::nieuw(0, 10, "hoi")]);
        // Met woorden komt het veld erbij; een oudere lezer negeert onbekende velden (zoals
        // serde standaard doet) en leest het segment gewoon.
        let met = Segment {
            woorden: vec![Woord {
                start_ms: 0,
                eind_ms: 10,
                tekst: "hoi".into(),
            }],
            ..Segment::nieuw(0, 10, "hoi")
        };
        assert_eq!(
            naar_regel(&met).unwrap(),
            r#"{"start_ms":0,"eind_ms":10,"tekst":"hoi","woorden":[{"start_ms":0,"eind_ms":10,"tekst":"hoi"}]}"#
        );
        let oud: OudSegment = serde_json::from_str(&naar_regel(&met).unwrap()).unwrap();
        assert_eq!(oud.tekst, "hoi");
    }

    #[test]
    fn samples_gaan_heen_en_terug_zonder_verlies() {
        let samples = [0.0, -0.0, 1.0, -1.0, 0.123_456_79, f32::MIN_POSITIVE, 1e-30];
        let terug = samples_uit_base64(&samples_naar_base64(&samples)).unwrap();
        assert_eq!(terug.len(), samples.len());
        for (a, b) in samples.iter().zip(&terug) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
        assert!(
            samples_uit_base64(&samples_naar_base64(&[]))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn samples_terugzetten_weigert_rommel() {
        assert_eq!(
            samples_uit_base64("dit is geen base64!"),
            Err(SamplesFout::OngeldigeBase64)
        );
        // Vijf bytes zijn geen hele 32-bit getallen.
        assert_eq!(
            samples_uit_base64(&STANDARD.encode([0_u8; 5])),
            Err(SamplesFout::OngeldigeLengte)
        );
    }

    #[test]
    fn zoek_bericht_vindt_een_bericht_na_native_uitvoer() {
        let hallo = Antwoord::Hallo {
            versie: 4,
            bouw: None,
        };
        let json = naar_regel(&hallo).unwrap();
        assert_eq!(zoek_bericht::<Antwoord>(&json), Some(hallo.clone()));
        assert_eq!(
            zoek_bericht::<Antwoord>(&format!("model laden 40%{json}")),
            Some(hallo.clone())
        );
        // Ook met tekst erachter, en met een onbekend bericht ervoor.
        assert_eq!(
            zoek_bericht::<Antwoord>(&format!("x{{\"type\":\"onbekend\"}}y{json}z")),
            Some(hallo)
        );
        assert_eq!(zoek_bericht::<Antwoord>("gewone tekst"), None);
        assert_eq!(zoek_bericht::<Antwoord>(r#"{"type":"onbekend"}"#), None);
    }

    #[test]
    fn onbekend_bericht_is_een_fout() {
        assert!(uit_regel::<Verzoek>(r#"{"type":"onbekend"}"#).is_err());
        assert!(uit_regel::<Verzoek>("geen json").is_err());
    }

    #[test]
    fn alleen_enginefouten_rechtvaardigen_een_nieuwe_poging() {
        assert!(FoutSoort::ModelLaden.kan_aan_apparaat_liggen());
        assert!(FoutSoort::Transcriberen.kan_aan_apparaat_liggen());
        assert!(!FoutSoort::ModelOntbreekt.kan_aan_apparaat_liggen());
        assert!(!FoutSoort::Audio.kan_aan_apparaat_liggen());
        assert!(!FoutSoort::Verzoek.kan_aan_apparaat_liggen());
        assert!(!FoutSoort::NietGeladen.kan_aan_apparaat_liggen());
        assert!(!FoutSoort::Sprekermodel.kan_aan_apparaat_liggen());
    }
}
