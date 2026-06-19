# Dynamic session name completion for attach, kill-session, delete-session, watch
functions[_zellij_clap]=$functions[_zellij]
_zellij() {
    # For session subcommands, complete session names directly (skip clap
    # for the session name position so that a unique match auto-inserts)
    local cur="${words[$CURRENT]}"
    if [[ "$cur" != -* ]]; then
        local subcmd
        for subcmd in "${words[@]}"; do
            case "$subcmd" in
                attach|a|kill-session|k|delete-session|d|watch|w)
                    local -a sessions=(${(f)"$(zellij list-sessions --short --no-formatting 2>/dev/null)"})
                    [[ ${#sessions} -gt 0 ]] && compadd -- "${sessions[@]}"
                    return
                    ;;
            esac
        done
    fi
    _zellij_clap "$@"
}

function zr () { zellij run --name "$*" -- zsh -ic "$*";}
function zrf () { zellij run --name "$*" --floating -- zsh -ic "$*";}
function zri () { zellij run --name "$*" --in-place -- zsh -ic "$*";}
function ze () { zellij edit "$*";}
function zef () { zellij edit --floating "$*";}
function zei () { zellij edit --in-place "$*";}
function zpipe () {
  if [ -z "$1" ]; then
    zellij pipe;
  else
    zellij pipe -p $1;
  fi
}