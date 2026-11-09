use base64::alphabet::STANDARD as BASE64_STANDARD_ALPHABET;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use base64::engine::{DecodePaddingMode, Engine as _};

const BASE64_DECODER: GeneralPurpose = GeneralPurpose::new(
    &BASE64_STANDARD_ALPHABET,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

pub const MAX_DECODED_BYTES: usize = 104_857_600;
pub const MAX_DIMENSION: u32 = 10_000;
// the most base64 text that can decode to at most MAX_DECODED_BYTES
pub const MAX_PAYLOAD_B64_BYTES: usize = MAX_DECODED_BYTES.div_ceil(3) * 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KittyAction {
    Transmit,
    TransmitAndDisplay,
    Display,
    Delete,
    Query,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KittyFormat {
    Rgb24,
    Rgba32,
    Png,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KittyMedium {
    Direct,
    File,
    TempFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KittyErrorCode {
    Einval,
    Enoent,
    Ebadf,
    Enodata,
    Enotsupported,
}

impl KittyErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            KittyErrorCode::Einval => "EINVAL",
            KittyErrorCode::Enoent => "ENOENT",
            KittyErrorCode::Ebadf => "EBADF",
            KittyErrorCode::Enodata => "ENODATA",
            KittyErrorCode::Enotsupported => "ENOTSUPPORTED",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KittyError {
    pub code: KittyErrorCode,
    pub message: String,
    pub image_id: Option<u32>,
    pub image_number: Option<u32>,
    pub placement_id: Option<u32>,
    pub quiet: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedImage {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: KittyFormat,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KittyCommand {
    pub action: KittyAction,
    pub format: KittyFormat,
    pub medium: KittyMedium,
    pub image_id: Option<u32>,
    pub image_number: Option<u32>,
    pub placement_id: Option<u32>,
    pub pixel_width: Option<u32>,
    pub pixel_height: Option<u32>,
    pub compressed: bool,
    pub more_chunks: bool,
    pub data_size: Option<u32>,
    pub data_offset: Option<u32>,
    pub source_x: u32,
    pub source_y: u32,
    pub source_w: u32,
    pub source_h: u32,
    pub columns: u32,
    pub rows: u32,
    pub cell_offset_x: u32,
    pub cell_offset_y: u32,
    pub z_index: i32,
    pub quiet: u8,
    pub suppress_cursor_movement: bool,
    pub delete_specifier: Option<char>,
    pub image: Option<DecodedImage>,
}

impl Default for KittyCommand {
    fn default() -> Self {
        KittyCommand {
            action: KittyAction::Transmit,
            format: KittyFormat::Rgba32,
            medium: KittyMedium::Direct,
            image_id: None,
            image_number: None,
            placement_id: None,
            pixel_width: None,
            pixel_height: None,
            compressed: false,
            more_chunks: false,
            data_size: None,
            data_offset: None,
            source_x: 0,
            source_y: 0,
            source_w: 0,
            source_h: 0,
            columns: 0,
            rows: 0,
            cell_offset_x: 0,
            cell_offset_y: 0,
            z_index: 0,
            quiet: 0,
            suppress_cursor_movement: false,
            delete_specifier: None,
            image: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct EchoFields {
    image_id: Option<u32>,
    image_number: Option<u32>,
    placement_id: Option<u32>,
    quiet: u8,
}

impl EchoFields {
    fn from_command(command: &KittyCommand) -> Self {
        EchoFields {
            image_id: command.image_id,
            image_number: command.image_number,
            placement_id: command.placement_id,
            quiet: command.quiet,
        }
    }
    fn error(&self, code: KittyErrorCode, message: &str) -> KittyError {
        KittyError {
            code,
            message: message.to_owned(),
            image_id: self.image_id,
            image_number: self.image_number,
            placement_id: self.placement_id,
            quiet: self.quiet,
        }
    }
}

#[derive(Debug, Clone)]
pub struct KittyCommandParser {
    pending: Option<PendingUpload>,
}

#[derive(Debug, Clone)]
struct PendingUpload {
    command: KittyCommand,
    payload_b64: Vec<u8>,
    // set once the assembled payload outgrew MAX_PAYLOAD_B64_BYTES: the data
    // is dropped, the remaining chunks are consumed unread until the final
    // one and the transmission then fails
    overflowed: bool,
}

impl PendingUpload {
    fn append(&mut self, chunk: &[u8]) {
        if self.overflowed {
            return;
        }
        if self.payload_b64.len().saturating_add(chunk.len()) > MAX_PAYLOAD_B64_BYTES {
            self.overflowed = true;
            self.payload_b64 = Vec::new();
        } else {
            self.payload_b64.extend_from_slice(chunk);
        }
    }
}

impl KittyCommandParser {
    pub fn new() -> Self {
        KittyCommandParser { pending: None }
    }
    pub fn parse(&mut self, raw: &[u8]) -> Option<Result<KittyCommand, KittyError>> {
        let (control, payload) = split_control_payload(raw);
        if self.pending.is_some() {
            let pairs = match tokenize(control) {
                Ok(pairs) => pairs,
                Err(()) => {
                    let pending = self.pending.take().unwrap();
                    let echo = EchoFields::from_command(&pending.command);
                    return Some(Err(
                        echo.error(KittyErrorCode::Einval, "malformed control data")
                    ));
                },
            };
            let is_delete = pairs
                .iter()
                .any(|(key, value)| *key == b"a" && *value == b"d");
            if is_delete {
                self.pending = None;
            } else {
                let mut more_chunks = false;
                for (key, value) in &pairs {
                    if *key == b"m" {
                        match parse_u32(value) {
                            Some(0) => more_chunks = false,
                            Some(1) => more_chunks = true,
                            _ => {
                                let pending = self.pending.take().unwrap();
                                let echo = EchoFields::from_command(&pending.command);
                                return Some(Err(
                                    echo.error(KittyErrorCode::Einval, "malformed control data")
                                ));
                            },
                        }
                    }
                }
                for (key, value) in &pairs {
                    if *key == b"q" {
                        if let Some(q @ 0..=2) = parse_u32(value) {
                            self.pending.as_mut().unwrap().command.quiet = q as u8;
                        }
                    }
                }
                let pending = self.pending.as_mut().unwrap();
                pending.append(payload);
                if more_chunks {
                    return None;
                }
                let mut pending = self.pending.take().unwrap();
                pending.command.more_chunks = false;
                if pending.overflowed {
                    let echo = EchoFields::from_command(&pending.command);
                    return Some(Err(echo.error(KittyErrorCode::Einval, "image too large")));
                }
                return Some(run_payload_pipeline(pending.command, &pending.payload_b64));
            }
        }
        let command = match parse_control_data(control) {
            Ok(command) => command,
            Err(err) => return Some(Err(err)),
        };
        match command.action {
            KittyAction::Delete | KittyAction::Display => Some(Ok(command)),
            KittyAction::Transmit | KittyAction::TransmitAndDisplay | KittyAction::Query => {
                if command.more_chunks {
                    let mut pending = PendingUpload {
                        command,
                        payload_b64: Vec::new(),
                        overflowed: false,
                    };
                    pending.append(payload);
                    self.pending = Some(pending);
                    None
                } else {
                    Some(run_payload_pipeline(command, payload))
                }
            },
        }
    }
    pub fn abort_pending(&mut self) {
        self.pending = None;
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}

fn split_control_payload(raw: &[u8]) -> (&[u8], &[u8]) {
    match raw.iter().position(|b| *b == b';') {
        Some(pos) => (&raw[..pos], &raw[pos + 1..]),
        None => (raw, &raw[raw.len()..]),
    }
}

fn tokenize(control: &[u8]) -> Result<Vec<(&[u8], &[u8])>, ()> {
    let mut pairs = Vec::new();
    if control.is_empty() {
        return Ok(pairs);
    }
    for segment in control.split(|b| *b == b',') {
        if segment.is_empty() {
            continue;
        }
        let eq_pos = segment.iter().position(|b| *b == b'=').ok_or(())?;
        if eq_pos == 0 {
            return Err(());
        }
        pairs.push((&segment[..eq_pos], &segment[eq_pos + 1..]));
    }
    Ok(pairs)
}

fn parse_u32(value: &[u8]) -> Option<u32> {
    std::str::from_utf8(value).ok()?.parse::<u32>().ok()
}

fn parse_i32(value: &[u8]) -> Option<i32> {
    std::str::from_utf8(value).ok()?.parse::<i32>().ok()
}

fn scan_echo_fields(control: &[u8]) -> EchoFields {
    let mut echo = EchoFields::default();
    for segment in control.split(|b| *b == b',') {
        if let Some(eq_pos) = segment.iter().position(|b| *b == b'=') {
            let key = &segment[..eq_pos];
            let value = &segment[eq_pos + 1..];
            match key {
                b"q" => {
                    if let Some(q @ 0..=2) = parse_u32(value) {
                        echo.quiet = q as u8;
                    }
                },
                b"i" => echo.image_id = parse_u32(value).or(echo.image_id),
                b"I" => echo.image_number = parse_u32(value).or(echo.image_number),
                b"p" => echo.placement_id = parse_u32(value).or(echo.placement_id),
                _ => {},
            }
        }
    }
    echo
}

pub fn parse_control_data(control: &[u8]) -> Result<KittyCommand, KittyError> {
    let echo = scan_echo_fields(control);
    let pairs = tokenize(control)
        .map_err(|_| echo.error(KittyErrorCode::Einval, "malformed control data"))?;
    let mut command = KittyCommand::default();
    for (key, value) in pairs {
        match key {
            b"a" => {
                if value.len() != 1 {
                    return Err(echo.error(KittyErrorCode::Einval, "invalid action"));
                }
                command.action = match value[0] {
                    b't' => KittyAction::Transmit,
                    b'T' => KittyAction::TransmitAndDisplay,
                    b'p' => KittyAction::Display,
                    b'd' => KittyAction::Delete,
                    b'q' => KittyAction::Query,
                    b'f' | b'a' | b'c' => {
                        return Err(
                            echo.error(KittyErrorCode::Enotsupported, "animation is not supported")
                        );
                    },
                    _ => return Err(echo.error(KittyErrorCode::Einval, "invalid action")),
                };
            },
            b"f" => {
                command.format = match parse_u32(value) {
                    Some(24) => KittyFormat::Rgb24,
                    Some(32) => KittyFormat::Rgba32,
                    Some(100) => KittyFormat::Png,
                    _ => return Err(echo.error(KittyErrorCode::Einval, "invalid format")),
                };
            },
            b"t" => {
                if value.len() != 1 {
                    return Err(echo.error(KittyErrorCode::Einval, "invalid transmission medium"));
                }
                command.medium = match value[0] {
                    b'd' => KittyMedium::Direct,
                    b'f' => KittyMedium::File,
                    b't' => KittyMedium::TempFile,
                    b's' => {
                        return Err(echo.error(
                            KittyErrorCode::Enotsupported,
                            "shared memory transfer is not supported",
                        ));
                    },
                    _ => {
                        return Err(
                            echo.error(KittyErrorCode::Einval, "invalid transmission medium")
                        );
                    },
                };
            },
            b"s" => {
                command.pixel_width = Some(parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 's'")
                })?);
            },
            b"v" => {
                command.pixel_height = Some(parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'v'")
                })?);
            },
            b"o" => {
                if value != b"z" {
                    return Err(echo.error(KittyErrorCode::Einval, "invalid compression"));
                }
                command.compressed = true;
            },
            b"m" => {
                command.more_chunks = match parse_u32(value) {
                    Some(0) => false,
                    Some(1) => true,
                    _ => {
                        return Err(echo.error(KittyErrorCode::Einval, "invalid value for key 'm'"));
                    },
                };
            },
            b"S" => {
                command.data_size = Some(parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'S'")
                })?);
            },
            b"O" => {
                command.data_offset = Some(parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'O'")
                })?);
            },
            b"i" => {
                command.image_id = Some(parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'i'")
                })?);
            },
            b"I" => {
                command.image_number = Some(parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'I'")
                })?);
            },
            b"p" => {
                command.placement_id = Some(parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'p'")
                })?);
            },
            b"x" => {
                command.source_x = parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'x'")
                })?;
            },
            b"y" => {
                command.source_y = parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'y'")
                })?;
            },
            b"w" => {
                command.source_w = parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'w'")
                })?;
            },
            b"h" => {
                command.source_h = parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'h'")
                })?;
            },
            b"c" => {
                command.columns = parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'c'")
                })?;
            },
            b"r" => {
                command.rows = parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'r'")
                })?;
            },
            b"X" => {
                command.cell_offset_x = parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'X'")
                })?;
            },
            b"Y" => {
                command.cell_offset_y = parse_u32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'Y'")
                })?;
            },
            b"z" => {
                command.z_index = parse_i32(value).ok_or_else(|| {
                    echo.error(KittyErrorCode::Einval, "invalid value for key 'z'")
                })?;
            },
            b"q" => {
                command.quiet = match parse_u32(value) {
                    Some(q @ 0..=2) => q as u8,
                    _ => {
                        return Err(echo.error(KittyErrorCode::Einval, "invalid value for key 'q'"));
                    },
                };
            },
            b"C" => {
                command.suppress_cursor_movement = match parse_u32(value) {
                    Some(0) => false,
                    Some(1) => true,
                    _ => {
                        return Err(echo.error(KittyErrorCode::Einval, "invalid value for key 'C'"));
                    },
                };
            },
            b"d" => {
                if value.len() != 1 || !b"aAiInNcCpPqQxXyYzZrR".contains(&value[0]) {
                    return Err(echo.error(KittyErrorCode::Einval, "invalid delete specifier"));
                }
                command.delete_specifier = Some(value[0] as char);
            },
            b"U" => match parse_u32(value) {
                Some(0) => {},
                Some(_) => {
                    return Err(echo.error(
                        KittyErrorCode::Enotsupported,
                        "unicode placeholders are not supported",
                    ));
                },
                None => {
                    return Err(echo.error(KittyErrorCode::Einval, "invalid value for key 'U'"));
                },
            },
            b"N" | b"P" | b"Q" | b"H" | b"V" => {},
            _ => {},
        }
    }
    if command.image_id.unwrap_or(0) != 0 && command.image_number.is_some() {
        return Err(echo.error(
            KittyErrorCode::Einval,
            "must not specify both image id and image number",
        ));
    }
    Ok(command)
}

const TEMP_FILE_MARKER: &str = "tty-graphics-protocol";

struct OpenedGraphicsFile {
    file: std::fs::File,
    len: u64,
    canonical_path: PathBuf,
    // the directory the file was opened from and its name in it, both from
    // the walk that opened it: deletion goes through these, not the path
    #[cfg(unix)]
    parent_dir: std::os::fd::OwnedFd,
    #[cfg(unix)]
    file_name: std::ffi::CString,
}

fn known_temp_dirs() -> Vec<PathBuf> {
    [
        std::env::temp_dir(),
        PathBuf::from("/tmp"),
        PathBuf::from("/dev/shm"),
    ]
    .into_iter()
    .filter_map(|dir| std::fs::canonicalize(dir).ok())
    .collect()
}

// kitty refuses to read from these. /dev/shm is a regular tmpfs and one of the
// temp directories the protocol knows about, so it stays usable.
fn is_forbidden_path(canonical_path: &Path) -> bool {
    canonical_path.starts_with("/proc")
        || canonical_path.starts_with("/sys")
        || (canonical_path.starts_with("/dev") && !canonical_path.starts_with("/dev/shm"))
}

#[cfg(unix)]
fn openat_nofollow(
    directory: std::os::fd::RawFd,
    name: &std::ffi::CStr,
    flags: libc::c_int,
) -> std::io::Result<std::os::fd::OwnedFd> {
    use std::os::fd::FromRawFd;
    let fd = unsafe {
        libc::openat(
            directory,
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(fd) })
    }
}

// Opens an absolute path one component at a time, each with openat(2) and
// O_NOFOLLOW relative to the directory opened before it. The canonical path
// has no symlinks in it, so any that shows up here was swapped in after it was
// resolved and is refused, instead of being followed to somewhere else.
// O_NONBLOCK keeps a FIFO swapped in at the last moment from blocking the open.
// Returns the file, the directory it was found in and its name there.
#[cfg(unix)]
fn open_without_following_symlinks(
    path: &Path,
) -> std::io::Result<(std::fs::File, std::os::fd::OwnedFd, std::ffi::CString)> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Component;
    let invalid = || std::io::Error::from(std::io::ErrorKind::InvalidInput);
    if !path.is_absolute() {
        return Err(invalid());
    }
    let mut names = vec![];
    for component in path.components() {
        match component {
            Component::RootDir => {},
            Component::Normal(name) => {
                names.push(std::ffi::CString::new(name.as_bytes()).map_err(|_| invalid())?)
            },
            _ => return Err(invalid()),
        }
    }
    let file_name = names.pop().ok_or_else(invalid)?;
    let root = std::ffi::CString::new("/").map_err(|_| invalid())?;
    let mut directory = openat_nofollow(libc::AT_FDCWD, &root, libc::O_RDONLY | libc::O_DIRECTORY)?;
    for name in &names {
        directory = openat_nofollow(
            directory.as_raw_fd(),
            name,
            libc::O_RDONLY | libc::O_DIRECTORY,
        )?;
    }
    let file = openat_nofollow(
        directory.as_raw_fd(),
        &file_name,
        libc::O_RDONLY | libc::O_NONBLOCK,
    )?;
    Ok((std::fs::File::from(file), directory, file_name))
}

// Resolves `..` and symlinks first, then opens the resolved path once; the
// regular-file check is done on the open handle (fstat), not on the path, so
// the file that is checked is the file that is read.
fn open_graphics_file(path: &Path) -> std::io::Result<OpenedGraphicsFile> {
    let canonical_path = std::fs::canonicalize(path)?;
    if is_forbidden_path(&canonical_path) {
        return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
    }
    #[cfg(unix)]
    let (file, parent_dir, file_name) = open_without_following_symlinks(&canonical_path)?;
    #[cfg(not(unix))]
    let file = std::fs::OpenOptions::new()
        .read(true)
        .open(&canonical_path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    Ok(OpenedGraphicsFile {
        len: metadata.len(),
        file,
        canonical_path,
        #[cfg(unix)]
        parent_dir,
        #[cfg(unix)]
        file_name,
    })
}

// Per the kitty protocol a `t=t` file may only be deleted if it lives directly
// in a temp directory and its name contains the marker string.
fn should_delete_temp_file(opened: &OpenedGraphicsFile) -> bool {
    let path = &opened.canonical_path;
    let name_has_marker = path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().contains(TEMP_FILE_MARKER));
    let in_temp_dir = path
        .parent()
        .is_some_and(|parent| known_temp_dirs().iter().any(|dir| dir == parent));
    name_has_marker && in_temp_dir
}

// Deletes the file that was read, but only if its name still refers to it.
//
// On unix the deletion is relative to the directory handle from the walk that
// opened the file (unlinkat), after checking with fstatat that the name there
// still has the device and inode of the handle we read from. A symlink or a
// different file swapped in under that name is left alone, and the name cannot
// be redirected through a swapped parent directory.
//
// A window remains between the fstatat and the unlinkat: POSIX has no way to
// unlink "this inode" atomically. Using it requires renaming entries inside
// the directory the file lives in; in a sticky temp directory such as /tmp
// that is only the file's owner (or root), i.e. someone who could delete the
// file anyway.
fn delete_opened_file(opened: &OpenedGraphicsFile) {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let mut read_stat: libc::stat = unsafe { std::mem::zeroed() };
        let mut named_stat: libc::stat = unsafe { std::mem::zeroed() };
        let (read_ok, named_ok) = unsafe {
            (
                libc::fstat(opened.file.as_raw_fd(), &mut read_stat) == 0,
                libc::fstatat(
                    opened.parent_dir.as_raw_fd(),
                    opened.file_name.as_ptr(),
                    &mut named_stat,
                    libc::AT_SYMLINK_NOFOLLOW,
                ) == 0,
            )
        };
        let same_file = read_ok
            && named_ok
            && (named_stat.st_mode & libc::S_IFMT) == libc::S_IFREG
            && named_stat.st_dev == read_stat.st_dev
            && named_stat.st_ino == read_stat.st_ino;
        if same_file {
            unsafe {
                libc::unlinkat(opened.parent_dir.as_raw_fd(), opened.file_name.as_ptr(), 0);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = std::fs::remove_file(&opened.canonical_path);
    }
}

fn read_file_range(
    file: &mut std::fs::File,
    file_len: u64,
    command: &KittyCommand,
    echo: &EchoFields,
) -> Result<Vec<u8>, KittyError> {
    let start = command.data_offset.unwrap_or(0) as u64;
    if start > file_len {
        return Err(echo.error(KittyErrorCode::Ebadf, "could not read file"));
    }
    let available = file_len - start;
    let length = match command.data_size {
        Some(size) if size > 0 => std::cmp::min(size as u64, available),
        _ => available,
    };
    if length > MAX_DECODED_BYTES as u64 {
        return Err(echo.error(KittyErrorCode::Einval, "image too large"));
    }
    let mut read = || -> std::io::Result<Vec<u8>> {
        if start > 0 {
            file.seek(std::io::SeekFrom::Start(start))?;
        }
        let mut bytes = Vec::new();
        file.take(length).read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    read().map_err(|_| echo.error(KittyErrorCode::Ebadf, "could not read file"))
}

fn run_payload_pipeline(
    mut command: KittyCommand,
    payload_b64: &[u8],
) -> Result<KittyCommand, KittyError> {
    let echo = EchoFields::from_command(&command);
    if payload_b64.len() > MAX_PAYLOAD_B64_BYTES {
        return Err(echo.error(KittyErrorCode::Einval, "image too large"));
    }
    let trimmed_len = payload_b64
        .iter()
        .rposition(|b| *b != b'=')
        .map(|pos| pos + 1)
        .unwrap_or(0);
    let decoded = BASE64_DECODER
        .decode(&payload_b64[..trimmed_len])
        .map_err(|_| echo.error(KittyErrorCode::Einval, "invalid base64 payload"))?;
    let data = match command.medium {
        KittyMedium::Direct => decoded,
        KittyMedium::File | KittyMedium::TempFile => {
            let path_string = String::from_utf8(decoded)
                .map_err(|_| echo.error(KittyErrorCode::Ebadf, "invalid file path"))?;
            let path = std::path::PathBuf::from(path_string);
            let mut opened = open_graphics_file(&path)
                .map_err(|_| echo.error(KittyErrorCode::Ebadf, "could not read file"))?;
            let delete_after = command.medium == KittyMedium::TempFile;
            let read_result = read_file_range(&mut opened.file, opened.len, &command, &echo);
            if delete_after && read_result.is_ok() && should_delete_temp_file(&opened) {
                delete_opened_file(&opened);
            }
            read_result?
        },
    };
    let data = if command.compressed {
        let mut inflated = Vec::new();
        flate2::read::ZlibDecoder::new(&data[..])
            .take((MAX_DECODED_BYTES + 1) as u64)
            .read_to_end(&mut inflated)
            .map_err(|_| echo.error(KittyErrorCode::Einval, "could not inflate payload"))?;
        if inflated.len() > MAX_DECODED_BYTES {
            return Err(echo.error(KittyErrorCode::Einval, "image too large"));
        }
        inflated
    } else {
        data
    };
    let image = match command.format {
        KittyFormat::Rgb24 | KittyFormat::Rgba32 => {
            let bpp: u64 = if command.format == KittyFormat::Rgb24 {
                3
            } else {
                4
            };
            let width = command.pixel_width.unwrap_or(0);
            let height = command.pixel_height.unwrap_or(0);
            if width == 0 || height == 0 {
                return Err(echo.error(
                    KittyErrorCode::Einval,
                    "dimensions missing for raw image data",
                ));
            }
            if width > MAX_DIMENSION || height > MAX_DIMENSION {
                return Err(echo.error(KittyErrorCode::Einval, "image dimensions too large"));
            }
            let expected = width as u64 * height as u64 * bpp;
            if expected > MAX_DECODED_BYTES as u64 {
                return Err(echo.error(KittyErrorCode::Einval, "image too large"));
            }
            if (data.len() as u64) < expected {
                return Err(echo.error(
                    KittyErrorCode::Enodata,
                    "insufficient image data for dimensions",
                ));
            }
            let mut data = data;
            data.truncate(expected as usize);
            data.shrink_to_fit();
            DecodedImage {
                bytes: data,
                width,
                height,
                format: command.format,
            }
        },
        KittyFormat::Png => decode_png(&data, &echo)?,
    };
    command.image = Some(image);
    Ok(command)
}

fn decode_png(data: &[u8], echo: &EchoFields) -> Result<DecodedImage, KittyError> {
    let limits = png::Limits {
        bytes: MAX_DECODED_BYTES,
    };
    let mut decoder = png::Decoder::new_with_limits(std::io::Cursor::new(data), limits);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|_| echo.error(KittyErrorCode::Einval, "invalid png data"))?;
    let width = reader.info().width;
    let height = reader.info().height;
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(echo.error(KittyErrorCode::Einval, "image dimensions too large"));
    }
    if width as u64 * height as u64 * 4 > MAX_DECODED_BYTES as u64 {
        return Err(echo.error(KittyErrorCode::Einval, "image too large"));
    }
    let mut buf = vec![0; reader.output_buffer_size()];
    let out = reader
        .next_frame(&mut buf)
        .map_err(|_| echo.error(KittyErrorCode::Einval, "invalid png data"))?;
    buf.truncate(out.buffer_size());
    let rgba = match out.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity(buf.len() / 3 * 4);
            for pixel in buf.chunks_exact(3) {
                rgba.extend_from_slice(pixel);
                rgba.push(255);
            }
            rgba
        },
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity(buf.len() * 4);
            for g in buf {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
            rgba
        },
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity(buf.len() * 2);
            for pixel in buf.chunks_exact(2) {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
            rgba
        },
        _ => return Err(echo.error(KittyErrorCode::Einval, "invalid png data")),
    };
    if rgba.len() > MAX_DECODED_BYTES {
        return Err(echo.error(KittyErrorCode::Einval, "image too large"));
    }
    Ok(DecodedImage {
        bytes: rgba,
        width: out.width,
        height: out.height,
        format: KittyFormat::Rgba32,
    })
}

#[cfg(test)]
#[path = "./unit/parser_tests.rs"]
mod parser_tests;
