//! Eén toestandsmodel voor alle taken (ADR 0012): dictaten, bestanden, vergaderingen
//! en downloads.
//!
//! Pil, venster, systeembalk en schermlezer lezen dezelfde stroom [`Gebeurtenis`]sen,
//! zodat elke functie hetzelfde aanvoelt. Het model weet niets van de interface: het
//! zegt alleen in welke toestand een taak zit en welke overgangen mogen.
//!
//! De vervolgactie bij een fout volgt uit de soort fout in `mondig-core`
//! ([`Vervolg::voor_engine_fout`]), nooit uit de tekst van de melding.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::aanbieder::AanbiederFout;
use crate::engine::protocol::FoutSoort;
use crate::engine::supervisor::EngineFout;
use crate::probleem::Probleem;

/// Herkenningsnummer van een taak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaakId(pub u64);

/// De gedeelde teller voor [`TaakId::nieuw`]. Hij begint hoog, zodat hij niet botst met de
/// tellers die de app en de motorbeheerder nu nog zelf bijhouden (die beginnen bij 1).
static VOLGEND_ID: AtomicU64 = AtomicU64::new(1 << 40);

impl TaakId {
    /// Een id dat in dit proces nog niet is uitgedeeld door deze teller.
    #[must_use]
    pub fn nieuw() -> Self {
        Self(VOLGEND_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// Wat voor taak het is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Soort {
    /// Een korte spraakopname die direct als tekst wordt geplakt.
    Dictaat,
    /// Een audiobestand uitschrijven.
    Bestand,
    /// Een lange opname van een vergadering.
    Vergadering,
    /// Een model downloaden.
    Download,
}

/// Wat de gebruiker na een fout kan doen. Er is er altijd hooguit één.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vervolg {
    /// Dezelfde taak nog eens starten.
    OpnieuwProberen,
    /// Een ander spraakmodel kiezen.
    AnderModelKiezen,
    /// De microfooninstellingen van het besturingssysteem openen.
    MicrofooninstellingOpenen,
    /// Het model opnieuw downloaden.
    OpnieuwDownloaden,
    /// Mondig afsluiten en de nieuwste versie opnieuw installeren.
    OpnieuwInstalleren,
    /// Er is niets dat de gebruiker kan doen.
    Geen,
}

impl Vervolg {
    /// De vervolgactie voor een fout van het werkproces, op basis van de soort fout.
    #[must_use]
    pub fn voor_engine_fout(fout: &EngineFout) -> Self {
        match fout {
            // Een crash, time-out of tijdelijke engine-fout is vaak eenmalig.
            EngineFout::Gecrasht { .. }
            | EngineFout::TijdOverschreden { .. }
            | EngineFout::NietGeladen
            | EngineFout::Intern => Self::OpnieuwProberen,
            EngineFout::Engine { soort, .. } => Self::voor_foutsoort(*soort),
            // Een werkproces uit een andere uitgave: alleen opnieuw installeren helpt.
            EngineFout::NietBijgewerkt { .. } => Self::OpnieuwInstalleren,
            // Een kapotte installatie, een ongeldig bericht of een te lang dictaat:
            // opnieuw proberen of een ander model helpt niet. Geannuleerd is geen fout
            // van het systeem; de gebruiker koos er zelf voor.
            EngineFout::Starten { .. }
            | EngineFout::Protocol(_)
            | EngineFout::TeLang { .. }
            | EngineFout::Geannuleerd => Self::Geen,
        }
    }

    /// De vervolgactie voor een fout van een aanbieder.
    #[must_use]
    pub fn voor_aanbieder_fout(fout: &AanbiederFout) -> Self {
        match fout {
            AanbiederFout::Engine(engine) => Self::voor_engine_fout(engine),
            // Een kapot bestand blijft kapot; de gebruiker kiest een ander bestand.
            AanbiederFout::Audio(_) => Self::Geen,
        }
    }

    fn voor_foutsoort(soort: FoutSoort) -> Self {
        match soort {
            FoutSoort::ModelLaden => Self::AnderModelKiezen,
            FoutSoort::ModelOntbreekt => Self::OpnieuwDownloaden,
            FoutSoort::NietGeladen | FoutSoort::Transcriberen => Self::OpnieuwProberen,
            FoutSoort::Audio | FoutSoort::Verzoek | FoutSoort::Sprekermodel => Self::Geen,
        }
    }
}

/// Een fout in gewone taal met de ene vervolgactie, voor een opdracht die al faalt
/// voordat er een taak is (bijvoorbeeld een ontbrekend model). Is er wel een taak, dan
/// gaat dezelfde fout als [`Toestand::Fout`] mee in een [`Gebeurtenis`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Foutmelding {
    /// Melding in gewone taal. Bevat nooit transcripttekst.
    pub melding: String,
    /// Wat de gebruiker nu kan doen.
    pub vervolg: Vervolg,
}

impl Foutmelding {
    /// Een foutmelding met vervolgactie.
    #[must_use]
    pub fn nieuw(melding: impl Into<String>, vervolg: Vervolg) -> Self {
        Self {
            melding: melding.into(),
            vervolg,
        }
    }
}

impl From<AanbiederFout> for Foutmelding {
    /// Een fout met een eigen soort ([`Probleem`]) krijgt de vaste uitleg van die soort; de rest
    /// houdt zijn eigen melding en vervolg.
    fn from(fout: AanbiederFout) -> Self {
        // Hier is niet bekend of de motor op de videokaart rekende. Een motor die daar faalde
        // viel al terug op de processor, dus een geheugenfout die hier aankomt is van de processor.
        if let Some(probleem) = Probleem::uit_aanbieder_fout(&fout, false) {
            return probleem.foutmelding();
        }
        Self::nieuw(fout.to_string(), Vervolg::voor_aanbieder_fout(&fout))
    }
}

/// Welke stap van een taak er loopt terwijl ze verwerkt wordt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stap {
    /// Uitschrijven (of downloaden): de gewone stap.
    #[default]
    Uitschrijven,
    /// Na het uitschrijven van een vergadering: de sprekers herkennen (4b-D). De voortgang is
    /// dan die van het lezen van het spoor; het rekenen daarna meldt zich niet.
    Sprekers,
}

impl Stap {
    /// Is dit de gewone stap?
    #[must_use]
    pub fn is_uitschrijven(&self) -> bool {
        *self == Self::Uitschrijven
    }
}

impl Toestand {
    /// Verwerken in de gewone stap, met dit percentage.
    #[must_use]
    pub const fn verwerkt(voortgang: Option<u8>) -> Self {
        Self::Verwerkt {
            voortgang,
            stap: Stap::Uitschrijven,
        }
    }
}

/// In welke toestand een taak zit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Toestand {
    /// De taak staat klaar maar is nog niet begonnen.
    Wacht,
    /// Er wordt opgenomen.
    Luistert,
    /// Er wordt gewerkt, met een voortgang van 0 tot en met 100 als die bekend is.
    Verwerkt {
        /// Percentage klaar, als het bekend is.
        voortgang: Option<u8>,
        /// Welke stap er loopt. Alleen een vergadering kent een tweede stap (sprekers
        /// herkennen, 4b-D); een ouder bericht zonder dit veld is gewoon uitschrijven. Bij
        /// uitschrijven staat het veld niet in de JSON, zodat die gelijk blijft aan vroeger.
        #[serde(default, skip_serializing_if = "Stap::is_uitschrijven")]
        stap: Stap,
    },
    /// De taak is gelukt.
    Klaar,
    /// De taak is mislukt.
    Fout {
        /// Melding in gewone taal. Bevat nooit transcripttekst.
        melding: String,
        /// Wat de gebruiker nu kan doen.
        vervolg: Vervolg,
    },
    /// De gebruiker heeft de taak afgebroken.
    Geannuleerd,
}

impl Toestand {
    /// Een fout met melding en vervolgactie.
    #[must_use]
    pub fn fout(melding: impl Into<String>, vervolg: Vervolg) -> Self {
        Self::Fout {
            melding: melding.into(),
            vervolg,
        }
    }

    /// Een fout van het werkproces: de melding is de tekst van de fout, de vervolgactie
    /// volgt uit de soort.
    #[must_use]
    pub fn uit_engine_fout(fout: &EngineFout) -> Self {
        Self::fout(fout.to_string(), Vervolg::voor_engine_fout(fout))
    }

    /// Een fout van een aanbieder.
    #[must_use]
    pub fn uit_aanbieder_fout(fout: &AanbiederFout) -> Self {
        Self::fout(fout.to_string(), Vervolg::voor_aanbieder_fout(fout))
    }

    /// Is de taak afgelopen (gelukt, mislukt of geannuleerd)? Daarna verandert er niets meer.
    #[must_use]
    pub fn is_afgelopen(&self) -> bool {
        matches!(self, Self::Klaar | Self::Fout { .. } | Self::Geannuleerd)
    }

    /// Loopt de taak nog? Een wachtende taak telt ook mee.
    #[must_use]
    pub fn is_actief(&self) -> bool {
        !self.is_afgelopen()
    }
}

/// Mag een taak van `van` naar `naar`?
///
/// Een afgelopen taak (klaar, fout, geannuleerd) verandert niet meer: opnieuw proberen
/// is een nieuwe taak. Verwerken mag zichzelf opvolgen voor een nieuwe voortgang, en
/// een voortgang boven 100 bestaat niet.
#[must_use]
pub fn overgang_toegestaan(van: &Toestand, naar: &Toestand) -> bool {
    use Toestand::{Fout, Geannuleerd, Klaar, Luistert, Verwerkt, Wacht};
    if let Verwerkt {
        voortgang: Some(procent),
        ..
    } = naar
        && *procent > 100
    {
        return false;
    }
    matches!(
        (van, naar),
        (
            Wacht,
            Luistert | Verwerkt { .. } | Fout { .. } | Geannuleerd
        ) | (Luistert, Verwerkt { .. } | Fout { .. } | Geannuleerd)
            | (
                Verwerkt { .. },
                Verwerkt { .. } | Klaar | Fout { .. } | Geannuleerd
            )
    )
}

/// Een overgang die niet mag.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("een taak kan niet van {van:?} naar {naar:?}")]
pub struct OngeldigeOvergang {
    /// Huidige toestand.
    pub van: Toestand,
    /// Gevraagde toestand.
    pub naar: Toestand,
}

/// Wat de interface te horen krijgt bij elke wijziging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gebeurtenis {
    /// Welke taak.
    pub taak: TaakId,
    /// Wat voor taak.
    pub soort: Soort,
    /// De nieuwe toestand.
    pub toestand: Toestand,
}

/// Een taak met haar toestand. Controleert zelf of een overgang mag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taak {
    id: TaakId,
    soort: Soort,
    toestand: Toestand,
}

impl Taak {
    /// Een nieuwe taak in de toestand [`Toestand::Wacht`].
    #[must_use]
    pub fn nieuw(id: TaakId, soort: Soort) -> Self {
        Self {
            id,
            soort,
            toestand: Toestand::Wacht,
        }
    }

    /// Het id van de taak.
    #[must_use]
    pub fn id(&self) -> TaakId {
        self.id
    }

    /// De soort taak.
    #[must_use]
    pub fn soort(&self) -> Soort {
        self.soort
    }

    /// De huidige toestand.
    #[must_use]
    pub fn toestand(&self) -> &Toestand {
        &self.toestand
    }

    /// Zet de taak in een nieuwe toestand en geeft de gebeurtenis voor de interface.
    ///
    /// # Errors
    ///
    /// Geeft [`OngeldigeOvergang`] als de overgang niet mag; de taak blijft dan zoals ze was.
    pub fn ga_naar(&mut self, naar: Toestand) -> Result<Gebeurtenis, OngeldigeOvergang> {
        if !overgang_toegestaan(&self.toestand, &naar) {
            return Err(OngeldigeOvergang {
                van: self.toestand.clone(),
                naar,
            });
        }
        self.toestand = naar.clone();
        Ok(Gebeurtenis {
            taak: self.id,
            soort: self.soort,
            toestand: naar,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;
    use crate::audio::AudioFout;
    use crate::engine::supervisor::Fase;

    fn verwerkt(voortgang: Option<u8>) -> Toestand {
        Toestand::verwerkt(voortgang)
    }

    fn fout() -> Toestand {
        Toestand::fout("Er ging iets mis.", Vervolg::Geen)
    }

    #[test]
    fn normale_levensloop_van_een_dictaat_mag() {
        let mut taak = Taak::nieuw(TaakId(1), Soort::Dictaat);
        assert_eq!(taak.toestand(), &Toestand::Wacht);
        for naar in [
            Toestand::Luistert,
            verwerkt(None),
            verwerkt(Some(40)),
            verwerkt(Some(100)),
            Toestand::Klaar,
        ] {
            let gebeurtenis = taak.ga_naar(naar.clone()).unwrap();
            assert_eq!(gebeurtenis.toestand, naar);
            assert_eq!(gebeurtenis.taak, TaakId(1));
            assert_eq!(gebeurtenis.soort, Soort::Dictaat);
        }
    }

    #[test]
    fn een_download_mag_zonder_luisteren_verwerken() {
        assert!(overgang_toegestaan(&Toestand::Wacht, &verwerkt(Some(0))));
    }

    #[test]
    fn afgelopen_taken_veranderen_niet_meer() {
        for van in [Toestand::Klaar, fout(), Toestand::Geannuleerd] {
            assert!(van.is_afgelopen());
            for naar in [
                Toestand::Wacht,
                Toestand::Luistert,
                verwerkt(None),
                Toestand::Klaar,
                fout(),
                Toestand::Geannuleerd,
            ] {
                assert!(
                    !overgang_toegestaan(&van, &naar),
                    "{van:?} naar {naar:?} had niet gemogen"
                );
            }
        }
    }

    #[test]
    fn andere_verboden_overgangen() {
        // Niet terug naar eerder, en niet klaar zonder te verwerken.
        assert!(!overgang_toegestaan(&verwerkt(None), &Toestand::Luistert));
        assert!(!overgang_toegestaan(&verwerkt(None), &Toestand::Wacht));
        assert!(!overgang_toegestaan(&Toestand::Luistert, &Toestand::Klaar));
        assert!(!overgang_toegestaan(&Toestand::Wacht, &Toestand::Klaar));
        assert!(!overgang_toegestaan(
            &Toestand::Luistert,
            &Toestand::Luistert
        ));
        // Een voortgang boven 100 bestaat niet.
        assert!(!overgang_toegestaan(&verwerkt(None), &verwerkt(Some(101))));
    }

    #[test]
    fn annuleren_en_falen_mag_overal_waar_de_taak_nog_loopt() {
        for van in [Toestand::Wacht, Toestand::Luistert, verwerkt(Some(5))] {
            assert!(van.is_actief());
            assert!(overgang_toegestaan(&van, &Toestand::Geannuleerd));
            assert!(overgang_toegestaan(&van, &fout()));
        }
    }

    #[test]
    fn verboden_overgang_laat_de_taak_ongemoeid() {
        let mut taak = Taak::nieuw(TaakId(2), Soort::Bestand);
        taak.ga_naar(Toestand::Geannuleerd).unwrap();
        let fout = taak.ga_naar(Toestand::Klaar).unwrap_err();
        assert_eq!(fout.van, Toestand::Geannuleerd);
        assert_eq!(fout.naar, Toestand::Klaar);
        assert_eq!(taak.toestand(), &Toestand::Geannuleerd);
    }

    #[test]
    fn foutvertaling_volgt_de_soort_en_niet_de_tekst() {
        let engine = |soort| EngineFout::Engine {
            soort,
            // Bewust misleidende tekst: de soort beslist.
            melding: "kies een ander model, download opnieuw, microfoon".into(),
        };
        let gevallen = [
            (engine(FoutSoort::ModelLaden), Vervolg::AnderModelKiezen),
            (
                engine(FoutSoort::ModelOntbreekt),
                Vervolg::OpnieuwDownloaden,
            ),
            (engine(FoutSoort::Transcriberen), Vervolg::OpnieuwProberen),
            (engine(FoutSoort::NietGeladen), Vervolg::OpnieuwProberen),
            (engine(FoutSoort::Audio), Vervolg::Geen),
            (engine(FoutSoort::Verzoek), Vervolg::Geen),
            (
                EngineFout::Gecrasht {
                    code: Some(139),
                    laatste_log: Vec::new(),
                },
                Vervolg::OpnieuwProberen,
            ),
            (
                EngineFout::TijdOverschreden {
                    fase: Fase::Transcriberen,
                    na: Duration::from_secs(5),
                },
                Vervolg::OpnieuwProberen,
            ),
            (EngineFout::NietGeladen, Vervolg::OpnieuwProberen),
            (
                EngineFout::Starten {
                    reden: "weg".into(),
                },
                Vervolg::Geen,
            ),
            (EngineFout::Protocol("x".into()), Vervolg::Geen),
            (
                EngineFout::NietBijgewerkt {
                    gevonden: None,
                    verwacht: "0.3.0-dev+afd2907".into(),
                    protocol: 4,
                },
                Vervolg::OpnieuwInstalleren,
            ),
            (
                EngineFout::TeLang {
                    duur: Duration::from_secs(700),
                    max: Duration::from_secs(600),
                },
                Vervolg::Geen,
            ),
        ];
        for (fout, verwacht) in gevallen {
            assert_eq!(Vervolg::voor_engine_fout(&fout), verwacht, "{fout:?}");
            assert_eq!(
                Vervolg::voor_aanbieder_fout(&AanbiederFout::Engine(fout)),
                verwacht
            );
        }
        let kapot = AanbiederFout::Audio(AudioFout::NietGevonden(PathBuf::from("x.wav")));
        assert_eq!(Vervolg::voor_aanbieder_fout(&kapot), Vervolg::Geen);
    }

    #[test]
    fn toestand_uit_fout_neemt_de_melding_over() {
        let fout = EngineFout::NietGeladen;
        assert_eq!(
            Toestand::uit_engine_fout(&fout),
            Toestand::fout("Er is nog geen model geladen.", Vervolg::OpnieuwProberen)
        );
        let aanbieder = AanbiederFout::Engine(EngineFout::NietGeladen);
        assert_eq!(
            Toestand::uit_aanbieder_fout(&aanbieder),
            Toestand::fout("Er is nog geen model geladen.", Vervolg::OpnieuwProberen)
        );
    }

    #[test]
    fn json_vorm_van_een_gebeurtenis_is_stabiel() {
        let json = |toestand| {
            serde_json::to_string(&Gebeurtenis {
                taak: TaakId(7),
                soort: Soort::Dictaat,
                toestand,
            })
            .unwrap()
        };
        assert_eq!(
            json(Toestand::Luistert),
            r#"{"taak":7,"soort":"dictaat","toestand":{"type":"luistert"}}"#
        );
        assert_eq!(
            json(verwerkt(Some(40))),
            r#"{"taak":7,"soort":"dictaat","toestand":{"type":"verwerkt","voortgang":40}}"#
        );
        assert_eq!(
            json(verwerkt(None)),
            r#"{"taak":7,"soort":"dictaat","toestand":{"type":"verwerkt","voortgang":null}}"#
        );
        // Sprekers herkennen (4b-D) noemt de stap; een bericht zonder stap leest als uitschrijven.
        assert_eq!(
            json(Toestand::Verwerkt {
                voortgang: Some(10),
                stap: Stap::Sprekers
            }),
            r#"{"taak":7,"soort":"dictaat","toestand":{"type":"verwerkt","voortgang":10,"stap":"sprekers"}}"#
        );
        assert_eq!(
            serde_json::from_str::<Toestand>(r#"{"type":"verwerkt","voortgang":5}"#).unwrap(),
            verwerkt(Some(5))
        );
        assert_eq!(
            json(Toestand::fout(
                "Kies een ander model.",
                Vervolg::AnderModelKiezen
            )),
            r#"{"taak":7,"soort":"dictaat","toestand":{"type":"fout","melding":"Kies een ander model.","vervolg":"ander_model_kiezen"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Gebeurtenis {
                taak: TaakId(1),
                soort: Soort::Vergadering,
                toestand: Toestand::Geannuleerd,
            })
            .unwrap(),
            r#"{"taak":1,"soort":"vergadering","toestand":{"type":"geannuleerd"}}"#
        );
    }

    #[test]
    fn gebeurtenis_overleeft_serde() {
        for toestand in [
            Toestand::Wacht,
            Toestand::Luistert,
            verwerkt(Some(100)),
            Toestand::Klaar,
            fout(),
            Toestand::Geannuleerd,
        ] {
            let gebeurtenis = Gebeurtenis {
                taak: TaakId(3),
                soort: Soort::Download,
                toestand,
            };
            let terug: Gebeurtenis =
                serde_json::from_str(&serde_json::to_string(&gebeurtenis).unwrap()).unwrap();
            assert_eq!(terug, gebeurtenis);
        }
    }
}
