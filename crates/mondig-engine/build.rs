//! Kopieert de bibliotheken van transcribe-cpp naast de exe.
//!
//! Bij een gedeelde build (Windows x86-64 en Linux, `dynamic-backends`) staan de
//! rekenmodules als losse bestanden in de build-map van transcribe-cpp-sys, en vindt het
//! besturingssysteem ze alleen als ze naast `mondig-engine` staan. Bij een statische build
//! (Mac, rest) zijn de variabelen niet gezet en is er niets te doen.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

// De bouwcode deelt het werkproces met de app (ADR 0012, aanvulling 2-11-2026).
#[path = "../../build-hulp/bouwcode.rs"]
mod bouwcode;

/// Sleutels waarmee transcribe-cpp de mappen met bibliotheken doorgeeft.
const SLEUTELS: [&str; 2] = [
    "DEP_TRANSCRIBE_CPP_RUNTIME_DIR",
    "DEP_TRANSCRIBE_CPP_MODULE_DIR",
];

/// Is dit een bestandsnaam van een gedeelde bibliotheek (`.dll`, `.dylib`, `.so`, `.so.1`)?
fn is_bibliotheek(naam: &str) -> bool {
    let extensie = Path::new(naam).extension().and_then(|e| e.to_str());
    extensie.is_some_and(|e| {
        ["dll", "dylib", "so"]
            .iter()
            .any(|b| e.eq_ignore_ascii_case(b))
    }) || naam.contains(".so.")
}

/// Staat er in `doel` al een bestand met dezelfde grootte en wijzigingstijd als `bron`?
/// Dan hoeft het niet opnieuw. Dat is meer dan snelheid: draait de ontwikkelversie van de
/// app, dan zitten de bibliotheken naast de exe op slot en faalt een kopie (os error 32).
fn is_al_gelijk(bron: &Path, doel: &Path) -> bool {
    let (Ok(b), Ok(d)) = (fs::metadata(bron), fs::metadata(doel)) else {
        return false;
    };
    match (b.modified(), d.modified()) {
        (Ok(tb), Ok(td)) => b.len() == d.len() && tb == td,
        _ => false,
    }
}

/// Kopieert één bibliotheek en neemt de wijzigingstijd van de bron over, zodat
/// [`is_al_gelijk`] hem de volgende keer herkent. Op Windows doet `fs::copy` dat zelf al,
/// op Linux niet.
fn kopieer_bibliotheek(bron: &Path, doel: &Path) -> std::io::Result<()> {
    fs::copy(bron, doel)?;
    let tijd = fs::metadata(bron)?.modified()?;
    fs::File::options()
        .write(true)
        .open(doel)?
        .set_modified(tijd)
}

/// Kopieert alle bibliotheken uit `bron` naar `doel`, behalve de bestanden die er al
/// ongewijzigd staan. Geeft het aantal bibliotheken terug dat nu in `doel` staat.
fn kopieer_bibliotheken(bron: &Path, doel: &Path) -> Result<usize, String> {
    let map =
        fs::read_dir(bron).map_err(|f| format!("{} is niet te lezen: {f}", bron.display()))?;
    let mut aantal = 0;
    for item in map.flatten() {
        let pad = item.path();
        let naam = pad.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if pad.is_file() && is_bibliotheek(naam) {
            let doelpad = doel.join(naam);
            if !is_al_gelijk(&pad, &doelpad) {
                kopieer_bibliotheek(&pad, &doelpad)
                    .map_err(|f| format!("{} kopiëren mislukt: {f}", pad.display()))?;
            }
            aantal += 1;
        }
    }
    Ok(aantal)
}

/// Zoekt de map met de exe en kopieert de bibliotheken erheen.
fn kopieer_alles() -> Result<(), String> {
    // OUT_DIR is target/<profiel>/build/<pakket>/out; de exe staat drie niveaus hoger.
    let exe_map = env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .and_then(|p| p.ancestors().nth(3).map(Path::to_path_buf))
        .ok_or("OUT_DIR ontbreekt of is te kort")?;
    let mut gezet = false;
    let mut gekopieerd = 0;
    for sleutel in SLEUTELS {
        if let Some(bron) = env::var_os(sleutel) {
            gezet = true;
            gekopieerd += kopieer_bibliotheken(Path::new(&bron), &exe_map)
                .map_err(|f| format!("{sleutel}: {f}"))?;
        }
    }
    // Zonder bibliotheken naast de exe start het werkproces niet: liever nu falen dan
    // bij de gebruiker (README van transcribe-cpp, sectie Packaging).
    if gezet && gekopieerd == 0 {
        return Err(
            "transcribe-cpp meldt gedeelde bibliotheken, maar er viel niets naast de exe te zetten"
                .to_string(),
        );
    }
    Ok(())
}

fn main() {
    // De bouwcode die het werkproces aan de app meldt; ook de bibliotheken van de motor
    // en de kern horen erbij, vandaar `-dirty` bij een wijziging in een van de crates.
    bouwcode::zet_commit(&["src", "../../crates"]);
    for sleutel in SLEUTELS {
        println!("cargo:rerun-if-env-changed={sleutel}");
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        koppel_clang_runtime();
    }
    if let Err(f) = kopieer_alles() {
        eprintln!("build.rs van mondig-engine: {f}");
        std::process::exit(1);
    }
}

/// Mac: koppelt de runtime van clang, waar `__isPlatformVersionAtLeast` in zit.
///
/// ggml-metal schermt nieuwere Metal-functies af met `@available`. Ligt de minimale
/// macOS-versie (`MACOSX_DEPLOYMENT_TARGET`) lager dan de SDK, dan maakt clang daar een
/// aanroep naar `libclang_rt.osx.a` van, en die koppelt rustc niet mee (`-nodefaultlibs`).
/// Zelfde oplossing als Handy (`src-tauri/build.rs`, `link_clang_rt_osx`, MIT).
fn koppel_clang_runtime() {
    let uit = std::process::Command::new("clang")
        .arg("--print-resource-dir")
        .output();
    let map = match uit {
        Ok(uit) if uit.status.success() => {
            Path::new(String::from_utf8_lossy(&uit.stdout).trim()).join("lib/darwin")
        }
        _ => {
            eprintln!("build.rs van mondig-engine: `clang --print-resource-dir` mislukte");
            std::process::exit(1);
        }
    };
    if !map.join("libclang_rt.osx.a").is_file() {
        eprintln!(
            "build.rs van mondig-engine: libclang_rt.osx.a niet gevonden in {}",
            map.display()
        );
        std::process::exit(1);
    }
    println!("cargo:rustc-link-search=native={}", map.display());
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
}
