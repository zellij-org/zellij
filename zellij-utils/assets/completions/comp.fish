function __fish_complete_sessions
    zellij list-sessions 2>/dev/null
end
complete -c zellij -n "__fish_seen_subcommand_from attach" -f -a "(__fish_complete_sessions)" -d "Session"
complete -c zellij -n "__fish_seen_subcommand_from a" -f -a "(__fish_complete_sessions)" -d "Session"
complete -c zellij -n "__fish_seen_subcommand_from kill-session" -f -a "(__fish_complete_sessions)" -d "Session"
complete -c zellij -n "__fish_seen_subcommand_from k" -f -a "(__fish_complete_sessions)" -d "Session"
complete -c zellij -n "__fish_seen_subcommand_from setup" -l "generate-completion" -x -a "bash elvish fish zsh powershell" -d "Shell"
function zp
  # zellij pane
  command zellij run --name "$argv" -- fish -c "$argv"
end
function zpf
  # zellij pane floating
  command zellij run --name "$argv" --floating -- fish -c "$argv"
end
function zo
  # zellij open
  command zellij edit $argv
end
function zof
  # zellij open floating
  command zellij edit --floating $argv
end
