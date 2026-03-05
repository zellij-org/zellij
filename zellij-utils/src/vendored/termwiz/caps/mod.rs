//! # Terminal Capabilities
//!
//! There are a few problems in the world of terminal capability
//! detection today; the terminal is typically local to a machine
//! running very up to date software, but applications may be running
//! on a remote system with relatively stale system software, or
//! even different operating systems.
//!
//! There are two well-specified standard approaches to querying terminal
//! capabilities: `termcap` and `terminfo`.  The databases for the
//! capabilities are typically deployed with the operating system
//! software and suffers from splay and freshness issues: they may
//! be different on different systems and out of date.
//!
//! A further complication is that the terminal may be connected
//! via a series of intermediaries or multiplexers (mosh, tmux, screen)
//! which hides or perturbs the true capabilities of the terminal.
//!
//! `terminfo` and `termcap` both allow for the user to supply a local
//! override.  `terminfo` needs a locally available compiled database,
//! and that database may have different binary representations on disk
//! depending on the operating system software, making it difficult for
//! users that need to login to many remote machines; the burden is on
//! the user to configure a local profile on many machines or solve the
//! challenge of having a `$HOME/.terminfo` directory that is NFS mountable
//! and readable to all potential machines.
//!
//! `termcap` defines a `TERMCAP` environment variable that can contain
//! overrides and be passed via SSH to the remote systems much more simply
//! than the `terminfo` database, but `termcap` is the old obsolete database
//! and lacks a way to express support for the newer, more interesting
//! features.
//!
//! It's a bit of a mess.
//!
//! There's more: `slang` defined the concept of `COLORTERM` to workaround
//! some of these concerns so that it was possible to influence the
//! availability of relatively recent higher color palette extensions.
//! This allows the user the ability to "know better" about their terminal
//! than the local configuration allows.  `COLORTERM` has the advantage of
//! being able to be passed on to remote systems via SSH.
//!
//! Regarding environment variables: on macOS the two main terminal
//! emulators export `TERM_PROGRAM` and `TERM_PROGRAM_VERSION` into the
//! environment, and if this were transported via SSH and adopted by
//! more terminal emulators then newer software could potentially also
//! look at this information too.
//!
//! Is there or will there ever be an ideal solution to this stuff?
//! Probably not.
//!
//! With all this in mind, this module presents a `Capabilities` struct
//! that holds information about a terminal.   The `new_from_env` method
//! implements some heuristics (a fancy word for guessing) to compute
//! the terminal capabilities, but also offers a `ProbeHints`
//! that can be used by the embedding application to override those choices.
use crate::vendored::termwiz::Result;
use crate::vendored_termwiz_builder as builder;
use std::env::var;
use terminfo::{self, capability as cap};

pub mod probed;

/// Environment variable name indicating that color output should be disabled.
/// See <https://no-color.org>
const NO_COLOR_ENV: &str = "NO_COLOR";

builder! {
    /// Use the `ProbeHints` to configure an instance of
    /// the `ProbeHints` struct.  `ProbeHints` are passed to the `Capabilities`
    /// constructor to influence the effective set of terminal capabilities.
    #[derive(Debug, Default, Clone)]
    pub struct ProbeHints {
        /// The contents of the TERM environment variable
        term: Option<String>,

        /// The contents of the COLORTERM environment variable.
        /// <http://invisible-island.net/ncurses/ncurses-slang.html#env_COLORTERM>
        colorterm: Option<String>,

        /// The contents of the TERM_PROGRAM environment variable
        term_program: Option<String>,

        /// Override the choice of the number of colors
        color_level: Option<ColorLevel>,

        /// The contents of the TERM_PROGRAM_VERSION environment variable
        term_program_version: Option<String>,

        /// Definitively set whether hyperlinks are supported.
        /// The default is to assume yes as this is mostly harmless.
        hyperlinks: Option<bool>,

        /// Configure whether sixel graphics are supported.
        sixel: Option<bool>,

        /// Configure whether iTerm2 style graphics embedding is supported
        /// See <https://www.iterm2.com/documentation-images.html>
        iterm2_image: Option<bool>,

        /// Specify whether `bce`, background color erase, is supported.
        bce: Option<bool>,

        /// The contents of the COLORTERM_BCE environment variable
        /// <http://invisible-island.net/ncurses/ncurses-slang.html#env_COLORTERM_BCE>
        colorterm_bce: Option<String>,

        /// A loaded terminfo database entry
        terminfo_db: Option<terminfo::Database>,

        /// Whether bracketed paste mode is supported
        bracketed_paste: Option<bool>,

        /// Whether mouse support is present and should be used
        mouse_reporting: Option<bool>,

        /// When true, rather than using the terminfo `sgr` or `sgr0` entries,
        /// assume that the terminal is ANSI/ECMA-48 compliant for the
        /// common SGR attributes of bold, dim, reverse, underline, blink,
        /// invisible and reset, and directly emit those sequences.
        /// This can improve rendered text compatibility with pagers.
        force_terminfo_render_to_use_ansi_sgr: Option<bool>,
    }
}

impl ProbeHints {
    pub fn new_from_env() -> Self {
        let mut probe_hints = ProbeHints::default()
            .term(var("TERM").ok())
            .colorterm(var("COLORTERM").ok())
            .colorterm_bce(var("COLORTERM_BCE").ok())
            .term_program(var("TERM_PROGRAM").ok())
            .term_program_version(var("TERM_PROGRAM_VERSION").ok());

        if !std::env::var(NO_COLOR_ENV)
            .unwrap_or("".to_string())
            .is_empty()
        {
            probe_hints.color_level = Some(ColorLevel::MonoChrome);
        }

        probe_hints
    }
}

/// Describes the level of color support available
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorLevel {
    /// Basic ANSI colors; 8 colors + bright versions
    Sixteen,
    /// In addition to the ANSI 16 colors, this has 24 levels of grey
    /// and 216 colors typically 6x6x6 color cube with 5 bits.  There
    /// is some variance in implementations: the precise color cube is
    /// different in different emulators.
    TwoFiftySix,
    /// Commonly accepted as 24-bit RGB color.  The implementation may
    /// display these exactly as specified or it may match to an internal
    /// palette with fewer than the theoretical maximum 16 million colors.
    /// What we care about here is whether the terminal supports the escape
    /// sequence to specify RGB values rather than a palette index.
    TrueColor,
    /// Describes monochrome (black and white) color support.
    /// Enabled via NO_COLOR environment variable.
    MonoChrome,
}

/// `Capabilities` holds information about the capabilities of a terminal.
/// On POSIX systems this is largely derived from an available terminfo
/// database, but there are some newish capabilities that are not yet
/// described by the majority of terminfo installations and thus have some
/// additional handling in this struct.
#[derive(Debug, Clone)]
pub struct Capabilities {
    color_level: ColorLevel,
    hyperlinks: bool,
    sixel: bool,
    iterm2_image: bool,
    bce: bool,
    terminfo_db: Option<terminfo::Database>,
    bracketed_paste: bool,
    mouse_reporting: bool,
    force_terminfo_render_to_use_ansi_sgr: bool,
}

impl Capabilities {
    /// Detect the capabilities of the terminal and return the
    /// Capability object holding the outcome.
    /// This function inspects the environment variables to build
    /// up configuration hints.
    pub fn new_from_env() -> Result<Self> {
        Self::new_with_hints(ProbeHints::new_from_env())
    }

    /// Return modified capabilities with the assumption that we're
    /// using an xterm compatible terminal and the built-in xterm
    /// terminfo database.  This is used on Windows when the TERM
    /// is set to xterm-256color and we didn't find an equivalent
    /// terminfo on the local filesystem.  We're using this as a
    /// way to opt in to using terminal escapes rather than the
    /// legacy win32 console API.
    #[cfg(windows)]
    pub(crate) fn apply_builtin_terminfo(mut self) -> Self {
        let data = include_bytes!("xterm-256color");
        let db = terminfo::Database::from_buffer(data.as_ref()).unwrap();
        self.terminfo_db = Some(db);
        self.color_level = ColorLevel::TrueColor;
        self
    }

    /// Build a `Capabilities` object based on the provided `ProbeHints` object.
    pub fn new_with_hints(hints: ProbeHints) -> Result<Self> {
        let terminfo_db = hints.terminfo_db.as_ref().cloned();
        let terminfo_db = if cfg!(test) {
            // Don't load from the system terminfo in tests, as it is unpredictable
            terminfo_db
        } else {
            terminfo_db.or_else(|| match hints.term.as_ref() {
                Some(t) => terminfo::Database::from_name(t).ok(),
                None => terminfo::Database::from_env().ok(),
            })
        };

        let color_level = hints.color_level.unwrap_or_else(|| {
            // If set, COLORTERM overrides any other source of information
            match hints.colorterm.as_ref().map(String::as_ref) {
                Some("truecolor") | Some("24bit") => ColorLevel::TrueColor,
                Some(_) => ColorLevel::TwoFiftySix,
                _ => {
                    // COLORTERM isn't set, so look at the terminfo.
                    if let Some(ref db) = terminfo_db.as_ref() {
                        let has_true_color = db
                            .get::<cap::TrueColor>()
                            .unwrap_or(cap::TrueColor(false))
                            .0;
                        if has_true_color {
                            ColorLevel::TrueColor
                        } else if let Some(cap::MaxColors(n)) = db.get::<cap::MaxColors>() {
                            if n >= 16777216 {
                                ColorLevel::TrueColor
                            } else if n >= 256 {
                                ColorLevel::TwoFiftySix
                            } else {
                                ColorLevel::Sixteen
                            }
                        } else {
                            ColorLevel::Sixteen
                        }
                    } else if let Some(ref term) = hints.term {
                        // if we don't have TERMINFO, use a somewhat awful
                        // substring test against the TERM name.
                        if term.contains("256color") {
                            ColorLevel::TwoFiftySix
                        } else {
                            ColorLevel::Sixteen
                        }
                    } else {
                        ColorLevel::Sixteen
                    }
                },
            }
        });

        // I don't know of a way to detect SIXEL support, so we
        // assume no by default.
        let sixel = hints.sixel.unwrap_or(false);

        // The use of OSC 8 for hyperlinks means that it is generally
        // safe to assume yes: if the terminal doesn't support it,
        // the text will look "OK", although some versions of VTE based
        // terminals had a bug where it look like garbage.
        let hyperlinks = hints.hyperlinks.unwrap_or(true);

        let bce = hints.bce.unwrap_or_else(|| {
            // Use the COLORTERM_BCE variable to override any terminfo
            match hints.colorterm_bce.as_ref().map(String::as_ref) {
                Some("1") => true,
                _ => {
                    // Look it up from terminfo
                    terminfo_db
                        .as_ref()
                        .map(|db| {
                            db.get::<cap::BackColorErase>()
                                .unwrap_or(cap::BackColorErase(false))
                                .0
                        })
                        .unwrap_or(false)
                },
            }
        });

        let iterm2_image = hints.iterm2_image.unwrap_or_else(|| {
            match hints.term_program.as_ref().map(String::as_ref) {
                Some("iTerm.app") => {
                    // We're testing whether it has animated gif support
                    // here because the iTerm2 docs don't say when the
                    // image protocol was first implemented, but do mention
                    // the gif version.
                    version_ge(
                        hints
                            .term_program_version
                            .as_ref()
                            .unwrap_or(&"0.0.0".to_owned()),
                        "2.9.20150512",
                    )
                },
                Some("WezTerm") => true,
                _ => false,
            }
        });

        let bracketed_paste = hints.bracketed_paste.unwrap_or(true);
        let mouse_reporting = hints.mouse_reporting.unwrap_or(true);

        let force_terminfo_render_to_use_ansi_sgr =
            hints.force_terminfo_render_to_use_ansi_sgr.unwrap_or(false);

        Ok(Self {
            color_level,
            sixel,
            hyperlinks,
            iterm2_image,
            bce,
            terminfo_db,
            bracketed_paste,
            mouse_reporting,
            force_terminfo_render_to_use_ansi_sgr,
        })
    }

    /// Indicates how many colors are supported
    pub fn color_level(&self) -> ColorLevel {
        self.color_level
    }

    /// Does the terminal support SIXEL graphics?
    pub fn sixel(&self) -> bool {
        self.sixel
    }

    /// Does the terminal support hyperlinks?
    /// See <https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda>
    pub fn hyperlinks(&self) -> bool {
        self.hyperlinks
    }

    /// Does the terminal support the iTerm2 image protocol?
    /// See <https://www.iterm2.com/documentation-images.html>
    pub fn iterm2_image(&self) -> bool {
        self.iterm2_image
    }

    /// Is `bce`, background color erase supported?
    /// <http://invisible-island.net/ncurses/ncurses-slang.html#env_COLORTERM_BCE>
    pub fn bce(&self) -> bool {
        self.bce
    }

    /// Returns a reference to the loaded terminfo, if any.
    pub fn terminfo_db(&self) -> Option<&terminfo::Database> {
        self.terminfo_db.as_ref()
    }

    /// Whether bracketed paste is supported
    pub fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    /// Whether mouse reporting is supported
    pub fn mouse_reporting(&self) -> bool {
        self.mouse_reporting
    }

    /// Whether to emit standard ANSI/ECMA-48 codes, overriding any
    /// SGR terminfo capabilities.
    pub fn force_terminfo_render_to_use_ansi_sgr(&self) -> bool {
        self.force_terminfo_render_to_use_ansi_sgr
    }
}

/// Returns true if the version string `a` is >= `b`
fn version_ge(a: &str, b: &str) -> bool {
    let mut a = a.split('.');
    let mut b = b.split('.');

    loop {
        match (a.next(), b.next()) {
            (Some(a), Some(b)) => match (a.parse::<u64>(), b.parse::<u64>()) {
                (Ok(a), Ok(b)) => {
                    if a > b {
                        return true;
                    }
                    if a < b {
                        return false;
                    }
                },
                _ => {
                    if a > b {
                        return true;
                    }
                    if a < b {
                        return false;
                    }
                },
            },
            (Some(_), None) => {
                // A is greater
                return true;
            },
            (None, Some(_)) => {
                // A is smaller
                return false;
            },
            (None, None) => {
                // Equal
                return true;
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn version_cmp() {
        assert!(version_ge("1", "0"));
        assert!(version_ge("1.0", "0"));
        assert!(!version_ge("0", "1"));
        assert!(version_ge("3.2", "2.9"));
        assert!(version_ge("3.2.0beta5", "2.9"));
        assert!(version_ge("3.2.0beta5", "3.2.0"));
        assert!(version_ge("3.2.0beta5", "3.2.0beta1"));
    }

    fn load_terminfo() -> terminfo::Database {
        // Load our own compiled data so that the tests have an
        // environment that doesn't vary machine by machine.
        let data = include_bytes!("xterm-256color");
        terminfo::Database::from_buffer(data.as_ref()).unwrap()
    }

    #[test]
    fn empty_hint() {
        let caps = Capabilities::new_with_hints(ProbeHints::default()).unwrap();

        assert_eq!(caps.color_level(), ColorLevel::Sixteen);
        assert_eq!(caps.sixel(), false);
        assert_eq!(caps.hyperlinks(), true);
        assert_eq!(caps.iterm2_image(), false);
        assert_eq!(caps.bce(), false);
    }

    #[test]
    fn bce() {
        let caps =
            Capabilities::new_with_hints(ProbeHints::default().colorterm_bce(Some("1".into())))
                .unwrap();

        assert_eq!(caps.bce(), true);
    }

    #[test]
    fn bce_terminfo() {
        let caps =
            Capabilities::new_with_hints(ProbeHints::default().terminfo_db(Some(load_terminfo())))
                .unwrap();

        assert_eq!(caps.bce(), true);
    }

    #[test]
    fn terminfo_color() {
        let caps =
            Capabilities::new_with_hints(ProbeHints::default().terminfo_db(Some(load_terminfo())))
                .unwrap();

        assert_eq!(caps.color_level(), ColorLevel::TrueColor);
    }

    #[test]
    fn term_but_not_colorterm() {
        let caps =
            Capabilities::new_with_hints(ProbeHints::default().term(Some("xterm-256color".into())))
                .unwrap();

        assert_eq!(caps.color_level(), ColorLevel::TwoFiftySix);
    }

    #[test]
    fn colorterm_but_no_term() {
        let caps =
            Capabilities::new_with_hints(ProbeHints::default().colorterm(Some("24bit".into())))
                .unwrap();

        assert_eq!(caps.color_level(), ColorLevel::TrueColor);
    }

    #[test]
    fn term_and_colorterm() {
        let caps = Capabilities::new_with_hints(
            ProbeHints::default()
                .term(Some("xterm-256color".into()))
                // bogus value
                .colorterm(Some("24bot".into())),
        )
        .unwrap();

        assert_eq!(caps.color_level(), ColorLevel::TwoFiftySix);

        let caps = Capabilities::new_with_hints(
            ProbeHints::default()
                .term(Some("xterm-256color".into()))
                .colorterm(Some("24bit".into())),
        )
        .unwrap();

        assert_eq!(caps.color_level(), ColorLevel::TrueColor);

        let caps = Capabilities::new_with_hints(
            ProbeHints::default()
                .term(Some("xterm-256color".into()))
                .colorterm(Some("truecolor".into())),
        )
        .unwrap();

        assert_eq!(caps.color_level(), ColorLevel::TrueColor);
    }

    #[test]
    fn iterm2_image() {
        let caps = Capabilities::new_with_hints(
            ProbeHints::default()
                .term_program(Some("iTerm.app".into()))
                .term_program_version(Some("1.0.0".into())),
        )
        .unwrap();
        assert_eq!(caps.iterm2_image(), false);

        let caps = Capabilities::new_with_hints(
            ProbeHints::default()
                .term_program(Some("iTerm.app".into()))
                .term_program_version(Some("2.9.0".into())),
        )
        .unwrap();
        assert_eq!(caps.iterm2_image(), false);

        let caps = Capabilities::new_with_hints(
            ProbeHints::default()
                .term_program(Some("iTerm.app".into()))
                .term_program_version(Some("2.9.20150512".into())),
        )
        .unwrap();
        assert_eq!(caps.iterm2_image(), true);

        let caps = Capabilities::new_with_hints(
            ProbeHints::default()
                .term_program(Some("iTerm.app".into()))
                .term_program_version(Some("3.2.0beta5".into())),
        )
        .unwrap();
        assert_eq!(caps.iterm2_image(), true);
    }
}
