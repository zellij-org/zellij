#!/usr/bin/env bash
exec </dev/tty >/dev/tty
saved="$(stty -g)"
stty -echo -icanon
cat "$1"
printf '\033[6n'
IFS= read -r -s -d R _
stty "$saved"
