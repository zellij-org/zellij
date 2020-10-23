use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use crate::tests::tty_inputs::{COL_10, COL_60, COL_14, COL_15, COL_19, COL_20, COL_24, COL_29, COL_30, COL_34, COL_39, COL_40, COL_50, COL_70, COL_90, COL_121};

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
    pub fn from_file_in_fixtures(file_name: &str) -> Self {
        let mut path_to_file = PathBuf::new();
        path_to_file.push("src");
        path_to_file.push("tests");
        path_to_file.push("fixtures");
        path_to_file.push(file_name);
        let content = fs::read(path_to_file).expect(&format!("could not read fixture {:?}", &file_name));
        Bytes {
            content,
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

pub fn get_possible_tty_inputs () -> HashMap<u16, Bytes> { // the key is the column count for this terminal input
    let mut possible_inputs = HashMap::new();
    let col_10_bytes = Bytes::new().content_from_str(&COL_10);
    let col_14_bytes = Bytes::new().content_from_str(&COL_14);
    let col_15_bytes = Bytes::new().content_from_str(&COL_15);
    let col_19_bytes = Bytes::new().content_from_str(&COL_19);
    let col_20_bytes = Bytes::new().content_from_str(&COL_20);
    let col_24_bytes = Bytes::new().content_from_str(&COL_24);
    let col_29_bytes = Bytes::new().content_from_str(&COL_29);
    let col_30_bytes = Bytes::new().content_from_str(&COL_30);
    let col_34_bytes = Bytes::new().content_from_str(&COL_34);
    let col_39_bytes = Bytes::new().content_from_str(&COL_39);
    let col_40_bytes = Bytes::new().content_from_str(&COL_40);
    let col_50_bytes = Bytes::new().content_from_str(&COL_50);
    let col_60_bytes = Bytes::new().content_from_str(&COL_60);
    let col_70_bytes = Bytes::new().content_from_str(&COL_70);
    let col_90_bytes = Bytes::new().content_from_str(&COL_90);
    let col_121_bytes = Bytes::new().content_from_str(&COL_121);
    possible_inputs.insert(10, col_10_bytes);
    possible_inputs.insert(14, col_14_bytes);
    possible_inputs.insert(15, col_15_bytes);
    possible_inputs.insert(19, col_19_bytes);
    possible_inputs.insert(20, col_20_bytes);
    possible_inputs.insert(24, col_24_bytes);
    possible_inputs.insert(29, col_29_bytes);
    possible_inputs.insert(30, col_30_bytes);
    possible_inputs.insert(34, col_34_bytes);
    possible_inputs.insert(39, col_39_bytes);
    possible_inputs.insert(40, col_40_bytes);
    possible_inputs.insert(50, col_50_bytes);
    possible_inputs.insert(60, col_60_bytes);
    possible_inputs.insert(70, col_70_bytes);
    possible_inputs.insert(90, col_90_bytes);
    possible_inputs.insert(121, col_121_bytes);
    possible_inputs
}
