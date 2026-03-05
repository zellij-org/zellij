//! A Terminal buffered with a Surface

use crate::vendored::termwiz::surface::{SequenceNo, Surface};
use crate::vendored::termwiz::terminal::Terminal;
use crate::vendored::termwiz::Result;
use std::ops::{Deref, DerefMut};

/// `BufferedTerminal` is a convenience wrapper around both
/// a `Terminal` and a `Surface`.  It enables easier use of
/// the output optimization features available to `Surface`
/// and internally keeps track of the sequence number.
/// `BufferedTerminal` derefs to `Surface` and makes available
/// the surface API.
/// The `flush` method is used to compute the optimized set
/// of changes and actually render them to the underlying
/// `Terminal`.  No output will be visible until it is flushed!
pub struct BufferedTerminal<T: Terminal> {
    terminal: T,
    surface: Surface,
    seqno: SequenceNo,
}

impl<T: Terminal> BufferedTerminal<T> {
    /// Create a new `BufferedTerminal` with a `Surface` of
    /// a matching size.
    pub fn new(mut terminal: T) -> Result<Self> {
        let size = terminal.get_screen_size()?;
        let surface = Surface::new(size.cols, size.rows);
        Ok(Self {
            terminal,
            surface,
            seqno: 0,
        })
    }

    /// Get a mutable reference to the underlying terminal instance
    pub fn terminal(&mut self) -> &mut T {
        &mut self.terminal
    }

    /// Compute the set of changes needed to update the screen to
    /// match the current contents of the embedded `Surface` and
    /// send them to the `Terminal`.
    /// If some other process has output over the terminal screen,
    /// or other artifacts are present, this routine has no way to
    /// detect the lose of synchronization.
    /// Applications typically build in a refresh function (CTRL-L
    /// is common for unix applications) to request a repaint.
    /// You can use the `repaint` function for that situation.
    pub fn flush(&mut self) -> Result<()> {
        {
            let (seq, changes) = self.surface.get_changes(self.seqno);
            // If we encounter an error during rendering, we want to
            // reset the sequence number so that a subsequent paint
            // renders all.
            self.seqno = 0;
            self.terminal.render(&changes)?;
            //self.terminal.flush()?;
            self.seqno = seq;
        }
        self.surface.flush_changes_older_than(self.seqno);
        Ok(())
    }

    /// Clears the screen and re-draws the surface contents onto
    /// the Terminal.
    pub fn repaint(&mut self) -> Result<()> {
        self.seqno = 0;
        self.flush()
    }

    /// Check to see if the Terminal has been resized by its user.
    /// If it has, resize the surface to match the new dimensions
    /// and return true.  If the terminal was resized, the application
    /// will typically want to apply changes to match the new size
    /// and follow it up with a `flush` call to update the screen.
    ///
    /// Why isn't this automatic?  On Unix systems the SIGWINCH signal
    /// is used to indicate that a terminal size has changed.  This notification
    /// is completely out of band from the interactions with the underlying
    /// terminal device, and thus requires a function such as this one to
    /// be called after receipt of SIGWINCH, or just speculatively from time
    /// to time.
    ///
    /// Attaching signal handlers unilaterally from a library is undesirable,
    /// as the embedding application may have strong opinions about how
    /// best to do such a thing, so we do not automatically configure a
    /// signal handler.
    ///
    /// On Windows it is possible to receive notification about window
    /// resizing by processing input events.  Enabling those requires
    /// manipulating the input mode and establishing a handler to
    /// consume the input records.  Such a thing is possible, but is
    /// better suited for a higher level abstraction than this basic
    /// `BufferedTerminal` interface.
    pub fn check_for_resize(&mut self) -> Result<bool> {
        let size = self.terminal.get_screen_size()?;
        let (width, height) = self.surface.dimensions();

        if (width != size.cols) || (height != size.rows) {
            self.surface.resize(size.cols, size.rows);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl<T: Terminal> Deref for BufferedTerminal<T> {
    type Target = Surface;

    fn deref(&self) -> &Surface {
        &self.surface
    }
}

impl<T: Terminal> DerefMut for BufferedTerminal<T> {
    fn deref_mut(&mut self) -> &mut Surface {
        &mut self.surface
    }
}
