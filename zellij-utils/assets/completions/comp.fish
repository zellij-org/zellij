function __fish_complete_sessions
    zellij list-sessions --short --no-formatting 2>/dev/null
end
complete -c zellij -n "__fish_seen_subcommand_from attach" -f -a "(__fish_complete_sessions)" -d "Session"
complete -c zellij -n "__fish_seen_subcommand_from a" -f -a "(__fish_complete_sessions)" -d "Session"
complete -c zellij -n "__fish_seen_subcommand_from kill-session" -f -a "(__fish_complete_sessions)" -d "Session"
complete -c zellij -n "__fish_seen_subcommand_from k" -f -a "(__fish_complete_sessions)" -d "Session"
complete -c zellij -n "__fish_seen_subcommand_from setup" -l "generate-completion" -x -a "bash elvish fish zsh powershell" -d "Shell"

# the zpipe alias completions
function __fish_complete_aliases
  zellij list-aliases 2>/dev/null
end

complete -c zpipe -f -a "(__fish_complete_aliases)" -d "Zpipes"

