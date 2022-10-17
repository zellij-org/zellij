function zp () { zellij run --name "$*" -- zsh -c "$*";} # zellij pane
function zpf () { zellij run --name "$*" --floating -- zsh -c "$*";} # zellij pane floating
function zo () { zellij edit "$*";} # zellij open
function zof () { zellij edit --floating "$*";} # zellij open floating
