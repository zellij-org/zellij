<h1 align="center">
  <br>
  <img src="https://raw.githubusercontent.com/zellij-org/zellij/main/assets/logo.png" alt="logo" width="200">
  <br>
  Zellij
  <br>
  <br>
</h1>

<p align="center">
  <img src="https://raw.githubusercontent.com/zellij-org/zellij/main/assets/demo.gif" alt="demo">
</p>

<p align="center">
  <a href="https://discord.gg/CrUAFH3"><img alt="Discord Chat" src="https://img.shields.io/discord/771367133715628073"></a>
</p>


# What is this?

[Zellij](https://en.wikipedia.org/wiki/Zellij) is a workspace aimed at developers, ops-oriented people and anyone who loves the terminal.
At its core, it is a terminal multiplexer (similar to [tmux](https://github.com/tmux/tmux) and [screen](https://www.gnu.org/software/screen/)), but this is merely its infrastructure layer.

For more details, read about upcoming features in our [roadmap](#roadmap).

Right now Zellij is in its early development stages and is not yet ready for everyday usage.
If you're interested, watch this space or better yet - get involved!

Zellij was initially called "Mosaic".

## How to use it?
* Clone the project
* In the project folder, run: `cargo run`

(note that right now Zellij only supports linux and mac)

The status bar on the bottom should guide you through the possible keyboard shortcuts in the app.

# What is the current status of the project?

Zellij is in the last stages of being VT compatible. As much as modern terminals are.
Most things should work inside a terminal pane, but some edge cases don't. Fixing these edge cases is a priority, so please open an issue if you find a bug.

Zellij is in its alpha stage right now. Please treat it accordingly.

# How do I get involved?

At the moment, the project is in early development.
A lot of the work needed to be done is product work (making decisions about what Zellij will be and do) as well as development work. Most tasks would probably involve a little of both.
We're a small team of enthusiasts, and we eagerly welcome people who would like to join in at this early stage.
We welcome all contributors, regardless of experience level. We believe any person who would like to contribute can make the project better!

To get started, you can:
1. Take a look at the "Issues" in this repository - especially those marked "Good first issue". Those with the "Help Wanted" tag probably don't have anyone else working on them.
2. Drop by our [chat](https://discord.gg/CrUAFH3) and ask what you can work on, or how to get started.
3. Open an issue with your idea(s) for the project or tell us about them in our chat.

And most importantly, please read our [code of conduct](CODE_OF_CONDUCT.md).

# Roadmap
This section contains an ever-changing list of the major features that are either currently being worked on, or planned for the near future.

  * <b>A layout engine</b> that would allow you to define how your panes will be (re)arranged when you open or close them. As well as when you change the terminal window size.
  * <b>Pane types beyond a simple shell prompt</b>, for example:
    - A file explorer (similar to ranger) that opens files for editing in a new pane.
    - A launcher that opens any command you enter in a new pane
    - A command pane that would run any command, display its output and re-run that command when clicked. Changing its frame colour to green/yellow/red depending on the exit status.
  * <b>A Webassembly plugin system for compiled languages</b> built using WASI to allow you to write plugins in any compiled language. These plugins would be able to create new panes, interact with existing ones, interact with the filesystem and subscribe to events. You could consume them at runtime and decide what permissions to give them.
  * <b>Built in support for portable workspaces across machines, projects and teams</b>: imagine being able to include a configuration file with your project that would include all the layouts and plugins that would best help new developers getting onboarded. Including all the shortcuts, customized panes and help-message hints triggered by things such as opening a file, entering a folder or running a command. How about being able to log into a new server or container, start Zellij with a URL of a git repository including your favorite configuration and plugins, and working with it as if you were on your own machine?
  * <b>Support for multiple terminal windows across screens</b>: Why limit yourself to one terminal window? Zellij would allow you to transfer panes, view powerlines, get alerts and control your workspace from different windows by having them all belong to the same session.

# Contributing

Take a look at [`Contributing.md`](CONTRIBUTING.md) guide.

# License

MIT
