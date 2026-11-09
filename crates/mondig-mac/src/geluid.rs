//! Het geluidje bij het starten en stoppen, met `NSSound`.
//!
//! De app maakt de toon als WAV in het geheugen (`geluid.rs` in de app); hier speelt hij af,
//! zonder te wachten. `NSSound` speelt asynchroon; zolang hij speelt, houden we hem vast (de
//! laatste paar), anders kan hij halverwege stoppen. Bron: developer.apple.com, `NSSound`,
//! `init(data:)` en `play()` (geraadpleegd 9-11-2026).

use std::cell::RefCell;
use std::collections::VecDeque;

use dispatch2::DispatchQueue;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::NSSound;
use objc2_foundation::NSData;

/// Zoveel geluiden houden we vast; een toon duurt 80 ms, dus dit is ruim.
const VASTHOUDEN: usize = 4;

thread_local! {
    /// De laatste geluiden, alleen op de hoofdthread gebruikt.
    static SPELEND: RefCell<VecDeque<Retained<NSSound>>> = const { RefCell::new(VecDeque::new()) };
}

/// Speelt een WAV af op de standaarduitgang, zonder te wachten.
pub fn speel(wav: &'static [u8]) {
    DispatchQueue::main().exec_async(move || {
        let data = NSData::with_bytes(wav);
        let Some(geluid) = NSSound::initWithData(NSSound::alloc(), &data) else {
            return;
        };
        if geluid.play() {
            SPELEND.with_borrow_mut(|spelend| {
                spelend.push_back(geluid);
                while spelend.len() > VASTHOUDEN {
                    spelend.pop_front();
                }
            });
        }
    });
}
