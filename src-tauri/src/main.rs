// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(e) = forza_designer_lib::run() {
        eprintln!("Forza Designer failed to start: {e}");
        std::process::exit(1);
    }
}
