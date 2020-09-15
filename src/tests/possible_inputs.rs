use std::collections::HashMap;
use crate::tests::tty_inputs::{COL_121, COL_60, COL_19, COL_29, COL_30, COL_40, COL_50, COL_70};

#[derive(Clone, Debug)]
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
    let col_19_bytes = Bytes::new().content_from_str(&COL_19);
    let col_29_bytes = Bytes::new().content_from_str(&COL_29);
    let col_30_bytes = Bytes::new().content_from_str(&COL_30);
    let col_40_bytes = Bytes::new().content_from_str(&COL_40);
    let col_50_bytes = Bytes::new().content_from_str(&COL_50);
    let col_60_bytes = Bytes::new().content_from_str(&COL_60);
    let col_70_bytes = Bytes::new().content_from_str(&COL_70);
    let col_121_bytes = Bytes::new().content_from_str(&COL_121);
    possible_inputs.insert(121, col_121_bytes);
    possible_inputs.insert(19, col_19_bytes);
    possible_inputs.insert(29, col_29_bytes);
    possible_inputs.insert(30, col_30_bytes);
    possible_inputs.insert(40, col_40_bytes);
    possible_inputs.insert(50, col_50_bytes);
    possible_inputs.insert(60, col_60_bytes);
    possible_inputs.insert(70, col_70_bytes);
    possible_inputs
}
