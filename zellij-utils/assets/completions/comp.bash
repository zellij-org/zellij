function zp () { zellij run --name "$*" -- bash -c "$*";} # zellij pane
function zpf () { zellij run --name "$*" --floating -- bash -c "$*";} # zellij pane floating
function zo () { zellij edit "$*";} # zellij open
function zof () { zellij edit --floating "$*";} # zellij open floating
