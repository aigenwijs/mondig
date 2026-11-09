//! De inhaak zelf: Objective-C via objc2, alleen op de Mac.
//!
//! Aannames over wry 0.57.1 (`src/wkwebview/class/wry_web_view.rs`, `drag_drop.rs`), die
//! [`installeer`] controleert voor hij iets verandert:
//!
//! - de view achter `PlatformWebview::inner()` is een `WryWebView` (zie [`crate::is_wry_klasse`]);
//! - die klasse definieert `draggingEntered:` en `performDragOperation:` zelf, met de
//!   signaturen `(id, SEL, id<NSDraggingInfo>) -> NSDragOperation` en `-> BOOL`.
//!
//! Klopt een van die twee niet, dan verandert er niets en geeft [`installeer`] een [`Fout`].
//! De originele implementaties bewaren we en roepen we altijd aan, zodat wry (en Tauri) alles
//! blijven zien wat ze nu zien. Onze eigen code loopt binnen `catch_unwind`: een paniek mag
//! nooit door Objective-C heen.

#![expect(
    unsafe_code,
    reason = "inhaken in de sleepafhandeling van de webview kan alleen via de Objective-C-runtime"
)]

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, Imp, Method, ProtocolObject, Sel};
use objc2::{ClassType, MainThreadMarker, Message, sel};
use objc2_app_kit::{NSDragOperation, NSDraggingInfo, NSFilePromiseReceiver, NSView};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSOperationQueue, NSString, NSURL};

use crate::{Fout, Melding, dropmap_naam, is_wry_klasse};

/// `draggingEntered:` zoals wry hem definieert.
type Binnenkomen = unsafe extern "C-unwind" fn(
    *mut AnyObject,
    Sel,
    *mut ProtocolObject<dyn NSDraggingInfo>,
) -> NSDragOperation;

/// `performDragOperation:` zoals wry hem definieert.
type Loslaten = unsafe extern "C-unwind" fn(
    *mut AnyObject,
    Sel,
    *mut ProtocolObject<dyn NSDraggingInfo>,
) -> Bool;

/// Alles wat de haak nodig heeft. Eén keer gezet, voor de methodes vervangen worden.
struct Haak {
    /// Het adres van de webview van het hoofdvenster: andere webviews (de pil) blijven zoals ze
    /// waren, ook al delen ze de klasse.
    webview: usize,
    /// De tijdelijke map waar beloftes heen gaan.
    map: PathBuf,
    ontvanger: Box<dyn Fn(Melding) + Send + Sync>,
    binnenkomen: Binnenkomen,
    loslaten: Loslaten,
}

static HAAK: OnceLock<Haak> = OnceLock::new();
static VOLGNUMMER: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// De wachtrij waarop de beloftes binnenkomen. Alleen de hoofdthread maakt hem (de drop
    /// komt daar binnen) en houdt hem zo lang als de app draait.
    static WACHTRIJ: Retained<NSOperationQueue> = NSOperationQueue::new();
}

/// Plaatst de haak op de webview van het hoofdvenster.
///
/// `map` is de tijdelijke map voor beloftes (de app maakt hem bij het starten leeg). `ontvanger`
/// krijgt elke [`Melding`], van de hoofdthread of van een achtergrondwachtrij.
///
/// # Errors
///
/// Een [`Fout`] als de webview niet is wat we verwachten; er is dan niets veranderd.
///
/// # Safety
///
/// `webview` moet de `WKWebView` zijn die `tauri::webview::PlatformWebview::inner()` teruggeeft,
/// en blijven bestaan zolang de app draait (het hoofdvenster wordt alleen verborgen, nooit
/// gesloten).
pub unsafe fn installeer(
    webview: *mut c_void,
    map: PathBuf,
    ontvanger: Box<dyn Fn(Melding) + Send + Sync>,
) -> Result<(), Fout> {
    if MainThreadMarker::new().is_none() {
        return Err(Fout::GeenHoofdthread);
    }
    if webview.is_null() {
        return Err(Fout::GeenWebview);
    }
    if HAAK.get().is_some() {
        return Err(Fout::AlGeplaatst);
    }
    // SAFETY: de aanroeper geeft een levende WKWebView, en dat is een Objective-C-object.
    let object: &AnyObject = unsafe { &*webview.cast::<AnyObject>() };
    let klasse = object.class();
    if !is_wry_klasse(&klasse.name().to_string_lossy()) {
        return Err(Fout::AndereKlasse);
    }
    // Alleen methodes die de klasse zelf heeft: een geërfde methode van WKWebView vervangen zou
    // elke WKWebView in het proces raken.
    let methodes = klasse.instance_methods();
    let eigen =
        |naam: Sel| -> Option<&Method> { methodes.iter().copied().find(|m| m.name() == naam) };
    let (Some(binnen), Some(los)) = (
        eigen(sel!(draggingEntered:)),
        eigen(sel!(performDragOperation:)),
    ) else {
        return Err(Fout::MethodeOntbreekt);
    };

    // SAFETY: de implementaties zijn van wry en hebben de signaturen van `Binnenkomen` en
    // `Loslaten` (zie de moduletekst); objc2 controleerde die toen wry de klasse maakte.
    let (binnenkomen, loslaten) = unsafe {
        (
            std::mem::transmute::<Imp, Binnenkomen>(binnen.implementation()),
            std::mem::transmute::<Imp, Loslaten>(los.implementation()),
        )
    };
    HAAK.set(Haak {
        webview: webview as usize,
        map,
        ontvanger,
        binnenkomen,
        loslaten,
    })
    .map_err(|_| Fout::AlGeplaatst)?;

    // SAFETY: onze functies hebben exact dezelfde signatuur als de originelen, en de haak staat
    // in `HAAK` voor ze aangeroepen kunnen worden. We zitten op de hoofdthread, waar AppKit
    // ook de sleepmethodes aanroept, dus er loopt geen drop tijdens het vervangen.
    unsafe {
        binnen.set_implementation(std::mem::transmute::<Binnenkomen, Imp>(bij_binnenkomen));
        los.set_implementation(std::mem::transmute::<Loslaten, Imp>(bij_loslaten));
    }

    // De belofte-typen erbij, naast wat WebKit zelf registreerde. Zonder deze typen is de webview
    // geen bestemming voor een sleep met alleen een belofte.
    // SAFETY: een WKWebView is een NSView, en we zitten op de hoofdthread.
    let view: &NSView = unsafe { &*webview.cast::<NSView>() };
    let mut typen = view.registeredDraggedTypes().to_vec();
    for soort in NSFilePromiseReceiver::readableDraggedTypes().to_vec() {
        if !typen.iter().any(|bestaand| **bestaand == *soort) {
            typen.push(soort);
        }
    }
    view.registerForDraggedTypes(&NSArray::from_retained_slice(&typen));
    Ok(())
}

/// Vervangt `draggingEntered:`: eerst wry, dan melden als er een belofte boven hangt.
unsafe extern "C-unwind" fn bij_binnenkomen(
    this: *mut AnyObject,
    cmd: Sel,
    info: *mut ProtocolObject<dyn NSDraggingInfo>,
) -> NSDragOperation {
    let Some(haak) = HAAK.get() else {
        return NSDragOperation::None;
    };
    // SAFETY: zelfde object, selector en argument als waarmee AppKit ons aanriep.
    let uitkomst = unsafe { (haak.binnenkomen)(this, cmd, info) };
    if this as usize == haak.webview {
        // SAFETY: AppKit geeft een geldig NSDraggingInfo-object mee voor de duur van de aanroep.
        if let Some(info) = unsafe { info.as_ref() } {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                let bord = info.draggingPasteboard();
                let beloftes = NSFilePromiseReceiver::readableDraggedTypes();
                if bord.availableTypeFromArray(&beloftes).is_some() {
                    (haak.ontvanger)(Melding::BelofteBoven);
                }
            }));
        }
    }
    uitkomst
}

/// Vervangt `performDragOperation:`: eerst de beloftes ophalen, dan wry.
unsafe extern "C-unwind" fn bij_loslaten(
    this: *mut AnyObject,
    cmd: Sel,
    info: *mut ProtocolObject<dyn NSDraggingInfo>,
) -> Bool {
    let Some(haak) = HAAK.get() else {
        return Bool::NO;
    };
    if this as usize == haak.webview {
        // SAFETY: AppKit geeft een geldig NSDraggingInfo-object mee voor de duur van de aanroep.
        if let Some(info) = unsafe { info.as_ref() } {
            let _ = catch_unwind(AssertUnwindSafe(|| haal_beloftes(haak, info)));
        }
    }
    // SAFETY: zelfde object, selector en argument als waarmee AppKit ons aanriep.
    unsafe { (haak.loslaten)(this, cmd, info) }
}

/// Leest het sleepbord: per item een belofte als die er is, anders een bestandspad (zoals Apple
/// aanraadt). Meldt de typen, en haalt de beloftes op. Zonder beloftes doet wry de rest.
fn haal_beloftes(haak: &Haak, info: &ProtocolObject<dyn NSDraggingInfo>) {
    let bord = info.draggingPasteboard();
    let typen: Vec<String> = bord
        .types()
        .map(|typen| typen.to_vec().iter().map(ToString::to_string).collect())
        .unwrap_or_default();

    let klassen = NSArray::from_slice(&[NSFilePromiseReceiver::class(), NSURL::class()]);
    // SAFETY: de klassen ondersteunen allebei NSPasteboardReading; zonder opties.
    let items = unsafe { bord.readObjectsForClasses_options(&klassen, None) };
    let mut beloftes: Vec<Retained<NSFilePromiseReceiver>> = Vec::new();
    let mut paden: Vec<PathBuf> = Vec::new();
    for item in items.map(|items| items.to_vec()).unwrap_or_default() {
        if let Some(belofte) = item.downcast_ref::<NSFilePromiseReceiver>() {
            beloftes.push(belofte.retain());
        } else if let Some(url) = item.downcast_ref::<NSURL>()
            && let Some(pad) = bestandspad(url)
        {
            paden.push(pad);
        }
    }

    // `draggingSource` is nil als de sleep uit een ander proces komt (Apple, NSDraggingInfo).
    let van_buiten = info.draggingSource().is_none();
    (haak.ontvanger)(Melding::Losgelaten {
        typen,
        beloftes: beloftes.len(),
        van_buiten,
    });
    if beloftes.is_empty() {
        return;
    }
    // De haak neemt de drop over: ook de gewone items komen nu van ons (de app negeert de paden
    // die Tauri straks voor deze drop doorgeeft).
    for pad in paden {
        (haak.ontvanger)(Melding::Bestand(pad));
    }

    let Some(doel) = maak_dropmap(&haak.map) else {
        for _ in &beloftes {
            (haak.ontvanger)(Melding::Mislukt);
        }
        return;
    };
    let doel =
        NSURL::fileURLWithPath_isDirectory(&NSString::from_str(&doel.to_string_lossy()), true);
    let opties = NSDictionary::new();
    // Komt op de achtergrondwachtrij binnen, per bestand, als de andere app het geschreven heeft.
    let lezer = RcBlock::new(|url: NonNull<NSURL>, fout: *mut NSError| {
        let Some(haak) = HAAK.get() else {
            return;
        };
        let melding = if fout.is_null() {
            // SAFETY: zonder fout is de url het bestand dat aankwam (geldig tijdens de aanroep).
            bestandspad(unsafe { url.as_ref() }).map_or(Melding::Mislukt, Melding::Bestand)
        } else {
            Melding::Mislukt
        };
        let _ = catch_unwind(AssertUnwindSafe(|| (haak.ontvanger)(melding)));
    });
    WACHTRIJ.with(|wachtrij| {
        for belofte in &beloftes {
            // SAFETY: de opties zijn een leeg woordenboek, de wachtrij is een gewone
            // NSOperationQueue die blijft bestaan, en het blok heeft de signatuur van de API.
            unsafe {
                belofte.receivePromisedFilesAtDestination_options_operationQueue_reader(
                    &doel, &opties, wachtrij, &lezer,
                );
            }
        }
    });
}

/// Het pad van een bestands-url; `None` voor een webadres of een url die niet oplost.
fn bestandspad(url: &NSURL) -> Option<PathBuf> {
    if !url.isFileURL() {
        return None;
    }
    // `filePathURL` lost een verwijzings-url (`file:///.file/id=...`) op naar een pad.
    url.filePathURL()
        .and_then(|url| url.path())
        .map(|pad| PathBuf::from(pad.to_string()))
}

/// Een nieuwe, eigen submap in `map` voor deze drop.
fn maak_dropmap(map: &Path) -> Option<PathBuf> {
    let tijd = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let volgnummer = VOLGNUMMER.fetch_add(1, Ordering::Relaxed);
    let doel = map.join(dropmap_naam(tijd, volgnummer));
    std::fs::create_dir_all(&doel).ok()?;
    Some(doel)
}
