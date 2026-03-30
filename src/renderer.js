// src/renderer.js
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open } = window.__TAURI__.dialog || {};
const { dirname } = window.__TAURI__.path || {};

let isInstalling = false;
let isVoiceChatActive = false;
const VOICE_CHAT_URL = 'https://ns.fiber-gate.ru';

document.addEventListener('DOMContentLoaded', async () => {
    console.log('[FRONTEND] DOM loaded');

    const gameStatus = document.getElementById('game-status');
    const launchBtn = document.getElementById('launch-btn');
    const addonsList = document.getElementById('addons-list');
    const logsBtn = document.getElementById('logs-btn');
    const voiceBtn = document.getElementById('voice-btn');
    const changePathBtn = document.getElementById('change-path-btn');
    const voiceChatView = document.getElementById('voice-chat-view');
    const voiceChatFrame = document.getElementById('voice-chat-frame');
    const voiceChatHeader = document.getElementById('voice-chat-header');
    const backToAddonsBtn = document.getElementById('back-to-addons-btn');
    const toggleMicGlobalBtn = document.getElementById('toggle-mic-global');

    await listen('progress', (event) => {
        const { name, progress } = event.payload;
        updateAddonProgress(name, progress);
    });

    await listen('operation-finished', (event) => {
        const { name, success } = event.payload;
        if (success) refreshAddonStatus(name);
    });

    await listen('operation-error', (event) => {
        showError(event.payload.message);
        document.querySelectorAll('.addon-card input[type="checkbox"]').forEach(cb => cb.disabled = false);
    });

    await listen('addon-install-started', (event) => {
        isInstalling = true;
        launchBtn.disabled = true;
        launchBtn.textContent = 'Установка...';
    });

    await listen('addon-install-finished', (event) => {
        isInstalling = false;
        launchBtn.disabled = false;
        launchBtn.textContent = 'Запустить игру';
        checkGame();
    });

    launchBtn.addEventListener('click', launchGame);
    logsBtn.addEventListener('click', openLogsFolder);
    voiceBtn.addEventListener('click', openVoiceChat);
    if (changePathBtn) changePathBtn.addEventListener('click', changeGamePath);
    if (backToAddonsBtn) backToAddonsBtn.addEventListener('click', closeVoiceChat);
    if (toggleMicGlobalBtn) toggleMicGlobalBtn.addEventListener('click', toggleGlobalMic);

    window.addEventListener('message', (event) => {
        if (event.origin !== 'https://ns.fiber-gate.ru') return;
        const { type, payload } = event.data;
        if (type === 'MIC_STATE_CHANGED' && toggleMicGlobalBtn) {
            toggleMicGlobalBtn.classList.toggle('active', payload.active);
        }
    });

    loadAddons();
    checkGame();

    async function loadAddons() {
        try {
            const addons = await invoke('load_addons');
            renderAddons(addons);
        } catch (error) {
            showError('Не удалось загрузить список аддонов: ' + error);
        }
    }

    function renderAddons(addons) {
        addonsList.innerHTML = '';
        for (const [name, addon] of Object.entries(addons)) {
            addonsList.appendChild(createAddonElement(name, addon));
        }
    }

    function createAddonElement(name, addon) {
        const card = document.createElement('div');
        card.className = 'addon-card';
        card.dataset.name = name;
        const contentWrapper = document.createElement('div');
        contentWrapper.className = 'addon-content-wrapper';
        const overlay = document.createElement('div');
        overlay.className = 'progress-overlay hidden';
        card.overlay = overlay;
        const topRow = document.createElement('div');
        topRow.className = 'addon-top';
        const nameEl = document.createElement('span');
        nameEl.className = 'addon-name';
        nameEl.textContent = name;
        const updateLabel = document.createElement('span');
        updateLabel.className = 'update-label';
        updateLabel.style.display = addon.needs_update ? 'inline' : 'none';
        updateLabel.textContent = 'Доступно обновление';
        const checkbox = document.createElement('input');
        checkbox.type = 'checkbox';
        checkbox.id = `checkbox-${name}`;
        checkbox.checked = addon.installed;
        checkbox.disabled = addon.being_processed || addon.updating || isInstalling;
        const label = document.createElement('label');
        label.htmlFor = `checkbox-${name}`;
        label.className = 'custom-checkbox';
        topRow.appendChild(nameEl);
        topRow.appendChild(updateLabel);
        topRow.appendChild(checkbox);
        topRow.appendChild(label);
        const description = document.createElement('div');
        description.className = 'addon-description';
        description.textContent = addon.description;
        card.checkbox = checkbox;
        card.updateLabel = updateLabel;
        card.appendChild(overlay);
        contentWrapper.appendChild(topRow);
        contentWrapper.appendChild(description);
        card.appendChild(contentWrapper);
        if (addon.installed) {
            checkbox.addEventListener('mouseenter', () => card.classList.add('deleting-warning'));
            checkbox.addEventListener('mouseleave', () => card.classList.remove('deleting-warning'));
        }
        checkbox.addEventListener('change', () => {
            const willInstall = checkbox.checked;
            const originalState = !willInstall;
            checkbox.disabled = true;
            card.classList.remove('deleting-warning');
            invoke('toggle_addon', { name, install: willInstall })
                .then(success => { if (!success) checkbox.checked = originalState; })
                .catch(error => { checkbox.checked = originalState; checkbox.disabled = false; });
        });
        return card;
    }

    function updateAddonProgress(name, progress) {
        const cards = document.querySelectorAll('.addon-card');
        for (const card of cards) {
            if (card.dataset.name === name && card.overlay) {
                const overlay = card.overlay;
                overlay.style.setProperty('--progress', Math.min(progress, 1.0) * 100 + '%');
                if (progress > 0) { overlay.classList.remove('hidden'); overlay.style.opacity = '1'; }
                if (progress >= 1.0) { setTimeout(() => overlay.classList.add('hidden'), 300); }
                break;
            }
        }
    }

    function refreshAddonStatus(name) {
        invoke('load_addons').then(addons => {
            const addon = addons[name];
            if (!addon) return;
            const cards = document.querySelectorAll('.addon-card');
            for (const card of cards) {
                if (card.dataset.name === name) {
                    card.checkbox.disabled = addon.being_processed || addon.updating || isInstalling;
                    card.checkbox.checked = addon.installed;
                    card.updateLabel.style.display = addon.needs_update ? 'inline' : 'none';
                    if (card.overlay) { card.overlay.classList.add('hidden'); card.overlay.style.opacity = '0'; }
                    break;
                }
            }
        });
    }

    async function checkGame() {
        try {
            const exists = await invoke('check_game');
            gameStatus.textContent = exists ? 'Готова к запуску' : 'Игра не найдена';
            gameStatus.style.color = exists ? '#4CAF50' : '#F44336';
            launchBtn.disabled = !exists || isInstalling || isVoiceChatActive;
        } catch (error) {
            gameStatus.textContent = 'Ошибка проверки игры';
            gameStatus.style.color = '#F44336';
            launchBtn.disabled = true;
        }
    }

    async function launchGame() {
        if (isInstalling) { showError('Идёт установка аддона, подождите...'); return; }
        if (isVoiceChatActive) { showError('Сначала закройте Щебетало'); return; }
        try {
            const success = await invoke('launch_game');
            if (!success) showError('Не удалось запустить игру');
        } catch (error) { showError('Не удалось запустить игру: ' + error); }
    }

    function openLogsFolder() { invoke('open_logs_folder'); }

    async function changeGamePath() {
        try {
            if (open && dirname) {
                const selected = await open({
                    title: 'Выберите Wow.exe',
                    multiple: false,
                    filters: [{ name: 'Executable', extensions: ['exe'] }]
                });
                if (selected && typeof selected === 'string') {
                    const gameDir = await dirname(selected);
                    const success = await invoke('change_game_path', { newPath: gameDir });
                    if (success) {
                        await invoke('set_game_path', { path: gameDir });
                        checkGame();
                        loadAddons();
                    }
                }
            } else {
                const newPath = prompt('Введите путь к папке с Wow.exe:');
                if (newPath) {
                    const success = await invoke('change_game_path', { newPath: newPath });
                    if (success) {
                        await invoke('set_game_path', { path: newPath });
                        checkGame();
                        loadAddons();
                    }
                }
            }
        } catch (error) { showError('Не удалось изменить путь: ' + error); }
    }

    function openVoiceChat() {
        isVoiceChatActive = true;
        document.body.classList.add('voice-chat-active');
        if (voiceChatView) voiceChatView.classList.remove('hidden');
        if (voiceChatFrame && !voiceChatFrame.dataset.loaded) {
            voiceChatFrame.src = VOICE_CHAT_URL;
            voiceChatFrame.dataset.loaded = 'true';
        }
        setTimeout(() => { if (voiceChatFrame) voiceChatFrame.focus(); }, 100);
        if (voiceChatHeader) voiceChatHeader.classList.add('show');
        launchBtn.disabled = true;
        console.log('[FRONTEND] Щебетало открыто');
        // Микрофон запрашивается нативно через iframe (WebView2 на Windows)
    }

    function closeVoiceChat() {
        isVoiceChatActive = false;
        document.body.classList.remove('voice-chat-active');
        if (voiceChatView) voiceChatView.classList.add('hidden');
        checkGame();
        console.log('[FRONTEND] Щебетало скрыто');
    }

    function toggleGlobalMic() {
        if (!voiceChatFrame) return;
        const iframe = voiceChatFrame.contentWindow;
        if (!iframe) return;
        iframe.postMessage({ type: 'TOGGLE_MIC' }, '*');
        if (toggleMicGlobalBtn) toggleMicGlobalBtn.classList.toggle('active');
    }

    function showError(message) {
        console.error('[FRONTEND] Error:', message);
        alert(`Ошибка: ${message}`);
    }
});