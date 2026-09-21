export function installImeBypass(term, sendFunction) {
    if (typeof window.__zjImeBypass === "undefined") {
        window.__zjImeBypass = {
            installed: false,
            sendFn: sendFunction,
            lastKeyWasProcess: false,
            deadKeyComposition: false,
        };
    }
    window.__zjImeBypass.sendFn = sendFunction;

    if (window.__zjImeBypass.installed) {
        return;
    }
    window.__zjImeBypass.installed = true;
    const state = window.__zjImeBypass;

    if (typeof state.deadKeyComposition === "undefined") {
        state.deadKeyComposition = false;
    }

    const isFirefox =
        typeof navigator !== "undefined" &&
        navigator.userAgent.includes("Firefox/");

    const resetImeState = () => {
        state.lastKeyWasProcess = false;
        state.deadKeyComposition = false;
    };

    document.addEventListener(
        "keydown",
        (ev) => {
            const isProcess = ev.key === "Process" || ev.keyCode === 229;
            if (isProcess) {
                // Latch until the commit input consumes it. It must NOT be
                // cleared by other keydowns: Firefox dispatches the final
                // printable keydown between Process and input, and clearing
                // here dropped the commit, leaving stale text in the helper
                // textarea for the next accent ("l á", "dl élio"). It also
                // must NOT use a time window: any insertText shortly after
                // an accent (e.g. the next "l" 250ms later) is a normal key
                // xterm already forwarded, and re-sending it duplicates the
                // character ("déllio").
                state.lastKeyWasProcess = true;
            } else if (isFirefox && ev.key === "Dead") {
                state.deadKeyComposition = true;
            } else if (ev.key === "Escape") {
                resetImeState();
            } else if (
                isFirefox &&
                state.deadKeyComposition &&
                !ev.isComposing &&
                typeof ev.key === "string" &&
                ev.key.length === 1 &&
                !ev.ctrlKey &&
                !ev.altKey &&
                !ev.metaKey
            ) {
                // A plain key outside any composition while the dead-key
                // flag is set means the composition was abandoned; drop the
                // flag so later words are never suppressed. The Process
                // latch is intentionally left untouched.
                state.deadKeyComposition = false;
            }

            /*
             * Firefox/Linux dead-key workaround.
             *
             * Firefox can emit the printable keydown while a dead-key
             * composition is still active and then commit the composed
             * character through Process + input(insertText).
             *
             * xterm.js can forward the intermediate character, producing:
             *
             *     ´ + a -> aá
             *
             * Stop only the intermediate printable keydown. Do NOT call
             * preventDefault(), because Firefox must still complete the
             * native composition.
             */
            if (
                isFirefox &&
                state.deadKeyComposition &&
                ev.isComposing &&
                typeof ev.key === "string" &&
                ev.key.length === 1 &&
                !ev.ctrlKey &&
                !ev.altKey &&
                !ev.metaKey
            ) {
                ev.stopPropagation();
            }
        },
        true
    );

    const attach = () => {
        const ta = term && term._core && term._core.textarea;
        if (!ta) {
            setTimeout(attach, 50);
            return;
        }

        // compositionstart/end are authoritative for the dead-key flag.
        // A Dead keydown alone is not enough: if a commit is missed, the
        // flag would stay latched and suppress the next word.
        ta.addEventListener(
            "compositionstart",
            () => {
                if (isFirefox) {
                    state.deadKeyComposition = true;
                }
            },
            true
        );
        ta.addEventListener("compositionend", () => {
            // The final insertText may still be in flight; clear on next tick.
            setTimeout(() => {
                state.deadKeyComposition = false;
            }, 0);
        });
        ta.addEventListener("blur", resetImeState);

        ta.addEventListener(
            "input",
            (ev) => {
                if (
                    state.lastKeyWasProcess &&
                    ev.inputType === "insertText" &&
                    !ev.isComposing &&
                    ev.data
                ) {
                    // Dead-key/IME commit: xterm ignores this input, so
                    // forward it here.
                    state.sendFn(ev.data);
                    // Shield xterm's async textarea diff
                    // (_handleAnyTextareaChanges) from seeing this commit
                    // again: it samples the textarea at Process keydown
                    // time and diffs later. Clearing here makes that diff
                    // empty. (xterm's own input listener was registered
                    // before ours and already ran; it ignores this input.)
                    ev.stopPropagation();
                    ev.target.value = "";
                    resetImeState();
                } else if (
                    ev.inputType === "insertText" &&
                    !ev.isComposing &&
                    ev.data &&
                    ev.data.length === 1
                ) {
                    // Firefox redundancy: xterm already forwarded this
                    // printable key via keydown, but the char also stayed in
                    // the helper textarea (e.g. space), where it polluted
                    // the next dead-key commit and xterm's async diff.
                    // Drop the residue without sending anything. Paste and
                    // multi-char input are never touched.
                    ev.target.value = "";
                }
                // Intermediate (isComposing) inputs: leave the textarea and
                // flags alone, the commit handler above finishes the job.
            },
            true
        );
    };

    attach();
}
