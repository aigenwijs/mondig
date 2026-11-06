//! Welk spraakmodel bij welke taak hoort (sprint 5-A, ADR 0014 en ADR 0012).
//!
//! De gebruiker kiest per taak een model: dicteren gebruikt het dicteermodel, bestanden het
//! bestandsmodel. Deze module bevat alleen de regels, zonder schijf of netwerk, zodat ze met
//! gewone tests vast te leggen zijn. De app bewaart de keuzes in de instellingen en vraagt
//! hier welk model nu geldt.
//!
//! - Een bewaarde keuze geldt zolang dat model helemaal op de computer staat.
//! - Anders het eerste complete model in de volgorde van de aanbeveling: dicteren Ultra,
//!   Canary; bestanden Canary, Ultra, Whisper. Een bewaarde keuze voor een model dat niet meer
//!   in de catalogus staat (Parakeet v3, zie `afgeschaft`) telt dus niet: de standaard geldt.
//! - Staat er niets, dan geldt Parakeet Ultra: dat model laat de pagina downloaden.
//! - Een model dat in gebruik is, verwijder je niet.

use crate::model::{self, Gebruik, Model};

/// Het model waarmee Mondig standaard dicteert.
pub const STANDAARD_DICTEREN: &str = model::STANDAARD_MODEL;

/// Het model waarmee Mondig standaard bestanden uitschrijft, als het op de computer staat.
pub const STANDAARD_BESTANDEN: &str = "canary-1b-v2-q8";

/// Whisper large v3 turbo, alleen voor bestanden.
const WHISPER_TURBO: &str = "whisper-large-v3-turbo-q8";

/// Zonder (geldige) keuze: het eerste model uit deze lijst dat compleet is en voor de taak
/// mag. Volgt de aanbeveling van het ontwerp van 5-A.
const VOLGORDE_DICTEREN: [&str; 2] = [STANDAARD_DICTEREN, STANDAARD_BESTANDEN];
const VOLGORDE_BESTANDEN: [&str; 3] = [STANDAARD_BESTANDEN, STANDAARD_DICTEREN, WHISPER_TURBO];

/// De bewaarde keuzes van de gebruiker. `None` is: geen keuze gemaakt, de standaard geldt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keuzes {
    /// Het gekozen dicteermodel.
    pub dicteren: Option<String>,
    /// Het gekozen bestandsmodel.
    pub bestanden: Option<String>,
}

impl Keuzes {
    /// De bewaarde keuze voor deze taak.
    #[must_use]
    pub fn van(&self, gebruik: Gebruik) -> Option<&str> {
        match gebruik {
            Gebruik::Dicteren => self.dicteren.as_deref(),
            Gebruik::Bestanden => self.bestanden.as_deref(),
        }
    }

    /// Onthoudt een keuze voor deze taak.
    pub fn zet(&mut self, gebruik: Gebruik, id: &str) {
        let veld = match gebruik {
            Gebruik::Dicteren => &mut self.dicteren,
            Gebruik::Bestanden => &mut self.bestanden,
        };
        *veld = Some(id.to_string());
    }
}

/// Waarom een keuze niet kan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum KeuzeFout {
    /// Het model staat niet in de catalogus of is een hulpmodel.
    #[error("Dit model kent Mondig niet.")]
    Onbekend,
    /// Het model is voor deze taak niet te kiezen (Whisper turbo om mee te dicteren).
    #[error("Dit model is voor deze taak niet te kiezen.")]
    NietKiesbaar,
}

/// Zoekt het model voor een keuze en controleert dat het voor deze taak mag.
///
/// # Errors
///
/// [`KeuzeFout::Onbekend`] als het id geen spraakmodel is, [`KeuzeFout::NietKiesbaar`] als
/// het model voor deze taak is uitgesloten.
pub fn controleer_keuze<'a>(
    catalogus: &'a [Model],
    gebruik: Gebruik,
    id: &str,
) -> Result<&'a Model, KeuzeFout> {
    let model = catalogus
        .iter()
        .find(|m| m.id == id && m.is_spraakmodel())
        .ok_or(KeuzeFout::Onbekend)?;
    if model.kiesbaar_voor(gebruik) {
        Ok(model)
    } else {
        Err(KeuzeFout::NietKiesbaar)
    }
}

/// Het model dat nu geldt voor een taak. `compleet` zegt of een model helemaal op de
/// computer staat.
///
/// 1. Een bewaarde keuze geldt als het model bestaat, voor de taak te kiezen is en compleet is.
/// 2. Anders het eerste complete model dat voor de taak mag, in de volgorde van de
///    aanbeveling (dicteren: Ultra, Canary; bestanden: Canary, Ultra, Whisper), en
///    daarna de rest van de catalogus.
/// 3. Staat er niets: Parakeet Ultra, het model dat de pagina laat downloaden.
#[must_use]
pub fn geldend_model(
    gebruik: Gebruik,
    keuzes: &Keuzes,
    catalogus: &[Model],
    compleet: &dyn Fn(&str) -> bool,
) -> String {
    let bruikbaar = |id: &str| controleer_keuze(catalogus, gebruik, id).is_ok() && compleet(id);
    if let Some(id) = keuzes.van(gebruik)
        && bruikbaar(id)
    {
        return id.to_string();
    }
    let volgorde: &[&str] = match gebruik {
        Gebruik::Dicteren => &VOLGORDE_DICTEREN,
        Gebruik::Bestanden => &VOLGORDE_BESTANDEN,
    };
    volgorde
        .iter()
        .copied()
        .chain(catalogus.iter().map(|m| m.id.as_str()))
        .find(|id| bruikbaar(id))
        .unwrap_or(STANDAARD_DICTEREN)
        .to_string()
}

/// Voor welke taken geldt dit model nu? Leeg als het nergens in gebruik is.
#[must_use]
pub fn gebruikt_voor(id: &str, geldend: &[(Gebruik, String)]) -> Vec<Gebruik> {
    geldend
        .iter()
        .filter(|(_, model)| model == id)
        .map(|(gebruik, _)| *gebruik)
        .collect()
}

/// Mag dit model weg? Alleen als het voor geen enkele taak geldt: wie het enige model
/// verwijdert, kan niet meer dicteren en weet niet waarom.
///
/// # Errors
///
/// Geeft de taken waarvoor het model nog geldt.
pub fn mag_verwijderen(id: &str, geldend: &[(Gebruik, String)]) -> Result<(), Vec<Gebruik>> {
    let taken = gebruikt_voor(id, geldend);
    if taken.is_empty() { Ok(()) } else { Err(taken) }
}

/// Welke modellen krijgen de badge "Nieuw"? Die van de zichtbare modellen die de gebruiker de
/// pagina nog niet heeft zien openen. `gezien` is wat bewaard is (`None`: nooit iets bewaard);
/// `op_schijf` zijn de modellen die al op de computer staan.
///
/// Een eerste installatie (niets bewaard en niets op schijf) heeft niets nieuws: iedereen ziet
/// de hele lijst voor het eerst. Een installatie van vóór 5-A (niets bewaard, wel een model
/// op schijf) kent wat er staat en ziet de rest als nieuw.
#[must_use]
pub fn nieuwe_modellen(
    zichtbaar: &[String],
    gezien: Option<&[String]>,
    op_schijf: &[String],
) -> Vec<String> {
    match gezien {
        None if op_schijf.is_empty() => Vec::new(),
        None => zichtbaar
            .iter()
            .filter(|id| !op_schijf.contains(id))
            .cloned()
            .collect(),
        Some(gezien) => zichtbaar
            .iter()
            .filter(|id| !gezien.contains(id))
            .cloned()
            .collect(),
    }
}

/// De lijst "gezien" na het markeren van `ids` als gezien, of `None` als er niets verandert.
///
/// Is er nog niets bewaard (`gezien` is `None`), dan begint de lijst bij wat de gebruiker
/// volgens [`nieuwe_modellen`] al kent: alles bij een eerste installatie, de modellen op
/// schijf bij een installatie van vóór 5-A. Zo krijgt de rest niet ineens de badge "Nieuw"
/// omdat er één model aan de lijst is toegevoegd.
#[must_use]
pub fn gezien_na(
    zichtbaar: &[String],
    gezien: Option<&[String]>,
    op_schijf: &[String],
    ids: &[String],
) -> Option<Vec<String>> {
    let mut lijst = if let Some(gezien) = gezien {
        gezien.to_vec()
    } else {
        let nieuw = nieuwe_modellen(zichtbaar, None, op_schijf);
        zichtbaar
            .iter()
            .filter(|id| !nieuw.contains(id))
            .cloned()
            .collect()
    };
    let voor = lijst.len();
    for id in ids {
        if !lijst.contains(id) {
            lijst.push(id.clone());
        }
    }
    (gezien.is_none() || lijst.len() != voor).then_some(lijst)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Een afgeschaft model: staat niet meer in de catalogus.
    const V3: &str = "parakeet-tdt-0.6b-v3-q8";
    const WHISPER: &str = WHISPER_TURBO;

    fn catalogus() -> Vec<Model> {
        model::catalogus().unwrap()
    }

    /// Een computer waar alleen deze modellen helemaal op staan.
    fn staat<'a>(ids: &'a [&'a str]) -> impl Fn(&str) -> bool + 'a {
        move |id| ids.contains(&id)
    }

    fn geldend(keuzes: &Keuzes, ids: &[&str], gebruik: Gebruik) -> String {
        geldend_model(gebruik, keuzes, &catalogus(), &staat(ids))
    }

    #[test]
    fn de_standaarden_staan_in_de_catalogus_en_zijn_te_kiezen() {
        let catalogus = catalogus();
        controleer_keuze(&catalogus, Gebruik::Dicteren, STANDAARD_DICTEREN).unwrap();
        controleer_keuze(&catalogus, Gebruik::Bestanden, STANDAARD_BESTANDEN).unwrap();
    }

    #[test]
    fn zonder_keuze_dicteert_een_nieuwe_installatie_met_ultra() {
        let keuzes = Keuzes::default();
        // Ook als er nog niets gedownload is: dat is het model dat de pagina laat downloaden.
        assert_eq!(geldend(&keuzes, &[], Gebruik::Dicteren), STANDAARD_DICTEREN);
        assert_eq!(
            geldend(&keuzes, &[STANDAARD_DICTEREN], Gebruik::Dicteren),
            STANDAARD_DICTEREN
        );
    }

    #[test]
    fn zonder_canary_schrijft_mondig_bestanden_uit_met_het_dicteermodel() {
        let keuzes = Keuzes::default();
        assert_eq!(
            geldend(&keuzes, &[STANDAARD_DICTEREN], Gebruik::Bestanden),
            STANDAARD_DICTEREN
        );
        // Nog niets op schijf: ook dan hetzelfde model als voor dicteren, zodat er één
        // download genoeg is om beide te laten werken.
        assert_eq!(
            geldend(&keuzes, &[], Gebruik::Bestanden),
            STANDAARD_DICTEREN
        );
    }

    #[test]
    fn met_canary_op_schijf_is_dat_het_bestandsmodel_zonder_dat_de_gebruiker_iets_koos() {
        let keuzes = Keuzes::default();
        let ids = [STANDAARD_DICTEREN, STANDAARD_BESTANDEN];
        assert_eq!(
            geldend(&keuzes, &ids, Gebruik::Bestanden),
            STANDAARD_BESTANDEN
        );
        // Dicteren blijft Ultra: Canary is daar geen standaard.
        assert_eq!(
            geldend(&keuzes, &ids, Gebruik::Dicteren),
            STANDAARD_DICTEREN
        );
    }

    #[test]
    fn v3_staat_niet_meer_in_de_catalogus_en_is_geen_keuze() {
        let catalogus = catalogus();
        assert!(catalogus.iter().all(|m| m.id != V3));
        for gebruik in Gebruik::ALLE {
            assert_eq!(
                controleer_keuze(&catalogus, gebruik, V3),
                Err(KeuzeFout::Onbekend)
            );
        }
    }

    #[test]
    fn een_bewaarde_keuze_voor_v3_valt_terug_op_de_standaard() {
        // Een installatie van vóór 1 november 2026 kan v3 nog bewaard hebben, voor dicteren of
        // bestanden, en de map kan er (nog) staan. Het model telt niet meer.
        let mut keuzes = Keuzes::default();
        keuzes.zet(Gebruik::Dicteren, V3);
        keuzes.zet(Gebruik::Bestanden, V3);
        assert_eq!(
            geldend(&keuzes, &[V3, STANDAARD_DICTEREN], Gebruik::Dicteren),
            STANDAARD_DICTEREN
        );
        assert_eq!(
            geldend(&keuzes, &[V3, STANDAARD_DICTEREN], Gebruik::Bestanden),
            STANDAARD_DICTEREN
        );
        // Met Canary erbij krijgt bestanden zijn gewone standaard.
        assert_eq!(
            geldend(
                &keuzes,
                &[V3, STANDAARD_DICTEREN, STANDAARD_BESTANDEN],
                Gebruik::Bestanden
            ),
            STANDAARD_BESTANDEN
        );
        // Alleen v3 op de computer: niets bruikbaar, dus Ultra (om te downloaden).
        assert_eq!(
            geldend(&keuzes, &[V3], Gebruik::Dicteren),
            STANDAARD_DICTEREN
        );
        assert_eq!(
            geldend(&keuzes, &[V3], Gebruik::Bestanden),
            STANDAARD_DICTEREN
        );
    }

    #[test]
    fn een_bewaarde_keuze_gaat_voor_de_standaard() {
        let mut keuzes = Keuzes::default();
        keuzes.zet(Gebruik::Bestanden, STANDAARD_DICTEREN);
        let ids = [STANDAARD_DICTEREN, STANDAARD_BESTANDEN];
        assert_eq!(
            geldend(&keuzes, &ids, Gebruik::Bestanden),
            STANDAARD_DICTEREN
        );
        // De keuze voor bestanden raakt dicteren niet.
        assert_eq!(
            geldend(&keuzes, &ids, Gebruik::Dicteren),
            STANDAARD_DICTEREN
        );
    }

    #[test]
    fn dicteren_en_bestanden_kunnen_een_ander_model_hebben() {
        let mut keuzes = Keuzes::default();
        keuzes.zet(Gebruik::Dicteren, STANDAARD_DICTEREN);
        keuzes.zet(Gebruik::Bestanden, WHISPER);
        let ids = [STANDAARD_DICTEREN, WHISPER];
        assert_eq!(
            geldend(&keuzes, &ids, Gebruik::Dicteren),
            STANDAARD_DICTEREN
        );
        assert_eq!(geldend(&keuzes, &ids, Gebruik::Bestanden), WHISPER);
    }

    #[test]
    fn een_bewaarde_keuze_die_niet_meer_klopt_valt_terug_op_de_standaard() {
        // Whisper turbo is te traag om mee te dicteren: een bewaarde waarde (door een oudere
        // versie, of met de hand in het bestand gezet) wordt genegeerd.
        let mut keuzes = Keuzes::default();
        keuzes.zet(Gebruik::Dicteren, WHISPER);
        assert_eq!(
            geldend(&keuzes, &[WHISPER], Gebruik::Dicteren),
            STANDAARD_DICTEREN
        );
        // Een model dat niet bestaat, idem.
        keuzes.zet(Gebruik::Bestanden, "bestaat-niet");
        assert_eq!(
            geldend(&keuzes, &[STANDAARD_DICTEREN], Gebruik::Bestanden),
            STANDAARD_DICTEREN
        );
        // En een hulpmodel is geen keuze.
        keuzes.zet(Gebruik::Dicteren, model::SILERO_MODEL);
        assert_eq!(geldend(&keuzes, &[], Gebruik::Dicteren), STANDAARD_DICTEREN);
    }

    #[test]
    fn een_bewaarde_keuze_geldt_alleen_als_dat_model_compleet_is() {
        // Canary gekozen om te dicteren, maar Canary is weg (of half): Ultra neemt het over.
        let mut keuzes = Keuzes::default();
        keuzes.zet(Gebruik::Dicteren, STANDAARD_BESTANDEN);
        assert_eq!(
            geldend(&keuzes, &[STANDAARD_DICTEREN], Gebruik::Dicteren),
            STANDAARD_DICTEREN
        );
        // Whisper gekozen voor bestanden en niet compleet: Canary, als dat er staat.
        keuzes.zet(Gebruik::Bestanden, WHISPER);
        assert_eq!(
            geldend(&keuzes, &[STANDAARD_BESTANDEN], Gebruik::Bestanden),
            STANDAARD_BESTANDEN
        );
        // Staat het gekozen model er wel, dan geldt het.
        assert_eq!(
            geldend(&keuzes, &[STANDAARD_BESTANDEN, WHISPER], Gebruik::Bestanden),
            WHISPER
        );
    }

    #[test]
    fn zonder_ultra_dicteert_mondig_met_canary() {
        let keuzes = Keuzes::default();
        assert_eq!(
            geldend(&keuzes, &[STANDAARD_BESTANDEN], Gebruik::Dicteren),
            STANDAARD_BESTANDEN
        );
        // Whisper turbo mag niet voor dicteren, ook als het het enige model is: dan "geen
        // model", het model dat de pagina laat downloaden.
        assert_eq!(
            geldend(&keuzes, &[WHISPER], Gebruik::Dicteren),
            STANDAARD_DICTEREN
        );
    }

    #[test]
    fn de_terugval_volgt_de_aanbeveling_per_taak() {
        let keuzes = Keuzes::default();
        let alles = [STANDAARD_DICTEREN, STANDAARD_BESTANDEN, WHISPER];
        assert_eq!(
            geldend(&keuzes, &alles, Gebruik::Dicteren),
            STANDAARD_DICTEREN
        );
        assert_eq!(
            geldend(&keuzes, &alles, Gebruik::Bestanden),
            STANDAARD_BESTANDEN
        );
        // Dicteren: Canary als Ultra er niet is.
        assert_eq!(
            geldend(&keuzes, &[STANDAARD_BESTANDEN], Gebruik::Dicteren),
            STANDAARD_BESTANDEN
        );
        // Bestanden: Ultra voor Whisper.
        assert_eq!(
            geldend(&keuzes, &[STANDAARD_DICTEREN, WHISPER], Gebruik::Bestanden),
            STANDAARD_DICTEREN
        );
        assert_eq!(geldend(&keuzes, &[WHISPER], Gebruik::Bestanden), WHISPER);
        // Niets op de computer: geen model, dus Ultra (om te downloaden).
        assert_eq!(
            geldend(&keuzes, &[], Gebruik::Bestanden),
            STANDAARD_DICTEREN
        );
    }

    #[test]
    fn de_volgorde_noemt_alleen_bestaande_spraakmodellen() {
        let catalogus = catalogus();
        for id in VOLGORDE_DICTEREN {
            controleer_keuze(&catalogus, Gebruik::Dicteren, id).unwrap();
        }
        for id in VOLGORDE_BESTANDEN {
            controleer_keuze(&catalogus, Gebruik::Bestanden, id).unwrap();
        }
    }

    #[test]
    fn whisper_turbo_is_alleen_voor_bestanden_te_kiezen() {
        let catalogus = catalogus();
        assert_eq!(
            controleer_keuze(&catalogus, Gebruik::Dicteren, WHISPER),
            Err(KeuzeFout::NietKiesbaar)
        );
        assert!(controleer_keuze(&catalogus, Gebruik::Bestanden, WHISPER).is_ok());
    }

    #[test]
    fn onbekende_modellen_en_hulpmodellen_zijn_geen_keuze() {
        let catalogus = catalogus();
        for id in ["bestaat-niet", model::SILERO_MODEL] {
            for gebruik in Gebruik::ALLE {
                assert_eq!(
                    controleer_keuze(&catalogus, gebruik, id),
                    Err(KeuzeFout::Onbekend),
                    "{id}"
                );
            }
        }
    }

    #[test]
    fn wat_in_gebruik_is_verwijder_je_niet() {
        let geldend = [
            (Gebruik::Dicteren, STANDAARD_DICTEREN.to_string()),
            (Gebruik::Bestanden, STANDAARD_BESTANDEN.to_string()),
        ];
        assert_eq!(
            mag_verwijderen(STANDAARD_DICTEREN, &geldend),
            Err(vec![Gebruik::Dicteren])
        );
        assert_eq!(
            mag_verwijderen(STANDAARD_BESTANDEN, &geldend),
            Err(vec![Gebruik::Bestanden])
        );
        assert!(mag_verwijderen(WHISPER, &geldend).is_ok());
    }

    #[test]
    fn een_model_voor_beide_taken_noemt_beide_taken() {
        let geldend = [
            (Gebruik::Dicteren, STANDAARD_DICTEREN.to_string()),
            (Gebruik::Bestanden, STANDAARD_DICTEREN.to_string()),
        ];
        assert_eq!(
            mag_verwijderen(STANDAARD_DICTEREN, &geldend),
            Err(vec![Gebruik::Dicteren, Gebruik::Bestanden])
        );
    }

    #[test]
    fn het_enige_model_verwijder_je_niet() {
        // Alleen Ultra op de computer: dat model geldt voor beide taken, dus het blijft staan.
        let keuzes = Keuzes::default();
        let ids = [STANDAARD_DICTEREN];
        let geldend: Vec<(Gebruik, String)> = Gebruik::ALLE
            .into_iter()
            .map(|g| (g, geldend_model(g, &keuzes, &catalogus(), &staat(&ids))))
            .collect();
        assert!(mag_verwijderen(STANDAARD_DICTEREN, &geldend).is_err());
    }

    fn lijst(ids: &[&str]) -> Vec<String> {
        ids.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn nieuw_is_wat_de_gebruiker_nog_niet_zag() {
        let zichtbaar = lijst(&["a", "b", "c"]);
        assert_eq!(
            nieuwe_modellen(&zichtbaar, Some(&lijst(&["a", "b"])), &lijst(&["a"])),
            lijst(&["c"])
        );
        assert!(
            nieuwe_modellen(
                &zichtbaar,
                Some(&lijst(&["a", "b", "c", "oud"])),
                &lijst(&["a"])
            )
            .is_empty()
        );
    }

    #[test]
    fn gezien_markeren_bij_een_eerste_installatie_houdt_alles_bekend() {
        // Niets bewaard, niets op schijf, en de eerste download markeert Ultra als gezien:
        // de andere modellen worden daardoor niet "nieuw".
        let zichtbaar = lijst(&["a", "b", "c"]);
        let na = gezien_na(&zichtbaar, None, &[], &lijst(&["a"])).unwrap();
        assert_eq!(na, lijst(&["a", "b", "c"]));
        assert!(nieuwe_modellen(&zichtbaar, Some(&na), &lijst(&["a"])).is_empty());
    }

    #[test]
    fn gezien_markeren_bij_een_installatie_van_voor_5a_houdt_de_rest_nieuw() {
        // Niets bewaard, "a" op schijf; de gebruiker downloadt "b". Alleen "c" blijft
        // nieuw, zoals voor de download.
        let zichtbaar = lijst(&["a", "b", "c"]);
        let na = gezien_na(&zichtbaar, None, &lijst(&["a"]), &lijst(&["b"])).unwrap();
        assert_eq!(na, lijst(&["a", "b"]));
        assert_eq!(
            nieuwe_modellen(&zichtbaar, Some(&na), &lijst(&["a", "b"])),
            lijst(&["c"])
        );
    }

    #[test]
    fn gezien_markeren_met_een_bewaarde_lijst_voegt_alleen_toe() {
        let zichtbaar = lijst(&["a", "b", "c"]);
        let bewaard = lijst(&["a"]);
        assert_eq!(
            gezien_na(&zichtbaar, Some(&bewaard), &[], &lijst(&["c"])),
            Some(lijst(&["a", "c"]))
        );
        // Al gezien: niets te schrijven.
        assert_eq!(
            gezien_na(&zichtbaar, Some(&bewaard), &[], &lijst(&["a"])),
            None
        );
    }

    #[test]
    fn een_eerste_installatie_heeft_niets_nieuws_maar_een_oudere_installatie_wel() {
        let zichtbaar = lijst(&["a", "b", "c"]);
        assert!(nieuwe_modellen(&zichtbaar, None, &[]).is_empty());
        // Wat er al stond, kent de gebruiker; de rest is nieuw.
        assert_eq!(
            nieuwe_modellen(&zichtbaar, None, &lijst(&["a"])),
            lijst(&["b", "c"])
        );
    }
}
