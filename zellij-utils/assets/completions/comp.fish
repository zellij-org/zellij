function __fish_complete_sessions
    zellij list-sessions 2>/dev/null
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
function ze
  command zellij edit $argv
end
function zef
  command zellij edit --floating $argv
end
