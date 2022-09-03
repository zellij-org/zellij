---
name: "\U0001F41B Bug Report"
about: "If something isn't working as expected."
labels: bug
---
Thank you for taking the time to file this issue! Please follow the instructions and fill in the missing parts below the instructions, if it is meaningful. Try to be brief and concise.

**In Case of Graphical or Performance Issues**

1. Delete the contents of `/tmp/zellij-1000/zellij-log`, ie with `cd /tmp/zellij-1000/` and `rm -fr zellij-log/`
2. Run `zellij --debug`
3. Run `stty size`, copy the result and attach it in the bug report
4. Recreate your issue.
5. Quit Zellij immediately with ctrl-q (your bug should ideally still be visible on screen)

Please attach the files that were created in `/tmp/zellij-1000/zellij-log/` to the extent you are comfortable with.

**Basic information**

`zellij --version`:
`stty size`:
`uname -av` or `ver`(Windows):

List of programs you interact with as, `PROGRAM --version`: output cropped meaningful, for example:
`nvim --version`: NVIM v0.5.0-dev+1299-g1c2e504d5  (used the appimage release)
`alacritty --version`: alacritty 0.7.2 (5ac8060b)

**Further information**
Reproduction steps, noticeable behavior, related issues, etc
