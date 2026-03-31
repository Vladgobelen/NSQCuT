// src/renderer.js
// ✅ Глобальный Tauri API (без импортов)
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open } = window.__TAURI__.dialog || {};
const { dirname } = window.__TAURI__.path || {};

// ✅ Глобальное состояние установщика аддонов
let isInstalling = false;

// ✅ Глобальное состояние Щебетало
let isVoiceChatActive = false;
const VOICE_CHAT_URL = "https://ns.fiber-gate.ru";

document.addEventListener("DOMContentLoaded", async () => {
  console.log("[NSQCuT] DOM loaded");

  const gameStatus = document.getElementById("game-status");
  const launchBtn = document.getElementById("launch-btn");
  const addonsList = document.getElementById("addons-list");
  const logsBtn = document.getElementById("logs-btn");
  const voiceBtn = document.getElementById("voice-btn");
  const changePathBtn = document.getElementById("change-path-btn");

  // ✅ Элементы Щебетало
  const voiceChatView = document.getElementById("voice-chat-view");
  const voiceChatFrame = document.getElementById("voice-chat-frame");
  const voiceChatHeader = document.getElementById("voice-chat-header");
  const backToAddonsBtn = document.getElementById("back-to-addons-btn");
  const voiceHoverZone = document.getElementById("voice-hover-zone");

  // ✅ Подписка на события прогресса
  await listen("progress", (event) => {
    console.log("[NSQCuT] Progress:", event.payload);
    const { name, progress } = event.payload;
    updateAddonProgress(name, progress);
  });

  // ✅ Подписка на событие окончания операции
  await listen("operation-finished", (event) => {
    console.log("[NSQCuT] Operation finished:", event.payload);
    const { name, success } = event.payload;
    if (success) refreshAddonStatus(name);
  });

  // ✅ Подписка на событие ошибки
  await listen("operation-error", (event) => {
    console.log("[NSQCuT] Error:", event.payload);
    showError(event.payload.message);
    document
      .querySelectorAll(".addon-card input[type='checkbox']")
      .forEach((cb) => (cb.disabled = false));
  });

  // ✅ Подписка на событие начала установки (блокировка кнопки запуска)
  await listen("addon-install-started", (event) => {
    console.log("[NSQCuT] Install started:", event.payload);
    isInstalling = true;
    launchBtn.disabled = true;
    launchBtn.textContent = "Установка...";
  });

  // ✅ Подписка на событие окончания установки (разблокировка кнопки запуска)
  await listen("addon-install-finished", (event) => {
    console.log("[NSQCuT] Install finished:", event.payload);
    isInstalling = false;
    launchBtn.disabled = false;
    launchBtn.textContent = "Запустить игру";
    checkGame();
  });

  // ✅ Подписка на состояние кнопки запуска (блокировка при проверке обновлений)
  await listen("launch-button-state", (event) => {
    console.log("[NSQCuT] Launch button state:", event.payload);
    const { enabled } = event.payload;
    launchBtn.disabled = !enabled;
    if (!enabled) {
      launchBtn.textContent = "Проверка обновлений...";
    } else {
      launchBtn.textContent = "Запустить игру";
      checkGame();
    }
  });

  // ✅ Обработчики кнопок менеджера аддонов
  launchBtn.addEventListener("click", launchGame);
  logsBtn.addEventListener("click", openLogsFolder);
  voiceBtn.addEventListener("click", openVoiceChat);
  changePathBtn.addEventListener("click", changeGamePath);

  // ✅ Обработчики кнопок Щебетало
  if (backToAddonsBtn) {
    backToAddonsBtn.addEventListener("click", closeVoiceChat);
  }

  // 🔥 Логика показа/скрытия хедера: зона 10px сверху + сама панель
  function showHeader() {
    if (voiceChatHeader) {
      voiceChatHeader.classList.add("show");
    }
  }

  function hideHeader() {
    if (voiceChatHeader) {
      voiceChatHeader.classList.remove("show");
    }
  }

  // 🔥 Обработчики для зоны ховера (верхние 10px)
  if (voiceHoverZone) {
    voiceHoverZone.addEventListener("mouseenter", showHeader);
    voiceHoverZone.addEventListener("mouseleave", hideHeader);
  }

  // 🔥 Обработчики для самой панели хедера (чтобы не скрывалась при наведении на неё)
  if (voiceChatHeader) {
    voiceChatHeader.addEventListener("mouseenter", showHeader);
    voiceChatHeader.addEventListener("mouseleave", hideHeader);
  }

  // ✅ Инициализация
  loadAddons();
  checkGame();

  // ==================== ФУНКЦИИ МЕНЕДЖЕРА АДДОНОВ ====================

  async function loadAddons() {
    try {
      console.log("[NSQCuT] Invoking load_addons...");
      const addons = await invoke("load_addons");
      console.log(
        "[NSQCuT] Received",
        Object.keys(addons).length,
        "addons"
      );
      renderAddons(addons);
    } catch (error) {
      console.error("[NSQCuT] Error loading addons:", error);
      showError("Не удалось загрузить список аддонов: " + error);
    }
  }

  function renderAddons(addons) {
    addonsList.innerHTML = "";
    for (const [name, addon] of Object.entries(addons)) {
      addonsList.appendChild(createAddonElement(name, addon));
    }
  }

  function createAddonElement(name, addon) {
    const card = document.createElement("div");
    card.className = "addon-card";
    card.dataset.name = name;

    const contentWrapper = document.createElement("div");
    contentWrapper.className = "addon-content-wrapper";

    const overlay = document.createElement("div");
    overlay.className = "progress-overlay hidden";
    card.overlay = overlay;

    const topRow = document.createElement("div");
    topRow.className = "addon-top";

    const nameEl = document.createElement("span");
    nameEl.className = "addon-name";
    nameEl.textContent = name;

    const updateLabel = document.createElement("span");
    updateLabel.className = "update-label";
    updateLabel.style.display = addon.needs_update ? "inline" : "none";
    updateLabel.textContent = "Доступно обновление";

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.id = `checkbox-${name}`;
    checkbox.checked = addon.installed;
    checkbox.disabled =
      addon.being_processed || addon.updating || isInstalling;

    const label = document.createElement("label");
    label.htmlFor = `checkbox-${name}`;
    label.className = "custom-checkbox";

    topRow.appendChild(nameEl);
    topRow.appendChild(updateLabel);
    topRow.appendChild(checkbox);
    topRow.appendChild(label);

    const description = document.createElement("div");
    description.className = "addon-description";
    description.textContent = addon.description;

    card.checkbox = checkbox;
    card.updateLabel = updateLabel;

    card.appendChild(overlay);
    contentWrapper.appendChild(topRow);
    contentWrapper.appendChild(description);
    card.appendChild(contentWrapper);

    if (addon.installed) {
      checkbox.addEventListener("mouseenter", () =>
        card.classList.add("deleting-warning")
      );
      checkbox.addEventListener("mouseleave", () =>
        card.classList.remove("deleting-warning")
      );
    }

    checkbox.addEventListener("change", () => {
      const willInstall = checkbox.checked;
      const originalState = !willInstall;

      // 🔥 Блокируем только этот чекбокс
      checkbox.disabled = true;
      card.classList.remove("deleting-warning");

      invoke("toggle_addon", { name, install: willInstall })
        .then((success) => {
          if (!success) {
            checkbox.checked = originalState;
            checkbox.disabled = false;
          }
          // 🔥 Не разблокируем здесь — дождёмся события operation-finished
        })
        .catch((error) => {
          console.error("[NSQCuT] Toggle error:", error);
          checkbox.checked = originalState;
          checkbox.disabled = false;
        });
    });

    return card;
  }

  function updateAddonProgress(name, progress) {
    const cards = document.querySelectorAll(".addon-card");
    for (const card of cards) {
      if (card.dataset.name === name && card.overlay) {
        const overlay = card.overlay;
        overlay.style.setProperty(
          "--progress",
          Math.min(progress, 1.0) * 100 + "%"
        );
        if (progress > 0) {
          overlay.classList.remove("hidden");
          overlay.style.opacity = "1";
        }
        if (progress >= 1.0) {
          setTimeout(() => overlay.classList.add("hidden"), 300);
        }
        break;
      }
    }
  }

  function refreshAddonStatus(name) {
    invoke("load_addons")
      .then((addons) => {
        const addon = addons[name];
        if (!addon) return;

        const cards = document.querySelectorAll(".addon-card");
        for (const card of cards) {
          if (card.dataset.name === name) {
            card.checkbox.disabled =
              addon.being_processed || addon.updating || isInstalling;
            card.checkbox.checked = addon.installed;
            card.updateLabel.style.display = addon.needs_update
              ? "inline"
              : "none";
            if (card.overlay) {
              card.overlay.classList.add("hidden");
              card.overlay.style.opacity = "0";
            }
            break;
          }
        }
      })
      .catch((error) => console.error("[NSQCuT] Refresh error:", error));
  }

  async function checkGame() {
    try {
      const exists = await invoke("check_game");
      gameStatus.textContent = exists ? "Готова к запуску" : "Игра не найдена";
      gameStatus.style.color = exists ? "#4CAF50" : "#F44336";
      launchBtn.disabled = !exists || isInstalling || isVoiceChatActive;
    } catch (error) {
      gameStatus.textContent = "Ошибка проверки игры";
      gameStatus.style.color = "#F44336";
      launchBtn.disabled = true;
    }
  }

  async function launchGame() {
    if (isInstalling) {
      showError("Идёт установка аддона, подождите...");
      return;
    }
    if (isVoiceChatActive) {
      showError("Сначала закройте Щебетало");
      return;
    }
    try {
      const success = await invoke("launch_game");
      if (!success) showError("Не удалось запустить игру");
    } catch (error) {
      showError("Не удалось запустить игру: " + error);
    }
  }

  function openLogsFolder() {
    invoke("open_logs_folder");
  }

  async function changeGamePath() {
    try {
      console.log("[NSQCuT] Changing game path...");

      if (open && dirname) {
        const selected = await open({
          title: "Выберите Wow.exe",
          multiple: false,
          filters: [
            {
              name: "Executable",
              extensions: ["exe"],
            },
          ],
        });

        console.log("[NSQCuT] Dialog result:", selected);

        if (selected && typeof selected === "string") {
          const gameDir = await dirname(selected);
          console.log("[NSQCuT] Game directory:", gameDir);

          const success = await invoke("change_game_path", {
            newPath: gameDir,
          });
          console.log("[NSQCuT] Change path result:", success);

          if (success) {
            await invoke("set_game_path", { path: gameDir });
            checkGame();
            loadAddons();
          }
        } else if (selected === null) {
          console.log("[NSQCuT] User canceled dialog");
        }
      } else {
        console.warn("[NSQCuT] Dialog API not available, using prompt");
        const newPath = prompt("Введите путь к папке с Wow.exe:");
        if (newPath) {
          const success = await invoke("change_game_path", { newPath });
          if (success) {
            await invoke("set_game_path", { path: newPath });
            checkGame();
            loadAddons();
          }
        }
      }
    } catch (error) {
      console.error("[NSQCuT] Error changing path:", error);
      showError("Не удалось изменить путь: " + error);
    }
  }

  // ==================== ФУНКЦИИ ЩЕБЕТАЛО ====================

  function openVoiceChat() {
    isVoiceChatActive = true;
    document.body.classList.add("voice-chat-active");

    if (voiceChatView) {
      voiceChatView.classList.remove("hidden");
    }

    if (voiceChatFrame && !voiceChatFrame.dataset.loaded) {
      voiceChatFrame.src = VOICE_CHAT_URL;
      voiceChatFrame.dataset.loaded = "true";
    }

    setTimeout(() => {
      if (voiceChatFrame) voiceChatFrame.focus();
    }, 100);

    launchBtn.disabled = true;
    console.log("[NSQCuT] Щебетало открыто");
  }

  function closeVoiceChat() {
    isVoiceChatActive = false;
    document.body.classList.remove("voice-chat-active");

    if (voiceChatView) {
      voiceChatView.classList.add("hidden");
    }

    hideHeader();
    checkGame();
    console.log("[NSQCuT] Щебетало скрыто (сохранено)");
  }

  function showError(message) {
    console.error("[NSQCuT] Error:", message);
    alert(`Ошибка: ${message}`);
  }
});