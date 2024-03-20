function zr () { zellij run --name "$*" -- bash -ic "$*";}
function zrf () { zellij run --name "$*" --floating -- bash -ic "$*";}
function ze () { zellij edit "$*";}
function zef () { zellij edit --floating "$*";}
function zpipe () { 
  if [ -z "$1" ]; then
    zellij pipe;
  else 
    zellij pipe -p $1;
  fi
}
