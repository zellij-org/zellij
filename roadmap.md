# Roadmap
This file contains an ever-changing list of the major features that are either currently being worked on, or planned for the near future.

  * <b>A layout engine</b> that would allow you to define how your panes will be (re)arranged when you open or close them. As well as when you change the terminal window size.
  * <b>Pane types beyond a simple shell prompt</b>, for example:
    - A file explorer (similar to ranger) that opens files for editing in a new pane.
    - A launcher that opens any command you enter in a new pane
    - A command pane that would run any command, display its output and re-run that command when clicked. Changing its frame colour to green/yellow/red depending on the exit status.
  * <b>A Webassembly plugin system for compiled languages</b> built using WASI to allow you to write plugins in any compiled language. These plugins would be able to create new panes, interact with existing ones, interact with the filesystem and subscribe to events. You could consume them at runtime and decide what permissions to give them.
  * <b>Built in support for portable workspaces across machines, projects and teams</b>: imagine being able to include a configuration file with your project that would include all the layouts and plugins that would best help new developers getting onboarded. Including all the shortcuts, customized panes and help-message hints triggered by things such as opening a file, entering a folder or running a command. How about being able to log into a new server or container, start mosaic with a URL of a git repository including your favorite configuration and plugins, and working with it as if you were on your own machine?
  * <b>Support for multiple terminal windows across screens</b>: Why limit yourself to one terminal window? Mosaic would allow you to transfer panes, view powerlines, get alerts and control your workspace from different windows by having them all belong to the same session.
