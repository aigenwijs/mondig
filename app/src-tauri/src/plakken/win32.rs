//! De Windows-kant van plakken (sprint 3c). Naar het voorbeeld van Handy
//! (`src-tauri/src/paste_tx/windows.rs`, MIT, commit `7bfb2e2`), met de beslissingen uit
//! `plakken.rs`.
//!
//! **Opzet: één vaste klembordthread.** Die thread heeft een onzichtbaar berichtvenster
//! (`HWND_MESSAGE`) en een wachtrij met opdrachten: plak een dictaat, zet tekst blijvend
//! op het klembord, sluit af. Hij handelt ze strikt na elkaar af. Een nieuwe opdracht rondt
//! eerst de lopende plakactie af, op dezelfde thread, dus er lopen nooit twee transacties
//! door elkaar en er is geen afstemming tussen threads nodig. Het venster leeft zo lang
//! als de app, dus een belofte op het klembord blijft altijd beantwoordbaar.
//!
//! - **Delayed rendering.** De tekst gaat als belofte op het klembord
//!   (`SetClipboardData(CF_UNICODETEXT, NULL)`). Vraagt een programma hem op, dan stuurt
//!   Windows `WM_RENDERFORMAT` naar de eigenaar: dat is het leesbewijs.
//! - **Buiten de geschiedenis.** Naast de tekst komen drie geregistreerde formaten die
//!   Windows kent: `ExcludeClipboardContentFromMonitorProcessing` (iets), en
//!   `CanIncludeInClipboardHistory` en `CanUploadToCloudClipboard` (een DWORD 0)
//!   (learn.microsoft.com/windows/win32/dataxchg/clipboard-formats, "Cloud Clipboard and
//!   Clipboard History Formats").
//! - **Terugzetten.** Het oude klembord wordt vooraf per formaat gekopieerd, met een
//!   tijdbudget, en na het leesbewijs teruggezet als het klembord nog van ons is.
//!   `GetClipboardSequenceNumber` telt bij delayed rendering pas op als er gerenderd is
//!   (learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getclipboardsequencenumber),
//!   dus na elke eigen render onthouden we het nieuwe nummer. Daarnaast meldt
//!   `WM_DESTROYCLIPBOARD` dat een ander het klembord leegmaakte.
//! - **Ctrl+V via `SendInput`.** Dat valt onder UIPI: invoer naar een programma op een
//!   hoger integriteitsniveau wordt stil tegengehouden, zonder foutcode
//!   (learn.microsoft.com/windows/win32/api/winuser/nf-winuser-sendinput). Daarom kijken we
//!   vooraf het niveau van het doelprogramma na. Ctrl gaat na 100 ms los via een timer,
//!   zodat de klembordthread intussen leesverzoeken kan beantwoorden.

#![expect(
    unsafe_code,
    reason = "klembord, toetsinvoer en procestokens kunnen alleen via de Win32-API"
)]

use std::cell::{Cell, RefCell};
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_SUCCESS, GlobalFree, HANDLE, HGLOBAL, HINSTANCE, HWND, LPARAM, LRESULT,
    SetLastError, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    CopyEnhMetaFileW, DeleteEnhMetaFile, DeleteObject, HENHMETAFILE, HGDIOBJ,
};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TokenIntegrityLevel,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardFormatNameW, GetClipboardOwner, GetClipboardSequenceNumber, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::{
    CF_BITMAP, CF_DSPBITMAP, CF_DSPENHMETAFILE, CF_DSPMETAFILEPICT, CF_DSPTEXT, CF_ENHMETAFILE,
    CF_METAFILEPICT, CF_OWNERDISPLAY, CF_PALETTE, CF_UNICODETEXT,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LWIN,
    VK_MENU, VK_RWIN, VK_SHIFT, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CopyImage, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, HWND_MESSAGE, IMAGE_BITMAP,
    KillTimer, LR_CREATEDIBSECTION, MSG, PostMessageW, PostQuitMessage, RegisterClassW, SetTimer,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_DESTROYCLIPBOARD, WM_RENDERALLFORMATS,
    WM_RENDERFORMAT, WM_TIMER, WNDCLASSW,
};
use windows::core::{PCWSTR, w};

use super::{
    Afloop, Afronding, Afweging, HERPOGING_STAP, Herpogingstap, Integriteit, PlakUitkomst, Plan,
    Transactiestand, Voorgrond, VorigKlembord, afronding, afweging, herpogingstap, kies_plan,
    merkbaar, met_spatie, nog_van_ons, te_bewaren, vorig_na_terugzetten, wacht_op_losse_modifiers,
};

const KLASSE: PCWSTR = w!("MondigKlembord");
/// Timer: kijken of de lopende plakactie af kan ronden.
const TIMER_WACHT: usize = 1;
const TIMER_WACHT_MS: u32 = 25;
/// Timer: Ctrl loslaten na Ctrl+V.
const TIMER_CTRL: usize = 2;
/// Timer: het oude klembord opnieuw proberen terug te zetten.
const TIMER_HERPOGING: usize = 3;
/// Hoe lang Ctrl ingedrukt blijft na V. Handy kwam op 100 ms omdat sommige systemen een te
/// snel losgelaten combinatie misten (Handy #165, `CHORD_HOLD_MS`).
const CTRL_VASTHOUDEN_MS: u32 = 100;
/// Eigen bericht: er staat een opdracht in de wachtrij.
const WM_MONDIG_OPDRACHT: u32 = WM_APP + 1;
/// Formaten groter dan dit worden niet bewaard (een enorme afbeelding uit een fotoprogramma).
const MAX_FORMAAT_BYTES: usize = 64 * 1024 * 1024;
/// Een ander programma kan het klembord even open hebben; zo vaak proberen we het opnieuw.
const OPENEN_POGINGEN: u32 = 20;
const OPENEN_PAUZE: Duration = Duration::from_millis(10);
/// Bij het terugzetten houden we het langer vol (1,5 s).
const TERUGZET_POGINGEN: u32 = 150;
/// Tijdbudget voor het bewaren van het oude klembord. Wat erover gaat, wordt niet bewaard:
/// plakken gaat voor.
const BEWAAR_BUDGET: Duration = Duration::from_millis(300);
/// Zo lang wacht de klembordthread hooguit op het bewaren (budget plus marge).
const BEWAAR_WACHT: Duration = Duration::from_millis(400);
/// Zo lang wacht een andere thread hooguit op een antwoord van de klembordthread.
const ANTWOORD_WACHT: Duration = Duration::from_secs(3);
/// Het niveau "midden" (`SECURITY_MANDATORY_MEDIUM_RID`): een gewoon programma.
const INTEGRITEIT_MIDDEN: u32 = 0x2000;

/// Een fout bij een Windows-aanroep. De tekst is alleen voor ontwikkelaars, nooit de inhoud
/// van het klembord.
#[derive(Debug)]
pub(crate) struct KlembordFout(
    #[expect(
        dead_code,
        reason = "alleen te zien in de Debug-uitvoer bij het ontwikkelen"
    )]
    &'static str,
);

/// Krijgt de afloop van een plakactie, één keer.
/// Kan voor dezelfde plakactie een tweede keer komen: als het terugzetten pas na de
/// herpogingen definitief mislukt.
type Melder = std::sync::Arc<dyn Fn(Afloop) + Send + Sync>;

// ---------------------------------------------------------------------------
// De drie formaten tegen geschiedenis en cloud
// ---------------------------------------------------------------------------

/// Namen en waarden van de formaten die de tekst buiten de geschiedenis en de cloud houden.
const UITSLUITEN: [(&str, u32); 3] = [
    ("ExcludeClipboardContentFromMonitorProcessing", 1),
    ("CanIncludeInClipboardHistory", 0),
    ("CanUploadToCloudClipboard", 0),
];

/// De nummers van die drie formaten, één keer geregistreerd (0 als dat mislukte).
fn uitsluit_formaten() -> &'static [u32; 3] {
    static NUMMERS: OnceLock<[u32; 3]> = OnceLock::new();
    NUMMERS.get_or_init(|| UITSLUITEN.map(|(naam, _)| formaat_van(naam)))
}

fn breed(tekst: &str) -> Vec<u16> {
    tekst.encode_utf16().chain(std::iter::once(0)).collect()
}

fn u16_als_bytes(tekst: &[u16]) -> Vec<u8> {
    tekst.iter().flat_map(|teken| teken.to_le_bytes()).collect()
}

/// Het nummer van een geregistreerd formaat, of 0.
fn formaat_van(naam: &str) -> u32 {
    let naam = breed(naam);
    // SAFETY: de naam is een geldige UTF-16-tekst met nul.
    unsafe { RegisterClipboardFormatW(PCWSTR(naam.as_ptr())) }
}

// ---------------------------------------------------------------------------
// Het bewaarde klembord
// ---------------------------------------------------------------------------

/// Een formaat van het oude klembord.
pub(super) enum Bewaard {
    /// Gewoon globaal geheugen (tekst, rich text, HTML, DIB-afbeeldingen, bestanden).
    Bytes { formaat: u32, data: Vec<u8> },
    /// Een GDI-bitmap, als kopie (alleen als er geen DIB naast staat).
    Bitmap(GdiHandle),
    /// Een vectorafbeelding (EMF), als kopie.
    Emf(GdiHandle),
}

/// Soort GDI-object, voor het opruimen.
#[derive(Clone, Copy)]
enum GdiSoort {
    Bitmap,
    Emf,
}

/// Een kopie van een GDI-object (`CopyImage`, `CopyEnhMetaFile`). Is het niet aan het
/// klembord overgedragen, dan ruimt `Drop` het op, zodat er niets weglekt.
pub(super) struct GdiHandle {
    handle: usize,
    soort: GdiSoort,
}

impl GdiHandle {
    /// Geeft de handle af; daarna is het klembord eigenaar en ruimt `Drop` niets op.
    fn overdragen(&mut self) {
        self.handle = 0;
    }

    fn als_handle(&self) -> HANDLE {
        HANDLE(self.handle as *mut core::ffi::c_void)
    }
}

impl Drop for GdiHandle {
    fn drop(&mut self) {
        if self.handle == 0 {
            return;
        }
        let wijzer = self.handle as *mut core::ffi::c_void;
        // SAFETY: de handle is een eigen kopie en van niemand anders.
        unsafe {
            match self.soort {
                GdiSoort::Bitmap => {
                    let _ = DeleteObject(HGDIOBJ(wijzer));
                }
                GdiSoort::Emf => {
                    let _ = DeleteEnhMetaFile(Some(HENHMETAFILE(wijzer)));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Klembord lezen en schrijven (elke thread)
// ---------------------------------------------------------------------------

/// Opent het klembord, met nieuwe pogingen. `venster` wordt eigenaar na `EmptyClipboard`.
fn open(venster: Option<HWND>, pogingen: u32) -> Result<(), KlembordFout> {
    for _ in 0..pogingen {
        // SAFETY: geen voorwaarden.
        if unsafe { OpenClipboard(venster) }.is_ok() {
            return Ok(());
        }
        std::thread::sleep(OPENEN_PAUZE);
    }
    Err(KlembordFout("klembord bezet"))
}

fn sluit() {
    // SAFETY: alleen aangeroepen na een gelukte `open`.
    let _ = unsafe { CloseClipboard() };
}

/// Zet bytes in globaal geheugen op het (open) klembord.
fn zet_globaal(formaat: u32, data: &[u8]) -> Result<(), KlembordFout> {
    // SAFETY: het geheugen is groot genoeg; bij succes is het van het klembord, anders
    // geven we het zelf vrij.
    unsafe {
        let geheugen =
            GlobalAlloc(GMEM_MOVEABLE, data.len().max(1)).map_err(|_| KlembordFout("geheugen"))?;
        let wijzer = GlobalLock(geheugen).cast::<u8>();
        if wijzer.is_null() {
            let _ = GlobalFree(Some(geheugen));
            return Err(KlembordFout("geheugen"));
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), wijzer, data.len());
        let _ = GlobalUnlock(geheugen);
        if SetClipboardData(formaat, Some(HANDLE(geheugen.0))).is_err() {
            let _ = GlobalFree(Some(geheugen));
            return Err(KlembordFout("zetten"));
        }
    }
    Ok(())
}

/// Zet de drie formaten tegen geschiedenis en cloud op het (open) klembord.
fn zet_uitsluiten() {
    for (formaat, (_, waarde)) in uitsluit_formaten().iter().zip(UITSLUITEN) {
        if *formaat != 0 {
            let _ = zet_globaal(*formaat, &waarde.to_le_bytes());
        }
    }
}

/// Formaten die geen globaal geheugen zijn en die we niet kopiëren. Bitmap en EMF hebben
/// een eigen route; de oude metafile bevat een handle in zijn geheugen en is dus niet als
/// bytes te kopiëren.
fn niet_te_kopieren(formaat: u32) -> bool {
    [
        CF_METAFILEPICT,
        CF_PALETTE,
        CF_DSPBITMAP,
        CF_DSPENHMETAFILE,
        CF_DSPMETAFILEPICT,
        CF_DSPTEXT,
        CF_OWNERDISPLAY,
    ]
    .iter()
    .any(|f| u32::from(f.0) == formaat)
}

/// De naam van een geregistreerd klembordformaat, of `None` bij een standaardformaat.
fn formaat_naam(formaat: u32) -> Option<String> {
    // Geregistreerde formaten beginnen bij 0xC000; daaronder zijn het standaardformaten.
    if formaat < 0xC000 {
        return None;
    }
    let mut buffer = [0u16; 128];
    // SAFETY: de buffer is groot genoeg voor de opgegeven lengte.
    let lengte = unsafe { GetClipboardFormatNameW(formaat, &mut buffer) };
    let lengte = usize::try_from(lengte).ok().filter(|l| *l > 0)?;
    Some(String::from_utf16_lossy(&buffer[..lengte]))
}

/// Merkt de gebruiker het verlies van dit formaat (zie [`merkbaar`])?
fn merkbaar_verlies(formaat: u32) -> bool {
    merkbaar(formaat, formaat_naam(formaat).as_deref())
}

/// Kopieert de formaten van het klembord die redelijk terug te zetten zijn, binnen
/// `budget`. Geeft ook terug of de kopie volledig is voor de gebruiker: niet als er tekst,
/// opmaak, een afbeelding of bestanden achterblijven omdat het budget op was, een formaat
/// te groot was, of niet te lezen of te kopiëren was. Interne formaten van programma's
/// tellen niet mee (zie [`merkbaar`]).
pub(super) fn bewaar(
    venster: Option<HWND>,
    budget: Duration,
) -> Result<(Vec<Bewaard>, bool), KlembordFout> {
    let begin = Instant::now();
    open(venster, OPENEN_POGINGEN)?;
    let mut formaten = Vec::new();
    let mut bewaard = Vec::new();
    let mut volledig = true;
    // SAFETY: het klembord is open; elke handle wordt alleen gelezen en niet vrijgegeven.
    unsafe {
        let mut formaat = 0u32;
        loop {
            formaat = EnumClipboardFormats(formaat);
            if formaat == 0 {
                break;
            }
            formaten.push(formaat);
        }
        let te_kopieren = te_bewaren(&formaten);
        for (plek, &formaat) in te_kopieren.iter().enumerate() {
            if begin.elapsed() > budget {
                // Wat overblijft, komt niet mee; telt alleen als de gebruiker het merkt.
                if te_kopieren[plek..].iter().any(|f| merkbaar_verlies(*f)) {
                    volledig = false;
                }
                break;
            }
            if niet_te_kopieren(formaat) {
                volledig &= !merkbaar_verlies(formaat);
                continue;
            }
            let Ok(handle) = GetClipboardData(formaat) else {
                volledig &= !merkbaar_verlies(formaat);
                continue;
            };
            if formaat == u32::from(CF_BITMAP.0) {
                if let Ok(kopie) = CopyImage(handle, IMAGE_BITMAP, 0, 0, LR_CREATEDIBSECTION) {
                    bewaard.push(Bewaard::Bitmap(GdiHandle {
                        handle: kopie.0 as usize,
                        soort: GdiSoort::Bitmap,
                    }));
                } else {
                    volledig = false;
                }
                continue;
            }
            if formaat == u32::from(CF_ENHMETAFILE.0) {
                let kopie = CopyEnhMetaFileW(HENHMETAFILE(handle.0), PCWSTR::null());
                if kopie.is_invalid() {
                    volledig &= !merkbaar_verlies(formaat);
                } else {
                    bewaard.push(Bewaard::Emf(GdiHandle {
                        handle: kopie.0 as usize,
                        soort: GdiSoort::Emf,
                    }));
                }
                continue;
            }
            let geheugen = HGLOBAL(handle.0);
            let grootte = GlobalSize(geheugen);
            if grootte > MAX_FORMAAT_BYTES {
                // Logregel zonder inhoud, zoals in sneltoets.rs.
                eprintln!(
                    "plakken: klembordformaat {formaat} van {} MB niet bewaard (te groot)",
                    grootte / 1_000_000
                );
                volledig &= !merkbaar_verlies(formaat);
                continue;
            }
            if grootte == 0 {
                continue;
            }
            let wijzer = GlobalLock(geheugen).cast::<u8>().cast_const();
            if wijzer.is_null() {
                volledig &= !merkbaar_verlies(formaat);
                continue;
            }
            let data = std::slice::from_raw_parts(wijzer, grootte).to_vec();
            let _ = GlobalUnlock(geheugen);
            bewaard.push(Bewaard::Bytes { formaat, data });
        }
    }
    sluit();
    Ok((bewaard, volledig))
}

/// Zet het bewaarde klembord terug. De teruggezette kopie houden we buiten de
/// geschiedenis: het origineel staat daar al, anders kwam het er dubbel in.
///
/// Een fout (klembord niet te openen of leeg te maken) betekent dat er niets veranderd is:
/// `bewaard` blijft dan intact voor een volgende poging. Bij `Ok` geeft de waarde aan of
/// elk formaat terugkwam; de aanroeper gooit `bewaard` daarna weg.
pub(super) fn zet_terug(
    venster: HWND,
    bewaard: &mut [Bewaard],
    pogingen: u32,
) -> Result<bool, KlembordFout> {
    open(Some(venster), pogingen)?;
    // SAFETY: het klembord is open.
    if unsafe { EmptyClipboard() }.is_err() {
        sluit();
        return Err(KlembordFout("leegmaken"));
    }
    let uitsluiten = uitsluit_formaten();
    let mut volledig = true;
    // SAFETY: het klembord is open; een GDI-handle gaat bij succes naar het klembord.
    unsafe {
        for item in bewaard.iter_mut() {
            let gelukt = match item {
                Bewaard::Bytes { formaat, data } => {
                    uitsluiten.contains(formaat)
                        || zet_globaal(*formaat, data).is_ok()
                        || !merkbaar_verlies(*formaat)
                }
                Bewaard::Bitmap(kopie) => {
                    let gezet =
                        SetClipboardData(u32::from(CF_BITMAP.0), Some(kopie.als_handle())).is_ok();
                    if gezet {
                        kopie.overdragen();
                    }
                    gezet
                }
                Bewaard::Emf(kopie) => {
                    let gezet =
                        SetClipboardData(u32::from(CF_ENHMETAFILE.0), Some(kopie.als_handle()))
                            .is_ok();
                    if gezet {
                        kopie.overdragen();
                    }
                    // Een EMF is de vectorversie naast tekst of opmaak: niet merkbaar.
                    gezet || !merkbaar_verlies(u32::from(CF_ENHMETAFILE.0))
                }
            };
            volledig &= gelukt;
        }
    }
    if !bewaard.is_empty() {
        zet_uitsluiten();
    }
    sluit();
    Ok(volledig)
}

/// Het oude klembord, zoals het bewaard is.
struct Kopie {
    items: Vec<Bewaard>,
    /// Kwam alles mee (zie [`bewaar`])?
    volledig: bool,
}

/// Bewaart het klembord op een hulpthread, zodat een hangend programma (een vertraagd
/// formaat dat niet antwoordt) de klembordthread niet ophoudt. `None` als het oude
/// klembord niet bewaard kon worden; een onvolledige kopie komt wel terug.
fn bewaar_met_budget() -> Option<Kopie> {
    let (zender, ontvanger) = mpsc::channel();
    let gestart = std::thread::Builder::new()
        .name("mondig-klembord-bewaren".into())
        .spawn(move || {
            let _ = zender.send(bewaar(None, BEWAAR_BUDGET));
        });
    if gestart.is_err() {
        return None;
    }
    match ontvanger.recv_timeout(BEWAAR_WACHT) {
        Ok(Ok((items, volledig))) => Some(Kopie { items, volledig }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// De klembordthread
// ---------------------------------------------------------------------------

/// Een opdracht voor de klembordthread.
enum Opdracht {
    /// Plak een dictaat. Met `toetsen` op onwaar wordt Ctrl+V niet echt verstuurd (tests).
    Plak {
        tekst: String,
        doel_onzeker: bool,
        toetsen: bool,
        melder: Melder,
    },
    /// Zet tekst blijvend op het klembord (Kopiëren, systeembalk, de terugval).
    ZetBlijvend {
        tekst: String,
        antwoord: Sender<bool>,
    },
    /// De app sluit af.
    Afsluiten { antwoord: Sender<()> },
}

/// Het handvat van de klembordthread.
struct Handvat {
    zender: Sender<Opdracht>,
    venster: isize,
}

static THREAD: OnceLock<Option<Handvat>> = OnceLock::new();

/// De klembordthread, gestart bij het eerste gebruik. `None` als hij niet kon starten.
fn klembordthread() -> Option<&'static Handvat> {
    THREAD
        .get_or_init(|| {
            let (zender, ontvanger) = mpsc::channel();
            let (klaar, wacht) = mpsc::channel();
            std::thread::Builder::new()
                .name("mondig-klembord".into())
                .spawn(move || draai(&klaar, ontvanger))
                .ok()?;
            let venster = wacht.recv().ok()??;
            Some(Handvat { zender, venster })
        })
        .as_ref()
}

/// Zet een opdracht in de wachtrij en maakt de thread wakker. Lukt dat niet (de thread
/// kon niet starten of is al gestopt), dan komt de opdracht terug.
fn stuur(opdracht: Opdracht) -> Result<(), Opdracht> {
    let Some(handvat) = klembordthread() else {
        return Err(opdracht);
    };
    handvat.zender.send(opdracht).map_err(|fout| fout.0)?;
    // SAFETY: een venster van de klembordthread; posten wacht nergens op.
    unsafe {
        let _ = PostMessageW(
            Some(HWND(handvat.venster as *mut core::ffi::c_void)),
            WM_MONDIG_OPDRACHT,
            WPARAM(0),
            LPARAM(0),
        );
    }
    Ok(())
}

/// Wat het berichtvenster tussen berichten door bijhoudt. Alleen kort lenen en nooit
/// vasthouden tijdens een klembordaanroep: die kan een bericht aan het eigen venster
/// sturen (`WM_DESTROYCLIPBOARD`, `WM_RENDERFORMAT`) dat dan meteen binnenkomt.
#[derive(Default)]
struct Signalen {
    /// De tekst van de belofte die nu op het klembord staat, of stond.
    tekst: Option<Vec<u16>>,
    leesbewijzen: Vec<Instant>,
    eigendom_kwijt: bool,
    /// Het volgnummer van het klembord na onze laatste eigen wijziging.
    volgnummer: u32,
}

/// De lopende plakactie.
struct Transactie {
    stand: Transactiestand,
    /// Het oude klembord, of `None` als het niet bewaard kon worden.
    bewaard: Option<Kopie>,
    /// Hebben we het klembord leeggemaakt?
    aangeraakt: bool,
    /// Lukte het publiceren niet?
    publicatie_mislukt: bool,
    melder: Melder,
}

/// Het oude klembord dat nog terug moet: terugzetten lukte niet omdat een ander programma
/// het klembord vasthield. De klembordthread probeert het om de [`HERPOGING_STAP`].
struct Herpoging {
    kopie: Kopie,
    begin: Instant,
    /// De afloop zoals al gemeld; bij definitief mislukken gaat hij opnieuw, met
    /// [`VorigKlembord::Kwijt`].
    afloop: Afloop,
    melder: Melder,
}

thread_local! {
    static ONTVANGER: RefCell<Option<Receiver<Opdracht>>> = const { RefCell::new(None) };
    static SIGNALEN: RefCell<Signalen> = RefCell::new(Signalen::default());
    static LOPEND: RefCell<Option<Transactie>> = const { RefCell::new(None) };
    static HERPOGING: RefCell<Option<Herpoging>> = const { RefCell::new(None) };
    static CTRL_NEER: Cell<bool> = const { Cell::new(false) };
}

fn registreer_klasse(instantie: HINSTANCE) {
    static EEN_KEER: OnceLock<()> = OnceLock::new();
    EEN_KEER.get_or_init(|| {
        let klasse = WNDCLASSW {
            lpfnWndProc: Some(vensterproc),
            hInstance: instantie,
            lpszClassName: KLASSE,
            ..Default::default()
        };
        // SAFETY: de klasse verwijst naar een statische naam en een geldige vensterproc.
        unsafe {
            RegisterClassW(&raw const klasse);
        }
    });
}

/// Maakt een onzichtbaar berichtvenster op de huidige thread.
fn maak_venster() -> Result<HWND, KlembordFout> {
    // SAFETY: gewone Win32-aanroepen; het venster wordt door de aanroeper opgeruimd.
    unsafe {
        let module = GetModuleHandleW(PCWSTR::null()).map_err(|_| KlembordFout("module"))?;
        let instantie = HINSTANCE(module.0);
        registreer_klasse(instantie);
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            KLASSE,
            w!("Mondig klembord"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instantie),
            None,
        )
        .map_err(|_| KlembordFout("venster"))
    }
}

/// De berichtenlus van de klembordthread.
fn draai(klaar: &Sender<Option<isize>>, ontvanger: Receiver<Opdracht>) {
    let Ok(venster) = maak_venster() else {
        let _ = klaar.send(None);
        return;
    };
    ONTVANGER.with_borrow_mut(|o| *o = Some(ontvanger));
    let _ = klaar.send(Some(venster.0 as isize));
    let mut bericht = MSG::default();
    // SAFETY: de standaard berichtenlus; stopt bij `PostQuitMessage` (afsluiten).
    unsafe {
        while GetMessageW(&raw mut bericht, None, 0, 0).as_bool() {
            DispatchMessageW(&raw const bericht);
        }
    }
}

unsafe extern "system" fn vensterproc(
    venster: HWND,
    bericht: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match bericht {
        WM_RENDERFORMAT => {
            let tekst = SIGNALEN.with_borrow_mut(|s| {
                s.leesbewijzen.push(Instant::now());
                s.tekst.clone()
            });
            if wparam.0 == usize::from(CF_UNICODETEXT.0)
                && let Some(tekst) = tekst
            {
                // Windows heeft het klembord voor ons geopend.
                render_tekst(&tekst);
            }
        }
        WM_RENDERALLFORMATS => {
            // Het venster verdwijnt (afsluiten) terwijl een belofte nog openstaat.
            render_belofte(venster);
        }
        WM_DESTROYCLIPBOARD => SIGNALEN.with_borrow_mut(|s| s.eigendom_kwijt = true),
        WM_TIMER if wparam.0 == TIMER_CTRL => laat_ctrl_los(venster),
        WM_TIMER if wparam.0 == TIMER_HERPOGING => probeer_opnieuw(venster),
        WM_TIMER if wparam.0 == TIMER_WACHT => {
            let klaar = LOPEND.with_borrow_mut(|lopend| {
                lopend.as_mut().is_some_and(|t| {
                    werk_stand_bij(&mut t.stand);
                    afweging(&t.stand, Instant::now()) == Afweging::Afronden
                })
            });
            if klaar {
                rond_af(venster);
            }
        }
        WM_MONDIG_OPDRACHT => verwerk_opdrachten(venster),
        // SAFETY: standaardafhandeling met de ontvangen argumenten.
        _ => return unsafe { DefWindowProcW(venster, bericht, wparam, lparam) },
    }
    LRESULT(0)
}

/// Neemt de leesbewijzen en het eigendom over in de stand.
fn werk_stand_bij(stand: &mut Transactiestand) {
    SIGNALEN.with_borrow(|s| {
        stand.leesbewijzen.clone_from(&s.leesbewijzen);
        stand.eigendom_kwijt = s.eigendom_kwijt;
    });
}

/// Zet de tekst gewoon op het (open) klembord en onthoudt het volgnummer.
fn render_tekst(tekst: &[u16]) {
    if zet_globaal(u32::from(CF_UNICODETEXT.0), &u16_als_bytes(tekst)).is_ok() {
        // SAFETY: geen voorwaarden.
        let nummer = unsafe { GetClipboardSequenceNumber() };
        SIGNALEN.with_borrow_mut(|s| s.volgnummer = nummer);
    }
}

/// Zet de tekst van de belofte op het klembord, als dit venster nog eigenaar is. Hier
/// opent Windows het klembord niet voor ons (learn.microsoft.com, "Using the Clipboard").
/// Lukt het openen niet, dan blijft de belofte staan: het venster leeft door en
/// beantwoordt een latere lezing alsnog.
fn render_belofte(venster: HWND) {
    let Some(tekst) = SIGNALEN.with_borrow(|s| s.tekst.clone()) else {
        return;
    };
    if open(Some(venster), TERUGZET_POGINGEN).is_ok() {
        // SAFETY: het klembord is open.
        if unsafe { GetClipboardOwner() }.is_ok_and(|eigenaar| eigenaar == venster) {
            render_tekst(&tekst);
        }
        sluit();
    }
}

/// Handelt alle opdrachten in de wachtrij af, op volgorde. Elke opdracht rondt eerst de
/// lopende plakactie af.
fn verwerk_opdrachten(venster: HWND) {
    loop {
        let Some(opdracht) = ONTVANGER.with_borrow(|o| o.as_ref().and_then(|o| o.try_recv().ok()))
        else {
            return;
        };
        rond_af(venster);
        // Een openstaande herpoging eerst nog één keer, met langer volhouden.
        let overgebleven = rond_herpoging_af(venster);
        match opdracht {
            Opdracht::Plak {
                tekst,
                doel_onzeker,
                toetsen,
                melder,
            } => plak(venster, &tekst, doel_onzeker, toetsen, melder, overgebleven),
            Opdracht::ZetBlijvend { tekst, antwoord } => {
                // De gebruiker kopieert zelf: dat wint van een oud klembord dat niet terug kon.
                if let Some(herpoging) = overgebleven {
                    meld_kwijt(&herpoging);
                }
                let _ = antwoord.send(zet_blijvend_hier(venster, &tekst));
            }
            Opdracht::Afsluiten { antwoord } => {
                if let Some(herpoging) = overgebleven {
                    // Het venster gaat weg; het dictaat blijft staan (zie hieronder).
                    meld_kwijt(&herpoging);
                }
                // Het venster gaat weg; staat er nog een belofte open, dan rendert
                // `WM_RENDERALLFORMATS` die eerst.
                // SAFETY: ons eigen venster, op zijn eigen thread.
                unsafe {
                    let _ = DestroyWindow(venster);
                    PostQuitMessage(0);
                }
                let _ = antwoord.send(());
                return;
            }
        }
    }
}

/// Waarom publiceren mislukte, en of het klembord daarbij al leeggemaakt was.
struct Publicatiefout {
    aangeraakt: bool,
}

/// Zet de tekst als belofte op het klembord, met de formaten tegen de geschiedenis.
fn publiceer(venster: HWND) -> Result<(), Publicatiefout> {
    open(Some(venster), OPENEN_POGINGEN).map_err(|_| Publicatiefout { aangeraakt: false })?;
    // SAFETY: het klembord is open en wordt op elk pad gesloten.
    let gelukt = unsafe {
        if EmptyClipboard().is_err() {
            Err(Publicatiefout { aangeraakt: false })
        } else {
            zet_uitsluiten();
            // Een NULL-handle is delayed rendering. `SetClipboardData` geeft dan bij succes
            // ook NULL terug, wat de crate als fout met `GetLastError` meldt; alleen een
            // echte foutcode telt (zelfde aanpak als Handy).
            SetLastError(ERROR_SUCCESS);
            match SetClipboardData(u32::from(CF_UNICODETEXT.0), None) {
                Err(fout) if fout.code().is_err() => Err(Publicatiefout { aangeraakt: true }),
                _ => Ok(()),
            }
        }
    };
    sluit();
    gelukt
}

/// Een plakactie: bewaren, publiceren, Ctrl+V, en dan wachten op de timer. Stond er nog
/// een oud klembord klaar dat niet terug kon (`overgebleven`), dan is dat het echte oude
/// klembord: het nieuwe dictaat neemt het over in plaats van het vorige dictaat te bewaren.
fn plak(
    venster: HWND,
    tekst: &str,
    doel_onzeker: bool,
    toetsen: bool,
    melder: Melder,
    overgebleven: Option<Herpoging>,
) {
    let bewaard = match overgebleven {
        Some(herpoging) => Some(herpoging.kopie),
        None => bewaar_met_budget(),
    };
    SIGNALEN.with_borrow_mut(|s| s.tekst = Some(breed(tekst)));
    let gepubliceerd = publiceer(venster);
    // Ons eigen leegmaken kan `WM_DESTROYCLIPBOARD` naar ons venster sturen (als het
    // vorige klembord van ons was): de signalen beginnen pas hier.
    // SAFETY: geen voorwaarden.
    let nummer = unsafe { GetClipboardSequenceNumber() };
    SIGNALEN.with_borrow_mut(|s| {
        s.leesbewijzen.clear();
        s.eigendom_kwijt = false;
        s.volgnummer = nummer;
    });
    let mut transactie = Transactie {
        stand: Transactiestand::nieuw(Instant::now(), doel_onzeker),
        bewaard,
        aangeraakt: true,
        publicatie_mislukt: false,
        melder,
    };
    if let Err(fout) = gepubliceerd {
        transactie.aangeraakt = fout.aangeraakt;
        transactie.publicatie_mislukt = true;
        LOPEND.with_borrow_mut(|l| *l = Some(transactie));
        rond_af(venster);
        return;
    }
    transactie.stand.ingevoegd = Some(Instant::now());
    if toetsen {
        if stuur_ctrl_v_neer().is_ok() {
            CTRL_NEER.set(true);
            // SAFETY: het venster is van deze thread.
            unsafe {
                let _ = SetTimer(Some(venster), TIMER_CTRL, CTRL_VASTHOUDEN_MS, None);
            }
        } else {
            transactie.stand.invoeren_mislukt = true;
            CTRL_NEER.set(true);
            laat_ctrl_los(venster);
        }
    }
    LOPEND.with_borrow_mut(|l| *l = Some(transactie));
    // SAFETY: het venster is van deze thread.
    unsafe {
        let _ = SetTimer(Some(venster), TIMER_WACHT, TIMER_WACHT_MS, None);
    }
}

/// Rondt de lopende plakactie af, als die er is: volgens [`afronding`] het oude klembord
/// terug, of anders de tekst erop. Daarna de afloop melden.
fn rond_af(venster: HWND) {
    let Some(mut transactie) = LOPEND.with_borrow_mut(Option::take) else {
        return;
    };
    // SAFETY: het venster is van deze thread.
    unsafe {
        let _ = KillTimer(Some(venster), TIMER_WACHT);
    }
    laat_ctrl_los(venster);
    werk_stand_bij(&mut transactie.stand);
    let eigen = SIGNALEN.with_borrow(|s| s.volgnummer);
    // SAFETY: geen voorwaarden.
    let nu = unsafe { GetClipboardSequenceNumber() };
    let van_ons = nog_van_ons(transactie.stand.eigendom_kwijt, nu, eigen);
    let uitkomst = if transactie.publicatie_mislukt {
        PlakUitkomst::Mislukt
    } else {
        transactie.stand.uitkomst()
    };
    let mut afloop = Afloop {
        uitkomst,
        vorig_klembord: VorigKlembord::Bewaard,
    };
    match afronding(transactie.aangeraakt, transactie.bewaard.is_some(), van_ons) {
        Afronding::Niets => {}
        Afronding::Terugzetten => {
            if let Some(mut kopie) = transactie.bewaard.take() {
                if let Ok(volledig) = zet_terug(venster, &mut kopie.items, OPENEN_POGINGEN) {
                    afloop.vorig_klembord = vorig_na_terugzetten(kopie.volledig, volledig);
                } else {
                    // Een ander programma houdt het klembord vast. De kopie blijft bewaard
                    // en de klembordthread probeert het opnieuw.
                    afloop.vorig_klembord = vorig_na_terugzetten(kopie.volledig, true);
                    start_herpoging(venster, kopie, afloop, transactie.melder.clone());
                }
            }
        }
        Afronding::Renderen => {
            render_belofte(venster);
            afloop.vorig_klembord = VorigKlembord::Kwijt;
        }
    }
    (transactie.melder)(afloop);
}

fn start_herpoging(venster: HWND, kopie: Kopie, afloop: Afloop, melder: Melder) {
    HERPOGING.with_borrow_mut(|h| {
        *h = Some(Herpoging {
            kopie,
            begin: Instant::now(),
            afloop,
            melder,
        });
    });
    // SAFETY: het venster is van deze thread.
    unsafe {
        let _ = SetTimer(
            Some(venster),
            TIMER_HERPOGING,
            u32::try_from(HERPOGING_STAP.as_millis()).unwrap_or(500),
            None,
        );
    }
}

fn stop_herpoging(venster: HWND) -> Option<Herpoging> {
    // SAFETY: het venster is van deze thread.
    unsafe {
        let _ = KillTimer(Some(venster), TIMER_HERPOGING);
    }
    HERPOGING.with_borrow_mut(Option::take)
}

/// Meldt opnieuw, nu met een oud klembord dat niet terug kon.
fn meld_kwijt(herpoging: &Herpoging) {
    (herpoging.melder)(Afloop {
        vorig_klembord: VorigKlembord::Kwijt,
        ..herpoging.afloop
    });
}

/// Is het klembord nog van ons (zie [`nog_van_ons`])?
fn klembord_van_ons() -> bool {
    let (kwijt, eigen) = SIGNALEN.with_borrow(|s| (s.eigendom_kwijt, s.volgnummer));
    // SAFETY: geen voorwaarden.
    let nu = unsafe { GetClipboardSequenceNumber() };
    nog_van_ons(kwijt, nu, eigen)
}

/// Een tik van de herpogingstimer.
fn probeer_opnieuw(venster: HWND) {
    let Some(mut herpoging) = stop_herpoging(venster) else {
        return;
    };
    match herpogingstap(klembord_van_ons(), herpoging.begin.elapsed()) {
        // De gebruiker kopieerde zelf iets: dat wint, de kopie vervalt.
        Herpogingstap::GebruikerWint => {}
        Herpogingstap::LaatsteRedmiddel => {
            render_belofte(venster);
            meld_kwijt(&herpoging);
        }
        Herpogingstap::Opnieuw => {
            if let Ok(volledig) = zet_terug(venster, &mut herpoging.kopie.items, OPENEN_POGINGEN) {
                if !volledig {
                    (herpoging.melder)(Afloop {
                        vorig_klembord: VorigKlembord::Onvolledig,
                        ..herpoging.afloop
                    });
                }
            } else {
                // Nog steeds vast: over een stap opnieuw, met de oorspronkelijke begintijd.
                let Herpoging {
                    kopie,
                    begin,
                    afloop,
                    melder,
                } = herpoging;
                start_herpoging(venster, kopie, afloop, melder);
                HERPOGING.with_borrow_mut(|h| {
                    if let Some(h) = h.as_mut() {
                        h.begin = begin;
                    }
                });
            }
        }
    }
}

/// Voor een nieuwe opdracht: een openstaande herpoging nog één keer, en langer volhouden.
/// Lukt het ook dan niet, dan komt de herpoging terug, zodat de opdracht kan beslissen.
fn rond_herpoging_af(venster: HWND) -> Option<Herpoging> {
    let mut herpoging = stop_herpoging(venster)?;
    if !klembord_van_ons() {
        return None;
    }
    match zet_terug(venster, &mut herpoging.kopie.items, TERUGZET_POGINGEN) {
        Ok(volledig) => {
            if !volledig {
                (herpoging.melder)(Afloop {
                    vorig_klembord: VorigKlembord::Onvolledig,
                    ..herpoging.afloop
                });
            }
            None
        }
        Err(_) => Some(herpoging),
    }
}

/// Zet tekst meteen en blijvend op het klembord, buiten de geschiedenis.
fn zet_blijvend_hier(venster: HWND, tekst: &str) -> bool {
    if open(Some(venster), OPENEN_POGINGEN).is_err() {
        return false;
    }
    // SAFETY: het klembord is open.
    let gelukt = if unsafe { EmptyClipboard() }.is_ok() {
        zet_uitsluiten();
        zet_globaal(u32::from(CF_UNICODETEXT.0), &u16_als_bytes(&breed(tekst))).is_ok()
    } else {
        false
    };
    sluit();
    // Geen belofte meer van ons: niets meer te renderen.
    SIGNALEN.with_borrow_mut(|s| s.tekst = None);
    gelukt
}

// ---------------------------------------------------------------------------
// De ingang voor de rest van de app
// ---------------------------------------------------------------------------

/// Zet tekst meteen en blijvend op het klembord, buiten de geschiedenis. Een lopende
/// plakactie wordt eerst afgerond.
pub(crate) fn zet_blijvend(tekst: &str) -> bool {
    let (antwoord, wacht) = mpsc::channel();
    stuur(Opdracht::ZetBlijvend {
        tekst: tekst.to_string(),
        antwoord,
    })
    .is_ok()
        && wacht.recv_timeout(ANTWOORD_WACHT).unwrap_or(false)
}

/// Bij afsluiten: de lopende plakactie afronden en de klembordthread stoppen. Start de
/// thread niet als hij er nog niet is.
pub(crate) fn afsluiten() {
    if THREAD.get().is_none() {
        return;
    }
    let (antwoord, wacht) = mpsc::channel();
    if stuur(Opdracht::Afsluiten { antwoord }).is_ok() {
        let _ = wacht.recv_timeout(ANTWOORD_WACHT);
    }
}

/// Zie [`super::na_dictaat`]. `meld` krijgt altijd een afloop (bij het eigen venster
/// "in Mondig"), en hooguit een tweede als het terugzetten later mislukt.
pub(super) fn na_dictaat(
    tekst: &str,
    alleen_klembord: bool,
    meld: impl Fn(Afloop) + Send + Sync + 'static,
) {
    let tekst = met_spatie(tekst);
    if tekst.is_empty() {
        meld(PlakUitkomst::InMondig.into());
        return;
    }
    let eigen = eigen_integriteit();
    let mut plan = kies_plan(alleen_klembord, voorgrond(), eigen);
    if matches!(plan, Plan::Plakken { .. }) {
        // Eerst wachten tot de toetsen los zijn; daarna opnieuw kijken, want intussen kan
        // de focus verschoven zijn.
        if !wacht_op_losse_modifiers(modifier_ingedrukt) {
            meld(PlakUitkomst::ToetsenIngedrukt.into());
            return;
        }
        plan = kies_plan(false, voorgrond(), eigen);
    }
    match plan {
        // Mondig of de pil staat vooraan: niet plakken, maar wel een afloop, anders blijft
        // de pil op "Bezig met uitschrijven" staan.
        Plan::Overslaan => meld(PlakUitkomst::InMondig.into()),
        Plan::Niet(uitkomst) => meld(uitkomst.into()),
        Plan::AlleenKlembord => meld(
            if zet_blijvend(&tekst) {
                PlakUitkomst::OpKlembord
            } else {
                PlakUitkomst::Mislukt
            }
            .into(),
        ),
        Plan::Plakken { doel_onzeker } => {
            let opdracht = Opdracht::Plak {
                tekst,
                doel_onzeker,
                toetsen: true,
                melder: std::sync::Arc::new(meld),
            };
            // Komt de opdracht niet aan, dan meldt de melder die erin zat het zelf.
            if let Err(Opdracht::Plak { melder, .. }) = stuur(opdracht) {
                melder(PlakUitkomst::Mislukt.into());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Toetsen en vensters
// ---------------------------------------------------------------------------

/// Is er nu een modifier ingedrukt (Ctrl, Alt, Shift of Windows)?
fn modifier_ingedrukt() -> bool {
    [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
        .iter()
        // SAFETY: geen voorwaarden. Het hoogste bit betekent "nu ingedrukt".
        .any(|toets| unsafe { GetAsyncKeyState(i32::from(toets.0)) } < 0)
}

/// Een toetsgebeurtenis, met de scancode erbij: sommige programma's (en
/// toetsenbordindelingen) kijken daarnaar in plaats van naar de virtuele toets.
fn toets(toets: VIRTUAL_KEY, vlaggen: KEYBD_EVENT_FLAGS) -> INPUT {
    // SAFETY: geen voorwaarden.
    let scancode = unsafe { MapVirtualKeyW(u32::from(toets.0), MAPVK_VK_TO_VSC) };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: toets,
                wScan: u16::try_from(scancode).unwrap_or(0),
                dwFlags: vlaggen,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn invoergrootte() -> i32 {
    i32::try_from(std::mem::size_of::<INPUT>()).unwrap_or(i32::MAX)
}

/// Stuurt Ctrl neer, V neer en V op. Ctrl gaat los via [`laat_ctrl_los`].
fn stuur_ctrl_v_neer() -> Result<(), KlembordFout> {
    let neer = [
        toets(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
        toets(VK_V, KEYBD_EVENT_FLAGS(0)),
        toets(VK_V, KEYEVENTF_KEYUP),
    ];
    // SAFETY: geldige INPUT-structuren met de juiste grootte.
    let verstuurd = unsafe { SendInput(&neer, invoergrootte()) };
    if verstuurd as usize == neer.len() {
        Ok(())
    } else {
        Err(KlembordFout("invoer"))
    }
}

/// Laat Ctrl los als Mondig hem neer heeft gezet. Altijd, ook bij afronden of afsluiten,
/// zodat Ctrl nooit blijft hangen.
fn laat_ctrl_los(venster: HWND) {
    // SAFETY: het venster is van deze thread.
    unsafe {
        let _ = KillTimer(Some(venster), TIMER_CTRL);
    }
    if !CTRL_NEER.replace(false) {
        return;
    }
    // SAFETY: een geldige INPUT-structuur met de juiste grootte.
    let los = unsafe { SendInput(&[toets(VK_CONTROL, KEYEVENTF_KEYUP)], invoergrootte()) };
    if los != 1 {
        LOPEND.with_borrow_mut(|l| {
            if let Some(t) = l.as_mut() {
                t.stand.invoeren_mislukt = true;
            }
        });
    }
}

/// Het integriteitsniveau van een proces.
fn integriteit_van(proces: HANDLE) -> Integriteit {
    // SAFETY: het token wordt geopend, gelezen in een buffer van de gevraagde grootte en
    // gesloten; de SID-wijzer wijst in die buffer.
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(proces, TOKEN_QUERY, &raw mut token).is_err() {
            return Integriteit::Onleesbaar;
        }
        let mut nodig = 0u32;
        let _ = GetTokenInformation(token, TokenIntegrityLevel, None, 0, &raw mut nodig);
        // u64 voor de uitlijning van TOKEN_MANDATORY_LABEL.
        let mut buffer = vec![0u64; (nodig as usize).div_ceil(8).max(1)];
        let gelezen = GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buffer.as_mut_ptr().cast()),
            nodig,
            &raw mut nodig,
        );
        let _ = CloseHandle(token);
        if gelezen.is_err() {
            return Integriteit::Onleesbaar;
        }
        let label = &*buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
        let sid = label.Label.Sid;
        let aantal = *GetSidSubAuthorityCount(sid);
        if aantal == 0 {
            return Integriteit::Onleesbaar;
        }
        Integriteit::Niveau(*GetSidSubAuthority(sid, u32::from(aantal - 1)))
    }
}

/// Het integriteitsniveau van Mondig zelf; bij twijfel "midden".
fn eigen_integriteit() -> u32 {
    // SAFETY: de pseudo-handle van het eigen proces hoeft niet gesloten te worden.
    match integriteit_van(unsafe { GetCurrentProcess() }) {
        Integriteit::Niveau(niveau) => niveau,
        Integriteit::Onleesbaar => INTEGRITEIT_MIDDEN,
    }
}

/// Het venster met de focus, of `None` als er geen is.
fn voorgrond() -> Option<Voorgrond> {
    // SAFETY: gewone Win32-aanroepen; de proceshandle wordt gesloten.
    unsafe {
        let venster = GetForegroundWindow();
        if venster.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(venster, Some(&raw mut pid));
        if pid == 0 {
            return None;
        }
        if pid == GetCurrentProcessId() {
            return Some(Voorgrond {
                eigen_proces: true,
                integriteit: Integriteit::Niveau(eigen_integriteit()),
            });
        }
        let integriteit = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(proces) => {
                let niveau = integriteit_van(proces);
                let _ = CloseHandle(proces);
                niveau
            }
            Err(_) => Integriteit::Onleesbaar,
        };
        Some(Voorgrond {
            eigen_proces: false,
            integriteit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Leest de tekst van het klembord, zoals een doelprogramma dat doet bij Ctrl+V.
    fn lees_tekst(venster: HWND) -> Option<String> {
        open(Some(venster), OPENEN_POGINGEN).ok()?;
        // SAFETY: het klembord is open; de handle wordt alleen gelezen.
        let tekst = unsafe {
            GetClipboardData(u32::from(CF_UNICODETEXT.0))
                .ok()
                .and_then(|handle| {
                    let geheugen = HGLOBAL(handle.0);
                    let wijzer = GlobalLock(geheugen).cast::<u16>().cast_const();
                    if wijzer.is_null() {
                        return None;
                    }
                    let lengte = GlobalSize(geheugen) / 2;
                    let tekens = std::slice::from_raw_parts(wijzer, lengte);
                    let eind = tekens.iter().position(|t| *t == 0).unwrap_or(lengte);
                    let tekst = String::from_utf16_lossy(&tekens[..eind]);
                    let _ = GlobalUnlock(geheugen);
                    Some(tekst)
                })
        };
        sluit();
        tekst
    }

    /// Zet gegevens op het klembord zoals een ander programma dat doet.
    fn zet_als_ander(venster: HWND, formaat: u32, data: &[u8]) {
        open(Some(venster), OPENEN_POGINGEN).expect("openen");
        // SAFETY: het klembord is open.
        unsafe {
            let _ = EmptyClipboard();
        }
        zet_globaal(formaat, data).expect("zetten");
        sluit();
    }

    /// Een plakactie zonder toetsen; geeft de ontvanger van de afloop.
    fn plak_zonder_toetsen(tekst: &str) -> Receiver<Afloop> {
        let (zender, ontvanger) = mpsc::channel();
        let gestuurd = stuur(Opdracht::Plak {
            tekst: tekst.to_string(),
            doel_onzeker: false,
            toetsen: false,
            melder: std::sync::Arc::new(move |afloop| {
                let _ = zender.send(afloop);
            }),
        });
        assert!(gestuurd.is_ok(), "klembordthread");
        ontvanger
    }

    /// Wacht tot de klembordthread de belofte heeft neergezet en leest hem dan.
    fn wacht_op_belofte(venster: HWND, tekst: &str) -> bool {
        let begin = Instant::now();
        while begin.elapsed() < Duration::from_secs(2) {
            if lees_tekst(venster).as_deref() == Some(tekst) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    fn afloop(ontvanger: &Receiver<Afloop>) -> Afloop {
        ontvanger
            .recv_timeout(Duration::from_secs(12))
            .expect("afgerond")
    }

    /// Een DIB van 2560x1440 (een schermafdruk), 32 bits.
    fn grote_afbeelding() -> Vec<u8> {
        let (breedte, hoogte) = (2560u32, 1440u32);
        let mut dib = Vec::new();
        for veld in [40u32, breedte, hoogte] {
            dib.extend_from_slice(&veld.to_le_bytes());
        }
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&(breedte * hoogte * 4).to_le_bytes());
        dib.extend_from_slice(&[0u8; 16]);
        dib.resize(dib.len() + (breedte * hoogte * 4) as usize, 0x5a);
        dib
    }

    fn cf_dib() -> u32 {
        u32::from(windows::Win32::System::Ole::CF_DIB.0)
    }

    fn heeft_afbeelding(venster: HWND, lengte: usize) -> bool {
        let (bewaard, _) = bewaar(Some(venster), Duration::from_secs(5)).expect("bewaren");
        bewaard.iter().any(|b| {
            matches!(b, Bewaard::Bytes { formaat, data }
                if *formaat == cf_dib() && data.len() == lengte)
        })
    }

    /// Integratietest met alleen het klembord, via de klembordthread: zetten met delayed
    /// rendering, uitlezen in hetzelfde proces (het leesbewijs), terugzetten, de gebruiker
    /// die intussen kopieert, een grote afbeelding, twee dictaten vlak na elkaar en een bezet
    /// klembord. Drukt geen toetsen in. Bewaart het klembord van de gebruiker vooraf en zet
    /// het na afloop terug. Handmatig draaien:
    /// `cargo test -p mondig klembord_ronde -- --ignored --nocapture`.
    #[test]
    #[ignore = "gebruikt het echte klembord van deze pc"]
    #[expect(
        clippy::print_stdout,
        reason = "de handmatige test toont metingen en loopt de stappen achter elkaar af"
    )]
    fn klembord_ronde() {
        let venster = maak_venster().expect("venster");
        let (gebruiker, _) = bewaar(Some(venster), Duration::from_secs(5)).expect("bewaren");
        let oud = lees_tekst(venster);

        // 1. Plakken en lezen: Geplakt, en het oude klembord is terug.
        let ontvanger = plak_zonder_toetsen("Proef van Mondig ");
        assert!(wacht_op_belofte(venster, "Proef van Mondig "), "belofte");
        let eerste = afloop(&ontvanger);
        println!("1: {eerste:?}");
        assert_eq!(eerste.uitkomst, PlakUitkomst::Geplakt);
        assert_eq!(eerste.vorig_klembord, VorigKlembord::Bewaard);
        assert_eq!(lees_tekst(venster), oud, "het oude klembord is terug");

        // 2. De gebruiker kopieert intussen zelf iets: dat blijft staan.
        let ontvanger = plak_zonder_toetsen("Tweede proef ");
        assert!(wacht_op_belofte(venster, "Tweede proef "), "belofte");
        zet_als_ander(
            venster,
            u32::from(CF_UNICODETEXT.0),
            &u16_als_bytes(&breed("Zelf gekopieerd")),
        );
        let tweede = afloop(&ontvanger);
        println!("2: {tweede:?}");
        assert_eq!(lees_tekst(venster).as_deref(), Some("Zelf gekopieerd"));

        // 3. Een grote afbeelding bewaren gaat snel.
        let dib = grote_afbeelding();
        zet_als_ander(venster, cf_dib(), &dib);
        let begin = Instant::now();
        let (bewaard, volledig) = bewaar(Some(venster), BEWAAR_BUDGET).expect("bewaren");
        let duur = begin.elapsed();
        println!(
            "3: afbeelding van {} MB bewaard in {} ms, {} formaten, volledig {volledig}",
            dib.len() / 1_000_000,
            duur.as_millis(),
            bewaard.len()
        );
        assert!(volledig);
        assert!(duur < Duration::from_millis(100), "bewaren duurde {duur:?}");
        drop(bewaard);

        // 4. Twee dictaten vlak na elkaar: het tweede rondt het eerste eerst af, dus het
        // bewaart de afbeelding en niet het eerste dictaat.
        let ontvanger_a = plak_zonder_toetsen("Eerste ");
        let ontvanger_b = plak_zonder_toetsen("Tweede ");
        let a = afloop(&ontvanger_a);
        assert!(wacht_op_belofte(venster, "Tweede "), "belofte");
        let b = afloop(&ontvanger_b);
        println!("4: {a:?} {b:?}");
        assert_eq!(b.uitkomst, PlakUitkomst::Geplakt);
        assert!(
            heeft_afbeelding(venster, dib.len()),
            "de afbeelding is terug, niet het eerste dictaat"
        );

        // 5. Het klembord is bezet: Mondig plakt niet, en het klembord blijft ongeschonden.
        open(Some(venster), 1).expect("klembord vasthouden");
        let ontvanger = plak_zonder_toetsen("Bezet ");
        std::thread::sleep(Duration::from_millis(1500));
        sluit();
        let bezet = afloop(&ontvanger);
        println!("5: {bezet:?}");
        assert_eq!(bezet.uitkomst, PlakUitkomst::Mislukt);
        assert_eq!(bezet.vorig_klembord, VorigKlembord::Bewaard);
        assert!(heeft_afbeelding(venster, dib.len()), "ongeschonden");

        // 6. Een ander programma houdt het klembord vast op het moment van terugzetten:
        // de klembordthread probeert het opnieuw en zet de afbeelding alsnog terug.
        let ontvanger = plak_zonder_toetsen("Vasthouden ");
        assert!(wacht_op_belofte(venster, "Vasthouden "), "belofte");
        open(Some(venster), 1).expect("klembord vasthouden");
        std::thread::sleep(Duration::from_millis(1200));
        sluit();
        let vast = afloop(&ontvanger);
        println!("6: {vast:?}");
        assert_eq!(vast.uitkomst, PlakUitkomst::Geplakt);
        std::thread::sleep(Duration::from_millis(1000));
        assert!(
            heeft_afbeelding(venster, dib.len()),
            "na de herpoging is de afbeelding terug"
        );

        // Het klembord van de gebruiker terug.
        let mut gebruiker = gebruiker;
        zet_terug(venster, &mut gebruiker, TERUGZET_POGINGEN).expect("terugzetten");
        assert_eq!(lees_tekst(venster), oud);
        // SAFETY: het testvenster is van deze thread.
        unsafe {
            let _ = DestroyWindow(venster);
        }
    }
}
