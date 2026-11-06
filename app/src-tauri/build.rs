use std::time::{SystemTime, UNIX_EPOCH};

// De commit-logica deelt de app met het werkproces (ADR 0012, aanvulling 2-11-2026).
#[path = "../../build-hulp/bouwcode.rs"]
mod bouwcode;

use bouwcode::git;

/// Zet de korte commit en de datum als omgevingsvariabelen voor `env!` in de app
/// (ADR 0016). De datum is die van de commit (een bouwdatum zou door de cache van Cargo
/// verouderen). Zonder git (bijvoorbeeld in een tarball) wordt de datum die van vandaag;
/// de build faalt daar nooit op.
fn main() {
    bouwcode::zet_commit(&["src", "../src", "../../crates"]);
    println!("cargo:rustc-env=MONDIG_BOUW_DATUM={}", bouwdatum());
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    tauri_build::build();
}

/// De datum voor in de app als `jjjj-mm-dd`: `SOURCE_DATE_EPOCH` (herhaalbare build) als die
/// gezet is, anders de commitdatum, anders (zonder git) vandaag in UTC.
fn bouwdatum() -> String {
    if let Some(seconden) = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
    {
        return datum_uit_seconden(seconden);
    }
    if let Some(datum) = git(&["log", "-1", "--format=%cs"]) {
        return datum;
    }
    let nu = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    datum_uit_seconden(nu)
}

/// Seconden sinds 1970 als `jjjj-mm-dd` (UTC).
fn datum_uit_seconden(seconden: u64) -> String {
    let dagen = i64::try_from(seconden / 86_400).unwrap_or(0);
    // Burgerlijke datum uit dagen sinds 1970 (algoritme van Howard Hinnant).
    let z = dagen + 719_468;
    let tijdperk = z.div_euclid(146_097);
    let dag_van_tijdperk = z.rem_euclid(146_097);
    let jaar_van_tijdperk = (dag_van_tijdperk - dag_van_tijdperk / 1_460
        + dag_van_tijdperk / 36_524
        - dag_van_tijdperk / 146_096)
        / 365;
    let dag_van_jaar = dag_van_tijdperk
        - (365 * jaar_van_tijdperk + jaar_van_tijdperk / 4 - jaar_van_tijdperk / 100);
    let mp = (5 * dag_van_jaar + 2) / 153;
    let dag = dag_van_jaar - (153 * mp + 2) / 5 + 1;
    let maand = if mp < 10 { mp + 3 } else { mp - 9 };
    let jaar = jaar_van_tijdperk + tijdperk * 400 + i64::from(maand <= 2);
    format!("{jaar:04}-{maand:02}-{dag:02}")
}
