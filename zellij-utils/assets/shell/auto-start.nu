if 'ZELLIJ' not-in $env {
    if $env.ZELLIJ_AUTO_ATTACH? == 'true' {
        zellij attach --create
    } else {
        zellij
    }
    if $env.ZELLIJ_AUTO_EXIT? == 'true' {
        exit
    }
}
