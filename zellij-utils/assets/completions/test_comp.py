#!/usr/bin/env python3
"""Behavior tests for zellij completion assets (bash, zsh and fish).

Run directly:
  python3 test_comp.py
  python3 test_comp.py --shell fish
"""

from __future__ import annotations

import argparse
import shlex
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence

SCRIPT_DIR = Path(__file__).resolve().parent
COMP_FILES = {
    "bash": SCRIPT_DIR / "comp.bash",
    "zsh": SCRIPT_DIR / "comp.zsh",
    "fish": SCRIPT_DIR / "comp.fish",
}

DEFAULT_SESSIONS = ["vitreous-galaxy", "work", "alpha"]
_active_shell = "bash"


class TestFailure(AssertionError):
    pass


def _quote_words(words: Iterable[str]) -> str:
    return " ".join(shlex.quote(w) for w in words)


def _run_bash(words: Sequence[str], sessions: list[str]) -> list[str]:
    words_expr = _quote_words(words)
    sessions_lines = "".join(
        f"    printf '%s\\n' {shlex.quote(s)}\n" for s in sessions
    )
    if not sessions_lines:
        sessions_lines = "    :\n"

    line = " ".join(words)
    script = f"""
source {shlex.quote(str(COMP_FILES['bash']))}

zellij() {{
  if [ "$1" = "list-sessions" ]; then
{sessions_lines}  fi
}}

COMP_WORDS=({words_expr})
COMP_CWORD=$((${{#COMP_WORDS[@]}} - 1))
COMP_LINE={shlex.quote(line)}
COMP_POINT=${{#COMP_LINE}}
COMPREPLY=()
_zellij zellij
printf '%s\\n' "${{COMPREPLY[@]}}"
"""

    result = subprocess.run(
        ["bash", "--noprofile", "--norc", "-c", script],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise TestFailure(
            f"bash scenario failed\\nwords={words}\\n"
            f"stdout:\\n{result.stdout}\\nstderr:\\n{result.stderr}\\n"
        )
    return [line for line in result.stdout.splitlines() if line]


def _run_zsh(words: Sequence[str], sessions: list[str]) -> list[str]:
    words_expr = " ".join(shlex.quote(w) for w in words)
    sessions_arr = " ".join(shlex.quote(s) for s in sessions)

    script = f"""
emulate -L zsh

typeset -ga __collected
__collected=()
typeset -ga words
words=(zellij "")
CURRENT=${{#words}}

# Collect candidates and apply simple prefix filtering.
compadd() {{
  while [[ $# -gt 0 && "$1" != "--" ]]; do shift; done
  [[ "$1" == "--" ]] && shift
  local __prefix="${{words[CURRENT]}}"
  local __w
  for __w in "$@"; do
    [[ -z "$__prefix" || "$__w" == "${{__prefix}}"* ]] && __collected+=("$__w")
  done
}}

source {shlex.quote(str(COMP_FILES['zsh']))}

zellij() {{
  if [[ "$1" == "list-sessions" ]]; then
    local __s
    for __s in {sessions_arr}; do
      print -r -- "$__s"
    done
  fi
}}

words=({words_expr})
CURRENT=${{#words}}
__collected=()

_zellij

printf '%s\\n' "${{__collected[@]}}"
"""

    result = subprocess.run(
        ["zsh", "--no-rcs", "-c", script],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise TestFailure(
            f"zsh scenario failed\\nwords={words}\\n"
            f"stdout:\\n{result.stdout}\\nstderr:\\n{result.stderr}\\n"
        )
    return [line for line in result.stdout.splitlines() if line]


def _run_fish(words: Sequence[str], sessions: list[str]) -> list[str]:
    sessions_echo = "\n".join(f"        echo {shlex.quote(s)}" for s in sessions)
    if not sessions_echo:
        sessions_echo = "        true"

    if words[-1] == "":
        cmdline = " ".join(words[:-1]) + " "
    else:
        cmdline = " ".join(words)

    script = f"""
function zellij
    if test "$argv[1]" = "list-sessions"
{sessions_echo}
    end
end

source {shlex.quote(str(COMP_FILES['fish']))}

complete -C {shlex.quote(cmdline)}
"""

    result = subprocess.run(
        ["fish", "--no-config", "-c", script],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise TestFailure(
            f"fish scenario failed\\nwords={words}\\n"
            f"stdout:\\n{result.stdout}\\nstderr:\\n{result.stderr}\\n"
        )

    completions: list[str] = []
    for line in result.stdout.splitlines():
        if line:
            completions.append(line.split("\t")[0])
    return completions


def run_completion(words: Sequence[str], sessions: Sequence[str] | None = None) -> list[str]:
    ss = list(sessions) if sessions is not None else list(DEFAULT_SESSIONS)
    if _active_shell == "fish":
        return _run_fish(words, ss)
    if _active_shell == "zsh":
        return _run_zsh(words, ss)
    return _run_bash(words, ss)


@dataclass
class Case:
    words: Sequence[str]
    expected: Sequence[str] | None = None
    sessions: Sequence[str] | None = None
    contains: Sequence[str] = ()
    excludes: Sequence[str] = ()
    sort: bool = False
    shells: tuple[str, ...] = ("bash", "zsh", "fish")

    def _ctx(self, shell: str) -> str:
        parts = [f"  shell:        {shell}", f"  words:        {list(self.words)}"]
        if self.sessions is not None:
            parts.append(f"  sessions:     {list(self.sessions)}")
        return "\n".join(parts)

    @property
    def label(self) -> str:
        return " ".join(self.words)

    def run(self, shell: str) -> None:
        completions = run_completion(self.words, self.sessions)

        if self.expected is not None:
            actual = sorted(completions) if self.sort else completions
            expected = sorted(self.expected) if self.sort else list(self.expected)
            if actual != expected:
                raise TestFailure(
                    f"exact match failed\\n{self._ctx(shell)}\\n"
                    f"  expected: {expected}\\n  actual:   {actual}"
                )

        for item in self.excludes:
            if item in completions:
                raise TestFailure(
                    f"unexpected item {item!r} present\\n{self._ctx(shell)}\\n"
                    f"  actual: {completions}"
                )

        missing = [item for item in self.contains if item not in completions]
        if missing:
            raise TestFailure(
                f"missing items\\n{self._ctx(shell)}\\n"
                f"  missing:  {missing}\\n  actual:   {completions}"
            )


CASES: list[Case] = [
    # Session completion in target subcommands.
    Case(["zellij", "attach", "v"], ["vitreous-galaxy"]),
    Case(["zellij", "a", "w"], ["work"]),
    Case(["zellij", "kill-session", "w"], ["work"]),
    Case(["zellij", "k", "w"], ["work"]),
    Case(["zellij", "watch", "a"], ["alpha"]),
    Case(["zellij", "w", "a"], ["alpha"]),
    Case(["zellij", "delete-session", "v"], ["vitreous-galaxy"]),
    Case(["zellij", "d", "v"], ["vitreous-galaxy"]),
    Case(["zellij", "--debug", "attach", "w"], ["work"]),
    Case(["zellij", "attach", ""], ["alpha", "vitreous-galaxy", "work"], sort=True),
    # Non-target context does not complete sessions.
    Case(["zellij", "ls", "v"], None, excludes=["vitreous-galaxy", "work", "alpha"]),
    Case(["zellij", "setup", ""], None, excludes=["vitreous-galaxy", "work", "alpha"]),
    Case(["zellij", ""], None, excludes=["vitreous-galaxy", "work", "alpha"]),
    # setup --generate-completion value completion.
    Case(["zellij", "setup", "--generate-completion", "b"], ["bash"]),
    Case(
        ["zellij", "setup", "--generate-completion", ""],
        ["bash", "elvish", "fish", "zsh", "powershell"],
        sort=True,
    ),
    # Custom session fixtures.
    Case(["zellij", "attach", "work"], ["work", "work-dev"], sessions=["work", "work-dev", "alpha"]),
    Case(["zellij", "k", "x"], [], sessions=["work", "alpha"]),
]


def test_user_requested_assertion_format() -> None:
    completions = run_completion(["zellij", "a", "v"])
    if len(completions) != 1:
        raise TestFailure(f"zellij a v<TAB> should yield 1 completion, got {completions}")
    if f"zellij a {completions[0]}" != "zellij a vitreous-galaxy":
        raise TestFailure(
            f"expected 'zellij a vitreous-galaxy', got 'zellij a {completions[0]}'"
        )


def run_suite(selected: str, shells: list[str]) -> int:
    global _active_shell

    failures: list[tuple[str, str]] = []
    ran = 0

    for shell in shells:
        _active_shell = shell
        print(f"\n--- {shell} ---")

        for i, case in enumerate(CASES, 1):
            if shell not in case.shells:
                continue
            label = f"[{shell}] case {i}: {case.label}"
            if selected != "all" and selected not in label:
                continue
            ran += 1
            try:
                case.run(shell)
                print(f"  PASS: case {i}: {case.label}")
            except Exception as exc:  # noqa: BLE001 - test runner
                failures.append((label, str(exc)))
                print(f"  FAIL: case {i}: {case.label}")
                print(exc, file=sys.stderr)

        label = f"[{shell}] user requested assertion format"
        if selected == "all" or selected in label:
            ran += 1
            try:
                test_user_requested_assertion_format()
                print("  PASS: user requested assertion format")
            except Exception as exc:  # noqa: BLE001 - test runner
                failures.append((label, str(exc)))
                print("  FAIL: user requested assertion format")
                print(exc, file=sys.stderr)

    if failures:
        print(f"\n{len(failures)} failure(s).", file=sys.stderr)
        return 1

    print(f"\nAll {ran} test(s) passed.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--shell",
        choices=["bash", "zsh", "fish", "all"],
        default="all",
        help="which shell to test (default: all)",
    )
    parser.add_argument(
        "--only",
        default="all",
        help="substring filter for test names (default: all)",
    )
    args = parser.parse_args()

    shells = ["bash", "zsh", "fish"] if args.shell == "all" else [args.shell]
    for shell in shells:
        if not COMP_FILES[shell].exists():
            print(f"FAIL: completion file not found: {COMP_FILES[shell]}", file=sys.stderr)
            return 1

    return run_suite(args.only, shells)


if __name__ == "__main__":
    raise SystemExit(main())
