# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)

## [Unreleased]
* fix(sessions): issue where sessions would occasionally become unresponsive (https://github.com/zellij-org/zellij/pull/3281)
* fix(cli): respect all options (eg. `default-layout`) when creating a session in the background from the CLI (https://github.com/zellij-org/zellij/pull/3288)
* fix(cli): rename tab and pane from cli (https://github.com/zellij-org/zellij/pull/3295)
* fix(plugins): respect $SHELL when opening a terminal from plugins (eg. from the filepicker strider) (https://github.com/zellij-org/zellij/pull/3296)

## [0.40.0] - 2024-04-16
* feat(plugins): skip plugin cache flag when loading plugins (https://github.com/zellij-org/zellij/pull/2971)
* fix(grid): recover from various errors (https://github.com/zellij-org/zellij/pull/2972)
* fix(grid): flaky scroll with scroll region (https://github.com/zellij-org/zellij/pull/2935)
* fix(plugins): display errors properly (https://github.com/zellij-org/zellij/pull/2975)
* feat(terminal): implement synchronized renders (https://github.com/zellij-org/zellij/pull/2977)
* perf(plugins): improve plugin download & load feature (https://github.com/zellij-org/zellij/pull/3001)
* chore: bump Rust toolchain to 1.75.0 (https://github.com/zellij-org/zellij/pull/3039)
* feat(plugins): introduce pipes to control data flow to plugins from the command line (https://github.com/zellij-org/zellij/pull/3066, https://github.com/zellij-org/zellij/pull/3170, https://github.com/zellij-org/zellij/pull/3210 and https://github.com/zellij-org/zellij/pull/3212)
* feat(xtask): allow publishing without pushing changes (https://github.com/zellij-org/zellij/pull/3040)
* fix(terminal): improve reflow performance as well as resource utilization and some misc ancient bugs (https://github.com/zellij-org/zellij/pull/3045, https://github.com/zellij-org/zellij/pull/3032, https://github.com/zellij-org/zellij/pull/3043 and https://github.com/zellij-org/zellij/pull/3125)
* feat(sessions): add welcome screen (https://github.com/zellij-org/zellij/pull/3112 and https://github.com/zellij-org/zellij/pull/3226)
* fix(cli): respect cwd in `zellij run` and `zellij plugin` commands (https://github.com/zellij-org/zellij/pull/3116)
* feat(panes): allow specifying floating pane coordinates when opening from cli/plugin/keybinding (https://github.com/zellij-org/zellij/pull/3122)
* fix(plugins): avoid crash when attaching to a session with a since-deleted cwd (https://github.com/zellij-org/zellij/pull/3126)
* fix(panes): break pane to new tab regression (https://github.com/zellij-org/zellij/pull/3130)
* feat: add moving tab to other position (https://github.com/zellij-org/zellij/pull/3047)
* feat(plugins): introduce plugin aliases (https://github.com/zellij-org/zellij/pull/3157)
* fix(plugins): respect cwd (https://github.com/zellij-org/zellij/pull/3161)
* fix(panes): handle race conditions when unsetting fullscreen (https://github.com/zellij-org/zellij/pull/3166)
* feat(plugins): allow specifying cwd when creating a new session with a layout (https://github.com/zellij-org/zellij/pull/3172)
* feat(plugins): session-manager cwd and new filepicker/strider (https://github.com/zellij-org/zellij/pull/3200)
* fix(stability): various client races (https://github.com/zellij-org/zellij/pull/3209)
* feat(cli): `list-sessions` show newest sessions last, for better user experience (https://github.com/zellij-org/zellij/pull/3194)
* fix(startup): recover from Zellij sometimes not filling the whole terminal window on startup (https://github.com/zellij-org/zellij/pull/3218)
* fix(config): support Ctrl/Alt modifier keys on F keys (eg. `Ctrl F1`, `Alt F2`) (https://github.com/zellij-org/zellij/pull/3179)
* fix(keybindings): allow binding `Ctrl Space` (https://github.com/zellij-org/zellij/pull/3101)
* feat(plugins): add API to dump the current session's layout to a plugin (https://github.com/zellij-org/zellij/pull/3227)
* fix(plugins): properly serialize remote urls (https://github.com/zellij-org/zellij/pull/3224)
* feat(plugins): add close_self API to allow plugins to close their own instance (https://github.com/zellij-org/zellij/pull/3228)
* feat(plugins): allow plugins to specify `zellij:OWN_URL` as a pipe destination (https://github.com/zellij-org/zellij/pull/3232)
* feat(cli): Add `move-tab` action (https://github.com/zellij-org/zellij/pull/3244)
* feat(plugins): add serialization methods to UI components (https://github.com/zellij-org/zellij/pull/3193)
* fix(layouts): recover from resurrection crash and fix swap layouts not being picked up by new-tab keybinding (https://github.com/zellij-org/zellij/pull/3249)
* feat(cli): allow starting a session in the background (detached) (https://github.com/zellij-org/zellij/pull/3257 and https://github.com/zellij-org/zellij/pull/3265)
* feat(config): allow disabling writing of session metadata to disk (https://github.com/zellij-org/zellij/pull/3258)
* fix(compact-bar): properly pad mode indicator (https://github.com/zellij-org/zellij/pull/3260)
* fix(resurrection): do not list empty sessions and fix search ux issue in session-manager (https://github.com/zellij-org/zellij/pull/3264)

## [0.39.2] - 2023-11-29
* fix(cli): typo in cli help (https://github.com/zellij-org/zellij/pull/2906)
* fix(sessions): slow session updates in the session-manager (https://github.com/zellij-org/zellij/pull/2951)
* fix: compiler warnings (https://github.com/zellij-org/zellij/pull/2873)

## [0.39.1] - 2023-11-13
* fix: styled underlines in editors (https://github.com/zellij-org/zellij/pull/2918)
* fix(plugins): add `LaunchPlugin` and some cwd fixes (https://github.com/zellij-org/zellij/pull/2916)
* fix(performance): significantly reduce CPU utilization when serializing sessions (https://github.com/zellij-org/zellij/pull/2920)
* fix(panes): reuse CWD when dropping to shell in command panes (https://github.com/zellij-org/zellij/pull/2915)
* fix(resurrection): reduce default serialization interval to 1m and make it configurable (https://github.com/zellij-org/zellij/pull/2923)
* fix(plugins): allow reloading plugins if they crashed (https://github.com/zellij-org/zellij/pull/2929)

## [0.39.0] - 2023-11-07
* feat(panes): start panes/editors/commands/plugins in-place (https://github.com/zellij-org/zellij/pull/2795)
* fix(theme): fg color for gruvbox light theme (https://github.com/zellij-org/zellij/pull/2791)
* fix: display parsing error for kdl files located under the 'themes' directory (https://github.com/zellij-org/zellij/pull/2762)
* refactor(plugins): wasmer v3.1.1 (https://github.com/zellij-org/zellij/pull/2706)
* refactor(config): dependency updates (https://github.com/zellij-org/zellij/pull/2820 and https://github.com/zellij-org/zellij/pull/2821)
* fix(plugins): address cranelift-codegen vulnerability (https://github.com/zellij-org/zellij/pull/2830)
* fix(plugins): use versioned path for plugin artifact cache (https://github.com/zellij-org/zellij/pull/2836)
* feat(sessions): session resurrection (https://github.com/zellij-org/zellij/pull/2801, https://github.com/zellij-org/zellij/pull/2851 and https://github.com/zellij-org/zellij/pull/2902)
* feat(rendering): terminal synchronized output (https://github.com/zellij-org/zellij/pull/2798)
* feat(plugins): plugin command API for executing commands in the background (https://github.com/zellij-org/zellij/pull/2862)
* feat(ui): cyberpunk themes (https://github.com/zellij-org/zellij/pull/2868)
* feat(ux): add ESC option to drop to shell in command panes (https://github.com/zellij-org/zellij/pull/2872)
* feat(plugins): allow plugins to make web requests behind a permission (https://github.com/zellij-org/zellij/pull/2879)
* feat(plugins): UI components for plugins (https://github.com/zellij-org/zellij/pull/2898)
* feat(plugins): load plugins from the web (https://github.com/zellij-org/zellij/pull/2863)
* feat(terminal): support styled underlines (https://github.com/zellij-org/zellij/pull/2730)
* feat(ux): allow renaming sessions (https://github.com/zellij-org/zellij/pull/2903)
* fix(plugins): open new plugins in the current cwd (https://github.com/zellij-org/zellij/pull/2905)

## [0.38.2] - 2023-09-15
* fix(terminal): wrap lines in alternate screen mode when adding characters (https://github.com/zellij-org/zellij/pull/2789)
* fix(utils): validate session name (https://github.com/zellij-org/zellij/pull/2607)

## [0.38.1] - 2023-08-31
* refactor(server): remove unnecessary mut (https://github.com/zellij-org/zellij/pull/2735)
* fix(status-bar): add break tab hints (https://github.com/zellij-org/zellij/pull/2748)
* fix(reconnect): glitches on windows terminal (https://github.com/zellij-org/zellij/pull/2750)
* fix(grid): memory leak with unfocused tabs (https://github.com/zellij-org/zellij/pull/2745)
* fix(input): enforce ordering of actions after opening a new pane (https://github.com/zellij-org/zellij/pull/2757)

## [0.38.0] - 2023-08-28
* fix(tab-bar,compact-bar): tab switching with mouse sometimes not working (https://github.com/zellij-org/zellij/pull/2587)
* fix(rendering): occasional glitches while resizing (https://github.com/zellij-org/zellij/pull/2621)
* fix(rendering): colored paneframes in mirrored sessions (https://github.com/zellij-org/zellij/pull/2625)
* fix(sessions): use custom lists of adjectives and nouns for generating session names (https://github.com/zellij-org/zellij/pull/2122)
* feat(plugins): make plugins configurable (https://github.com/zellij-org/zellij/pull/2646 and https://github.com/zellij-org/zellij/pull/2727)
* fix(terminal): occasional glitches while changing focus (https://github.com/zellij-org/zellij/pull/2654)
* feat(plugins): add utility functions to get focused tab/pane (https://github.com/zellij-org/zellij/pull/2652)
* feat(ui): break pane to new tab and move panes between tabs (https://github.com/zellij-org/zellij/pull/2664)
* fix(performance): plug memory leak (https://github.com/zellij-org/zellij/pull/2675)
* feat(plugins): use protocol buffers to communicate across the wasm boundary (https://github.com/zellij-org/zellij/pull/2686 and https://github.com/zellij-org/zellij/pull/2729)
* feat(plugins): add permission system (https://github.com/zellij-org/zellij/pull/2624, https://github.com/zellij-org/zellij/pull/2722 and https://github.com/zellij-org/zellij/pull/2731)
* feat(session): session manager to switch between sessions (https://github.com/zellij-org/zellij/pull/2721)
* feat(plugins): move_to_focused_tab attribute for launching/focusing plugins (https://github.com/zellij-org/zellij/pull/2725)
* fix(keybinds): allow opening floating pane from a keybinding (https://github.com/zellij-org/zellij/pull/2726)
* fix(panes): occasional glitches when changing tab focus for stacked panes (https://github.com/zellij-org/zellij/pull/2734)

## [0.37.2] - 2023-06-20
* hotfix: include theme files into binary (https://github.com/zellij-org/zellij/pull/2566)
* fix: make plugin hide_self api idempotent (https://github.com/zellij-org/zellij/pull/2568)

## [0.37.1] - 2023-06-19
* hotfix: theme options does not work (https://github.com/zellij-org/zellij/pull/2562)
* fix: various plugin api methods (https://github.com/zellij-org/zellij/pull/2564)

## [0.37.0] - 2023-06-18
* fix(plugin): respect hide session option on compact-bar (https://github.com/zellij-org/zellij/pull/2368)
* feat: allow excluding tabs from tab sync in layouts (https://github.com/zellij-org/zellij/pull/2314)
* feat: support default cwd (https://github.com/zellij-org/zellij/pull/2290)
* feat: cli action to reload plugins at runtime for easier plugin development (https://github.com/zellij-org/zellij/pull/2372)
* docs(architecture): update architecture docs (https://github.com/zellij-org/zellij/pull/2371)
* feat(themes): add nightfox themes (https://github.com/zellij-org/zellij/pull/2384)
* feat: provide default themes (https://github.com/zellij-org/zellij/pull/2307)
* feat: update and render plugins asynchronously (https://github.com/zellij-org/zellij/pull/2410)
* fix: support environment variables and shell expansions in layout cwds (https://github.com/zellij-org/zellij/pull/2291)
* fix: add file paths to file not found errors (https://github.com/zellij-org/zellij/pull/2412)
* fix: error loading non-existant themes directory (https://github.com/zellij-org/zellij/pull/2411)
* build: speed up build and ci https://github.com/zellij-org/zellij/pull/2396
* fix: sticky bit FreeBSD crash https://github.com/zellij-org/zellij/pull/2424
* build: Bump rust toolchain version to 1.67 (https://github.com/zellij-org/zellij/pull/2375)
* fix: update config file output (https://github.com/zellij-org/zellij/pull/2443)
* feat: plugin workers for background tasks (https://github.com/zellij-org/zellij/pull/2449)
* fix: cwd of newtab action (https://github.com/zellij-org/zellij/pull/2455)
* feat: plugin system overhaul (https://github.com/zellij-org/zellij/pull/2510)
* feat: add virtually all of Zellij's API to plugins (https://github.com/zellij-org/zellij/pull/2516)
* fix: runtime panic because of local cache (https://github.com/zellij-org/zellij/pull/2522)
* fix: cursor flickering (https://github.com/zellij-org/zellij/pull/2528)
* fix: focus tab as well as pane when relaunching plugin (https://github.com/zellij-org/zellij/pull/2530)
* feat: ui improvements for strider search (https://github.com/zellij-org/zellij/pull/2531)
* fix: only watch fs if plugins explicitly request it (https://github.com/zellij-org/zellij/pull/2529)
* fix: suppress debug logging when not debugging (https://github.com/zellij-org/zellij/pull/2532)
* feat: send pane events to plugins (https://github.com/zellij-org/zellij/pull/2545)
* fix: use debounced watcher for watching filesystem (https://github.com/zellij-org/zellij/pull/2546)
* feat: add more plugin api methods (https://github.com/zellij-org/zellij/pull/2550)

## [0.36.0] - 2023-04-13
* fix: when moving pane focus off screen edge to the next tab, the pane on the screen edge is now focused (https://github.com/zellij-org/zellij/pull/2293)
* fix: adding panes to lone stack (https://github.com/zellij-org/zellij/pull/2298)
* fix: closing a stacked pane now properly moves to the previous swap layout if appropriate (https://github.com/zellij-org/zellij/pull/2312)
* deps: update interprocess: fix crash and reduce memory usage by not leaking socket file descriptors on client attach (https://github.com/zellij-org/zellij/pull/2322)
* feat: load plugins asynchronously (https://github.com/zellij-org/zellij/pull/2327)
* feat: cli and bindable action to clear the current terminal's buffer and scrollback (https://github.com/zellij-org/zellij/pull/2239)
* feat: add option to `hide_session_name` in tab-bar (https://github.com/zellij-org/zellij/pull/2301)
* fix: do not use default swap layouts when opening a new tab with a custom layout (https://github.com/zellij-org/zellij/pull/2336)
* fix: properly truncate panes with attributes when applying swap layouts (https://github.com/zellij-org/zellij/pull/2337)
* fix: support spaces in scrollback_editor (https://github.com/zellij-org/zellij/pull/2339)
* fix: tab focus race condition when applying layout (https://github.com/zellij-org/zellij/pull/2340)
* feat: allow specifying an "expanded" pane in a stack when defining layouts (https://github.com/zellij-org/zellij/pull/2343)
* fix: stacked pane focus glitches in layout (https://github.com/zellij-org/zellij/pull/2344)
* fix: strider now no longer opens one pane per client when editing files (https://github.com/zellij-org/zellij/pull/2346)
* fix: set sticky bit on socket files to avoid automatic cleanup (https://github.com/zellij-org/zellij/pull/2141)
* fix: memory leak when attaching/detaching from sessions (https://github.com/zellij-org/zellij/pull/2328)
* fix: allow loading plugins from relative urls (https://github.com/zellij-org/zellij/pull/2539)

## [0.35.2] - 2023-03-10
* fix: get "zellij attach --create" working again (https://github.com/zellij-org/zellij/pull/2247)
* fix: crash when closing tab with command panes (https://github.com/zellij-org/zellij/pull/2251)
* Terminal compatibility: pad end of line on `CSI P` (https://github.com/zellij-org/zellij/pull/2259)

## [0.35.1] - 2023-03-07
* fix: show visual error when unable to split panes vertically/horizontally (https://github.com/zellij-org/zellij/pull/2025)
* build: Use `xtask` as build system (https://github.com/zellij-org/zellij/pull/2012)
* fix: show visual error when failing to resize panes in various situations (https://github.com/zellij-org/zellij/pull/2036)
* dist: remove nix support (https://github.com/zellij-org/zellij/pull/2038)
* feat: support floating panes in layouts (https://github.com/zellij-org/zellij/pull/2047)
* feat: add tmux close pane key (https://github.com/zellij-org/zellij/pull/2058)
* fix: copy_on_select = false sticky selection (https://github.com/zellij-org/zellij/pull/2086)
* fix: do not drop wide chars when resizing to width of 1 column (https://github.com/zellij-org/zellij/pull/2082)
* fix: disallow path-like names for sessions (https://github.com/zellij-org/zellij/pull/2082)
* errors: Remove more `unwrwap`s from server code (https://github.com/zellij-org/zellij/pull/2069)
* fix: support UTF-8 character in tab name and pane name (https://github.com/zellij-org/zellij/pull/2102)
* fix: handle missing/inaccessible cache directory (https://github.com/zellij-org/zellij/pull/2093)
* errors: Improve client disconnect handling (https://github.com/zellij-org/zellij/pull/2068)
* feat: add ScrollToTop action (https://github.com/zellij-org/zellij/pull/2110)
* fix: the status-bar now does the right thing when set to one line (https://github.com/zellij-org/zellij/pull/2091)
* feat: add cli action to switch to tab by name (https://github.com/zellij-org/zellij/pull/2120)
* dev: use the wasmer Singlepass compiler when compiling plugins in development (https://github.com/zellij-org/zellij/pull/2134 + https://github.com/zellij-org/zellij/pull/2146)
* feat: add pencil light theme (https://github.com/zellij-org/zellij/pull/2157)
* fix: apply correct color on 'more tabs' message (https://github.com/zellij-org/zellij/pull/2166)
* deps: upgrade termwiz to 0.20.0 (https://github.com/zellij-org/zellij/pull/2169)
* feat: swap layouts and stacked panes (https://github.com/zellij-org/zellij/pull/2167, https://github.com/zellij-org/zellij/pull/2191 and 
)
* fix: cache STDIN queries to prevent startup delay (https://github.com/zellij-org/zellij/pull/2173)
* fix: scrollback positioning with Helix (https://github.com/zellij-org/zellij/pull/2156)
* fix: allow CJK characters in tab names (https://github.com/zellij-org/zellij/pull/2119)
* fix: fullscreen navigation (https://github.com/zellij-org/zellij/pull/2117)
* fix: glitchy resizes (https://github.com/zellij-org/zellij/pull/2182)
* fix: race when opening command panesin layout (https://github.com/zellij-org/zellij/pull/2196)
* fix: `focus` attribute in tab layouts now works (https://github.com/zellij-org/zellij/pull/2197)
* fix: new-tab cli action now properly looks in the layout folder as well (https://github.com/zellij-org/zellij/pull/2198)
* fix: new-tab keybind now properly looks in the layout folder as well (https://github.com/zellij-org/zellij/pull/2200)
* fix: cwd for edit panes (https://github.com/zellij-org/zellij/pull/2201)
* fix: get config parameters from config file when opening new-tab through the cli (https://github.com/zellij-org/zellij/pull/2203)
* Terminal compatibility: fix wrong styling interpretation when deleting characters (https://github.com/zellij-org/zellij/pull/2204)
* fix: report pixel size in ioctl (https://github.com/zellij-org/zellij/pull/2212)
* fix: handle empty cwd from unreadable processes (https://github.com/zellij-org/zellij/pull/2213)
* fix: properly decode plugin urls with spaces (https://github.com/zellij-org/zellij/pull/2190)
* feat: QueryTabNames cli action (https://github.com/zellij-org/zellij/pull/2145)
* fix: log error instead of crashing when unable to set CWD in a template (https://github.com/zellij-org/zellij/pull/2214)
* fix: tab names in layout and gototabname crash on create (https://github.com/zellij-org/zellij/pull/2225)
* feat: allow simulating releases (https://github.com/zellij-org/zellij/pull/2194)
* feat: add args to new-tab action in keybinds (https://github.com/zellij-org/zellij/pull/2072)

  Eg:
  ```kdl
  tab {
    bind "n" { NewTab; SwitchToMode "Normal"; }
    bind "m" { NewTab { cwd "/tmp"; name "example"; layout "/tmp/example.kdl"; }; SwitchToMode "Normal"; }
  }
  ```

## [0.34.4] - 2022-12-13

* hotfix: fix panics when resizing with flexible plugin panes in layout (https://github.com/zellij-org/zellij/pull/2019)
* hotfix: allow non-absolute `SHELL` variables (https://github.com/zellij-org/zellij/pull/2013)

## [0.34.3] - 2022-12-09

* (BREAKING CHANGE) performance: change plugin data flow to improve render speed (https://github.com/zellij-org/zellij/pull/1934)
* (BREAKING CHANGE) performance: various render pipeline improvements (https://github.com/zellij-org/zellij/pull/1960)
* feat: support text input from clipboard (https://github.com/zellij-org/zellij/pull/1926)
* errors: Don't log errors from panes when quitting zellij (https://github.com/zellij-org/zellij/pull/1918)
* docs(contributing): update log path (https://github.com/zellij-org/zellij/pull/1927)
* fix: Fallback to `/bin/sh` if `SHELL` can't be read, panic if shell doesn't exist (https://github.com/zellij-org/zellij/pull/1769)
* feat(themes): add catppuccin themes (https://github.com/zellij-org/zellij/pull/1937)
* fix: treat relative paths properly in cli commands (https://github.com/zellij-org/zellij/pull/1947)
* fix: ensure ejected pane always has a frame (https://github.com/zellij-org/zellij/pull/1950)
* fix(compact-bar): mouse-click in simplified-ui (https://github.com/zellij-org/zellij/pull/1917)
* fix(themes): black and white inverted (https://github.com/zellij-org/zellij/pull/1953)
* fix(stability): gracefully handle SSH timeouts and other client buffer overflow issues (https://github.com/zellij-org/zellij/pull/1955)
* fix: empty session name (https://github.com/zellij-org/zellij/pull/1959)
* plugins: Cache plugins, don't load builtin plugins from disk (https://github.com/zellij-org/zellij/pull/1924)
* fix: server on longer crashes on client crash (https://github.com/zellij-org/zellij/pull/1965)
* fix: preserve pane focus properly when closing panes and switching tabs (https://github.com/zellij-org/zellij/pull/1966)
* fix(themes): missing tokyo-night-dark theme (https://github.com/zellij-org/zellij/pull/1972)
* refactor(plugins): fix plugin loading data flow (https://github.com/zellij-org/zellij/pull/1995)
* refactor(messaging): reduce extraneous cross-thread messaging (https://github.com/zellij-org/zellij/pull/1996)
* errors: preserve caller location in `to_log` (https://github.com/zellij-org/zellij/pull/1994)
* feat: show loading screen on startup (https://github.com/zellij-org/zellij/pull/1997)
* feat: Allow "reducing" resizes, refactor resizing code (https://github.com/zellij-org/zellij/pull/1990)

## [0.33.0] - 2022-11-10

* debugging: improve error handling in `zellij_server::pty` (https://github.com/zellij-org/zellij/pull/1840)
* feat: allow command panes to optionally close on exit (https://github.com/zellij-org/zellij/pull/1869)
* add: everforest-dark, everforest-light themes to the example theme directory (https://github.com/zellij-org/zellij/pull/1873)
* feat: support multiple themes in one file (https://github.com/zellij-org/zellij/pull/1855)
* debugging: Remove calls to unwrap in `zellij_server::ui::*` (https://github.com/zellij-org/zellij/pull/1870)
* debugging: Remove calls to unwrap in `zellij_server::pty_writer` (https://github.com/zellij-org/zellij/pull/1872)
* docs(example): update the format of the themes for the example directory (https://github.com/zellij-org/zellij/pull/1877)
* debugging: Remove calls to unwrap in `zellij_server::terminal_bytes` (https://github.com/zellij-org/zellij/pull/1876)
* debugging: Remove calls to unwrap in `zellij_server::output` (https://github.com/zellij-org/zellij/pull/1878)
* fix: resolve `zellij setup --clean` panic (https://github.com/zellij-org/zellij/pull/1882)
* feat: allow toggling mouse mode at runtime (https://github.com/zellij-org/zellij/pull/1883)
* fix: display status bar properly if limited to only 1 line (https://github.com/zellij-org/zellij/pull/1875)
* feat: allow starting command panes suspended (https://github.com/zellij-org/zellij/pull/1887)
* debugging: Remove calls to unwrap in `zellij_server::os_input_output` (https://github.com/zellij-org/zellij/pull/1895)
* fix: remove space key from shared_except (https://github.com/zellij-org/zellij/pull/1884)
* fix: clear search when sending terminating char (https://github.com/zellij-org/zellij/pull/1853)
* fix: properly convert the backslash key from old YAML config files (https://github.com/zellij-org/zellij/pull/1879)
* fix: clear floating panes indication when closing a floating command pane (https://github.com/zellij-org/zellij/pull/1897)
* Terminal compatibility: do not reset bold when resetting DIM (https://github.com/zellij-org/zellij/pull/1803)
* fix: Do not advertise 24 bit color support unchecked (https://github.com/zellij-org/zellij/pull/1900)
* fix: treat CWD properly when opening your editor through `zellij edit` or `ze` (https://github.com/zellij-org/zellij/pull/1904)
* fix: allow cli actions to be run outside of a tty environment (https://github.com/zellij-org/zellij/pull/1905)
* Terminal compatibility: send focus in/out events to terminal panes (https://github.com/zellij-org/zellij/pull/1908)
* fix: various bugs with no-frames and floating panes (https://github.com/zellij-org/zellij/pull/1909)
* debugging: Improve error logging in server (https://github.com/zellij-org/zellij/pull/1881)
* docs: add kanagawa theme (https://github.com/zellij-org/zellij/pull/1913)
* fix: use 'temp_dir' instead of hard-coded '/tmp/' (https://github.com/zellij-org/zellij/pull/1898)
* debugging: Don't strip debug symbols from release binaries (https://github.com/zellij-org/zellij/pull/1916)
* deps: upgrade termwiz to 0.19.0 and rust MSRV to 1.60.0 (https://github.com/zellij-org/zellij/pull/1896)

## [0.32.0] - 2022-10-25

* BREAKING CHANGE: switch config/layout/theme language to KDL (https://github.com/zellij-org/zellij/pull/1759)
* debugging: Improve error handling in screen thread (https://github.com/zellij-org/zellij/pull/1670)
* fix: Server exits when client panics (https://github.com/zellij-org/zellij/pull/1731)
* fix: Server panics when writing to suppressed pane (https://github.com/zellij-org/zellij/pull/1749)
* debugging: Improve error handling in screen thread private functions (https://github.com/zellij-org/zellij/pull/1770)
* fix(nix): add DiskArbitration and Foundation to darwin builds (https://github.com/zellij-org/zellij/pull/1724)
* debugging: Remove calls to `panic` in server/tab (https://github.com/zellij-org/zellij/pull/1748)
* debugging: Improve error format in server/thread_bus (https://github.com/zellij-org/zellij/pull/1775)
* feat: command pane - send commands to Zellij and re-run them with ENTER (https://github.com/zellij-org/zellij/pull/1787)
* fix: escape quotes and backslashes when converting YAML to KDL (https://github.com/zellij-org/zellij/pull/1790)
* fix: frameless pane wrong size after closing other panes (https://github.com/zellij-org/zellij/pull/1776)
* fix: error on mixed nodes in layouts (https://github.com/zellij-org/zellij/pull/1791)
* fix: error on duplicate pane_template / tab_template definitions in layouts (https://github.com/zellij-org/zellij/pull/1792)
* fix: accept session-name through the cli properly (https://github.com/zellij-org/zellij/pull/1793)
* fix: Prevent recursive sessions from layout files (https://github.com/zellij-org/zellij/pull/1766)
* fix: better error messages and recovery from layout issues (https://github.com/zellij-org/zellij/pull/1797)
* feat: allow layouts to have a global cwd (https://github.com/zellij-org/zellij/pull/1798)
* feat: edit panes in layouts (https://github.com/zellij-org/zellij/pull/1799)
* debugging: Log `thread_bus` IPC messages only in debug mode (https://github.com/zellij-org/zellij/pull/1800)
* feat: improve zellij run CLI (https://github.com/zellij-org/zellij/pull/1804)
* docs: Add tips for code contributions to CONTRIBUTING (https://github.com/zellij-org/zellij/pull/1805)
* feat: change floating panes to be grouped rather than scattered (https://github.com/zellij-org/zellij/pull/1810)
* fix: default to vi editor when we can't an editor in EDITOR or VISUAL and none is configured (https://github.com/zellij-org/zellij/pull/1811)
* deps: upgrade log4rs to 1.2.0 (https://github.com/zellij-org/zellij/pull/1814)
* feat: allow `DumpScreen` to dump the viewport by default (https://github.com/zellij-org/zellij/pull/1794)
* Terminal compatibility: clear scroll region when terminal pane is cleared (https://github.com/zellij-org/zellij/pull/1826)
* feat: allow defining tab cwd in layouts (https://github.com/zellij-org/zellij/pull/1828)
* debugging: Remove calls to `unwrap` from plugin WASM VM (https://github.com/zellij-org/zellij/pull/1827)
* debugging: Improve error handling in `server/route` (https://github.com/zellij-org/zellij/pull/1808)
* debugging: Detect plugin version mismatches (https://github.com/zellij-org/zellij/pull/1838)
* feat: add help to cli options (https://github.com/zellij-org/zellij/pull/1839)

## [0.31.4] - 2022-09-09
* Terminal compatibility: improve vttest compliance (https://github.com/zellij-org/zellij/pull/1671)
* fix: bracketed paste handling regression (https://github.com/zellij-org/zellij/pull/1689)
* fix: occasional startup crashes (https://github.com/zellij-org/zellij/pull/1706)
* fix: gracefully handle SSH disconnects (https://github.com/zellij-org/zellij/pull/1710)
* fix: handle osc params larger than 1024 bytes (https://github.com/zellij-org/zellij/pull/1711)
* Terminal compatibility: implement faux scrolling when in alternate screen mode(https://github.com/zellij-org/zellij/pull/1678)
* fix: mouse-click on tab-bar in simplified-ui now always focuses the correct tab (https://github.com/zellij-org/zellij/pull/1658)
* fix: sort UI cursors properly when multiple users are focused on the same pane (https://github.com/zellij-org/zellij/pull/1719)

## [0.31.3] - 2022-08-18
* HOTFIX: fix up-arrow regression

## [0.31.2] - 2022-08-17
* fix: crash when attaching to a session without the first tab (https://github.com/zellij-org/zellij/pull/1648)
* fix: race crash on startup when server is not ready (https://github.com/zellij-org/zellij/pull/1651)
* Terminal compatibility: forward OSC52 clipboard copy events from terminals (https://github.com/zellij-org/zellij/pull/1644)
* refactor: terminal characters (https://github.com/zellij-org/zellij/pull/1663)
* Terminal compatibility: properly send mouse clicks and drags to terminal panes (https://github.com/zellij-org/zellij/pull/1664)

## [0.31.1] - 2022-08-02
* add: `solarized-light` theme to the example theme directory (https://github.com/zellij-org/zellij/pull/1608)
* add(readme): more links to the documentation (https://github.com/zellij-org/zellij/pull/1621)
* fix theme not loading without config (https://github.com/zellij-org/zellij/pull/1631)

## [0.31.0] - 2022-07-28
* feat: Log errors causing "empty message received from client" (https://github.com/zellij-org/zellij/pull/1459)
* chore(dependencies): update `crossbeam` `0.8.0` -> `0.8.1` (https://github.com/zellij-org/zellij/pull/1463)
* add(option): `default-layout` setting for changing the default layout upon start, example: `default_layout: compact` (https://github.com/zellij-org/zellij/pull/1467)
* fix: many typos (https://github.com/zellij-org/zellij/pull/1481)
* add: checksum for release binary (https://github.com/zellij-org/zellij/pull/1482)
* fix: update cli tooltips (https://github.com/zellij-org/zellij/pull/1488)
* refactor: deduplicate code in `screen.rs` (https://github.com/zellij-org/zellij/pull/1453)
* chore(dependencies): update  `clap`: `3.1.18` -> `3.2.2` (https://github.com/zellij-org/zellij/pull/1496)
* fix: send `WriteChars:` once per action (https://github.com/zellij-org/zellij/pull/1516)
* feat: allow swapping tabs, in a fullscreen pane (https://github.com/zellij-org/zellij/pull/1515)
* feat: add action of undo rename (https://github.com/zellij-org/zellij/pull/1513)
* fix(docs): fix macport installation instructions (https://github.com/zellij-org/zellij/pull/1529)
* feat: allow hex colors for themes (https://github.com/zellij-org/zellij/pull/1536)
* fix: client hang when server is killed / shutdown delay (https://github.com/zellij-org/zellij/pull/1535)
* fix: properly handle in-place editor in full-screen (https://github.com/zellij-org/zellij/pull/1544)
* Terminal compatibility: properly trim whitespace in lines with wide-characters when resizing panes (https://github.com/zellij-org/zellij/pull/1545)
* fix: reset scroll properly when typing in certain edge cases (https://github.com/zellij-org/zellij/pull/1547)
* fix: logging may fill up /tmp, now logs are capped at 100 kB (https://github.com/zellij-org/zellij/pull/1548)
* fix: crash when terminal rows or columns are 0 (https://github.com/zellij-org/zellij/pull/1552)
* refactor: moved shared data structures to zellij-utils (https://github.com/zellij-org/zellij/pull/1541)
* feat: support displaying images/video in the terminal with sixel graphics (https://github.com/zellij-org/zellij/pull/1557)
* fix: add usage comment to fish `auto-start` script (https://github.com/zellij-org/zellij/pull/1583)
* fix: refactor match session name (https://github.com/zellij-org/zellij/pull/1582)
* fix: print "Session detached" rather than "Bye from Zellij!" when detaching from a session (https://github.com/zellij-org/zellij/pull/1573#issuecomment-1181562138)
* performance: improve terminal responsiveness (https://github.com/zellij-org/zellij/pull/1585 and https://github.com/zellij-org/zellij/pull/1610)
* Terminal compatibility: persist cursor show/hide across alternate screen (https://github.com/zellij-org/zellij/pull/1586)
* fix: support multi-argument EDITOR/VISUAL/scrollback-editor commands (https://github.com/zellij-org/zellij/pull/1587)
* fix: avoid sending mouse click events on pane frames to applications (https://github.com/zellij-org/zellij/pull/1584)
* feat: search through terminal scrollback (https://github.com/zellij-org/zellij/pull/1521)
* feat: support themes directory (https://github.com/zellij-org/zellij/pull/1577)
* feat: Improve logging by writing server panics into the logfile (https://github.com/zellij-org/zellij/pull/1602)
* fix: reflect configured keybindings in the status bar (https://github.com/zellij-org/zellij/pull/1242)
* add: capability to dispatch actions from the cli (https://github.com/zellij-org/zellij/pull/1265)

  This feature is gated behind the `unstable` feature flag.
  Because the serialization format will be changed at some point.
  We would still already be glad about early feedback on this feature.

  Can be invoked through `zellij action [ACTIONS]`.

  Automatically sends the action to the current session, or if there is just one
  to the single session, if there are multiple sessions, then the session name
  must be specified.

  Example:

  ```
  zellij
  zellij action NewTab:
  ```

  Send actions to a specific session:
  ```
  zellij -s fluffy-cat
  zellij -s fluffy-cat action 'NewPane: , WriteChars: "echo Purrr\n"'
  ```

  Open `htop` in a new tab:
  ```
  zj action "NewTab: {run: {command: {cmd: htop}}}"
  ```

## [0.30.0] - 2022-06-07
* fix: right and middle clicks creating selection (https://github.com/zellij-org/zellij/pull/1372)
* feat: Attach to sessions more conveniently by only typing their name's first character(s) (https://github.com/zellij-org/zellij/pull/1360)
* fix: a small typo (https://github.com/zellij-org/zellij/pull/1390)
* feat: show subcommand aliases in help output (https://github.com/zellij-org/zellij/pull/1409)
* chore(dependencies): rename crate `suggestion` -> `suggest` (https://github.com/zellij-org/zellij/pull/1387)
* fix: update to output error when using `--layout` (https://github.com/zellij-org/zellij/pull/1413)
* fix: ANSI output sent to terminal on resize in certain cases (https://github.com/zellij-org/zellij/pull/1384)
* fix: freeze when pasting large amounts of text to vim (https://github.com/zellij-org/zellij/pull/1383)
* feat: new action to dump the scrollbuffer to a file (https://github.com/zellij-org/zellij/pull/1375)
* fix(strider): update out of range index in files (https://github.com/zellij-org/zellij/pull/1425)
* feat: strip debug symbols of release builds 20% size reduction, MSRV is now `1.59` (https://github.com/zellij-org/zellij/pull/1177)
* chore(dependencies): update `names` and `dialoguer` crates (https://github.com/zellij-org/zellij/pull/1430)
* fix: add checking for missing extensions (https://github.com/zellij-org/zellij/pull/1432)
* fix: client process hanging / not exiting when terminal emulator was closed (https://github.com/zellij-org/zellij/pull/1433)
* BREAKING CHANGE: merge `--layout` and `--layout-path` (https://github.com/zellij-org/zellij/pull/1426)
* add: a version of the `tab-bar` plugin, that carries mode information, called `compact-bar`
also adds a new default layout called `compact`, which can be loaded with: `zellij --layout compact`,
that loads the `compact-bar`. (https://github.com/zellij-org/zellij/pull/1450)
* feat: allow searching through and editing the pane scrollback with your default editor (https://github.com/zellij-org/zellij/pull/1456)
* fix: exit client loop on empty message from server (https://github.com/zellij-org/zellij/pull/1454)
* fix: mouse selection sometimes getting stuck (https://github.com/zellij-org/zellij/pull/1418)
* feat: tweak simplified UI (https://github.com/zellij-org/zellij/pull/1458)
* feat: add status more tips (https://github.com/zellij-org/zellij/pull/1462)
* add: new features to manpage (https://github.com/zellij-org/zellij/pull/1549)

## [0.29.1] - 2022-05-02
* fix: forward mouse events to plugin panes (https://github.com/zellij-org/zellij/pull/1369)

## [0.29.0] - 2022-05-02
* add: clarify copy to clipboard message (https://github.com/zellij-org/zellij/pull/1321)
* Terminal compatibility: fix ANSI scrolling regression (https://github.com/zellij-org/zellij/pull/1324)
* fix: send SIGHUP instead of SIGTERM when closing a pane (https://github.com/zellij-org/zellij/pull/1320)
* add: `copy_on_select` option to configure automatic copy behavior (https://github.com/zellij-org/zellij/pull/1298)
* fix: minor system improvements (https://github.com/zellij-org/zellij/pull/1328)
* add: add command for auto-start script (https://github.com/zellij-org/zellij/pull/1281)
* Terminal compatibility: fix cursor pane escape and invalid ansi crash (https://github.com/zellij-org/zellij/pull/1349)
* fix: recover from corrupted ipc bus state (https://github.com/zellij-org/zellij/pull/1351)
* Terminal compatibility: respond to foreground/background color ansi requests (OSC 10 and 11) (https://github.com/zellij-org/zellij/pull/1358)
* fix: avoid panic in link_handler.rs (https://github.com/zellij-org/zellij/pull/1356)
* Terminal compatibility: prevent wide chars from overflowing the title line (https://github.com/zellij-org/zellij/pull/1361)
* Terminal compatibility: adjust saved cursor position on resize (https://github.com/zellij-org/zellij/pull/1362)
* fix: avoid panic on renaming a floating pane (https://github.com/zellij-org/zellij/pull/1357)
* fix: change the way sessions are sorted (https://github.com/zellij-org/zellij/pull/1347)
* fix: improve mouse event reporting, avoid clicks on plugin panes causing active pane scrolling (https://github.com/zellij-org/zellij/pull/1329)

## [0.28.1] - 2022-04-13
* (BREAKING CHANGE) Feature: Improve theme usage and add default themes. Remove gray color from themes. (https://github.com/zellij-org/zellij/pull/1274)
* repo: add `.git-blame-ignore-revs-file` (https://github.com/zellij-org/zellij/pull/1295)
* add: `musl` target to `rust-toolchain` (https://github.com/zellij-org/zellij/pull/1294)
* fix: update termwiz to fix crash when pasting on wsl (https://github.com/zellij-org/zellij/pull/1303)
* add: nord theme example (https://github.com/zellij-org/zellij/pull/1304)
* Terminal compatibility: preserve background color when scrolling (https://github.com/zellij-org/zellij/pull/1305 and https://github.com/zellij-org/zellij/pull/1307)
* add: `overlays` to the `flake` `outputs`  (https://github.com/zellij-org/zellij/pull/1312)
* refactor: reduce code duplication in tiled_panes (https://github.com/zellij-org/zellij/pull/1299)
* Terminal compatibility: support XTWINOPS CSI 14 + 16 to query terminal pixel info (https://github.com/zellij-org/zellij/pull/1316)
* Fix: Update UI when next-to-last user manually detaches from the session (https://github.com/zellij-org/zellij/pull/1317)

## [0.27.0] - 2022-03-31
* Fix: feature `disable_automatic_asset_installation` (https://github.com/zellij-org/zellij/pull/1226)
* Fix: `wasm_vm` use `cache_dirs` for ephemeral plugin data (https://github.com/zellij-org/zellij/pull/1230)
* Bump `nix` version to `0.23.1` (https://github.com/zellij-org/zellij/pull/1234)
* Refactor: move tiled_panes to their own module (https://github.com/zellij-org/zellij/pull/1239)
* Add: allow rounded frame corners to be selected in the config (https://github.com/zellij-org/zellij/pull/1227)
* Deps: move from termion to termwiz (https://github.com/zellij-org/zellij/pull/1249)
* Fix: resolve crash when opening tab and zellij tmp dir does not exist (https://github.com/zellij-org/zellij/pull/1256)
* Fix: Behave properly when embedding floating pane into a fullscreen tiled pane (https://github.com/zellij-org/zellij/pull/1267)
* Fix: various screen crashes in some edge cases (https://github.com/zellij-org/zellij/pull/1269)
* Feat: Add Alt+Arrows quick navigation (https://github.com/zellij-org/zellij/pull/1264)
* Fix: don't crash on bad intermediate tab state (https://github.com/zellij-org/zellij/pull/1272)
* Fix: resolve crash when closing panes on single core systems (https://github.com/zellij-org/zellij/pull/1051)
* Terminal Compatibility: Behave properly when ansi scrolling down with an undefined scroll region (https://github.com/zellij-org/zellij/pull/1279)
* Fix: properly render selection when background color of characters is not set (https://github.com/zellij-org/zellij/pull/1250)
* Terminal Compatibility: revert previous incorrect change to csi erase display (https://github.com/zellij-org/zellij/pull/1283)

## [0.26.1] - 2022-03-16
* HOTFIX: Paste regression (https://github.com/zellij-org/zellij/commit/08d2014cfea1583059338a338bc4d5f632763fdb)
* Add: add error reporting system (https://github.com/zellij-org/zellij/pull/1038)
* Fix: switch to annotated release tags (https://github.com/zellij-org/zellij/pull/1223)

## [0.26.0] - 2022-03-11
* Fix: invalid assignment of `client_id` (https://github.com/zellij-org/zellij/pull/1052)
* Add: action to send `^b` in `tmux-mode` (https://github.com/zellij-org/zellij/pull/1106)
* Add: various action bindings to `tmux-mode` (https://github.com/zellij-org/zellij/pull/1098)
* Terminal compatibility: set terminal title properly (https://github.com/zellij-org/zellij/pull/1094)
* Fix: handle discontiguous STDIN input (https://github.com/zellij-org/zellij/issues/1117)
* Terminal compatibility: fix alternate screen clearing (https://github.com/zellij-org/zellij/pull/1120)
* Add: information about clippy lints (https://github.com/zellij-org/zellij/pull/1126)
* Bump `suggestion` dependency (https://github.com/zellij-org/zellij/pull/1124)
* Add: detach `action` to `tmux-mode` (https://github.com/zellij-org/zellij/pull/1116)
* Add: initial `nix` support (https://github.com/zellij-org/zellij/pull/1131)
* Fix: unused code warnings (https://github.com/zellij-org/zellij/pull/1087)
* Add: support `cargo-binstall` (https://github.com/zellij-org/zellij/pull/1129)
* Fix: do not use current cursor style in csi erase display (solve `btm` rendering issue) (https://github.com/zellij-org/zellij/pull/1142)
* Fix: ensure e2e tests use current plugins (https://github.com/zellij-org/zellij/pull/1047)
* Add: manpage to nix package (https://github.com/zellij-org/zellij/pull/1148)
* Fix: terminal title passthrough on not showing pane frames (https://github.com/zellij-org/zellij/pull/1113)
* Add: ability to set `ENVIRONMENT VARIABLES` inside of the config and layout's (https://github.com/zellij-org/zellij/pull/1154)
* Add: binary cache to zellij `cachix use zellij` (https://github.com/zellij-org/zellij/pull/1157)
* Fix: improve layout naming (https://github.com/zellij-org/zellij/pull/1160)
* Add: installation instructions for `Void Linux` (https://github.com/zellij-org/zellij/pull/1165)
* (BREAKING CHANGE) Fix: `list-session` to error and stderr on fail (https://github.com/zellij-org/zellij/pull/1174)
  This is a BREAKING CHANGE for people that relied on the
  error code and the stdout of this command on fail.
* Add: dynamic completions for `fish` shell (https://github.com/zellij-org/zellij/pull/1176)
* Fix: typo in completion (https://github.com/zellij-org/zellij/pull/1183)
* Fix: improve detach instruction (https://github.com/zellij-org/zellij/pull/1161)
* Fix: update tooltip after hiding floating panes with mouse (https://github.com/zellij-org/zellij/pull/1186)
* Fix: do not start move floating pane when selecting with mouse and cursor leaves pane (https://github.com/zellij-org/zellij/pull/1186)
* Terminal compatibility: replace wide-characters under cursor properly (https://github.com/zellij-org/zellij/pull/1196)
* Terminal compatibility: only adjust home and end keys in cursor keys mode (https://github.com/zellij-org/zellij/pull/1190)
* Add: initial support for forwarding mouse events to applications (`SGR` format only) (https://github.com/zellij-org/zellij/pull/1191)
* Fix: allow `POSIX` style overrides for most config flags (https://github.com/zellij-org/zellij/pull/1205)

## [0.25.0] - 2022-02-22
* Fix: replace the library with the dependency problem (https://github.com/zellij-org/zellij/pull/1001)
* Fix: crash when opening pane in non-existent cwd (https://github.com/zellij-org/zellij/pull/995)
* Feature: add `copy-command` option (https://github.com/zellij-org/zellij/pull/996)
* Feature: update parsing crate to `clap v3.0` (https://github.com/zellij-org/zellij/pull/1017)
* Feature: accept only printable unicode char when rename pane or tab name (https://github.com/zellij-org/zellij/pull/1016)
* Fix: scroll page up/down by actual amount of rows (https://github.com/zellij-org/zellij/pull/1025)
* Fix: handle csi erase param 3 (https://github.com/zellij-org/zellij/pull/1026)
* Add: theme example for `tokyo-night` (https://github.com/zellij-org/zellij/pull/1015)
* Fix: log a warning, if a user-configured mode has no actions associated and is active (https://github.com/zellij-org/zellij/pull/1035)
* Feature: add focus attribute in layout (https://github.com/zellij-org/zellij/pull/958)
* Compatibility: disable scrollback in alternate screen (https://github.com/zellij-org/zellij/pull/1032)
* Feature: add `copy-clipboard` option (https://github.com/zellij-org/zellij/pull/1022)
* Fix: update the confusing tips on `RenamePane` (https://github.com/zellij-org/zellij/pull/1045)
* Feature: add floating panes (https://github.com/zellij-org/zellij/pull/1066)
* Fix: bump up internal `autocfg` dependency to `1.1.0` (https://github.com/zellij-org/zellij/pull/1071)
* Feature: add tmux mode (https://github.com/zellij-org/zellij/pull/1073)
* Fix: improve copy of wrapped lines (https://github.com/zellij-org/zellij/pull/1069)
* Fix: prefer last active pane when changing focus (https://github.com/zellij-org/zellij/pull/1076)

## [0.24.0] - 2022-01-05
* Terminal compatibility: properly handle insertion of characters in a line with wide characters (https://github.com/zellij-org/zellij/pull/964)
* Terminal compatibility: properly handle deletion of characters in a line with wide characters (https://github.com/zellij-org/zellij/pull/965)
* Fix: properly remove clients when detaching from a session (https://github.com/zellij-org/zellij/pull/966)
* Fix: plugin theme coloring (https://github.com/zellij-org/zellij/pull/975)
* Fix: prevent unhandled mouse events escape to terminal (https://github.com/zellij-org/zellij/pull/976)
* Fix: ensure clippy runs on all targets (https://github.com/zellij-org/zellij/pull/972)
* Fix: atomically create default assets every time a session starts (https://github.com/zellij-org/zellij/pull/961)
* Fix: Allow multiple users to switch tabs with the mouse (https://github.com/zellij-org/zellij/pull/959)
* Fix: Allow switching tabs with the mouse when pane is in fullscreen (https://github.com/zellij-org/zellij/pull/977)
* Fix: pass bell (helpful for eg. desktop notifications) from terminal to desktop (https://github.com/zellij-org/zellij/pull/981)
* Fix: tab click crash on mouse click with multiple users (https://github.com/zellij-org/zellij/pull/984)
* Fix: accidental tab synchronization bug between multiple users when clicking with mouse (https://github.com/zellij-org/zellij/pull/986)
* Fix: Properly move users out of closed tab in a multiuser session (https://github.com/zellij-org/zellij/pull/990)
* Feature: Pass active pane title to terminal emulator (https://github.com/zellij-org/zellij/pull/980)
* Feature: Improve default keybindings (https://github.com/zellij-org/zellij/pull/991)
* Feature: Configurable scroll buffer size (https://github.com/zellij-org/zellij/pull/936)

## [0.23.0] - 2021-12-20
* Feature: add collaboration support - multiple users using multiple cursors (https://github.com/zellij-org/zellij/pull/957)

## [0.22.1] - 2021-12-14
* Hotfix: Focus fullscreen pane when switching tab focus (https://github.com/zellij-org/zellij/pull/941)

## [0.22.0] - 2021-12-13
* Fix: missing themes in configuration merge (https://github.com/zellij-org/zellij/pull/913)
* Fix: add `gray` to theme section (https://github.com/zellij-org/zellij/pull/914)
* Fix: prevent zellij session from attaching to itself (https://github.com/zellij-org/zellij/pull/911)
* Terminal compatibility: fix flaky scrolling issue (https://github.com/zellij-org/zellij/pull/915)
* Fix: handle pasted text properly in windows terminal (https://github.com/zellij-org/zellij/pull/917)
* Fix: update example config options (https://github.com/zellij-org/zellij/pull/920)
* Fix: correct handling of unbinds (https://github.com/zellij-org/zellij/issues/923)
* Fix: improve performance when resizing window with a large scrollback buffer (https://github.com/zellij-org/zellij/pull/895)
* Fix: support multiple users in plugins (https://github.com/zellij-org/zellij/pull/930)
* Fix: update default layouts (https://github.com/zellij-org/zellij/pull/926)
* Add: infrastructure to show distinct tips in the `status-bar` plugin (https://github.com/zellij-org/zellij/pull/926)
* Feature: Allow naming panes (https://github.com/zellij-org/zellij/pull/928)

## [0.21.0] - 2021-11-29
* Add: initial preparations for overlay's (https://github.com/zellij-org/zellij/pull/871)
* Add: initial `zellij.desktop` file (https://github.com/zellij-org/zellij/pull/870)
* Add: section for third party repositories `THIRD_PARTY_INSTALL.md` (https://github.com/zellij-org/zellij/pull/857)
* Add: suggestion for similar session name, on attach (https://github.com/zellij-org/zellij/pull/843)
* Fix: handling and overwriting options through the cli (https://github.com/zellij-org/zellij/pull/859)

  THIS IS A BREAKING CHANGE:
  Previously it was only possible to turn off certain features through the cli,
  now it also is possible to overwrite this behavior - for that the following changed:

  - renamed and inverted:
  ```
  disable_mouse_mode -> mouse_mode
  no_pane_frames -> pane_frames
  ```
  - cli options added:
  ```
  mouse-mode [bool]
  pane-frames [bool]
  simplified-ui [bool]
  ```
  - cli flag removed:
  ```
  simplified-ui
  ```

  Now the cli options can optionally be toggled on, even if the config
  turns it off, example:
  ```
  zellij options --mouse-mode true
  ```
* Fix: fix CSI cursor next line not moving cursor to beginning of line after moving it down (https://github.com/zellij-org/zellij/pull/863)
* Refactor: Support multiple users in `Tab`s (https://github.com/zellij-org/zellij/pull/864)
* Refactor: close_pane returns closed pane (https://github.com/zellij-org/zellij/pull/853)
* Add: ability to configure zellij through layouts (https://github.com/zellij-org/zellij/pull/866)
* Refactor: simplify terminal character style diff (https://github.com/zellij-org/zellij/pull/839)
* Fix: improve performance with large scrollback buffer (https://github.com/zellij-org/zellij/pull/881)
* Add: support osc8 escape code (https://github.com/zellij-org/zellij/pull/822)
* Add: optionally leave ephemeral modes by pressing the `esc` key to default config (https://github.com/zellij-org/zellij/pull/889)
* Feature: Multiple users UI for panes behind a turned-off feature flag (https://github.com/zellij-org/zellij/pull/897)
* Add: plugin api, to provide version information to plugins (https://github.com/zellij-org/zellij/pull/894)


## [0.20.1] - 2021-11-10
* Add: initial session name to layout template (https://github.com/zellij-org/zellij/pull/789)
* Fix: simplify matches (https://github.com/zellij-org/zellij/pull/844)
* Add: support darwin builds on ci (https://github.com/zellij-org/zellij/pull/846)
* Add: e2e instructions for x86 and arm darwin systems (https://github.com/zellij-org/zellij/pull/846)
* Fix: use key-value style for `docker-compose` (https://github.com/zellij-org/zellij/issues/338)
* Fix: unify zellij environment variable handling (https://github.com/zellij-org/zellij/pull/842)
* Add: toggle boolean options with cli flags (https://github.com/zellij-org/zellij/pull/855)

* HOTFIX: fix pasting regression (https://github.com/zellij-org/zellij/pull/858)

## [0.20.0] - 2021-11-08
* Fix: improve performance of echoed keystrokes (https://github.com/zellij-org/zellij/pull/798)
* Add: Use hyperlinks for the setup information (https://github.com/zellij-org/zellij/pull/768)
* Feature: Rotate Pane location (https://github.com/zellij-org/zellij/pull/802)
* Terminal compatibility: improve handling of wide-characters when inserted mid-line (https://github.com/zellij-org/zellij/pull/806)
* Fix: plugins are now only compiled once and cached on disk (https://github.com/zellij-org/zellij/pull/807)
* Fix: pasted text performs much faster and doesn't kill Termion (https://github.com/zellij-org/zellij/pull/810)
* Fix: resizing/scrolling through heavily wrapped panes no longer hangs (https://github.com/zellij-org/zellij/pull/814)
* Terminal compatibility: properly handle HOME/END keys in eg. vim/zsh (https://github.com/zellij-org/zellij/pull/815)
* Fix: Typo (https://github.com/zellij-org/zellij/pull/821)
* Fix: Update `cargo-make` instructions post `v0.35.3` (https://github.com/zellij-org/zellij/pull/819)
* Fix: Unused import for darwin systems (https://github.com/zellij-org/zellij/pull/820)
* Add: `WriteChars` action (https://github.com/zellij-org/zellij/pull/825)
* Fix: typo and grammar (https://github.com/zellij-org/zellij/pull/826)
* Add: `rust-version` - msrv field to `Cargo.toml` (https://github.com/zellij-org/zellij/pull/828)
* Fix: improve memory utilization, reap both sides of pty properly and do not expose open FDs to child processes (https://github.com/zellij-org/zellij/pull/830)
* Fix: move from the deprecated `colors_transform` to `colorsys` (https://github.com/zellij-org/zellij/pull/832)
* Feature: plugins can now detect right mouse clicks (https://github.com/zellij-org/zellij/pull/801)
* Fix: open pane in cwd even when explicitly specifying shell (https://github.com/zellij-org/zellij/pull/834)
* Fix: do not resize panes below minimum (https://github.com/zellij-org/zellij/pull/838)
* Feature: Non directional resize of panes (https://github.com/zellij-org/zellij/pull/520)
* Add: `colored` crate to replace manual color formatting (https://github.com/zellij-org/zellij/pull/837)
* Add: introduce `thiserrror` to simplify error types (https://github.com/zellij-org/zellij/pull/836)
* Add: support `--index` option for the `attach` subcommand in order to
  choose the session indexed by the provided creation date (https://github.com/zellij-org/zellij/pull/824)
* Fix: simplify the main function significantly (https://github.com/zellij-org/zellij/pull/829)
* Feature: half page scrolling actions (https://github.com/zellij-org/zellij/pull/813)

## [0.19.0] - 2021-10-20
* Fix: Prevent text overwrite when scrolled up (https://github.com/zellij-org/zellij/pull/655)
* Add: Treat empty config files as empty yaml documents (https://github.com/zellij-org/zellij/pull/720)
* Fix: Commands that don't interact with the config file don't throw errors on malformed config files (https://github.com/zellij-org/zellij/pull/765)
* Add: Add config options to default config file (https://github.com/zellij-org/zellij/pull/766)
* Fix: Properly clear "FULLSCREEN" status when a pane exits on its own (https://github.com/zellij-org/zellij/pull/757)
* Refactor: handle clients in tabs/screen (https://github.com/zellij-org/zellij/pull/770)
* Feature: kill-session and kill-all-sessions cli commands (https://github.com/zellij-org/zellij/pull/745)
* Fix: Keep default file permissions for new files (https://github.com/zellij-org/zellij/pull/777)
* Feature: Add mouse events to plugins – including strider and the tab-bar (https://github.com/zellij-org/zellij/pull/629)
* Feature: Directional movement of panes (https://github.com/zellij-org/zellij/pull/762)
* Refactor: More groundwork to support multiple-clients in tabs (https://github.com/zellij-org/zellij/pull/788)

## [0.18.1] - 2021-09-30

* HOTFIX: mouse selection now working (https://github.com/zellij-org/zellij/pull/752)
* HOTFIX: prevent strider from descending into /host folder (https://github.com/zellij-org/zellij/pull/753)

## [0.18.0] - 2021-09-29
* Fix: Properly open new pane with CWD also when switching to a new tab (https://github.com/zellij-org/zellij/pull/729)
* Feature: Option to create a new session if attach fails (`zellij attach --create`) (https://github.com/zellij-org/zellij/pull/731)
* Feature: Added the new `Visible` event, allowing plugins to detect if they are visible in the current tab (https://github.com/zellij-org/zellij/pull/717)
* Feature: Plugins now have access to a data directory at `/data` – the working directory is now mounted at `/host` instead of `.` (https://github.com/zellij-org/zellij/pull/723)
* Feature: Add ability to solely specify the tab name in the `tabs` section (https://github.com/zellij-org/zellij/pull/722)
* Feature: Plugins can be configured and the groundwork for "Headless" plugins has been laid (https://github.com/zellij-org/zellij/pull/660)
* Automatically update `example/default.yaml` on release (https://github.com/zellij-org/zellij/pull/736)
* Feature: allow mirroring sessions in multiple terminal windows (https://github.com/zellij-org/zellij/pull/740)
* Feature: display a message when the current pane is in full-screen (https://github.com/zellij-org/zellij/pull/450)
* Terminal compatibility: handle cursor movements outside scroll region (https://github.com/zellij-org/zellij/pull/746)
* Terminal compatibility: scroll lines into scrollback when clearing viewport (https://github.com/zellij-org/zellij/pull/747)

## [0.17.0] - 2021-09-15
* New panes/tabs now open in CWD of focused pane (https://github.com/zellij-org/zellij/pull/691)
* Fix bug when opening new tab the new pane's viewport would sometimes be calculated incorrectly (https://github.com/zellij-org/zellij/pull/683)
* Fix bug when in some cases closing a tab would not clear the previous pane's contents (https://github.com/zellij-org/zellij/pull/684)
* Fix bug where tabs would sometimes be created with the wrong index in their name (https://github.com/zellij-org/zellij/pull/686)
* Fix bug where wide chars would mess up pane titles (https://github.com/zellij-org/zellij/pull/698)
* Fix various borderless-frame in viewport bugs (https://github.com/zellij-org/zellij/pull/697)
* Fix example configuration file (https://github.com/zellij-org/zellij/pull/693)
* Fix various tab bar responsiveness issues (https://github.com/zellij-org/zellij/pull/703)
* Allow plugins to run system commands (https://github.com/zellij-org/zellij/pull/666)
  * This has also added a temporary new permission flag that needs to be specified in the layout. This is a breaking change:
    ```yaml
    ...
    plugin: strider
    ...
    ```
    has become:
    ```yaml
    plugin:
      path: strider
    ```
    A plugin can be given command executing permission with:
    ```yaml
    plugin:
      path: strider
      _allow_exec_host_cmd: true
    ```
* Use the unicode width in tab-bar plugin, for tab names (https://github.com/zellij-org/zellij/pull/709)
* Fix automated builds that make use of the `setup` subcommand (https://github.com/zellij-org/zellij/pull/711)
* Add option to specify a tabs name in the tab `layout` file (https://github.com/zellij-org/zellij/pull/715)
* Improve handling of empty valid `yaml` files (https://github.com/zellij-org/zellij/pull/716)
* Add options subcommand to attach (https://github.com/zellij-org/zellij/pull/718)
* Fix: do not pad empty pane frame title (https://github.com/zellij-org/zellij/pull/724)
* Fix: Do not overflow empty lines when resizing panes (https://github.com/zellij-org/zellij/pull/725)


## [0.16.0] - 2021-08-31
* Plugins don't crash zellij anymore on receiving mouse events (https://github.com/zellij-org/zellij/pull/620)
* A universal logging system has been implemented (https://github.com/zellij-org/zellij/pull/592)
  * Added [`log`](https://docs.rs/log/0.4.14/log/#macros) crate support for logging within Zellij
  * Messages sent over the `stderr` of plugins are now logged as well, bringing back `dbg!` support!
* Add displaying of the `session-name` to the `tab-bar` (https://github.com/zellij-org/zellij/pull/608)
* Add command to dump `layouts` to stdout (https://github.com/zellij-org/zellij/pull/623)
  * `zellij setup --dump-layout [LAYOUT]` [default, strider, disable-status]
* Add `action`: `ScrollToBottom` (https://github.com/zellij-org/zellij/pull/626)
  * Bound by default to `^c` in `scroll` mode, scrolls to bottom and exists the scroll mode
* Simplify deserialization slightly (https://github.com/zellij-org/zellij/pull/633)
* Fix update plugin attributes on inactive tab (https://github.com/zellij-org/zellij/pull/634)
* New pane UI: draw pane frames - can be disabled with ctrl-p + z, or through configuration (https://github.com/zellij-org/zellij/pull/643)
* Terminal compatibility: support changing index colors through OSC 4 and similar (https://github.com/zellij-org/zellij/pull/646)
* Fix various shells (eg. nushell) unexpectedly exiting when the user presses ctrl-c (https://github.com/zellij-org/zellij/pull/648)
* Fix line wrapping while scrolling (https://github.com/zellij-org/zellij/pull/650)
* Indicate to the user when text is copied to the clipboard with the mouse (https://github.com/zellij-org/zellij/pull/642)
* Terminal compatibility: properly paste multilines (https://github.com/zellij-org/zellij/pull/653 + https://github.com/zellij-org/zellij/pull/658)
* Terminal compatibility: fix progress bar line overflow (http://github.com/zellij-org/zellij/pull/656)
* Add action to toggle between tabs `ToggleTab`, bound by default to [TAB] in tab mode (https://github.com/zellij-org/zellij/pull/622)
* Terminal compatibility: properly handle cursor shape changes in eg. Neovim (https://github.com/zellij-org/zellij/pull/659)
* The resize and layout systems have been overhauled (https://github.com/zellij-org/zellij/pull/568)
  * Resizing a terminal then returning it to its original size will now always return panes to their original sizes and positions
  * Resize mode resizes panes by 5% of the space on screen, not some fixed number
  * Panes on-screen keep their ratios – a screen split 50/50 between two panes will remain 50/50 even as the terminal is resized (https://github.com/zellij-org/zellij/issues/406)
  * The terminal can now be resized without leaving fullscreen mode
  * Layout parts are split into equal percentages if no explicit split-size is given (https://github.com/zellij-org/zellij/issues/619)
  * Fixed display of the tab bar at small terminal widths
* Add `tabs` to `layouts` (https://github.com/zellij-org/zellij/pull/625)

  The layout has now a template, and tabs section.
  The template specifies the location a tab is inserted in with `body: true`.

  Eg:
  ```
  ---
  template:
    direction: Horizontal
    parts:
      - direction: Vertical
        borderless: true
        split_size:
          Fixed: 1
        run:
          plugin: tab-bar
      - direction: Vertical # <= The location of
        body: true          # <= the inserted tab.
      - direction: Vertical
        borderless: true
        split_size:
          Fixed: 2
        run:
          plugin: status-bar
  tabs:
    - direction: Vertical # <= Multiple tabs can be
    - direction: Vertical # <= specified in the layout.
    - direction: Vertical
  ```

  The `NewTab` action can optionally be bound to open
  a layout that is assumed to be in the new `tabs` section

  This is a BREAKING CHANGE for people that have the
  `NewTab` action already bound in the config file:
  ```
  - action: [NewTab, ]
    key: [F: 5,]
  ```
  must now be specified as:
  ```
  - action: [NewTab: ,]
    key: [F: 5,]
  ```

  Optionally a layout that should be opened on the new tab can be
  specified:
  ```
  - action: [NewTab: {
    direction: Vertical,
    parts: [ {direction: Horizontal, split_size: {Percent: 50}},
    {direction: Horizontal, run: {command: {cmd: "htop"}}},],
    key: [F: 6,]
  ```


## [0.15.0] - 2021-07-19
* Kill children properly (https://github.com/zellij-org/zellij/pull/601)
* Change name of `Run` binding for actions (https://github.com/zellij-org/zellij/pull/602)
* Add running commands to `layouts` (https://github.com/zellij-org/zellij/pull/600)

  POSSIBLE BREAKING CHANGE for custom layouts:
  Plugins are under the run category now, that means:
  ```
  plugin: status-bar
  ```
  is now:
  ```
  run:
      plugin: status-bar
  ```
* Add `on_force_close` config option (https://github.com/zellij-org/zellij/pull/609)


## [0.14.0] - 2021-07-05
* Add improved error handling for layouts (https://github.com/zellij-org/zellij/pull/576)
* Change layout directory from data to config (https://github.com/zellij-org/zellij/pull/577)
  POSSIBLE BREAKING CHANGE:
  In case of having custom layouts in the previous
  `layout-dir` one can switch either the layouts to
  the new dir, or set the `layout-dir` to be the current
  `layout-dir`
* Fix `Makefile.toml` because of missing directory (https://github.com/zellij-org/zellij/pull/580)
* Autodetach on force close (https://github.com/zellij-org/zellij/pull/581)
* Add option to specify a default shell (https://github.com/zellij-org/zellij/pull/594)
* Add action to run bound commands in a pane (https://github.com/zellij-org/zellij/pull/596)
* Initial mouse support (https://github.com/zellij-org/zellij/pull/448)
* Add `layout-dir` to `setup --check` subcommand (https://github.com/zellij-org/zellij/pull/599)

## [0.13.0] - 2021-06-04
* Fix crash when padding before widechar (https://github.com/zellij-org/zellij/pull/540)
* Do not lag when reading input too fast (https://github.com/zellij-org/zellij/pull/536)
* Session name optional in attach command (https://github.com/zellij-org/zellij/pull/542)
* Fix build on platforms with TIOCGWINSZ / ioctl() integer type mismatch (https://github.com/zellij-org/zellij/pull/547)
* Fix(ui): session mode should be disabled in locked mode (https://github.com/zellij-org/zellij/pull/548)
* Add option to start in arbitrary modes (https://github.com/zellij-org/zellij/pull/513)
* Attaching to a session respects the `default_mode` setting of the client (https://github.com/zellij-org/zellij/pull/549)
* Add option to specify a color theme in the config (https://github.com/zellij-org/zellij/pull/550)
* Fix config options to not depend on `simplified_ui` (https://github.com/zellij-org/zellij/pull/556)
* Don't rename `unnamed` tabs upon deletion of other tabs (https://github.com/zellij-org/zellij/pull/554)
* Add layout to disable the status bar (https://github.com/zellij-org/zellij/pull/555)
* Significantly improve terminal pane performance (https://github.com/zellij-org/zellij/pull/567)

## [0.12.1] - 2021-05-28
* HOTFIX: fix Zellij not responding to input on certain terminals (https://github.com/zellij-org/zellij/issues/538)

## [0.12.0] - 2021-05-27
* Remove unused imports (https://github.com/zellij-org/zellij/pull/504)
* More Infrastructure changes for the upcoming session detach feature: run server and client in separate processes (https://github.com/zellij-org/zellij/pull/499)
* Restructuring cargo workspace: Separate client, server and utils into separate crates (https://github.com/zellij-org/zellij/pull/515)
* Terminal compatibility: handle most OSC sequences (https://github.com/zellij-org/zellij/pull/517)
* Split `layout` flag into `layout` and `layout-path` (https://github.com/zellij-org/zellij/pull/514)
* Fix behaviour of the `clean` flag (https://github.com/zellij-org/zellij/pull/519)
* Make distinction clearer between certain configuration flags (https://github.com/zellij-org/zellij/pull/529)
* Resource usage and performance improvements (https://github.com/zellij-org/zellij/pull/523)
* Feature: Detachable/Persistent sessions (https://github.com/zellij-org/zellij/pull/531)
* Terminal compatibility: Support wide characters (https://github.com/zellij-org/zellij/pull/535)

## [0.11.0] - 2021-05-15

This version is mostly an installation hotfix.

* Add `check` flag to `setup` subcommand, move `generate-completions` subcommand to `setup` flag (https://github.com/zellij-org/zellij/pull/503)
* Change the asset installation from an opt-in to an opt-out (https://github.com/zellij-org/zellij/pull/512)

## [0.10.0] - 2021-05-14
* Change Switch default config loading order of `HOME` and system (https://github.com/zellij-org/zellij/pull/488)
* Add support for requesting a simpler layout from plugins, move `clean` flag from `options` to `setup` (https://github.com/zellij-org/zellij/pull/479)
* Improve config loading slightly (https://github.com/zellij-org/zellij/pull/492)
* Terminal compatibility: preserve current style when clearing viewport (https://github.com/zellij-org/zellij/pull/493)
* Fix propagation of plugin ui request (https://github.com/zellij-org/zellij/pull/495)
* Handle pasted text properly (https://github.com/zellij-org/zellij/pull/494)
* Fix default keybinds for tab -> resize mode (https://github.com/zellij-org/zellij/pull/497)
* Terminal compatibility: device reports (https://github.com/zellij-org/zellij/pull/500)
* Forward unknown keys to the active terminal (https://github.com/zellij-org/zellij/pull/501)

## [0.9.0] - 2021-05-11
* Add more functionality to unbinding the default keybindings (https://github.com/zellij-org/zellij/pull/468)
* Terminal compatibility: fix support for CSI subparameters (https://github.com/zellij-org/zellij/pull/469)
* Move the sync command to tab mode (https://github.com/zellij-org/zellij/pull/412)
* Fix exit code of `dump-default-config` (https://github.com/zellij-org/zellij/pull/480)
* Feature: Switch tabs using `Alt + h/l` in normal mode if there are no panes in the direction (https://github.com/zellij-org/zellij/pull/471)
* Terminal Compatibility: various behaviour fixes (https://github.com/zellij-org/zellij/pull/486)
* Fix handling of `$HOME` `config` directory, especially relevant for darwin systems (https://github.com/zellij-org/zellij/pull/487)

## [0.8.0] - 2021-05-07
* Terminal compatibility: pass vttest 8 (https://github.com/zellij-org/zellij/pull/461)
* Add a Manpage (https://github.com/zellij-org/zellij/pull/455)
* Code infrastructure changes to support the upcoming session detach (https://github.com/zellij-org/zellij/pull/223)

## [0.7.0] - 2021-05-04
* Fix the tab '(Sync)' suffix in named tabs (https://github.com/zellij-org/zellij/pull/410)
* Improve performance when multiple panes are open (https://github.com/zellij-org/zellij/pull/318)
* Improve error reporting and tests of configuration (https://github.com/zellij-org/zellij/pull/423)
* Refactor install module to setup module (https://github.com/zellij-org/zellij/pull/431)
* Add theme support through xrdb (https://github.com/zellij-org/zellij/pull/239)
* Fix default keybindings in resize mode and add arrow parity in tab and scroll mode (https://github.com/zellij-org/zellij/pull/441)
* Terminal compatibility: pass vttest 2 and 3 (https://github.com/zellij-org/zellij/pull/447)
* Stabilize colors (https://github.com/zellij-org/zellij/pull/453)

## [0.6.0] - 2021-04-29
* Doesn't quit anymore on single `q` press while in tab mode  (https://github.com/zellij-org/zellij/pull/342)
* Completions are not assets anymore, but commands `option --generate-completion [shell]` (https://github.com/zellij-org/zellij/pull/369)
* Fixes in the default configuration `default.yaml` file. Adds initial tmux-compat keybindings `tmux.yaml` (https://github.com/zellij-org/zellij/pull/362)
* Added the `get_plugin_ids()` query function to the plugin API (https://github.com/zellij-org/zellij/pull/392)
* Implemented simple plugin timers via the `set_timeout()` call (https://github.com/zellij-org/zellij/pull/394)
* Added more configuration locations, changed `ZELLIJ_CONFIG` to `ZELLIJ_CONFIG_FILE` (https://github.com/zellij-org/zellij/pull/391)
* Improved keybind handling (https://github.com/zellij-org/zellij/pull/400)
* Added initial screen-compat keybinds `screen.yaml` (https://github.com/zellij-org/zellij/pull/399)
* Added the ability to synchronize input sent to panes (https://github.com/zellij-org/zellij/pull/395)
* Terminal fix: pass vttest 1 (https://github.com/zellij-org/zellij/pull/408)

## [0.5.1] - 2021-04-23
* Change config to flag (https://github.com/zellij-org/zellij/pull/300)
* Add ZELLIJ environment variable on startup (https://github.com/zellij-org/zellij/pull/305)
* Terminal fix: do not clear line if it's not there (https://github.com/zellij-org/zellij/pull/289)
* Do not allow opening new pane on the status bar (https://github.com/zellij-org/zellij/pull/314)
* Allow scrolling by full pages (https://github.com/zellij-org/zellij/pull/298)
* Reduce crate size by 4.8MB using `cargo diet`, to 77kB (https://github.com/zellij-org/zellij/pull/293)
* Draw UI properly when instantiated as the default terminal command (https://github.com/zellij-org/zellij/pull/323)
* Resolve ambiguous pane movements by their activity history (https://github.com/zellij-org/zellij/pull/294)

## [0.5.0] - 2021-04-20
Beta release with all the things
