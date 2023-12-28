---
name: "\U0001F41B Bug Report"
about: If something isn't working as expected.
title: ''
labels: bug
assignees: ''

---

<!-- Please choose the relevant section, follow the instructions and delete the other sections:

1. Graphical issue inside a terminal pane (eg. something does not look as it should or as it looks outside of Zellij)
2. Issues with the Zellij UI / behavior / crash

** Please note: comparisons of desired behavior to tmux are usually not relevant. tmux and Zellij are two extremely different programs that do things very differently. Zellij is not, nor does it try to be, a tmux clone. Please try to refrain from such comparisons. **
-->

# 1. Graphical issue inside a terminal pane (eg. something does not look as it should)

1. Delete the contents of `/tmp/zellij-1000/zellij-log`, ie with `cd /tmp/zellij-1000/` and `rm -fr zellij-log/` (`/tmp/` is `$TMPDIR/` on OSX)
2. Run `zellij --debug`
3. Run `stty size`, copy the result and attach it in the bug report
4. Recreate your issue.
5. Quit Zellij immediately with ctrl-q (your bug should ideally still be visible on screen)

Please attach the files that were created in `/tmp/zellij-1000/zellij-log/` to the extent you are comfortable with.

**Basic information**

`zellij --version`:

`stty size`:

`uname -av` or `ver`(Windows):

## Further information
Reproduction steps, noticeable behavior, related issues, etc

# 2. Issues with the Zellij UI / behavior / crash
<!-- Please find a minimal reproduction. 

If you have a complex setup that causes an issue, try to troubleshoot and narrow the problem down to as minimal a reproduction as possible. Remove as many parts of the equation as you can. 

If you are unsure what to do, you are welcome to ask for help either in the issue itself or in one of our community chats (Discord / Matrix). We will be happy to try and assist or suggest directions, but please note that the responsibility to troubleshoot the issue and find the problem is ultimately on your shoulders. 

You're the expert on your system and we believe you are in the best position to troubleshoot it. Thank you for understanding.

Example of a good issue report: "The `default_tab_template` layout node does not work when resurrecting sessions".

Example of an issue report that needs work before being submitted: "Zellij randomly crashes without an error when I use it with the attached script".
-->

## Issue description

## Minimal reproduction

## Other relevant information
