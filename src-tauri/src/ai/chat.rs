use crate::config::Config;
use keyring::Entry;
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::{
    io::Read,
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tauri::{AppHandle, Emitter};

pub fn stream_text(
    app: &AppHandle,
    config: &Config,
    user_text: String,
    history: Vec<Value>,
    cancel: Arc<AtomicBool>,
) -> Result<String, String> {
    let messages = build_messages(config, history, json!({"role": "user", "content": user_text}));
    stream(app, config, messages, cancel)
}

pub fn stream_image(
    app: &AppHandle,
    config: &Config,
    image_base64: String,
    history: Vec<Value>,
    cancel: Arc<AtomicBool>,
) -> Result<String, String> {
    let user_message = json!({"role": "user", "content": [
            {"type": "text", "text": "Baca pertanyaan di layar ini dan beri jawaban singkat, jelas, siap diucapkan."},
            {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{image_base64}")}}
        ]});
    let messages = build_messages(config, history, user_message);
    stream(app, config, messages, cancel)
}

pub fn stream_audio(
    app: &AppHandle,
    config: &Config,
    wav_base64: String,
    history: Vec<Value>,
    cancel: Arc<AtomicBool>,
) -> Result<String, String> {
    let user_message = json!({"role": "user", "content": [
            {"type": "text", "text": "Dengarkan audio ini, pahami pertanyaannya, lalu berikan jawaban yang sesuai dan lengkap."},
            {"type": "input_audio", "input_audio": {"data": wav_base64, "format": "wav"}}
        ]});
    let messages = build_messages(config, history, user_message);
    stream(app, config, messages, cancel)
}

fn build_messages(config: &Config, history: Vec<Value>, user_message: Value) -> Value {
    let mut messages = vec![json!({"role": "system", "content": config.system_prompt.clone()})];
    messages.extend(history);
    messages.push(user_message);
    Value::Array(messages)
}

fn stream(
    app: &AppHandle,
    config: &Config,
    messages: Value,
    cancel: Arc<AtomicBool>,
) -> Result<String, String> {
    let api_key = match Entry::new("anyq", "api_key")
        .map_err(|error| error.to_string())?
        .get_password()
    {
        Ok(value) => value,
        Err(keyring::Error::NoEntry) => Entry::new("overlay-ai", "api_key")
            .map_err(|error| error.to_string())?
            .get_password()
            .map_err(|_| "API key belum disimpan".to_string())?,
        Err(error) => return Err(error.to_string()),
    };
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .post(format!("{}/chat/completions", config.base_url))
        .bearer_auth(api_key)
        .json(&json!({
            "model": config.chat_model,
            "messages": messages,
            "stream": true
        }))
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Chat API returned HTTP {}.", response.status()));
    }

    let mut response = response;
    let mut buffer = [0_u8; 4096];
    let mut pending = String::new();
    let mut assistant_text = String::new();
    while !cancel.load(Ordering::SeqCst) {
        let count = response.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        pending.push_str(&String::from_utf8_lossy(&buffer[..count]));
        while let Some(newline) = pending.find('\n') {
            let line = pending[..newline].trim().to_string();
            pending.drain(..=newline);
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                return Ok(assistant_text);
            }
            let value: Value = serde_json::from_str(data).map_err(|error| error.to_string())?;
            if let Some(text) = value["choices"][0]["delta"]["content"].as_str() {
                assistant_text.push_str(text);
                app.emit("ai-chunk", json!({"text": text}))
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    if cancel.load(Ordering::SeqCst) {
        return Ok(assistant_text);
    }
    Ok(assistant_text)
}
