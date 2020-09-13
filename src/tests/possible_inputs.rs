use std::collections::HashMap;
use crate::tests::tty_inputs::{COL_121, COL_60};

#[derive(Clone)]
pub struct Bytes {
    pub content: Vec<u8>,
    pub read_position: usize,
}

impl Bytes {
    pub fn new() -> Self {
        Bytes {
            content: vec![],
            read_position: 0
        }
    }
    pub fn content(mut self, content: Vec<u8>) -> Self {
        self.content = content;
        self
    }
    pub fn content_from_str(mut self, content: &[&'static str]) -> Self {
        let mut content_as_bytes = vec![];
        for line in content {
            for char in line.chars() {
                content_as_bytes.push(char as u8);
            }
        }
        self.content = content_as_bytes;
        self
    }
    pub fn set_read_position(&mut self, read_position: usize) {
        self.read_position = read_position;
    }
}

pub fn get_possible_inputs () -> HashMap<u16, Bytes> { // the key is the column count for this terminal input
    let mut possible_inputs = HashMap::new();
    let col_60_bytes = Bytes::new().content_from_str(&COL_60);
    let col_121_bytes = Bytes::new().content_from_str(&COL_121);
    possible_inputs.insert(121, col_121_bytes);
    possible_inputs.insert(60, col_60_bytes);
    possible_inputs
}
