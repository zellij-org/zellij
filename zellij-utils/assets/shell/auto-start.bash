# Only in interactive shells with a terminal, not running a -c command.
if [[ -z "$ZELLIJ" && $- == *i* && -z "$BASH_EXECUTION_STRING" && -t 0 ]]; then
    if [[ "$ZELLIJ_AUTO_ATTACH" == "true" ]]; then
        if [[ -z "$ZELLIJ_AUTO_ATTACH_SESSION_NAME" ]]; then
            zellij attach -c
        else
            zellij attach -c "$ZELLIJ_AUTO_ATTACH_SESSION_NAME"
        fi
    else
        zellij
    fi

    if [[ "$ZELLIJ_AUTO_EXIT" == "true" ]]; then
        exit
    fi
fi
