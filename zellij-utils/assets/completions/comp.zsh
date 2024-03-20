function zr () { zellij run --name "$*" -- zsh -ic "$*";}
function zrf () { zellij run --name "$*" --floating -- zsh -ic "$*";}
function ze () { zellij edit "$*";}
function zef () { zellij edit --floating "$*";}
function zpipe () { 
  if [ -z "$1" ]; then
    /home/aram/code/zellij/target/dev-opt/zellij pipe;
  else 
    /home/aram/code/zellij/target/dev-opt/zellij pipe -p $1;
  fi
}
