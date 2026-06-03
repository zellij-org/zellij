/**
 * Pinch-to-zoom snapshot overlay.
 *
 * xterm.js's WebGL renderer reassigns canvas.width/height inside the fontSize
 * setter and term.resize, which clears the framebuffer. On mobile the next
 * frame is rAF-throttled during a touch gesture, so the cleared canvas gets
 * composited before xterm.js repaints — a visible blank flash. The overlay
 * hides that window: a <canvas> snapshot of the live pixels is pinned over the
 * xterm canvas while the gesture runs, re-blitted on every term.onRender (so
 * server data landing mid-pinch reaches the user), and removed on the next
 * render after the gesture ends.
 */

import { clampFontSize } from "./terminal.js";

/**
 * Create a pinch overlay controller bound to a terminal. Registers a single
 * term.onRender hook internally. Returns the methods the touch-gesture handler
 * drives: snapshot() on pinch activation, applyFontSize(px) on move,
 * armRemoval() on gesture end, destroy() on cancel.
 */
export function createPinchController(term) {
    let pinchOverlay = null;
    let pinchOverlayAwaitingRender = false;
    let pinchOverlaySafetyTimer = null;

    const destroy = () => {
        if (pinchOverlay) {
            pinchOverlay.remove();
            pinchOverlay = null;
        }
        pinchOverlayAwaitingRender = false;
        if (pinchOverlaySafetyTimer !== null) {
            clearTimeout(pinchOverlaySafetyTimer);
            pinchOverlaySafetyTimer = null;
        }
    };

    const snapshot = () => {
        destroy();
        if (!term.element) return;
        const sourceCanvases = term.element.querySelectorAll("canvas");
        if (sourceCanvases.length === 0) return;
        const ref = sourceCanvases[0];
        const rect = ref.getBoundingClientRect();
        if (rect.width <= 0 || rect.height <= 0) return;

        const overlay = document.createElement("canvas");
        overlay.width = ref.width;
        overlay.height = ref.height;
        const outer = document.getElementById("terminal");
        const bg = outer
            ? window.getComputedStyle(outer).backgroundColor
            : "transparent";
        Object.assign(overlay.style, {
            position: "fixed",
            left: rect.left + "px",
            top: rect.top + "px",
            width: rect.width + "px",
            height: rect.height + "px",
            zIndex: "9999",
            pointerEvents: "none",
            background: bg,
        });

        const ctx = overlay.getContext("2d");
        if (ctx) {
            // Composite every xterm.js canvas (WebGL render + cursor/selection/
            // link layers) so the snapshot matches what the user sees.
            for (const c of sourceCanvases) {
                try {
                    ctx.drawImage(c, 0, 0);
                } catch (e) {
                    // Tainted canvases throw; skip and continue.
                }
            }
        }

        document.body.appendChild(overlay);
        pinchOverlay = overlay;
    };

    // Re-draw the overlay with the current canvas state (from the onRender
    // hook). The resize-clear and the following drawImage run synchronously
    // inside the callback, so the browser never composites the cleared overlay.
    const refresh = () => {
        if (!pinchOverlay) return;
        if (!term.element) return;
        const sourceCanvases = term.element.querySelectorAll("canvas");
        if (sourceCanvases.length === 0) return;
        const ref = sourceCanvases[0];
        const rect = ref.getBoundingClientRect();
        if (rect.width <= 0 || rect.height <= 0) return;

        if (pinchOverlay.width !== ref.width) {
            pinchOverlay.width = ref.width;
        }
        if (pinchOverlay.height !== ref.height) {
            pinchOverlay.height = ref.height;
        }
        pinchOverlay.style.left = rect.left + "px";
        pinchOverlay.style.top = rect.top + "px";
        pinchOverlay.style.width = rect.width + "px";
        pinchOverlay.style.height = rect.height + "px";

        const ctx = pinchOverlay.getContext("2d");
        if (ctx) {
            ctx.clearRect(0, 0, pinchOverlay.width, pinchOverlay.height);
            for (const c of sourceCanvases) {
                try {
                    ctx.drawImage(c, 0, 0);
                } catch (e) {
                    // Tainted canvases throw; skip and continue.
                }
            }
        }
    };

    const armRemoval = () => {
        if (!pinchOverlay) return;
        pinchOverlayAwaitingRender = true;
        // Safety net: drop the overlay even if onRender never fires (e.g. the
        // server didn't respond). 600 ms covers a roundtrip + a few paints.
        if (pinchOverlaySafetyTimer !== null) {
            clearTimeout(pinchOverlaySafetyTimer);
        }
        pinchOverlaySafetyTimer = setTimeout(() => {
            pinchOverlaySafetyTimer = null;
            destroy();
        }, 600);
    };

    // Apply a new font size during a pinch. Deliberately does NOT call
    // fitAddon.fit(): fit() would call term.resize with the new dims, so the
    // resize handler would see term.rows/cols already matching
    // proposeDimensions() and short-circuit — never telling the server, which
    // would keep rendering the old grid and clip the mobile keyboard rows. By
    // skipping fit(), term.rows/cols stay at the OLD grid; the resize handler
    // then sees a real mismatch and emits the resize itself. The renderer
    // updates cell metrics synchronously on fontSize assignment, so
    // proposeDimensions is correct by the next frame. A dedicated
    // `zellij:rendering-resize` event (vs plain `resize`) routes to a
    // RenderingPreference cause so a pinch crossing the mobile threshold does
    // not auto-demote the client out of the mobile layout.
    const applyFontSize = (px) => {
        const clamped = clampFontSize(px);
        if (term.options.fontSize === clamped) {
            return;
        }
        term.options.fontSize = clamped;
        window.dispatchEvent(new CustomEvent("zellij:rendering-resize"));
    };

    // Single onRender hook. While the overlay is up, each render is a moment the
    // canvas underneath has fresh content — re-snapshot so server data mid-pinch
    // reaches the user. Once removal is armed (gesture ended), the next render
    // has the post-commit content: defer one rAF for the browser to composite
    // the refreshed overlay, then drop it.
    if (term && typeof term.onRender === "function") {
        term.onRender(() => {
            if (!pinchOverlay) return;
            refresh();
            if (pinchOverlayAwaitingRender) {
                pinchOverlayAwaitingRender = false;
                requestAnimationFrame(() => {
                    destroy();
                });
            }
        });
    }

    return { snapshot, refresh, applyFontSize, armRemoval, destroy };
}
