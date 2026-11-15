import assert from "node:assert/strict";
import test from "node:test";
import { shouldDedupeCommittedChar } from "./soft-keyboard.js";

test("intentional repeated characters are never time-window deduped", () => {
    for (const ch of ["s", "o", "&", "1", "a"]) {
        assert.equal(shouldDedupeCommittedChar(ch, 0, 8), false);
        assert.equal(shouldDedupeCommittedChar(ch, 1, 8), false);
    }
});

test("backspace within the window is deduped; outside it is not", () => {
    assert.equal(shouldDedupeCommittedChar("\x7f", 0, 8), true);
    assert.equal(shouldDedupeCommittedChar("\x7f", 7, 8), true);
    assert.equal(shouldDedupeCommittedChar("\x7f", 8, 8), false);
    assert.equal(shouldDedupeCommittedChar("\x7f", 20, 8), false);
});

test("paste of ssh keeps both s characters under the helper contract", () => {
    const sent = [];
    let lastBackspaceAt = null;
    const DEDUPE_MS = 8;
    const now = 1000;
    for (const ch of "ssh") {
        if (ch === "\x7f") {
            if (
                lastBackspaceAt !== null &&
                shouldDedupeCommittedChar(ch, now - lastBackspaceAt, DEDUPE_MS)
            ) {
                continue;
            }
            lastBackspaceAt = now;
        } else {
            lastBackspaceAt = null;
        }
        sent.push(ch);
    }
    assert.equal(sent.join(""), "ssh");
});

test("a text keystroke resets the backspace duplicate window", () => {
    const sent = [];
    let lastBackspaceAt = null;
    const DEDUPE_MS = 8;
    const dispatch = (ch, now) => {
        if (ch === "\x7f") {
            if (
                lastBackspaceAt !== null &&
                shouldDedupeCommittedChar(ch, now - lastBackspaceAt, DEDUPE_MS)
            ) {
                return;
            }
            lastBackspaceAt = now;
        } else {
            lastBackspaceAt = null;
        }
        sent.push(ch);
    };

    dispatch("\x7f", 1000);
    dispatch("x", 1002);
    dispatch("\x7f", 1004);
    assert.equal(
        sent.join(""),
        "\x7fx\x7f",
        "the second backspace is a separate edit, not the duplicate event"
    );
});
