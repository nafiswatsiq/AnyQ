function waitForTauri(callback) {
  if (window.__TAURI__) return callback();
  const timer = setInterval(() => {
    if (window.__TAURI__) { clearInterval(timer); callback(); }
  }, 10);
}

waitForTauri(init);

async function init() {
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;
  const liveWindow = window.__TAURI__.window.getCurrentWindow();
  const root = document.querySelector("#overlay-root");
  const output = document.querySelector("#output");
  const eventLog = document.querySelector("#event-log");
  const status = document.querySelector("#status");
  let clickThrough = false;
  let aiText = "";
  let conversationEnabled = false;
  let chatMessages = []; // [{role, content}] for UI rendering only
  let activeAssistant = -1;
  let activeTab = "home";
  let autoHide = true;
  let collapseDelay = 800;
  let collapseTimer;
  let pillHoverTimer;
  let notes = JSON.parse(localStorage.getItem("anyq-notes") || "[]");
  let selectedNote = notes[0]?.id || null;
  let saveTimer;

  const escapeHtml = (value) => String(value).replace(/[&<>"']/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  }[c]));
  function markdown(value) {
    return escapeHtml(value).replace(/^### (.+)$/gm, "<h3>$1</h3>")
      .replace(/^## (.+)$/gm, "<h2>$1</h2>").replace(/^# (.+)$/gm, "<h1>$1</h1>")
      .replace(/```([\s\S]*?)```/g, "<pre><code>$1</code></pre>")
      .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>").replace(/\*([^*]+)\*/g, "<em>$1</em>")
      .split(/\n{2,}/).map((part) => part.startsWith("<h") || part.startsWith("<pre") ? part : `<p>${part.replace(/\n/g, "<br>")}</p>`).join("");
  }
  const setLog = (message) => { eventLog.textContent = message; };
  const formatTime = (date = new Date()) => date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
  const formatDate = (date = new Date()) => date.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" });

  function renderHome(config = {}) {
    autoHide = config.auto_hide !== false;
    collapseDelay = Number(config.collapse_delay_ms) || 800;
    const hour = new Date().getHours();
    const greeting = hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";
    const latest = notes.slice().sort((a, b) => b.updated - a.updated)[0];
    document.querySelector("#home-tab").innerHTML = `
      <div class="home-grid">
        <section class="home-welcome"><img class="mascot" src="assets/anyq-icon.gif" alt="AnyQ mascot" />
          <p class="eyebrow">${greeting}</p><h1 class="home-time">${formatTime()}</h1>
          <p class="home-date">${formatDate()}</p>
          <label class="switch-row"><input id="auto-hide" type="checkbox" ${config.auto_hide !== false ? "checked" : ""}/> Auto-hide</label>
        </section>
        <section class="quick-actions">
          <button class="quick-card" id="home-capture" type="button"><span><strong>Screenshot → AI</strong><small>${config.hotkeys?.capture_screen || "Ctrl+Shift+S"}</small></span><svg class="quick-icon"><use href="#icon-screenshot"></use></svg></button>
          <button class="quick-card" id="home-record" type="button"><span><strong>Voice → AI</strong><small>${config.hotkeys?.toggle_recording || "Ctrl+Shift+A"}</small></span><svg class="quick-icon"><use href="#icon-voice"></use></svg></button>
          <button class="quick-card" id="home-note" type="button"><span><strong>${latest ? escapeHtml(latest.title || "Untitled note") : "Buat catatan baru"}</strong><small>${latest ? new Date(latest.updated).toLocaleString() : "Open Notes"}</small></span><svg class="quick-icon"><use href="#icon-notes"></use></svg></button>
        </section>
      </div>`;
    document.querySelector("#auto-hide").addEventListener("change", async (event) => {
      try {
        const current = await invoke("get_config");
        await invoke("save_config", { newConfig: { ...current, auto_hide: event.target.checked } });
      } catch (error) { setLog(`Auto-hide error: ${error}`); }
    });
    document.querySelector("#home-capture").addEventListener("click", () => invoke("capture_screen").catch((e) => setLog(`Capture error: ${e}`)));
    document.querySelector("#home-record").addEventListener("click", () => invoke("toggle_recording").catch((e) => setLog(`Audio error: ${e}`)));
    document.querySelector("#home-note").addEventListener("click", () => { if (!latest) createNote(); selectTab("notes"); });
  }

  function saveNotes() { localStorage.setItem("anyq-notes", JSON.stringify(notes)); }
  function createNote(text = "") {
    const now = Date.now();
    const note = { id: crypto.randomUUID(), body: text, updated: now };
    notes.unshift(note); selectedNote = note.id; saveNotes(); renderNotes();
  }
  function renderNotes() {
    const query = document.querySelector("#note-search").value.toLowerCase();
    const list = document.querySelector("#notes-list");
    const visible = notes.filter((note) => note.body.toLowerCase().includes(query));
    list.innerHTML = visible.map((note) => {
      const title = note.body.split(/\r?\n/)[0] || "Untitled note";
      return `<button class="note-item ${note.id === selectedNote ? "active" : ""}" data-note="${note.id}" type="button"><strong>${escapeHtml(title)}</strong><small>${new Date(note.updated).toLocaleDateString()}</small></button>`;
    }).join("") || `<small style="color:rgba(255,255,255,.45)">No notes yet</small>`;
    list.querySelectorAll("[data-note]").forEach((item) => item.addEventListener("click", () => {
      selectedNote = item.dataset.note; renderNotes(); loadSelectedNote();
    }));
    loadSelectedNote();
  }
  function loadSelectedNote() {
    const editor = document.querySelector("#note-body");
    editor.value = notes.find((note) => note.id === selectedNote)?.body || "";
  }
  function selectTab(tab, resizeWindow = true) {
    activeTab = tab;
    document.querySelectorAll(".tab").forEach((button) => button.classList.toggle("active", button.dataset.tab === tab));
    document.querySelectorAll(".tab-content").forEach((element) => { element.hidden = !element.id.startsWith(`${tab}-`); });
    document.querySelector("#tab-title").textContent = tab[0].toUpperCase() + tab.slice(1);
    if (tab === "notes") {
      renderNotes();
      if (resizeWindow) invoke("set_window_state", { state: "ExpandedWide" }).catch(() => {});
    } else if (resizeWindow) {
      invoke("set_window_state", { state: "Expanded" }).catch(() => {});
    }
  }
  function setExpanded(expanded, tab = activeTab) {
    root.classList.toggle("expanded", expanded);
    root.classList.toggle("collapsed", !expanded);
    if (expanded) { selectTab(tab); invoke("set_window_state", { state: tab === "notes" ? "ExpandedWide" : "Expanded" }).catch((e) => setLog(`Window error: ${e}`)); }
    else invoke("set_window_state", { state: "Collapsed" }).catch(() => {});
  }
  function setIndicator(active) {
    clickThrough = active;
    const indicator = document.querySelector("#clickthrough-indicator");
    indicator.textContent = active ? "Click-through ON" : "Click-through OFF";
    indicator.classList.toggle("active", active);
  }
  function setConversationIndicator(enabled) {
    conversationEnabled = enabled;
    const conversationToggle = document.querySelector("#conversation-toggle");
    conversationToggle.textContent = `Chat ${enabled ? "ON" : "OFF"}`;
    conversationToggle.classList.toggle("active", enabled);
    conversationToggle.setAttribute("aria-pressed", String(enabled));
  }
  function renderChatMessages() {
    if (!conversationEnabled || chatMessages.length === 0) return;
    output.innerHTML = chatMessages.map((msg) =>
      `<div class="chat-bubble chat-bubble--${msg.role}">${
        msg.content
          ? markdown(msg.content)
          : '<span class="thinking-indicator" aria-label="Thinking"><i></i><i></i><i></i></span>'
      }</div>`
    ).join("");
    output.scrollTop = output.scrollHeight;
  }
  function resetConversationUi() {
    chatMessages = [];
    activeAssistant = -1;
    aiText = "";
    output.textContent = "Screen share protection is active.";
    status.textContent = "Ready";
    document.querySelector("#cancel-ai").hidden = true;
  }

  document.querySelector("#pill").addEventListener("mouseenter", () => {
    clearTimeout(pillHoverTimer);
    pillHoverTimer = setTimeout(() => setExpanded(true), 150);
  });
  document.querySelector("#pill").addEventListener("click", () => setExpanded(true));
  document.querySelector("#panel").addEventListener("mouseleave", () => {
    if (!autoHide || activeTab === "notes" || document.activeElement === document.querySelector("#note-body")) return;
    clearTimeout(collapseTimer);
    collapseTimer = setTimeout(() => setExpanded(false), collapseDelay);
  });
  document.querySelector("#panel").addEventListener("mouseenter", () => clearTimeout(collapseTimer));
  document.querySelectorAll(".tab").forEach((button) => button.addEventListener("click", () => selectTab(button.dataset.tab)));
  document.querySelector("#open-settings").addEventListener("click", () => invoke("open_settings").catch((e) => setLog(`Settings error: ${e}`)));
  document.querySelector("#open-help").addEventListener("click", () => invoke("open_help").catch((e) => setLog(`Help error: ${e}`)));
  document.querySelector("#toggle-clickthrough").addEventListener("click", () => invoke("toggle_click_through").catch((e) => setLog(`Click-through error: ${e}`)));
  document.querySelector("#new-note").addEventListener("click", () => createNote());
  document.querySelector("#note-search").addEventListener("input", renderNotes);
  document.querySelector("#note-body").addEventListener("input", (event) => {
    const note = notes.find((item) => item.id === selectedNote);
    if (!note) return;
    note.body = event.target.value; note.updated = Date.now();
    document.querySelector("#note-save-status").textContent = "Saving...";
    clearTimeout(saveTimer); saveTimer = setTimeout(() => { saveNotes(); renderNotes(); document.querySelector("#note-save-status").textContent = "Saved"; }, 500);
  });
  document.querySelector("#delete-note").addEventListener("click", () => {
    notes = notes.filter((note) => note.id !== selectedNote); selectedNote = notes[0]?.id || null; saveNotes(); renderNotes();
  });

  document.querySelector("#clear-output").addEventListener("click", async () => {
    try {
      await invoke("new_conversation");
      resetConversationUi();
    } catch (error) { setLog(`Clear conversation error: ${error}`); }
  });
  document.querySelector("#copy-output").addEventListener("click", () => navigator.clipboard.writeText(aiText || output.textContent).catch((e) => setLog(`Copy error: ${e}`)));
  document.querySelector("#new-conversation").addEventListener("click", async () => {
    try {
      await invoke("new_conversation");
      resetConversationUi();
      setLog("New conversation");
    } catch (error) { setLog(`New conversation error: ${error}`); }
  });
  document.querySelector("#cancel-ai").addEventListener("click", () => invoke("cancel_ai").catch(() => {}));
  document.querySelector("#conversation-toggle").addEventListener("click", async () => {
    const conversationToggle = document.querySelector("#conversation-toggle");
    const next = !conversationEnabled;
    conversationToggle.disabled = true;
    try {
      setConversationIndicator(Boolean(await invoke("set_conversation_mode", { enabled: next })));
    } catch (error) {
      setLog(`Chat mode error: ${error}`);
    } finally {
      conversationToggle.disabled = false;
    }
  });
  document.querySelector("#chat-form").addEventListener("submit", async (event) => {
    event.preventDefault(); const input = document.querySelector("#chat-input"); const text = input.value.trim(); if (!text) return;
    if (conversationEnabled) { chatMessages.push({ role: "user", content: text }); renderChatMessages(); }
    input.value = ""; await invoke("ask_ai", { text }).catch((e) => setLog(`Chat error: ${e}`));
  });
  document.querySelector("#chat-input").addEventListener("keydown", (event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); document.querySelector("#chat-form").requestSubmit(); } });

  await listen("clickthrough-changed", (event) => setIndicator(event.payload === true));
  await listen("conversation-mode-changed", (event) => setConversationIndicator(event.payload === true));
  await listen("opacity-changed", (event) => { const value = Number(event.payload); document.body.style.opacity = String(value); document.querySelector("#opacity-value").textContent = `${Math.round(value * 100)}%`; });
  await listen("config-changed", (event) => { const value = Number(event.payload.opacity); if (Number.isFinite(value)) document.body.style.opacity = String(value); renderHome(event.payload); });
  await listen("window-state-requested", () => setExpanded(true));
  await listen("audio-recording", (event) => {
    if (event.payload === true) {
      setExpanded(true, "assistant");
      status.textContent = "Listening...";
    }
  });
  await listen("ai-start", () => {
    setExpanded(true, "assistant");
    aiText = "";
    if (conversationEnabled) {
      activeAssistant = chatMessages.length;
      chatMessages.push({ role: "assistant", content: "" });
      renderChatMessages();
    } else {
      activeAssistant = -1;
      output.innerHTML = "";
    }
    status.textContent = "Thinking...";
    document.querySelector("#cancel-ai").hidden = false;
  });
  await listen("ai-chunk", (event) => {
    aiText += event.payload.text || "";
    if (conversationEnabled && activeAssistant >= 0) {
      chatMessages[activeAssistant].content = aiText;
      renderChatMessages();
    } else {
      output.innerHTML = markdown(aiText);
    }
  });
  await listen("ai-done", () => { status.textContent = "Done"; document.querySelector("#cancel-ai").hidden = true; activeAssistant = -1; });
  await listen("ai-error", (event) => { status.textContent = "Error"; output.textContent = event.payload.message || "AI request failed."; document.querySelector("#cancel-ai").hidden = true; });
  await listen("capture-error", (event) => setLog(`Capture error: ${event.payload}`));
  await listen("audio-error", (event) => { status.textContent = "Audio error"; setLog(`Audio error: ${event.payload}`); });

  try {
    const config = await invoke("get_config");
    const value = Number(config.opacity); if (Number.isFinite(value)) { document.body.style.opacity = String(value); document.querySelector("#opacity-value").textContent = `${Math.round(value * 100)}%`; }
    renderHome(config);
    selectTab(config.default_tab?.toLowerCase() || "home", false);
    root.classList.add("collapsed");
    root.classList.remove("expanded");
    setIndicator(await invoke("get_click_through")); setConversationIndicator(await invoke("get_conversation_mode"));
    await invoke("set_window_state", { state: "Collapsed" });
  } catch (error) { setLog(`Startup error: ${error}`); renderHome(); }
  setInterval(() => { document.querySelector("#pill-time").textContent = formatTime(); if (activeTab === "home") renderHome(); }, 60000);
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && document.activeElement !== document.querySelector("#note-body")) setExpanded(false);
    if (activeTab === "notes" && event.ctrlKey && event.key.toLowerCase() === "n") { event.preventDefault(); createNote(); }
  });
}
