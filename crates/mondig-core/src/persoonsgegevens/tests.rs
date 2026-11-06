//! Tests voor de herkenning van persoonsgegevens. Alle gegevens zijn synthetisch: BSN's en IBAN's
//! worden hier gemaakt met de controle zelf, geen enkel nummer is van een echt mens.

use super::*;

/// Een eenvoudige, vaste reeks pseudo-willekeurige getallen (LCG), zodat de tests herhaalbaar zijn.
struct Reeks(u64);

impl Reeks {
    fn volgende(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn tot(&mut self, grens: u64) -> u64 {
        self.volgende() % grens
    }
}

/// Een geldig BSN van negen cijfers: willekeurige acht cijfers en dan het laatste cijfer dat de
/// elfproef laat kloppen (kan dat niet, dan een nieuwe poging).
fn geldig_bsn(r: &mut Reeks) -> String {
    loop {
        let mut cijfers: Vec<u32> = (0..8)
            .map(|i| u32::try_from(if i == 0 { 1 + r.tot(9) } else { r.tot(10) }).unwrap())
            .collect();
        let som: u32 = cijfers
            .iter()
            .enumerate()
            .map(|(i, c)| (9 - u32::try_from(i).unwrap()) * c)
            .sum();
        // Het laatste cijfer heeft gewicht -1: som - laatste moet deelbaar zijn door 11.
        let laatste = som % 11;
        if laatste <= 9 {
            cijfers.push(laatste);
            let tekst: String = cijfers
                .iter()
                .map(|c| char::from_digit(*c, 10).unwrap())
                .collect();
            if elfproef(&tekst) {
                return tekst;
            }
        }
    }
}

/// Een negencijferig getal dat de elfproef niet haalt.
fn ongeldig_bsn(r: &mut Reeks) -> String {
    loop {
        let tekst = format!("{:09}", 100_000_000 + r.tot(899_999_999));
        if !elfproef(&tekst) {
            return tekst;
        }
    }
}

/// Een geldig Nederlands IBAN: bank en tien cijfers, met de controlecijfers uitgerekend.
fn geldig_iban(r: &mut Reeks) -> String {
    let banken = ["ABNA", "INGB", "RABO", "TRIO", "KNAB"];
    let bank = banken[usize::try_from(r.tot(5)).unwrap()];
    let nummer = format!("{:010}", r.tot(9_999_999_999));
    let zonder = format!("{bank}{nummer}NL00");
    let mut rest = 0u32;
    for c in zonder.chars() {
        let w = c.to_digit(36).unwrap();
        rest = if w >= 10 {
            (rest * 100 + w) % 97
        } else {
            (rest * 10 + w) % 97
        };
    }
    format!("NL{:02}{bank}{nummer}", 98 - rest)
}

/// Groepeert een rij cijfers met `scheiding` in groepen van de opgegeven lengtes.
fn groepeer(cijfers: &str, lengtes: &[usize], scheiding: &str) -> String {
    let mut delen = Vec::new();
    let mut van = 0;
    for l in lengtes {
        delen.push(&cijfers[van..van + l]);
        van += l;
    }
    delen.join(scheiding)
}

fn soorten(tekst: &str) -> Vec<Soort> {
    vind(tekst).into_iter().map(|v| v.soort).collect()
}

fn stukken(tekst: &str) -> Vec<(Soort, &str)> {
    vind(tekst)
        .into_iter()
        .map(|v| (v.soort, &tekst[v.van..v.tot]))
        .collect()
}

#[test]
fn de_elfproef_en_mod97_kennen_bekende_voorbeelden() {
    // 111222333 is het bekende voorbeeld-BSN uit de documentatie van de Belastingdienst.
    assert!(elfproef("111222333"));
    assert!(!elfproef("111222334"));
    assert!(!elfproef("000000000"));
    assert!(!elfproef("111111111"));
    assert!(!elfproef("12345678"));
    // Het voorbeeld-IBAN uit de IBAN-standaard (ISO 13616) en uit de documentatie van banken.
    assert!(iban_klopt("NL91ABNA0417164300"));
    assert!(!iban_klopt("NL91ABNA0417164301"));
    assert!(iban_klopt("GB82WEST12345698765432"));
}

#[test]
fn een_gemaakte_testset_van_bsns_wordt_volledig_gevonden_in_elke_schrijfwijze() {
    let mut r = Reeks(7);
    for _ in 0..300 {
        let bsn = geldig_bsn(&mut r);
        // Aaneengesloten: ook zonder BSN-woord.
        for zin in [
            format!("Mijn BSN is {bsn}."),
            bsn.clone(),
            format!("Het nummer {bsn}, dank u."),
        ] {
            assert_eq!(
                stukken(&zin),
                vec![(Soort::Bsn, bsn.as_str())],
                "in {zin:?}"
            );
        }
        // In groepen of met punten: met een BSN-woord ervoor.
        let in_groepen = [
            groepeer(&bsn, &[3, 3, 3], " "),
            groepeer(&bsn, &[4, 2, 3], " "),
            groepeer(&bsn, &[2, 2, 2, 3], " "),
            groepeer(&bsn, &[1, 1, 1, 1, 1, 1, 1, 1, 1], " "),
            groepeer(&bsn, &[3, 3, 3], "-"),
            groepeer(&bsn, &[3, 3, 3], "."),
            groepeer(&bsn, &[3, 2, 4], " "),
        ];
        for schrijfwijze in in_groepen {
            for zin in [
                format!("Mijn BSN is {schrijfwijze}."),
                format!("Het burgerservicenummer is {schrijfwijze}, dank u."),
                format!("Sofinummer {schrijfwijze}"),
            ] {
                assert_eq!(
                    stukken(&zin),
                    vec![(Soort::Bsn, schrijfwijze.as_str())],
                    "in {zin:?}"
                );
            }
            // Zonder BSN-woord telt een nummer in groepen niet: het kan een zaaknummer zijn.
            assert!(soorten(&format!("Het nummer {schrijfwijze}, dank u.")).is_empty());
        }
    }
}

#[test]
fn een_gemaakte_testset_van_ibans_wordt_volledig_gevonden_ook_in_groepen_van_vier() {
    let mut r = Reeks(11);
    for _ in 0..300 {
        let iban = geldig_iban(&mut r);
        let groepen: Vec<String> = iban
            .as_bytes()
            .chunks(4)
            .map(|c| String::from_utf8(c.to_vec()).unwrap())
            .collect();
        for schrijfwijze in [iban.clone(), groepen.join(" "), iban.to_lowercase()] {
            let zin = format!("Maak het over naar {schrijfwijze} onder vermelding van de factuur.");
            assert_eq!(
                stukken(&zin),
                vec![(Soort::Iban, schrijfwijze.as_str())],
                "in {zin:?}"
            );
        }
    }
}

#[test]
fn een_iban_met_een_verkeerd_controlegetal_wordt_niet_gevonden() {
    let mut r = Reeks(13);
    for _ in 0..100 {
        let iban = geldig_iban(&mut r);
        // Verander het laatste cijfer: de mod-97 klopt dan niet meer.
        let laatste = iban.chars().last().unwrap().to_digit(10).unwrap();
        let fout = format!("{}{}", &iban[..iban.len() - 1], (laatste + 1) % 10);
        assert!(
            soorten(&format!("Rekening {fout} graag.")).is_empty(),
            "{fout}"
        );
    }
}

#[test]
fn telefoonnummers_in_alle_nederlandse_schrijfwijzen() {
    let goed = [
        "06 12345678",
        "0612345678",
        "06-12345678",
        "06 1234 5678",
        "06 12 34 56 78",
        "+31 6 12345678",
        "+31612345678",
        "+31 6 1234 5678",
        "0031 6 12345678",
        "+31 (0)6 12345678",
        "020 1234567",
        "020-1234567",
        "(020) 123 45 67",
        "0201234567",
        "+31 20 1234567",
        "+31 (0)20 123 4567",
        "010 123 4567",
        "0343-123456",
    ];
    for nummer in goed {
        let zin = format!("Bel mij op {nummer} als het kan.");
        let mut gevonden = stukken(&zin);
        assert_eq!(gevonden.len(), 1, "{nummer}: {gevonden:?}");
        let (soort, stuk) = gevonden.remove(0);
        assert_eq!(soort, Soort::Telefoon, "{nummer}");
        assert_eq!(stuk, nummer);
    }
}

#[test]
fn telefoonachtige_reeksen_die_geen_nummer_zijn_blijven_met_rust() {
    for zin in [
        "Bel 06 1234 567 voor meer.",
        "Het zijn 0612 vragen.",
        "Het getal 0.5 is klein.",
        "Totaal 00000000000 niet.",
        "Mijn nummer is 12 0612345678 hier",
    ] {
        assert!(!soorten(zin).contains(&Soort::Telefoon), "{zin}");
    }
}

#[test]
fn e_mailadressen() {
    assert_eq!(
        stukken("Mail naar jan.de-vries+werk@example.com of naar info@example.com."),
        vec![
            (Soort::Email, "jan.de-vries+werk@example.com"),
            (Soort::Email, "info@example.com")
        ]
    );
    // Een punt aan het einde van de zin hoort er niet bij.
    assert_eq!(
        stukken("Mijn adres is piet@sub.example.org."),
        vec![(Soort::Email, "piet@sub.example.org")]
    );
    for geen in [
        "Dat kost 5 @ 3 euro.",
        "@gebruiker en @ander",
        "a@b",
        "x@y.1",
    ] {
        assert!(soorten(geen).is_empty(), "{geen}");
    }
}

#[test]
fn postcode_met_huisnummer_en_straat_met_huisnummer() {
    assert_eq!(
        stukken("Ik woon op 1234 AB 12 in de stad, en jij op 5678 CD 3a."),
        vec![(Soort::Adres, "1234 AB 12"), (Soort::Adres, "5678 CD 3a")]
    );
    assert_eq!(
        stukken("Het kantoor zit aan de Kerkstraat 12 en 3011AA 4-2."),
        vec![
            (Soort::Adres, "Kerkstraat 12"),
            (Soort::Adres, "3011AA 4-2")
        ]
    );
    // Kleine letters met huisnummer, en alleen een postcode na het woord "postcode".
    assert_eq!(
        stukken("Het kantoor 1234 ab 12 hier"),
        vec![(Soort::Adres, "1234 ab 12")]
    );
    assert_eq!(
        stukken("De postcode is 1234 AB."),
        vec![(Soort::Adres, "1234 AB")]
    );
    assert_eq!(
        stukken("Postcode 5678 cd graag"),
        vec![(Soort::Adres, "5678 cd")]
    );
    // Zonder huisnummer, met de verboden lettercombinaties of zonder hoofdletter bij de straat:
    // geen adres.
    for geen in [
        "In 2019 AD begon het.",
        "Vergadering 3 is verzet.",
        "De Woningmarkt 2024 was lastig.",
        "Leerpad 2 start morgen.",
        "Het Kerkstraat 2024 klopt niet.",
        "Op 1234 ab 2024 niet.",
        "Het is 1234 ab voorbij.",
        "Op 1234 SA 12 niet.",
        "We zaten bij kerkstraat 12.",
        "Het weg 12 is kort.",
        "In 20245 AB 12 niet.",
        "In 1999 en 2024 gebeurde het.",
    ] {
        assert!(!soorten(geen).contains(&Soort::Adres), "{geen}");
    }
}

#[test]
fn datums_met_dag_maand_en_jaar() {
    assert_eq!(
        stukken("Geboren op 12-03-1985, nee 5/6/85, of 1.2.2020, of 12 maart 1985, of 1985-03-12."),
        vec![
            (Soort::Datum, "12-03-1985"),
            (Soort::Datum, "5/6/85"),
            (Soort::Datum, "1.2.2020"),
            (Soort::Datum, "12 maart 1985"),
            (Soort::Datum, "1985-03-12"),
        ]
    );
    for geen in [
        "In 2024 was het druk.",
        "Om 14.30 uur en 9.15.",
        "De vergadering van 12 maart is verzet.",
        "Versie 1.2.3 is uit.",
        "Op 32-01-2024 niet, en 12-13-2024 ook niet.",
        "Het getal 3.14.159 is pi.",
        "Tot 12-03-1985-1 niet.",
    ] {
        assert!(!soorten(geen).contains(&Soort::Datum), "{geen}");
    }
}

#[test]
fn kentekens_in_de_sidecodes() {
    let goed = [
        "XX-99-99", "99-99-XX", "99-XX-99", "XX-99-XX", "XX-XX-99", "99-XX-XX", "99-XXX-9",
        "9-XXX-99", "XX-999-X", "X-999-XX", "XXX-99-X", "X-99-XXX", "9-XX-999",
    ];
    for patroon in goed {
        // Vul met medeklinkers en cijfers: X wordt een medeklinker, 9 een cijfer.
        let mut letters = "BDFGHJKLNPRSTVXZ".chars().cycle();
        let kenteken: String = patroon
            .chars()
            .map(|c| match c {
                'X' => letters.next().unwrap(),
                '9' => '7',
                andere => andere,
            })
            .collect();
        let zin = format!("De auto met kenteken {kenteken} stond daar.");
        assert_eq!(
            stukken(&zin),
            vec![(Soort::Kenteken, kenteken.as_str())],
            "{zin}"
        );
    }
    for geen in [
        "Het ISO-12-34 document.",
        "Order AB-12-CD-34 klaar.",
        "TV-12-AB-",
        "EU-12-ES",
    ] {
        assert!(!soorten(geen).contains(&Soort::Kenteken), "{geen}");
    }
}

#[test]
fn gewone_getallen_in_een_gesprek_geven_geen_valse_treffers() {
    let zinnen = [
        // Bedragen
        "Het kost € 123.456.789 in totaal.",
        "Het kost 123456782 euro.",
        "Dat is 123 456 782 procent meer.",
        "Een bedrag van 1.234.567,89 euro.",
        "Dat komt neer op 123456782,50.",
        // Jaartallen en tijden
        "In 1999 en 2024 en 2031 gebeurde het.",
        "Om 14:30 en 09.45 en 23:59:59.",
        "Tussen 2019-2024 groeide het.",
        // Aantallen
        "Er waren 120 mensen, 3.500 bezoekers en 1 miljoen kijkers.",
        "We hebben 45 000 000 exemplaren verkocht.",
        "Pagina 12 van 340.",
        // Telefoonnummer-achtige reeksen die de elfproef niet halen
        "Mijn telefoon is 123456789 volgens het formulier.",
        "Het dossiernummer is 20240312 en referentie 9876543.",
        // Versienummers en decimalen
        "Versie 10.2.1 en 3.14159.",
        "De snelheid is 12,5 km per uur.",
    ];
    for zin in zinnen {
        assert!(
            soorten(zin).is_empty(),
            "valse treffer in {zin:?}: {:?}",
            stukken(zin)
        );
    }
}

#[test]
fn negencijferige_getallen_die_de_elfproef_niet_halen_worden_nooit_gemarkeerd() {
    let mut r = Reeks(17);
    for _ in 0..500 {
        let getal = ongeldig_bsn(&mut r);
        for zin in [
            format!("Het nummer is {getal}."),
            groepeer(&getal, &[3, 3, 3], " "),
            format!("Reeks {}.", groepeer(&getal, &[4, 2, 3], " ")),
        ] {
            assert!(soorten(&zin).is_empty(), "valse treffer in {zin:?}");
        }
    }
}

#[test]
fn een_geldig_bsn_in_een_bedrag_of_met_punten_wordt_niet_gemarkeerd() {
    let mut r = Reeks(19);
    let bsn = geldig_bsn(&mut r);
    let met_punten = groepeer(&bsn, &[3, 3, 3], ".");
    assert!(soorten(&format!("Het kost € {bsn} totaal")).is_empty());
    assert!(soorten(&format!("Het kost {bsn} euro")).is_empty());
    assert!(soorten(&format!("Het is {met_punten} stuks")).is_empty());
    assert!(soorten(&format!("Het is {bsn},5 stuks")).is_empty());
    // Maar in een gewone zin wel, ook vlak voor een leesteken.
    assert_eq!(soorten(&format!("Mijn BSN: {bsn}!")), vec![Soort::Bsn]);
}

#[test]
fn bij_overlap_wint_de_soort_met_voorrang() {
    let mut r = Reeks(23);
    let iban = geldig_iban(&mut r);
    // De cijfers van een IBAN zijn geen BSN of telefoonnummer.
    assert_eq!(
        stukken(&format!("Rek {iban}")),
        vec![(Soort::Iban, iban.as_str())]
    );
    // Een mobiel nummer is geen BSN, ook al heeft het negen cijfers na de nul.
    assert_eq!(soorten("Bel 0612345678"), vec![Soort::Telefoon]);
    // E-mail met een nummer erin is één e-mailadres.
    assert_eq!(
        stukken("Stuur naar 111222333@example.com graag"),
        vec![(Soort::Email, "111222333@example.com")]
    );
}

#[test]
fn alle_soorten_in_een_zin_op_volgorde() {
    let mut r = Reeks(29);
    let bsn = geldig_bsn(&mut r);
    let iban = geldig_iban(&mut r);
    let zin = format!(
        "Jan (geboren 3 mei 1970) woont Kerkstraat 9, belt 06-12345678, mailt jan@example.com, \
         heeft BSN {bsn}, IBAN {iban} en rijdt in 12-AB-34."
    );
    // 12-AB-34 bevat een klinker (A): geen kenteken; met consonanten wel.
    let zin = zin.replace("12-AB-34", "12-BD-34");
    assert_eq!(
        soorten(&zin),
        vec![
            Soort::Datum,
            Soort::Adres,
            Soort::Telefoon,
            Soort::Email,
            Soort::Bsn,
            Soort::Iban,
            Soort::Kenteken
        ]
    );
}

#[test]
fn bereiken_staan_op_tekengrenzen_ook_bij_accenten_en_emoji() {
    let tekst = "Één café 😀 bel 06 12345678 straks";
    let v = vind(tekst);
    assert_eq!(v.len(), 1);
    assert_eq!(&tekst[v[0].van..v[0].tot], "06 12345678");
    // In UTF-16 telt de emoji voor twee.
    let van = utf16_plek(tekst, v[0].van);
    assert_eq!(byte_plek(tekst, van), Some(v[0].van));
    assert_eq!(
        van,
        tekst
            .chars()
            .take_while(|c| *c != '0')
            .map(char::len_utf16)
            .sum::<usize>()
    );
    assert_eq!(byte_plek("a😀b", 2), None, "midden in een teken");
    assert_eq!(byte_plek("ab", 2), Some(2));
    assert_eq!(byte_plek("ab", 3), None);
}

#[test]
fn vervangen_zet_de_plaatsvervanger_in_en_bewaart_geen_sleutel() {
    let mut r = Reeks(31);
    let bsn = geldig_bsn(&mut r);
    let tekst = format!("BSN {bsn}, bel 06 12345678 of mail a@b.nl op 12-03-1985.");
    let mut m = Markeringen::default();
    // Uit: ongewijzigd.
    assert_eq!(m.voor_uitvoer(&tekst), tekst);
    m.zet_vervangen(true);
    assert_eq!(
        m.voor_uitvoer(&tekst),
        "BSN [BSN], bel [TELEFOON] of mail [E-MAIL] op [DATUM]."
    );
    // Twee keer hetzelfde nummer krijgt twee keer hetzelfde label: geen nummering, geen sleutel.
    assert_eq!(m.vervang_altijd("a@b.nl en a@b.nl"), "[E-MAIL] en [E-MAIL]");
}

#[test]
fn een_markering_weghalen_geldt_voor_dat_stuk_overal_en_zelf_markeren_ook() {
    let tekst = "Mail jan@example.com of piet@example.com. Jansen belde, en jansen mailde.";
    let mut m = Markeringen::default();
    assert_eq!(m.vondsten(tekst).len(), 2);
    assert!(m.haal_weg("jan@example.com"));
    let v = m.vondsten(tekst);
    assert_eq!(v.len(), 1);
    assert_eq!(&tekst[v[0].van..v[0].tot], "piet@example.com");
    // Zelf markeren: overal, zonder op hoofdletters te letten, alleen als los woord.
    m.markeer("Jansen").unwrap();
    m.zet_vervangen(true);
    assert_eq!(
        m.voor_uitvoer(tekst),
        "Mail jan@example.com of [E-MAIL]. [GEGEVEN] belde, en [GEGEVEN] mailde."
    );
    assert_eq!(m.eigen(), vec!["jansen".to_string()]);
    assert!(m.markeer("Jansenberg").is_ok());
    assert_eq!(m.vondsten("Jansenberg en Jansens").len(), 1);
    // Weghalen van een eigen markering verwijdert hem; opnieuw markeren haalt een weggehaalde terug.
    assert!(m.haal_weg("JANSEN"));
    assert!(!m.eigen().contains(&"jansen".to_string()));
    m.markeer("jan@example.com").unwrap();
    assert!(
        m.vondsten(tekst)
            .iter()
            .any(|v| &tekst[v.van..v.tot] == "jan@example.com")
    );
}

#[test]
fn zelf_markeren_weigert_te_korte_lange_en_meerregelige_stukken() {
    let mut m = Markeringen::default();
    assert!(m.markeer("a").is_err());
    assert!(m.markeer("  ").is_err());
    assert!(m.markeer("twee\nregels").is_err());
    assert!(m.markeer(&"x".repeat(101)).is_err());
    assert!(!m.haal_weg("  "));
    assert!(m.eigen().is_empty());
}

#[test]
fn een_eigen_markering_gaat_voor_niet_over_een_gevonden_gegeven_heen() {
    let tekst = "Bel 06 12345678 nu.";
    let mut m = Markeringen::default();
    m.markeer("12345678").unwrap();
    let v = m.vondsten(tekst);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].soort, Soort::Telefoon);
}

#[test]
fn een_datum_met_een_tijd_erachter_en_telefoon_gaat_niet_door_elkaar() {
    assert_eq!(
        stukken("Op 12-11-2026 14.00 uur zijn we er."),
        vec![(Soort::Datum, "12-11-2026")]
    );
    assert_eq!(
        stukken("Vanaf 03-11-2026 14 uur"),
        vec![(Soort::Datum, "03-11-2026")]
    );
    // Een telefoonnummer vlak voor een punt aan het einde van de zin blijft een nummer.
    assert_eq!(
        stukken("Bel 0612345678."),
        vec![(Soort::Telefoon, "0612345678")]
    );
    assert_eq!(
        stukken("Bel 06 12345678-"),
        vec![(Soort::Telefoon, "06 12345678")]
    );
}

#[test]
fn zaaknummers_met_een_streepje_zijn_geen_bsn_en_punten_alleen_met_context() {
    let mut r = Reeks(37);
    // 2024-00025 haalt de elfproef toevallig niet, maar zonder BSN-woord telt het toch niet.
    for zin in [
        "Zaaknummer 2024-00025 is open.",
        "Zaaknummer 2024 000 025 is open.",
        "Kenmerk 123.456.782 staat erbij.",
    ] {
        assert!(soorten(zin).is_empty(), "{zin}");
    }
    let bsn = geldig_bsn(&mut r);
    let met_punten = groepeer(&bsn, &[3, 3, 3], ".");
    assert_eq!(
        stukken(&format!("BSN {met_punten}")),
        vec![(Soort::Bsn, met_punten.as_str())]
    );
}

#[test]
fn iban_met_een_spatie_na_het_land_en_kentekens_in_andere_schrijfwijzen() {
    let mut r = Reeks(41);
    let iban = geldig_iban(&mut r);
    let met_spatie = format!("{} {}", &iban[..2], &iban[2..]);
    assert_eq!(
        stukken(&format!("Rek {met_spatie} ok")),
        vec![(Soort::Iban, met_spatie.as_str())]
    );
    for (zin, kenteken) in [
        ("De auto 12-bd-34 stond er", "12-bd-34"),
        ("De auto 12 BD 34 stond er", "12 BD 34"),
        ("De auto 12BD34 stond er", "12BD34"),
        ("De auto BD-12-FG stond er", "BD-12-FG"),
    ] {
        assert_eq!(stukken(zin), vec![(Soort::Kenteken, kenteken)], "{zin}");
    }
    // Gewone tekst met cijfers en medeklinkers blijft met rust.
    for geen in [
        "Het zijn 12 kg 45 gram.",
        "Pagina 12 bd 34 niet.",
        "Ze kocht 12bd34x.",
    ] {
        assert!(!soorten(geen).contains(&Soort::Kenteken), "{geen}");
    }
}

#[test]
fn datums_met_een_afkorting_of_spaties_alleen_met_context() {
    assert_eq!(
        stukken("Hij is geboren op 3 nov. 2026 in Delft."),
        vec![(Soort::Datum, "3 nov. 2026")]
    );
    assert_eq!(
        stukken("Geboortedatum 3 11 1980."),
        vec![(Soort::Datum, "3 11 1980")]
    );
    for geen in [
        "We spraken 3 nov. 2026 af.",
        "Het zijn 3 11 1980 stuks.",
        "Hoofdstuk 3 11 1980 niet.",
    ] {
        assert!(!soorten(geen).contains(&Soort::Datum), "{geen}");
    }
}

#[test]
fn eigen_stukken_die_overlappen_worden_een_markering() {
    let mut m = Markeringen::default();
    m.markeer("Jan").unwrap();
    m.markeer("Jan de Vries").unwrap();
    m.zet_vervangen(true);
    assert_eq!(
        m.voor_uitvoer("Jan de Vries belde, en Jan ook."),
        "[GEGEVEN] belde, en [GEGEVEN] ook."
    );
    // Een eigen stuk dat een gevonden adres omvat, vervangt het geheel.
    let mut m = Markeringen::default();
    m.markeer("Kerkstraat 12, 1234 AB Utrecht").unwrap();
    m.zet_vervangen(true);
    assert_eq!(
        m.voor_uitvoer("Ik woon op Kerkstraat 12, 1234 AB Utrecht sinds mei."),
        "Ik woon op [GEGEVEN] sinds mei."
    );
    // Eén stuk dat een eigen stuk deelt met een gevonden e-mail wordt ook één markering.
    let mut m = Markeringen::default();
    m.markeer("jan@example.com").unwrap();
    assert_eq!(m.vondsten("mail jan@example.com nu").len(), 1);
}

#[test]
fn weghalen_haalt_een_stuk_weg_dat_gevonden_en_zelf_gemarkeerd_is() {
    let mut m = Markeringen::default();
    m.markeer("jan@example.com").unwrap();
    assert!(m.haal_weg("jan@example.com"));
    assert!(m.vondsten("mail jan@example.com nu").is_empty());
    assert!(m.eigen().is_empty());
    // Opnieuw markeren zet hem terug.
    m.markeer("jan@example.com").unwrap();
    assert_eq!(m.vondsten("mail jan@example.com nu").len(), 1);
}

#[test]
fn een_nummer_over_een_segmentgrens_wordt_gevonden_en_vervangen_in_het_eerste_segment() {
    let teksten = ["Mijn nummer is 06 1234", "5678 en dat was het."];
    let mut m = Markeringen::default();
    let v = m.vondsten_over(&teksten);
    assert_eq!(v.len(), 2);
    assert_eq!(&teksten[0][v[0][0].van..v[0][0].tot], "06 1234");
    assert_eq!(&teksten[1][v[1][0].van..v[1][0].tot], "5678");
    assert_eq!(v[0][0].soort, Soort::Telefoon);
    // Uit: ongewijzigd. Aan: de plaatsvervanger in het eerste segment, het restant weg.
    assert_eq!(
        m.uitvoer_over(&teksten),
        vec![teksten[0].to_string(), teksten[1].to_string()]
    );
    m.zet_vervangen(true);
    assert_eq!(
        m.uitvoer_over(&teksten),
        vec![
            "Mijn nummer is [TELEFOON]".to_string(),
            " en dat was het.".to_string()
        ]
    );
}

#[test]
fn eigen_markeringen_die_niet_meer_in_de_tekst_staan_worden_gemeld() {
    let mut m = Markeringen::default();
    m.markeer("Jan de Vries").unwrap();
    m.markeer("Kerkstraat").unwrap();
    m.markeer("Piet").unwrap();
    // "Jan de Vries" heet nu "Jan de Fries"; "Pieter" telt niet als "Piet" (geen los stuk); de
    // Kerkstraat staat in een naam van een spreker.
    let teksten = [
        "Dag, ik ben JAN DE FRIES.",
        "Pieter komt ook.",
        "Kerkstraat",
    ];
    assert_eq!(
        m.verdwenen(&teksten),
        vec!["jan de vries".to_string(), "piet".to_string()]
    );
    // Wat nog voorkomt, ook met andere hoofdletters, is niet verdwenen.
    assert!(
        m.verdwenen(&["jan de vries en piet, kerkstraat 1"])
            .is_empty()
    );
    // De stukken blijven gemarkeerd, zodat de gebruiker ze in de lijst terugziet.
    assert_eq!(m.eigen().len(), 3);
    assert!(Markeringen::default().verdwenen(&["iets"]).is_empty());
}
