//! Platte tekst en ondertitels.
//!
//! - **TXT**: UTF-8 met BOM (dan leest ook Kladblok of Word de é goed), regeleinden CRLF,
//!   bovenaan de titel en de gegevens.
//! - **MD**: Markdown, UTF-8 zonder BOM (de gangbare vorm), LF, een kop, de gegevens als lijst en
//!   per regel een alinea met de spreker en de tijd vet erbij. Tekens met een betekenis in
//!   Markdown krijgen een backslash.
//! - **HTML**: een zelfstandige pagina in UTF-8 zonder BOM: alle tekst met entiteiten (`&`, `<`,
//!   `>`, `"` en `'`), de opmaak in een `<style>` in de pagina zelf, geen lettertypen, scripts,
//!   plaatjes of verwijzingen naar buiten, en een Content-Security-Policy die dat afdwingt.
//! - **SRT**: genummerde blokken met `00:00:07,000 --> 00:00:15,000`, UTF-8 met BOM en CRLF,
//!   zoals de meeste spelers het verwachten. SRT heeft geen plek voor gegevens.
//! - **VTT**: WebVTT (`w3.org/TR/webvtt1`): `WEBVTT`, de gegevens in een `NOTE`-blok, tijden
//!   als `00:00:07.000`, `&`, `<` en `>` als entiteit, nergens `-->` in de tekst.
//!
//! Bij een gesprek (regels met een spreker) staat het label voor de tekst: in TXT als
//! "Jij: tekst", in SRT voor de eerste ondertitel van een regel, in VTT als stem
//! (`<v Jij>`) bij elke ondertitel van de regel. Zonder spreker is de uitvoer ongewijzigd.
//!
//! Een ondertitel is hooguit twee regels van ongeveer [`REGEL`] tekens. Een lang stuk van het
//! model wordt op woordgrenzen in blokken gesplitst; de tijd van het stuk wordt naar het
//! aantal tekens over de blokken verdeeld.

use std::fmt::Write as _;

use super::{Document, Notulenblok, Notulenregel, OPSOMTEKEN, schone_tekst, tijdcode};

/// Ongeveer zoveel tekens per ondertitelregel (de gangbare richtlijn is 42).
const REGEL: usize = 42;
/// Zo lang staat een ondertitel minstens als het model geen eindtijd gaf.
const MINIMAAL_MS: u64 = 1_000;

const BOM: &str = "\u{feff}";

/// Platte tekst met de gegevens bovenaan.
pub(super) fn txt(document: &Document, notulen: Option<&Notulenblok>, tijden: bool) -> Vec<u8> {
    let mut regels = vec![document.titel()];
    regels.extend(
        document
            .gegevens_met(notulen)
            .into_iter()
            .map(|(sleutel, waarde)| format!("{sleutel}: {waarde}")),
    );
    regels.push(String::new());
    if let Some(notulen) = notulen {
        for regel in &notulen.regels {
            match regel {
                Notulenregel::Kop(tekst) | Notulenregel::Alinea(tekst) => {
                    regels.push(schone_tekst(tekst));
                }
                Notulenregel::Tussenkop(tekst) => {
                    regels.push(String::new());
                    regels.push(schone_tekst(tekst));
                }
                Notulenregel::Punt { tijd_ms, tekst } => {
                    let voor = tijd_ms.map_or_else(|| OPSOMTEKEN.to_string(), tijdcode);
                    regels.push(format!("{voor}  {}", schone_tekst(tekst)));
                }
            }
        }
        regels.push(String::new());
        regels.push(schone_tekst(&notulen.verslagkop));
        regels.push(String::new());
    }
    let tekst = document.tekstregels();
    if tekst.is_empty() {
        regels.push(document.zonder_tekst());
    }
    for regel in tekst {
        let zin = match &regel.spreker {
            Some(spreker) => format!("{spreker}: {}", regel.tekst),
            None => regel.tekst,
        };
        regels.push(if tijden {
            format!("{}  {zin}", tijdcode(regel.start_ms))
        } else {
            zin
        });
    }
    let mut uit = String::from(BOM);
    for regel in regels {
        uit.push_str(&regel);
        uit.push_str("\r\n");
    }
    uit.into_bytes()
}

/// Markdown: tekens met een betekenis krijgen een backslash. Rechte haken blijven staan (zoals in
/// "[BSN]") tenzij ze een verwijzing zouden vormen.
fn md_tekst(tekst: &str) -> String {
    let tekens: Vec<char> = tekst.chars().collect();
    let mut uit = String::with_capacity(tekst.len());
    for (i, teken) in tekens.iter().enumerate() {
        let volgende = tekens.get(i + 1);
        let escape = match teken {
            '\\' | '`' | '*' | '_' | '<' | '>' | '~' | '|' | '&' => true,
            // Een link of verwijzing begint met "](" of "][".
            ']' => matches!(volgende, Some('(' | '[')),
            _ => false,
        };
        if escape {
            uit.push('\\');
        }
        uit.push(*teken);
    }
    // Aan het begin van een alinea zou dit een kop, lijst, citaat of definitie worden.
    let begin: String = uit.chars().take(1).collect();
    if matches!(begin.as_str(), "#" | "+" | "-" | "=" | "!" | "[") {
        return format!("\\{uit}");
    }
    let cijfers = uit.chars().take_while(char::is_ascii_digit).count();
    if cijfers > 0 && uit[cijfers..].starts_with(['.', ')']) {
        return format!("{}\\{}", &uit[..cijfers], &uit[cijfers..]);
    }
    uit
}

/// Markdown (.md): een kop, de gegevens als lijst en per regel een alinea.
pub(super) fn md(document: &Document, notulen: Option<&Notulenblok>, tijden: bool) -> Vec<u8> {
    let mut uit = format!("# {}\n\n", md_tekst(&document.titel()));
    for (sleutel, waarde) in document.gegevens_met(notulen) {
        let _ = writeln!(uit, "- **{sleutel}:** {}", md_tekst(&waarde));
    }
    uit.push('\n');
    if let Some(notulen) = notulen {
        let mut in_lijst = false;
        for regel in &notulen.regels {
            let punt = matches!(regel, Notulenregel::Punt { .. });
            if in_lijst && !punt {
                uit.push('\n');
            }
            in_lijst = punt;
            match regel {
                Notulenregel::Kop(tekst) => {
                    let _ = write!(uit, "## {}\n\n", md_tekst(&schone_tekst(tekst)));
                }
                Notulenregel::Tussenkop(tekst) => {
                    let _ = write!(uit, "### {}\n\n", md_tekst(&schone_tekst(tekst)));
                }
                Notulenregel::Alinea(tekst) => {
                    let _ = write!(uit, "{}\n\n", md_tekst(&schone_tekst(tekst)));
                }
                Notulenregel::Punt { tijd_ms, tekst } => {
                    let tijd = tijd_ms
                        .map(|ms| format!("`{}` ", tijdcode(ms)))
                        .unwrap_or_default();
                    let _ = writeln!(uit, "- {tijd}{}", md_tekst(&schone_tekst(tekst)));
                }
            }
        }
        if in_lijst {
            uit.push('\n');
        }
        let _ = write!(
            uit,
            "## {}\n\n",
            md_tekst(&schone_tekst(&notulen.verslagkop))
        );
    }
    let tekst = document.tekstregels();
    if tekst.is_empty() {
        uit.push_str(&md_tekst(&document.zonder_tekst()));
        uit.push('\n');
    }
    for regel in tekst {
        let mut kop = Vec::new();
        if tijden {
            kop.push(format!("`{}`", tijdcode(regel.start_ms)));
        }
        if let Some(spreker) = &regel.spreker {
            kop.push(format!("**{}:**", md_tekst(spreker)));
        }
        kop.push(md_tekst(&regel.tekst));
        // Een alinea per regel, met een lege regel ertussen.
        let _ = write!(uit, "{}\n\n", kop.join(" "));
    }
    uit.truncate(uit.trim_end().len());
    uit.push('\n');
    uit.into_bytes()
}

/// HTML: tekst met entiteiten, zodat niets als opmaak of script wordt gelezen.
fn html_tekst(tekst: &str) -> String {
    let mut uit = String::with_capacity(tekst.len());
    for teken in tekst.chars() {
        match teken {
            '&' => uit.push_str("&amp;"),
            '<' => uit.push_str("&lt;"),
            '>' => uit.push_str("&gt;"),
            '"' => uit.push_str("&quot;"),
            '\'' => uit.push_str("&#39;"),
            andere => uit.push(andere),
        }
    }
    uit
}

/// De opmaak van de pagina: licht en donker volgens de computer, goed afdrukbaar, geen
/// lettertypen van buiten.
const HTML_OPMAAK: &str = "\
:root{color-scheme:light dark}\
body{max-width:46rem;margin:2rem auto;padding:0 1rem;font:16px/1.6 system-ui,-apple-system,'Segoe UI',sans-serif}\
h1{font-size:1.5rem;line-height:1.25;margin:0 0 1rem}\
h2{font-size:1.2rem;margin:1.5rem 0 .5rem}h3{font-size:1rem;margin:1rem 0 .4rem}\
dl{display:grid;grid-template-columns:max-content 1fr;gap:.15rem 1rem;margin:0 0 1.5rem;font-size:.9rem;opacity:.8}\
dt{font-weight:600}dd{margin:0}\
.regel{margin:0 0 .9rem}\
.tijd{font-variant-numeric:tabular-nums;font-size:.85rem;opacity:.65;margin-right:.5rem}\
.spreker{font-weight:600;margin-right:.4rem}\
@media print{body{margin:0;max-width:none}}";

/// Een zelfstandige webpagina (.html).
pub(super) fn html(document: &Document, notulen: Option<&Notulenblok>, tijden: bool) -> Vec<u8> {
    let titel = html_tekst(&document.titel());
    let mut uit = format!(
        "<!doctype html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n",
        super::Taalcode::van(notulen).taal
    );
    uit.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    // Niets van buiten: de pagina laadt geen script, lettertype, plaatje of stijlblad.
    uit.push_str("<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\">\n");
    uit.push_str("<meta name=\"generator\" content=\"Mondig\">\n");
    let _ = write!(
        uit,
        "<title>{titel}</title>\n<style>{HTML_OPMAAK}</style>\n</head>\n<body>\n"
    );
    let _ = writeln!(uit, "<h1>{titel}</h1>\n<dl>");
    for (sleutel, waarde) in document.gegevens_met(notulen) {
        let _ = writeln!(
            uit,
            "<dt>{}</dt><dd>{}</dd>",
            html_tekst(sleutel),
            html_tekst(&waarde)
        );
    }
    uit.push_str("</dl>\n");
    if let Some(notulen) = notulen {
        let mut in_lijst = false;
        for regel in &notulen.regels {
            let punt = matches!(regel, Notulenregel::Punt { .. });
            if punt && !in_lijst {
                uit.push_str("<ul>\n");
            } else if !punt && in_lijst {
                uit.push_str("</ul>\n");
            }
            in_lijst = punt;
            match regel {
                Notulenregel::Kop(tekst) => {
                    let _ = writeln!(uit, "<h2>{}</h2>", html_tekst(&schone_tekst(tekst)));
                }
                Notulenregel::Tussenkop(tekst) => {
                    let _ = writeln!(uit, "<h3>{}</h3>", html_tekst(&schone_tekst(tekst)));
                }
                Notulenregel::Alinea(tekst) => {
                    let _ = writeln!(uit, "<p>{}</p>", html_tekst(&schone_tekst(tekst)));
                }
                Notulenregel::Punt { tijd_ms, tekst } => {
                    uit.push_str("<li>");
                    if let Some(ms) = tijd_ms {
                        let _ = write!(uit, "<span class=\"tijd\">{}</span>", tijdcode(*ms));
                    }
                    let _ = writeln!(uit, "{}</li>", html_tekst(&schone_tekst(tekst)));
                }
            }
        }
        if in_lijst {
            uit.push_str("</ul>\n");
        }
        let _ = writeln!(
            uit,
            "<h2>{}</h2>",
            html_tekst(&schone_tekst(&notulen.verslagkop))
        );
    }
    let tekst = document.tekstregels();
    if tekst.is_empty() {
        let _ = writeln!(uit, "<p>{}</p>", html_tekst(&document.zonder_tekst()));
    }
    for regel in tekst {
        uit.push_str("<p class=\"regel\">");
        if tijden {
            let _ = write!(
                uit,
                "<span class=\"tijd\">{}</span>",
                tijdcode(regel.start_ms)
            );
        }
        if let Some(spreker) = &regel.spreker {
            let _ = write!(
                uit,
                "<span class=\"spreker\">{}:</span>",
                html_tekst(spreker)
            );
        }
        let _ = writeln!(uit, "{}</p>", html_tekst(&regel.tekst));
    }
    uit.push_str("</body>\n</html>\n");
    uit.into_bytes()
}

/// Eén ondertitel: begin, einde en een of twee regels.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Ondertitel {
    pub(super) start_ms: u64,
    pub(super) eind_ms: u64,
    pub(super) regels: Vec<String>,
    /// Wie er spreekt; bij alle ondertitels van een regel dezelfde.
    pub(super) spreker: Option<String>,
    /// Of dit de eerste ondertitel van de regel van het model is.
    pub(super) eerste: bool,
}

/// Splitst woorden in stukken van hooguit `max` tekens (een woord dat langer is, staat alleen).
fn stukken(woorden: &[&str], max: usize) -> Vec<String> {
    let mut uit: Vec<String> = Vec::new();
    let mut huidig = String::new();
    for woord in woorden {
        let erbij = usize::from(!huidig.is_empty()) + woord.chars().count();
        if !huidig.is_empty() && huidig.chars().count() + erbij > max {
            uit.push(std::mem::take(&mut huidig));
        }
        if !huidig.is_empty() {
            huidig.push(' ');
        }
        huidig.push_str(woord);
    }
    if !huidig.is_empty() {
        uit.push(huidig);
    }
    uit
}

/// Verdeelt een blok over twee regels, zo gelijk mogelijk, als het langer is dan [`REGEL`].
fn twee_regels(blok: &str) -> Vec<String> {
    let lengte = blok.chars().count();
    if lengte <= REGEL {
        return vec![blok.to_string()];
    }
    // De spatie die het dichtst bij het midden ligt.
    let midden = lengte / 2;
    let beste = blok
        .char_indices()
        .filter(|(_, c)| *c == ' ')
        .enumerate()
        .min_by_key(|(_, (byte, _))| blok[..*byte].chars().count().abs_diff(midden))
        .map(|(_, (byte, _))| byte);
    match beste {
        Some(byte) => vec![blok[..byte].to_string(), blok[byte + 1..].to_string()],
        None => vec![blok.to_string()],
    }
}

/// De ondertitels: korte blokken, oplopend in tijd en zonder overlap.
pub(super) fn ondertitels(document: &Document) -> Vec<Ondertitel> {
    let mut uit: Vec<Ondertitel> = Vec::new();
    for regel in document.tekstregels() {
        let (start, eind, zin) = (regel.start_ms, regel.eind_ms, regel.tekst);
        let woorden: Vec<&str> = zin.split(' ').collect();
        let blokken = stukken(&woorden, REGEL * 2);
        let tekens: u64 = blokken.iter().map(|b| b.chars().count() as u64).sum();
        let duur = eind
            .saturating_sub(start)
            .max(MINIMAAL_MS * blokken.len() as u64);
        let mut begin = start;
        let mut geteld = 0_u64;
        for (volgnummer, blok) in blokken.into_iter().enumerate() {
            geteld += blok.chars().count() as u64;
            let einde = start + duur * geteld / tekens.max(1);
            uit.push(Ondertitel {
                start_ms: begin,
                eind_ms: einde,
                regels: twee_regels(&blok),
                spreker: regel.spreker.clone(),
                eerste: volgnummer == 0,
            });
            begin = einde;
        }
    }
    // Oplopend en zonder overlap, ook als het model tijden gaf die elkaar raken.
    let mut vorige_eind = 0;
    for titel in &mut uit {
        titel.start_ms = titel.start_ms.max(vorige_eind);
        titel.eind_ms = titel.eind_ms.max(titel.start_ms + 1);
        vorige_eind = titel.eind_ms;
    }
    uit
}

/// "00:00:07,000" (SRT) of "00:00:07.000" (VTT).
fn ondertiteltijd(ms: u64, scheiding: char) -> String {
    let (uren, rest) = (ms / 3_600_000, ms % 3_600_000);
    let (minuten, rest) = (rest / 60_000, rest % 60_000);
    let (seconden, milli) = (rest / 1000, rest % 1000);
    format!("{uren:02}:{minuten:02}:{seconden:02}{scheiding}{milli:03}")
}

/// SubRip (.srt).
pub(super) fn srt(document: &Document) -> Vec<u8> {
    let mut uit = String::from(BOM);
    for (nummer, titel) in ondertitels(document).into_iter().enumerate() {
        let _ = write!(
            uit,
            "{}\r\n{} --> {}\r\n",
            nummer + 1,
            ondertiteltijd(titel.start_ms, ','),
            ondertiteltijd(titel.eind_ms, ',')
        );
        for (volgnummer, regel) in titel.regels.iter().enumerate() {
            // Het label staat alleen voor de eerste ondertitel van een regel van het model.
            let label = match (&titel.spreker, titel.eerste, volgnummer) {
                (Some(spreker), true, 0) => format!("{spreker}: "),
                _ => String::new(),
            };
            uit.push_str(&format!("{label}{regel}").replace("-->", "->"));
            uit.push_str("\r\n");
        }
        uit.push_str("\r\n");
    }
    uit.into_bytes()
}

/// Tekst voor WebVTT: entiteiten voor `&`, `<` en `>`, en nooit `-->`.
fn vtt_tekst(tekst: &str) -> String {
    tekst
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace("--&gt;", "-&gt;")
}

/// WebVTT (.vtt), met de gegevens in een `NOTE`-blok.
pub(super) fn vtt(document: &Document) -> Vec<u8> {
    let mut uit = String::from("WEBVTT\n\nNOTE\n");
    uit.push_str(&document.titel().replace("-->", "->"));
    uit.push('\n');
    for (sleutel, waarde) in document.gegevens() {
        let _ = writeln!(uit, "{sleutel}: {}", waarde.replace("-->", "->"));
    }
    uit.push('\n');
    for titel in ondertitels(document) {
        let _ = writeln!(
            uit,
            "{} --> {}",
            ondertiteltijd(titel.start_ms, '.'),
            ondertiteltijd(titel.eind_ms, '.')
        );
        for (volgnummer, regel) in titel.regels.iter().enumerate() {
            // De stem geldt voor de hele ondertitel: één keer, vóór de eerste regel.
            if let (Some(spreker), 0) = (&titel.spreker, volgnummer) {
                let _ = write!(uit, "<v {}>", vtt_tekst(spreker));
            }
            uit.push_str(&vtt_tekst(regel));
            uit.push('\n');
        }
        uit.push('\n');
    }
    uit.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::super::Regel;
    use super::super::tests::{document, gesprek_document};
    use super::*;

    fn tekst(bytes: Vec<u8>) -> String {
        String::from_utf8(bytes).expect("utf-8")
    }

    #[test]
    fn txt_met_en_zonder_tijden() {
        let met = tekst(txt(&document(), None, true));
        assert!(
            met.starts_with("\u{feff}Interview wethouder\r\nDatum: 1 november 2026, 14:32\r\n")
        );
        assert!(met.contains("Klant: Z-2026-1043\r\nVerwerking: Op deze computer\r\n\r\n"));
        assert!(met.contains("00:00  Goedemiddag, fijn dat u tijd heeft.\r\n"));
        assert!(met.contains("00:07  Ja, dank u."));
        let zonder = tekst(txt(&document(), None, false));
        assert!(zonder.contains("\r\nGoedemiddag, fijn dat u tijd heeft.\r\n"));
        assert!(!zonder.contains("00:07"));
    }

    #[test]
    fn txt_noemt_een_aanbieder_buiten_deze_computer_en_anders_lokaal() {
        use super::super::tests::blok;
        let extern_tekst = tekst(txt(
            &document(),
            Some(&blok(Some("Mistral (Europees)"))),
            true,
        ));
        assert!(
            extern_tekst.contains(
                "Verwerking: Uitschrift op deze computer, notulen door Mistral (Europees)"
            )
        );
        let lokaal = tekst(txt(&document(), Some(&blok(None)), true));
        assert!(lokaal.contains("Verwerking: Op deze computer"));
        let md_tekst = String::from_utf8(md(
            &document(),
            Some(&blok(Some("Mistral (Europees)"))),
            true,
        ))
        .unwrap();
        assert!(md_tekst.contains("notulen door Mistral (Europees)"));
    }

    #[test]
    fn txt_zonder_tekst_toont_de_melding() {
        let doc = Document {
            regels: Vec::new(),
            melding: Some("Geen geluid gehoord.".to_string()),
            ..document()
        };
        assert!(tekst(txt(&doc, None, true)).ends_with("\r\n\r\nGeen geluid gehoord.\r\n"));
    }

    #[test]
    fn markdown_met_en_zonder_tijden_en_met_tekens_die_een_betekenis_hebben() {
        let met = tekst(md(&document(), None, true));
        assert!(!met.starts_with('\u{feff}'), "Markdown heeft geen BOM");
        assert!(met.starts_with("# Interview wethouder\n\n- **Datum:** 1 november 2026, 14:32\n"));
        assert!(met.contains("- **Klant:** Z-2026-1043\n"));
        assert!(met.contains("`00:00` Goedemiddag, fijn dat u tijd heeft.\n\n"));
        // &, < en > krijgen een backslash zodat ze geen opmaak worden.
        assert!(met.contains(
            "`00:07` Ja, dank u. Het belangrijkste is de markt \\& minder \\<auto's\\>.\n"
        ));
        assert!(met.ends_with(".\n") && !met.ends_with("\n\n"));
        let zonder = tekst(md(&document(), None, false));
        assert!(zonder.contains("\n\nGoedemiddag, fijn dat u tijd heeft.\n\n"));
        assert!(!zonder.contains("`00:07`"));
    }

    #[test]
    fn markdown_zet_de_spreker_vet_voor_de_tekst_en_beschermt_het_begin_van_een_alinea() {
        let doc = Document {
            regels: vec![
                Regel {
                    start_ms: 0,
                    eind_ms: 1_000,
                    tekst: "# geen kop, *geen* _nadruk_ [en](geen link)".to_string(),
                    spreker: Some("Jij".to_string()),
                },
                Regel {
                    start_ms: 1_000,
                    eind_ms: 2_000,
                    tekst: "- geen lijst".to_string(),
                    spreker: None,
                },
                Regel {
                    start_ms: 2_000,
                    eind_ms: 3_000,
                    tekst: "1. geen genummerde lijst".to_string(),
                    spreker: None,
                },
            ],
            ..document()
        };
        let uit = tekst(md(&doc, None, false));
        assert!(
            uit.contains("**Jij:** \\# geen kop, \\*geen\\* \\_nadruk\\_ [en\\](geen link)\n\n")
        );
        assert!(uit.contains("\n\n\\- geen lijst\n\n"));
        assert!(uit.contains("\n\n1\\. geen genummerde lijst\n"));
    }

    #[test]
    fn markdown_zonder_tekst_toont_de_melding() {
        let doc = Document {
            regels: Vec::new(),
            melding: Some("Geen geluid gehoord.".to_string()),
            ..document()
        };
        assert!(tekst(md(&doc, None, true)).ends_with("\n\nGeen geluid gehoord.\n"));
    }

    #[test]
    fn html_is_zelfstandig_en_alles_is_ge_escaped() {
        let doc = Document {
            titel: "<script>alert(1)</script> & \"quotes\"".to_string(),
            klant: Some("<b>zaak</b>".to_string()),
            regels: vec![Regel {
                start_ms: 0,
                eind_ms: 1_000,
                tekst: "<img src=x onerror=alert(1)> & 'dit' \"dat\"".to_string(),
                spreker: Some("<i>Jan</i>".to_string()),
            }],
            ..document()
        };
        let uit = tekst(html(&doc, None, true));
        assert!(uit.starts_with("<!doctype html>\n<html lang=\"nl\">"));
        // Geen enkele tag die we niet zelf zetten: de tekst staat alleen als entiteiten.
        assert!(!uit.contains("<script"), "{uit}");
        assert!(!uit.contains("<img"), "{uit}");
        assert!(!uit.contains("<b>") && !uit.contains("<i>"), "{uit}");
        assert!(uit.contains("&lt;script&gt;alert(1)&lt;/script&gt; &amp; &quot;quotes&quot;"));
        assert!(
            uit.contains("&lt;img src=x onerror=alert(1)&gt; &amp; &#39;dit&#39; &quot;dat&quot;")
        );
        assert!(uit.contains("<span class=\"spreker\">&lt;i&gt;Jan&lt;/i&gt;:</span>"));
        assert!(uit.contains("<span class=\"tijd\">00:00</span>"));
        // Zelfstandig: geen verwijzing naar buiten en een beleid dat dat afdwingt.
        for verboden in [
            "http://", "https://", "src=\"", "href=", "@import", "url(", "<link", "<iframe",
        ] {
            assert!(!uit.contains(verboden), "bevat {verboden}: {uit}");
        }
        assert!(uit.contains(
            "Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\""
        ));
        assert!(uit.contains("<meta charset=\"utf-8\">"));
        assert!(!uit.starts_with('\u{feff}'));
    }

    #[test]
    fn html_met_en_zonder_tijden_en_zonder_tekst() {
        let met = tekst(html(&document(), None, true));
        assert!(met.contains("<p class=\"regel\"><span class=\"tijd\">00:07</span>Ja, dank u."));
        assert!(met.contains("markt &amp; minder &lt;auto&#39;s&gt;."));
        let zonder = tekst(html(&document(), None, false));
        assert!(!zonder.contains("class=\"tijd\""));
        let doc = Document {
            regels: Vec::new(),
            melding: Some("Geen geluid gehoord.".to_string()),
            ..document()
        };
        assert!(tekst(html(&doc, None, true)).contains("<p>Geen geluid gehoord.</p>"));
    }

    #[test]
    fn html_en_markdown_met_sprekers_en_zonder() {
        let met = gesprek_document();
        let uit = tekst(html(&met, None, false));
        assert!(uit.contains("<span class=\"spreker\">"), "{uit}");
        let md_uit = tekst(md(&met, None, false));
        assert!(md_uit.contains(":** "), "{md_uit}");
        let zonder = tekst(html(&document(), None, false));
        assert!(!zonder.contains("class=\"spreker\""));
    }

    #[test]
    fn srt_volgens_de_vorm() {
        let uit = tekst(srt(&document()));
        let verwacht = "\u{feff}1\r\n00:00:00,000 --> 00:00:07,000\r\nGoedemiddag, fijn dat u tijd heeft.\r\n\r\n\
                        2\r\n00:00:07,000 --> 00:00:15,000\r\nJa, dank u. Het belangrijkste\r\nis de markt & minder <auto's>.\r\n\r\n";
        assert_eq!(uit, verwacht);
    }

    #[test]
    fn vtt_volgens_de_specificatie() {
        let uit = tekst(vtt(&document()));
        assert!(
            uit.starts_with("WEBVTT\n\nNOTE\nInterview wethouder\nDatum: 1 november 2026, 14:32\n")
        );
        assert!(uit.contains("Verwerking: Op deze computer\n\n00:00:00.000 --> 00:00:07.000\n"));
        assert!(uit.contains("markt &amp; minder &lt;auto's&gt;.\n"));
        // Elk blok: een tijdregel, tekst, en een lege regel; in de tekst nergens "-->".
        let blokken: Vec<&str> = uit.trim_end().split("\n\n").collect();
        assert_eq!(blokken.len(), 4);
        for blok in &blokken[2..] {
            let mut regels = blok.lines();
            assert!(regels.next().is_some_and(|r| r.contains(" --> ")));
            assert!(regels.all(|r| !r.contains("-->") && !r.is_empty()));
        }
    }

    #[test]
    fn txt_met_spreker_zet_het_label_voor_de_tekst() {
        let zonder = tekst(txt(&gesprek_document(), None, false));
        assert!(zonder.contains("\r\nJij: Goedemiddag, welkom.\r\n"));
        assert!(
            zonder.contains("\r\nWethouder <De Vries> &: Dank u wel & fijn dat u er bent.\r\n")
        );
        // Een spreker die na het schoonmaken leeg is, krijgt geen label.
        assert!(zonder.ends_with("\r\nZonder naam.\r\n"));
        let met = tekst(txt(&gesprek_document(), None, true));
        assert!(met.contains("\r\n01:02  Jij: Goedemiddag, welkom.\r\n"));
        assert!(met.contains("\r\n01:21  Zonder naam.\r\n"));
    }

    #[test]
    fn srt_met_spreker_zet_het_label_alleen_voor_de_eerste_ondertitel() {
        let uit = tekst(srt(&gesprek_document()));
        assert!(uit.starts_with(
            "\u{feff}1\r\n00:01:02,000 --> 00:01:10,000\r\nJij: Goedemiddag, welkom.\r\n\r\n2\r\n"
        ));
        assert!(uit.contains("Wethouder <De Vries> &: Dank u wel & fijn dat u er bent.\r\n"));
        assert!(uit.ends_with("Zonder naam.\r\n\r\n"));
        // Een lang stuk wordt meer ondertitels: het label staat er maar één keer.
        let doc = Document {
            regels: vec![Regel {
                start_ms: 0,
                eind_ms: 40_000,
                tekst: "woord ".repeat(60),
                spreker: Some("Anderen".to_string()),
            }],
            ..document()
        };
        let uit = tekst(srt(&doc));
        assert!(ondertitels(&doc).len() >= 4);
        assert_eq!(uit.matches("Anderen:").count(), 1);
        assert!(uit.contains("\r\nAnderen: woord "));
    }

    #[test]
    fn vtt_met_spreker_gebruikt_een_stem_bij_elke_ondertitel() {
        let uit = tekst(vtt(&gesprek_document()));
        assert!(uit.contains("00:01:02.000 --> 00:01:10.000\n<v Jij>Goedemiddag, welkom.\n\n"));
        // De naam is ontsnapt volgens WebVTT: geen kale &, < of >.
        assert!(uit.contains(
            "<v Wethouder &lt;De Vries&gt; &amp;>Dank u wel &amp; fijn dat u er bent.\n"
        ));
        assert!(uit.contains("\nZonder naam.\n"));
        assert_eq!(uit.matches("<v ").count(), 2);
        // Een lang stuk: elke ondertitel van de regel heeft de stem.
        let doc = Document {
            regels: vec![Regel {
                start_ms: 0,
                eind_ms: 40_000,
                tekst: "woord ".repeat(60),
                spreker: Some("Anderen".to_string()),
            }],
            ..document()
        };
        let uit = tekst(vtt(&doc));
        let aantal = ondertitels(&doc).len();
        assert!(aantal >= 4);
        assert_eq!(uit.matches("<v Anderen>").count(), aantal);
        // Een blok blijft een tijdregel, de tekst en een lege regel.
        for blok in uit.trim_end().split("\n\n").skip(2) {
            assert!(
                blok.lines()
                    .nth(1)
                    .is_some_and(|r| r.starts_with("<v Anderen>"))
            );
        }
    }

    #[test]
    fn lange_stukken_worden_korte_ondertitels() {
        let lang = "woord ".repeat(60);
        let doc = Document {
            regels: vec![Regel {
                start_ms: 10_000,
                eind_ms: 40_000,
                tekst: lang,
                spreker: None,
            }],
            ..document()
        };
        let titels = ondertitels(&doc);
        assert!(titels.len() >= 4);
        assert_eq!(titels[0].start_ms, 10_000);
        assert_eq!(titels.last().map(|t| t.eind_ms), Some(40_000));
        for paar in titels.windows(2) {
            assert_eq!(paar[0].eind_ms, paar[1].start_ms);
        }
        for titel in &titels {
            assert!(titel.regels.len() <= 2);
            assert!(titel.regels.iter().all(|r| r.chars().count() <= REGEL + 6));
        }
    }

    #[test]
    fn tijden_lopen_op_zonder_overlap() {
        let doc = Document {
            regels: vec![
                Regel {
                    start_ms: 5_000,
                    eind_ms: 5_000,
                    tekst: "Zonder eindtijd.".to_string(),
                    spreker: None,
                },
                Regel {
                    start_ms: 4_000,
                    eind_ms: 7_000,
                    tekst: "Begint te vroeg.".to_string(),
                    spreker: None,
                },
            ],
            ..document()
        };
        let titels = ondertitels(&doc);
        assert_eq!((titels[0].start_ms, titels[0].eind_ms), (5_000, 6_000));
        assert_eq!((titels[1].start_ms, titels[1].eind_ms), (6_000, 7_000));
        assert_eq!(ondertiteltijd(3_723_456, ','), "01:02:03,456");
    }
}
