//! PDF voor archief: PDF/A-2b met krilla (ADR 0017).
//!
//! krilla controleert bij het maken zelf de eisen van PDF/A-2b die het kan nagaan (ingebedde
//! lettertypen, een sRGB-uitvoerprofiel, XMP-metadata, geen .notdef-tekens) en geeft een fout
//! in plaats van een PDF die er niet aan voldoet. De semantische eisen (geen transparantie of
//! externe inhoud) volgen hier uit de opbouw: alleen tekst in effen kleuren.
//!
//! Opmaak: A4, marges van 2,5 cm, de titel in Sora (gewicht 600), de rest in Geist (400),
//! zoals de huisstijl. De lettertypen zitten in de app (OFL-1.1, `lettertypen/`); krilla zet er
//! alleen de gebruikte tekens van in de PDF. Een teken dat in Geist ontbreekt, wordt een
//! vraagteken: PDF/A staat het lege .notdef-teken niet toe.
//!
//! Bij een gesprek (regels met een spreker) staat het label vet (Geist, gewicht 700) voor de
//! tekst, op dezelfde regel, na de tijd. Zonder spreker is de uitvoer ongewijzigd.
//!
//! krilla breekt geen regels af. Dat doet [`Zetter`]: elk woord één keer meten met skrifa
//! (dezelfde lettertypelezer die krilla gebruikt) en per regel zoveel woorden als passen. skrifa meet
//! zonder overhang tussen letterparen (kerning); krilla tekent met kerning, wat een regel
//! hooguit iets korter maakt. Voor de zekerheid blijft er [`SPELING`] over.

use krilla::color::rgb;
use krilla::configure::{Archival, ConfigurationBuilder};
use krilla::geom::Point;
use krilla::metadata::{DateTime, Metadata};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule};
use krilla::text::{Font, Tag, TextDirection};
use krilla::{Data, SerializeSettings};
use skrifa::charmap::Charmap;
use skrifa::instance::{Location, Size};
use skrifa::metrics::GlyphMetrics;
use skrifa::{FontRef, MetadataProvider};

use super::{
    BewaarFout, Document, Notulenblok, Notulenregel, OPSOMTEKEN, Taalcode, VERSIE, Zoned,
    schone_tekst, tijdcode,
};

static SORA: &[u8] = include_bytes!("../../lettertypen/Sora.ttf");
static GEIST: &[u8] = include_bytes!("../../lettertypen/Geist.ttf");

/// A4 in punten, zoals de standaard van krilla.
const BREEDTE: f32 = 595.0;
const HOOGTE: f32 = 842.0;
/// 2,5 cm.
const MARGE: f32 = 70.87;
/// De kolom voor de tijd: 1,5 cm.
const TIJDKOLOM: f32 = 42.52;
/// Zoveel van de regelbreedte blijft vrij, omdat het meten zonder kerning gaat.
const SPELING: f32 = 0.98;

/// Tekstkleur en grijs van de huisstijl (licht thema).
const TEKST: (u8, u8, u8) = (0x1a, 0x18, 0x15);
const GRIJS: (u8, u8, u8) = (0x5a, 0x54, 0x4c);

/// Een lettertype voor de PDF, met wat nodig is om tekst in hetzelfde lettertype te meten.
struct Letter {
    pdf: Font,
    maat: FontRef<'static>,
    locatie: Location,
    em: f32,
}

impl Letter {
    fn nieuw(data: &'static [u8], gewicht: f32) -> Result<Self, BewaarFout> {
        let fout = || BewaarFout::Pdf("lettertype niet te lezen".to_string());
        let pdf = Font::new_variable(Data::from(data), 0, &[(Tag::new(b"wght"), gewicht)])
            .ok_or_else(fout)?;
        let maat = FontRef::from_index(data, 0).map_err(|_| fout())?;
        let locatie = maat.axes().location([("wght", gewicht)]);
        let em = f32::from(maat.metrics(Size::unscaled(), &locatie).units_per_em.max(1));
        Ok(Self {
            pdf,
            maat,
            locatie,
            em,
        })
    }

    /// Een meter met de tekentabel en de breedtes, één keer opgezocht.
    fn meter(&self) -> Meter<'_> {
        Meter {
            tekens: self.maat.charmap(),
            maten: self.maat.glyph_metrics(Size::unscaled(), &self.locatie),
            em: self.em,
        }
    }
}

/// Meet tekst in één lettertype (zonder kerning).
struct Meter<'a> {
    tekens: Charmap<'a>,
    maten: GlyphMetrics<'a>,
    em: f32,
}

impl Meter<'_> {
    /// De breedte van één teken in punten.
    fn teken(&self, teken: char, grootte: f32) -> f32 {
        let eenheden = self
            .tekens
            .map(teken)
            .and_then(|glyph| self.maten.advance_width(glyph))
            .unwrap_or(0.0);
        eenheden * grootte / self.em
    }

    /// De breedte van een stuk tekst in punten.
    fn breedte(&self, tekst: &str, grootte: f32) -> f32 {
        tekst.chars().map(|c| self.teken(c, grootte)).sum()
    }

    /// De tekst met een vraagteken voor elk teken dat het lettertype niet heeft.
    fn dekbaar(&self, tekst: &str) -> String {
        tekst
            .chars()
            .map(|c| {
                if c == ' ' || self.tekens.map(c).is_some() {
                    c
                } else {
                    '?'
                }
            })
            .collect()
    }
}

/// Hoe een stuk tekst eruitziet.
#[derive(Clone, Copy)]
enum Stijl {
    Titel,
    Gegevens,
    Tijd,
    Tekst,
    /// Het label van een spreker: de tekststijl, vet.
    Spreker,
}

struct Maat {
    grootte: f32,
    regel: f32,
    kleur: (u8, u8, u8),
}

impl Stijl {
    const fn maat(self) -> Maat {
        match self {
            Self::Titel => Maat {
                grootte: 20.0,
                regel: 26.0,
                kleur: TEKST,
            },
            Self::Gegevens | Self::Tijd => Maat {
                grootte: 9.0,
                regel: 13.5,
                kleur: GRIJS,
            },
            Self::Tekst | Self::Spreker => Maat {
                grootte: 10.5,
                regel: 15.5,
                kleur: TEKST,
            },
        }
    }
}

/// Een stuk tekst op een plek: `y` is de basislijn, van boven gemeten.
struct Stuk {
    x: f32,
    y: f32,
    stijl: Stijl,
    tekst: String,
}

/// Zet de tekst op pagina's.
struct Zetter<'a> {
    titel: Meter<'a>,
    tekst: Meter<'a>,
    vet: Meter<'a>,
    paginas: Vec<Vec<Stuk>>,
    y: f32,
}

impl<'a> Zetter<'a> {
    fn new(titel: &'a Letter, tekst: &'a Letter, vet: &'a Letter) -> Self {
        Self {
            titel: titel.meter(),
            tekst: tekst.meter(),
            vet: vet.meter(),
            paginas: vec![Vec::new()],
            y: MARGE,
        }
    }

    fn meter(&self, stijl: Stijl) -> &Meter<'a> {
        match stijl {
            Stijl::Titel => &self.titel,
            Stijl::Spreker => &self.vet,
            _ => &self.tekst,
        }
    }

    /// Breekt tekst af in regels die binnen `breedte` passen. Elk woord wordt één keer gemeten,
    /// dus de tijd groeit gewoon mee met de lengte van de tekst. Een woord dat alleen al te
    /// breed is, wordt per teken afgebroken.
    fn regels(&self, stijl: Stijl, tekst: &str, breedte: f32) -> Vec<String> {
        let meter = self.meter(stijl);
        let grootte = stijl.maat().grootte;
        let max = breedte * SPELING;
        let spatie = meter.teken(' ', grootte);
        let mut regels = Vec::new();
        let mut huidig = String::new();
        let mut gebruikt = 0.0_f32;
        for woord in tekst.split(' ').filter(|w| !w.is_empty()) {
            let woordbreedte = meter.breedte(woord, grootte);
            if !huidig.is_empty() && gebruikt + spatie + woordbreedte <= max {
                huidig.push(' ');
                huidig.push_str(woord);
                gebruikt += spatie + woordbreedte;
                continue;
            }
            if !huidig.is_empty() {
                regels.push(std::mem::take(&mut huidig));
                gebruikt = 0.0;
            }
            if woordbreedte <= max {
                huidig.push_str(woord);
                gebruikt = woordbreedte;
                continue;
            }
            for teken in woord.chars() {
                let tekenbreedte = meter.teken(teken, grootte);
                if !huidig.is_empty() && gebruikt + tekenbreedte > max {
                    regels.push(std::mem::take(&mut huidig));
                    gebruikt = 0.0;
                }
                huidig.push(teken);
                gebruikt += tekenbreedte;
            }
        }
        if !huidig.is_empty() {
            regels.push(huidig);
        }
        regels
    }

    /// Zorgt voor ruimte voor een regel van `hoogte`; anders een nieuwe pagina.
    fn ruimte(&mut self, hoogte: f32) {
        if self.y + hoogte > HOOGTE - MARGE {
            self.paginas.push(Vec::new());
            self.y = MARGE;
        }
    }

    /// De basislijn van een regel in deze stijl: ongeveer vier vijfde van de lettergrootte
    /// onder de bovenkant, midden in de regelhoogte.
    fn basislijn(&self, stijl: Stijl) -> f32 {
        let maat = stijl.maat();
        self.y + (maat.regel - maat.grootte) / 2.0 + maat.grootte * 0.8
    }

    fn plaats(&mut self, x: f32, y: f32, stijl: Stijl, tekst: String) {
        if let Some(pagina) = self.paginas.last_mut() {
            pagina.push(Stuk { x, y, stijl, tekst });
        }
    }

    /// Een alinea over de volle breedte.
    fn alinea(&mut self, stijl: Stijl, tekst: &str, na: f32) {
        let tekst = self.meter(stijl).dekbaar(tekst);
        let regel = stijl.maat().regel;
        for zin in self.regels(stijl, &tekst, BREEDTE - 2.0 * MARGE) {
            self.ruimte(regel);
            let y = self.basislijn(stijl);
            self.plaats(MARGE, y, stijl, zin);
            self.y += regel;
        }
        self.y += na;
    }

    /// Een alinea tekst, optioneel met de tijd vooraan (op de basislijn van de eerste regel,
    /// de tekst ingesprongen) en optioneel met het label van de spreker vet vóór de tekst.
    fn tekstalinea(&mut self, tijd: Option<&str>, spreker: Option<&str>, tekst: &str, na: f32) {
        let tekst = self.tekst.dekbaar(tekst);
        let regel = Stijl::Tekst.maat().regel;
        let links = if tijd.is_some() { TIJDKOLOM } else { 0.0 };
        let breedte = BREEDTE - 2.0 * MARGE - links;
        let (label, inspringing) = self.label(spreker, breedte);
        // Past het label naast de tekst, dan staat het vóór de eerste regel; anders krijgt
        // het een eigen regel.
        let (mut label_los, mut label_naast) = match label {
            Some(l) if inspringing > 0.0 => (None, Some(l)),
            label => (label, None),
        };
        let mut tijd = tijd;
        if let Some(label) = label_los.take() {
            self.ruimte(regel);
            let y = self.basislijn(Stijl::Tekst);
            if let Some(tijd) = tijd.take() {
                self.plaats(MARGE, y, Stijl::Tijd, tijd.to_string());
            }
            self.plaats(MARGE + links, y, Stijl::Spreker, label);
            self.y += regel;
        }
        for zin in self.regels_na_label(&tekst, breedte, inspringing) {
            self.ruimte(regel);
            let y = self.basislijn(Stijl::Tekst);
            if let Some(tijd) = tijd.take() {
                self.plaats(MARGE, y, Stijl::Tijd, tijd.to_string());
            }
            let mut x = MARGE + links;
            if let Some(label) = label_naast.take() {
                self.plaats(x, y, Stijl::Spreker, label);
                x += inspringing;
            }
            self.plaats(x, y, Stijl::Tekst, zin);
            self.y += regel;
        }
        self.y += na;
    }

    /// Het label ("Jij:") en hoeveel ruimte het op de eerste regel inneemt (met een spatie
    /// erachter). Is het label breder dan de helft van de regel, dan is de ruimte nul en
    /// hoort het op een eigen regel.
    fn label(&self, spreker: Option<&str>, breedte: f32) -> (Option<String>, f32) {
        let Some(spreker) = spreker else {
            return (None, 0.0);
        };
        let label = self.vet.dekbaar(&format!("{spreker}:"));
        let grootte = Stijl::Spreker.maat().grootte;
        let ruimte = self.vet.breedte(&label, grootte) + self.vet.teken(' ', grootte);
        let inspringing = if ruimte * 2.0 <= breedte * SPELING {
            ruimte
        } else {
            0.0
        };
        (Some(label), inspringing)
    }

    /// Breekt tekst af voor een alinea waarvan de eerste regel `inspringing` smaller is
    /// (door het label ervoor). Zonder inspringing is dit gewoon [`Zetter::regels`].
    fn regels_na_label(&self, tekst: &str, breedte: f32, inspringing: f32) -> Vec<String> {
        if inspringing <= 0.0 {
            return self.regels(Stijl::Tekst, tekst, breedte);
        }
        let mut eerste = self.regels(Stijl::Tekst, tekst, breedte - inspringing);
        if eerste.is_empty() {
            return eerste;
        }
        // De rest van de tekst na de eerste regel gaat over de volle breedte.
        let kop = eerste.remove(0);
        let rest = tekst.get(kop.len()..).unwrap_or_default().trim_start();
        let mut regels = vec![kop];
        regels.extend(self.regels(Stijl::Tekst, rest, breedte));
        regels
    }
}

fn vul(kleur: (u8, u8, u8)) -> Fill {
    Fill {
        paint: rgb::Color::new(kleur.0, kleur.1, kleur.2).into(),
        opacity: NormalizedF32::ONE,
        rule: FillRule::default(),
    }
}

fn pdf_datum(moment: &Zoned) -> DateTime {
    let offset = moment.offset().seconds();
    let uren = i8::try_from(offset / 3600).unwrap_or(0);
    let minuten = u8::try_from((offset.abs() % 3600) / 60).unwrap_or(0);
    DateTime::new(u16::try_from(moment.year()).unwrap_or(1970))
        .month(u8::try_from(moment.month()).unwrap_or(1))
        .day(u8::try_from(moment.day()).unwrap_or(1))
        .hour(u8::try_from(moment.hour()).unwrap_or(0))
        .minute(u8::try_from(moment.minute()).unwrap_or(0))
        .second(u8::try_from(moment.second()).unwrap_or(0))
        .utc_offset_hour(uren)
        .utc_offset_minute(minuten)
}

/// Een PDF/A-2b-document.
pub(super) fn pdf(
    document: &Document,
    notulen: Option<&Notulenblok>,
    tijden: bool,
) -> Result<Vec<u8>, BewaarFout> {
    let titel_letter = Letter::nieuw(SORA, 600.0)?;
    let tekst_letter = Letter::nieuw(GEIST, 400.0)?;
    let vet_letter = Letter::nieuw(GEIST, 700.0)?;

    let mut zetter = Zetter::new(&titel_letter, &tekst_letter, &vet_letter);
    zetter.alinea(Stijl::Titel, &document.titel(), 8.0);
    for (sleutel, waarde) in document.gegevens() {
        zetter.alinea(Stijl::Gegevens, &format!("{sleutel}: {waarde}"), 0.0);
    }
    zetter.y += 18.0;
    if let Some(notulen) = notulen {
        // Koppen vet in de tekstgrootte, met wat ruimte erboven; een punt met de tijd (of een
        // opsomteken) in de tijdkolom, zoals een regel van het verslag.
        for regel in &notulen.regels {
            match regel {
                Notulenregel::Kop(tekst) => {
                    zetter.alinea(Stijl::Spreker, &schone_tekst(tekst), 6.0);
                }
                Notulenregel::Tussenkop(tekst) => {
                    zetter.y += 6.0;
                    zetter.alinea(Stijl::Spreker, &schone_tekst(tekst), 4.0);
                }
                Notulenregel::Alinea(tekst) => {
                    zetter.alinea(Stijl::Tekst, &schone_tekst(tekst), 6.0);
                }
                Notulenregel::Punt { tijd_ms, tekst } => {
                    let voor = tijd_ms.map_or_else(|| OPSOMTEKEN.to_string(), tijdcode);
                    zetter.tekstalinea(Some(&voor), None, &schone_tekst(tekst), 4.0);
                }
            }
        }
        zetter.y += 12.0;
        zetter.alinea(Stijl::Spreker, &schone_tekst(&notulen.verslagkop), 6.0);
    }
    let regels = document.tekstregels();
    if regels.is_empty() {
        zetter.alinea(Stijl::Tekst, &document.zonder_tekst(), 0.0);
    }
    for regel in regels {
        let tijd = tijden.then(|| tijdcode(regel.start_ms));
        zetter.tekstalinea(tijd.as_deref(), regel.spreker.as_deref(), &regel.tekst, 6.0);
    }
    let paginas = zetter.paginas;

    let configuratie = ConfigurationBuilder::new()
        .with_archival_validator(Archival::A2_B)
        .finish()
        .map_err(|e| BewaarFout::Pdf(format!("{e:?}")))?;
    let instellingen = SerializeSettings {
        configuration: configuratie,
        ..SerializeSettings::default()
    };
    let mut pdf = krilla::Document::new_with(instellingen);
    let mut trefwoorden = vec!["Transcript".to_string()];
    trefwoorden.extend(document.label());
    pdf.set_metadata(
        Metadata::new()
            .title(document.titel())
            .description(document.beschrijving())
            .keywords(trefwoorden)
            .language(Taalcode::van(notulen).volledig.to_string())
            .creator(format!("Mondig {VERSIE}"))
            .creation_date(pdf_datum(&document.moment)),
    );

    let totaal = paginas.len();
    for (nummer, stukken) in paginas.into_iter().enumerate() {
        let mut pagina = pdf.start_page_with(PageSettings::default());
        let mut vlak = pagina.surface();
        for stuk in stukken {
            let maat = stuk.stijl.maat();
            let letter = match stuk.stijl {
                Stijl::Titel => &titel_letter,
                Stijl::Spreker => &vet_letter,
                _ => &tekst_letter,
            };
            vlak.set_fill(Some(vul(maat.kleur)));
            vlak.draw_text(
                Point::from_xy(stuk.x, stuk.y),
                letter.pdf.clone(),
                maat.grootte,
                &stuk.tekst,
                false,
                TextDirection::LeftToRight,
            );
        }
        // Paginanummer rechtsonder.
        let voet = format!("Pagina {} van {totaal}", nummer + 1);
        let grootte = 8.5;
        let x = BREEDTE - MARGE - tekst_letter.meter().breedte(&voet, grootte);
        vlak.set_fill(Some(vul(GRIJS)));
        vlak.draw_text(
            Point::from_xy(x, HOOGTE - MARGE / 2.0),
            tekst_letter.pdf.clone(),
            grootte,
            &voet,
            false,
            TextDirection::LeftToRight,
        );
        vlak.finish();
        pagina.finish();
    }
    pdf.finish().map_err(|e| BewaarFout::Pdf(format!("{e:?}")))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{document, gesprek_document};
    use super::super::{Document, Regel};
    use super::*;

    fn bevat(bytes: &[u8], zoek: &str) -> bool {
        bytes.windows(zoek.len()).any(|w| w == zoek.as_bytes())
    }

    #[test]
    fn pdf_a_2b_met_metadata() {
        let bytes = pdf(&document(), None, true).expect("pdf");
        assert!(bytes.starts_with(b"%PDF-1.7"));
        // De PDF/A-verklaring en de gegevens in de XMP-metadata.
        assert!(bevat(&bytes, "<pdfaid:part>2</pdfaid:part>"));
        assert!(bevat(&bytes, "<pdfaid:conformance>B</pdfaid:conformance>"));
        assert!(bevat(&bytes, "Interview wethouder"));
        assert!(bevat(&bytes, "Lokaal verwerkt"));
        assert!(bevat(&bytes, "Z-2026-1043"));
        assert!(bevat(&bytes, "OutputIntent"));
    }

    #[test]
    fn lange_tekst_loopt_over_meer_paginas() {
        let regels = (0..200)
            .map(|i| Regel {
                start_ms: i * 5_000,
                eind_ms: i * 5_000 + 4_000,
                tekst:
                    "Dit is een zin die een paar keer terugkomt om de pagina te vullen, met é en ë."
                        .repeat(2),
                spreker: None,
            })
            .collect();
        let doc = Document {
            regels,
            ..document()
        };
        let bytes = pdf(&doc, None, true).expect("pdf");
        assert!(bevat(&bytes, "/Count "));
        assert!(!bevat(&bytes, "/Count 1\n") && !bevat(&bytes, "/Count 1 "));
    }

    #[test]
    fn pdf_met_spreker_is_pdf_a_met_een_vet_lettertype_erbij() {
        let met = pdf(&gesprek_document(), None, true).expect("pdf");
        assert!(bevat(&met, "<pdfaid:part>2</pdfaid:part>"));
        let zonder = pdf(&document(), None, true).expect("pdf");
        // Het vette lettertype zit er alleen in als een label het gebruikt.
        let aantal = |bytes: &[u8]| bytes.windows(10).filter(|w| w == b"/FontFile2").count();
        assert!(aantal(&met) > aantal(&zonder));
        // Ook zonder tijden en met een naam die te breed is voor naast de tekst.
        let lang = Document {
            regels: vec![Regel {
                start_ms: 0,
                eind_ms: 1_000,
                tekst: "Dit is een zin na een heel lange naam.".to_string(),
                spreker: Some("Wethouder Ruimtelijke Ordening en Wonen ".repeat(6)),
            }],
            ..document()
        };
        assert!(pdf(&lang, None, false).is_ok());
        assert!(pdf(&lang, None, true).is_ok());
    }

    #[test]
    fn de_eerste_regel_met_een_label_is_smaller_en_de_rest_niet() {
        let titel = Letter::nieuw(SORA, 600.0).expect("sora");
        let tekst = Letter::nieuw(GEIST, 400.0).expect("geist");
        let vet = Letter::nieuw(GEIST, 700.0).expect("geist vet");
        let zetter = Zetter::new(&titel, &tekst, &vet);
        // Vet is breder dan gewoon, dus het label neemt meer ruimte dan zijn tekens in normaal.
        let (label, ruimte) = zetter.label(Some("Anderen"), 400.0);
        assert_eq!(label.as_deref(), Some("Anderen:"));
        assert!(ruimte > tekst.meter().breedte("Anderen: ", 10.5));
        assert_eq!(zetter.label(None, 400.0), (None, 0.0));
        // Een naam die breder is dan de helft van de regel, krijgt een eigen regel.
        assert!(zetter.label(Some(&"W".repeat(60)), 400.0).1 <= 0.0);

        let zin = "woord ".repeat(80);
        let zin = zin.trim();
        let regels = zetter.regels_na_label(zin, 400.0, ruimte);
        let meter = tekst.meter();
        assert!(regels[0].len() < zetter.regels(Stijl::Tekst, zin, 400.0)[0].len());
        assert!(meter.breedte(&regels[0], 10.5) <= (400.0 - ruimte) * SPELING);
        assert!(
            regels[1..]
                .iter()
                .all(|r| meter.breedte(r, 10.5) <= 400.0 * SPELING)
        );
        // Niets gaat verloren bij het afbreken.
        assert_eq!(regels.join(" "), zin);
    }

    #[test]
    fn onbekende_tekens_worden_een_vraagteken() {
        let letter = Letter::nieuw(GEIST, 400.0).expect("geist");
        assert_eq!(letter.meter().dekbaar("café 東京"), "café ??");
        // Het opsomteken van de notulen staat wel in Geist.
        assert_eq!(
            letter.meter().dekbaar(super::super::OPSOMTEKEN),
            super::super::OPSOMTEKEN
        );
        // En de PDF lukt dan nog steeds (PDF/A weigert het .notdef-teken).
        let doc = Document {
            regels: vec![Regel {
                start_ms: 0,
                eind_ms: 1_000,
                tekst: "Tokio heet 東京.".to_string(),
                spreker: None,
            }],
            ..document()
        };
        assert!(pdf(&doc, None, false).is_ok());
    }

    #[test]
    fn breed_woord_wordt_afgebroken() {
        let titel = Letter::nieuw(SORA, 600.0).expect("sora");
        let tekst = Letter::nieuw(GEIST, 400.0).expect("geist");
        let vet = Letter::nieuw(GEIST, 700.0).expect("geist vet");
        let zetter = Zetter::new(&titel, &tekst, &vet);
        let regels = zetter.regels(Stijl::Tekst, &"x".repeat(400), 200.0);
        assert!(regels.len() > 1);
        let meter = tekst.meter();
        assert!(regels.iter().all(|r| meter.breedte(r, 10.5) <= 200.0));
        // Een heel lang woord gaat in lineaire tijd: elk teken wordt één keer gemeten.
        let begin = std::time::Instant::now();
        let lang = zetter.regels(Stijl::Tekst, &"x".repeat(200_000), 200.0);
        assert!(lang.len() > 100);
        assert!(begin.elapsed() < std::time::Duration::from_secs(5));
    }
}
