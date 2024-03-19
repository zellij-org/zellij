function zr
  command zellij run --name "$argv" -- fish -c "$argv"
end
function zrf
  command zellij run --name "$argv" --floating -- fish -c "$argv"
end
function ze
  command zellij edit $argv
end
function zef
  command zellij edit --floating $argv
end
