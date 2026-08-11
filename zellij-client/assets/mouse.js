export function installMouseHandlers(term, terminalElement, sendFunction) {
    let prev_col = 0;
    let prev_row = 0;

    // xterm.js doesn't emit mousemove (xtermjs/xterm.js#1062); synthesize SGR motion reports.
    terminalElement.addEventListener("mousemove", function (event) {
        if (event.buttons == 0) {
            let coordEvent = event;
            if (window.__zjMobilePan && window.__zjMobilePan.isActive()) {
                const off = window.__zjMobilePan.getOffset();
                coordEvent = {
                    clientX: event.clientX + off.x,
                    clientY: event.clientY + off.y,
                };
            }
            const { col, row } = term._core._mouseService.getMouseReportCoords(
                coordEvent,
                terminalElement
            );
            if (prev_col != col || prev_row != row) {
                sendFunction(`\x1b[<35;${col + 1};${row + 1}M`);
            }
            prev_col = col;
            prev_row = row;
        }
    });

    document.addEventListener("contextmenu", function (event) {
        if (event.altKey) {
            event.preventDefault();
        }
    });
}
