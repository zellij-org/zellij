/**
 * Terminal initialization and management
 */

import { build_link_handler } from "./links.js";

/**
 * Initialize the terminal with all required addons and configuration
 * @returns {object} Object containing term and fitAddon instances
 */
export function initTerminal() {
    const term = new Terminal({
        fontFamily: "Monospace",
        allowProposedApi: true,
        scrollback: 0,
    });
    // for debugging
    window.term = term;
    const fitAddon = new FitAddon.FitAddon();
    const clipboardAddon = new ClipboardAddon.ClipboardAddon();

    const { linkHandler, activateLink } = build_link_handler();
    const webLinksAddon = new WebLinksAddon.WebLinksAddon(
        activateLink,
        linkHandler
    );
    term.options.linkHandler = linkHandler;

    const webglAddon = new WebglAddon.WebglAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(clipboardAddon);
    term.loadAddon(webLinksAddon);
    webglAddon.onContextLoss((e) => {
        // TODO: reload, or?
        webglAddon.dispose();
    });
    term.loadAddon(webglAddon);
    term.open(document.getElementById("terminal"));
    fitAddon.fit();
    term.focus();
    return { term, fitAddon };
}

/**
 * localStorage key used by the pinch-to-zoom gesture to remember
 * the user's preferred terminal font size across reloads.
 */
const FONT_SIZE_STORAGE_KEY = "zellij.fontSize";
/**
 * Bounds applied to any font size we accept from config, server, or
 * a pinch gesture. The lower bound keeps the terminal legible enough
 * for xterm.js's WebGL renderer to draw without artifacts; the upper
 * bound stops a runaway pinch from producing single-cell grids.
 */
export const MIN_FONT_SIZE_PX = 6;
export const MAX_FONT_SIZE_PX = 96;

/**
 * Read the persisted (pinch-set) terminal font size, if any.
 * Returns `null` when the key is missing, malformed, or out of bounds.
 */
export function getPersistedFontSize() {
    try {
        const raw = window.localStorage.getItem(FONT_SIZE_STORAGE_KEY);
        if (raw === null) {
            return null;
        }
        const value = parseInt(raw, 10);
        if (
            !Number.isFinite(value) ||
            value < MIN_FONT_SIZE_PX ||
            value > MAX_FONT_SIZE_PX
        ) {
            return null;
        }
        return value;
    } catch (e) {
        // localStorage may be unavailable in privacy modes / sandboxed
        // contexts; degrade silently to "no persisted value".
        return null;
    }
}

/**
 * Persist the user's chosen font size (set by the pinch gesture) so
 * it survives reloads. Silently ignored if storage is unavailable.
 */
export function setPersistedFontSize(px) {
    try {
        window.localStorage.setItem(FONT_SIZE_STORAGE_KEY, String(px));
    } catch (e) {
        // Same rationale as getPersistedFontSize: just skip.
    }
}

/**
 * Choose the effective terminal font size, assign it, and re-fit
 * the addon. A previously persisted (pinch-set) value always wins
 * over the caller's `requestedPx` so the user's manual zoom
 * survives an incoming `SetConfig` from the server.
 *
 * The `fit()` call here is load-bearing for the SetConfig flow.
 * It synchronously resizes `term` so the SetConfig handler's
 * downstream `proposeDimensions == term.rows/cols` comparison
 * short-circuits and no follow-up `TerminalResize` is sent. That
 * matters because SetConfig represents a *rendering* preference
 * change (which cells to draw at which pixel size), NOT a device
 * viewport change. The server already learned the client's
 * viewport from `wsControl.onopen`'s initial `TerminalResize`; if
 * we broadcast a second resize derived from the new font size,
 * `ReevaluateMobileMode` on the server side could push the client
 * out of mobile mode for purely cosmetic reasons (e.g., a
 * previously persisted small font enlarges cols/rows past the
 * mobile threshold).
 *
 * The runtime pinch path lives in `input.js` and intentionally
 * does NOT call fit() — there the resize IS the user's intent and
 * the server must learn about the new grid so the mobile plugin
 * redraws the keyboard within the new bounds. That asymmetry is
 * deliberate.
 */
export function applyFontSize(term, fitAddon, requestedPx) {
    const persisted = getPersistedFontSize();
    const requested =
        typeof requestedPx === "number" && requestedPx > 0
            ? requestedPx
            : term.options.fontSize || 12;
    const effective = clampFontSize(persisted !== null ? persisted : requested);
    if (term.options.fontSize !== effective) {
        term.options.fontSize = effective;
    }
    try {
        fitAddon.fit();
    } catch (e) {
        // fit() throws when the host element is not measurable
        // (e.g. very early in bootstrap). The next resize/fit pass
        // will pick the new size up.
    }
}

/**
 * Clamp an arbitrary numeric font size into the legible/sane range
 * enforced by `applyFontSize`. Exported so the pinch handler can
 * apply the same clamp before deciding whether the value has
 * actually changed.
 */
export function clampFontSize(px) {
    const n = Math.round(Number(px) || 0);
    if (n < MIN_FONT_SIZE_PX) return MIN_FONT_SIZE_PX;
    if (n > MAX_FONT_SIZE_PX) return MAX_FONT_SIZE_PX;
    return n;
}
