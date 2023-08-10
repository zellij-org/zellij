pub use super::generated_api::api::file::File as ProtobufFile;
use crate::data::FileToOpen;

use std::convert::TryFrom;
use std::path::PathBuf;

impl TryFrom<ProtobufFile> for FileToOpen {
    type Error = &'static str;
    fn try_from(protobuf_file: ProtobufFile) -> Result<Self, &'static str> {
        let path = PathBuf::from(protobuf_file.path);
        let line_number = protobuf_file.line_number.map(|l| l as usize);
        let cwd = protobuf_file.cwd.map(|c| PathBuf::from(c));
        Ok(FileToOpen {
            path,
            line_number,
            cwd,
        })
    }
}

impl TryFrom<FileToOpen> for ProtobufFile {
    type Error = &'static str;
    fn try_from(file_to_open: FileToOpen) -> Result<Self, &'static str> {
        Ok(ProtobufFile {
            path: file_to_open.path.display().to_string(),
            line_number: file_to_open.line_number.map(|l| l as i32),
            cwd: file_to_open.cwd.map(|c| c.display().to_string()),
        })
    }
}
