mod dsp;
mod sdr;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(SdrPlayer::default())
        .invoke_handler(tauri::generate_handler![start_fm, stop_fm])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
