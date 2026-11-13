# Only in interactive shells with a terminal, not running a -c command.
if [[ -z "$ZELLIJ" && -o interactive && -z "$ZSH_EXECUTION_STRING" && -t 0 ]]; then
    if [[ "$ZELLIJ_AUTO_ATTACH" == "true" ]]; then
        zellij attach -c
    else
        zellij
    fi

    if [[ "$ZELLIJ_AUTO_EXIT" == "true" ]]; then
        exit
    fi
fi
