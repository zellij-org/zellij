/**
 * Terminal initialization and management
 */

import { build_link_handler } from "./links.js";

/**
 * Force every WebGL context created on this page to enable
 * `preserveDrawingBuffer`. The pinch overlay snapshots the
 * xterm.js canvas via `drawImage`, which returns blank pixels on
 * a WebGL canvas that has been composited (default behaviour).
 * Setting the flag makes the drawing buffer survive compositing,
 * which is what `drawImage` reads. The cost is a small perf hit
 * because the browser can no longer discard the buffer after
 * paint; acceptable for the use case.
 *
 * Must run before xterm.js's WebglAddon initialises its context.
 * Idempotent via the global guard.
 */
function ensurePreserveDrawingBuffer() {
    if (window.__zjPreserveDrawingBuffer) return;
    window.__zjPreserveDrawingBuffer = true;
    const orig = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function (type, options) {
        if (type === "webgl" || type === "webgl2") {
            options = Object.assign({}, options || {}, {
                preserveDrawingBuffer: true,
            });
        }
        return orig.call(this, type, options);
    };
}

/**
 * Initialize the terminal with all required addons and configuration
 * @returns {object} Object containing term and fitAddon instances
 */
export function initTerminal() {
    ensurePreserveDrawingBuffer();
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
 * Bounds applied to any font size we accept from config, server, or
 * a pinch gesture. The lower bound keeps the terminal legible enough
 * for xterm.js's WebGL renderer to draw without artifacts; the upper
 * bound stops a runaway pinch from producing single-cell grids.
 */
export const MIN_FONT_SIZE_PX = 6;
export const MAX_FONT_SIZE_PX = 96;

/**
 * Choose the effective terminal font size, assign it, and re-fit
 * the addon. The pinched font size is intentionally NOT persisted
 * across reloads — a refresh always falls back to the server-/
 * browser-driven default. Keeping pinch state ephemeral avoids the
 * footgun where a prior session's zoom would push a freshly
 * attached mobile client past the threshold checks.
 *
 * The `fit()` call here is load-bearing for the SetConfig flow.
 * It synchronously resizes `term` so the SetConfig handler can
 * detect a dim change (by comparing pre- and post-call rows/cols)
 * and broadcast a `RenderingPreference`-tagged resize to the
 * server. SetConfig represents a *rendering* preference change —
 * NOT a device-viewport change — so the `RenderingPreference`
 * cause prevents the server's mobile-mode re-evaluation. The
 * server still must learn about the new cell count: it owns the
 * pane layout and serialises terminal output at the client's grid
 * size. An earlier version of this code intentionally suppressed
 * the resize broadcast on the theory that "rendering preference
 * means server doesn't need to know"; that was wrong and caused
 * the server to render at the original (pre-font-change) grid
 * size, producing scrambled output on the mobile client.
 *
 * The runtime pinch path lives in `input.js` and intentionally
 * does NOT call fit() — there the dispatched
 * `zellij:rendering-resize` event triggers the same broadcast via
 * a different path. The two flows converge on the same server
 * message; only the trigger differs.
 */
export function applyFontSize(term, fitAddon, requestedPx) {
    const requested =
        typeof requestedPx === "number" && requestedPx > 0
            ? requestedPx
            : term.options.fontSize || 12;
    const effective = clampFontSize(requested);
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
