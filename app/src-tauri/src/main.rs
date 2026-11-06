// Voorkomt een extra consolevenster op Windows in release-builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    match mondig_lib::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(fout) => {
            eprintln!("Mondig kon niet starten: {fout}");
            std::process::ExitCode::FAILURE
        }
    }
}
