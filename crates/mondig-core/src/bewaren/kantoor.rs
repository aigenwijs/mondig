//! Word (.docx, Office Open XML, ECMA-376) en LibreOffice (.odt, OpenDocument 1.3), met de
//! hand gemaakt: een zip met een paar kleine XML-delen (ADR 0017).
//!
//! Beide krijgen dezelfde opbouw: de titel (stijl Titel), de gegevens klein en grijs, een
//! witregel, en per stuk van het model een alinea. Met tijden staat de tijd grijs vooraan,
//! met een tab en een hangende inspringing, zodat de tekst netjes onder elkaar blijft.
//!
//! De gegevens staan ook in de documenteigenschappen: in Word in `docProps/core.xml` (titel,
//! beschrijving, trefwoord, taal, datum) en `docProps/custom.xml` (Bron, Duur, Model, Klant,
//! Verwerking), in LibreOffice in `meta.xml` (dezelfde velden als `meta:user-defined`).
//!
//! Bij een gesprek (regels met een spreker) staat het label vet voor de tekst ("**Jij:** tekst"),
//! na de tijd. Zonder spreker is de uitvoer ongewijzigd.
//!
//! Lettertype: in Word Calibri, in LibreOffice de standaard. Sora en Geist staan niet op elke
//! computer en worden alleen in de PDF meegeleverd.

use std::fmt::Write as _;
use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use super::{
    BewaarFout, Document, Notulenblok, Notulenregel, OPSOMTEKEN, Taalcode, VERSIE, Zoned,
    iso_lokaal, iso_utc, schone_tekst, tijdcode,
};

/// Tekst voor XML: de vijf entiteiten, en tekens die XML 1.0 niet toestaat vallen weg.
pub(super) fn xml(tekst: &str) -> String {
    let mut uit = String::with_capacity(tekst.len());
    for teken in tekst.chars() {
        match teken {
            '&' => uit.push_str("&amp;"),
            '<' => uit.push_str("&lt;"),
            '>' => uit.push_str("&gt;"),
            '"' => uit.push_str("&quot;"),
            '\'' => uit.push_str("&apos;"),
            '\t' | '\n' | '\r' => uit.push(teken),
            c if c.is_control() || matches!(c, '\u{fffe}' | '\u{ffff}') => {}
            c => uit.push(c),
        }
    }
    uit
}

/// Pakt de delen in een zip, in deze volgorde. Delen met `opslaan` gaan zonder compressie
/// (de `mimetype` van ODF moet zo, en als eerste).
fn inpakken(delen: &[(&str, String, bool)], moment: &Zoned) -> Result<Vec<u8>, BewaarFout> {
    let fout = |e: zip::result::ZipError| BewaarFout::Inpakken(e.to_string());
    let tijd = DateTime::from_date_and_time(
        u16::try_from(moment.year()).unwrap_or(1980),
        u8::try_from(moment.month()).unwrap_or(1),
        u8::try_from(moment.day()).unwrap_or(1),
        u8::try_from(moment.hour()).unwrap_or(0),
        u8::try_from(moment.minute()).unwrap_or(0),
        u8::try_from(moment.second()).unwrap_or(0),
    )
    .unwrap_or_default();
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (naam, inhoud, opslaan) in delen {
        let methode = if *opslaan {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        let opties = SimpleFileOptions::default()
            .compression_method(methode)
            .last_modified_time(tijd);
        zip.start_file(*naam, opties).map_err(fout)?;
        zip.write_all(inhoud.as_bytes())
            .map_err(|e| BewaarFout::Inpakken(e.to_string()))?;
    }
    Ok(zip.finish().map_err(fout)?.into_inner())
}

const KOP: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;

// ---------------------------------------------------------------------------
// Word
// ---------------------------------------------------------------------------

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const REL_DOC: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn docx_content_types() -> String {
    format!(
        r#"{KOP}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/><Override PartName="/docProps/custom.xml" ContentType="application/vnd.openxmlformats-officedocument.custom-properties+xml"/></Types>"#
    )
}

fn docx_rels() -> String {
    format!(
        r#"{KOP}<Relationships xmlns="{REL}"><Relationship Id="rId1" Type="{REL_DOC}/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="{REL_DOC}/extended-properties" Target="docProps/app.xml"/><Relationship Id="rId4" Type="{REL_DOC}/custom-properties" Target="docProps/custom.xml"/></Relationships>"#
    )
}

fn docx_document_rels() -> String {
    format!(
        r#"{KOP}<Relationships xmlns="{REL}"><Relationship Id="rId1" Type="{REL_DOC}/styles" Target="styles.xml"/></Relationships>"#
    )
}

fn docx_core(document: &Document, notulen: Option<&Notulenblok>, taal: Taalcode) -> String {
    let taal = taal.volledig;
    let trefwoord = document
        .klant()
        .map(|l| format!("<cp:keywords>{}</cp:keywords>", xml(&l)))
        .unwrap_or_default();
    let datum = iso_utc(&document.moment);
    format!(
        r#"{KOP}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{titel}</dc:title><dc:subject>Transcript</dc:subject>{trefwoord}<dc:description>{beschrijving}</dc:description><dc:language>{taal}</dc:language><dcterms:created xsi:type="dcterms:W3CDTF">{datum}</dcterms:created><dcterms:modified xsi:type="dcterms:W3CDTF">{datum}</dcterms:modified></cp:coreProperties>"#,
        titel = xml(&document.titel()),
        beschrijving = xml(&document.beschrijving_met(notulen)),
    )
}

fn docx_app() -> String {
    format!(
        r#"{KOP}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application>Mondig {VERSIE}</Application></Properties>"#
    )
}

fn docx_custom(document: &Document, notulen: Option<&Notulenblok>) -> String {
    let mut eigenschappen = String::new();
    // De pid begint bij 2 (ECMA-376 deel 1, sectie 22.3.2); de fmtid is voor alle eigen velden gelijk.
    for (pid, (sleutel, waarde)) in (2..).zip(document.gegevens_met(notulen)) {
        let _ = write!(
            eigenschappen,
            r#"<property fmtid="{{D5CDD505-2E9C-101B-9397-08002B2CF9AE}}" pid="{pid}" name="{sleutel}"><vt:lpwstr>{}</vt:lpwstr></property>"#,
            xml(&waarde)
        );
    }
    format!(
        r#"{KOP}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">{eigenschappen}</Properties>"#
    )
}

fn docx_styles(taal: Taalcode) -> String {
    let taal = taal.volledig;
    format!(
        r#"{KOP}<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:eastAsia="Calibri" w:cs="Calibri"/><w:sz w:val="22"/><w:szCs w:val="22"/><w:lang w:val="{taal}"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="276" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style><w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="200"/></w:pPr><w:rPr><w:b/><w:sz w:val="40"/><w:szCs w:val="40"/></w:rPr></w:style><w:style w:type="paragraph" w:customStyle="1" w:styleId="MondigGegevens"><w:name w:val="Mondig gegevens"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0"/></w:pPr><w:rPr><w:color w:val="5A544C"/><w:sz w:val="18"/><w:szCs w:val="18"/></w:rPr></w:style><w:style w:type="paragraph" w:customStyle="1" w:styleId="MondigTekstMetTijd"><w:name w:val="Mondig tekst met tijd"/><w:basedOn w:val="Normal"/><w:pPr><w:tabs><w:tab w:val="left" w:pos="851"/></w:tabs><w:ind w:left="851" w:hanging="851"/></w:pPr></w:style><w:style w:type="paragraph" w:customStyle="1" w:styleId="MondigKop"><w:name w:val="Mondig kop"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="120"/></w:pPr><w:rPr><w:b/><w:sz w:val="30"/><w:szCs w:val="30"/></w:rPr></w:style><w:style w:type="paragraph" w:customStyle="1" w:styleId="MondigTussenkop"><w:name w:val="Mondig tussenkop"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:pPr><w:keepNext/><w:spacing w:before="200" w:after="80"/></w:pPr><w:rPr><w:b/><w:sz w:val="24"/><w:szCs w:val="24"/></w:rPr></w:style><w:style w:type="character" w:customStyle="1" w:styleId="MondigTijd"><w:name w:val="Mondig tijd"/><w:rPr><w:color w:val="5A544C"/><w:sz w:val="18"/><w:szCs w:val="18"/></w:rPr></w:style></w:styles>"#
    )
}

fn w_tekst(tekst: &str) -> String {
    format!(
        r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
        xml(tekst)
    )
}

fn w_alinea(stijl: Option<&str>, inhoud: &str) -> String {
    let stijl = stijl
        .map(|s| format!(r#"<w:pPr><w:pStyle w:val="{s}"/></w:pPr>"#))
        .unwrap_or_default();
    format!("<w:p>{stijl}{inhoud}</w:p>")
}

/// De notulen in Word: koppen, alinea's en per punt de tijd (of een opsomteken) met een tab.
fn docx_notulen(notulen: &Notulenblok) -> String {
    let mut body = String::new();
    for regel in &notulen.regels {
        match regel {
            Notulenregel::Kop(tekst) => {
                body.push_str(&w_alinea(Some("MondigKop"), &w_tekst(&schone_tekst(tekst))));
            }
            Notulenregel::Tussenkop(tekst) => {
                body.push_str(&w_alinea(
                    Some("MondigTussenkop"),
                    &w_tekst(&schone_tekst(tekst)),
                ));
            }
            Notulenregel::Alinea(tekst) => {
                body.push_str(&w_alinea(None, &w_tekst(&schone_tekst(tekst))));
            }
            Notulenregel::Punt { tijd_ms, tekst } => {
                let voor = tijd_ms.map_or_else(|| OPSOMTEKEN.to_string(), tijdcode);
                let inhoud = format!(
                    r#"<w:r><w:rPr><w:rStyle w:val="MondigTijd"/></w:rPr><w:t>{}</w:t></w:r><w:r><w:tab/><w:t xml:space="preserve">{}</w:t></w:r>"#,
                    xml(&voor),
                    xml(&schone_tekst(tekst))
                );
                body.push_str(&w_alinea(Some("MondigTekstMetTijd"), &inhoud));
            }
        }
    }
    body.push_str(&w_alinea(
        Some("MondigKop"),
        &w_tekst(&schone_tekst(&notulen.verslagkop)),
    ));
    body
}

fn docx_document(document: &Document, notulen: Option<&Notulenblok>, tijden: bool) -> String {
    let mut body = w_alinea(Some("Title"), &w_tekst(&document.titel()));
    for (sleutel, waarde) in document.gegevens_met(notulen) {
        body.push_str(&w_alinea(
            Some("MondigGegevens"),
            &w_tekst(&format!("{sleutel}: {waarde}")),
        ));
    }
    body.push_str("<w:p/>");
    if let Some(notulen) = notulen {
        body.push_str(&docx_notulen(notulen));
    }
    let regels = document.tekstregels();
    if regels.is_empty() {
        body.push_str(&w_alinea(None, &w_tekst(&document.zonder_tekst())));
    }
    for regel in regels {
        let zin = regel.tekst;
        // Het label vet vóór de tekst; zonder spreker blijft er niets over.
        let label = regel
            .spreker
            .map(|s| {
                format!(
                    r#"<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">{}: </w:t></w:r>"#,
                    xml(&s)
                )
            })
            .unwrap_or_default();
        if tijden {
            let tijd = tijdcode(regel.start_ms);
            let inhoud = if label.is_empty() {
                format!(
                    r#"<w:r><w:rPr><w:rStyle w:val="MondigTijd"/></w:rPr><w:t>{tijd}</w:t></w:r><w:r><w:tab/><w:t xml:space="preserve">{}</w:t></w:r>"#,
                    xml(&zin)
                )
            } else {
                format!(
                    r#"<w:r><w:rPr><w:rStyle w:val="MondigTijd"/></w:rPr><w:t>{tijd}</w:t></w:r><w:r><w:tab/></w:r>{label}{}"#,
                    w_tekst(&zin)
                )
            };
            body.push_str(&w_alinea(Some("MondigTekstMetTijd"), &inhoud));
        } else {
            body.push_str(&w_alinea(None, &format!("{label}{}", w_tekst(&zin))));
        }
    }
    // A4 met marges van 2,5 cm (1418 twips).
    body.push_str(r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1418" w:right="1418" w:bottom="1418" w:left="1418" w:header="709" w:footer="709" w:gutter="0"/></w:sectPr>"#);
    format!(r#"{KOP}<w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#)
}

/// Een Word-document.
pub(super) fn docx(
    document: &Document,
    notulen: Option<&Notulenblok>,
    tijden: bool,
) -> Result<Vec<u8>, BewaarFout> {
    let taal = Taalcode::van(notulen);
    inpakken(
        &[
            ("[Content_Types].xml", docx_content_types(), false),
            ("_rels/.rels", docx_rels(), false),
            (
                "docProps/core.xml",
                docx_core(document, notulen, taal),
                false,
            ),
            ("docProps/app.xml", docx_app(), false),
            ("docProps/custom.xml", docx_custom(document, notulen), false),
            ("word/_rels/document.xml.rels", docx_document_rels(), false),
            ("word/styles.xml", docx_styles(taal), false),
            (
                "word/document.xml",
                docx_document(document, notulen, tijden),
                false,
            ),
        ],
        &document.moment,
    )
}

// ---------------------------------------------------------------------------
// LibreOffice
// ---------------------------------------------------------------------------

const ODF_VERSIE: &str = "1.3";
const MIMETYPE: &str = "application/vnd.oasis.opendocument.text";
const NS_OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
const NS_STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
const NS_TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
const NS_FO: &str = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";
/// De naam van de vette tekststijl voor het label van een spreker (een automatische stijl).
const SPREKER_STIJL: &str = "Mondig_20_spreker";
const NS_META: &str = "urn:oasis:names:tc:opendocument:xmlns:meta:1.0";

fn odt_manifest() -> String {
    format!(
        r#"{KOP}<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="{ODF_VERSIE}"><manifest:file-entry manifest:full-path="/" manifest:version="{ODF_VERSIE}" manifest:media-type="{MIMETYPE}"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/><manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/><manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/></manifest:manifest>"#
    )
}

fn odt_meta(document: &Document, notulen: Option<&Notulenblok>, taal: Taalcode) -> String {
    let code = taal.volledig;
    let trefwoord = document
        .klant()
        .map(|l| format!("<meta:keyword>{}</meta:keyword>", xml(&l)))
        .unwrap_or_default();
    let mut eigen = String::new();
    for (sleutel, waarde) in document.gegevens_met(notulen) {
        let _ = write!(
            eigen,
            r#"<meta:user-defined meta:name="{sleutel}">{}</meta:user-defined>"#,
            xml(&waarde)
        );
    }
    let datum = iso_lokaal(&document.moment);
    format!(
        r#"{KOP}<office:document-meta xmlns:office="{NS_OFFICE}" xmlns:meta="{NS_META}" xmlns:dc="http://purl.org/dc/elements/1.1/" office:version="{ODF_VERSIE}"><office:meta><meta:generator>Mondig/{VERSIE}</meta:generator><dc:title>{titel}</dc:title><dc:subject>Transcript</dc:subject><dc:description>{beschrijving}</dc:description><dc:language>{code}</dc:language>{trefwoord}<meta:creation-date>{datum}</meta:creation-date><dc:date>{datum}</dc:date>{eigen}</office:meta></office:document-meta>"#,
        titel = xml(&document.titel()),
        beschrijving = xml(&document.beschrijving_met(notulen)),
    )
}

fn odt_styles(taal: Taalcode) -> String {
    let (taal, land) = (taal.taal, taal.land);
    format!(
        r##"{KOP}<office:document-styles xmlns:office="{NS_OFFICE}" xmlns:style="{NS_STYLE}" xmlns:text="{NS_TEXT}" xmlns:fo="{NS_FO}" office:version="{ODF_VERSIE}"><office:styles><style:default-style style:family="paragraph"><style:paragraph-properties fo:margin-top="0cm" fo:margin-bottom="0.21cm" fo:line-height="115%"/><style:text-properties fo:font-size="11pt" fo:language="{taal}" fo:country="{land}"/></style:default-style><style:style style:name="Standard" style:family="paragraph" style:class="text"/><style:style style:name="Title" style:display-name="Titel" style:family="paragraph" style:parent-style-name="Standard" style:next-style-name="Standard" style:class="chapter"><style:paragraph-properties fo:margin-bottom="0.35cm"/><style:text-properties fo:font-size="20pt" fo:font-weight="bold"/></style:style><style:style style:name="Mondig_20_gegevens" style:display-name="Mondig gegevens" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:margin-bottom="0cm"/><style:text-properties fo:font-size="9pt" fo:color="#5a544c"/></style:style><style:style style:name="Mondig_20_tekst_20_met_20_tijd" style:display-name="Mondig tekst met tijd" style:family="paragraph" style:parent-style-name="Standard"><style:paragraph-properties fo:margin-left="1.5cm" fo:text-indent="-1.5cm"><style:tab-stops><style:tab-stop style:position="0cm"/></style:tab-stops></style:paragraph-properties></style:style><style:style style:name="Mondig_20_kop" style:display-name="Mondig kop" style:family="paragraph" style:parent-style-name="Standard" style:next-style-name="Standard"><style:paragraph-properties fo:margin-top="0.42cm" fo:margin-bottom="0.21cm" fo:keep-with-next="always"/><style:text-properties fo:font-size="15pt" fo:font-weight="bold"/></style:style><style:style style:name="Mondig_20_tussenkop" style:display-name="Mondig tussenkop" style:family="paragraph" style:parent-style-name="Standard" style:next-style-name="Standard"><style:paragraph-properties fo:margin-top="0.35cm" fo:margin-bottom="0.14cm" fo:keep-with-next="always"/><style:text-properties fo:font-size="12pt" fo:font-weight="bold"/></style:style><style:style style:name="Mondig_20_tijd" style:display-name="Mondig tijd" style:family="text"><style:text-properties fo:font-size="9pt" fo:color="#5a544c"/></style:style></office:styles><office:automatic-styles><style:page-layout style:name="pm1"><style:page-layout-properties fo:page-width="21cm" fo:page-height="29.7cm" fo:margin-top="2.5cm" fo:margin-bottom="2.5cm" fo:margin-left="2.5cm" fo:margin-right="2.5cm"/></style:page-layout></office:automatic-styles><office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"/></office:master-styles></office:document-styles>"##
    )
}

fn odt_content(document: &Document, notulen: Option<&Notulenblok>, tijden: bool) -> String {
    let alinea = |stijl: &str, inhoud: &str| {
        format!(r#"<text:p text:style-name="{stijl}">{inhoud}</text:p>"#)
    };
    let mut tekst = alinea("Title", &xml(&document.titel()));
    for (sleutel, waarde) in document.gegevens_met(notulen) {
        tekst.push_str(&alinea(
            "Mondig_20_gegevens",
            &xml(&format!("{sleutel}: {waarde}")),
        ));
    }
    tekst.push_str(r#"<text:p text:style-name="Standard"/>"#);
    if let Some(notulen) = notulen {
        for regel in &notulen.regels {
            match regel {
                Notulenregel::Kop(t) => {
                    tekst.push_str(&alinea("Mondig_20_kop", &xml(&schone_tekst(t))));
                }
                Notulenregel::Tussenkop(t) => {
                    tekst.push_str(&alinea("Mondig_20_tussenkop", &xml(&schone_tekst(t))));
                }
                Notulenregel::Alinea(t) => {
                    tekst.push_str(&alinea("Standard", &xml(&schone_tekst(t))));
                }
                Notulenregel::Punt { tijd_ms, tekst: t } => {
                    let voor = tijd_ms.map_or_else(|| OPSOMTEKEN.to_string(), tijdcode);
                    let inhoud = format!(
                        r#"<text:span text:style-name="Mondig_20_tijd">{}</text:span><text:tab/>{}"#,
                        xml(&voor),
                        xml(&schone_tekst(t))
                    );
                    tekst.push_str(&alinea("Mondig_20_tekst_20_met_20_tijd", &inhoud));
                }
            }
        }
        tekst.push_str(&alinea(
            "Mondig_20_kop",
            &xml(&schone_tekst(&notulen.verslagkop)),
        ));
    }
    let regels = document.tekstregels();
    if regels.is_empty() {
        tekst.push_str(&alinea("Standard", &xml(&document.zonder_tekst())));
    }
    let mut met_spreker = false;
    for regel in regels {
        // Het label vet vóór de tekst; zonder spreker blijft er niets over.
        let label = regel
            .spreker
            .map(|s| {
                met_spreker = true;
                format!(
                    r#"<text:span text:style-name="{SPREKER_STIJL}">{}: </text:span>"#,
                    xml(&s)
                )
            })
            .unwrap_or_default();
        if tijden {
            let inhoud = format!(
                r#"<text:span text:style-name="Mondig_20_tijd">{}</text:span><text:tab/>{label}{}"#,
                tijdcode(regel.start_ms),
                xml(&regel.tekst)
            );
            tekst.push_str(&alinea("Mondig_20_tekst_20_met_20_tijd", &inhoud));
        } else {
            tekst.push_str(&alinea(
                "Standard",
                &format!("{label}{}", xml(&regel.tekst)),
            ));
        }
    }
    // De vette stijl voor het label staat alleen in het document als er een spreker is.
    let stijlen = if met_spreker {
        format!(
            r#"<office:automatic-styles><style:style style:name="{SPREKER_STIJL}" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style></office:automatic-styles>"#
        )
    } else {
        String::new()
    };
    format!(
        r#"{KOP}<office:document-content xmlns:office="{NS_OFFICE}" xmlns:style="{NS_STYLE}" xmlns:text="{NS_TEXT}" xmlns:fo="{NS_FO}" office:version="{ODF_VERSIE}">{stijlen}<office:body><office:text>{tekst}</office:text></office:body></office:document-content>"#
    )
}

/// Een LibreOffice-document. De `mimetype` staat als eerste en zonder compressie (ODF 1.3,
/// deel 2, 3.3).
pub(super) fn odt(
    document: &Document,
    notulen: Option<&Notulenblok>,
    tijden: bool,
) -> Result<Vec<u8>, BewaarFout> {
    let taal = Taalcode::van(notulen);
    inpakken(
        &[
            ("mimetype", MIMETYPE.to_string(), true),
            ("META-INF/manifest.xml", odt_manifest(), false),
            ("meta.xml", odt_meta(document, notulen, taal), false),
            ("styles.xml", odt_styles(taal), false),
            ("content.xml", odt_content(document, notulen, tijden), false),
        ],
        &document.moment,
    )
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::super::tests::{document, gesprek_document};
    use super::*;

    /// Leest alle delen uit een zip, met hun compressie.
    fn lees(bytes: &[u8]) -> Vec<(String, String, CompressionMethod)> {
        let mut archief = zip::ZipArchive::new(Cursor::new(bytes)).expect("geldige zip");
        (0..archief.len())
            .map(|i| {
                let mut deel = archief.by_index(i).expect("deel");
                let mut inhoud = String::new();
                deel.read_to_string(&mut inhoud).expect("utf-8");
                (
                    deel.name().expect("naam").to_string(),
                    inhoud,
                    deel.compression(),
                )
            })
            .collect()
    }

    fn deel<'a>(delen: &'a [(String, String, CompressionMethod)], naam: &str) -> &'a str {
        delen
            .iter()
            .find(|(n, _, _)| n == naam)
            .map(|(_, inhoud, _)| inhoud.as_str())
            .expect("het deel staat in de zip")
    }

    #[test]
    fn xml_ontsnapt_en_laat_ongeldige_tekens_weg() {
        assert_eq!(
            xml(r#"a & b < c > "d" 'e'"#),
            "a &amp; b &lt; c &gt; &quot;d&quot; &apos;e&apos;"
        );
        assert_eq!(xml("a\u{1}b\u{fffe}c"), "abc");
    }

    #[test]
    fn docx_heeft_alle_delen_en_de_gegevens() {
        let bytes = docx(&document(), None, true).expect("docx");
        let delen = lees(&bytes);
        let namen: Vec<_> = delen.iter().map(|(n, _, _)| n.as_str()).collect();
        assert_eq!(namen[0], "[Content_Types].xml");
        for nodig in [
            "_rels/.rels",
            "word/document.xml",
            "word/styles.xml",
            "docProps/core.xml",
            "docProps/custom.xml",
        ] {
            assert!(namen.contains(&nodig), "{nodig}");
        }
        let core = deel(&delen, "docProps/core.xml");
        assert!(core.contains("<dc:title>Interview wethouder</dc:title>"));
        assert!(core.contains("<cp:keywords>Z-2026-1043</cp:keywords>"));
        assert!(core.contains(">2026-11-01T13:32:05Z</dcterms:created>"));
        let custom = deel(&delen, "docProps/custom.xml");
        assert!(
            custom
                .contains(r#"pid="2" name="Datum"><vt:lpwstr>1 november 2026, 14:32</vt:lpwstr>"#)
        );
        assert!(custom.contains(r#"name="Verwerking"><vt:lpwstr>Op deze computer</vt:lpwstr>"#));
        let tekst = deel(&delen, "word/document.xml");
        assert!(tekst.contains("<w:t>00:07</w:t></w:r><w:r><w:tab/>"));
        assert!(tekst.contains("de markt &amp; minder &lt;auto&apos;s&gt;."));
    }

    #[test]
    fn docx_en_odt_noemen_een_aanbieder_buiten_deze_computer() {
        use super::super::tests::blok;
        let extern_blok = blok(Some("Mistral (Europees)"));
        let delen = lees(&docx(&document(), Some(&extern_blok), true).expect("docx"));
        let custom = deel(&delen, "docProps/custom.xml");
        assert!(
            custom.contains(
                "Uitschrift op deze computer, notulen door Mistral (Europees)</vt:lpwstr>"
            )
        );
        let tekst = deel(&delen, "word/document.xml");
        assert!(tekst.contains("notulen door Mistral (Europees)"));
        let lokaal = lees(&docx(&document(), Some(&blok(None)), true).expect("docx"));
        assert!(deel(&lokaal, "docProps/custom.xml").contains(">Op deze computer</vt:lpwstr>"));
        let odt = lees(&odt(&document(), Some(&extern_blok), true).expect("odt"));
        assert!(deel(&odt, "meta.xml").contains("notulen door Mistral (Europees)"));
    }

    #[test]
    fn docx_zonder_tijden_heeft_geen_tijden() {
        let bytes = docx(&document(), None, false).expect("docx");
        let delen = lees(&bytes);
        let tekst = deel(&delen, "word/document.xml");
        assert!(!tekst.contains("MondigTijd"));
        assert!(tekst.contains("Goedemiddag, fijn dat u tijd heeft."));
    }

    #[test]
    fn docx_met_spreker_zet_het_label_vet_voor_de_tekst() {
        let delen = lees(&docx(&gesprek_document(), None, true).expect("docx"));
        let tekst = deel(&delen, "word/document.xml");
        assert!(tekst.contains(
            r#"<w:t>01:02</w:t></w:r><w:r><w:tab/></w:r><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">Jij: </w:t></w:r><w:r><w:t xml:space="preserve">Goedemiddag, welkom.</w:t></w:r>"#
        ));
        assert!(tekst.contains("Wethouder &lt;De Vries&gt; &amp;: </w:t>"));
        // De spreker die na het schoonmaken leeg is, krijgt geen label.
        assert!(tekst.contains(r#"<w:tab/><w:t xml:space="preserve">Zonder naam.</w:t>"#));
        assert_eq!(tekst.matches("<w:b/>").count(), 2);
        // Zonder tijden staat het label vooraan in de alinea.
        let delen = lees(&docx(&gesprek_document(), None, false).expect("docx"));
        assert!(deel(&delen, "word/document.xml").contains(
            r#"<w:p><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">Jij: </w:t></w:r><w:r><w:t xml:space="preserve">Goedemiddag, welkom.</w:t></w:r></w:p>"#
        ));
        // Zonder spreker geen vette tekst.
        let delen = lees(&docx(&document(), None, true).expect("docx"));
        assert!(!deel(&delen, "word/document.xml").contains("<w:b/>"));
    }

    #[test]
    fn odt_met_spreker_zet_het_label_vet_voor_de_tekst() {
        let delen = lees(&odt(&gesprek_document(), None, true).expect("odt"));
        let inhoud = deel(&delen, "content.xml");
        assert!(inhoud.contains(
            r#"<office:automatic-styles><style:style style:name="Mondig_20_spreker" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style></office:automatic-styles><office:body>"#
        ));
        assert!(inhoud.contains(
            r#"<text:span text:style-name="Mondig_20_tijd">01:02</text:span><text:tab/><text:span text:style-name="Mondig_20_spreker">Jij: </text:span>Goedemiddag, welkom."#
        ));
        assert!(inhoud.contains("Wethouder &lt;De Vries&gt; &amp;: </text:span>"));
        assert!(inhoud.contains("<text:tab/>Zonder naam."));
        let delen = lees(&odt(&gesprek_document(), None, false).expect("odt"));
        assert!(deel(&delen, "content.xml").contains(
            r#"<text:p text:style-name="Standard"><text:span text:style-name="Mondig_20_spreker">Jij: </text:span>Goedemiddag, welkom.</text:p>"#
        ));
        // Zonder spreker geen extra stijl en geen span.
        let delen = lees(&odt(&document(), None, true).expect("odt"));
        let inhoud = deel(&delen, "content.xml");
        assert!(!inhoud.contains("automatic-styles") && !inhoud.contains("spreker"));
    }

    #[test]
    fn odt_begint_met_een_ongecomprimeerde_mimetype() {
        let bytes = odt(&document(), None, true).expect("odt");
        // Het mimetype staat op een vaste plek, zodat een programma het herkent zonder uitpakken.
        assert_eq!(&bytes[30..38], b"mimetype");
        assert_eq!(&bytes[38..38 + MIMETYPE.len()], MIMETYPE.as_bytes());
        let delen = lees(&bytes);
        assert_eq!(delen[0].0, "mimetype");
        assert_eq!(delen[0].2, CompressionMethod::Stored);
        let meta = deel(&delen, "meta.xml");
        assert!(meta.contains("<meta:keyword>Z-2026-1043</meta:keyword>"));
        assert!(meta.contains(
            r#"<meta:user-defined meta:name="Bron">Interview wethouder.m4a</meta:user-defined>"#
        ));
        assert!(meta.contains("<meta:creation-date>2026-11-01T14:32:05</meta:creation-date>"));
        let inhoud = deel(&delen, "content.xml");
        assert!(inhoud.contains(
            r#"<text:span text:style-name="Mondig_20_tijd">00:07</text:span><text:tab/>Ja, dank u."#
        ));
    }
}
