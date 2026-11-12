use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use serde_json::{Value, json};

use tauri::{
    AppHandle, Emitter, Manager, State,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

mod config;
mod config_api;
mod capture;
mod audio;
mod ai;
mod window;
use window::{apply_stealth, apply_window_state, set_click_through, set_topmost, WindowState};

pub struct AppState {
    click_through: Arc<AtomicBool>,
    recording: Arc<AtomicBool>,
    active_request: Mutex<Option<Arc<AtomicBool>>>,
    conversation_enabled: AtomicBool,
    conversation: Mutex<Vec<Value>>,
    pub config: Mutex<config::Config>,
}

fn keep_settings_hidden_on_close(settings: &WebviewWindow) {
    let window_to_hide = settings.clone();
    settings.clone().on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if let Err(error) = window_to_hide.hide() {
                eprintln!("Failed to hide settings window: {error}");
            }
        }
    });
}

#[tauri::command]
fn set_opacity(app: AppHandle, value: f32) -> Result<(), String> {
    let clamped = value.clamp(0.1, 1.0);
    app.emit("opacity-changed", clamped)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn set_window_state(app: AppHandle, state: WindowState) -> Result<(), String> {
    apply_window_state(&app, state).map_err(|error| error.to_string())
}

#[tauri::command]
fn toggle_click_through(app: AppHandle) -> Result<bool, String> {
    toggle_click_through_for_app(&app)
}

#[tauri::command]
fn get_click_through(app: AppHandle) -> Result<bool, String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "App not ready yet".to_string())?;
    Ok(state.click_through.load(Ordering::SeqCst))
}

#[tauri::command]
fn capture_screen(app: AppHandle) -> Result<String, String> {
    let image = capture::capture_primary_screen()?;
    app.emit("screen-captured", &image)
        .map_err(|error| error.to_string())?;
    let config = app.state::<AppState>().config.lock()
        .map_err(|_| "Config lock poisoned".to_string())?.clone();
    start_vision_request(app.clone(), config, image.clone(), conversation_history(&app));
    Ok(image)
}

#[tauri::command]
fn ask_ai(app: AppHandle, text: String) -> Result<(), String> {
    let config = app.state::<AppState>().config.lock()
        .map_err(|_| "Config lock poisoned".to_string())?.clone();
    start_text_request(app.clone(), config, text, conversation_history(&app));
    Ok(())
}

#[tauri::command]
fn get_conversation_mode(state: State<'_, AppState>) -> bool {
    state.conversation_enabled.load(Ordering::SeqCst)
}

#[tauri::command]
fn set_conversation_mode(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<bool, String> {
    state.conversation_enabled.store(enabled, Ordering::SeqCst);
    if !enabled {
        state.conversation.lock()
            .map_err(|_| "Conversation lock poisoned".to_string())?
            .clear();
    }
    app.emit("conversation-mode-changed", enabled)
        .map_err(|error| error.to_string())?;
    Ok(enabled)
}

#[tauri::command]
fn new_conversation(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(cancel) = state.active_request.lock()
        .map_err(|_| "Request lock poisoned".to_string())?.take()
    {
        cancel.store(true, Ordering::SeqCst);
    }
    state.conversation.lock()
        .map_err(|_| "Conversation lock poisoned".to_string())?
        .clear();
    Ok(())
}

#[tauri::command]
fn cancel_ai(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(cancel) = state.active_request.lock()
        .map_err(|_| "Request lock poisoned".to_string())?.take()
    {
        cancel.store(true, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
fn toggle_overlay(app: AppHandle) -> Result<bool, String> {
    let live = app
        .get_webview_window("live")
        .ok_or_else(|| "Live window not found".to_string())?;
    let visible = live.is_visible().map_err(|error| error.to_string())?;
    if visible {
        live.hide().map_err(|error| error.to_string())?;
    } else {
        live.show().map_err(|error| error.to_string())?;
        live.set_focus().map_err(|error| error.to_string())?;
    }
    set_tray_visible(&app, !visible)?;
    Ok(!visible)
}

fn set_tray_visible(app: &AppHandle, visible: bool) -> Result<(), String> {
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_icon(if visible {
            Some(tauri::include_image!("icons/icon.png"))
        } else {
            None
        })
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn start_request(app: &AppHandle) -> Arc<AtomicBool> {
    let cancel = Arc::new(AtomicBool::new(false));
    if let Some(previous) = app.state::<AppState>().active_request.lock().ok()
        .and_then(|mut active| active.replace(Arc::clone(&cancel)))
    {
        previous.store(true, Ordering::SeqCst);
    }
    cancel
}

fn finish_request(app: &AppHandle, cancel: &Arc<AtomicBool>) {
    if let Ok(mut active) = app.state::<AppState>().active_request.lock() {
        if active.as_ref().is_some_and(|current| Arc::ptr_eq(current, cancel)) {
            active.take();
        }
    }
}

fn conversation_history(app: &AppHandle) -> Vec<Value> {
    let state = app.state::<AppState>();
    if !state.conversation_enabled.load(Ordering::SeqCst) {
        return Vec::new();
    }
    state.conversation.lock().map(|history| history.clone()).unwrap_or_default()
}

fn append_conversation(app: &AppHandle, user_message: Value, assistant_text: String) {
    let state = app.state::<AppState>();
    if !state.conversation_enabled.load(Ordering::SeqCst) {
        return;
    }
    if let Ok(mut history) = state.conversation.lock() {
        history.push(user_message);
        history.push(json!({"role": "assistant", "content": assistant_text}));
    }
}

fn start_text_request(app: AppHandle, config: config::Config, text: String, history: Vec<Value>) {
    let cancel = start_request(&app);
    let user_message = json!({"role": "user", "content": text.clone()});
    let _ = app.emit("ai-start", ());
    std::thread::spawn(move || {
        let result = ai::chat::stream_text(&app, &config, text, history, Arc::clone(&cancel));
        if cancel.load(Ordering::SeqCst) {
            let _ = app.emit("ai-done", ());
        } else {
            match result {
                Err(message) => {
                    let _ = app.emit("ai-error", serde_json::json!({"message": message}));
                }
                Ok(response) => {
                    append_conversation(&app, user_message, response);
                    let _ = app.emit("ai-done", ());
                }
            }
        }
        finish_request(&app, &cancel);
    });
}

fn start_vision_request(app: AppHandle, config: config::Config, image: String, history: Vec<Value>) {
    let cancel = start_request(&app);
    let user_message = json!({"role": "user", "content": [
        {"type": "text", "text": "Baca pertanyaan di layar ini dan beri jawaban singkat, jelas, siap diucapkan."},
        {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{}", image)}}
    ]});
    let _ = app.emit("ai-start", ());
    std::thread::spawn(move || {
        let result = ai::chat::stream_image(&app, &config, image, history, Arc::clone(&cancel));
        if cancel.load(Ordering::SeqCst) {
            let _ = app.emit("ai-done", ());
        } else {
            match result {
                Err(message) => {
                    let _ = app.emit("ai-error", serde_json::json!({"message": message}));
                }
                Ok(response) => {
                    append_conversation(&app, user_message, response);
                    let _ = app.emit("ai-done", ());
                }
            }
        }
        finish_request(&app, &cancel);
    });
}

fn start_audio_request(app: AppHandle, config: config::Config, wav: String, history: Vec<Value>) {
    let cancel = start_request(&app);
    let user_message = json!({"role": "user", "content": [
        {"type": "text", "text": "Dengarkan audio ini, pahami pertanyaannya, lalu berikan jawaban yang sesuai dan lengkap."},
        {"type": "input_audio", "input_audio": {"data": wav.clone(), "format": "wav"}}
    ]});
    let _ = app.emit("ai-start", ());
    std::thread::spawn(move || {
        let result = ai::chat::stream_audio(&app, &config, wav, history, Arc::clone(&cancel));
        if cancel.load(Ordering::SeqCst) {
            let _ = app.emit("ai-done", ());
        } else {
            match result {
                Err(message) => {
                    let _ = app.emit("ai-error", serde_json::json!({"message": message}));
                }
                Ok(response) => {
                    append_conversation(&app, user_message, response);
                    let _ = app.emit("ai-done", ());
                }
            }
        }
        finish_request(&app, &cancel);
    });
}

#[tauri::command]
fn toggle_recording(app: AppHandle) -> Result<bool, String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "App not ready yet".to_string())?;
    let recording = !state.recording.load(Ordering::SeqCst);
    state.recording.store(recording, Ordering::SeqCst);
    app.emit("audio-recording", recording)
        .map_err(|error| error.to_string())?;
    if recording {
        let app = app.clone();
        let stop = Arc::clone(&state.recording);
        let config = state
            .config
            .lock()
            .map_err(|_| "Config lock poisoned".to_string())?
            .clone();
        let device_name = config.mic_device.clone();
        let audio_source = config.audio_source.clone();
        std::thread::spawn(move || {
            let result = audio::record(device_name.as_deref(), &audio_source, stop.clone());
            stop.store(false, Ordering::SeqCst);
            match result {
                Ok(wav) => {
                    let _ = app.emit("audio-recorded", &wav);
                    start_audio_request(app.clone(), config, wav, conversation_history(&app));
                    let _ = app.emit("audio-recording", false);
                }
                Err(error) => {
                    let _ = app.emit("audio-error", error);
                    let _ = app.emit("audio-recording", false);
                }
            }
        });
    }
    Ok(recording)
}

#[tauri::command]
fn open_settings(app: AppHandle) -> Result<(), String> {
    let settings = match app.get_webview_window("settings") {
        Some(settings) => settings,
        None => WebviewWindowBuilder::new(
            &app,
            "settings",
            WebviewUrl::App("settings.html".into()),
        )
        .title("AnyQ Settings")
        .inner_size(680.0, 520.0)
        .visible(false)
        .build()
        .map_err(|error| error.to_string())?,
    };
    keep_settings_hidden_on_close(&settings);
    settings.show().map_err(|error| error.to_string())?;
    settings.set_focus().map_err(|error| error.to_string())
}

#[tauri::command]
fn open_help(app: AppHandle) -> Result<(), String> {
    let help = match app.get_webview_window("help") {
        Some(help) => help,
        None => WebviewWindowBuilder::new(
            &app,
            "help",
            WebviewUrl::App("help.html".into()),
        )
        .title("AnyQ Help")
        .inner_size(620.0, 620.0)
        .visible(false)
        .build()
        .map_err(|error| error.to_string())?,
    };
    keep_settings_hidden_on_close(&help);
    help.show().map_err(|error| error.to_string())?;
    help.set_focus().map_err(|error| error.to_string())
}

fn toggle_click_through_for_app(app: &AppHandle) -> Result<bool, String> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| "App not ready yet".to_string())?;
    let next = !state.click_through.load(Ordering::SeqCst);

    let live = app
        .get_webview_window("live")
        .ok_or_else(|| "Live window not found".to_string())?;

    // Emit BEFORE activating click-through so the webview can still receive
    // and process the event. When next=true the webview would stop receiving
    // events the moment set_ignore_cursor_events(true) is called.
    live.emit("clickthrough-changed", next)
        .map_err(|error| error.to_string())?;

    // Small delay so the webview JS has time to process the event before the
    // window becomes non-interactive.
    if next {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    set_click_through(&live, next).map_err(|error| error.to_string())?;
    state.click_through.store(next, Ordering::SeqCst);
    Ok(next)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let live = app
                .get_webview_window("live")
                .expect("live window should exist");

            let loaded_config = config::load().unwrap_or_else(|error| {
                eprintln!("Failed to load config: {error}");
                config::Config::default()
            });
            let state = AppState {
                // Start interactive (OFF) so buttons are immediately clickable.
                // User activates click-through manually via hotkey or Toggle button.
                click_through: Arc::new(AtomicBool::new(false)),
                recording: Arc::new(AtomicBool::new(false)),
                active_request: Mutex::new(None),
                conversation_enabled: AtomicBool::new(false),
                conversation: Mutex::new(Vec::new()),
                config: Mutex::new(loaded_config.clone()),
            };
            app.manage(state);

            apply_window_state(app.handle(), WindowState::Collapsed)
                .expect("failed to initialize collapsed window state");
            apply_stealth(&live, loaded_config.stealth)
                .expect("failed to apply content protection");
            set_click_through(&live, false).expect("failed to set click-through");
            set_topmost(&live, true).expect("failed to keep window on top");

            let settings_item = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
            let help_item = MenuItemBuilder::with_id("help", "Help").build(app)?;
            let toggle_item =
                MenuItemBuilder::with_id("toggle-overlay", "Toggle Overlay").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&help_item, &settings_item, &toggle_item, &quit_item])
                .build()?;
            let mut tray = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .tooltip("AnyQ")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "settings" => {
                        if let Err(error) = open_settings(app.clone()) {
                            eprintln!("Failed to open settings: {error}");
                        }
                    }
                    "help" => {
                        if let Err(error) = open_help(app.clone()) {
                            eprintln!("Failed to open help: {error}");
                        }
                    }
                    "toggle-overlay" => {
                        if let Err(error) = toggle_click_through_for_app(app) {
                            let _ = app.emit("clickthrough-error", error);
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                });
            tray = tray.icon(tauri::include_image!("icons/icon.png"));
            tray.build(app)?;

            if let Some(settings) = app.get_webview_window("settings") {
                keep_settings_hidden_on_close(&settings);
            }

            if let Err(error) = app.global_shortcut().register("Ctrl+Shift+Space") {
                eprintln!("Failed to register Ctrl+Shift+Space: {error}");
            }
            if let Err(error) = app.global_shortcut().register("Ctrl+Shift+S") {
                eprintln!("Failed to register Ctrl+Shift+S: {error}");
            }
            if let Err(error) = app.global_shortcut().register("Ctrl+Shift+A") {
                eprintln!("Failed to register Ctrl+Shift+A: {error}");
            }
            if let Err(error) = app.global_shortcut().register("Esc") {
                eprintln!("Failed to register Esc: {error}");
            }
            if let Err(error) = app.global_shortcut().register("Ctrl+Shift+H") {
                eprintln!("Failed to register Ctrl+Shift+H: {error}");
            }
            if let Err(error) = app.global_shortcut().register("Ctrl+Shift+I") {
                eprintln!("Failed to register Ctrl+Shift+I: {error}");
            }
            if let Err(error) = app.global_shortcut().register("Ctrl+Shift+Q") {
                eprintln!("Failed to register Ctrl+Shift+Q: {error}");
            }

            Ok(())
        })
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    let click_through_id = "Ctrl+Shift+Space"
                        .parse::<Shortcut>()
                        .expect("valid click-through shortcut")
                        .id();
                    let capture_id = "Ctrl+Shift+S"
                        .parse::<Shortcut>()
                        .expect("valid capture shortcut")
                        .id();
                    let recording_id = "Ctrl+Shift+A"
                        .parse::<Shortcut>()
                        .expect("valid recording shortcut")
                        .id();
                    let escape_id = "Esc"
                        .parse::<Shortcut>()
                        .expect("valid cancel shortcut")
                        .id();
                    let hide_id = "Ctrl+Shift+H"
                        .parse::<Shortcut>()
                        .expect("valid hide shortcut")
                        .id();
                    let focus_chat_id = "Ctrl+Shift+I"
                        .parse::<Shortcut>()
                        .expect("valid focus chat shortcut")
                        .id();
                    let quit_id = "Ctrl+Shift+Q"
                        .parse::<Shortcut>()
                        .expect("valid quit shortcut")
                        .id();

                    if shortcut.id() == quit_id {
                        app.exit(0);
                    } else if shortcut.id() == focus_chat_id {
                        if let Some(live) = app.get_webview_window("live") {
                            let _ = live.show();
                            let _ = live.set_focus();
                            let _ = set_tray_visible(app, true);
                            let _ = live.emit("focus-chat", ());
                        }
                    } else if shortcut.id() == capture_id {
                        if let Err(error) = capture_screen(app.clone()) {
                            let _ = app.emit("capture-error", error);
                        }
                    } else if shortcut.id() == recording_id {
                        if let Err(error) = toggle_recording(app.clone()) {
                            let _ = app.emit("audio-error", error);
                        }
                    } else if shortcut.id() == click_through_id
                        && let Err(error) = toggle_click_through_for_app(app)
                    {
                        let _ = app.emit("clickthrough-error", error);
                    }
                    else if shortcut.id() == hide_id {
                        let _ = app.emit("window-state-requested", "Expanded");
                    } else if shortcut.id() == escape_id {
                        if let Ok(mut active) = app.state::<AppState>().active_request.lock()
                            && let Some(cancel) = active.take()
                        {
                            cancel.store(true, Ordering::SeqCst);
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            set_opacity,
            set_window_state,
            toggle_click_through,
            get_click_through,
            capture_screen,
            ask_ai,
            cancel_ai,
            toggle_overlay,
            toggle_recording,
            open_settings,
            open_help,
            get_conversation_mode,
            set_conversation_mode,
            new_conversation,
            config_api::get_config,
            config_api::save_config,
            config_api::set_api_key,
            config_api::has_api_key,
            config_api::list_mic_devices,
            config_api::test_connection
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
