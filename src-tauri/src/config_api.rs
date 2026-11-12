use crate::{config, AppState};
use crate::window::apply_stealth;
use cpal::traits::{DeviceTrait, HostTrait};
use keyring::Entry;
use reqwest::blocking::Client;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

const KEYRING_SERVICE: &str = "anyq";
const LEGACY_KEYRING_SERVICE: &str = "overlay-ai";
const KEYRING_USER: &str = "api_key";

#[derive(Debug, Serialize)]
pub struct Microphone {
    pub name: String,
}

#[tauri::command]
pub fn get_config() -> Result<config::Config, String> {
    config::load().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_config(
    app: AppHandle,
    state: State<'_, AppState>,
    mut new_config: config::Config,
) -> Result<(), String> {
    new_config.normalize();
    config::save(&new_config).map_err(|error| error.to_string())?;

    {
        let mut current = state.config.lock().map_err(|_| "Config lock poisoned".to_string())?;
        *current = new_config.clone();
    }

    let live = app
        .get_webview_window("live")
        .ok_or_else(|| "Live window not found".to_string())?;
    apply_stealth(&live, new_config.stealth).map_err(|error| error.to_string())?;
    live.emit("config-changed", &new_config)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn set_api_key(api_key: String) -> Result<(), String> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|error| error.to_string())?
            .delete_credential()
            .map_err(|error| error.to_string());
    }
    Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|error| error.to_string())?
        .set_password(trimmed)
        .map_err(|error| error.to_string())
}

fn get_api_key() -> Result<String, String> {
    match Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|error| error.to_string())?
        .get_password()
    {
        Ok(value) => Ok(value),
        Err(keyring::Error::NoEntry) => Entry::new(LEGACY_KEYRING_SERVICE, KEYRING_USER)
            .map_err(|error| error.to_string())?
            .get_password()
            .map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub fn has_api_key() -> Result<bool, String> {
    match get_api_key() {
        Ok(value) => Ok(!value.is_empty()),
        Err(_) => Ok(false),
    }
}

#[tauri::command]
pub fn list_mic_devices() -> Result<Vec<Microphone>, String> {
    let host = cpal::default_host();
    let devices = host.input_devices().map_err(|error| error.to_string())?;
    let mut result = Vec::new();
    for device in devices {
        let name = device
            .description()
            .map_err(|error| error.to_string())?
            .name()
            .to_string();
        result.push(Microphone { name });
    }
    Ok(result)
}

#[tauri::command]
pub fn test_connection(state: State<'_, AppState>) -> Result<String, String> {
    let base_url = state
        .config
        .lock()
        .map_err(|_| "Config lock poisoned".to_string())?
        .base_url
        .clone();
    let api_key = get_api_key()
        .map_err(|_| "API key belum disimpan".to_string())?;
    let url = format!("{base_url}/models");
    let response = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?
        .get(url)
        .bearer_auth(api_key)
        .send()
        .map_err(|error| error.to_string())?;
    if response.status().is_success() {
        Ok("Connection successful.".to_string())
    } else {
        Err(format!("API returned HTTP {}.", response.status()))
    }
}
