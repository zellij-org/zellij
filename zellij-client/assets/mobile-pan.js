let term = null;
let containerEl = null;
let panX = 0;
let panY = 0;
let active = false;

function getScreenEl() {
    if (!term || !term.element) {
        return null;
    }
    return term.element;
}

function cellDims() {
    try {
        const cell =
            term && term._core && term._core._renderService &&
            term._core._renderService.dimensions &&
            term._core._renderService.dimensions.css &&
            term._core._renderService.dimensions.css.cell;
        if (cell && cell.width && cell.height) {
            return { width: cell.width, height: cell.height };
        }
    } catch (_) {}
    return null;
}

function contentSize() {
    const cell = cellDims();
    if (!cell) {
        return null;
    }
    return {
        width: cell.width * term.cols,
        height: cell.height * term.rows,
    };
}

function containerSize() {
    if (!containerEl) {
        return { width: 0, height: 0 };
    }
    const rect = containerEl.getBoundingClientRect();
    return { width: rect.width, height: rect.height };
}

function bounds() {
    const content = contentSize();
    if (!content) {
        return { maxX: 0, maxY: 0 };
    }
    const view = containerSize();
    return {
        maxX: Math.max(0, content.width - view.width),
        maxY: Math.max(0, content.height - view.height),
    };
}

function clamp() {
    const b = bounds();
    if (panX > b.maxX) panX = b.maxX;
    if (panY > b.maxY) panY = b.maxY;
    if (panX < 0) panX = 0;
    if (panY < 0) panY = 0;
}

function applyTransform() {
    const el = getScreenEl();
    if (!el) {
        return;
    }
    if (active) {
        el.style.transform = `translate3d(${-panX}px, ${-panY}px, 0)`;
        el.style.transformOrigin = "top left";
        el.style.willChange = "transform";
    } else {
        el.style.transform = "";
        el.style.willChange = "";
    }
}

function setActive(on) {
    const next = !!on;
    if (next === active) {
        if (active) {
            clamp();
            applyTransform();
        }
        return;
    }
    active = next;
    if (!active) {
        panX = 0;
        panY = 0;
    }
    document.body.classList.toggle("zj-mobile-panning", active);
    clamp();
    applyTransform();
}

function panBy(dx, dy) {
    if (!active) {
        return;
    }
    panX += dx;
    panY += dy;
    clamp();
    applyTransform();
}

function recompute() {
    if (!active) {
        return;
    }
    clamp();
    applyTransform();
}

export function initMobilePan(context) {
    term = context.term;
    containerEl = document.getElementById("terminal");
    if (window.__zjMobilePan) {
        return window.__zjMobilePan;
    }
    window.__zjMobilePan = {
        setActive,
        panBy,
        recompute,
        isActive: () => active,
        getOffset: () => ({ x: panX, y: panY }),
    };
    window.addEventListener("resize", recompute);
    if (window.visualViewport) {
        window.visualViewport.addEventListener("resize", recompute);
    }
    window.addEventListener("zellij:rendering-resize", () => {
        requestAnimationFrame(recompute);
    });
    return window.__zjMobilePan;
}
