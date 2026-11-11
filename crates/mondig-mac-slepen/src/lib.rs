//! Slepen vanuit Dictafoon, Foto's en Mail op de Mac (ADR 0030).
//!
//! De Finder legt bij het slepen een bestandspad op het sleepbord. Apps die het bestand eerst
//! moeten maken, leggen er een bestandsbelofte op: de ontvanger vraagt het bestand op en krijgt
//! het later op een achtergrondwachtrij. Wry 0.57 leest alleen paden, dus Mondig krijgt bij een
//! belofte een lege lijst. Deze crate haakt in de sleepafhandeling van de webview van wry in:
//!
//! - bij het opzetten registreert hij de belofte-typen
//!   (`NSFilePromiseReceiver.readableDraggedTypes`) erbij, naast wat `WebKit` al had;
//! - `draggingEntered:` meldt een belofte boven het venster, zodat de sleep-markering verschijnt;
//! - `performDragOperation:` haalt beloftes op naar een eigen tijdelijke map, vóór de originele
//!   methode van wry. Per item gaat een belofte voor een pad, zoals Apple aanraadt.
//!
//! Bron: developer.apple.com, "Supporting drag and drop through file promises" en
//! `NSFilePromiseReceiver` (geraadpleegd 9-11-2026).
//!
//! Op andere platforms bevat de crate alleen de gedeelde typen en de zuivere hulpfuncties, zodat
//! de app ze overal kan gebruiken en de tests overal draaien.

use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod mac;

#[cfg(target_os = "macos")]
pub use mac::installeer;

/// Wat de haak aan de app meldt. Komt van de hoofdthread (`BelofteBoven`, `Losgelaten` en de
/// paden van gewone items bij een drop met beloftes) of van de achtergrondwachtrij (een
/// aangekomen of mislukte belofte).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Melding {
    /// Er hangt een belofte boven het hoofdvenster.
    BelofteBoven,
    /// Er is iets losgelaten op het hoofdvenster. Komt altijd vóór de `DragDrop` van Tauri
    /// voor dezelfde drop, omdat Tauri die pas later doorgeeft.
    Losgelaten {
        /// De typenamen op het sleepbord (alleen namen, geen inhoud), voor het logboek.
        typen: Vec<String>,
        /// Hoeveel beloftes de haak ophaalt. Is dit meer dan nul, dan heeft de haak de drop
        /// overgenomen: de paden van gewone items komen dan ook via [`Melding::Bestand`], en de
        /// app negeert de paden die Tauri voor deze drop doorgeeft (anders dubbel).
        beloftes: usize,
        /// Kwam de drop van een andere app (`draggingSource` is nil)? Tekst die binnen Mondig
        /// versleept wordt, komt van het eigen proces en hoort geen melding te geven.
        van_buiten: bool,
    },
    /// Een bestand dat via deze drop binnenkwam: een nagekomen belofte of het pad van een gewoon
    /// item naast beloftes.
    Bestand(PathBuf),
    /// Een belofte kwam niet aan (de andere app gaf een fout, of de map lukte niet).
    Mislukt,
}

/// Waarom de haak niet geplaatst is. Mondig werkt dan zoals zonder haak: paden wel,
/// beloftes niet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fout {
    /// Niet aangeroepen op de hoofdthread.
    GeenHoofdthread,
    /// Geen webview meegegeven.
    GeenWebview,
    /// De haak staat er al.
    AlGeplaatst,
    /// De webview is geen `WryWebView` (wry is veranderd).
    AndereKlasse,
    /// De klasse heeft `draggingEntered:` of `performDragOperation:` niet zelf.
    MethodeOntbreekt,
}

impl Fout {
    /// Een vaste code voor het logboek.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::GeenHoofdthread => "geen_hoofdthread",
            Self::GeenWebview => "geen_webview",
            Self::AlGeplaatst => "al_geplaatst",
            Self::AndereKlasse => "andere_klasse",
            Self::MethodeOntbreekt => "methode_ontbreekt",
        }
    }
}

/// Is dit de klasse van de webview van wry? Wry geeft zijn klasse geen vaste naam, dus objc2
/// maakt er `wry::<module>::WryWebView<versie>` van, bijvoorbeeld
/// `wry::wkwebview::class::wry_web_view::WryWebView0.57.1`. We kijken naar het begin en de
/// laatste schakel, zodat een nieuwe versie van wry de haak niet zomaar uitschakelt.
#[must_use]
pub fn is_wry_klasse(naam: &str) -> bool {
    naam.starts_with("wry::")
        && naam
            .rsplit("::")
            .next()
            .and_then(|laatste| laatste.strip_prefix("WryWebView"))
            // Na de naam alleen het versienummer, zodat `WryWebViewParent` niet meetelt.
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(|c: char| c.is_ascii_digit()))
}

/// De naam van de submap voor één drop: elke drop een eigen map, zodat twee opnames met
/// dezelfde naam elkaar niet raken.
#[must_use]
pub fn dropmap_naam(tijd_ms: u128, volgnummer: u64) -> String {
    format!("drop-{tijd_ms}-{volgnummer}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn de_klasse_van_wry_wordt_herkend() {
        assert!(is_wry_klasse(
            "wry::wkwebview::class::wry_web_view::WryWebView0.57.1"
        ));
        assert!(is_wry_klasse("wry::wkwebview::WryWebView0.60.0"));
        assert!(!is_wry_klasse("WKWebView"));
        assert!(!is_wry_klasse(
            "wry::wkwebview::class::wry_web_view_parent::WryWebViewParent0.57.1"
        ));
        assert!(!is_wry_klasse("iets::WryWebView0.57.1"));
    }

    #[test]
    fn de_foutcodes_passen_in_het_logboek() {
        for fout in [
            Fout::GeenHoofdthread,
            Fout::GeenWebview,
            Fout::AlGeplaatst,
            Fout::AndereKlasse,
            Fout::MethodeOntbreekt,
        ] {
            assert!(
                fout.code()
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_')
            );
        }
    }

    #[test]
    fn elke_drop_krijgt_een_eigen_map() {
        assert_eq!(dropmap_naam(1_700, 3), "drop-1700-3");
        assert_ne!(dropmap_naam(1_700, 3), dropmap_naam(1_700, 4));
    }
}
