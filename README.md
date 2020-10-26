# mosaic
# What is this?

Mosaic is a workspace aimed at developers and ops-oriented people who love the terminal.
At its core, it is a terminal multiplexer (similar to tmux[link] and screen[link]), but this is merely its infrastructure layer.

Mosaic is a work in progress and is currently in active development. See below if you'd like to get involved!

Mosaic would let you not only split your workspace into different panes, tabs and sessions, but also reimagine how you work with the terminal. It will include:
  * <b>A layout engine</b> that would allow you to define how your panes will be (re)arranged when you open or close a pane. As well as when you change the terminal window size.
  * <b>Pane types beyond a simple shell prompt</b> such as a file explorer that can open files for editing in new panes, a launcher that would open commands in a new pane, a command pane that would run just one command when clicked, changing its frame colour with each exit status it receives appropriately, and many more.
  * <b>A Webassembly plugin system for compiled languages</b> built using WASI to allow you to write plugins in any compiled language. These plugins would be able to create new panes, interact with existing ones and subscribe to events. And the best thing: you could consume them at runtime and decide what permissions to give them.
  * <b>Built in support for portable workspaces across machines, projects and teams</b>: imagine being able to include a configuration file with your project that would include all the layouts and plugins that would best help new developers getting onboarded. Including all the shortcuts, customized panes and help-message hints triggered by things such as opening a file, entering a folder or running a command. How about being able to log into a new server or container, start mosaic with a URL of a git repository including your favorite configuration and plugins, and working with it as if you were on your own machine?
  * <b>Support for multiple terminal windows across screens</b>: Why limit yourself to one terminal window? Mosaic would allow you to transfer panes, view powerlines, get alerts and control your workspace from different windows by having them all belong to the same session.

# What is the current status of the project?

Mosaic is not yet ready for everyday usage, but is getting there fast.
Right now it:
  * Successfully renders shells (all experiments have been done with fish-cli).
  * Can split the terminal into multiple horizontal/vertical panes
  * Can resize panes, as well as close them
  * Can scroll up and down inside a pane
  * Can render a vim pane
  * Can render most so called "raw mode" applications that draw a textual user interface and refresh themselves.

Work is being done on compatibility to make it at least as VTE compatible as most terminal emulators today so that working with it would be seamless.

## How to use it?
* Clone the project
* In the project folder, run: `cargo run`

(note that right now Mosaic only supports linux and maybe mac)

Some temporary controls (these will be changed to something more convenient when the project matures a little):
  * ctrl-n - split focused pane vertically
  * ctrl-b - split focused pane horizontally
  * ctrl-j - resize focused pane down
  * ctrl-k - resize focused pane up 
  * ctrl-h - resize focused pane left
  * ctrl-l - resize focused pane right
  * ctrl-p - move focus to next pane
  * ctrl-[ - scroll up in focused pane
  * ctrl-] - scroll down in focused pane
  * ctrl-x - close focused pane
  * ctrl-q - quit Mosaic

# How do I get involved?

At the moment, the project is in early development and prototyping.
A lot of the work needed to be done is product work (making decisions about what Mosaic will be and do) as well as development work.
We're a small team of hobbyists, and we eagerly welcome people who would like to join in at this early stage.

Because of the above, it's not trivial for us to have issues tagged "Help Wanted" or "Good First Issue", because all work would likely need some prior discussion.
That said, we would absolutely love to have these discussions and to bring more people on. Even if you are unsure of your abilities or have never contributed to open source before.
Please drop us a line (TODO: team email? gitter? irc?)

# License

MIT
