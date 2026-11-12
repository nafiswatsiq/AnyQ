<p align="center">
  <img src="src-tauri/icons/icon.png" alt="AnyQ logo" width="180" />
</p>

<h1 align="center">AnyQ</h1>

<p align="center">
  A desktop AI overlay for text, screenshots, and audio questions.
</p>

AnyQ is a desktop overlay built with [Tauri 2](https://v2.tauri.app/) for
sending text questions, screenshots, and audio to an OpenAI-compatible AI API.
The overlay can stay above other applications, become click-through, and be
protected from screen capture.

## Preview

<video
  controls
  muted
  loop
  preload="metadata"
  width="800">
  <source
    src="https://jumpshare.com/embed/DUeBPfo0xA8xfXz9s2sy"
    type="video/mp4">
  <a href="https://jumpshare.com/embed/DUeBPfo0xA8xfXz9s2sy">
    Watch the AnyQ preview video
  </a>
</video>

## Features

- Streaming AI chat with Markdown output.
- Text questions through the chat input.
- Screenshot capture from the active monitor and vision requests.
- Microphone or system audio input through Windows WASAPI loopback.
- Global hotkeys for click-through, screenshots, audio, show/hide, chat focus,
  and quit.
- Dedicated Settings and Help windows.
- API keys stored in Windows Credential Manager.
- Configurable opacity and stealth/content protection.
- Notch-style Home, Assistant, and Notes tabs with a compact collapsed pill.
- Local notes with autosave and search; notes are stored in the browser profile for
  the overlay window and are never sent to the AI service.
- System tray menu with Settings, Help, Toggle Overlay, and Quit.

## Requirements

- Windows 10 or Windows 11.
- [Node.js](https://nodejs.org/) 20 or newer.
- [Rust](https://www.rust-lang.org/tools/install) stable with the
  `x86_64-pc-windows-msvc` target.
- Microsoft WebView2 Runtime.
- An API key from the AI provider you want to use.

System audio capture requires a Windows output device that supports loopback
capture.

## Installation and development

Install JavaScript dependencies:

```powershell
npm install
```

Run the development application:

```powershell
npm run tauri dev
```

Build the executable and installers:

```powershell
npm run tauri build
```

Build output:

```text
src-tauri/target/release/anyq.exe
src-tauri/target/release/bundle/msi/AnyQ_0.1.0_x64_en-US.msi
src-tauri/target/release/bundle/nsis/AnyQ_0.1.0_x64-setup.exe
```

## AI configuration

Open **Settings** from the overlay header or the system tray menu. Configure:

- **Base URL**: The provider API URL without a trailing slash.
- **API Key**: Stored in Windows Credential Manager, not in the config file.
- **Chat model**: The model name available from the selected provider.
- **System prompt**: Instructions that control the model's behavior.
- **Audio source**: Microphone or System audio.
- **Opacity** and **Hide overlay from screen capture**.

### OpenAI

```text
Base URL: https://api.openai.com/v1
Chat model: gpt-4o-mini
```

### DeepSeek

```text
Base URL: https://api.deepseek.com/v1
Chat model: deepseek-chat
```

### OpenRouter

OpenRouter can be used to access models from multiple providers:

```text
Base URL: https://openrouter.ai/api/v1
Chat model: deepseek/deepseek-chat
```

Anthropic, Google, Mistral, and other provider models can be used through
OpenRouter when they are available and support the required endpoint format.

### Other providers

AnyQ uses the **OpenAI-compatible Chat Completions** format:

```text
POST {base_url}/chat/completions
```

AnyQ can therefore work with providers or local servers such as Groq, Together
AI, Ollama, LM Studio, vLLM, and other gateways that expose a compatible
endpoint.

The direct Anthropic API uses `/v1/messages` instead of `/chat/completions`, so
it cannot currently be used by changing only the Base URL. Use an
OpenAI-compatible gateway such as OpenRouter, or add a dedicated adapter if
direct Anthropic API support is required.

Multimodal support depends on the provider and model:

- Text requires Chat Completions support.
- Screenshots require vision support and `image_url`.
- Audio requires `input_audio` support with WAV input.

Use **Test Connection** to verify the Base URL and API key.

## Global hotkeys

| Shortcut | Action |
| --- | --- |
| `Ctrl+Shift+Space` | Toggle click-through |
| `Ctrl+Shift+S` | Capture the active monitor and send it to AI |
| `Ctrl+Shift+A` | Start/stop microphone or system audio recording |
| `Ctrl+Shift+H` | Expand the overlay |
| `Ctrl+Shift+I` | Show the overlay and focus the chat input |
| `Ctrl+Shift+Q` | Quit AnyQ |
| `Esc` | Cancel the currently streaming AI response |

When the chat input is focused, press `Enter` to send. Press `Shift+Enter` to
insert a new line.

### Starting AnyQ with a Windows shortcut

Global hotkeys are available while the AnyQ process is running. To start AnyQ
when it is closed:

1. Create a shortcut to `anyq.exe`.
2. Open the shortcut **Properties**.
3. Set a **Shortcut key**, for example `Ctrl+Alt+A`.
4. Click **Apply** and **OK**.

## User data locations

AnyQ stores its configuration at:

```text
%APPDATA%\anyq\config.json
```

Notes are stored locally by the overlay and can be backed up by copying the
AnyQ WebView profile data. Note contents are not included in AI requests unless
you explicitly send them from the Assistant.

The API key is stored in Windows Credential Manager under the `anyq` service
with the `api_key` username. Older versions used the `overlay-ai` name; AnyQ
still reads legacy data as a fallback to support migration.

## Project structure

```text
src/                 Frontend HTML, CSS, and JavaScript
src-tauri/src/       Rust backend and Tauri commands
src-tauri/icons/     Application and installer icons
src-tauri/tauri.conf.json
                     Tauri window, bundle, and identifier configuration
```

The frontend uses Tauri static assets with `withGlobalTauri: true`, so browser
code accesses the API through `window.__TAURI__`.

## Troubleshooting

### Settings are empty after the rename

Make sure you are using the latest build. AnyQ reads
`%APPDATA%\anyq\config.json` and falls back to
`%APPDATA%\overlay-ai\config.json`. Open Settings, click **Save**, and restart
AnyQ.

### The old icon is still displayed

Windows caches application icons. Exit AnyQ completely, install the latest
build, and restart Windows Explorer or Windows if necessary.

### System audio is not recorded

- Select **System audio** in Settings.
- Make sure an output device is currently playing audio.
- Make sure the Windows device supports WASAPI loopback.
- Check audio underrun/overrun logs if the input stream is unstable.

### The audio model rejects the request

Not every provider or model supports `input_audio`. Try a documented
multimodal model with audio support, or use a provider that supports audio input
through Chat Completions.

### The drive runs out of space during a build

Rust build artifacts can become large. If debug artifacts are not needed,
remove `src-tauri/target/debug` and run the release build again:

```powershell
npm run tauri build
```

## Development checks

This project uses Rust, Tauri, and vanilla HTML/CSS/JavaScript. Run the
following checks locally:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml
node --check src/main.js
node --check src/settings.js
```
