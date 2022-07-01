## ANSI/VT
This refers to the stream of text one receives when listening to the primary side of the pty.
For example `[31mHi [5mthere!` would print the word "Hi" in red and then the word "there!" in a blinking red. You can try this in your terminal with the echo command:

`echo -e "\033[31mHi \033[5mthere!"` (`\033` is an escape character).

## CSI (Control Sequence Identifier)
Part of the ANSI/VT specification that includes instructions for the terminal emulator. These instructions can be a style change (eg. change color to red) or cursor position change (eg. go to line x/y).

## OSC (Operating System Command)
Part of the ANSI/VT specification that includes instructions for the underlying operating system (eg. change window title).

## pty
A pty (pseudoterminal) is a character device that emulates a traditional terminal. It is a pair of processes (traditionally given culturally incensitive names, here they will be referred to as primary/secondary).

The primary part is the part connected to the terminal emulator. The emulator listens to this part of the channel, reads instructions from it that it uses in order to draw characters on the screen.
The secondary part is used by the program running inside the terminal (eg. the shell) in order to send those instructions.

In Zellij, there is one pty pair  for each terminal pane.
