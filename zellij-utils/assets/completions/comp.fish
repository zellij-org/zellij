function __fish_complete_sessions
    zellij list-sessions --short --no-formatting 2>/dev/null
end
complete -c zellij -n "__fish_seen_subcommand_from attach" -f -a "(__fish_complete_sessions)" -d "Session"
complete -c zellij -n "__fish_seen_subcommand_from a" -f -a "(__fish_complete_sessions)" -d "Session"
complete -c zellij -n "__fish_seen_subcommand_from kill-session" -f -a "(__fish_complete_sessions)" -d "Session"
complete -c zellij -n "__fish_seen_subcommand_from k" -f -a "(__fish_complete_sessions)" -d "Session"
complete -c zellij -n "__fish_seen_subcommand_from setup" -l "generate-completion" -x -a "bash elvish fish zsh powershell" -d "Shell"
function zr
  command zellij run --name "$argv" -- fish -c "$argv"
end
function zrf
  command zellij run --name "$argv" --floating -- fish -c "$argv"
end
function zri
  command zellij run --name "$argv" --in-place -- fish -c "$argv"
end
function ze
  command zellij edit $argv
end
function zef
  command zellij edit --floating $argv
end
function zei
  command zellij edit --in-place $argv
end

# the zpipe alias and its completions
function __fish_complete_aliases
  zellij list-aliases 2>/dev/null
end
function zpipe
  if count $argv > /dev/null
    command zellij pipe -p $argv
  else
    command zellij pipe
  end
end
complete -c zpipe -f -a "(__fish_complete_aliases)" -d "Zpipes"