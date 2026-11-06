//! De hardwarecheck en het modeladvies (hoofdstuk 9 van het plan, fase 5, ADR 0011).
//!
//! De app leest de computer uit ([`Computer`]: werkgeheugen, processorkernen, vrije
//! schijfruimte, videokaarten) en vraagt hier [`adviseer`] om een advies in gewone taal, apart
//! voor dicteren en voor bestanden. Deze module doet zelf geen metingen en raakt geen schijf,
//! zodat elke computer met een gewone test na te bootsen is.
//!
//! - Dicteren: altijd Parakeet Ultra. Dat is het snelste model en past op elke computer
//!   waarvoor Mondig bedoeld is.
//! - Bestanden: Canary, tenzij de computer weinig werkgeheugen heeft; dan schrijft Mondig ook
//!   bestanden uit met het dicteermodel, zodat er maar één model in het geheugen staat en er
//!   niet twee downloads nodig zijn. Het tweede model is altijd over te slaan.
//! - De videokaart: alleen aanraden als de proefrun ([`crate::snelheid`]) laat zien dat hij
//!   werkt en minstens anderhalf keer sneller is (ADR 0011). Zonder proef geen aanbeveling,
//!   ook niet bij een dure kaart.
//!
//! Volgorde: de proefrun heeft een model nodig dat al op de computer staat, dus de
//! videokaart wordt pas beoordeeld na de eerste download (afwijking van de volgorde in
//! hoofdstuk 9, die uitging van een proefrun vóór het advies).

use serde::{Deserialize, Serialize};

use crate::engine::protocol::{KaartSoort, Videokaart};
use crate::modelkeuze::{STANDAARD_BESTANDEN, STANDAARD_DICTEREN};
use crate::snelheid::{Oordeel, Vergelijking};

const GIB: u64 = 1024 * 1024 * 1024;

/// Een computer meldt vaak iets minder dan de verpakking zegt (een deel gaat naar de
/// ingebouwde videochip): een laptop van 8 GB toont ongeveer 7,4 GiB. De grenzen liggen
/// daarom iets onder de ronde getallen.
///
/// Onder deze grens (een computer van 4 GB) gaat het uitschrijven moeizaam.
pub const TE_WEINIG_GEHEUGEN: u64 = GIB * 7 / 2;
/// Onder deze grens (minder dan een laptop van 8 GB) krijgt bestanden geen apart model.
pub const WEINIG_GEHEUGEN: u64 = GIB * 7;
/// Vanaf deze grens (een computer van 16 GB) is er ruim genoeg voor twee modellen na elkaar.
pub const RUIM_GEHEUGEN: u64 = GIB * 29 / 2;

/// Wat de app van de computer weet. `None` is: niet te meten.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Computer {
    /// Werkgeheugen in bytes.
    pub geheugen_bytes: Option<u64>,
    /// Aantal processorkernen (logisch).
    pub kernen: Option<u32>,
    /// Vrije schijfruimte op de schijf van de modelmap.
    pub vrij_bytes: Option<u64>,
    /// De videokaarten die het werkproces zag: leeg als er geen is, `None` als zoeken niet lukte.
    pub videokaarten: Option<Vec<Videokaart>>,
}

/// Hoeveel er nog te downloaden is, zodat het advies de schijfruimte kan toetsen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Nodig {
    /// Wat het dicteermodel nog nodig heeft (0 als het er helemaal staat).
    pub dicteren_bytes: u64,
    /// Wat het bestandsmodel nog nodig heeft (0 als het er staat of hetzelfde model is).
    pub bestanden_bytes: u64,
}

/// Hoeveel werkgeheugen de computer heeft, in vier klassen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Geheugen {
    /// Niet te meten: Mondig gaat uit van genoeg.
    Onbekend,
    /// Een computer van 4 GB of minder.
    TeWeinig,
    /// Meer dan 4 maar minder dan 8 GB.
    Weinig,
    /// 8 GB tot 16 GB.
    Genoeg,
    /// 16 GB of meer.
    Ruim,
}

/// De klasse bij een hoeveelheid werkgeheugen.
#[must_use]
pub fn geheugenklasse(bytes: Option<u64>) -> Geheugen {
    match bytes {
        None => Geheugen::Onbekend,
        Some(b) if b < TE_WEINIG_GEHEUGEN => Geheugen::TeWeinig,
        Some(b) if b < WEINIG_GEHEUGEN => Geheugen::Weinig,
        Some(b) if b < RUIM_GEHEUGEN => Geheugen::Genoeg,
        Some(_) => Geheugen::Ruim,
    }
}

/// Wat het advies over de videokaart zegt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideokaartStand {
    /// Er is geen videokaart gevonden (of zoeken lukte niet): Mondig rekent op de processor.
    Geen,
    /// Alleen een videochip in de processor. Die is vaak niet sneller (ADR 0011), dus geen
    /// automatische proef; wie wil, test zelf in Instellingen.
    Ingebouwd,
    /// Er is een losse videokaart die nog niet getest is: eerst de proefrun.
    Testen,
    /// De proefrun toonde dat de videokaart werkt en sneller is: aanzetten loont.
    Aanzetten,
    /// De proefrun toonde dat de videokaart niet sneller is: laat hem uit.
    Laten,
    /// De videokaart gaf een fout in de proefrun: laat hem uit.
    Faalde,
}

/// Het advies over de videokaart, met de winst als die gemeten is.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideokaartAdvies {
    /// Wat Mondig aanraadt.
    pub stand: VideokaartStand,
    /// Hoeveel keer sneller de videokaart is, als gemeten.
    pub keer_sneller: Option<f64>,
}

/// Het advies voor deze computer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Advies {
    /// Hoeveel werkgeheugen de computer heeft.
    pub geheugen: Geheugen,
    /// Het model om mee te dicteren.
    pub dicteren: String,
    /// Het model voor bestanden. Gelijk aan `dicteren` als de computer geen tweede model aankan.
    pub bestanden: String,
    /// Is voor bestanden een tweede model aan te raden (en dus een tweede download)?
    pub bestanden_apart: bool,
    /// Een computer van 8 tot 12 GB: twee modellen passen, maar Mondig laadt het tweede alleen
    /// tijdens het uitschrijven. De interface zegt dat erbij.
    pub krap: bool,
    /// Hoeveel schijfruimte er te weinig is voor alleen het dicteermodel (met marge); 0 als het past.
    pub tekort_dicteren_bytes: u64,
    /// Hoeveel schijfruimte er te weinig is voor beide modellen samen; 0 als het past.
    pub tekort_beide_bytes: u64,
    /// Wat Mondig over de videokaart zegt.
    pub videokaart: VideokaartAdvies,
}

/// Het advies voor `computer`. `nodig` is wat er nog te downloaden is voor het dicteermodel
/// en het aangeraden bestandsmodel (0 voor een model dat er al staat; de app kent die
/// groottes uit de catalogus). `proef` is de uitkomst van de snelheidstest, als die al liep.
#[must_use]
pub fn adviseer(computer: &Computer, nodig: &Nodig, proef: Option<&Vergelijking>) -> Advies {
    let geheugen = geheugenklasse(computer.geheugen_bytes);
    let apart = !matches!(geheugen, Geheugen::TeWeinig | Geheugen::Weinig);
    let bestanden = if apart {
        STANDAARD_BESTANDEN
    } else {
        STANDAARD_DICTEREN
    };
    let krap = apart && computer.geheugen_bytes.is_some_and(|b| b < GIB * 12);
    let bestanden_nodig = if apart { nodig.bestanden_bytes } else { 0 };
    Advies {
        geheugen,
        dicteren: STANDAARD_DICTEREN.to_string(),
        bestanden: bestanden.to_string(),
        bestanden_apart: apart,
        krap,
        tekort_dicteren_bytes: ruimtetekort(computer.vrij_bytes, nodig.dicteren_bytes),
        tekort_beide_bytes: ruimtetekort(
            computer.vrij_bytes,
            nodig.dicteren_bytes.saturating_add(bestanden_nodig),
        ),
        videokaart: videokaart_advies(computer.videokaarten.as_deref(), proef),
    }
}

/// Hoeveel schijfruimte er te weinig is, met dezelfde marge als de downloads zelf.
fn ruimtetekort(vrij: Option<u64>, nog_nodig: u64) -> u64 {
    crate::download::ruimtetekort(vrij, nog_nodig)
}

/// De videokaart alleen aanraden als de proefrun het laat zien (ADR 0011).
fn videokaart_advies(
    kaarten: Option<&[Videokaart]>,
    proef: Option<&Vergelijking>,
) -> VideokaartAdvies {
    let zonder = |stand| VideokaartAdvies {
        stand,
        keer_sneller: None,
    };
    let Some(kaarten) = kaarten.filter(|k| !k.is_empty()) else {
        return zonder(VideokaartStand::Geen);
    };
    if let Some(proef) = proef {
        return match proef.oordeel {
            Oordeel::Loont => VideokaartAdvies {
                stand: VideokaartStand::Aanzetten,
                keer_sneller: proef.keer_sneller,
            },
            Oordeel::LoontNiet => VideokaartAdvies {
                stand: VideokaartStand::Laten,
                keer_sneller: proef.keer_sneller,
            },
            Oordeel::VideokaartFaalde => zonder(VideokaartStand::Faalde),
        };
    }
    if kaarten.iter().any(|k| k.soort == KaartSoort::Los) {
        zonder(VideokaartStand::Testen)
    } else {
        zonder(VideokaartStand::Ingebouwd)
    }
}

/// Hoe lang een download van `bytes` ongeveer duurt, in hele minuten: eerst bij een snelle
/// verbinding (100 Mbit/s), dan bij een trage (20 Mbit/s). Minstens een minuut. Geen
/// meting, alleen een bandbreedte om de verwachting te zetten.
#[must_use]
pub fn download_minuten(bytes: u64) -> (u64, u64) {
    let minuten = |mbit_per_s: u64| {
        let bytes_per_minuut = mbit_per_s * 1_000_000 / 8 * 60;
        bytes.div_ceil(bytes_per_minuut).max(1)
    };
    (minuten(100), minuten(20))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1_000_000;

    fn computer(gib: Option<u64>) -> Computer {
        Computer {
            geheugen_bytes: gib.map(|g| g * GIB),
            kernen: Some(8),
            vrij_bytes: Some(200 * GIB),
            videokaarten: Some(Vec::new()),
        }
    }

    fn kaart(soort: KaartSoort) -> Videokaart {
        Videokaart {
            naam: "Testkaart".into(),
            soort,
        }
    }

    const NODIG: Nodig = Nodig {
        dicteren_bytes: 740 * MB,
        bestanden_bytes: 1144 * MB,
    };

    fn vergelijking(oordeel: Oordeel, keer: Option<f64>) -> Vergelijking {
        Vergelijking {
            oordeel,
            keer_sneller: keer,
            processor: 0.2,
            videokaart: keer.map(|k| 0.2 / k),
        }
    }

    #[test]
    fn een_kantoorlaptop_van_16_gb_krijgt_ultra_en_canary() {
        let advies = adviseer(&computer(Some(16)), &NODIG, None);
        assert_eq!(advies.geheugen, Geheugen::Ruim);
        assert_eq!(advies.dicteren, STANDAARD_DICTEREN);
        assert_eq!(advies.bestanden, STANDAARD_BESTANDEN);
        assert!(advies.bestanden_apart);
        assert!(!advies.krap);
        assert_eq!(advies.tekort_beide_bytes, 0);
    }

    #[test]
    fn een_laptop_van_8_gb_krijgt_canary_met_de_kanttekening_krap() {
        // Windows toont op zo'n laptop ruim 7 GiB.
        let mut c = computer(None);
        c.geheugen_bytes = Some(GIB * 74 / 10);
        let advies = adviseer(&c, &NODIG, None);
        assert_eq!(advies.geheugen, Geheugen::Genoeg);
        assert!(advies.bestanden_apart);
        assert!(advies.krap);
    }

    #[test]
    fn met_weinig_geheugen_blijft_het_bij_een_model() {
        for gib in [2, 3, 4, 6] {
            let advies = adviseer(&computer(Some(gib)), &NODIG, None);
            assert!(!advies.bestanden_apart, "{gib} GiB");
            assert_eq!(advies.bestanden, advies.dicteren, "{gib} GiB");
            assert!(!advies.krap);
            // Geen tweede download: alleen het dicteermodel telt voor de schijfruimte.
            assert_eq!(advies.tekort_beide_bytes, advies.tekort_dicteren_bytes);
        }
        assert_eq!(geheugenklasse(Some(GIB * 3)), Geheugen::TeWeinig);
        assert_eq!(geheugenklasse(Some(GIB * 5)), Geheugen::Weinig);
    }

    #[test]
    fn de_grenzen_liggen_bij_de_klassen() {
        assert_eq!(geheugenklasse(None), Geheugen::Onbekend);
        assert_eq!(geheugenklasse(Some(WEINIG_GEHEUGEN - 1)), Geheugen::Weinig);
        assert_eq!(geheugenklasse(Some(WEINIG_GEHEUGEN)), Geheugen::Genoeg);
        assert_eq!(geheugenklasse(Some(RUIM_GEHEUGEN - 1)), Geheugen::Genoeg);
        assert_eq!(geheugenklasse(Some(RUIM_GEHEUGEN)), Geheugen::Ruim);
        // Een laptop van 16 GB met een ingebouwde videochip meldt iets minder dan 16 GiB.
        assert_eq!(geheugenklasse(Some(GIB * 15)), Geheugen::Ruim);
    }

    #[test]
    fn onbekend_geheugen_is_geen_reden_om_het_tweede_model_te_weigeren() {
        let advies = adviseer(&computer(None), &NODIG, None);
        assert_eq!(advies.geheugen, Geheugen::Onbekend);
        assert!(advies.bestanden_apart);
        assert!(!advies.krap);
    }

    #[test]
    fn schijfruimte_wordt_apart_getoetst_voor_een_en_voor_beide() {
        let mut c = computer(Some(16));
        // Past het dicteermodel (740 MB + marge) maar niet beide samen (1,9 GB + marge).
        c.vrij_bytes = Some(1_200 * MB);
        let advies = adviseer(&c, &NODIG, None);
        assert_eq!(advies.tekort_dicteren_bytes, 0);
        assert!(advies.tekort_beide_bytes > 0);
        // Helemaal vol.
        c.vrij_bytes = Some(300 * MB);
        assert!(adviseer(&c, &NODIG, None).tekort_dicteren_bytes > 0);
        // Niet te meten: geen reden om te weigeren.
        c.vrij_bytes = None;
        let advies = adviseer(&c, &NODIG, None);
        assert_eq!(advies.tekort_dicteren_bytes, 0);
        assert_eq!(advies.tekort_beide_bytes, 0);
    }

    #[test]
    fn staat_een_model_er_al_dan_telt_het_niet_mee_voor_de_ruimte() {
        let mut c = computer(Some(16));
        c.vrij_bytes = Some(200 * MB);
        let klaar = Nodig::default();
        let advies = adviseer(&c, &klaar, None);
        assert_eq!(advies.tekort_dicteren_bytes, 0);
        assert_eq!(advies.tekort_beide_bytes, 0);
    }

    #[test]
    fn zonder_videokaart_is_er_niets_aan_te_raden() {
        let advies = adviseer(&computer(Some(16)), &NODIG, None);
        assert_eq!(advies.videokaart.stand, VideokaartStand::Geen);
        // Zoeken mislukt: hetzelfde.
        let mut c = computer(Some(16));
        c.videokaarten = None;
        assert_eq!(
            adviseer(&c, &NODIG, None).videokaart.stand,
            VideokaartStand::Geen
        );
        // Een oude proefuitslag zonder kaart telt niet.
        let proef = vergelijking(Oordeel::Loont, Some(9.0));
        assert_eq!(
            adviseer(&computer(Some(16)), &NODIG, Some(&proef))
                .videokaart
                .stand,
            VideokaartStand::Geen
        );
    }

    #[test]
    fn een_losse_kaart_wordt_pas_aangeraden_na_de_proefrun() {
        let mut c = computer(Some(16));
        c.videokaarten = Some(vec![kaart(KaartSoort::Los)]);
        // Zonder proef: eerst testen, nog geen aanbeveling.
        assert_eq!(
            adviseer(&c, &NODIG, None).videokaart.stand,
            VideokaartStand::Testen
        );
        let snel = vergelijking(Oordeel::Loont, Some(9.0));
        let advies = adviseer(&c, &NODIG, Some(&snel)).videokaart;
        assert_eq!(advies.stand, VideokaartStand::Aanzetten);
        assert_eq!(advies.keer_sneller, Some(9.0));
        let traag = vergelijking(Oordeel::LoontNiet, Some(1.1));
        assert_eq!(
            adviseer(&c, &NODIG, Some(&traag)).videokaart.stand,
            VideokaartStand::Laten
        );
        let kapot = vergelijking(Oordeel::VideokaartFaalde, None);
        let advies = adviseer(&c, &NODIG, Some(&kapot)).videokaart;
        assert_eq!(advies.stand, VideokaartStand::Faalde);
        assert_eq!(advies.keer_sneller, None);
    }

    #[test]
    fn een_ingebouwde_chip_krijgt_geen_automatische_proef() {
        let mut c = computer(Some(16));
        c.videokaarten = Some(vec![kaart(KaartSoort::Ingebouwd)]);
        assert_eq!(
            adviseer(&c, &NODIG, None).videokaart.stand,
            VideokaartStand::Ingebouwd
        );
        // Wie hem zelf test, krijgt wel het oordeel van de proef.
        let snel = vergelijking(Oordeel::Loont, Some(2.0));
        assert_eq!(
            adviseer(&c, &NODIG, Some(&snel)).videokaart.stand,
            VideokaartStand::Aanzetten
        );
    }

    #[test]
    fn de_downloadduur_loopt_van_snel_naar_traag() {
        assert_eq!(download_minuten(740 * MB), (1, 5));
        assert_eq!(download_minuten(1144 * MB), (2, 8));
        // Nooit minder dan een minuut.
        assert_eq!(download_minuten(MB), (1, 1));
    }

    #[test]
    fn het_advies_serialiseert_met_camelcase_sleutels() {
        let advies = adviseer(&computer(Some(16)), &NODIG, None);
        let json = serde_json::to_value(&advies).unwrap();
        assert_eq!(json["bestandenApart"], true);
        assert_eq!(json["tekortDicterenBytes"], 0);
        assert_eq!(json["geheugen"], "ruim");
        assert_eq!(json["videokaart"]["stand"], "geen");
    }
}
