//! Tests van de notulen, zonder netwerk.

use std::time::Duration;

use super::*;
use crate::gesprek::{Beurt, Kant, Spoorstand, Vergadersoort};

fn beurt(start_s: u64, naam: &str, tekst: &str) -> Beurt {
    Beurt {
        kant: Kant::Anderen,
        sprekernummer: None,
        start_ms: start_s * 1000,
        eind_ms: start_s * 1000 + 900,
        tekst: tekst.to_string(),
        naam: Some(naam.to_string()),
        verbeterd: false,
    }
}

pub(super) fn gesprek() -> Gesprek {
    Gesprek {
        beurten: vec![
            beurt(
                0,
                "Anouk",
                "Goedemorgen allemaal, we beginnen met het speelveld.",
            ),
            beurt(
                12,
                "Pieter",
                "Ik stel voor dat we het hek aan de noordkant plaatsen.",
            ),
            beurt(
                25,
                "Anouk",
                "Akkoord, dan besluiten we het hek aan de noordkant te plaatsen.",
            ),
            beurt(
                40,
                "Sanne",
                "Ik pak de bewonersbrief op, die gaat voor vrijdag de deur uit. Bel me op 06 12345678.",
            ),
            beurt(61, "Anouk", "Prima. Dan sluiten we af."),
        ],
        duur: Duration::from_secs(70),
        meldingen: Vec::new(),
        jij: Spoorstand::Opgenomen,
        anderen: Spoorstand::Opgenomen,
        weggelaten: 0,
        soort: Vergadersoort::Online,
    }
}

pub(super) fn invoer(gesprek: &Gesprek) -> Invoer {
    Invoer::van(gesprek, &Markeringen::default())
}

pub(super) fn teksten(gesprek: &Gesprek) -> Vec<String> {
    Invoer::van(gesprek, &Markeringen::default()).teksten
}

pub(super) fn notulen_json() -> String {
    json!({
        "samenvatting": "Het overleg ging over het speelveld.",
        "besluiten": [
            { "tekst": "Het hek komt aan de noordkant.",
              "bron": { "beurt": 3, "citaat": "dan besluiten we het hek aan de noordkant te plaatsen" } },
        ],
        "actiepunten": [
            { "wat": "Bewonersbrief versturen", "wie": "Sanne", "wanneer": "vrijdag",
              "bron": { "beurt": 4, "citaat": "Ik pak de bewonersbrief op" } },
            { "wat": "Wethouder informeren", "wie": "", "wanneer": "",
              "bron": { "beurt": 2, "citaat": "de wethouder bellen we morgen" } },
        ],
    })
    .to_string()
}

fn bron(beurt: i64, citaat: &str) -> Bron {
    Bron {
        beurt,
        citaat: citaat.to_string(),
    }
}

#[test]
fn het_schema_gaat_heen_en_terug() {
    let notulen: Notulen = serde_json::from_str(&notulen_json()).unwrap();
    assert_eq!(notulen.besluiten.len(), 1);
    assert_eq!(notulen.actiepunten[0].wie, "Sanne");
    assert_eq!(notulen.actiepunten[1].wie, "");
    let terug: Notulen = serde_json::from_str(&serde_json::to_string(&notulen).unwrap()).unwrap();
    assert!(terug == notulen);

    // De velden van het JSON-schema zijn precies die van de Rust-typen.
    let schema = json_schema();
    let velden = |w: &Value| {
        let mut v: Vec<String> = w["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        velden(&schema),
        ["actiepunten", "besluiten", "samenvatting"]
    );
    assert_eq!(
        velden(&schema["properties"]["actiepunten"]["items"]),
        ["bron", "wanneer", "wat", "wie"]
    );
    assert_eq!(
        velden(&schema["properties"]["besluiten"]["items"]["properties"]["bron"]),
        ["beurt", "citaat"]
    );
    let als_waarde = serde_json::to_value(&notulen).unwrap();
    let mut sleutels: Vec<_> = als_waarde["actiepunten"][0]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    sleutels.sort();
    assert_eq!(sleutels, ["bron", "wanneer", "wat", "wie"]);

    // Een antwoord zonder verplicht veld is geen notulen.
    assert!(serde_json::from_str::<Notulen>(r#"{"samenvatting":"x","besluiten":[]}"#).is_err());
}

#[test]
fn bewaarde_notulen_gaan_heen_en_terug_met_de_controle_per_punt() {
    let g = gesprek();
    let notulen: Notulen = serde_json::from_str(&notulen_json()).unwrap();
    let bewaard = controleer_notulen(notulen, &invoer(&g), &g, herkomst());
    let json = serde_json::to_value(&bewaard).unwrap();
    assert_eq!(json["besluiten"][0]["controle"]["uitkomst"], "gevonden");
    assert_eq!(json["besluiten"][0]["controle"]["beurt"], 3);
    assert_eq!(json["besluiten"][0]["bron"]["beurt"], 3);
    assert_eq!(
        json["actiepunten"][1]["controle"]["uitkomst"],
        "niet_teruggevonden"
    );
    let terug: BewaardeNotulen = serde_json::from_value(json).unwrap();
    assert!(terug == bewaard);
    assert_eq!(terug.telling(), (2, 1));
}

fn herkomst() -> Herkomst {
    Herkomst {
        aanbieder: "mistral".into(),
        model: "mistral-small-latest".into(),
        moment_ms: 1_000,
        taal: Taal::Nl,
    }
}

#[test]
fn een_bron_wordt_gevonden_ook_met_andere_hoofdletters_witruimte_en_leestekens() {
    let t = teksten(&gesprek());
    assert_eq!(
        controleer(&bron(3, "Akkoord dan besluiten we"), &t),
        Controle::Gevonden { beurt: 3 }
    );
    assert_eq!(
        controleer(&bron(3, "  AKKOORD,   dan besluiten... we "), &t),
        Controle::Gevonden { beurt: 3 }
    );
}

#[test]
fn een_bron_in_de_beurt_ervoor_of_erna_telt_als_gevonden_met_de_echte_beurt() {
    let t = teksten(&gesprek());
    // Het model noemde beurt 3, het citaat staat in beurt 2.
    assert_eq!(
        controleer(&bron(3, "aan de noordkant te plaatsen"), &t),
        Controle::Gevonden { beurt: 3 }
    );
    assert_eq!(
        controleer(&bron(3, "hek aan de noordkant plaatsen"), &t),
        Controle::Gevonden { beurt: 2 }
    );
    assert_eq!(
        controleer(&bron(3, "Ik stel voor dat we"), &t),
        Controle::Gevonden { beurt: 2 }
    );
    assert_eq!(
        controleer(&bron(3, "Ik pak de bewonersbrief op"), &t),
        Controle::Gevonden { beurt: 4 }
    );
    // Twee beurten verder telt niet.
    assert_eq!(
        controleer(&bron(2, "Dan sluiten we af"), &t),
        Controle::NietTeruggevonden
    );
}

#[test]
fn een_verzonnen_citaat_of_een_beurt_die_niet_bestaat_is_niet_teruggevonden() {
    let t = teksten(&gesprek());
    assert_eq!(
        controleer(&bron(3, "we schaffen het speelveld af"), &t),
        Controle::NietTeruggevonden
    );
    // Alleen hele woorden: "plaats" staat niet los in de tekst.
    assert_eq!(
        controleer(&bron(3, "plaats"), &t),
        Controle::NietTeruggevonden
    );
    for beurt in [-3, 0, 6, 99] {
        assert_eq!(
            controleer(&bron(beurt, "Akkoord dan besluiten we"), &t),
            Controle::NietTeruggevonden
        );
    }
    // Een citaat van minder dan vier woorden telt niet, ook al staat het er letterlijk.
    assert_eq!(
        controleer(&bron(3, "noordkant te plaatsen"), &t),
        Controle::NietTeruggevonden
    );
    assert_eq!(
        controleer(&bron(3, "Akkoord"), &t),
        Controle::NietTeruggevonden
    );
    // Een leeg citaat, of alleen leestekens.
    assert_eq!(
        controleer(&bron(3, " ... "), &t),
        Controle::NietTeruggevonden
    );
}

#[test]
fn opdelen_gebeurt_op_beurtgrenzen_met_overlap() {
    // Alles past: één deel.
    assert_eq!(delen(&[10, 10, 10], 100, 2), vec![0..3]);
    // Delen van hooguit 30 met twee beurten overlap gevraagd: de overlap is hooguit de helft van
    // een deel (hier één beurt), zodat elk deel minstens een half deel verder begint.
    let stukken = delen(&[10; 10], 30, 2);
    assert_eq!(stukken, vec![0..3, 2..5, 4..7, 6..9, 8..10]);
    let stukken = delen(&[10; 12], 60, 2);
    assert_eq!(stukken, vec![0..6, 4..10, 8..12]);
    let stukken = delen(&[10; 10], 50, 1);
    assert_eq!(stukken, vec![0..5, 4..9, 8..10]);
    // Elke beurt zit in minstens één deel, elk deel past in het budget.
    for stuk in &stukken {
        assert!(stuk.len() * 10 <= 50);
    }
    // Een beurt groter dan het budget is een deel op zich, en het opdelen eindigt.
    assert_eq!(delen(&[5, 100, 5], 20, 3), vec![0..1, 1..2, 2..3]);
    // Overlap groter dan een deel: toch steeds verder.
    assert_eq!(delen(&[10; 4], 20, 9), vec![0..2, 1..3, 2..4]);
    assert!(delen(&[], 20, 2).is_empty());
}

#[test]
fn de_invoer_is_genummerd_met_tijd_en_spreker() {
    let invoer = Invoer::van(&gesprek(), &Markeringen::default());
    assert_eq!(
        invoer.regels[0],
        "[1] 00:00 Anouk: Goedemorgen allemaal, we beginnen met het speelveld."
    );
    assert!(invoer.regels[4].starts_with("[5] 01:01 Anouk: "));
    assert!(invoer.regels[3].contains("06 12345678"));
}

#[test]
fn met_vervangen_aan_krijgt_het_model_plaatsvervangers() {
    let mut m = Markeringen::default();
    m.zet_vervangen(true);
    m.markeer("Sanne").unwrap();
    let invoer = Invoer::van(&gesprek(), &m);
    let alles = invoer.regels.join("\n");
    assert!(!alles.contains("12345678"), "{alles}");
    assert!(alles.contains("[TELEFOON]"));
    assert!(!alles.contains("Sanne"), "ook de sprekernaam is vervangen");
    // De controle gebruikt dezelfde, vervangen tekst.
    assert!(invoer.teksten[3].contains("[TELEFOON]"));
}

#[test]
fn het_kenmerk_hangt_aan_de_opbouw_en_niet_aan_tekst_of_naam() {
    let g = gesprek();
    let k = kenmerk(&g);
    assert!(k.starts_with("fnv2:"));
    let mut andere_naam = g.clone();
    andere_naam.beurten[0].naam = Some("Voorzitter".into());
    assert_eq!(kenmerk(&andere_naam), k);
    // Een tekstverbetering verandert de opbouw niet: de notulen blijven bij dit verslag horen.
    let mut andere_tekst = g.clone();
    andere_tekst.beurten[1].tekst.push('!');
    assert_eq!(kenmerk(&andere_tekst), k);
    let mut andere_tijd = g.clone();
    andere_tijd.beurten[1].start_ms += 1;
    assert_ne!(kenmerk(&andere_tijd), k);
    let mut andere_spreker = g.clone();
    andere_spreker.beurten[1].sprekernummer = Some(2);
    assert_ne!(kenmerk(&andere_spreker), k);
    let mut minder = g;
    minder.beurten.pop();
    assert_ne!(kenmerk(&minder), k);
}

#[test]
fn notulen_in_verslag_json_en_herkennen_van_een_oudere_versie() {
    use crate::verslag;

    let map = tempfile::tempdir().unwrap();
    let g = gesprek();
    let m = Markeringen::default();
    let notulen: Notulen = serde_json::from_str(&notulen_json()).unwrap();
    let bewaard = controleer_notulen(notulen, &invoer(&g), &g, herkomst());

    // Zonder notulen: geen veld, en lezen geeft None.
    verslag::schrijf(
        map.path(),
        &verslag::Bewaren::nieuw(&g, "m", "M", None, &m, None),
    )
    .unwrap();
    let tekst = std::fs::read_to_string(map.path().join(verslag::BESTAND)).unwrap();
    assert!(!tekst.contains("\"notulen\""));
    assert!(verslag::lees(map.path()).unwrap().notulen.is_none());

    // Met notulen: ze komen gelijk terug, bij dezelfde versie van het gesprek.
    verslag::schrijf(
        map.path(),
        &verslag::Bewaren::nieuw(
            &g,
            "m",
            "M",
            None,
            &m,
            Some(&OpgeslagenNotulen::Leesbaar(bewaard.clone())),
        ),
    )
    .unwrap();
    let terug = verslag::lees(map.path()).unwrap();
    let opgeslagen = terug.notulen.unwrap();
    let notulen = opgeslagen.leesbaar().unwrap();
    assert!(*notulen == bewaard);
    assert!(!notulen.bij_vorige_versie(&terug.gesprek));

    // Opnieuw uitgeschreven (andere tekst): de notulen horen bij de vorige versie.
    let mut nieuw = g.clone();
    nieuw.beurten[2].start_ms += 500;
    assert!(notulen.bij_vorige_versie(&nieuw));

    // Notulen die deze versie niet kan lezen: het verslag blijft leesbaar, de notulen worden niet
    // getoond, maar gaan bij het volgende schrijven ongewijzigd terug naar schijf.
    let kapot = std::fs::read_to_string(map.path().join(verslag::BESTAND))
        .unwrap()
        .replace("\"besluiten\":[", "\"besluiten\":[42,");
    std::fs::write(map.path().join(verslag::BESTAND), &kapot).unwrap();
    let terug = verslag::lees(map.path()).unwrap();
    assert_eq!(terug.gesprek.beurten.len(), 5);
    let opgeslagen = terug.notulen.unwrap();
    assert!(opgeslagen.leesbaar().is_none());
    let ruw = match &opgeslagen {
        OpgeslagenNotulen::Onleesbaar(waarde) => waarde.clone(),
        OpgeslagenNotulen::Leesbaar(_) => unreachable!("onleesbaar verwacht"),
    };
    verslag::schrijf(
        map.path(),
        &verslag::Bewaren::nieuw(&g, "m", "M", Some("Nieuwe naam"), &m, Some(&opgeslagen)),
    )
    .unwrap();
    let weer = verslag::lees(map.path()).unwrap().notulen.unwrap();
    assert!(weer == OpgeslagenNotulen::Onleesbaar(ruw));
}

#[test]
fn een_beurtnummer_onder_een_maakt_alleen_dat_punt_niet_teruggevonden() {
    let json = notulen_json().replace("\"beurt\":3", "\"beurt\":-2");
    let notulen: Notulen = serde_json::from_str(&json).unwrap();
    let g = gesprek();
    let bewaard = controleer_notulen(notulen, &invoer(&g), &g, herkomst());
    assert_eq!(bewaard.besluiten[0].controle, Controle::NietTeruggevonden);
    assert!(bewaard.actiepunten[0].controle.gevonden());
    assert_eq!(
        json_schema()["properties"]["besluiten"]["items"]["properties"]["bron"]["properties"]["beurt"]
            ["minimum"],
        1
    );
}

#[test]
fn punten_van_delen_worden_samengevoegd_zonder_dubbele_en_zonder_verlies() {
    let a: Notulen = serde_json::from_str(&notulen_json()).unwrap();
    let mut b = a.clone();
    // Hetzelfde besluit, één beurt verder en met een korter citaat: dubbel.
    b.besluiten[0].bron = bron(4, "het hek aan de noordkant");
    // Een ander actiepunt in een andere beurt: blijft.
    b.actiepunten[1].bron = bron(5, "Dan sluiten we af nu");
    let samen = voeg_punten_samen(vec![a, b]);
    assert_eq!(samen.besluiten.len(), 1);
    assert_eq!(samen.actiepunten.len(), 3);
    assert!(samen.samenvatting.is_empty());
}

#[test]
fn wie_en_wanneer_worden_gecontroleerd_tegen_de_bronbeurt() {
    let g = gesprek();
    let mut notulen: Notulen = serde_json::from_str(&notulen_json()).unwrap();
    // Sanne is de spreker van beurt 4 en "vrijdag" staat erin: allebei goed.
    let goed = controleer_notulen(notulen.clone(), &invoer(&g), &g, herkomst());
    assert!(!goed.actiepunten[0].wie_niet_teruggevonden);
    assert!(!goed.actiepunten[0].wanneer_niet_teruggevonden);
    assert_eq!(goed.veldmarkeringen(), 0);
    // Een naam in de tekst van een buurbeurt telt ook.
    let mut met_naam = g.clone();
    met_naam.beurten[2]
        .tekst
        .push_str(" Pieter, regel jij de offerte?");
    notulen.actiepunten[0].wie = "Pieter".into();
    let ok = controleer_notulen(notulen.clone(), &invoer(&met_naam), &met_naam, herkomst());
    assert!(!ok.actiepunten[0].wie_niet_teruggevonden);
    notulen.actiepunten[0].wanneer = "14 november".into();
    // Geraden: niemand zegt Pieter in beurt 3 tot 5, en 14 november wordt nergens genoemd.
    let fout = controleer_notulen(notulen, &invoer(&g), &g, herkomst());
    assert!(fout.actiepunten[0].wie_niet_teruggevonden);
    assert!(fout.actiepunten[0].wanneer_niet_teruggevonden);
    assert_eq!(fout.veldmarkeringen(), 1);
    // De markering gaat mee in verslag.json.
    let json = serde_json::to_value(&fout).unwrap();
    assert_eq!(json["actiepunten"][0]["wie_niet_teruggevonden"], true);
    assert!(
        json["actiepunten"][1]
            .get("wie_niet_teruggevonden")
            .is_none()
    );
}

#[cfg(feature = "bewaren")]
mod export {
    use super::*;
    use crate::bewaren::{Document, Formaat, lokaal_moment, maak, maak_met_notulen};

    /// Het verslag als tekst, met de persoonsgegevens vervangen zoals de app dat doet.
    fn txt_met(blok: &crate::bewaren::Notulenblok, g: &Gesprek, m: &Markeringen) -> String {
        let mut doc = Document::van_gesprek(g, "Parakeet", lokaal_moment(0));
        for regel in &mut doc.regels {
            regel.tekst = m.voor_uitvoer(&regel.tekst);
            regel.spreker = regel.spreker.take().map(|s| m.voor_naam(&s));
        }
        String::from_utf8(maak_met_notulen(&doc, Some(blok), Formaat::Txt, true).unwrap()).unwrap()
    }

    fn txt(blok: &crate::bewaren::Notulenblok, g: &Gesprek) -> String {
        let doc = Document::van_gesprek(g, "Parakeet", lokaal_moment(0));
        String::from_utf8(maak_met_notulen(&doc, Some(blok), Formaat::Txt, true).unwrap()).unwrap()
    }

    #[test]
    fn de_notulen_staan_voor_het_verslag_met_de_tijd_van_de_bron() {
        let g = gesprek();
        let notulen: Notulen = serde_json::from_str(&notulen_json()).unwrap();
        let bewaard = controleer_notulen(notulen, &invoer(&g), &g, herkomst());
        let uit = txt(&bewaard.als_blok(&g, &Markeringen::default()), &g);
        let notulen_plek = uit.find("Notulen").unwrap();
        let verslag_plek = uit.find("\r\nVerslag\r\n").unwrap();
        let eerste_beurt = uit.find("Goedemorgen allemaal").unwrap();
        assert!(notulen_plek < verslag_plek && verslag_plek < eerste_beurt);
        // Het besluit met de tijd van beurt 3 (00:25), het actiepunt met wie en wanneer.
        assert!(
            uit.contains("00:25  Het hek komt aan de noordkant."),
            "{uit}"
        );
        assert!(uit.contains("00:40  Bewonersbrief versturen (wie: Sanne; wanneer: vrijdag)"));
        // Het verzonnen punt blijft staan, zonder tijd en met het label.
        assert!(uit.contains("\u{2022}  Wethouder informeren [Niet teruggevonden in het verslag]"));
        assert!(uit.contains("Gemaakt door een taalmodel (Mistral, mistral-small-latest)."));
        assert!(uit.contains("Samenvatting gemaakt door een taalmodel; lees het verslag na."));

        // Bij een vorige versie van het gesprek: de melding en geen tijden.
        let mut nieuw = g.clone();
        nieuw.beurten[0].eind_ms += 500;
        let uit = txt(&bewaard.als_blok(&nieuw, &Markeringen::default()), &nieuw);
        assert!(uit.contains("Gemaakt bij de vorige versie van het verslag."));
        assert!(!uit.contains("00:25  Het hek"));
    }

    #[test]
    fn persoonsgegevens_worden_vervangen_zoals_in_het_verslag() {
        let g = gesprek();
        let mut notulen: Notulen = serde_json::from_str(&notulen_json()).unwrap();
        notulen.actiepunten[0].wat = "Sanne belt 06 12345678".into();
        let bewaard = controleer_notulen(notulen, &invoer(&g), &g, herkomst());
        let mut m = Markeringen::default();
        m.zet_vervangen(true);
        m.markeer("Sanne").unwrap();
        let uit = txt_met(&bewaard.als_blok(&g, &m), &g, &m);
        assert!(!uit.contains("12345678"));
        assert!(!uit.contains("Sanne"), "{uit}");
        assert!(uit.contains("[TELEFOON]"));
    }

    #[test]
    fn alle_documentformaten_krijgen_de_notulen_en_ondertitels_niet() {
        let g = gesprek();
        let notulen: Notulen = serde_json::from_str(&notulen_json()).unwrap();
        let bewaard = controleer_notulen(notulen, &invoer(&g), &g, herkomst());
        let blok = bewaard.als_blok(&g, &Markeringen::default());
        let doc = Document::van_gesprek(&g, "Parakeet", lokaal_moment(0));
        for formaat in [Formaat::Md, Formaat::Html] {
            let uit =
                String::from_utf8(maak_met_notulen(&doc, Some(&blok), formaat, true).unwrap())
                    .unwrap();
            assert!(uit.contains("Besluiten"), "{formaat:?}");
            assert!(
                uit.find("Besluiten") < uit.find("Goedemorgen"),
                "{formaat:?}"
            );
        }
        for formaat in [Formaat::Docx, Formaat::Odt, Formaat::Pdf] {
            let met = maak_met_notulen(&doc, Some(&blok), formaat, true).unwrap();
            let zonder = maak(&doc, formaat, true).unwrap();
            assert!(met.len() > zonder.len(), "{formaat:?}");
        }
        for formaat in [Formaat::Srt, Formaat::Vtt] {
            let met = maak_met_notulen(&doc, Some(&blok), formaat, true).unwrap();
            assert_eq!(met, maak(&doc, formaat, true).unwrap(), "{formaat:?}");
        }
    }

    #[test]
    fn engelse_notulen_hebben_engelse_koppen() {
        let g = gesprek();
        let notulen: Notulen = serde_json::from_str(&notulen_json()).unwrap();
        let mut h = herkomst();
        h.taal = Taal::En;
        let bewaard = controleer_notulen(notulen, &invoer(&g), &g, h);
        let uit = txt(&bewaard.als_blok(&g, &Markeringen::default()), &g);
        assert!(uit.contains("Minutes") && uit.contains("Action items"));
        assert!(uit.contains("[Not found in the transcript]"));
        // De taal van het document volgt de notulen.
        let doc = Document::van_gesprek(&g, "Parakeet", lokaal_moment(0));
        let blok = bewaard.als_blok(&g, &Markeringen::default());
        let html =
            String::from_utf8(maak_met_notulen(&doc, Some(&blok), Formaat::Html, true).unwrap())
                .unwrap();
        assert!(html.contains("<html lang=\"en\">"));
        let zonder = String::from_utf8(maak(&doc, Formaat::Html, true).unwrap()).unwrap();
        assert!(zonder.contains("<html lang=\"nl\">"));
    }

    #[test]
    fn een_geraden_wie_of_wanneer_krijgt_een_aanduiding_in_de_export() {
        let g = gesprek();
        let mut notulen: Notulen = serde_json::from_str(&notulen_json()).unwrap();
        notulen.actiepunten[0].wie = "Pieter".into();
        let bewaard = controleer_notulen(notulen, &invoer(&g), &g, herkomst());
        let uit = txt(&bewaard.als_blok(&g, &Markeringen::default()), &g);
        assert!(
            uit.contains("(wie: Pieter (niet in het verslag gevonden); wanneer: vrijdag)"),
            "{uit}"
        );
    }
}

#[cfg(feature = "taalmodel")]
mod maken_tests {
    use std::sync::atomic::AtomicBool;

    use super::*;
    use crate::notulen::maken::tests::NepVrager;
    use crate::notulen::maken::{Instellingen, NotulenFout, maak};
    use crate::taalmodel::Voorinstelling;

    fn instellingen() -> Instellingen {
        Instellingen::voor(Voorinstelling::Mistral, "mistral-small-latest", Taal::Nl)
    }

    #[test]
    fn een_kort_verslag_is_een_aanvraag_met_schema_en_controle() {
        let nep = NepVrager::met(&[&notulen_json()]);
        let g = gesprek();
        let mut stappen = Vec::new();
        let uit = maak(
            &nep,
            &g,
            &Markeringen::default(),
            &instellingen(),
            &AtomicBool::new(false),
            &mut |klaar, totaal| stappen.push((klaar, totaal)),
        )
        .unwrap();
        assert_eq!(stappen, vec![(0, 1), (1, 1)]);
        assert_eq!(uit.statistiek.delen, 1);
        assert_eq!(uit.statistiek.aanvragen, 1);
        assert_eq!(uit.statistiek.invoer_tokens, 100);
        assert_eq!(uit.notulen.telling(), (2, 1));
        assert_eq!(uit.notulen.aanbieder, "mistral");
        let gezien = nep.gezien();
        assert_eq!(gezien.len(), 1);
        assert!(gezien[0].schema.is_some());
        assert!(gezien[0].systeem.starts_with("Je maakt notulen"));
        assert!(gezien[0].gebruiker.contains("[3] 00:25 Anouk: Akkoord"));
    }

    #[test]
    fn ongeldig_json_wordt_een_keer_opnieuw_gevraagd_met_de_fout_erbij() {
        let nep = NepVrager::met(&["Hier zijn de notulen: {", &notulen_json()]);
        let g = gesprek();
        let uit = maak(
            &nep,
            &g,
            &Markeringen::default(),
            &instellingen(),
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(uit.statistiek.aanvragen, 2);
        assert_eq!(uit.statistiek.herhaald, 1);
        let gezien = nep.gezien();
        assert!(gezien[1].gebruiker.contains("geen geldig JSON"));
        assert!(gezien[1].gebruiker.starts_with(&gezien[0].gebruiker));

        // Twee keer ongeldig: een duidelijke fout.
        let nep = NepVrager::met(&["nee", "{\"samenvatting\": 3}"]);
        let fout = maak(
            &nep,
            &g,
            &Markeringen::default(),
            &instellingen(),
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .err()
        .unwrap();
        assert_eq!(fout, NotulenFout::OngeldigAntwoord("Mistral"));
        assert!(
            fout.to_string()
                .contains("twee keer geen bruikbare notulen")
        );
        assert_eq!(nep.gezien().len(), 2);
    }

    #[test]
    fn een_lang_verslag_gaat_in_delen_en_wordt_samengevoegd_en_daarna_gecontroleerd() {
        let g = gesprek();
        let mut inst = instellingen();
        // Zo klein dat elke beurt plus overlap een eigen deel wordt.
        inst.budget_tokens = 40;
        inst.overlap_beurten = 1;
        let aantal = crate::notulen::delen(
            &Invoer::van(&g, &Markeringen::default()).schattingen(),
            40,
            1,
        )
        .len();
        assert!(aantal > 1);
        // Elk deel geeft dezelfde punten (zoals bij overlap); het laatste antwoord is alleen de
        // samenvatting.
        let deel = notulen_json();
        let mut antwoorden = vec![deel.as_str(); aantal];
        antwoorden.push(r#"{"samenvatting":"Alles samen."}"#);
        let nep = NepVrager::met(&antwoorden);
        let mut stappen = Vec::new();
        let uit = maak(
            &nep,
            &g,
            &Markeringen::default(),
            &inst,
            &AtomicBool::new(false),
            &mut |klaar, totaal| stappen.push((klaar, totaal)),
        )
        .unwrap();
        assert_eq!(uit.statistiek.delen, aantal);
        assert_eq!(uit.statistiek.aanvragen, aantal + 1);
        assert_eq!(stappen.last(), Some(&(aantal + 1, aantal + 1)));
        let gezien = nep.gezien();
        assert!(
            gezien[0]
                .gebruiker
                .contains(&format!("deel 1 van {aantal}"))
        );
        let laatste = gezien.last().unwrap();
        assert!(
            laatste
                .systeem
                .contains("samenvattingen van opeenvolgende delen")
        );
        assert_eq!(
            laatste.schema.as_ref().map(|s| s.naam.as_str()),
            Some("samenvatting")
        );
        assert!(
            laatste
                .gebruiker
                .contains("Het overleg ging over het speelveld.")
        );
        // De punten zijn door Mondig samengevoegd: dubbele weg, niets gevallen.
        assert_eq!(uit.notulen.samenvatting, "Alles samen.");
        assert_eq!(uit.notulen.besluiten.len(), 1);
        assert_eq!(uit.notulen.actiepunten.len(), 2);
        // De controle gaat over het hele verslag, na het samenvoegen.
        assert_eq!(uit.notulen.telling(), (2, 1));
    }

    #[test]
    fn met_vervangen_aan_krijgt_de_aanbieder_geen_telefoonnummer() {
        let nep = NepVrager::met(&[&notulen_json()]);
        let mut m = Markeringen::default();
        m.zet_vervangen(true);
        maak(
            &nep,
            &gesprek(),
            &m,
            &instellingen(),
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .unwrap();
        let gebruiker = &nep.gezien()[0].gebruiker;
        assert!(!gebruiker.contains("12345678"));
        assert!(gebruiker.contains("[TELEFOON]"));
    }

    #[test]
    fn een_verslag_zonder_tekst_geeft_een_fout_zonder_aanvraag() {
        let nep = NepVrager::met(&[]);
        let mut g = gesprek();
        for b in &mut g.beurten {
            b.tekst = "  ".into();
        }
        let fout = maak(
            &nep,
            &g,
            &Markeringen::default(),
            &instellingen(),
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .err()
        .unwrap();
        assert_eq!(fout, NotulenFout::GeenTekst);
        assert!(nep.gezien().is_empty());
    }

    /// Met een echte [`crate::taalmodel::Verbinding`] naar een nep-server op 127.0.0.1: het
    /// schema gaat mee als `response_format` en het antwoord komt gecontroleerd terug.
    #[test]
    fn via_een_nep_server_met_de_echte_verbinding() {
        use std::sync::{Arc, Mutex};

        use crate::sleutel::Sleutel;
        use crate::taalmodel::Verbinding;

        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let adres = format!("http://{}/v1", server.server_addr().to_ip().unwrap());
        let gezien = Arc::new(Mutex::new(String::new()));
        let g2 = Arc::clone(&gezien);
        let draad = std::thread::spawn(move || {
            let mut verzoek = server.recv().unwrap();
            let mut body = String::new();
            std::io::Read::read_to_string(verzoek.as_reader(), &mut body).unwrap();
            *g2.lock().unwrap() = body;
            let antwoord = json!({
                "choices": [{ "message": { "content": notulen_json() }, "finish_reason": "stop" }],
                "usage": { "prompt_tokens": 321, "completion_tokens": 45 },
            });
            verzoek
                .respond(tiny_http::Response::from_string(antwoord.to_string()))
                .unwrap();
        });
        let verbinding = Verbinding::op_adres(
            Voorinstelling::Mistral,
            &adres,
            Some(Sleutel::nieuw("sk-test-123").unwrap()),
        );
        let uit = maak(
            &verbinding,
            &gesprek(),
            &Markeringen::default(),
            &instellingen(),
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .unwrap();
        draad.join().unwrap();
        assert_eq!(uit.statistiek.invoer_tokens, 321);
        assert_eq!(uit.notulen.telling(), (2, 1));
        let body: Value = serde_json::from_str(&gezien.lock().unwrap()).unwrap();
        assert_eq!(body["response_format"]["json_schema"]["name"], "notulen");
        assert_eq!(body["model"], "mistral-small-latest");
    }
}

#[test]
fn een_wegverbeterd_citaat_wordt_bij_hercontrole_niet_teruggevonden() {
    let mut g = gesprek();
    let notulen: Notulen = serde_json::from_str(&notulen_json()).unwrap();
    let invoer = Invoer::van(&g, &Markeringen::default());
    let mut bewaard = controleer_notulen(notulen, &invoer, &g, herkomst());
    let eerder = bewaard.telling();
    // De gebruiker verbetert de tekst van beurt 3 zo dat het citaat verdwijnt; de opbouw blijft.
    g.beurten[2].tekst = "Iets heel anders.".into();
    assert!(!bewaard.bij_vorige_versie(&g));
    bewaard.hercontroleer(&Invoer::van(&g, &Markeringen::default()));
    assert!(
        bewaard.telling().1 > eerder.1,
        "{:?} {:?}",
        eerder,
        bewaard.telling()
    );
}

#[test]
fn punten_krijgen_vaste_nummers_die_bij_weghalen_blijven() {
    let g = gesprek();
    let notulen: Notulen = serde_json::from_str(&notulen_json()).unwrap();
    let invoer = Invoer::van(&g, &Markeringen::default());
    let mut bewaard = controleer_notulen(notulen, &invoer, &g, herkomst());
    let tweede = bewaard.besluiten[0].clone();
    bewaard.besluiten.push(tweede);
    bewaard.besluiten[1].id = 0;
    assert!(bewaard.nummer_punten());
    let ids: Vec<u32> = bewaard.besluiten.iter().map(|b| b.id).collect();
    assert_eq!(ids, [1, 2]);
    bewaard.besluiten.remove(0);
    assert!(!bewaard.nummer_punten(), "al genummerd");
    assert_eq!(bewaard.besluiten[0].id, 2);
    bewaard.besluiten[0].id = 0;
    assert!(bewaard.nummer_punten());
    assert_eq!(bewaard.besluiten[0].id, 1);
}
