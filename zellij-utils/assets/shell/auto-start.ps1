# The following snippet is meant to be used like this in your PowerShell profile ($PROFILE):
#
# # Configure auto-attach/exit to your likings (default is off).
# # $env:ZELLIJ_AUTO_ATTACH = 'true'
# # $env:ZELLIJ_AUTO_EXIT = 'true'
# Invoke-Expression (& zellij setup --generate-auto-start powershell | Out-String)

# The profile is also loaded by non-interactive sessions (pwsh -Command, -File, ...)
$zellijNonInteractive = [Environment]::GetCommandLineArgs() |
    Where-Object { $_ -match '^-(NonI|c$|Command|f$|File|e$|ec$|EncodedCommand)' }
if (-not $env:ZELLIJ -and -not $zellijNonInteractive -and $Host.Name -eq 'ConsoleHost') {
    # Zellij uses $SHELL as the default shell and falls back to cmd.exe when it is unset
    if (-not $env:SHELL) {
        $env:SHELL = (Get-Process -Id $PID).Path
    }

    if ($env:ZELLIJ_AUTO_ATTACH -eq 'true') {
        zellij attach -c
    } else {
        zellij
    }

    if ($env:ZELLIJ_AUTO_EXIT -eq 'true') {
        # A plain `exit` would only leave the profile script, not the shell
        $Host.SetShouldExit(0)
        exit
    }
}
Remove-Variable zellijNonInteractive -ErrorAction SilentlyContinue
