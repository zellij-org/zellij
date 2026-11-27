# AGENTS.md

Notes for AI coding agents working in this repository. Written for a reader
that has no memory of this project and cannot ask anyone.

## Commands that change state

`zellij` exposes 17 commands. 4 of them change something that
outlives the process.

| Command | What it changes | If you only meant to look |
|---|---|---|
| `zellij kill-session` | Terminates a running session and the processes in it. | `zellij list-sessions` |
| `zellij kill-all-sessions` | Terminates every running session and the processes in them. | `zellij list-sessions` |
| `zellij delete-session` | Deletes a session's saved state from disk. Not recoverable. | `zellij list-sessions` |
| `zellij delete-all-sessions` | Deletes every session's saved state from disk. Not recoverable. | `zellij list-sessions` |

## Commands that only read

`zellij list-sessions`, `zellij list-aliases`, `zellij watch`, `zellij subscribe`

## Commands that start or control a process

These do not write files themselves. What they start can do anything.

| Command | What it does |
|---|---|
| `zellij attach` | Attaches to a session, creating it if it does not exist. |
| `zellij run` | Runs a command in a new pane. Whatever it runs can do anything. |
| `zellij edit` | Opens a file in a new pane using $EDITOR, which may write it. |
| `zellij plugin` | Loads a plugin in a new pane. |
| `zellij web` | Runs a web server serving terminal sessions over the network. |

## Commands not classified here

`zellij action`, `zellij pipe`, `zellij options`, `zellij setup`

**Unclassified is not the same as safe.** It means nobody has written it down
yet. Read the implementation before running one.

## Before running anything

- Being able to run a command is not being allowed to run it.
- `zellij kill-session` and `zellij list-sessions` differ by one word and do very different things.
- If you cannot tell what a command does, say so rather than guessing. An
  honest `unknown` costs a question; a confident wrong answer costs a rollback.

<sub>Command list verified against af38660c5884 on 2026-09-09.</sub>
