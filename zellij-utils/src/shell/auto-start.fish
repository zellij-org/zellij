if not set -q ZELLIJ
    zellij

    # auto quit the shell
    kill $fish_pid
end
