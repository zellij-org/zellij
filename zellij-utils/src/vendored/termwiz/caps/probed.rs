use crate::vendored::termwiz::escape::csi::{Device, Window};
use crate::vendored::termwiz::escape::parser::Parser;
use crate::vendored::termwiz::escape::{Action, DeviceControlMode, Esc, EscCode, CSI};
use crate::vendored::termwiz::terminal::ScreenSize;
use crate::vendored::termwiz::Result;
use crate::vendored_termwiz_bail as bail;
use std::io::{Read, Write};

const TMUX_BEGIN: &str = "\u{1b}Ptmux;\u{1b}";
const TMUX_END: &str = "\u{1b}\\";

/// Represents a terminal name and version.
/// The name XtVersion is because this value is produced
/// by querying the terminal using the XTVERSION escape
/// sequence, which was defined by xterm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XtVersion(String);

impl XtVersion {
    /// Split the version string into a name component and a version
    /// component.  Currently it recognizes `Name(Version)` and
    /// `Name Version` forms. If a form is not recognized, returns None.
    pub fn name_and_version(&self) -> Option<(&str, &str)> {
        if self.0.ends_with(")") {
            let paren = self.0.find('(')?;
            Some((&self.0[0..paren], &self.0[paren + 1..self.0.len() - 1]))
        } else {
            let space = self.0.find(' ')?;
            Some((&self.0[0..space], &self.0[space + 1..]))
        }
    }

    /// Returns true if this represents tmux
    pub fn is_tmux(&self) -> bool {
        self.0.starts_with("tmux ")
    }

    /// Return the full underlying version string
    pub fn full_version(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_xtversion_name() {
        for (input, result) in [
            ("WezTerm something", Some(("WezTerm", "something"))),
            ("xterm(something)", Some(("xterm", "something"))),
            ("something-else", None),
        ] {
            let version = XtVersion(input.to_string());
            assert_eq!(version.name_and_version(), result, "{input}");
        }
    }
}

/// This struct is a helper that uses probing to determine specific capabilities
/// of the associated Terminal instance.
/// It will write and read data to and from the associated Terminal.
pub struct ProbeCapabilities<'a> {
    read: Box<&'a mut dyn Read>,
    write: Box<&'a mut dyn Write>,
}

impl<'a> ProbeCapabilities<'a> {
    pub fn new<R: Read, W: Write>(read: &'a mut R, write: &'a mut W) -> Self {
        Self {
            read: Box::new(read),
            write: Box::new(write),
        }
    }

    /// Probe for the XTVERSION response
    pub fn xt_version(&mut self) -> Result<XtVersion> {
        self.xt_version_impl(false)
    }

    /// Assuming that we are talking to tmux, probe for the XTVERSION response
    /// of its outer terminal.
    pub fn outer_xt_version(&mut self) -> Result<XtVersion> {
        self.xt_version_impl(true)
    }

    fn xt_version_impl(&mut self, tmux_escape: bool) -> Result<XtVersion> {
        let xt_version = CSI::Device(Box::new(Device::RequestTerminalNameAndVersion));
        let dev_attributes = CSI::Device(Box::new(Device::RequestPrimaryDeviceAttributes));

        if tmux_escape {
            write!(self.write, "{TMUX_BEGIN}{xt_version}{TMUX_END}")?;
            self.write.flush()?;
            std::thread::sleep(std::time::Duration::from_millis(100));
            write!(self.write, "{dev_attributes}")?;
        } else {
            write!(self.write, "{xt_version}{dev_attributes}")?;
        }
        self.write.flush()?;
        let mut term = vec![];
        let mut parser = Parser::new();
        let mut done = false;

        while !done {
            let mut byte = [0u8];
            self.read.read(&mut byte)?;

            parser.parse(&byte, |action| {
                // print!("{action:?}\r\n");
                match action {
                    Action::Esc(Esc::Code(EscCode::StringTerminator)) => {},
                    Action::DeviceControl(dev) => {
                        if let DeviceControlMode::Data(b) = dev {
                            term.push(b);
                        }
                    },
                    _ => {
                        done = true;
                    },
                }
            });
        }

        Ok(XtVersion(String::from_utf8_lossy(&term).into()))
    }

    /// Probe the terminal and determine the ScreenSize.
    pub fn screen_size(&mut self) -> Result<ScreenSize> {
        let xt_version = self.xt_version()?;

        let is_tmux = xt_version.is_tmux();

        // some tmux versions have their rows/cols swapped in ReportTextAreaSizeCells
        let swapped_cols_rows = match xt_version.full_version() {
            "tmux 3.2" | "tmux 3.2a" | "tmux 3.3" | "tmux 3.3a" => true,
            _ => false,
        };

        let query_cells = CSI::Window(Box::new(Window::ReportTextAreaSizeCells));
        let query_pixels = CSI::Window(Box::new(Window::ReportCellSizePixels));
        let dev_attributes = CSI::Device(Box::new(Device::RequestPrimaryDeviceAttributes));

        write!(self.write, "{query_cells}{query_pixels}")?;

        // tmux refuses to directly support responding to 14t or 16t queries
        // for pixel dimensions, so we need to jump through to the outer
        // terminal and see what it says
        if is_tmux {
            write!(self.write, "{TMUX_BEGIN}{query_pixels}{TMUX_END}")?;
        }

        if is_tmux || cfg!(windows) {
            self.write.flush()?;
            // I really wanted to avoid a delay here, but tmux and conpty will
            // both re-order the response to dev_attributes before sending the
            // response for the passthru of query_pixels if we don't delay.
            // The delay is potentially imperfect for things like a laggy ssh
            // connection. The consequence of the timing being wrong is that
            // we won't be able to reason about the pixel dimensions, which is
            // "OK", but that was kinda the whole point of probing this way
            // vs. termios.

            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        write!(self.write, "{dev_attributes}")?;
        self.write.flush()?;

        let mut parser = Parser::new();
        let mut done = false;
        let mut size = ScreenSize {
            rows: 0,
            cols: 0,
            xpixel: 0,
            ypixel: 0,
        };

        while !done {
            let mut byte = [0u8];
            self.read.read(&mut byte)?;

            parser.parse(&byte, |action| {
                // print!("{action:?}\r\n");
                match action {
                    // ConPTY appears to trigger 1 or more xtversion queries
                    // to wezterm in response to this probe, so we need to
                    // prepared to accept and discard data of that shape
                    // here, so that we keep going until we get our reports
                    Action::DeviceControl(_) => {},
                    Action::Esc(Esc::Code(EscCode::StringTerminator)) => {},

                    // and now look for the actual responses we're expecting
                    Action::CSI(csi) => match csi {
                        CSI::Window(win) => match *win {
                            Window::ResizeWindowCells { width, height } => {
                                let width = width.unwrap_or(1);
                                let height = height.unwrap_or(1);
                                if width > 0 && height > 0 {
                                    let width = width as usize;
                                    let height = height as usize;
                                    if swapped_cols_rows {
                                        size.rows = width;
                                        size.cols = height;
                                    } else {
                                        size.rows = height;
                                        size.cols = width;
                                    }
                                }
                            },
                            Window::ReportCellSizePixelsResponse { width, height } => {
                                let width = width.unwrap_or(1);
                                let height = height.unwrap_or(1);
                                if width > 0 && height > 0 {
                                    let width = width as usize;
                                    let height = height as usize;
                                    size.xpixel = width;
                                    size.ypixel = height;
                                }
                            },
                            _ => {
                                done = true;
                            },
                        },
                        _ => {
                            done = true;
                        },
                    },
                    _ => {
                        done = true;
                    },
                }
            });
        }

        if size.rows == 0 && size.cols == 0 {
            bail!("no size information available");
        }

        Ok(size)
    }
}
