# Dynamic session name completion for attach, kill-session, delete-session, watch
eval "$(declare -f _zellij | sed '1s/_zellij/_zellij_clap/')"
_zellij() {
    _zellij_clap
    local cur="${COMP_WORDS[$COMP_CWORD]}"
    [[ "$cur" == -* ]] && return
    local i
    for ((i=1; i < COMP_CWORD; i++)); do
        case "${COMP_WORDS[$i]}" in
            attach|a|kill-session|k|delete-session|d|watch|w)
                COMPREPLY+=($(compgen -W "$(zellij list-sessions --short --no-formatting 2>/dev/null)" -- "$cur"))
                return
                ;;
        esac
    done
}

function zr () { zellij run --name "$*" -- bash -ic "$*";}
function zrf () { zellij run --name "$*" --floating -- bash -ic "$*";}
function zri () { zellij run --name "$*" --in-place -- bash -ic "$*";}
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