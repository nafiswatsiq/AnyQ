function waitForTauri(callback) {
  if (window.__TAURI__) { callback(); return; }
  const id = setInterval(() => {
    if (window.__TAURI__) { clearInterval(id); callback(); }
  }, 10);
}

waitForTauri(initSettings);

async function initSettings() {
  const { invoke } = window.__TAURI__.core;

  const form = document.querySelector("#settings-form");
  const status = document.querySelector("#settings-status");
  const baseUrl = document.querySelector("#base-url");
  const apiKey = document.querySelector("#api-key");
  const chatModel = document.querySelector("#chat-model");
  const systemPrompt = document.querySelector("#system-prompt");
  const micDevice = document.querySelector("#mic-device");
  const audioSource = document.querySelector("#audio-source");
  const opacity = document.querySelector("#opacity");
  const opacityValue = document.querySelector("#opacity-value");
  const captureMode = document.querySelector("#capture-mode");
  const autoHide = document.querySelector("#auto-hide");
  const defaultTab = document.querySelector("#default-tab");
  const collapseDelay = document.querySelector("#collapse-delay");
  let currentConfig;

  function showStatus(message, error = false) {
    status.textContent = message;
    status.classList.toggle("error", error);
  }

  function setOpacityLabel() {
    opacityValue.textContent = `${opacity.value}%`;
  }

  async function loadSettings() {
    const config = await invoke("get_config");
    currentConfig = config;
    baseUrl.value = config.base_url;
    chatModel.value = config.chat_model;
    systemPrompt.value = config.system_prompt;
    opacity.value = Math.round(config.opacity * 100);
    captureMode.value = config.stealth ? "protect" : "allow";
    autoHide.checked = config.auto_hide !== false;
    defaultTab.value = String(config.default_tab || "home").toLowerCase();
    collapseDelay.value = config.collapse_delay_ms || 800;
    audioSource.value = config.audio_source || "microphone";
    setOpacityLabel();

    const devices = await invoke("list_mic_devices");
    for (const device of devices) {
      const option = document.createElement("option");
      option.value = device.name;
      option.textContent = device.name;
      option.selected = device.name === config.mic_device;
      micDevice.append(option);
    }
  }

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    try {
      const config = {
        base_url: baseUrl.value,
        chat_model: chatModel.value,
        system_prompt: systemPrompt.value,
        opacity: Number(opacity.value) / 100,
        stealth: captureMode.value === "protect",
        mic_device: micDevice.value || null,
        audio_source: audioSource.value,
        hotkeys: currentConfig.hotkeys,
        auto_hide: autoHide.checked,
        default_tab: defaultTab.value,
        collapse_delay_ms: Number(collapseDelay.value),
      };
      await invoke("save_config", { newConfig: config });
      if (apiKey.value.trim()) {
        await invoke("set_api_key", { apiKey: apiKey.value });
        apiKey.value = "";
      }
      showStatus("Settings saved.");
    } catch (error) {
      showStatus(`Save failed: ${error}`, true);
    }
  });

  document.querySelector("#test-connection").addEventListener("click", async () => {
    try {
      if (apiKey.value.trim()) {
        await invoke("set_api_key", { apiKey: apiKey.value });
        apiKey.value = "";
      }
      showStatus("Testing connection...");
      showStatus(await invoke("test_connection"));
    } catch (error) {
      showStatus(`Connection failed: ${error}`, true);
    }
  });

  opacity.addEventListener("input", setOpacityLabel);

  loadSettings().catch((error) => showStatus(`Load failed: ${error}`, true));
}
