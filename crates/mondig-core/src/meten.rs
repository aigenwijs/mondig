//! Nauwkeurigheid meten: tekst normaliseren en woord- en tekenfouten tellen.
//!
//! De merge-drempel van Mondig (meer dan 1 procentpunt achteruit in woordfouten
//! blokkeert) hangt af van deze regels. Wijzig [`normaliseer`] dus nooit zomaar: een
//! nieuwe regel verschuift alle eerdere metingen en vraagt een nieuwe basislijn.

/// Normaliseert tekst voor het vergelijken van een referentie met een uitkomst.
///
/// De regels, in deze volgorde:
///
/// 1. Alles naar kleine letters (Unicode-bewust, dus `É` wordt `é`).
/// 2. Letters en cijfers blijven. Diakrieten blijven ook (`é`, `ë`, `ï`, `ö`): `café` en
///    `cafe` zijn voor ons verschillende woorden.
/// 3. Een apostrof blijft staan als hij midden in een woord zit (`zo'n`, `auto's`) of
///    een bekend Nederlands woordbegin vormt (`'s`, `'t`, `'n`, `'k` als los woord).
///    Typografische apostroffen (`’`, `ʼ`) tellen als gewone apostrof. Een apostrof als
///    aanhalingsteken (`'hallo'`) verdwijnt.
/// 4. Een punt of komma tussen twee cijfers blijft staan, zodat getallen als `3,5`
///    en `1.000` heel blijven. Getallen worden verder niet omgezet: `10` blijft `10`
///    en wordt niet `tien`.
/// 5. Al het andere is een scheiding: leestekens, aanhalingstekens (ook typografische),
///    gedachtestreepjes, koppeltekens (`e-mail` wordt `e mail`) en haakjes. Elk daarvan
///    telt als een spatie.
/// 6. Witruimte wordt samengevoegd tot enkele spaties, begin en eind worden geschoond.
///
/// Niet gedaan: Unicode-normalisatievorm (NFC/NFD) wordt niet aangepast. Bronnen die
/// `é` als losse `e` plus accent aanleveren, komen dus niet overeen met voorgesamengestelde
/// tekens. Voor de huidige testsets speelt dat niet.
#[must_use]
pub fn normaliseer(tekst: &str) -> String {
    let tekens: Vec<char> = tekst.chars().flat_map(char::to_lowercase).collect();
    let mut uit = String::with_capacity(tekst.len());
    for (i, &c) in tekens.iter().enumerate() {
        let vorige = i.checked_sub(1).map(|j| tekens[j]);
        let volgende = tekens.get(i + 1).copied();
        let behouden = if c.is_alphanumeric() {
            true
        } else if is_apostrof(c) {
            apostrof_blijft(&tekens, i)
        } else if c == '.' || c == ',' {
            vorige.is_some_and(char::is_numeric) && volgende.is_some_and(char::is_numeric)
        } else {
            false
        };
        if behouden {
            uit.push(if is_apostrof(c) { '\'' } else { c });
        } else {
            uit.push(' ');
        }
    }
    uit.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_apostrof(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '\u{02BC}' | '\u{2018}')
}

/// Beslist of de apostrof op plaats `i` bij een woord hoort.
fn apostrof_blijft(tekens: &[char], i: usize) -> bool {
    let vorige = i.checked_sub(1).map(|j| tekens[j]);
    let volgende = tekens.get(i + 1).copied();
    // Midden in een woord: zo'n, auto's, m'n.
    if vorige.is_some_and(char::is_alphanumeric) && volgende.is_some_and(char::is_alphanumeric) {
        return true;
    }
    // Aan het begin van een los woord: alleen de bekende verkortingen 's, 't, 'n, 'k.
    let woordbegin = vorige.is_none_or(|v| !v.is_alphanumeric());
    let derde = tekens.get(i + 2).copied();
    woordbegin
        && matches!(volgende, Some('s' | 't' | 'n' | 'k'))
        && derde.is_none_or(|d| !d.is_alphanumeric())
}

/// Aantal fouten tussen twee rijen, gesplitst in vervangen, weggelaten en ingevoegd
/// (Levenshtein, alle drie de bewerkingen kosten 1).
fn tel_fouten<T: PartialEq>(referentie: &[T], hypothese: &[T]) -> (usize, usize, usize) {
    let n = referentie.len();
    let m = hypothese.len();
    let breedte = m + 1;
    let mut kosten = vec![0_u32; (n + 1) * breedte];
    for (j, kost) in kosten.iter_mut().take(breedte).enumerate() {
        *kost = u32::try_from(j).unwrap_or(u32::MAX);
    }
    for i in 1..=n {
        kosten[i * breedte] = u32::try_from(i).unwrap_or(u32::MAX);
        for j in 1..=m {
            let gelijk = u32::from(referentie[i - 1] != hypothese[j - 1]);
            let diagonaal = kosten[(i - 1) * breedte + (j - 1)] + gelijk;
            let boven = kosten[(i - 1) * breedte + j] + 1;
            let links = kosten[i * breedte + (j - 1)] + 1;
            kosten[i * breedte + j] = diagonaal.min(boven).min(links);
        }
    }
    // Terugzoeken welke bewerkingen de goedkoopste weg vormen. Bij gelijke kosten gaat
    // de voorkeur naar diagonaal, dan weglaten, dan invoegen; het totaal is altijd gelijk.
    let (mut i, mut j) = (n, m);
    let (mut vervangen, mut weggelaten, mut ingevoegd) = (0, 0, 0);
    while i > 0 || j > 0 {
        let nu = kosten[i * breedte + j];
        if i > 0 && j > 0 {
            let gelijk = u32::from(referentie[i - 1] != hypothese[j - 1]);
            if kosten[(i - 1) * breedte + (j - 1)] + gelijk == nu {
                vervangen += usize::from(gelijk == 1);
                i -= 1;
                j -= 1;
                continue;
            }
        }
        if i > 0 && kosten[(i - 1) * breedte + j] + 1 == nu {
            weggelaten += 1;
            i -= 1;
        } else {
            ingevoegd += 1;
            j -= 1;
        }
    }
    (vervangen, weggelaten, ingevoegd)
}

/// Woordfouten van een hypothese ten opzichte van een referentie.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Woordfouten {
    /// Woorden die anders zijn geschreven.
    pub vervangen: usize,
    /// Woorden uit de referentie die ontbreken in de hypothese.
    pub weggelaten: usize,
    /// Extra woorden in de hypothese.
    pub ingevoegd: usize,
    /// Aantal woorden in de referentie.
    pub referentiewoorden: usize,
}

impl Woordfouten {
    /// Totaal aantal fouten (vervangen + weggelaten + ingevoegd).
    #[must_use]
    pub fn fouten(&self) -> usize {
        self.vervangen + self.weggelaten + self.ingevoegd
    }

    /// Word error rate als fractie van het aantal referentiewoorden. Kan boven 1 komen
    /// als er veel woorden zijn bijgevoegd. Bij een lege referentie is dat niet te delen:
    /// dan is het 0 als er ook niets is uitgeschreven en anders 1.
    #[must_use]
    pub fn wer(&self) -> f64 {
        fractie(self.fouten(), self.referentiewoorden)
    }
}

impl std::ops::AddAssign for Woordfouten {
    /// Telt de fouten van een ander bestand erbij. De WER van de som is daarmee
    /// gewogen naar het aantal referentiewoorden.
    fn add_assign(&mut self, andere: Self) {
        self.vervangen += andere.vervangen;
        self.weggelaten += andere.weggelaten;
        self.ingevoegd += andere.ingevoegd;
        self.referentiewoorden += andere.referentiewoorden;
    }
}

/// Tekenfouten (voor de CER) van een hypothese ten opzichte van een referentie.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tekenfouten {
    /// Tekens die anders zijn.
    pub vervangen: usize,
    /// Tekens uit de referentie die ontbreken.
    pub weggelaten: usize,
    /// Extra tekens in de hypothese.
    pub ingevoegd: usize,
    /// Aantal tekens in de referentie (spaties tellen mee).
    pub referentietekens: usize,
}

impl Tekenfouten {
    /// Totaal aantal fouten.
    #[must_use]
    pub fn fouten(&self) -> usize {
        self.vervangen + self.weggelaten + self.ingevoegd
    }

    /// Character error rate als fractie, met dezelfde regels als [`Woordfouten::wer`].
    #[must_use]
    pub fn cer(&self) -> f64 {
        fractie(self.fouten(), self.referentietekens)
    }
}

impl std::ops::AddAssign for Tekenfouten {
    fn add_assign(&mut self, andere: Self) {
        self.vervangen += andere.vervangen;
        self.weggelaten += andere.weggelaten;
        self.ingevoegd += andere.ingevoegd;
        self.referentietekens += andere.referentietekens;
    }
}

#[allow(clippy::cast_precision_loss)]
fn fractie(fouten: usize, totaal: usize) -> f64 {
    if totaal == 0 {
        if fouten == 0 { 0.0 } else { 1.0 }
    } else {
        fouten as f64 / totaal as f64
    }
}

/// Telt de woordfouten tussen referentie en hypothese. Beide worden eerst met
/// [`normaliseer`] gelijkgetrokken en dan op witruimte in woorden gesplitst.
#[must_use]
pub fn woordfouten(referentie: &str, hypothese: &str) -> Woordfouten {
    let r = normaliseer(referentie);
    let h = normaliseer(hypothese);
    let rw: Vec<&str> = r.split(' ').filter(|w| !w.is_empty()).collect();
    let hw: Vec<&str> = h.split(' ').filter(|w| !w.is_empty()).collect();
    let (vervangen, weggelaten, ingevoegd) = tel_fouten(&rw, &hw);
    Woordfouten {
        vervangen,
        weggelaten,
        ingevoegd,
        referentiewoorden: rw.len(),
    }
}

/// Telt de tekenfouten tussen referentie en hypothese, na [`normaliseer`]. Spaties
/// tellen als teken mee.
#[must_use]
pub fn tekenfouten(referentie: &str, hypothese: &str) -> Tekenfouten {
    let r: Vec<char> = normaliseer(referentie).chars().collect();
    let h: Vec<char> = normaliseer(hypothese).chars().collect();
    let (vervangen, weggelaten, ingevoegd) = tel_fouten(&r, &h);
    Tekenfouten {
        vervangen,
        weggelaten,
        ingevoegd,
        referentietekens: r.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normaliseer_kleine_letters_en_leestekens() {
        assert_eq!(normaliseer("Hallo, Wereld!"), "hallo wereld");
        assert_eq!(normaliseer("  Wat?!   Nee...  "), "wat nee");
    }

    #[test]
    fn normaliseer_typografische_tekens_en_streepjes() {
        assert_eq!(
            normaliseer("\u{201C}Hallo\u{201D} \u{2013} zei hij \u{2014} zacht"),
            "hallo zei hij zacht"
        );
        assert_eq!(normaliseer("e-mail"), "e mail");
        assert_eq!(normaliseer("(tussen haakjes)"), "tussen haakjes");
    }

    #[test]
    fn normaliseer_behoudt_apostrof_in_woorden() {
        assert_eq!(normaliseer("Zo'n auto's"), "zo'n auto's");
        assert_eq!(normaliseer("zo\u{2019}n"), "zo'n");
        assert_eq!(
            normaliseer("'s Ochtends en 't huis"),
            "'s ochtends en 't huis"
        );
    }

    #[test]
    fn normaliseer_haalt_aanhalingsteken_apostrof_weg() {
        assert_eq!(
            normaliseer("hij zei 'hallo' en ging"),
            "hij zei hallo en ging"
        );
        assert_eq!(normaliseer("\u{2018}hallo\u{2019}"), "hallo");
    }

    #[test]
    fn normaliseer_behoudt_diakrieten() {
        assert_eq!(normaliseer("Café ÉÉN ëxtra"), "café één ëxtra");
    }

    #[test]
    fn normaliseer_laat_getallen_staan() {
        assert_eq!(
            normaliseer("Het kost 3,5 euro of 1.000."),
            "het kost 3,5 euro of 1.000"
        );
        assert_eq!(normaliseer("10 mensen, 20."), "10 mensen 20");
    }

    #[test]
    fn normaliseer_lege_en_witruimte_invoer() {
        assert_eq!(normaliseer(""), "");
        assert_eq!(normaliseer(" \t\n "), "");
        assert_eq!(normaliseer("...!?"), "");
    }

    #[test]
    fn identiek_is_nul() {
        let f = woordfouten("hoor je den haan kraaien", "Hoor je den haan kraaien.");
        assert_eq!(f.fouten(), 0);
        assert_eq!(f.referentiewoorden, 5);
        assert!(f.wer().abs() < f64::EPSILON);
    }

    #[test]
    fn alles_fout_is_een() {
        let f = woordfouten("een twee drie", "vier vijf zes");
        assert_eq!((f.vervangen, f.weggelaten, f.ingevoegd), (3, 0, 0));
        assert!((f.wer() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn weggelaten_en_ingevoegd_worden_apart_geteld() {
        let w = woordfouten("een twee drie vier", "een drie vier");
        assert_eq!((w.vervangen, w.weggelaten, w.ingevoegd), (0, 1, 0));
        let i = woordfouten("een twee", "een zomaar twee");
        assert_eq!((i.vervangen, i.weggelaten, i.ingevoegd), (0, 0, 1));
        assert!((i.wer() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn invoegingen_kunnen_boven_een_uitkomen() {
        let f = woordfouten("hoi", "hoi dit is veel te veel tekst");
        assert_eq!(f.ingevoegd, 6);
        assert!(f.wer() > 1.0);
    }

    #[test]
    fn lege_referentie_en_hypothese() {
        assert!(woordfouten("", "").wer().abs() < f64::EPSILON);
        let alleen_hyp = woordfouten("", "zomaar tekst");
        assert_eq!(alleen_hyp.ingevoegd, 2);
        assert!((alleen_hyp.wer() - 1.0).abs() < f64::EPSILON);
        let alleen_ref = woordfouten("drie hele woorden", "");
        assert_eq!(alleen_ref.weggelaten, 3);
        assert!((alleen_ref.wer() - 1.0).abs() < f64::EPSILON);
        // Leestekens zijn na normalisatie leeg.
        assert_eq!(woordfouten("...", "!!!").fouten(), 0);
    }

    #[test]
    fn mix_van_bewerkingen() {
        // ref: de kat zat op de mat; hyp: de hond zat de mat nu
        let f = woordfouten("de kat zat op de mat", "de hond zat de mat nu");
        assert_eq!(f.fouten(), 3);
        assert_eq!(f.referentiewoorden, 6);
    }

    #[test]
    fn optellen_weegt_naar_referentiewoorden() {
        let mut som = woordfouten("een twee", "een twee"); // 0 van 2
        som += woordfouten("een twee drie vier", "x x x x"); // 4 van 4
        assert_eq!(som.referentiewoorden, 6);
        assert_eq!(som.fouten(), 4);
        assert!((som.wer() - 4.0 / 6.0).abs() < 1e-12);
    }

    #[test]
    fn tekenfouten_tellen_per_teken() {
        let c = tekenfouten("kat", "kot");
        assert_eq!((c.vervangen, c.weggelaten, c.ingevoegd), (1, 0, 0));
        assert!((c.cer() - 1.0 / 3.0).abs() < 1e-12);
        assert!(tekenfouten("hallo", "Hallo!").cer().abs() < f64::EPSILON);
        assert!(tekenfouten("", "").cer().abs() < f64::EPSILON);
        assert!((tekenfouten("", "x").cer() - 1.0).abs() < f64::EPSILON);
    }
}
