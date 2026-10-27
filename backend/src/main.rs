// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod stats;

use bridge::events::{
    do_send_connection_status, listen_connect_elm, listen_decode_vin, listen_send_ports,
};
use obdium::{
    vin::{vpic_db_path, APP_DATA_DIR},
    OBD,
};
use stats::{
    critical_frequency_calls, custom_pid_calls, frequent_calls, high_frequency_calls,
    less_frequent_calls, once_calls, oxygen_sensors,
};

use std::{
    fs::File,
    io::{copy, BufReader, BufWriter, Error},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::sleep,
    time::Duration,
};
use tauri::{async_runtime::spawn, Emitter, Listener, Manager, WebviewWindow};

use crate::bridge::{events::listen_track_custom_pid, FrontendNotification};

use xz2::read::XzDecoder;

/// Loads the `vpic.sqlite` file by decoding the
/// `vpic.sqlite.xz` file.
///
/// This alternative was created to avoid having to break the bank
/// paying for Git LFS to store a single, static 1.3GB file.
///
/// The VPIC sqlite database is critical for receiving information from a VIN
/// in the VIN decoder.
fn try_load_vpic_database(
    window: &Arc<WebviewWindow>,
    db_path: PathBuf,
    xz_path: PathBuf,
) -> Result<(), Error> {
    const EXPECTED_VPIC_SQLITE_FILE_SIZE_BYTES: u64 = 1453146112;

    if let Ok(vpic_sqlite) = File::open(&db_path) {
        // vpic sqlite file exists. verify the size
        if let Ok(metadata) = vpic_sqlite.metadata() {
            if metadata.len() == EXPECTED_VPIC_SQLITE_FILE_SIZE_BYTES {
                println!("[LOAD_VPIC_DATABASE] `vpic.sqlite` already loaded.");
                return Ok(());
            }
        }
    }

    println!(
        "[LOAD_VPIC_DATABASE] Loading `vpic.sqlite` - decompressing data from `vpic.sqlite.xz"
    );
    let _ = window.emit("display-notification", FrontendNotification {
        title: "VIN Decoder Database",
        description: "Loading local VIN (VPIC) database (SQLite) for the first and only time - please wait before using the VIN decoder."
    });

    // io files
    let input = File::open(xz_path)?;
    let output = File::create(db_path)?;

    let reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);

    let mut decompressor = XzDecoder::new(reader);
    if copy(&mut decompressor, &mut writer).is_ok() {
        println!("[LOAD_VPIC_DATABASE] Successfully decompressed and loaded `vpic.sqlite.xz` into `vpic.sqlite`");
        let _ = window.emit("display-notification", FrontendNotification {
            title: "VIN Decoder Database",
            description: "Successfully loaded local VIN (VPIC) database - you may now decode VINs as expected."
        });
    } else {
        let _ = window.emit("display-notification", FrontendNotification {
            title: "VIN Deocder Database",
            description: "Failed loading database for the first time - VIN decoding may not function properly. Try restarting the app or opening a issue on the GitHub (github.com/provrb/obdium/issues/."
        });
    }

    Ok(())
}

fn track_data(window: &Arc<WebviewWindow>, obd: &Arc<Mutex<OBD>>) {
    critical_frequency_calls(window, obd);
    high_frequency_calls(window, obd);
    frequent_calls(window, obd);
    less_frequent_calls(window, obd);
    oxygen_sensors(window, obd);
    once_calls(window, obd);
    custom_pid_calls(window, obd);
}

fn connect_obd(window: &WebviewWindow, port: String, baud_rate: u32, protocol: u8) -> Option<OBD> {
    // Try to connect obd
    let mut obd = OBD::new();

    match obd.connect(&port, baud_rate, protocol) {
        Ok(()) => {
            let band = obd.serial_port_baud_rate().unwrap_or_default();
            let port = obd.serial_port_name().unwrap_or_default();

            do_send_connection_status(
                window,
                &obd,
                format!("Connected to port {port} on {band} baud"),
                true,
            );

            Some(obd)
        }
        Err(error) => {
            do_send_connection_status(
                window,
                &obd,
                format!("Failed to connect. Error: {error}"),
                false,
            );

            None
        }
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            bridge::recordings::recording_start,
            bridge::recordings::recording_stop,
            bridge::recordings::recording_status,
            bridge::recordings::recording_list,
            bridge::recordings::recording_metrics,
            bridge::recordings::recording_samples,
            bridge::recordings::recording_summary,
            bridge::recordings::recording_assign_vehicle,
            bridge::recordings::recording_delete_preview,
            bridge::recordings::recording_delete,
            bridge::ai::ai_preferences_get,
            bridge::ai::ai_preferences_set,
            bridge::ai::ai_key_status,
            bridge::ai::ai_key_set,
            bridge::ai::ai_key_remove,
            bridge::ai::ai_disclosure_accept,
            bridge::ai::ai_models_refresh,
            bridge::ai::ai_model_test,
            bridge::ai::ai_conversation_create,
            bridge::ai::ai_conversation_list,
            bridge::ai::ai_conversation_get,
            bridge::ai::ai_conversation_delete,
            bridge::ai::ai_message_send,
            bridge::ai::ai_turn_cancel
        ])
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            let frontend_ready = Arc::new(AtomicBool::new(false));

            // load app data dir for the first time
            let app_data_dir = app.path().app_data_dir()?;
            APP_DATA_DIR.get_or_init(|| app_data_dir.clone());
            std::fs::create_dir_all(&app_data_dir)?;
            match obdium::recording::RecorderHandle::open(&app_data_dir.join("diagnostics.sqlite3"))
            {
                Ok(recorder) => {
                    if let Ok(service) = obdium::ai::AssistantService::network(recorder.clone()) {
                        let _ = obdium::ai::AI_SERVICE.set(service);
                    }
                    let _ = obdium::recording::ACTIVE_RECORDER.set(recorder);
                }
                Err(_) => eprintln!(
                    "Local recording storage is unavailable; scanner features remain available."
                ),
            }
            let db_path = vpic_db_path().unwrap();
            let xz_path = app
                .path()
                .resolve("data/vpic.sqlite.xz", tauri::path::BaseDirectory::Resource)
                .unwrap();

            spawn(async move {
                // Detect when the frontend is loaded
                let frontend_ready_listener = Arc::clone(&frontend_ready);
                window.listen("frontend-loaded", move |_| {
                    frontend_ready_listener.store(true, Ordering::SeqCst);
                });

                let window_arc = Arc::new(window);

                while !frontend_ready.load(Ordering::SeqCst) {
                    sleep(Duration::from_millis(100));
                }

                listen_decode_vin(&window_arc);
                listen_send_ports(&window_arc);
                listen_track_custom_pid(&window_arc);
                listen_connect_elm(&window_arc);

                sleep(Duration::from_secs(1));

                // load vpic database
                let _ = try_load_vpic_database(&window_arc, db_path, xz_path);
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if let Some(service) = obdium::ai::AI_SERVICE.get() {
                    service.cancel_all();
                }
                if let Some(handle) = obdium::recording::ACTIVE_RECORDER.get() {
                    if handle
                        .call(|r| {
                            if r.status().recording {
                                r.stop("shutdown", obdium::recording::Timestamp::now())?;
                            }
                            Ok(())
                        })
                        .is_err()
                    {
                        api.prevent_close();
                        let _ = window.emit(
                            "display-notification",
                            FrontendNotification {
                                title: "Recording could not be saved",
                                description:
                                    "Stop the recording and check local storage before closing.",
                            },
                        );
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
