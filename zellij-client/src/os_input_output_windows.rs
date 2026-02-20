use crate::os_input_output::SignalEvent;

use async_trait::async_trait;

use std::io;

/// Windows async signal listener stub. Not yet implemented.
pub(crate) struct AsyncSignalListener;

impl AsyncSignalListener {
    pub fn new() -> io::Result<Self> {
        unimplemented!("Windows AsyncSignalListener not yet implemented")
    }
}

#[async_trait]
impl crate::os_input_output::AsyncSignals for AsyncSignalListener {
    async fn recv(&mut self) -> Option<SignalEvent> {
        unimplemented!("Windows AsyncSignalListener not yet implemented")
    }
}

/// Windows blocking signal iterator stub. Not yet implemented.
pub(crate) struct BlockingSignalIterator;

impl BlockingSignalIterator {
    pub fn new() -> io::Result<Self> {
        unimplemented!("Windows BlockingSignalIterator not yet implemented")
    }
}

impl Iterator for BlockingSignalIterator {
    type Item = SignalEvent;

    fn next(&mut self) -> Option<SignalEvent> {
        unimplemented!("Windows BlockingSignalIterator not yet implemented")
    }
}
