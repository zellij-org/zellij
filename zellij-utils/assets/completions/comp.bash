__zellij_session_subcommands="attach a kill-session k watch w delete-session d"
__zellij_completion_shells="bash elvish fish zsh powershell"

# This file is appended after clap output. Keep clap's original _zellij
# completer so we can delegate all non-dynamic contexts to it.
if declare -F _zellij >/dev/null 2>&1; then
  __zellij_clap_def="$(declare -f _zellij)"
  __zellij_clap_def="${__zellij_clap_def/#_zellij/__zellij_clap_complete}"
  eval "${__zellij_clap_def}"
  unset __zellij_clap_def
fi

function _zellij () {
  local cur prev i word
  local sessions
  local has_session_subcommand=0
  local has_setup=0
  cur="${COMP_WORDS[COMP_CWORD]}"
  COMPREPLY=()

  # Inspect already typed words (excluding the current one) to detect context.
  for (( i = 1; i < COMP_CWORD; i++ )); do
    word="${COMP_WORDS[i]}"
    case "${word}" in
      setup)
        has_setup=1
      ;;
    esac
    case " ${__zellij_session_subcommands} " in
      *" ${word} "*)
        has_session_subcommand=1
      ;;
    esac
  done

  # Keep the previous token handy for option-value completion.
  prev=""
  if (( COMP_CWORD > 0 )); then
    prev="${COMP_WORDS[COMP_CWORD-1]}"
  fi

  # Keep dynamic value completion for setup --generate-completion.
  if (( has_setup )) && [ "${prev}" = "--generate-completion" ]; then
    COMPREPLY=($(compgen -W "${__zellij_completion_shells}" -- "${cur}"))
    return 0
  fi

  # Session names are dynamic. Prefer them for non-option argument positions.
  if (( has_session_subcommand )) && [[ "${cur}" != -* ]]; then
    sessions="$(zellij list-sessions --short --no-formatting 2>/dev/null)"
    COMPREPLY=($(compgen -W "${sessions}" -- "${cur}"))
    return 0
  fi

  # Fallback to clap-generated completion for subcommands, flags and options.
  if declare -F __zellij_clap_complete >/dev/null 2>&1; then
    __zellij_clap_complete "$@"
    return $?
  fi

  return 0
}

complete -F _zellij zellij

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
