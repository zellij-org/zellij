## ANSI/VT
This refers to the stream of text one receives when listening to the primary side of the pty.
For example `[31mHi [5mthere!` would print the word "Hi" in red and then the word "there!" in a blinking red. You can try this in your terminal with the echo command:

`echo -e "\033[31mHi \033[5mthere!"` (`\033` is an escape character).

---

zellij follows [DEC_ANSI_PARSER](https://vt100.net/emu/dec_ansi_parser) to handle ANSI escape code.

## CSI (Control Sequence Identifier)

Part of the ANSI/VT specification that includes instructions for the terminal emulator. These instructions can be a style change (eg. change color to red) or cursor position change (eg. go to line x/y).

## SGR (Select Graph Rendition)
It's a type of control code sequence used to control attributes such as color, boldness, underline, and others. The general form of the CSI SGR sequence is `CSI n m` where n is a numeric parameter, and m is the final character indicating Select Graphic Rendition.

## OSC (Operating System Command)
Part of the ANSI/VT specification that includes instructions for the underlying operating system (eg. change window title).

## DCS (Device Control String)
The DCS sequence is used to send device-specific control strings. The sequence begins with a DCS introducer and ends with a String Terminator (ST). The characters in between make up the device control string, which is specific to the device and the system it is being used on.


## Example -  `"Hello, \033[1;33;mWorld!\033[0m"`

1. `Hello,`: The parser starts in the `GROUND` state and simply prints these characters as normal text.
2. `\033`: This is the ASCII escape character, which takes the parser from the `GROUND` state to the `ESCAPE` state.
3. `[`: This character takes the parser from the `ESCAPE` state to the `CSI_ENTRY` state.
4. `1;33`: These characters are collected as parameters, taking the parser from the `CSI_ENTRY` state to the `CSI_PARAM` state.
5. `;`: The semicolon is an additional parameter character in the `CSI_PARAM` state.
6. `m`: This character is the final character of the CSI sequence. Since there are no intermediate characters in this sequence, it doesn't take the parser to the `CSI_INTERMEDIATE` state. Instead, it triggers the action associated with the CSI sequence (setting the text color to bright yellow), and the parser returns to the `GROUND` state.
7. `World!`: The parser is in the `GROUND` state and simply prints these characters as normal text, but they appear in bright yellow due to the preceding CSI sequence
8. `\033[0m`：This is another CSI sequence that **resets** the text color to its default value. The parser goes through the `ESCAPE`, `CSI_ENTRY`, and `CSI_PARAM` states just like before. The `m` character triggers the action associated with the CSI sequence, which in this case is resetting the text color.

> Note - According to [parser rust's implementation](https://docs.rs/vte/latest/vte/), the state machine expected that implementation of the `Perform` trait is provided which does something meaningful and useful with the parsed data. It's done in zellij's [Grid](../zellij-server/src/panes/grid.rs)

---

## pty
A pty (pseudoterminal) is a character device that emulates a traditional terminal. It is a pair of processes (traditionally given culturally incensitive names, here they will be referred to as primary/secondary).

The primary part is the part connected to the terminal emulator. The emulator listens to this part of the channel, reads instructions from it that it uses in order to draw characters on the screen.
The secondary part is used by the program running inside the terminal (eg. the shell) in order to send those instructions.

In Zellij, there is one pty pair  for each terminal pane.
