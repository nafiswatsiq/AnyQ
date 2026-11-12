use crate::config::Config;
use keyring::Entry;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
}

pub fn transcribe(config: &Config, wav: Vec<u8>) -> Result<String, String> {
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
    let part = reqwest::blocking::multipart::Part::bytes(wav)
        .file_name("recording.wav")
        .mime_str("audio/wav")
        .map_err(|error| error.to_string())?;
    let response = client
        .post(format!("{}/audio/transcriptions", config.base_url))
        .bearer_auth(api_key)
        .multipart(
            reqwest::blocking::multipart::Form::new()
                .text("model", config.transcribe_model.clone())
                .part("file", part),
        )
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Transcription API returned HTTP {}.", response.status()));
    }
    response
        .json::<TranscriptionResponse>()
        .map(|result| result.text)
        .map_err(|error| error.to_string())
}
