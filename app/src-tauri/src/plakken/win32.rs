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
//! - **Typen als terugval.** Komt er geen leesbewijs binnen `GRENS_TYPEN`, dan kijkt de
//!   klembordthread eerst of het doel reageert (`IsHungAppWindow`, `SendMessageTimeout` met
//!   `WM_NULL`) en of er een bewerkbaar tekstveld de focus heeft (UI Automation, aangevuld met
//!   de systeemcaret uit `GetGUIThreadInfo`). Zo ja, dan trekt hij de belofte in met het
//!   klembord open ([`trek_in`]): zolang wij het open hebben, kan geen ander programma lezen,
//!   dus wat dan in de signalen staat, is alles. Het klembord blijft leeg tot het typen klaar
//!   is. Hij typt met `SendInput` en `KEYEVENTF_UNICODE`: elke UTF-16-eenheid als eigen
//!   gebeurtenis (een surrogaatpaar dus als twee), zonder virtuele toetscode, zodat de
//!   toetsenbordindeling er niet toe doet
//!   (learn.microsoft.com/windows/win32/api/winuser/ns-winuser-keybdinput, `KEYEVENTF_UNICODE`;
//!   learn.microsoft.com/windows/win32/api/uiautomationclient, `IUIAutomation::GetFocusedElement`;
//!   geraadpleegd 9-11-2026). Het typen loopt op de klembordthread; tussen de blokken handelt
//!   hij verzonden berichten af, zodat een ander programma dat intussen kopieert niet hoeft te
//!   wachten.
//! - **Wie las er?** Bij `WM_RENDERFORMAT` heeft de lezer het klembord open
//!   (`GetOpenClipboardWindow`). Alleen een lezing door het proces van het doel telt als
//!   leesbewijs; las een ander programma (een klembordbeheerder), dan staat de tekst wel echt op
//!   het klembord en typt Mondig niet.

#![expect(
    unsafe_code,
    reason = "klembord, toetsinvoer, procestokens en UI Automation kunnen alleen via de Win32-API"
)]

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU8, Ordering as AtomicOrdering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};
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
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardFormatNameW, GetClipboardOwner, GetClipboardSequenceNumber, GetOpenClipboardWindow,
    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
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
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationValuePattern, UIA_DocumentControlTypeId,
    UIA_EditControlTypeId, UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VIRTUAL_KEY,
    VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CopyImage, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GUITHREADINFO,
    GetForegroundWindow, GetGUIThreadInfo, GetMessageW, GetWindowThreadProcessId, HWND_MESSAGE,
    IMAGE_BITMAP, IsHungAppWindow, KillTimer, LR_CREATEDIBSECTION, MSG, PM_NOREMOVE, PeekMessageW,
    PostMessageW, PostQuitMessage, RegisterClassW, SMTO_ABORTIFHUNG, SendMessageTimeoutW, SetTimer,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_DESTROYCLIPBOARD, WM_NULL, WM_RENDERALLFORMATS,
    WM_RENDERFORMAT, WM_TIMER, WNDCLASSW,
};
use windows::core::{Interface, PCWSTR, w};

use super::{
    Afloop, Afronding, Afweging, Focusbeeld, HERPOGING_STAP, Herpogingstap, Integriteit,
    PlakUitkomst, Plan, Stopreden, Transactiestand, Typafloop, Typbesluit, Uiarol, Voorgrond,
    VorigKlembord, afronding, afweging, herpogingstap, is_tekstveld, kies_plan, lezer_is_doel,
    merkbaar, met_spatie, nog_van_ons, rest_van, te_bewaren, typ_in_blokken, typbesluit,
    typstukken, uitkomst_na_typen, vorig_na_terugzetten, wacht_op_losse_modifiers,
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
/// Zoveel UTF-16-eenheden typen we per `SendInput` (64 gebeurtenissen). Grote reeksen in één
/// keer raken in sommige programma's (Electron, sessies op afstand) deels kwijt.
const TYPBLOK: usize = 32;
/// De pauze tussen twee blokken, zodat het doel de vorige invoer verwerkt. Tweehonderd
/// tekens kosten zo ongeveer vijftig milliseconden.
const TYPPAUZE: Duration = Duration::from_millis(8);
/// Zo lang mag het doel over een `WM_NULL` doen; anders reageert het niet.
const REACTIE_WACHT_MS: u32 = 150;
/// Zo lang wacht de klembordthread hooguit op UI Automation.
const TEKSTVELD_WACHT: Duration = Duration::from_millis(500);

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
    let volledig = zet_items(bewaard);
    sluit();
    Ok(volledig)
}

/// Zet de bewaarde formaten op het (open en leeggemaakte) klembord. Geeft aan of elk
/// formaat terugkwam.
fn zet_items(bewaard: &mut [Bewaard]) -> bool {
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
    volledig
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
        /// Of de vrager nog wacht (zie [`neem_op`] en [`geef_op`]).
        staat: Arc<AtomicU8>,
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
    /// Lezingen door het doel (zie [`lezer_is_doel`]).
    leesbewijzen: Vec<Instant>,
    /// Las een ander programma dan het doel de tekst?
    andere_lezer: bool,
    eigendom_kwijt: bool,
    /// Het volgnummer van het klembord na onze laatste eigen wijziging.
    volgnummer: u32,
    /// Het proces van het venster dat bij Ctrl+V vooraan stond, als dat bekend is.
    doel_proces: Option<u32>,
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
    /// De tekst, om te typen als het leesbewijs uitblijft.
    tekst: String,
    /// Het venster dat bij Ctrl+V vooraan stond. Staat er tijdens het typen een ander
    /// venster vooraan, dan stopt het typen.
    doel: isize,
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
            // Wie leest? Het venster dat het klembord nu open heeft. Alleen het doel zelf
            // levert een leesbewijs; Ditto of het klembord van een ander programma niet.
            let lezer = lezend_proces();
            let tekst = SIGNALEN.with_borrow_mut(|s| {
                if lezer_is_doel(lezer, s.doel_proces) {
                    s.leesbewijzen.push(Instant::now());
                } else {
                    s.andere_lezer = true;
                }
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
            let besluit = LOPEND.with_borrow_mut(|lopend| {
                lopend.as_mut().map(|t| {
                    werk_stand_bij(&mut t.stand);
                    afweging(&t.stand, Instant::now())
                })
            });
            match besluit {
                Some(Afweging::Afronden) => rond_af(venster),
                Some(Afweging::Typen) => typ_terugval(venster),
                Some(Afweging::Wachten) | None => {}
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
        stand.andere_lezer = s.andere_lezer;
        stand.eigendom_kwijt = s.eigendom_kwijt;
    });
}

/// Het proces van het venster dat het klembord nu open heeft (de lezer bij `WM_RENDERFORMAT`),
/// of `None` als het zonder venster geopend is.
fn lezend_proces() -> Option<u32> {
    // SAFETY: geen voorwaarden; het proces wordt alleen gelezen.
    unsafe {
        let venster = GetOpenClipboardWindow().ok()?;
        let mut proces = 0u32;
        GetWindowThreadProcessId(venster, Some(&raw mut proces));
        (proces != 0).then_some(proces)
    }
}

/// Het proces achter een venster, of `None`.
fn proces_van(venster: isize) -> Option<u32> {
    let mut proces = 0u32;
    // SAFETY: een venstergreep als getal; een ongeldige greep geeft proces 0.
    unsafe {
        GetWindowThreadProcessId(
            HWND(venster as *mut core::ffi::c_void),
            Some(&raw mut proces),
        );
    }
    (proces != 0).then_some(proces)
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
        // De lopende plakactie wordt afgebroken: geen tijd meer om te lezen of te typen.
        LOPEND.with_borrow_mut(|lopend| {
            if let Some(t) = lopend.as_mut() {
                t.stand.afgebroken = true;
            }
        });
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
            Opdracht::ZetBlijvend {
                tekst,
                antwoord,
                staat,
            } => {
                // De gebruiker kopieert zelf: dat wint van een oud klembord dat niet terug kon.
                if let Some(herpoging) = overgebleven {
                    meld_kwijt(&herpoging);
                }
                // Gaf de vrager het al op (hij meldde "mislukt"), dan ook niet meer uitvoeren.
                if neem_op(&staat) {
                    let _ = antwoord.send(zet_blijvend_hier(venster, &tekst));
                }
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
    let doel = voorgrondvenster();
    SIGNALEN.with_borrow_mut(|s| {
        s.leesbewijzen.clear();
        s.andere_lezer = false;
        s.eigendom_kwijt = false;
        s.volgnummer = nummer;
        // Zonder toetsen (de tests) leest de test zelf: dan is het doel onbekend.
        s.doel_proces = if toetsen { proces_van(doel) } else { None };
    });
    let mut transactie = Transactie {
        stand: Transactiestand::nieuw(Instant::now(), doel_onzeker),
        bewaard,
        aangeraakt: true,
        publicatie_mislukt: false,
        tekst: tekst.to_string(),
        doel,
        melder,
    };
    // Zonder toetsen (de tests) ook niet typen.
    transactie.stand.typen_mag = toetsen;
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
    // Kwam de tekst bij een onzeker doel niet aan (waarschijnlijk UIPI, dat ook typen
    // tegenhoudt), dan blijft hij op het klembord: de laatste terugval. Niet als een volgende
    // opdracht de plakactie afbrak; dan was er geen tijd om te lezen.
    let tekst_laten = uitkomst == PlakUitkomst::NietGeplakt && !transactie.stand.afgebroken;
    match afronding(
        transactie.aangeraakt,
        transactie.bewaard.is_some() && !tekst_laten,
        van_ons,
    ) {
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

/// Hoe het intrekken van de belofte afliep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Intrekking {
    /// De belofte is weg en het klembord is leeg; het oude klembord zit nog in de transactie.
    Ingetrokken,
    /// Nog niet: het klembord was even bezet (misschien leest het doel net), of er kwam toch
    /// een lezing. Bij de volgende tik opnieuw kijken.
    NogNiet,
    /// De gebruiker kopieerde intussen zelf iets: dat wint.
    NietVanOns,
}

/// Trekt de belofte in en laat het klembord leeg, in één keer met het klembord open. Zolang
/// wij het klembord open hebben, kan geen ander programma lezen (`GetClipboardData` vraagt een
/// open klembord), en een lezing die eerder begon, hield het klembord zelf open, zodat
/// `OpenClipboard` hier mislukt. Wat er na het openen in de signalen staat, is dus alles:
/// zonder lezing kan het dictaat na deze stap niet meer geplakt worden, en een laat plakken
/// krijgt niets (ook niet het oude klembord, dat iets gevoeligs kan bevatten).
fn trek_in(venster: HWND, transactie: &mut Transactie) -> Intrekking {
    // Eén poging: lukt het niet, dan pakt de berichtenlus intussen een leesverzoek op.
    if open(Some(venster), 1).is_err() {
        return Intrekking::NogNiet;
    }
    werk_stand_bij(&mut transactie.stand);
    if !klembord_van_ons() {
        sluit();
        return Intrekking::NietVanOns;
    }
    let stand = &transactie.stand;
    // SAFETY: het klembord is open.
    if stand.laatste_leesbewijs().is_some()
        || stand.al_gerenderd()
        || unsafe { EmptyClipboard() }.is_err()
    {
        sluit();
        return Intrekking::NogNiet;
    }
    sluit();
    // Geen belofte meer van ons: niets meer te renderen. Ons eigen leegmaken stuurde
    // `WM_DESTROYCLIPBOARD`; het lege klembord is nu van ons, met een nieuw volgnummer.
    // SAFETY: geen voorwaarden.
    let nummer = unsafe { GetClipboardSequenceNumber() };
    SIGNALEN.with_borrow_mut(|s| {
        s.tekst = None;
        s.eigendom_kwijt = false;
        s.volgnummer = nummer;
    });
    Intrekking::Ingetrokken
}

/// Handelt berichten af die andere threads ons sturen (`WM_DESTROYCLIPBOARD` als de gebruiker
/// tijdens het typen kopieert), zonder de wachtrij met opdrachten en timers aan te raken. Zo
/// hoeft een ander programma niet op het typen te wachten.
fn verwerk_verzonden_berichten() {
    let mut bericht = MSG::default();
    // SAFETY: `PeekMessageW` met `PM_NOREMOVE` levert eerst de verzonden berichten af en laat
    // geplaatste berichten in de rij.
    unsafe {
        let _ = PeekMessageW(&raw mut bericht, None, 0, 0, PM_NOREMOVE);
    }
}

/// Reageert het venster? Een `WM_NULL` met een korte time-out; een programma dat hangt,
/// leest misschien alsnog, dus dan typt Mondig niet. Tijdens het wachten beantwoordt deze
/// thread verzonden berichten (geen `SMTO_BLOCK`), dus ook een leesverzoek van het doel.
fn reageert(doel: isize) -> bool {
    let venster = HWND(doel as *mut core::ffi::c_void);
    // SAFETY: een venstergreep als getal; een ongeldige greep geeft alleen een fout.
    unsafe {
        if IsHungAppWindow(venster).as_bool() {
            return false;
        }
        SendMessageTimeoutW(
            venster,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            REACTIE_WACHT_MS,
            None,
        )
        .0 != 0
    }
}

/// Heeft een bewerkbaar tekstveld de focus (zie [`is_tekstveld`])? UI Automation op een
/// hulpthread met een tijdslimiet, zodat een programma dat traag antwoordt de klembordthread
/// niet ophoudt. `None` bij een fout of time-out.
fn tekstveld(doel: isize) -> Option<bool> {
    let caret = heeft_caret(doel);
    let (zender, ontvanger) = mpsc::channel();
    std::thread::Builder::new()
        .name("mondig-tekstveld".into())
        .spawn(move || {
            let _ = zender.send(focusbeeld(caret));
        })
        .ok()?;
    ontvanger
        .recv_timeout(TEKSTVELD_WACHT)
        .ok()
        .flatten()
        .map(is_tekstveld)
}

/// Heeft de thread van het venster een systeemcaret (`GetGUIThreadInfo`, `hwndCaret`)?
fn heeft_caret(doel: isize) -> bool {
    let venster = HWND(doel as *mut core::ffi::c_void);
    // SAFETY: de structuur heeft de juiste grootte; een ongeldige greep geeft thread 0.
    unsafe {
        let thread = GetWindowThreadProcessId(venster, None);
        let mut info = GUITHREADINFO {
            cbSize: u32::try_from(std::mem::size_of::<GUITHREADINFO>()).unwrap_or(0),
            ..Default::default()
        };
        thread != 0
            && GetGUIThreadInfo(thread, &raw mut info).is_ok()
            && !info.hwndCaret.is_invalid()
    }
}

/// Wat UI Automation over het element met de focus zegt. Alleen op een eigen thread: die
/// krijgt een eigen COM-omgeving (MTA).
fn focusbeeld(caret: bool) -> Option<Focusbeeld> {
    // SAFETY: COM wordt op deze thread geopend en aan het eind gesloten; de interfaces worden
    // vrijgegeven voordat COM sluit (ze vallen eerder uit beeld).
    unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            return None;
        }
        let beeld = (|| {
            let automatisering: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let element = automatisering.GetFocusedElement().ok()?;
            let soort = element.CurrentControlType().ok()?;
            let rol = if soort == UIA_EditControlTypeId {
                Uiarol::Invoer
            } else if soort == UIA_DocumentControlTypeId {
                Uiarol::Document
            } else {
                Uiarol::Anders
            };
            let waarde_alleen_lezen = element
                .GetCurrentPattern(UIA_ValuePatternId)
                .ok()
                .and_then(|patroon| patroon.cast::<IUIAutomationValuePattern>().ok())
                .and_then(|waarde| waarde.CurrentIsReadOnly().ok())
                .map(windows::core::BOOL::as_bool);
            let tekstpatroon = element.GetCurrentPattern(UIA_TextPatternId).is_ok();
            Some(Focusbeeld {
                rol,
                waarde_alleen_lezen,
                tekstpatroon,
                caret,
            })
        })();
        CoUninitialize();
        beeld
    }
}

/// Geen leesbewijs binnen de grens. Reageert het doel niet, dan wacht Mondig tot de gewone
/// grens. Heeft er geen tekstveld de focus, dan blijft de tekst op het klembord. Anders: de
/// belofte intrekken, typen met een leeg klembord, en daarna het oude klembord terug of, als
/// het typen stopte, de rest op het klembord.
fn typ_terugval(venster: HWND) {
    let Some(mut transactie) = LOPEND.with_borrow_mut(Option::take) else {
        return;
    };
    let doel = transactie.doel;
    let reageert = reageert(doel);
    let besluit = typbesluit(reageert, reageert.then(|| tekstveld(doel)).flatten());
    // Tijdens het vragen kan het doel toch gelezen hebben (zie `reageert`).
    werk_stand_bij(&mut transactie.stand);
    if transactie.stand.laatste_leesbewijs().is_some() || besluit == Typbesluit::Wachten {
        // Gewoon plakken, of het doel hangt: niet meer typen, wachten tot de gewone grens.
        transactie.stand.typen_mag = false;
        LOPEND.with_borrow_mut(|l| *l = Some(transactie));
        return;
    }
    if besluit == Typbesluit::Klembord {
        // Geen tekstveld: getypte letters zouden sneltoetsen worden.
        rond_af_zonder_tekstveld(venster, &transactie);
        return;
    }
    match trek_in(venster, &mut transactie) {
        Intrekking::Ingetrokken => {}
        Intrekking::NogNiet => {
            // De wachttimer loopt door; de afweging beslist bij de volgende tik opnieuw.
            LOPEND.with_borrow_mut(|l| *l = Some(transactie));
            return;
        }
        Intrekking::NietVanOns => {
            LOPEND.with_borrow_mut(|l| *l = Some(transactie));
            rond_af(venster);
            return;
        }
    }
    // SAFETY: het venster is van deze thread.
    unsafe {
        let _ = KillTimer(Some(venster), TIMER_WACHT);
    }
    laat_ctrl_los(venster);
    let stukken = typstukken(&transactie.tekst, TYPBLOK);
    let afloop = typ_in_blokken(
        &stukken,
        || {
            if !wacht_op_losse_modifiers(modifier_ingedrukt) {
                Some(Stopreden::Toetsen)
            } else if voorgrondvenster() != doel {
                Some(Stopreden::AnderVenster)
            } else {
                None
            }
        },
        stuur_stuk,
        || {
            std::thread::sleep(TYPPAUZE);
            verwerk_verzonden_berichten();
        },
    );
    verwerk_verzonden_berichten();
    let mut afloop_ui = Afloop {
        uitkomst: uitkomst_na_typen(afloop),
        vorig_klembord: VorigKlembord::Bewaard,
    };
    if !klembord_van_ons() {
        // De gebruiker kopieerde tijdens het typen: dat wint, er gaat niets overheen.
    } else if let Typafloop::Gestopt { bij, .. } = afloop {
        // De rest op het klembord, zodat de gebruiker hem zelf plakt; dat vervangt het oude
        // klembord. Brak `SendInput` midden in een blok af, dan staat het begin van dat blok
        // ook in de rest: liever iets dubbel dan iets kwijt.
        if zet_blijvend_hier(venster, &rest_van(&stukken[bij..])) {
            afloop_ui.vorig_klembord = VorigKlembord::Kwijt;
        } else {
            afloop_ui.uitkomst = PlakUitkomst::Mislukt;
        }
    } else if let Some(mut kopie) = transactie.bewaard.take() {
        if let Ok(volledig) = zet_terug(venster, &mut kopie.items, OPENEN_POGINGEN) {
            afloop_ui.vorig_klembord = vorig_na_terugzetten(kopie.volledig, volledig);
        } else {
            // Een ander programma houdt het klembord vast: later opnieuw, zoals bij plakken.
            afloop_ui.vorig_klembord = vorig_na_terugzetten(kopie.volledig, true);
            start_herpoging(venster, kopie, afloop_ui, transactie.melder.clone());
        }
    } else {
        // Het oude klembord was niet bewaard: het blijft leeg.
        afloop_ui.vorig_klembord = VorigKlembord::Kwijt;
    }
    (transactie.melder)(afloop_ui);
}

/// Er kwam geen leesbewijs en er had geen tekstveld de focus: de tekst blijft op het klembord
/// (de belofte wordt echte tekst, buiten de geschiedenis). Kopieerde de gebruiker intussen
/// zelf, dan blijft dat staan.
fn rond_af_zonder_tekstveld(venster: HWND, transactie: &Transactie) {
    // SAFETY: het venster is van deze thread.
    unsafe {
        let _ = KillTimer(Some(venster), TIMER_WACHT);
    }
    laat_ctrl_los(venster);
    let afloop = if klembord_van_ons() {
        render_belofte(venster);
        Afloop {
            uitkomst: PlakUitkomst::GeenTekstveld,
            vorig_klembord: VorigKlembord::Kwijt,
        }
    } else {
        PlakUitkomst::NietGelezen.into()
    };
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
///
/// Typt de klembordthread nog (dat kan langer duren dan [`ANTWOORD_WACHT`]), dan is het
/// antwoord altijd eerlijk: geeft deze kant het op, dan voert de klembordthread de opdracht
/// ook niet meer uit; was hij er al mee bezig, dan wacht deze kant op de echte uitslag.
pub(crate) fn zet_blijvend(tekst: &str) -> bool {
    let (antwoord, wacht) = mpsc::channel();
    let staat = Arc::new(AtomicU8::new(OPDRACHT_WACHT));
    if stuur(Opdracht::ZetBlijvend {
        tekst: tekst.to_string(),
        antwoord,
        staat: Arc::clone(&staat),
    })
    .is_err()
    {
        return false;
    }
    match wacht.recv_timeout(ANTWOORD_WACHT) {
        Ok(gelukt) => gelukt,
        Err(_) if geef_op(&staat) => false,
        // Al bezig: het antwoord komt zo (openen en schrijven duurt hooguit enkele tienden).
        Err(_) => wacht.recv().unwrap_or(false),
    }
}

/// De opdracht wacht nog in de rij.
const OPDRACHT_WACHT: u8 = 0;
/// De klembordthread voert hem uit.
const OPDRACHT_BEZIG: u8 = 1;
/// De vrager gaf het op en meldde "mislukt".
const OPDRACHT_OPGEGEVEN: u8 = 2;

/// De klembordthread neemt de opdracht op, als de vrager nog wacht.
fn neem_op(staat: &AtomicU8) -> bool {
    staat
        .compare_exchange(
            OPDRACHT_WACHT,
            OPDRACHT_BEZIG,
            AtomicOrdering::SeqCst,
            AtomicOrdering::SeqCst,
        )
        .is_ok()
}

/// De vrager geeft het op, als de klembordthread er nog niet aan begon.
fn geef_op(staat: &AtomicU8) -> bool {
    staat
        .compare_exchange(
            OPDRACHT_WACHT,
            OPDRACHT_OPGEGEVEN,
            AtomicOrdering::SeqCst,
            AtomicOrdering::SeqCst,
        )
        .is_ok()
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
        Plan::Klembord(uitkomst) => meld(if !zet_blijvend(&tekst) {
            PlakUitkomst::Mislukt.into()
        } else if uitkomst == PlakUitkomst::Beheerrechten {
            // Het dictaat vervangt het klembord: niet "bewaard" melden.
            Afloop {
                uitkomst,
                vorig_klembord: VorigKlembord::Kwijt,
            }
        } else {
            uitkomst.into()
        }),
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

/// Eén gebeurtenis met een UTF-16-eenheid als teken: geen virtuele toets (`wVk` 0), de
/// eenheid in `wScan` en `KEYEVENTF_UNICODE`. Windows maakt er `VK_PACKET` van en geeft het
/// teken door zonder naar de toetsenbordindeling te kijken.
fn unicode_toets(eenheid: u16, los: bool) -> INPUT {
    let vlaggen = if los {
        KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
    } else {
        KEYEVENTF_UNICODE
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: eenheid,
                dwFlags: vlaggen,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// De gebeurtenissen voor een blok: per UTF-16-eenheid neer en los (een surrogaatpaar dus als
/// twee tekens achter elkaar, zoals Windows dat verwacht). Geen enkele echte toets: ook
/// regeleindes zijn in [`typstukken`] al spaties geworden.
fn invoer_voor(stuk: &str) -> Vec<INPUT> {
    stuk.encode_utf16()
        .flat_map(|eenheid| [unicode_toets(eenheid, false), unicode_toets(eenheid, true)])
        .collect()
}

/// Typt één blok in het venster met de focus. `false` als Windows niet alles aannam.
fn stuur_stuk(stuk: &str) -> bool {
    let invoer = invoer_voor(stuk);
    // SAFETY: geldige INPUT-structuren met de juiste grootte.
    let verstuurd = unsafe { SendInput(&invoer, invoergrootte()) };
    verstuurd as usize == invoer.len()
}

/// Het venster vooraan als getal, om te vergelijken (0 als er geen is).
fn voorgrondvenster() -> isize {
    // SAFETY: geen voorwaarden.
    unsafe { GetForegroundWindow() }.0 as isize
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

    /// (wVk, wScan, losgelaten, unicode) van elke gebeurtenis.
    fn ontleed(invoer: &[INPUT]) -> Vec<(u16, u16, bool, bool)> {
        invoer
            .iter()
            .map(|i| {
                // SAFETY: alle gebeurtenissen hier zijn toetsgebeurtenissen.
                let ki = unsafe { i.Anonymous.ki };
                (
                    ki.wVk.0,
                    ki.wScan,
                    ki.dwFlags.contains(KEYEVENTF_KEYUP),
                    ki.dwFlags.contains(KEYEVENTF_UNICODE),
                )
            })
            .collect()
    }

    #[test]
    fn tekens_gaan_als_unicode_zonder_toetscode() {
        // "é" en "z" zitten op AZERTY en QWERTZ op andere toetsen; als teken maakt dat niet uit.
        let invoer = ontleed(&invoer_voor("éz"));
        assert_eq!(
            invoer,
            vec![
                (0, 0xE9, false, true),
                (0, 0xE9, true, true),
                (0, u16::from(b'z'), false, true),
                (0, u16::from(b'z'), true, true),
            ]
        );
    }

    #[test]
    fn een_surrogaatpaar_als_twee_gebeurtenissen() {
        let invoer = ontleed(&invoer_voor("\u{1F600}"));
        assert_eq!(
            invoer,
            vec![
                (0, 0xD83D, false, true),
                (0, 0xD83D, true, true),
                (0, 0xDE00, false, true),
                (0, 0xDE00, true, true),
            ]
        );
    }

    #[test]
    fn geen_enter_of_tab_in_de_typroute() {
        // Een regeleinde en een tab worden een spatie; er gaat geen enkele echte toets mee.
        let stukken = typstukken("Een\r\nTwee\tdrie", TYPBLOK);
        let invoer: Vec<_> = stukken
            .iter()
            .flat_map(|s| ontleed(&invoer_voor(s)))
            .collect();
        assert!(
            invoer
                .iter()
                .all(|(vk, _, _, unicode)| *vk == 0 && *unicode)
        );
        assert!(
            !invoer
                .iter()
                .any(|(_, teken, _, _)| *teken == 0x0D || *teken == 0x09)
        );
    }

    #[test]
    fn een_blok_past_in_een_aanroep() {
        // Een vol blok is 64 gebeurtenissen; zo blijft één `SendInput` klein.
        let stukken = typstukken(&"a".repeat(100), TYPBLOK);
        assert!(stukken.iter().all(|s| invoer_voor(s).len() <= 2 * TYPBLOK));
    }

    /// Vraagt UI Automation naar het element met de focus op deze pc (alleen het soort, geen
    /// inhoud) en of het venster vooraan reageert. Drukt geen toetsen in. Handmatig draaien:
    /// `cargo test -p mondig tekstveld_ronde -- --ignored --nocapture`.
    #[test]
    #[ignore = "leest het venster met de focus op deze pc"]
    #[expect(
        clippy::print_stdout,
        reason = "de handmatige test toont wat Windows meldt"
    )]
    fn tekstveld_ronde() {
        let doel = voorgrondvenster();
        let beeld = focusbeeld(heeft_caret(doel));
        println!(
            "reageert {}, beeld {beeld:?}, tekstveld {:?}",
            reageert(doel),
            tekstveld(doel)
        );
        assert!(beeld.is_some(), "UI Automation gaf antwoord");
    }
    #[test]
    fn kopieren_eindigt_nooit_als_mislukt_terwijl_het_later_toch_gebeurt() {
        // De klembordthread begon eerst: de vrager kan het niet meer opgeven en wacht.
        let staat = AtomicU8::new(OPDRACHT_WACHT);
        assert!(neem_op(&staat));
        assert!(!geef_op(&staat));
        // De vrager gaf het eerst op ("mislukt"): de klembordthread voert hem niet meer uit.
        let staat = AtomicU8::new(OPDRACHT_WACHT);
        assert!(geef_op(&staat));
        assert!(!neem_op(&staat));
    }

    /// Een plakactie op deze thread met `venster` als eigenaar, zoals `plak` hem klaarzet,
    /// maar zonder Ctrl+V.
    fn klaargezet(venster: HWND, tekst: &str) -> Transactie {
        let bewaard = bewaar(Some(venster), BEWAAR_BUDGET)
            .ok()
            .map(|(items, volledig)| Kopie { items, volledig });
        SIGNALEN.with_borrow_mut(|s| s.tekst = Some(breed(tekst)));
        assert!(publiceer(venster).is_ok(), "publiceren");
        // SAFETY: geen voorwaarden.
        let nummer = unsafe { GetClipboardSequenceNumber() };
        SIGNALEN.with_borrow_mut(|s| {
            s.leesbewijzen.clear();
            s.eigendom_kwijt = false;
            s.volgnummer = nummer;
        });
        let mut stand = Transactiestand::nieuw(Instant::now(), false);
        stand.ingevoegd = Some(Instant::now());
        Transactie {
            stand,
            bewaard,
            aangeraakt: true,
            publicatie_mislukt: false,
            tekst: tekst.to_string(),
            doel: 0,
            melder: std::sync::Arc::new(|_| {}),
        }
    }

    /// Integratietest voor het intrekken vóór het typen, met het echte klembord: zonder
    /// lezing is de belofte daarna weg en leest een laat plakken niets, niet het dictaat en
    /// niet het oude klembord; met een lezing ervoor trekt Mondig niets in (dan was het gewoon plakken).
    /// Drukt geen toetsen in. Handmatig draaien:
    /// `cargo test -p mondig intrekken_ronde -- --ignored --nocapture`.
    #[test]
    #[ignore = "gebruikt het echte klembord van deze pc"]
    fn intrekken_ronde() {
        let venster = maak_venster().expect("venster");
        let (gebruiker, _) = bewaar(Some(venster), Duration::from_secs(5)).expect("bewaren");
        zet_als_ander(
            venster,
            u32::from(CF_UNICODETEXT.0),
            &u16_als_bytes(&breed("Oud klembord")),
        );

        // 1. Geen lezing: intrekken lukt, en een late lezing krijgt niets: niet het dictaat en
        // niet het oude klembord. Dat komt pas na het typen terug.
        let mut transactie = klaargezet(venster, "Dictaat ");
        assert_eq!(trek_in(venster, &mut transactie), Intrekking::Ingetrokken);
        assert_eq!(lees_tekst(venster), None);
        assert!(klembord_van_ons(), "het lege klembord is van Mondig");
        let mut kopie = transactie
            .bewaard
            .take()
            .expect("het oude klembord is bewaard");
        assert_eq!(
            zet_terug(venster, &mut kopie.items, OPENEN_POGINGEN).ok(),
            Some(true)
        );
        assert_eq!(lees_tekst(venster).as_deref(), Some("Oud klembord"));
        assert!(
            SIGNALEN.with_borrow(|s| s.leesbewijzen.is_empty()),
            "de late lezing vroeg niets meer aan Mondig"
        );

        // 2. Het doel las net vóór het besluit: niets intrekken, de belofte blijft staan.
        let mut transactie = klaargezet(venster, "Tweede dictaat ");
        assert_eq!(lees_tekst(venster).as_deref(), Some("Tweede dictaat "));
        assert_eq!(trek_in(venster, &mut transactie), Intrekking::NogNiet);
        assert_eq!(lees_tekst(venster).as_deref(), Some("Tweede dictaat "));

        // 3. De gebruiker kopieerde intussen zelf: dat wint.
        let mut transactie = klaargezet(venster, "Derde dictaat ");
        zet_als_ander(
            venster,
            u32::from(CF_UNICODETEXT.0),
            &u16_als_bytes(&breed("Zelf gekopieerd")),
        );
        assert_eq!(trek_in(venster, &mut transactie), Intrekking::NietVanOns);
        assert_eq!(lees_tekst(venster).as_deref(), Some("Zelf gekopieerd"));

        // 4. Een ander proces dan het doel leest (hier deze test, met een verzonnen doel): geen
        // leesbewijs, maar de tekst staat nu echt op het klembord, dus niet intrekken of typen.
        let mut transactie = klaargezet(venster, "Vierde dictaat ");
        SIGNALEN.with_borrow_mut(|s| s.doel_proces = Some(u32::MAX));
        assert_eq!(lees_tekst(venster).as_deref(), Some("Vierde dictaat "));
        assert!(SIGNALEN.with_borrow(|s| s.leesbewijzen.is_empty() && s.andere_lezer));
        assert_eq!(trek_in(venster, &mut transactie), Intrekking::NogNiet);
        assert_eq!(
            transactie.stand.uitkomst(),
            PlakUitkomst::WaarschijnlijkGeplakt
        );
        SIGNALEN.with_borrow_mut(|s| {
            s.doel_proces = None;
            s.andere_lezer = false;
        });

        // Het klembord van de gebruiker terug.
        let mut gebruiker = gebruiker;
        zet_terug(venster, &mut gebruiker, TERUGZET_POGINGEN).expect("terugzetten");
        // SAFETY: het testvenster is van deze thread.
        unsafe {
            let _ = DestroyWindow(venster);
        }
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
