const MAX_CAPTURE_LEN: usize = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Ground,
    Esc,
    EscUnderscore,
    Capturing,
    CapturingEsc,
}

#[derive(Debug)]
pub struct KittyApcInterceptor {
    state: State,
    buffer: Vec<u8>,
    overflowed: bool,
}

pub struct ForwardBytes {
    bytes: [u8; 3],
    len: usize,
}

impl ForwardBytes {
    fn one(b0: u8) -> Self {
        ForwardBytes {
            bytes: [b0, 0, 0],
            len: 1,
        }
    }
    fn two(b0: u8, b1: u8) -> Self {
        ForwardBytes {
            bytes: [b0, b1, 0],
            len: 2,
        }
    }
    fn three(b0: u8, b1: u8, b2: u8) -> Self {
        ForwardBytes {
            bytes: [b0, b1, b2],
            len: 3,
        }
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

pub enum InterceptorResult {
    Forward(ForwardBytes),
    Swallow,
    Captured(Vec<u8>),
}

impl KittyApcInterceptor {
    pub fn new() -> Self {
        KittyApcInterceptor {
            state: State::Ground,
            buffer: Vec::new(),
            overflowed: false,
        }
    }
    pub fn reset(&mut self) {
        self.state = State::Ground;
        self.buffer.clear();
        self.overflowed = false;
    }
    pub fn advance(&mut self, byte: u8) -> InterceptorResult {
        match self.state {
            State::Ground => match byte {
                0x1b => {
                    self.state = State::Esc;
                    InterceptorResult::Swallow
                },
                b => InterceptorResult::Forward(ForwardBytes::one(b)),
            },
            State::Esc => match byte {
                0x5f => {
                    self.state = State::EscUnderscore;
                    InterceptorResult::Swallow
                },
                0x1b => InterceptorResult::Forward(ForwardBytes::one(0x1b)),
                b => {
                    self.state = State::Ground;
                    InterceptorResult::Forward(ForwardBytes::two(0x1b, b))
                },
            },
            State::EscUnderscore => match byte {
                0x47 => {
                    self.state = State::Capturing;
                    self.buffer.clear();
                    self.overflowed = false;
                    InterceptorResult::Swallow
                },
                0x1b => {
                    self.state = State::Esc;
                    InterceptorResult::Forward(ForwardBytes::two(0x1b, 0x5f))
                },
                b => {
                    self.state = State::Ground;
                    InterceptorResult::Forward(ForwardBytes::three(0x1b, 0x5f, b))
                },
            },
            State::Capturing => self.advance_capturing(byte),
            State::CapturingEsc => match byte {
                0x5c => {
                    self.state = State::Ground;
                    if self.overflowed {
                        self.overflowed = false;
                        self.buffer.clear();
                        InterceptorResult::Swallow
                    } else {
                        InterceptorResult::Captured(std::mem::take(&mut self.buffer))
                    }
                },
                b => {
                    self.push_capture_byte(0x1b);
                    self.state = State::Capturing;
                    self.advance_capturing(b)
                },
            },
        }
    }
    fn advance_capturing(&mut self, byte: u8) -> InterceptorResult {
        match byte {
            0x1b => {
                self.state = State::CapturingEsc;
                InterceptorResult::Swallow
            },
            0x9c => {
                self.state = State::Ground;
                if self.overflowed {
                    self.overflowed = false;
                    self.buffer.clear();
                    InterceptorResult::Swallow
                } else {
                    InterceptorResult::Captured(std::mem::take(&mut self.buffer))
                }
            },
            0x18 | 0x1a => {
                self.state = State::Ground;
                self.buffer.clear();
                self.overflowed = false;
                InterceptorResult::Forward(ForwardBytes::one(byte))
            },
            b => {
                self.push_capture_byte(b);
                InterceptorResult::Swallow
            },
        }
    }
    fn push_capture_byte(&mut self, byte: u8) {
        if self.overflowed {
            return;
        }
        if self.buffer.len() == MAX_CAPTURE_LEN {
            self.overflowed = true;
            self.buffer.clear();
            return;
        }
        self.buffer.push(byte);
    }
}

#[cfg(test)]
#[path = "./unit/interceptor_tests.rs"]
mod interceptor_tests;
