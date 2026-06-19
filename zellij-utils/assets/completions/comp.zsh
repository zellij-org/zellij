typeset -ga __zellij_session_subcommands
__zellij_session_subcommands=(attach a kill-session k watch w delete-session d)

typeset -ga __zellij_completion_shells
__zellij_completion_shells=(bash elvish fish zsh powershell)

# This file is appended after clap output. Preserve clap's original _zellij
# completer so we can delegate non-dynamic contexts to it.
if (( $+functions[_zellij] )); then
  functions[__zellij_clap_complete]="${functions[_zellij]}"
fi

function _zellij () {
  local -a sessions
  local cur word
  local i has_session_subcommand=0 has_setup=0
  cur="${words[CURRENT]}"

  # Inspect already typed words (excluding the current one) to detect context.
  for (( i = 2; i < CURRENT; i++ )); do
    word="${words[i]}"
    case "${word}" in
      setup)
        has_setup=1
      ;;
    esac
    if (( ${__zellij_session_subcommands[(I)$word]} )); then
      has_session_subcommand=1
    fi
  done

  # Keep dynamic value completion for setup --generate-completion.
  if (( has_setup )) && (( CURRENT > 1 )) && [[ "${words[CURRENT-1]}" == "--generate-completion" ]]; then
    compadd -- "${__zellij_completion_shells[@]}"
    return 0
  fi

  # Session names are dynamic. Prefer them for non-option argument positions.
  if (( has_session_subcommand )) && [[ "${cur}" != -* ]]; then
    sessions=("${(@f)$(zellij list-sessions --short --no-formatting 2>/dev/null)}")
    (( ${#sessions[@]} )) && compadd -- "${sessions[@]}"
    return 0
  fi

  # Fallback to clap-generated completion for subcommands, flags and options.
  if (( $+functions[__zellij_clap_complete] )); then
    __zellij_clap_complete "$@"
    return $?
  fi

  return 0
}

if (( $+functions[compdef] )); then
  compdef _zellij zellij
fi

# Convenience wrappers for running/editing commands in zellij panes.
function zr () { zellij run --name "$*" -- zsh -ic "$*";}
function zrf () { zellij run --name "$*" --floating -- zsh -ic "$*";}
function zri () { zellij run --name "$*" --in-place -- zsh -ic "$*";}
function ze () { zellij edit "$*";}
function zef () { zellij edit --floating "$*";}
function zei () { zellij edit --in-place "$*";}

# Pipe helper: with argument -> `-p <alias>`, without argument -> plain `pipe`.
function zpipe () {
  if [ -z "$1" ]; then
    zellij pipe;
  else
    zellij pipe -p $1;
  fi
}
