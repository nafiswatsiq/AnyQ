use serde::{Deserialize, Serialize};
use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hotkeys {
    pub toggle_click_through: String,
    #[serde(default = "default_capture_screen")]
    pub capture_screen: String,
    #[serde(default = "default_toggle_recording")]
    pub toggle_recording: String,
    #[serde(default = "default_focus_chat")]
    pub focus_chat: String,
}

fn default_capture_screen() -> String {
    "Ctrl+Shift+S".to_string()
}

fn default_toggle_recording() -> String {
    "Ctrl+Shift+A".to_string()
}

fn default_focus_chat() -> String {
    "Ctrl+Shift+I".to_string()
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            toggle_click_through: "Ctrl+Shift+Space".to_string(),
            capture_screen: default_capture_screen(),
            toggle_recording: default_toggle_recording(),
            focus_chat: default_focus_chat(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub base_url: String,
    pub chat_model: String,
    pub system_prompt: String,
    pub opacity: f32,
    pub stealth: bool,
    pub mic_device: Option<String>,
    #[serde(default)]
    pub audio_source: AudioSource,
    pub hotkeys: Hotkeys,
    #[serde(default = "default_auto_hide")]
    pub auto_hide: bool,
    #[serde(default)]
    pub default_tab: DefaultTab,
    #[serde(default = "default_collapse_delay")]
    pub collapse_delay_ms: u64,
}

fn default_auto_hide() -> bool { true }
fn default_collapse_delay() -> u64 { 800 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DefaultTab {
    #[default]
    Home,
    Assistant,
    Notes,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioSource {
    #[default]
    Microphone,
    SystemAudio,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            chat_model: "gpt-4o-mini".to_string(),
            system_prompt: "Kamu asisten yang membantu menjawab pertanyaan dengan bahasa yang sama seperti pertanyaan pengguna. Jawab sesuai kebutuhan dan konteks: tetap langsung ke inti, jelas, dan tidak bertele-tele, tetapi berikan penjelasan lengkap jika pertanyaannya membutuhkan langkah penyelesaian, contoh, kode, analisis, atau hasil akhir. Untuk pertanyaan coding, jelaskan solusi secara menyeluruh, berikan kode lengkap jika relevan, dan hitung serta tampilkan contoh output konkret dari kode tersebut, bukan hanya deskripsi hasilnya. Tampilkan output program di dalam fenced code block agar spasi, indentasi, dan baris baru tetap terjaga. Jika program menghasilkan pola atau banyak baris, tampilkan seluruh pola atau contoh representatif yang lengkap. Pertanyaan juga dapat berupa psikotes atau soal penalaran yang sengaja dibuat menjebak. Untuk soal seperti itu, baca setiap kata dan batasan dengan teliti, jangan terburu-buru mengambil asumsi, periksa kemungkinan ambiguitas serta maksud tersembunyi, lalu verifikasi kembali logika dan perhitungan sebelum memberikan jawaban. Jika terdapat lebih dari satu interpretasi yang masuk akal, jelaskan perbedaannya dan sebutkan asumsi yang digunakan. Jangan membatasi jawaban secara kaku pada jumlah kalimat tertentu.".to_string(),
            opacity: 0.9,
            stealth: true,
            mic_device: None,
            audio_source: AudioSource::Microphone,
            hotkeys: Hotkeys::default(),
            auto_hide: true,
            default_tab: DefaultTab::Home,
            collapse_delay_ms: 800,
        }
    }
}

impl Config {
    pub fn normalize(&mut self) {
        self.base_url = self.base_url.trim_end_matches('/').to_string();
        self.opacity = self.opacity.clamp(0.1, 1.0);
        if self.chat_model.trim().is_empty() {
            self.chat_model = Self::default().chat_model;
        }
        self.collapse_delay_ms = self.collapse_delay_ms.clamp(200, 5000);
    }
}

pub fn path() -> io::Result<PathBuf> {
    let root = dirs::config_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Config directory unavailable"))?;
    Ok(root.join("anyq").join("config.json"))
}

fn legacy_path() -> io::Result<PathBuf> {
    let root = dirs::config_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Config directory unavailable"))?;
    Ok(root.join("overlay-ai").join("config.json"))
}

pub fn load() -> io::Result<Config> {
    let file = path()?;
    let migrate_legacy = !file.exists();
    let file = if migrate_legacy {
        let legacy = legacy_path()?;
        if legacy.exists() { legacy } else { file }
    } else {
        file
    };
    if !file.exists() {
        return Ok(Config::default());
    }

    let contents = fs::read_to_string(file)?;
    let mut config: Config = serde_json::from_str(&contents)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    config.normalize();
    if migrate_legacy {
        save(&config)?;
    }
    Ok(config)
}

pub fn save(config: &Config) -> io::Result<()> {
    let file = path()?;
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = temp_path(&file);
    let contents = serde_json::to_vec_pretty(config)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(&temp, contents)?;
    replace_file(&temp, &file)
}

fn temp_path(file: &Path) -> PathBuf {
    file.with_extension("json.tmp")
}

#[cfg(windows)]
fn replace_file(temp: &Path, file: &Path) -> io::Result<()> {
    if file.exists() {
        fs::remove_file(file)?;
    }
    fs::rename(temp, file)
}

#[cfg(not(windows))]
fn replace_file(temp: &Path, file: &Path) -> io::Result<()> {
    fs::rename(temp, file)
}
