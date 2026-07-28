import { isMobileViewport, getBaseUrl } from "./utils.js";

const COLORS = {
    green: "#A3BD8D",
    greenDark: "#7A9B6A",
    blue: "#7E9FBE",
    blueDark: "#5A7EA0",
    yellow: "#EACB8B",
    red: "#BE616B",
    dark: "#000000",
    medium: "#1C1C1C",
    light: "#3A3A3A",
    text: "#FFFFFF",
    textDim: "#CCCCCC",
};

const state = {
    active: false,
    renderMode: "single-pane",
    fitEnabled: true,
    activeOverlay: "none",
    armed: { ctrl: false, alt: false },
    kbdVisible: false,
    data: emptyMobileState(),
    dataReceivedAt: 0,
};

let ctx = null;
let els = null;
let activityTimer = null;

export function initMobileUi(context) {
    ctx = context;
    if (document.querySelector("#zj-mobile-root")) {
        return;
    }
    injectStyles();
    buildDom();
    installKeyMergeHook();
    installSoftKeyboardBinding();
    installResizeReconcileHooks();
    evaluateActivation();
    window.addEventListener("resize", evaluateActivation);
    window.__zjMobileUi = {
        setData,
        getState: () => state,
        getRenderSizing,
    };
    if (window.__zjLastMobileState) {
        setData(window.__zjLastMobileState);
    }
}

const ACTIVATION_PREFERENCE_KEY = "zellij:mobile-ui";

function activationPreference() {
    try {
        const stored = localStorage.getItem(ACTIVATION_PREFERENCE_KEY);
        if (stored === "on") return "on";
        if (stored === "off") return "off";
        return "auto";
    } catch (_) {
        return "auto";
    }
}

function setActivationPreference(value) {
    try {
        localStorage.setItem(ACTIVATION_PREFERENCE_KEY, value);
    } catch (_) {}
}

function shouldActivate() {
    const preference = activationPreference();
    if (preference === "on") return true;
    if (preference === "off") return false;
    return isMobileViewport();
}

function evaluateActivation() {
    const next = shouldActivate();
    if (next === state.active) {
        renderReturnPill();
        return;
    }
    state.active = next;
    document.body.classList.toggle("zj-mobile-active", next);
    if (!next) {
        state.activeOverlay = "none";
        state.kbdVisible = false;
        stopActivityTimer();
    }
    render();
    updateKeyboardOffset();
    reconcileSize();
}

function setData(data) {
    const prevDesktop = state.data.desktop_size;
    state.data = data;
    state.dataReceivedAt = Date.now();
    reconcileRenderPrefs(data.render_prefs);
    if (!state.fitEnabled && desktopSizeChanged(prevDesktop, data.desktop_size)) {
        window.dispatchEvent(new Event("zellij:rendering-resize"));
    }
    requestAnimationFrame(syncPan);
    render();
}

function desktopSizeChanged(a, b) {
    if (!a && !b) return false;
    if (!a || !b) return true;
    return a.cols !== b.cols || a.rows !== b.rows;
}

function reconcileRenderPrefs(prefs) {
    if (!prefs) {
        return;
    }
    const nextRenderMode = prefs.single_pane ? "single-pane" : "full";
    const changed =
        state.renderMode !== nextRenderMode || state.fitEnabled !== prefs.fit;
    state.renderMode = nextRenderMode;
    state.fitEnabled = prefs.fit;
    if (changed) {
        reconcileSize();
    }
}

function nowSecs() {
    const base = state.data.now_secs || Math.floor(Date.now() / 1000);
    const driftMs = state.dataReceivedAt ? Date.now() - state.dataReceivedAt : 0;
    return base + Math.floor(driftMs / 1000);
}

function injectStyles() {
    if (document.querySelector("#zj-mobile-styles")) {
        return;
    }
    const style = document.createElement("style");
    style.id = "zj-mobile-styles";
    style.textContent = `
    #zj-mobile-root {
      font-family: 'JetBrains Mono', 'Consolas', 'Monaco', 'Courier New', monospace;
      display: none;
    }
    body.zj-mobile-active #zj-mobile-root { display: block; }

    body.zj-mobile-active #terminal {
      height: calc(var(--dynamic-vh, 100vh) - var(--zj-chrome-top, 0px) - var(--zj-chrome-bottom, 0px));
      margin-top: var(--zj-chrome-top, 0px);
    }

    body.zj-mobile-panning #terminal {
      touch-action: none;
    }

    .zj-mobile-return {
      display: none;
      position: fixed; right: 12px; bottom: 12px;
      z-index: 700;
      min-height: 44px; padding: 0 16px;
      border: 1px solid ${COLORS.green};
      border-radius: 22px;
      background: ${COLORS.medium};
      color: ${COLORS.text};
      font-family: 'JetBrains Mono', 'Consolas', 'Monaco', 'Courier New', monospace;
      font-size: 14px;
      cursor: pointer;
    }
    body.zj-mobile-active .zj-mobile-return { display: none !important; }

    .zj-mobile-topbar {
      position: fixed; top: 0; left: 0; right: 0;
      display: flex; align-items: stretch;
      height: 44px; z-index: 500;
      background: ${COLORS.dark};
      border-bottom: 1px solid ${COLORS.green};
      color: ${COLORS.text};
    }
    .zj-mobile-topbar button {
      background: transparent; border: 0; color: inherit;
      font-family: inherit; font-size: 14px;
      padding: 0 12px; cursor: pointer;
      display: flex; align-items: center;
      min-height: 44px;
      white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    }
    .zj-mobile-topbar .zj-session { color: ${COLORS.blue}; }
    .zj-mobile-topbar .zj-pane { color: ${COLORS.green}; flex: 1 1 auto; min-width: 0; }
    .zj-mobile-topbar .zj-hamburger {
      color: ${COLORS.yellow}; font-size: 20px;
      flex: 0 0 auto; min-width: 44px; justify-content: center;
    }

    .zj-mobile-modbar {
      position: fixed; left: 0; right: 0;
      bottom: var(--zj-kbd-offset, 0px);
      display: none; z-index: 500;
      background: ${COLORS.medium};
      border-top: 1px solid ${COLORS.green};
    }
    body.zj-mobile-active .zj-mobile-modbar.zj-visible { display: flex; }
    .zj-mobile-modbar button {
      flex: 1 1 0; min-width: 0;
      background: transparent; border: 0;
      border-right: 1px solid ${COLORS.light};
      color: ${COLORS.text}; font-family: inherit; font-size: 15px;
      min-height: 48px; cursor: pointer;
    }
    .zj-mobile-modbar button:last-child { border-right: 0; }
    .zj-mobile-modbar button.zj-armed {
      background: ${COLORS.green}; color: ${COLORS.dark}; font-weight: 600;
    }

    .zj-mobile-menu {
      position: fixed; top: 44px; right: 0; z-index: 600;
      min-width: 220px; max-width: 90vw;
      background: ${COLORS.dark};
      border: 1px solid ${COLORS.green};
      display: none; flex-direction: column;
    }
    .zj-mobile-menu.zj-open { display: flex; }
    .zj-mobile-menu button {
      background: transparent; border: 0; color: ${COLORS.text};
      font-family: inherit; font-size: 14px; text-align: left;
      padding: 14px 16px; min-height: 48px; cursor: pointer;
    }
    .zj-mobile-menu button:hover { background: ${COLORS.medium}; }
    .zj-mobile-menu button.zj-disabled {
      color: ${COLORS.textDim}; opacity: 0.5; cursor: default;
    }
    .zj-mobile-menu .zj-sep {
      height: 1px; background: ${COLORS.light}; margin: 4px 0;
    }

    .zj-mobile-overlay {
      position: fixed; inset: 0; z-index: 700;
      background: ${COLORS.dark}; color: ${COLORS.text};
      display: none; flex-direction: column;
    }
    .zj-mobile-overlay.zj-open { display: flex; }
    .zj-mobile-overlay-header {
      display: flex; align-items: center; gap: 8px;
      padding: 10px 12px; border-bottom: 1px solid ${COLORS.light};
    }
    .zj-mobile-overlay-header .zj-back {
      background: transparent; border: 1px solid ${COLORS.light};
      color: ${COLORS.text}; font-family: inherit; font-size: 14px;
      padding: 8px 12px; min-height: 40px; cursor: pointer;
    }
    .zj-mobile-overlay-header .zj-title {
      color: ${COLORS.blue}; font-size: 16px; font-weight: 600;
    }
    .zj-mobile-overlay input.zj-search, .zj-mobile-overlay input.zj-name {
      margin: 10px 12px; padding: 12px; box-sizing: border-box;
      width: calc(100% - 24px);
      background: ${COLORS.medium}; color: ${COLORS.text};
      border: 1px solid ${COLORS.light}; font-family: inherit; font-size: 15px;
    }
    .zj-mobile-cards { flex: 1 1 auto; overflow-y: auto; padding: 0 12px; }
    .zj-mobile-card {
      background: ${COLORS.medium};
      border: 1px solid ${COLORS.light};
      padding: 12px; margin-bottom: 8px; cursor: pointer;
    }
    .zj-mobile-card .zj-card-title { color: ${COLORS.green}; font-size: 15px; }
    .zj-mobile-card .zj-card-title .zj-match { color: ${COLORS.yellow}; }
    .zj-mobile-card .zj-card-meta { color: ${COLORS.textDim}; font-size: 13px; margin-top: 4px; }
    .zj-mobile-footer {
      display: flex; gap: 8px; padding: 10px 12px;
      border-top: 1px solid ${COLORS.light};
    }
    .zj-mobile-footer button {
      flex: 1 1 0; background: transparent;
      border: 1px solid ${COLORS.green}; color: ${COLORS.green};
      font-family: inherit; font-size: 14px; min-height: 44px; cursor: pointer;
    }
    .zj-mobile-prompt-buttons { display: flex; gap: 8px; padding: 10px 12px; }
    .zj-mobile-prompt-buttons button {
      flex: 1 1 0; font-family: inherit; font-size: 14px; min-height: 44px;
      background: transparent; cursor: pointer;
    }
    .zj-mobile-prompt-buttons .zj-cancel { border: 1px solid ${COLORS.red}; color: ${COLORS.red}; }
    .zj-mobile-prompt-buttons .zj-accept { border: 1px solid ${COLORS.green}; color: ${COLORS.green}; }
    `;
    document.head.appendChild(style);
}

function buildDom() {
    const root = document.createElement("div");
    root.id = "zj-mobile-root";
    root.className = "zj-mobile-chrome";

    const topbar = document.createElement("div");
    topbar.className = "zj-mobile-topbar zj-mobile-chrome";
    const sessionBtn = document.createElement("button");
    sessionBtn.className = "zj-session";
    sessionBtn.addEventListener("click", () => openOverlay("sessions"));
    const paneBtn = document.createElement("button");
    paneBtn.className = "zj-pane";
    paneBtn.addEventListener("click", () => openOverlay("panes"));
    const hamburgerBtn = document.createElement("button");
    hamburgerBtn.className = "zj-hamburger";
    hamburgerBtn.textContent = "\u2630";
    hamburgerBtn.addEventListener("click", toggleMenu);
    topbar.append(sessionBtn, paneBtn, hamburgerBtn);

    const menu = buildMenu();

    const modbar = document.createElement("div");
    modbar.className = "zj-mobile-modbar zj-mobile-chrome";
    buildModifierBar(modbar);

    const panes = buildSwitcherOverlay("panes");
    const sessions = buildSwitcherOverlay("sessions");
    const newSession = buildNewSessionOverlay();

    root.append(topbar, menu, modbar, panes, sessions, newSession);
    document.body.appendChild(root);

    const returnPill = document.createElement("button");
    returnPill.id = "zj-mobile-return";
    returnPill.className = "zj-mobile-return";
    returnPill.textContent = "\u2630 Mobile UI";
    returnPill.addEventListener("click", returnToMobileUi);
    document.body.appendChild(returnPill);

    els = {
        root,
        topbar,
        sessionBtn,
        paneBtn,
        hamburgerBtn,
        menu,
        modbar,
        panes,
        sessions,
        newSession,
        returnPill,
    };
}

function buildMenu() {
    const menu = document.createElement("div");
    menu.className = "zj-mobile-menu zj-mobile-chrome";

    const renderToggle = document.createElement("button");
    renderToggle.dataset.role = "render-toggle";
    renderToggle.addEventListener("click", () => {
        state.renderMode =
            state.renderMode === "single-pane" ? "full" : "single-pane";
        if (state.renderMode === "single-pane") {
            state.fitEnabled = true;
        } else {
            state.fitEnabled = state.data.desktop_client_connected ? false : true;
        }
        closeMenu();
        sendRenderPrefs();
        render();
        reconcileSize();
    });

    const fitToggle = document.createElement("button");
    fitToggle.dataset.role = "fit-toggle";
    fitToggle.addEventListener("click", () => {
        if (!state.data.desktop_client_connected) {
            return;
        }
        state.fitEnabled = !state.fitEnabled;
        closeMenu();
        sendRenderPrefs();
        render();
        reconcileSize();
    });

    const changePane = document.createElement("button");
    changePane.textContent = "Change Pane";
    changePane.addEventListener("click", () => openOverlay("panes"));

    const changeSession = document.createElement("button");
    changeSession.textContent = "Change Session";
    changeSession.addEventListener("click", () => openOverlay("sessions"));

    const sep = document.createElement("div");
    sep.className = "zj-sep";

    const switchDesktop = document.createElement("button");
    switchDesktop.textContent = "Switch to Desktop";
    switchDesktop.addEventListener("click", switchToDesktop);

    menu.append(
        renderToggle,
        fitToggle,
        changePane,
        changeSession,
        sep,
        switchDesktop
    );
    return menu;
}

const MOD_CELLS = [
    { id: "esc", label: "ESC", kind: "key", key: "esc" },
    { id: "tab", label: "TAB", kind: "key", key: "tab" },
    { id: "ctrl", label: "CTRL", kind: "mod", mod: "ctrl" },
    { id: "alt", label: "ALT", kind: "mod", mod: "alt" },
    { id: "left", label: "\u2190", kind: "key", key: "left" },
    { id: "down", label: "\u2193", kind: "key", key: "down" },
    { id: "up", label: "\u2191", kind: "key", key: "up" },
    { id: "right", label: "\u2192", kind: "key", key: "right" },
    { id: "minus", label: "-", kind: "key", key: "minus" },
];

function buildModifierBar(modbar) {
    for (const cell of MOD_CELLS) {
        const btn = document.createElement("button");
        btn.dataset.cell = cell.id;
        btn.textContent = cell.label;
        btn.addEventListener("pointerdown", (ev) => {
            ev.preventDefault();
        });
        btn.addEventListener("click", () => handleModCell(cell));
        modbar.appendChild(btn);
    }
}

function buildSwitcherOverlay(kind) {
    const overlay = document.createElement("div");
    overlay.className = "zj-mobile-overlay zj-mobile-chrome";
    overlay.dataset.kind = kind;

    const header = document.createElement("div");
    header.className = "zj-mobile-overlay-header";
    const back = document.createElement("button");
    back.className = "zj-back";
    back.textContent = "\u2190 BACK";
    back.addEventListener("click", closeOverlay);
    const title = document.createElement("div");
    title.className = "zj-title";
    header.append(back, title);

    const search = document.createElement("input");
    search.className = "zj-search";
    search.type = "text";
    search.placeholder = kind === "panes" ? "Pane:" : "Session:";
    search.addEventListener("input", () => renderCards(kind));

    const cards = document.createElement("div");
    cards.className = "zj-mobile-cards";

    const footer = document.createElement("div");
    footer.className = "zj-mobile-footer";

    overlay.append(header, search, cards, footer);
    overlay._parts = { back, title, search, cards, footer };
    return overlay;
}

function buildNewSessionOverlay() {
    const overlay = document.createElement("div");
    overlay.className = "zj-mobile-overlay zj-mobile-chrome";
    overlay.dataset.kind = "new-session";

    const header = document.createElement("div");
    header.className = "zj-mobile-overlay-header";
    const title = document.createElement("div");
    title.className = "zj-title";
    title.textContent = "New Session";
    header.append(title);

    const name = document.createElement("input");
    name.className = "zj-name";
    name.type = "text";
    name.placeholder = "Name:";

    const buttons = document.createElement("div");
    buttons.className = "zj-mobile-prompt-buttons";
    const cancel = document.createElement("button");
    cancel.className = "zj-cancel";
    cancel.textContent = "Cancel";
    cancel.addEventListener("click", () => openOverlay("sessions"));
    const accept = document.createElement("button");
    accept.className = "zj-accept";
    accept.textContent = "Accept";
    accept.addEventListener("click", () => {
        navigateToSession(name.value.trim());
    });
    buttons.append(cancel, accept);

    overlay.append(header, name, buttons);
    overlay._parts = { name };
    return overlay;
}

function toggleMenu() {
    els.menu.classList.toggle("zj-open");
    renderMenu();
}

function closeMenu() {
    els.menu.classList.remove("zj-open");
}

function openOverlay(kind) {
    closeMenu();
    state.activeOverlay = kind;
    render();
    if (kind === "panes" || kind === "sessions") {
        startActivityTimer();
    } else {
        stopActivityTimer();
    }
    if (kind === "sessions") {
        requestSessionList();
    }
}

function sendControl(payload) {
    if (window.__zjSendControl) {
        window.__zjSendControl(payload);
    }
}

function requestSessionList() {
    sendControl({ type: "RequestSessionList" });
}

function sendRenderPrefs() {
    sendControl({
        type: "SetMobileRenderPreferences",
        single_pane: state.renderMode === "single-pane",
        fit: state.fitEnabled,
    });
}

function closeOverlay() {
    state.activeOverlay = "none";
    stopActivityTimer();
    render();
}

function switchToDesktop() {
    closeMenu();
    state.activeOverlay = "none";
    sendControl({
        type: "SetMobileRenderPreferences",
        single_pane: false,
        fit: true,
    });
    setActivationPreference("off");
    state.active = false;
    document.body.classList.remove("zj-mobile-active");
    render();
    reconcileSize();
}

function returnToMobileUi() {
    setActivationPreference("on");
    evaluateActivation();
}

function handleModCell(cell) {
    if (cell.kind === "mod") {
        state.armed[cell.mod] = !state.armed[cell.mod];
        renderModifierBar();
        return;
    }
    const bytes = serializeKey(cell.key, state.armed);
    state.armed.ctrl = false;
    state.armed.alt = false;
    renderModifierBar();
    const send = ctx.getSendAnsiKey();
    if (send) {
        send(bytes);
    }
}

function modifierParam(armed, shift) {
    let m = 1;
    if (shift) m += 1;
    if (armed.alt) m += 2;
    if (armed.ctrl) m += 4;
    return m;
}

function ctrlByte(ch) {
    const c = ch.charCodeAt(0);
    if (ch === "@" || ch === " ") return "\x00";
    if (ch >= "a" && ch <= "z") return String.fromCharCode(c - 0x60);
    if (ch >= "A" && ch <= "Z") return String.fromCharCode(c - 0x40);
    if (ch === "[") return "\x1b";
    if (ch === "\\") return "\x1c";
    if (ch === "]") return "\x1d";
    if (ch === "^") return "\x1e";
    if (ch === "_" || ch === "?") return "\x1f";
    return ch;
}

function serializeKey(key, armed) {
    const modified = armed.ctrl || armed.alt;
    const arrowLetter = { left: "D", down: "B", up: "A", right: "C" };
    if (key in arrowLetter) {
        const letter = arrowLetter[key];
        if (modified) {
            return `\x1b[1;${modifierParam(armed, false)}${letter}`;
        }
        return `\x1b[${letter}`;
    }
    if (key === "esc") {
        return armed.alt ? "\x1b\x1b" : "\x1b";
    }
    if (key === "tab") {
        return armed.alt ? "\x1b\t" : "\t";
    }
    if (key === "minus") {
        return mergeChar("-", armed);
    }
    return "";
}

function mergeChar(ch, armed) {
    let body = ch;
    if (armed.ctrl) {
        body = ctrlByte(ch);
    }
    if (armed.alt) {
        body = "\x1b" + body;
    }
    return body;
}

function installKeyMergeHook() {
    window.__zjMobileMergeKey = (payload) => {
        if (!state.active) {
            return payload;
        }
        if (!state.armed.ctrl && !state.armed.alt) {
            return payload;
        }
        if (typeof payload !== "string" || payload.length !== 1) {
            return payload;
        }
        const code = payload.charCodeAt(0);
        if (code < 0x20 || code === 0x7f) {
            return payload;
        }
        const merged = mergeChar(payload, state.armed);
        state.armed.ctrl = false;
        state.armed.alt = false;
        renderModifierBar();
        return merged;
    };
}

function installSoftKeyboardBinding() {
    window.addEventListener("zellij:soft-keyboard-visibility", (ev) => {
        state.kbdVisible = !!(ev.detail && ev.detail.visible);
        renderModifierBar();
        updateKeyboardOffset();
        reconcileSize();
    });
}

function installResizeReconcileHooks() {
    const onViewportChange = () => {
        updateKeyboardOffset();
        reconcileSize();
    };
    window.addEventListener("resize", onViewportChange);
    if (window.visualViewport) {
        window.visualViewport.addEventListener("resize", onViewportChange);
        window.visualViewport.addEventListener("scroll", onViewportChange);
    }
}

function updateKeyboardOffset() {
    const root = document.documentElement;
    const vv = window.visualViewport;
    if (!state.active || !vv) {
        root.style.setProperty("--zj-kbd-offset", "0px");
        return;
    }
    const occluded = Math.max(
        0,
        window.innerHeight - vv.height - vv.offsetTop
    );
    root.style.setProperty("--zj-kbd-offset", `${occluded}px`);
}

function getRenderSizing() {
    if (!state.active || state.fitEnabled) {
        return { pinned: false };
    }
    const size = state.data.desktop_size;
    if (
        !state.data.desktop_client_connected ||
        !size ||
        !size.cols ||
        !size.rows
    ) {
        return { pinned: false };
    }
    return { pinned: true, cols: size.cols, rows: size.rows };
}

function syncPan() {
    if (!window.__zjMobilePan) {
        return;
    }
    window.__zjMobilePan.setActive(getRenderSizing().pinned);
}

function reconcileSize() {
    const root = document.documentElement;
    if (!state.active) {
        root.style.setProperty("--zj-chrome-top", "0px");
        root.style.setProperty("--zj-chrome-bottom", "0px");
        window.dispatchEvent(new Event("zellij:rendering-resize"));
        requestAnimationFrame(syncPan);
        return;
    }
    const topVisible =
        state.renderMode === "single-pane" &&
        els.topbar.style.display !== "none";
    const topH = topVisible ? els.topbar.offsetHeight : 0;
    const modVisible = els.modbar.classList.contains("zj-visible");
    const modH = modVisible ? els.modbar.offsetHeight : 0;
    root.style.setProperty("--zj-chrome-top", `${topH}px`);
    root.style.setProperty("--zj-chrome-bottom", `${modH}px`);
    window.dispatchEvent(new Event("zellij:rendering-resize"));
    requestAnimationFrame(syncPan);
}

function render() {
    if (!els) {
        return;
    }
    const singlePane = state.renderMode === "single-pane";
    els.topbar.style.display = singlePane ? "flex" : "none";

    els.sessionBtn.textContent = state.data.session_name || "session";
    els.paneBtn.textContent = activePaneLabel();

    renderMenu();
    renderModifierBar();
    renderOverlays();
    renderReturnPill();
}

function renderReturnPill() {
    if (!els || !els.returnPill) {
        return;
    }
    const visible = activationPreference() === "off" && isMobileViewport();
    els.returnPill.style.display = visible ? "block" : "none";
}

function activePaneLabel() {
    const active = state.data.active_pane;
    if (!active) {
        return "\u2014";
    }
    const pane = state.data.panes.find(
        (p) => p.pane_id === active.pane_id && p.is_plugin === active.is_plugin
    );
    if (pane && pane.title) {
        return pane.title;
    }
    return active.pane_id != null ? `#${active.pane_id}` : "\u2014";
}

function renderMenu() {
    if (!els.menu.classList.contains("zj-open")) {
        return;
    }
    const renderToggle = els.menu.querySelector('[data-role="render-toggle"]');
    renderToggle.textContent =
        state.renderMode === "single-pane"
            ? "Render mode: Single pane"
            : "Render mode: Full render";

    const fitToggle = els.menu.querySelector('[data-role="fit-toggle"]');
    const canFit = state.data.desktop_client_connected;
    fitToggle.textContent = `Fit: ${state.fitEnabled ? "Enabled" : "Disabled"}`;
    fitToggle.classList.toggle("zj-disabled", !canFit);
}

function renderModifierBar() {
    const visible = state.active && state.kbdVisible;
    els.modbar.classList.toggle("zj-visible", visible);
    for (const cell of MOD_CELLS) {
        const btn = els.modbar.querySelector(`[data-cell="${cell.id}"]`);
        if (!btn) continue;
        if (cell.kind === "mod") {
            btn.classList.toggle("zj-armed", state.armed[cell.mod]);
        }
    }
}

function renderOverlays() {
    for (const kind of ["panes", "sessions", "new-session"]) {
        const overlay = els[camel(kind)];
        overlay.classList.toggle("zj-open", state.activeOverlay === kind);
    }
    if (state.activeOverlay === "panes") {
        setupPanesFooter();
        renderCards("panes");
        els.panes._parts.title.textContent = "Switch Pane";
    } else if (state.activeOverlay === "sessions") {
        setupSessionsFooter();
        renderSessionsHeader();
        renderCards("sessions");
    }
}

function camel(kind) {
    return kind === "new-session" ? "newSession" : kind;
}

function renderSessionsHeader() {
    const parts = els.sessions._parts;
    const welcome = state.data.is_welcome_screen;
    parts.title.textContent = welcome ? "Hi from Zellij!" : "Switch Session";
    parts.back.style.display = welcome ? "none" : "";
}

function setupPanesFooter() {
    const footer = els.panes._parts.footer;
    footer.innerHTML = "";
    const newTab = document.createElement("button");
    newTab.textContent = "+ New Tab";
    newTab.addEventListener("click", () => {
        sendControl({ type: "NewTab" });
        closeOverlay();
    });
    footer.append(newTab);
    const active = state.data.active_pane;
    if (active && active.tab_position != null) {
        const newPane = document.createElement("button");
        newPane.textContent = "+ New Pane";
        newPane.addEventListener("click", () => {
            sendControl({
                type: "NewPaneInTab",
                tab_id: active.tab_position,
            });
            closeOverlay();
        });
        footer.append(newPane);
    }
}

function setupSessionsFooter() {
    const footer = els.sessions._parts.footer;
    footer.innerHTML = "";
    const newSession = document.createElement("button");
    newSession.textContent = "+ New Session";
    newSession.addEventListener("click", () => openOverlay("new-session"));
    footer.append(newSession);
}

function renderCards(kind) {
    const overlay = els[camel(kind)];
    const parts = overlay._parts;
    const query = parts.search.value || "";
    parts.cards.innerHTML = "";
    const items =
        kind === "panes" ? paneCardData() : sessionCardData();
    const matched = filterAndSort(items, query, (it) => it.searchText);
    for (const { item, indices } of matched) {
        parts.cards.appendChild(renderCard(kind, item, indices));
    }
}

function paneCardData() {
    const tabsByPos = {};
    for (const t of state.data.tabs) {
        tabsByPos[t.position] = t.name;
    }
    const active = state.data.active_pane;
    const now = nowSecs();
    return state.data.panes.map((p) => {
        const title = p.title || `#${p.pane_id}`;
        const isCurrent =
            active &&
            active.pane_id === p.pane_id &&
            active.is_plugin === p.is_plugin;
        const tabName = tabsByPos[p.tab_position] || "";
        const activity = isCurrent
            ? "[CURRENT PANE]"
            : `Active ${formatActivity(p.last_activity_secs_ago, now)}`;
        return {
            searchText: title,
            title,
            meta: `${tabName}, ${activity}`,
            pane_id: p.pane_id,
            is_plugin: p.is_plugin,
        };
    });
}

function sessionCardData() {
    const current = state.data.session_name;
    const now = nowSecs();
    const base = state.data.now_secs || now;
    return state.data.sessions
        .filter(
            (s) => s.name !== current && s.web_clients_allowed !== false
        )
        .map((s) => {
            const createdSecsAgo =
                s.creation_secs_ago != null
                    ? s.creation_secs_ago + (now - base)
                    : null;
            const counts = `${s.tab_count} tabs, ${s.pane_count} panes, ${s.connected_clients} client(s)`;
            const created =
                createdSecsAgo != null
                    ? `Created ${formatActivity(createdSecsAgo, now)}`
                    : "";
            return {
                searchText: s.name,
                title: s.name,
                meta: created ? `${counts} · ${created}` : counts,
                name: s.name,
            };
        });
}

function renderCard(kind, item, indices) {
    const card = document.createElement("div");
    card.className = "zj-mobile-card";
    const title = document.createElement("div");
    title.className = "zj-card-title";
    title.append(highlight(item.title, indices));
    const meta = document.createElement("div");
    meta.className = "zj-card-meta";
    meta.textContent = item.meta;
    card.append(title, meta);
    card.addEventListener("click", () => {
        if (kind === "sessions") {
            navigateToSession(item.name);
        } else {
            sendControl({
                type: "FocusPane",
                pane_id: item.pane_id,
                is_plugin: item.is_plugin,
            });
            closeOverlay();
        }
    });
    return card;
}

function highlight(text, indices) {
    const frag = document.createDocumentFragment();
    const set = new Set(indices || []);
    let run = "";
    let runMatch = false;
    const flush = () => {
        if (!run) return;
        if (runMatch) {
            const span = document.createElement("span");
            span.className = "zj-match";
            span.textContent = run;
            frag.append(span);
        } else {
            frag.append(document.createTextNode(run));
        }
        run = "";
    };
    for (let i = 0; i < text.length; i++) {
        const m = set.has(i);
        if (m !== runMatch && run) {
            flush();
        }
        runMatch = m;
        run += text[i];
    }
    flush();
    return frag;
}

function filterAndSort(items, query, keyOf) {
    if (!query) {
        return items
            .slice()
            .sort((a, b) => keyOf(a).localeCompare(keyOf(b)))
            .map((item) => ({ item, indices: [] }));
    }
    const scored = [];
    for (const item of items) {
        const res = fuzzyScore(keyOf(item), query);
        if (res) {
            scored.push({ item, score: res.score, indices: res.indices });
        }
    }
    scored.sort((a, b) => {
        if (b.score !== a.score) return b.score - a.score;
        return keyOf(a.item).localeCompare(keyOf(b.item));
    });
    return scored;
}

function fuzzyScore(text, query) {
    const lowerText = text.toLowerCase();
    const lowerQuery = query.toLowerCase();
    let score = 0;
    let ti = 0;
    let prevMatch = -2;
    const indices = [];
    for (let qi = 0; qi < lowerQuery.length; qi++) {
        const qc = lowerQuery[qi];
        let found = -1;
        for (let i = ti; i < lowerText.length; i++) {
            if (lowerText[i] === qc) {
                found = i;
                break;
            }
        }
        if (found === -1) {
            return null;
        }
        indices.push(found);
        score += 16;
        if (found === prevMatch + 1) {
            score += 8;
        }
        if (found === 0 || /\W/.test(lowerText[found - 1])) {
            score += 4;
        }
        score -= found - ti;
        prevMatch = found;
        ti = found + 1;
    }
    return { score, indices };
}

function formatActivity(secsAgo, now) {
    if (secsAgo == null) {
        return "\u2014";
    }
    const diff = secsAgo;
    if (diff < 5) return "just now";
    if (diff < 60) return `${diff}s ago`;
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86400)}d ago`;
}

function startActivityTimer() {
    stopActivityTimer();
    activityTimer = setInterval(() => {
        if (
            state.activeOverlay === "panes" ||
            state.activeOverlay === "sessions"
        ) {
            renderCards(state.activeOverlay);
        }
    }, 1000);
}

function stopActivityTimer() {
    if (activityTimer) {
        clearInterval(activityTimer);
        activityTimer = null;
    }
}

function navigateToSession(name) {
    const baseUrl = getBaseUrl();
    if (name) {
        window.location.href = `${baseUrl}/${encodeURIComponent(name)}`;
    } else {
        window.location.href = baseUrl;
    }
}

function emptyMobileState() {
    return {
        session_name: "",
        now_secs: Math.floor(Date.now() / 1000),
        is_welcome_screen: false,
        desktop_client_connected: false,
        desktop_size: null,
        active_pane: null,
        tabs: [],
        panes: [],
        sessions: [],
        render_prefs: {
            single_pane: true,
            fit: true,
            active_pane_is_fullscreen: false,
        },
    };
}
