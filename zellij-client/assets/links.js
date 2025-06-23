/**
 * Link handling functions for terminal
 */

/**
 * Build a link handler object for terminal links
 * @returns {object} Object containing linkHandler and activateLink function
 */
export function build_link_handler() {
    let _linkPopup;
    
    function removeLinkPopup(event, text, range) {
        if (_linkPopup) {
            _linkPopup.remove();
            _linkPopup = undefined;
        }
    }

    function showLinkPopup(event, text, range) {
        let popup = document.createElement('div');
        popup.classList.add('xterm-link-popup');
        popup.style.position = 'absolute';
        popup.style.top = (event.clientY + 25) + 'px';
        popup.style.left = (event.clientX + 25) + 'px';
        popup.style.fontSize = 'small';
        popup.style.lineBreak = 'normal';
        popup.style.padding = '4px';
        popup.style.minWidth = '15em';
        popup.style.maxWidth = '80%';
        popup.style.border = 'thin solid';
        popup.style.borderRadius = '6px';
        popup.style.background = '#6c4c4c';
        popup.style.borderColor = '#150262';
        popup.innerText = "Shift-Click: " + text;
        const topElement = event.target.parentNode;
        topElement.appendChild(popup);
        const popupHeight = popup.offsetHeight;
        _linkPopup = popup;
    }
    
    function activateLink(event, uri) {
        const newWindow = window.open(uri, '_blank');
        if (newWindow) newWindow.opener = null; // prevent the opened link from gaining access to the terminal instance
    }

    let linkHandler = {};
    linkHandler.hover = showLinkPopup;
    linkHandler.leave = removeLinkPopup;
    linkHandler.activate = activateLink;
    return { linkHandler, activateLink };
}
