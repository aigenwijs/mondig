//! API-sleutels van taalmodellen in de sleutelbos van het besturingssysteem (ADR 0025).
//!
//! De sleutel staat onder de dienst [`DIENST`] met een vaste naam per aanbieder
//! (`taalmodel-mistral`). Mondig bewaart hem nergens anders: geen bestand, geen store, geen
//! logboek. Is er geen sleutelbos, dan kan de gebruiker geen sleutel bewaren; er is bewust geen
//! terugval naar een bestand.
//!
//! Alle code haalt de sleutel op via [`sleutel_voor`]. Komt de sleutel later uit het beleid van
//! een organisatie, dan verandert alleen die functie.
//!
//! De waarde zit in een [`Sleutel`]: dat type toont de waarde nooit (`Debug` zegt "verborgen",
//! er is geen `Display` en geen `Serialize`), en alleen de kern zelf kan de waarde lezen. Bij
//! het opruimen wordt de waarde in het geheugen overschreven (`zeroize`); dat geldt ook voor de
//! tekst uit de sleutelbos.

use std::fmt;
use std::io::Read;

use zeroize::{Zeroize, Zeroizing};

use crate::taak::{Foutmelding, Vervolg};
use crate::taalmodel::Voorinstelling;

/// De dienstnaam in de sleutelbos: de bundle-identifier van Mondig (ADR 0005).
pub const DIENST: &str = "com.aigenwijs.spraak";

/// Zo lang mag een sleutel hooguit zijn. Echte sleutels zijn veel korter; dit houdt geplakte
/// tekst die geen sleutel is buiten de sleutelbos.
const MAX_LENGTE: usize = 512;

/// Een API-sleutel. Toont zijn waarde nooit: geen `Display`, geen `Serialize`, en `Debug` zegt
/// alleen "verborgen". Alleen de kern leest de waarde, om hem mee te sturen naar de aanbieder.
#[derive(Clone, PartialEq, Eq)]
pub struct Sleutel(Zeroizing<String>);

impl Sleutel {
    /// Een sleutel uit tekst die de gebruiker invult. Witruimte aan het begin en eind valt weg;
    /// daarna mag hij alleen zichtbare ASCII-tekens bevatten (zo past hij ook altijd in een
    /// HTTP-header).
    ///
    /// # Errors
    ///
    /// [`SleutelFout::Ongeldig`] als de sleutel leeg of te lang is of een spatie bevat, en
    /// [`SleutelFout::OnzichtbaarTeken`] bij een ander teken, zoals een BOM (U+FEFF) of een
    /// spatie zonder breedte (U+200B) die mee is gekopieerd.
    pub fn nieuw(tekst: &str) -> Result<Self, SleutelFout> {
        let tekst = tekst.trim();
        if tekst.is_empty() || tekst.len() > MAX_LENGTE || tekst.contains([' ', '\t']) {
            return Err(SleutelFout::Ongeldig);
        }
        if !tekst.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(SleutelFout::OnzichtbaarTeken);
        }
        Ok(Self(Zeroizing::new(tekst.to_string())))
    }

    /// Leest een sleutel uit een lezer (bijvoorbeeld stdin), in een buffer die vooraf groot
    /// genoeg is en na afloop wordt gewist. Zo blijven er geen kopieen van een gegroeide buffer
    /// achter in het geheugen.
    ///
    /// # Errors
    ///
    /// [`SleutelFout::Ongeldig`] als er niets te lezen valt of de tekst te lang is, en de
    /// fouten van [`Sleutel::nieuw`].
    pub fn lees(lezer: impl Read) -> Result<Self, SleutelFout> {
        // Ruimte voor de langste sleutel, een regeleinde en wat witruimte; meer lezen we niet.
        let grens = MAX_LENGTE * 2;
        let mut buffer = Zeroizing::new(Vec::with_capacity(grens + 1));
        lezer
            .take(u64::try_from(grens + 1).unwrap_or(u64::MAX))
            .read_to_end(&mut buffer)
            .map_err(|_| SleutelFout::Ongeldig)?;
        if buffer.len() > grens {
            return Err(SleutelFout::Ongeldig);
        }
        let tekst = std::str::from_utf8(&buffer).map_err(|_| SleutelFout::OnzichtbaarTeken)?;
        Self::nieuw(tekst)
    }

    /// Een sleutel zonder controle, alleen voor de tests van de koppeling.
    #[cfg(test)]
    pub(crate) fn ongecontroleerd(tekst: &str) -> Self {
        Self(Zeroizing::new(tekst.to_string()))
    }

    /// De waarde, alleen voor de kern (de Authorization-header naar de aanbieder).
    pub(crate) fn waarde(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for Sleutel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sleutel(verborgen)")
    }
}

/// Wat er mis kan gaan met de sleutelbos. Bevat nooit de sleutel en geen tekst van het
/// besturingssysteem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SleutelFout {
    /// Deze computer heeft geen sleutelbos die Mondig kan gebruiken (bijvoorbeeld Linux zonder
    /// Secret Service).
    #[error(
        "Deze computer heeft geen sleutelbos waarin Mondig de sleutel veilig kan bewaren. Daarom kan Mondig de sleutel niet bewaren."
    )]
    GeenSleutelbos,
    /// De sleutelbos is er wel, maar gaf een fout (bijvoorbeeld omdat hij op slot zit).
    #[error(
        "De sleutelbos van deze computer gaf een fout. Controleer of hij ontgrendeld is en probeer het opnieuw."
    )]
    Sleutelbos,
    /// Deze aanbieder werkt zonder sleutel.
    #[error("Deze aanbieder heeft geen sleutel nodig.")]
    GeenSleutelNodig,
    /// Wat de gebruiker invulde, lijkt geen sleutel.
    #[error("Dit lijkt geen geldige sleutel: hij is leeg, te lang of bevat spaties.")]
    Ongeldig,
    /// De sleutel bevat een teken dat er niet in hoort, meestal onzichtbaar meegekopieerd.
    #[error("De sleutel bevat een onzichtbaar teken. Kopieer hem opnieuw.")]
    OnzichtbaarTeken,
    /// In de sleutelbos staat iets onder de naam van Mondig, maar het is geen geldige sleutel.
    #[error("De bewaarde sleutel is ongeldig. Vul hem opnieuw in.")]
    BewaardOngeldig,
}

impl SleutelFout {
    /// Een vaste code voor het logboek, zonder tekst.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::GeenSleutelbos => "geen_sleutelbos",
            Self::Sleutelbos => "sleutelbos",
            Self::GeenSleutelNodig => "geen_sleutel_nodig",
            Self::Ongeldig => "sleutel_ongeldig",
            Self::OnzichtbaarTeken => "sleutel_onzichtbaar_teken",
            Self::BewaardOngeldig => "bewaarde_sleutel_ongeldig",
        }
    }

    /// De melding in gewone taal met de vervolgactie.
    #[must_use]
    pub fn foutmelding(self) -> Foutmelding {
        let vervolg = match self {
            Self::Sleutelbos => Vervolg::OpnieuwProberen,
            Self::GeenSleutelbos
            | Self::GeenSleutelNodig
            | Self::Ongeldig
            | Self::OnzichtbaarTeken
            | Self::BewaardOngeldig => Vervolg::Geen,
        };
        Foutmelding::nieuw(self.to_string(), vervolg)
    }
}

/// Vertaalt een fout van `keyring` naar een [`SleutelFout`] zonder de tekst van het
/// besturingssysteem over te nemen. Bytes van een onleesbare sleutel worden gewist.
fn vertaal(fout: keyring::Error) -> SleutelFout {
    match fout {
        keyring::Error::BadEncoding(mut bytes) | keyring::Error::BadDataFormat(mut bytes, _) => {
            bytes.zeroize();
            SleutelFout::BewaardOngeldig
        }
        keyring::Error::NoDefaultStore | keyring::Error::NotSupportedByStore(_) => {
            SleutelFout::GeenSleutelbos
        }
        keyring::Error::Invalid(ref parameter, _) if parameter == "platform" => {
            SleutelFout::GeenSleutelbos
        }
        _ => SleutelFout::Sleutelbos,
    }
}

/// De plek in de sleutelbos voor deze aanbieder.
fn plek(voorinstelling: Voorinstelling) -> Result<keyring::Entry, SleutelFout> {
    let naam = voorinstelling
        .sleutelnaam()
        .ok_or(SleutelFout::GeenSleutelNodig)?;
    keyring::Entry::new(DIENST, naam).map_err(vertaal)
}

/// Bewaart de sleutel voor deze aanbieder in de sleutelbos; een oude sleutel wordt vervangen.
///
/// # Errors
///
/// [`SleutelFout`] als de aanbieder geen sleutel kent of de sleutelbos faalt.
pub fn zet(voorinstelling: Voorinstelling, sleutel: &Sleutel) -> Result<(), SleutelFout> {
    plek(voorinstelling)?
        .set_password(sleutel.waarde())
        .map_err(vertaal)
}

/// Staat er iets voor deze aanbieder in de sleutelbos? Ook een ongeldige waarde telt als ja;
/// [`sleutel_voor`] meldt die dan als [`SleutelFout::BewaardOngeldig`].
///
/// Vraagt alleen de kenmerken op (`Entry::get_attributes` van keyring-core 1.0), niet het geheim,
/// zodat de sleutel hiervoor niet in het geheugen van Mondig komt. Op Windows leest het
/// besturingssysteem de vermelding wel in zijn geheel (`CredReadW`); de winkel van keyring wist het
/// geheim daarna en geeft alleen de kenmerken door (windows-native-keyring-store 1.1,
/// `extract_from_credential` met `extract_attributes`).
///
/// # Errors
///
/// [`SleutelFout`] als de aanbieder geen sleutel kent of de sleutelbos faalt.
pub fn bestaat(voorinstelling: Voorinstelling) -> Result<bool, SleutelFout> {
    match plek(voorinstelling)?.inner.get_attributes() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(vertaal(e)),
    }
}

/// Haalt de sleutel voor deze aanbieder uit de sleutelbos. Geeft `true` als er een stond.
/// Mondig verwijderen laat de sleutel staan; daarvoor is deze functie.
///
/// # Errors
///
/// [`SleutelFout`] als de aanbieder geen sleutel kent of de sleutelbos faalt.
pub fn wis(voorinstelling: Voorinstelling) -> Result<bool, SleutelFout> {
    match plek(voorinstelling)?.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(vertaal(e)),
    }
}

/// De enige bron van de sleutel voor een aanbieder: `None` als er (nog) geen sleutel is.
/// Later kan het beleid van een organisatie hier de sleutel leveren (ADR 0025).
///
/// # Errors
///
/// [`SleutelFout`] als de aanbieder geen sleutel kent of de sleutelbos faalt.
pub fn sleutel_voor(voorinstelling: Voorinstelling) -> Result<Option<Sleutel>, SleutelFout> {
    match plek(voorinstelling)?.get_password() {
        // De tekst uit de sleutelbos wordt na het controleren gewist. Een waarde die er buiten
        // Mondig om in kwam en geen geldige sleutel is, krijgt een eigen melding.
        Ok(tekst) => {
            let tekst = Zeroizing::new(tekst);
            Sleutel::nieuw(&tekst)
                .map(Some)
                .map_err(|_| SleutelFout::BewaardOngeldig)
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(vertaal(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn een_sleutel_toont_zijn_waarde_nooit() {
        let sleutel = Sleutel::nieuw("  geheim-123  ").unwrap();
        assert_eq!(sleutel.waarde(), "geheim-123");
        let debug = format!("{sleutel:?}");
        assert_eq!(debug, "Sleutel(verborgen)");
        assert!(!debug.contains("geheim"));
    }

    #[test]
    fn lege_of_rare_tekst_is_geen_sleutel() {
        assert_eq!(Sleutel::nieuw("   "), Err(SleutelFout::Ongeldig));
        assert_eq!(Sleutel::nieuw("twee woorden"), Err(SleutelFout::Ongeldig));
        assert_eq!(
            Sleutel::nieuw("a\u{7}b"),
            Err(SleutelFout::OnzichtbaarTeken)
        );
        assert_eq!(
            Sleutel::nieuw(&"x".repeat(MAX_LENGTE + 1)),
            Err(SleutelFout::Ongeldig)
        );
        assert!(Sleutel::nieuw(&"x".repeat(MAX_LENGTE)).is_ok());
    }

    #[test]
    fn onzichtbare_tekens_geven_een_eigen_fout() {
        for tekst in ["\u{feff}abc123", "abc\u{200b}123", "sleutel\u{e9}"] {
            assert_eq!(Sleutel::nieuw(tekst), Err(SleutelFout::OnzichtbaarTeken));
        }
        assert!(
            SleutelFout::OnzichtbaarTeken
                .to_string()
                .contains("onzichtbaar teken")
        );
    }

    #[test]
    fn lezen_uit_een_pipe_haalt_het_regeleinde_weg() {
        let sleutel = Sleutel::lees(&b"abc-123\r\n"[..]).unwrap();
        assert_eq!(sleutel.waarde(), "abc-123");
        assert_eq!(
            Sleutel::lees(&b"\xef\xbb\xbfabc"[..]),
            Err(SleutelFout::OnzichtbaarTeken)
        );
        let te_lang = vec![b'x'; MAX_LENGTE * 3];
        assert_eq!(Sleutel::lees(&te_lang[..]), Err(SleutelFout::Ongeldig));
        assert_eq!(Sleutel::lees(&b""[..]), Err(SleutelFout::Ongeldig));
    }

    #[test]
    fn onleesbare_bytes_uit_de_sleutelbos_zijn_een_ongeldige_bewaarde_sleutel() {
        assert_eq!(
            vertaal(keyring::Error::BadEncoding(vec![1, 2, 3])),
            SleutelFout::BewaardOngeldig
        );
        assert_eq!(
            vertaal(keyring::Error::NoDefaultStore),
            SleutelFout::GeenSleutelbos
        );
    }

    #[test]
    fn ollama_heeft_geen_plek_in_de_sleutelbos() {
        assert_eq!(
            bestaat(Voorinstelling::OllamaLokaal),
            Err(SleutelFout::GeenSleutelNodig)
        );
        let sleutel = Sleutel::nieuw("abc").unwrap();
        assert_eq!(
            zet(Voorinstelling::OllamaLokaal, &sleutel),
            Err(SleutelFout::GeenSleutelNodig)
        );
    }

    /// Met de echte sleutelbos, onder een eigen testdienst zodat een echte sleutel van Mistral
    /// niet geraakt wordt. Handmatig: `cargo test -p mondig-core --features taalmodel -- --ignored sleutelbos`.
    #[test]
    #[ignore = "raakt de echte sleutelbos van deze computer"]
    fn sleutelbos_zetten_lezen_en_wissen() {
        let plek = keyring::Entry::new("com.aigenwijs.spraak.test", "taalmodel-test").unwrap();
        let _ = plek.delete_credential();
        assert!(matches!(
            plek.inner.get_attributes(),
            Err(keyring::Error::NoEntry)
        ));
        plek.set_password("test-sleutel-1").unwrap();
        assert_eq!(plek.get_password().unwrap(), "test-sleutel-1");
        // Bestaan vaststellen zonder het geheim op te halen.
        assert!(plek.inner.get_attributes().is_ok());
        plek.delete_credential().unwrap();
        assert!(matches!(plek.get_password(), Err(keyring::Error::NoEntry)));
    }
}
