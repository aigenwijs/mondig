//! Het klembord van de Mac (`NSPasteboard`), met de tekst als belofte.
//!
//! De werkwijze van Handy (`src-tauri/src/paste_tx/macos.rs`, MIT): `declareTypes:owner:` zet
//! een belofte op het klembord in plaats van de tekst. Vraagt een programma de tekst op, dan
//! roept `AppKit` `pasteboard:provideDataForType:` aan op de eigenaar: dat is het leesbewijs.
//! Neemt een ander het klembord over, dan volgt `pasteboardChangedOwner:`. Beide komen op de
//! hoofdthread (`AppKit` bedient beloftes vanuit de run loop daar), dus alles wat het klembord
//! aanraakt, gaat via [`run_on_main`] naar de hoofdthread. Wie wacht, wacht op een eigen thread.
//!
//! Bron voor het informele protocol `NSPasteboardOwner`: developer.apple.com, `NSPasteboard`,
//! `declareTypes(_:owner:)` en `pasteboard(_:provideDataForType:)` (geraadpleegd 9-11-2026).

#![expect(
    unsafe_code,
    reason = "een eigen Objective-C-klasse als eigenaar van het klembord kan alleen via objc2"
)]

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dispatch2::run_on_main;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, define_class, msg_send};
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSObject, NSString};

use crate::{
    Afsluiting, Bord, Herstel, Intrekking, Kopie, MAX_BEWAREN_BYTES, Signalen, TEKST_TYPE,
    VERBERG_TYPEN, Voorgrond, is_tekstveld, merkbaar, telt_als_leesbewijs,
};

/// Wat de belofte bij zich draagt.
pub struct Belofte {
    tekst: String,
    signalen: Arc<Signalen>,
}

define_class!(
    // SAFETY:
    // - NSObject stelt geen eisen aan subklassen.
    // - `Plakbelofte` heeft geen `Drop`; de ivars zijn gewone Rust-waarden.
    // - De methodes hebben de signaturen van het informele protocol `NSPasteboardOwner`:
    //   `- (void)pasteboard:(NSPasteboard *)sender provideDataForType:(NSPasteboardType)type`
    //   en `- (void)pasteboardChangedOwner:(NSPasteboard *)sender`.
    #[unsafe(super(NSObject))]
    #[name = "MondigPlakbelofte"]
    #[ivars = Belofte]
    pub struct Plakbelofte;

    impl Plakbelofte {
        #[unsafe(method(pasteboard:provideDataForType:))]
        fn lever(&self, bord: &NSPasteboard, soort: &NSString) {
            // Een paniek mag nooit door Objective-C heen.
            let _ = catch_unwind(AssertUnwindSafe(|| {
                let belofte = self.ivars();
                if belofte.signalen.ingetrokken() {
                    // Mondig typt: een laat leesverzoek krijgt niets, ook geen lezing.
                    return;
                }
                if telt_als_leesbewijs(&soort.to_string()) {
                    belofte.signalen.lees(Instant::now());
                    let _ = bord.setString_forType(&NSString::from_str(&belofte.tekst), soort);
                } else {
                    // Een markering: alleen dat hij er is, telt. Geen inhoud.
                    let _ = bord.setData_forType(Some(&NSData::new()), soort);
                }
            }));
        }

        #[unsafe(method(pasteboardChangedOwner:))]
        fn eigenaar_weg(&self, _bord: &NSPasteboard) {
            let _ = catch_unwind(AssertUnwindSafe(|| self.ivars().signalen.verlies_eigendom()));
        }
    }
);

impl Plakbelofte {
    fn nieuw(tekst: String, signalen: Arc<Signalen>) -> Retained<Self> {
        let dit = Self::alloc().set_ivars(Belofte { tekst, signalen });
        // SAFETY: `init` van NSObject op een net gemaakt object met ivars.
        unsafe { msg_send![super(dit), init] }
    }
}

thread_local! {
    /// De belofte die nu op het klembord staat. `declareTypes:owner:` houdt de eigenaar niet
    /// zelf vast, dus wij doen dat, tot het klembord weer van iemand anders is. Alleen op de
    /// hoofdthread gebruikt (elke toegang loopt via `run_on_main`).
    static BELOFTE: RefCell<Option<Retained<Plakbelofte>>> = const { RefCell::new(None) };
}

/// Het algemene klembord.
fn bord() -> Retained<NSPasteboard> {
    NSPasteboard::generalPasteboard()
}

/// Een item met platte tekst en de markeringen.
fn tekst_item(tekst: &str) -> Retained<NSPasteboardItem> {
    let item = NSPasteboardItem::new();
    let _ = item.setString_forType(&NSString::from_str(tekst), &NSString::from_str(TEKST_TYPE));
    for soort in VERBERG_TYPEN {
        let _ = item.setData_forType(&NSData::new(), &NSString::from_str(soort));
    }
    item
}

/// Maakt het klembord leeg en zet deze items erop. Een lege lijst laat het klembord leeg.
fn schrijf(items: Vec<Retained<NSPasteboardItem>>) -> bool {
    let bord = bord();
    let _ = bord.clearContents();
    if items.is_empty() {
        return true;
    }
    let items: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = items
        .into_iter()
        .map(ProtocolObject::from_retained)
        .collect();
    bord.writeObjects(&NSArray::from_retained_slice(&items))
}

/// De echte uitvoering van [`Bord`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Systeem;

impl Bord for Systeem {
    fn voorgrond(&self) -> Voorgrond {
        crate::rechten::voorgrond()
    }

    fn toegankelijk(&self) -> bool {
        crate::rechten::toegankelijk()
    }

    fn beveiligde_invoer(&self) -> bool {
        crate::invoer::beveiligde_invoer()
    }

    fn modifier_ingedrukt(&self) -> bool {
        crate::invoer::modifier_ingedrukt()
    }

    fn bewaar(&self, budget: Duration) -> Kopie {
        run_on_main(move |_| {
            let begin = Instant::now();
            let mut kopie = Kopie {
                items: Vec::new(),
                volledig: true,
            };
            let mut totaal = 0usize;
            let Some(items) = bord().pasteboardItems() else {
                return kopie;
            };
            for item in &items {
                let mut velden = Vec::new();
                for soort in &item.types() {
                    let naam = soort.to_string();
                    // Buiten het budget of te groot: niet bewaren. Plakken gaat voor.
                    let data = if begin.elapsed() < budget {
                        item.dataForType(&soort)
                    } else {
                        None
                    };
                    match data {
                        Some(data) if totaal + data.len() <= MAX_BEWAREN_BYTES => {
                            totaal += data.len();
                            velden.push((naam, data.to_vec()));
                        }
                        _ => {
                            if merkbaar(&naam) {
                                kopie.volledig = false;
                            }
                        }
                    }
                }
                if !velden.is_empty() {
                    kopie.items.push(velden);
                }
            }
            kopie
        })
    }

    fn publiceer(&self, tekst: &str, signalen: Arc<Signalen>) -> Option<isize> {
        let tekst = tekst.to_string();
        run_on_main(move |_| {
            let belofte = Plakbelofte::nieuw(tekst, signalen);
            let typen: Vec<Retained<NSString>> = std::iter::once(TEKST_TYPE)
                .chain(VERBERG_TYPEN)
                .map(NSString::from_str)
                .collect();
            let typen = NSArray::from_retained_slice(&typen);
            let eigenaar: &AnyObject = &belofte;
            // SAFETY: de eigenaar begrijpt `pasteboard:provideDataForType:` en
            // `pasteboardChangedOwner:` (zie `define_class!` hierboven) en blijft bestaan
            // zolang hij in `BELOFTE` staat.
            let nummer = unsafe { bord().declareTypes_owner(&typen, Some(eigenaar)) };
            // De vorige belofte (als die er nog was) is nu geen eigenaar meer.
            BELOFTE.with_borrow_mut(|b| *b = Some(belofte));
            (nummer > 0).then_some(nummer)
        })
    }

    fn stuur_cmd_v(&self) -> Option<Instant> {
        crate::invoer::stuur_cmd_v()
    }

    fn zet_terug(&self, kopie: &Kopie) -> bool {
        let kopie = kopie.clone();
        run_on_main(move |_| zet_terug_hier(&kopie))
    }

    fn zet_tekst(&self, tekst: &str) -> bool {
        let tekst = tekst.to_string();
        run_on_main(move |_| zet_tekst_hier(&tekst))
    }

    fn herstel(
        &self,
        signalen: &Arc<Signalen>,
        nummer: isize,
        afsluiting: Afsluiting,
        kopie: &Kopie,
        tekst: &str,
    ) -> Herstel {
        let signalen = Arc::clone(signalen);
        let kopie = kopie.clone();
        let tekst = tekst.to_string();
        // Controle en schrijven in één stap op de hoofdthread: daar publiceert ook een volgende
        // plakactie, dus die kan er niet tussen komen.
        run_on_main(move |_| {
            let van_ons = !signalen.eigendom_kwijt() && bord().changeCount() == nummer;
            if !van_ons {
                laat_eigen_belofte_los(&signalen);
                return Herstel::NietVanOns;
            }
            match afsluiting {
                Afsluiting::Terugzetten if zet_terug_hier(&kopie) => Herstel::Teruggezet,
                // Liever het dictaat dan een lege belofte.
                _ => {
                    let _ = zet_tekst_hier(&tekst);
                    Herstel::TekstGezet
                }
            }
        })
    }

    fn herstel_voorgrond(&self) -> bool {
        crate::pil::herstel_voorgrond(false)
    }

    fn op_hoofdthread(&self) -> bool {
        MainThreadMarker::new().is_some()
    }

    fn voorgrond_id(&self) -> Option<i32> {
        crate::rechten::voorgrond_id()
    }

    fn tekstveld(&self) -> Option<bool> {
        crate::focus::focusveld().map(|veld| is_tekstveld(&veld))
    }

    fn trek_in(&self, signalen: &Arc<Signalen>, nummer: isize, sinds: Instant) -> Intrekking {
        let signalen = Arc::clone(signalen);
        // Op de hoofdthread: `pasteboard:provideDataForType:` komt daar ook binnen, dus tussen
        // de controle en het leegmaken kan geen lezing vallen.
        run_on_main(move |_| {
            if signalen
                .leesbewijzen()
                .iter()
                .any(|moment| *moment >= sinds)
            {
                return Intrekking::Gelezen;
            }
            if signalen.eigendom_kwijt() || bord().changeCount() != nummer {
                laat_eigen_belofte_los(&signalen);
                return Intrekking::NietVanOns;
            }
            // Eerst de vlag, dan leegmaken: een leesverzoek dat AppKit nog in de rij had, levert
            // daarna niets. De belofte zelf blijft in `BELOFTE` leven tot de volgende schrijfactie,
            // zodat zo'n laat verzoek nooit een vrijgegeven eigenaar raakt.
            signalen.trek_in();
            let bord = bord();
            let _ = bord.clearContents();
            Intrekking::Ingetrokken {
                nummer: bord.changeCount(),
            }
        })
    }

    fn na_typen(&self, nummer: isize, kopie: &Kopie, rest: Option<&str>) -> Option<bool> {
        let kopie = kopie.clone();
        let rest = rest.map(ToString::to_string);
        // Controle en schrijven in één stap: kopieerde de gebruiker intussen (ook via Kopiëren
        // in Mondig), dan blijft dat staan.
        run_on_main(move |_| {
            if bord().changeCount() != nummer {
                return None;
            }
            Some(match rest {
                Some(rest) => zet_tekst_hier(&rest),
                None => zet_terug_hier(&kopie),
            })
        })
    }

    fn typ_tekst(&self, tekst: &str) -> bool {
        crate::invoer::typ_tekst(tekst)
    }
}

/// Laat de eigen belofte los, niet die van een nieuwere plakactie. Alleen op de hoofdthread.
fn laat_eigen_belofte_los(signalen: &Arc<Signalen>) {
    BELOFTE.with_borrow_mut(|b| {
        if b.as_ref()
            .is_some_and(|belofte| Arc::ptr_eq(&belofte.ivars().signalen, signalen))
        {
            *b = None;
        }
    });
}

/// Zet een bewaard klembord terug. Alleen op de hoofdthread.
fn zet_terug_hier(kopie: &Kopie) -> bool {
    let mut items: Vec<Retained<NSPasteboardItem>> = Vec::new();
    for velden in &kopie.items {
        let item = NSPasteboardItem::new();
        for (naam, data) in velden {
            let _ = item.setData_forType(&NSData::with_bytes(data), &NSString::from_str(naam));
        }
        items.push(item);
    }
    // Het origineel staat al in een klembordgeschiedenis; de teruggezette kopie krijgt daarom
    // de markering "tijdelijk", anders komt hij er dubbel in (zoals op Windows).
    if let Some(eerste) = items.first() {
        let soort = NSString::from_str(VERBERG_TYPEN[1]);
        if eerste.dataForType(&soort).is_none() {
            let _ = eerste.setData_forType(&NSData::new(), &soort);
        }
    }
    let gelukt = schrijf(items);
    BELOFTE.with_borrow_mut(|b| *b = None);
    gelukt
}

/// Zet tekst blijvend op het klembord. Alleen op de hoofdthread.
fn zet_tekst_hier(tekst: &str) -> bool {
    let gelukt = schrijf(vec![tekst_item(tekst)]);
    BELOFTE.with_borrow_mut(|b| *b = None);
    gelukt
}
