//! Foutmeldingen per soort in gewone taal, met één vervolgactie (S-2c, idee uit Vibe).
//!
//! Vibe onderscheidt in `desktop/src/lib/server-errors.ts` (MIT) vijf soorten problemen die de
//! gebruiker anders te zien krijgt als "er ging iets mis": onbruikbare audio, een model dat
//! ontbreekt, te weinig videogeheugen (de enige oplossing is terugvallen op de processor), te
//! weinig werkgeheugen (de processor faalt dan hetzelfde, dus geen terugval) en een volle schijf.
//! Het patroon is overgenomen, de code is eigen (ADR 0006).
//!
//! De soort komt waar mogelijk uit de soort fout ([`FoutSoort`], [`io::ErrorKind`]) en nooit
//! uit de tekst van een melding. Alleen een werkproces dat sterft zonder foutcode (een
//! geheugenfout in de driver of in ggml) heeft geen andere aanwijzing dan zijn laatste
//! regels; die zoekt [`geheugenprobleem`] na, zoals Vibe dat ook doet.

use std::io;

use crate::aanbieder::AanbiederFout;
use crate::engine::protocol::FoutSoort;
use crate::engine::supervisor::EngineFout;
use crate::taak::{Foutmelding, Vervolg};

/// Een soort probleem waar Mondig een vaste, begrijpelijke uitleg bij heeft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probleem {
    /// Het geluid is niet te gebruiken (kapot bestand, niets te horen, onbekend formaat).
    OnbruikbaarGeluid,
    /// De videokaart heeft te weinig geheugen. Mondig valt dan terug op de processor.
    VideogeheugenTeWeinig,
    /// Het werkgeheugen van de computer is op. De processor faalt dan op dezelfde manier,
    /// dus er is geen terugval.
    WerkgeheugenTeWeinig,
    /// Het model staat niet (helemaal) op de computer.
    ModelOntbreekt,
    /// De schijf is vol.
    SchijfVol,
}

impl Probleem {
    /// Alle soorten, voor tests en overzichten.
    pub const ALLE: [Probleem; 5] = [
        Probleem::OnbruikbaarGeluid,
        Probleem::VideogeheugenTeWeinig,
        Probleem::WerkgeheugenTeWeinig,
        Probleem::ModelOntbreekt,
        Probleem::SchijfVol,
    ];

    /// Een vaste code zonder tekst van de gebruiker, voor het logboek.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::OnbruikbaarGeluid => "onbruikbaar_geluid",
            Self::VideogeheugenTeWeinig => "videogeheugen_te_weinig",
            Self::WerkgeheugenTeWeinig => "werkgeheugen_te_weinig",
            Self::ModelOntbreekt => "model_ontbreekt",
            Self::SchijfVol => "schijf_vol",
        }
    }

    /// De melding in gewone taal, met wat er aan de hand is en wat de gebruiker kan doen.
    #[must_use]
    pub const fn melding(self) -> &'static str {
        match self {
            Self::OnbruikbaarGeluid => {
                "Mondig kan dit geluid niet gebruiken. Probeer een nieuwe opname of een ander bestand."
            }
            Self::VideogeheugenTeWeinig => {
                "De videokaart heeft te weinig geheugen voor dit model. Mondig rekent daarom op de processor."
            }
            Self::WerkgeheugenTeWeinig => {
                "Er is te weinig werkgeheugen vrij om het model te laden. Sluit andere programma's en probeer het opnieuw."
            }
            Self::ModelOntbreekt => {
                "Het spraakmodel ontbreekt of is niet compleet. Download het opnieuw."
            }
            Self::SchijfVol => {
                "De schijf is vol. Maak ruimte vrij en probeer het opnieuw; wat al binnen is, blijft bewaard."
            }
        }
    }

    /// De ene vervolgactie. Bij een kapot geluid helpt alleen een ander bestand, en bij te
    /// weinig videogeheugen gebeurt het vervolg (terugvallen) al vanzelf.
    #[must_use]
    pub const fn vervolg(self) -> Vervolg {
        match self {
            Self::OnbruikbaarGeluid | Self::VideogeheugenTeWeinig => Vervolg::Geen,
            Self::WerkgeheugenTeWeinig | Self::SchijfVol => Vervolg::OpnieuwProberen,
            Self::ModelOntbreekt => Vervolg::OpnieuwDownloaden,
        }
    }

    /// Welk probleem hoort bij deze vaste melding? Voor het logboek: een taak die faalde
    /// draagt alleen de tekst van de melding, en alleen een tekst die Mondig zelf uit
    /// [`Probleem::melding`] koos wordt herkend. Elke andere tekst geeft `None` en komt dus nooit
    /// in het logboek.
    #[must_use]
    pub fn uit_melding(melding: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|p| p.melding() == melding)
    }

    /// De foutmelding met vervolgactie.
    #[must_use]
    pub fn foutmelding(self) -> Foutmelding {
        Foutmelding::nieuw(self.melding(), self.vervolg())
    }

    /// Probleem bij een I/O-fout: alleen een volle schijf heeft een eigen soort.
    #[must_use]
    pub fn uit_io(soort: io::ErrorKind) -> Option<Self> {
        (soort == io::ErrorKind::StorageFull).then_some(Self::SchijfVol)
    }

    /// Probleem bij een fout van het werkproces. `op_videokaart` zegt of de motor op de
    /// videokaart rekende: een geheugenfout is dan een tekort aan videogeheugen.
    #[must_use]
    pub fn uit_engine_fout(fout: &EngineFout, op_videokaart: bool) -> Option<Self> {
        match fout {
            EngineFout::Engine { soort, melding } => match soort {
                FoutSoort::ModelOntbreekt => Some(Self::ModelOntbreekt),
                FoutSoort::Audio => Some(Self::OnbruikbaarGeluid),
                FoutSoort::ModelLaden | FoutSoort::Transcriberen => {
                    geheugenprobleem([melding.as_str()], op_videokaart)
                }
                FoutSoort::NietGeladen | FoutSoort::Verzoek | FoutSoort::Sprekermodel => None,
            },
            EngineFout::Gecrasht { laatste_log, .. } => {
                geheugenprobleem(laatste_log.iter().map(String::as_str), op_videokaart)
            }
            EngineFout::Starten { .. }
            | EngineFout::TijdOverschreden { .. }
            | EngineFout::NietBijgewerkt { .. }
            | EngineFout::Protocol(_)
            | EngineFout::NietGeladen
            | EngineFout::Geannuleerd
            | EngineFout::Intern
            | EngineFout::TeLang { .. } => None,
        }
    }

    /// Probleem bij een fout van een aanbieder. Een kapot bestand dat Mondig zelf al
    /// afwees (te lang, niet gevonden) houdt zijn eigen, specifiekere melding.
    #[must_use]
    pub fn uit_aanbieder_fout(fout: &AanbiederFout, op_videokaart: bool) -> Option<Self> {
        match fout {
            AanbiederFout::Engine(engine) => Self::uit_engine_fout(engine, op_videokaart),
            AanbiederFout::Audio(_) => None,
        }
    }
}

/// Zoekt in de regels die een werkproces schreef naar tekenen dat het geheugen op was.
///
/// - Een tekort aan geheugen van het apparaat zelf ("device memory", Vulkan, CUDA): altijd
///   videogeheugen.
/// - Rust die afbreekt met "memory allocation of N bytes failed": videogeheugen als de motor
///   op de videokaart rekende, anders werkgeheugen (Vibe `ALLOCATION_FAILED`).
/// - Andere meldingen over geheugen ("out of memory", `bad_alloc`): werkgeheugen.
fn geheugenprobleem<'a>(
    regels: impl IntoIterator<Item = &'a str>,
    op_videokaart: bool,
) -> Option<Probleem> {
    let mut afgebroken = false;
    let mut werk = false;
    for regel in regels {
        let regel = regel.to_lowercase();
        if regel.contains("out_of_device_memory")
            || regel.contains("out of device memory")
            || regel.contains("device memory")
                && (regel.contains("alloc") || regel.contains("out of"))
        {
            return Some(Probleem::VideogeheugenTeWeinig);
        }
        if regel.contains("memory allocation of") && regel.contains("failed") {
            afgebroken = true;
        }
        if regel.contains("out of memory")
            || regel.contains("bad_alloc")
            || regel.contains("not enough memory")
            || regel.contains("cannot allocate memory")
        {
            werk = true;
        }
    }
    if afgebroken {
        return Some(if op_videokaart {
            Probleem::VideogeheugenTeWeinig
        } else {
            Probleem::WerkgeheugenTeWeinig
        });
    }
    werk.then_some(Probleem::WerkgeheugenTeWeinig)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(soort: FoutSoort, melding: &str) -> EngineFout {
        EngineFout::Engine {
            soort,
            melding: melding.into(),
        }
    }

    fn gecrasht(regels: &[&str]) -> EngineFout {
        EngineFout::Gecrasht {
            code: Some(-1),
            laatste_log: regels.iter().map(|r| (*r).to_string()).collect(),
        }
    }

    #[test]
    fn elke_soort_heeft_een_melding_een_code_en_een_vervolg() {
        let mut codes = Vec::new();
        for probleem in Probleem::ALLE {
            assert!(!probleem.melding().is_empty());
            assert!(!probleem.melding().contains('\u{2014}'));
            assert!(!codes.contains(&probleem.code()), "dubbele code");
            codes.push(probleem.code());
            assert_eq!(probleem.foutmelding().vervolg, probleem.vervolg());
        }
    }

    #[test]
    fn alleen_een_vaste_melding_wordt_teruggevonden() {
        for probleem in Probleem::ALLE {
            assert_eq!(Probleem::uit_melding(probleem.melding()), Some(probleem));
        }
        // Een andere tekst, bijvoorbeeld met een bestandsnaam, wordt nooit herkend.
        assert_eq!(
            Probleem::uit_melding("Het bestand is niet gevonden: C:/geheim.wav"),
            None
        );
    }

    #[test]
    fn vervolgacties_volgen_de_soort() {
        assert_eq!(Probleem::OnbruikbaarGeluid.vervolg(), Vervolg::Geen);
        // De terugval op de processor gebeurt al; de gebruiker hoeft niets te doen.
        assert_eq!(Probleem::VideogeheugenTeWeinig.vervolg(), Vervolg::Geen);
        assert_eq!(
            Probleem::WerkgeheugenTeWeinig.vervolg(),
            Vervolg::OpnieuwProberen
        );
        assert_eq!(Probleem::SchijfVol.vervolg(), Vervolg::OpnieuwProberen);
        assert_eq!(
            Probleem::ModelOntbreekt.vervolg(),
            Vervolg::OpnieuwDownloaden
        );
    }

    #[test]
    fn de_soort_van_de_engine_fout_beslist() {
        assert_eq!(
            Probleem::uit_engine_fout(&engine(FoutSoort::ModelOntbreekt, "x"), false),
            Some(Probleem::ModelOntbreekt)
        );
        assert_eq!(
            Probleem::uit_engine_fout(&engine(FoutSoort::Audio, "kapot"), false),
            Some(Probleem::OnbruikbaarGeluid)
        );
        // Een gewone laadfout zonder aanwijzing over geheugen blijft zoals hij was.
        assert_eq!(
            Probleem::uit_engine_fout(&engine(FoutSoort::ModelLaden, "driverfout"), true),
            None
        );
        assert_eq!(
            Probleem::uit_engine_fout(&engine(FoutSoort::Verzoek, "out of memory"), false),
            None,
            "alleen laden en uitschrijven kunnen aan geheugen liggen"
        );
    }

    #[test]
    fn videogeheugen_wordt_herkend_aan_het_apparaat() {
        for regel in [
            "ggml_vulkan: VK_ERROR_OUT_OF_DEVICE_MEMORY",
            "failed to allocate device memory for buffer",
            "CUDA error: out of device memory",
        ] {
            assert_eq!(
                Probleem::uit_engine_fout(&engine(FoutSoort::ModelLaden, regel), false),
                Some(Probleem::VideogeheugenTeWeinig),
                "{regel}"
            );
        }
    }

    #[test]
    fn een_afgebroken_toewijzing_is_videogeheugen_alleen_op_de_videokaart() {
        let log = gecrasht(&["memory allocation of 4294967296 bytes failed"]);
        assert_eq!(
            Probleem::uit_engine_fout(&log, true),
            Some(Probleem::VideogeheugenTeWeinig)
        );
        assert_eq!(
            Probleem::uit_engine_fout(&log, false),
            Some(Probleem::WerkgeheugenTeWeinig)
        );
    }

    #[test]
    fn werkgeheugen_heeft_geen_terugval() {
        for regel in [
            "terminate called after throwing an instance of 'std::bad_alloc'",
            "Out of memory while loading model",
            "cannot allocate memory",
        ] {
            let fout = gecrasht(&["start", regel]);
            // Ook op de videokaart: "out of memory" zonder apparaat erbij is werkgeheugen.
            assert_eq!(
                Probleem::uit_engine_fout(&fout, true),
                Some(Probleem::WerkgeheugenTeWeinig),
                "{regel}"
            );
        }
    }

    #[test]
    fn een_crash_zonder_geheugenregel_blijft_een_gewone_crash() {
        assert_eq!(
            Probleem::uit_engine_fout(&gecrasht(&["access violation"]), true),
            None
        );
        assert_eq!(Probleem::uit_engine_fout(&EngineFout::Intern, true), None);
    }

    #[test]
    fn alleen_een_volle_schijf_heeft_een_io_soort() {
        assert_eq!(
            Probleem::uit_io(io::ErrorKind::StorageFull),
            Some(Probleem::SchijfVol)
        );
        assert_eq!(Probleem::uit_io(io::ErrorKind::NotFound), None);
        assert_eq!(Probleem::uit_io(io::ErrorKind::PermissionDenied), None);
    }

    #[test]
    fn een_kapot_bestand_dat_mondig_zelf_afwees_houdt_zijn_eigen_melding() {
        let fout = AanbiederFout::Audio(crate::audio::AudioFout::IsMap);
        assert_eq!(Probleem::uit_aanbieder_fout(&fout, false), None);
    }
}
