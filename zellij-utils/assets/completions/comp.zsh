(( $+functions[__zellij_sessions] )) ||
__zellij_sessions() {
    local -a sessions
    sessions=(${(f)"$(zellij list-sessions --short --no-formatting 2>/dev/null)"})
    _describe -t sessions 'zellij sessions' sessions
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
