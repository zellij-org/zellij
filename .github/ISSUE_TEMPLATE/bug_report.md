---
name: "\U0001F41B Bug Report"
about: "If something isn't working as expected."
labels: bug
---
Thank you for taking the time to file an issue!
You can erase any parts of this template not applicable to your issue.

## In Case of Graphical, or Performance Issues

Please:
1. Delete the contents of `/tmp/zellij-1000/zellij-log`.
2. Run `zellij --debug` and then recreate your issue.
3. Quit Zellij immediately with ctrl-q (your bug should ideally still be visible on screen)

Please attach the files that were created in

`/tmp/zellij-1000/zellij-log/`

To the extent you are comfortable with.

Also please add the size in columns/lines of the terminal in which the bug happened. You can usually find these out with `tput lines` and `tput cols`.

And the name and version of progams you interacted with as well as
the operating system.

## Information

`zellij --version`:
