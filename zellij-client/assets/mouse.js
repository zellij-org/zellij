/**
 * Mouse motion reporting and context-menu suppression.
 */

/**
 * Install the mouse handlers. Zellij always requests AnyEvent mouse handling,
 * but xterm.js doesn't listen to mousemove (xtermjs/xterm.js#1062), so motion
 * events are synthesized here from x/y coords via xterm.js's internal mouse
 * service. Alt-right-click's context menu is suppressed so it can be used to
 * ungroup panes.
 */
export function installMouseHandlers(term, terminalElement, sendFunction) {
    let prev_col = 0;
    let prev_row = 0;

    terminalElement.addEventListener("mousemove", function (event) {
        if (event.buttons == 0) {
            // no buttons pressed — plain motion
            const { col, row } = term._core._mouseService.getMouseReportCoords(
                event,
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
            // suppress the menu so alt-right-click can ungroup panes
            event.preventDefault();
        }
    });
}
