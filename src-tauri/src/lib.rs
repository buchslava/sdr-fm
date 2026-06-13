mod config;
mod dsp;
#[cfg(target_os = "macos")]
mod macos_menu;
mod macos_spellcheck;
mod sdr;

use config::{Station, load_stations, save_stations};
use sdr::SdrPlayer;

fn init_logging() {
    if std::env::var_os("FUTURESDR_LOG").is_none() {
        unsafe { std::env::set_var("FUTURESDR_LOG", "off") };
    }
    if std::env::var_os("RUST_LOG").is_none() {
        unsafe { std::env::set_var("RUST_LOG", "off") };
    }
}

#[tauri::command]
fn start_fm(frequency_khz: u32, player: tauri::State<'_, SdrPlayer>) -> Result<String, String> {
    player.start(frequency_khz)
}

#[tauri::command]
fn stop_fm(player: tauri::State<'_, SdrPlayer>) -> Result<(), String> {
    player.stop()
}

#[tauri::command]
fn get_stations() -> Vec<Station> {
    load_stations()
}

#[tauri::command]
fn set_stations(stations: Vec<Station>) -> Result<(), String> {
    save_stations(&stations)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    macos_spellcheck::disable_webview_spellcheck();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(SdrPlayer::default())
        .invoke_handler(tauri::generate_handler![
            start_fm,
            stop_fm,
            get_stations,
            set_stations
        ]);

    #[cfg(target_os = "macos")]
    let builder = builder.menu(|app| macos_menu::default_menu(app));

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
