//! Woorden aan sprekers koppelen (4b-D, fase D-1, ADR 0018).
//!
//! Het sprekermodel (Sortformer) zegt per spoor wie wanneer sprak; het spraakmodel geeft de
//! uitingen, met tijden per woord als het model die heeft. [`koppel`] brengt die twee samen.
//! Dat is een pure functie op tijden, zodat de regels zonder werkproces te testen zijn.
//!
//! De regels:
//!
//! - Elk woord krijgt de spreker met de meeste overlap in tijd. Overlapt geen spreker, dan de
//!   dichtstbijzijnde binnen [`MAX_AFSTAND`]; anders geen spreker.
//! - Een woord zonder spreker volgt het woord ervoor in dezelfde uiting (aan het begin: het
//!   woord erna). Zo valt een kort "eh" niet als losse beurt uit een zin.
//! - Opvolgende woorden van dezelfde spreker vormen samen een deel; wisselt de spreker midden
//!   in een uiting, dan wordt de uiting daar gesplitst. In de meting van D-0 gebeurde dat 326
//!   keer in een uur: het is de normale situatie, geen uitzondering.
//! - Zonder bruikbare woordtijden (Canary, of het filter veranderde de tekst) gaat de hele
//!   uiting naar de spreker met de meeste overlap, met dezelfde regel voor geen overlap.
//!
//! [`zonder_nummers_bij_een_spreker`] haalt de nummers weg als er maar één spreker is: dan
//! verandert er voor de gebruiker niets. [`vul_aan_met_dichtstbijzijnde`] geeft delen zonder
//! spreker de spreker van het dichtstbijzijnde deel; dat gebruikt het verslag van een
//! vergadering in de ruimte, waar "geen spreker" geen naam heeft.

use std::time::Duration;

use crate::engine::protocol::{Segment, SprekerSegment, Woord};

/// Overlapt geen spreker met een woord, dan telt de dichtstbijzijnde spreker binnen deze
/// afstand. Verder weg is te onzeker: dan geen spreker.
pub const MAX_AFSTAND: Duration = Duration::from_secs(1);

/// Een (deel van een) uiting met de spreker die erbij hoort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toegewezen {
    /// Het deel van de uiting; de woorden erin horen allemaal bij `spreker`.
    pub segment: Segment,
    /// Het nummer van de spreker, of `None` als er geen spreker in de buurt was.
    pub spreker: Option<u8>,
}

/// Koppelt de uitingen van één spoor aan de sprekers op dat spoor (zie de moduledocumentatie).
/// Beide in dezelfde tijdbasis; de sprekers mogen in elke volgorde staan en elkaar overlappen.
/// De uitkomst staat op volgorde van de uitingen, met per uiting een of meer delen.
#[must_use]
pub fn koppel(segmenten: &[Segment], sprekers: &[SprekerSegment]) -> Vec<Toegewezen> {
    let index = Sprekerindex::nieuw(sprekers);
    segmenten
        .iter()
        .flat_map(|segment| koppel_segment(segment, &index))
        .collect()
}

/// Geen nummers als er maar één spreker (of geen) gevonden is: dan is er niets te
/// onderscheiden en blijft het verslag zoals het zonder sprekerherkenning was.
pub fn zonder_nummers_bij_een_spreker(delen: &mut [Toegewezen]) {
    let mut nummers = delen.iter().filter_map(|d| d.spreker);
    let eerste = nummers.next();
    let meer = nummers.any(|s| Some(s) != eerste);
    if !meer {
        for deel in delen {
            deel.spreker = None;
        }
    }
}

/// Geeft elk deel zonder spreker de spreker van het dichtstbijzijnde deel met een spreker (in
/// tijd, op hetzelfde spoor), hoe ver ook. Heeft geen enkel deel een spreker, dan blijft alles
/// zonder. Bij gelijke afstand wint het deel ervoor.
pub fn vul_aan_met_dichtstbijzijnde(delen: &mut [Toegewezen]) {
    let bekend: Vec<(u64, u64, u8)> = delen
        .iter()
        .filter_map(|d| {
            d.spreker
                .map(|s| (d.segment.start_ms, d.segment.eind_ms, s))
        })
        .collect();
    for deel in delen.iter_mut().filter(|d| d.spreker.is_none()) {
        deel.spreker = bekend
            .iter()
            .min_by_key(|(start, eind, _)| {
                let afstand = afstand(deel.segment.start_ms, deel.segment.eind_ms, *start, *eind);
                // Gelijke afstand: het deel ervoor (eerder begin) wint.
                (afstand, deel.segment.start_ms < *start)
            })
            .map(|(_, _, s)| *s);
    }
}

/// De afstand tussen twee tijdvakken; 0 als ze elkaar raken of overlappen.
fn afstand(a_start: u64, a_eind: u64, b_start: u64, b_eind: u64) -> u64 {
    b_start
        .saturating_sub(a_eind)
        .max(a_start.saturating_sub(b_eind))
}

/// De sprekersegmenten op volgorde van begin, met de langste duur, zodat per woord alleen de
/// segmenten in de buurt bekeken worden.
struct Sprekerindex {
    segmenten: Vec<SprekerSegment>,
    langste_ms: u64,
}

impl Sprekerindex {
    fn nieuw(sprekers: &[SprekerSegment]) -> Self {
        let mut segmenten: Vec<SprekerSegment> = sprekers
            .iter()
            .filter(|s| s.eind_ms > s.start_ms)
            .copied()
            .collect();
        segmenten.sort_by_key(|s| (s.start_ms, s.eind_ms, s.spreker));
        let langste_ms = segmenten
            .iter()
            .map(|s| s.eind_ms - s.start_ms)
            .max()
            .unwrap_or(0);
        Self {
            segmenten,
            langste_ms,
        }
    }

    /// De spreker voor het tijdvak `start..eind` (zie de moduledocumentatie).
    fn spreker_voor(&self, start: u64, eind: u64) -> Option<u8> {
        let marge = u64::try_from(MAX_AFSTAND.as_millis()).unwrap_or(u64::MAX);
        // Alleen segmenten die nog kunnen raken of dichtbij zijn: hun begin ligt na
        // `start - langste - marge` en voor `eind + marge`.
        let vanaf = start.saturating_sub(self.langste_ms.saturating_add(marge));
        let begin = self.segmenten.partition_point(|s| s.start_ms < vanaf);
        let buurt = self.segmenten[begin..]
            .iter()
            .take_while(|s| s.start_ms <= eind.saturating_add(marge));

        // Overlap per spreker opgeteld (een spreker kan in twee segmenten tegelijk staan
        // als het model zijn segmenten zo knipt).
        let mut overlap: Vec<(u8, u64)> = Vec::new();
        let mut dichtst: Option<(u64, u8)> = None;
        for s in buurt {
            let gedeeld = eind.min(s.eind_ms).saturating_sub(start.max(s.start_ms));
            if gedeeld > 0 {
                match overlap.iter_mut().find(|(n, _)| *n == s.spreker) {
                    Some((_, totaal)) => *totaal += gedeeld,
                    None => overlap.push((s.spreker, gedeeld)),
                }
            } else {
                let a = afstand(start, eind, s.start_ms, s.eind_ms);
                if a <= marge && dichtst.is_none_or(|(beste, n)| (a, s.spreker) < (beste, n)) {
                    dichtst = Some((a, s.spreker));
                }
            }
        }
        // Meeste overlap wint; bij gelijke overlap het laagste nummer, zodat de uitkomst vastligt.
        overlap
            .iter()
            .max_by_key(|(n, totaal)| (*totaal, std::cmp::Reverse(*n)))
            .map(|(n, _)| *n)
            .or(dichtst.map(|(_, n)| n))
    }
}

/// De tekst van een uiting verdeeld over haar woorden: per woord de stukken tekst (tussen
/// spaties) die erbij horen. `None` als de woorden niet bij de tekst passen (een ander aantal
/// stukken), want dan zou splitsen tekst verschuiven of laten vallen.
fn tekst_per_woord(segment: &Segment) -> Option<Vec<Vec<&str>>> {
    if segment.woorden.is_empty() {
        return None;
    }
    let mut stukken = segment.tekst.split_whitespace();
    let mut uit = Vec::with_capacity(segment.woorden.len());
    for woord in &segment.woorden {
        let aantal = woord.tekst.split_whitespace().count();
        if aantal == 0 {
            return None;
        }
        let deel: Vec<&str> = stukken.by_ref().take(aantal).collect();
        if deel.len() != aantal {
            return None;
        }
        uit.push(deel);
    }
    stukken.next().is_none().then_some(uit)
}

fn koppel_segment(segment: &Segment, index: &Sprekerindex) -> Vec<Toegewezen> {
    let Some(tekst) = tekst_per_woord(segment) else {
        return vec![Toegewezen {
            segment: segment.clone(),
            spreker: index.spreker_voor(segment.start_ms, segment.eind_ms),
        }];
    };
    let mut per_woord: Vec<Option<u8>> = segment
        .woorden
        .iter()
        .map(|w| index.spreker_voor(w.start_ms, w.eind_ms.max(w.start_ms + 1)))
        .collect();
    // Een woord zonder spreker volgt het woord ervoor, aan het begin het eerste met een spreker.
    let mut vorige = per_woord.iter().flatten().next().copied();
    for spreker in &mut per_woord {
        match spreker {
            Some(s) => vorige = Some(*s),
            None => *spreker = vorige,
        }
    }

    let mut delen: Vec<Toegewezen> = Vec::new();
    let mut huidig: Vec<(usize, &Woord)> = Vec::new();
    let sluit = |delen: &mut Vec<Toegewezen>, groep: &[(usize, &Woord)], spreker| {
        let (Some((eerste_i, eerste)), Some((laatste_i, laatste))) = (groep.first(), groep.last())
        else {
            return;
        };
        let start_ms = if *eerste_i == 0 {
            segment.start_ms
        } else {
            eerste.start_ms
        };
        let eind_ms = if *laatste_i + 1 == segment.woorden.len() {
            segment.eind_ms
        } else {
            laatste.eind_ms
        };
        delen.push(Toegewezen {
            segment: Segment {
                start_ms,
                eind_ms: eind_ms.max(start_ms),
                tekst: groep
                    .iter()
                    .flat_map(|(i, _)| tekst[*i].iter().copied())
                    .collect::<Vec<_>>()
                    .join(" "),
                woorden: groep.iter().map(|(_, w)| (*w).clone()).collect(),
            },
            spreker,
        });
    };
    let mut spreker_groep = None;
    for (i, woord) in segment.woorden.iter().enumerate() {
        if !huidig.is_empty() && per_woord[i] != spreker_groep {
            sluit(&mut delen, &huidig, spreker_groep);
            huidig.clear();
        }
        spreker_groep = per_woord[i];
        huidig.push((i, woord));
    }
    sluit(&mut delen, &huidig, spreker_groep);
    delen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn woord(start_ms: u64, eind_ms: u64, tekst: &str) -> Woord {
        Woord {
            start_ms,
            eind_ms,
            tekst: tekst.into(),
        }
    }

    fn spreker(spreker: u8, start_ms: u64, eind_ms: u64) -> SprekerSegment {
        SprekerSegment {
            spreker,
            start_ms,
            eind_ms,
        }
    }

    /// Een uiting met woorden; de tekst is de woorden achter elkaar.
    fn uiting(start_ms: u64, eind_ms: u64, woorden: &[(u64, u64, &str)]) -> Segment {
        let woorden: Vec<Woord> = woorden.iter().map(|&(s, e, t)| woord(s, e, t)).collect();
        Segment {
            tekst: woorden
                .iter()
                .map(|w| w.tekst.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            woorden,
            ..Segment::nieuw(start_ms, eind_ms, "")
        }
    }

    fn kort(delen: &[Toegewezen]) -> Vec<(Option<u8>, u64, u64, &str)> {
        delen
            .iter()
            .map(|d| {
                (
                    d.spreker,
                    d.segment.start_ms,
                    d.segment.eind_ms,
                    d.segment.tekst.as_str(),
                )
            })
            .collect()
    }

    #[test]
    fn een_wissel_midden_in_een_uiting_splitst_haar_op_de_woorden() {
        let segment = uiting(
            900,
            4100,
            &[
                (1000, 1400, "Dat"),
                (1400, 1800, "klopt."),
                (2100, 2500, "Maar"),
                (2500, 3000, "wanneer"),
                (3000, 4000, "dan?"),
            ],
        );
        let sprekers = [spreker(1, 800, 1900), spreker(2, 2000, 4200)];
        let delen = koppel(&[segment], &sprekers);
        assert_eq!(
            kort(&delen),
            vec![
                (Some(1), 900, 1800, "Dat klopt."),
                (Some(2), 2100, 4100, "Maar wanneer dan?"),
            ]
        );
        assert_eq!(delen[0].segment.woorden.len(), 2);
        assert_eq!(delen[1].segment.woorden.len(), 3);
    }

    #[test]
    fn bij_overlap_wint_de_spreker_met_de_meeste_gedeelde_tijd() {
        // Het woord 1000-2000 deelt 300 ms met spreker 1 en 700 ms met spreker 2.
        let segment = uiting(1000, 2000, &[(1000, 2000, "ja")]);
        let sprekers = [spreker(1, 0, 1300), spreker(2, 1300, 3000)];
        assert_eq!(kort(&koppel(&[segment], &sprekers))[0].0, Some(2));
        // Twee sprekers tegelijk (door elkaar praten): de meeste overlap telt, ook als het
        // ene segment het woord helemaal bevat.
        let segment = uiting(1000, 2000, &[(1000, 2000, "ja")]);
        let sprekers = [spreker(1, 0, 5000), spreker(2, 1000, 2000)];
        // Gelijke overlap: het laagste nummer, zodat de uitkomst vastligt.
        assert_eq!(kort(&koppel(&[segment], &sprekers))[0].0, Some(1));
    }

    #[test]
    fn zonder_woordtijden_gaat_de_hele_uiting_naar_de_meeste_overlap() {
        let segment = Segment::nieuw(1000, 5000, "Een hele zin zonder woordtijden.");
        let sprekers = [spreker(1, 0, 2000), spreker(2, 2000, 6000)];
        let delen = koppel(std::slice::from_ref(&segment), &sprekers);
        assert_eq!(delen.len(), 1);
        assert_eq!(delen[0].spreker, Some(2));
        assert_eq!(delen[0].segment, segment);
    }

    #[test]
    fn woorden_die_niet_bij_de_tekst_passen_tellen_als_geen_woordtijden() {
        // Het filter kortte de tekst in, of het model schreef de tekst anders dan de woorden.
        let mut segment = uiting(0, 3000, &[(0, 1000, "een"), (2000, 3000, "twee")]);
        segment.tekst = "een".into();
        let sprekers = [spreker(1, 0, 1000), spreker(2, 1500, 3000)];
        let delen = koppel(std::slice::from_ref(&segment), &sprekers);
        assert_eq!(delen.len(), 1, "niet splitsen als de woorden niet passen");
        assert_eq!(delen[0].segment.tekst, "een");
    }

    #[test]
    fn zonder_spreker_in_de_buurt_is_er_geen_spreker() {
        // De dichtstbijzijnde spreker ligt 1,5 s weg: te ver.
        let segment = Segment::nieuw(10_000, 11_000, "hallo");
        let sprekers = [spreker(1, 0, 8_500)];
        assert_eq!(koppel(&[segment], &sprekers)[0].spreker, None);
        // Binnen een seconde wel.
        let segment = Segment::nieuw(10_000, 11_000, "hallo");
        let sprekers = [spreker(1, 0, 9_200), spreker(2, 11_900, 13_000)];
        assert_eq!(koppel(&[segment], &sprekers)[0].spreker, Some(1));
        // Geen sprekers: niets.
        let segment = Segment::nieuw(0, 1000, "hallo");
        assert_eq!(koppel(&[segment], &[])[0].spreker, None);
    }

    #[test]
    fn een_woord_zonder_spreker_volgt_zijn_buurwoord() {
        // "eh" valt in een gat van 2 s zonder spreker; het hoort bij de zin eromheen.
        let segment = uiting(
            0,
            6000,
            &[(0, 500, "Ik"), (3000, 3200, "eh"), (5500, 6000, "denk")],
        );
        let sprekers = [spreker(1, 0, 600), spreker(1, 5400, 6000)];
        assert_eq!(
            kort(&koppel(&[segment], &sprekers)),
            vec![(Some(1), 0, 6000, "Ik eh denk")]
        );
        // Aan het begin volgt het woord het eerste woord met een spreker.
        let segment = uiting(0, 6000, &[(0, 200, "Nou"), (4000, 6000, "goed")]);
        let sprekers = [spreker(2, 4000, 6000)];
        assert_eq!(
            kort(&koppel(&[segment], &sprekers)),
            vec![(Some(2), 0, 6000, "Nou goed")]
        );
    }

    #[test]
    fn heen_en_weer_geeft_drie_delen_en_de_tekst_blijft_heel() {
        let segment = uiting(
            0,
            3000,
            &[
                (0, 500, "A,"),
                (500, 1000, "a."),
                (1000, 1500, "B"),
                (1500, 2000, "b."),
                (2000, 3000, "A!"),
            ],
        );
        let sprekers = [
            spreker(1, 0, 1000),
            spreker(3, 1000, 2000),
            spreker(1, 2000, 3000),
        ];
        let delen = koppel(std::slice::from_ref(&segment), &sprekers);
        assert_eq!(
            kort(&delen),
            vec![
                (Some(1), 0, 1000, "A, a."),
                (Some(3), 1000, 2000, "B b."),
                (Some(1), 2000, 3000, "A!"),
            ]
        );
        let samen: Vec<&str> = delen.iter().map(|d| d.segment.tekst.as_str()).collect();
        assert_eq!(samen.join(" "), segment.tekst);
    }

    #[test]
    fn een_spreker_geeft_geen_nummers() {
        let sprekers = [spreker(1, 0, 10_000)];
        let mut delen = koppel(
            &[
                Segment::nieuw(0, 1000, "een"),
                Segment::nieuw(2000, 3000, "twee"),
                Segment::nieuw(20_000, 21_000, "ver weg"),
            ],
            &sprekers,
        );
        assert_eq!(
            delen.iter().map(|d| d.spreker).collect::<Vec<_>>(),
            vec![Some(1), Some(1), None]
        );
        zonder_nummers_bij_een_spreker(&mut delen);
        assert!(delen.iter().all(|d| d.spreker.is_none()));

        // Twee sprekers: de nummers blijven.
        let mut delen = koppel(
            &[
                Segment::nieuw(0, 1000, "een"),
                Segment::nieuw(2000, 3000, "twee"),
            ],
            &[spreker(1, 0, 1000), spreker(2, 2000, 3000)],
        );
        zonder_nummers_bij_een_spreker(&mut delen);
        assert_eq!(
            delen.iter().map(|d| d.spreker).collect::<Vec<_>>(),
            vec![Some(1), Some(2)]
        );
    }

    #[test]
    fn aanvullen_geeft_de_spreker_van_het_dichtstbijzijnde_deel() {
        let deel = |spreker, start_ms, eind_ms| Toegewezen {
            segment: Segment::nieuw(start_ms, eind_ms, "x"),
            spreker,
        };
        let mut delen = vec![
            deel(None, 0, 1000),
            deel(Some(2), 5000, 6000),
            deel(None, 6500, 7000),
            deel(Some(1), 9000, 10_000),
            deel(None, 7500, 8000),
        ];
        vul_aan_met_dichtstbijzijnde(&mut delen);
        assert_eq!(
            delen.iter().map(|d| d.spreker).collect::<Vec<_>>(),
            vec![Some(2), Some(2), Some(2), Some(1), Some(1)]
        );
        // Zonder enige spreker blijft het leeg.
        let mut leeg = vec![deel(None, 0, 1000)];
        vul_aan_met_dichtstbijzijnde(&mut leeg);
        assert_eq!(leeg[0].spreker, None);
    }

    #[test]
    fn koppelen_blijft_snel_bij_een_uur_vergaderen() {
        // 10.000 woorden en 2.000 sprekersegmenten: de index houdt het bij lineair werk.
        let sprekers: Vec<SprekerSegment> = (0..2000u64)
            .map(|i| spreker(u8::try_from(i % 4).unwrap() + 1, i * 1800, i * 1800 + 1700))
            .collect();
        let segmenten: Vec<Segment> = (0..1000u64)
            .map(|i| {
                let woorden: Vec<(u64, u64, &str)> = (0..10u64)
                    .map(|k| (i * 3600 + k * 300, i * 3600 + k * 300 + 250, "woord"))
                    .collect();
                uiting(i * 3600, i * 3600 + 3000, &woorden)
            })
            .collect();
        let begin = std::time::Instant::now();
        let delen = koppel(&segmenten, &sprekers);
        assert!(delen.len() >= 1000);
        assert!(
            begin.elapsed() < Duration::from_secs(2),
            "{:?}",
            begin.elapsed()
        );
    }
}
