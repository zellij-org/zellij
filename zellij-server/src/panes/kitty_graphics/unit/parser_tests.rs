use super::*;
use base64::engine::general_purpose::STANDARD as BASE64_ENCODER;
use std::io::{Seek, Write};
use std::path::PathBuf;

const PNG_2X2_RGBA: [u8; 75] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0x72, 0xb6, 0x0d,
    0x24, 0x00, 0x00, 0x00, 0x12, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x0c, 0x81, 0x34, 0x18, 0x00, 0x00, 0x49, 0xc8, 0x09, 0xf7, 0x03, 0xd9, 0x64, 0xf1, 0x00,
    0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];
const PNG_2X2_EXPECTED_RGBA: [u8; 16] = [
    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
];

fn b64(data: &[u8]) -> String {
    BASE64_ENCODER.encode(data)
}

fn raw_cmd(control: &str, payload: &[u8]) -> Vec<u8> {
    format!("{};{}", control, b64(payload)).into_bytes()
}

fn parse_one(parser: &mut KittyCommandParser, raw: &[u8]) -> Result<KittyCommand, KittyError> {
    parser
        .parse(raw)
        .expect("expected a completed parse result")
}

fn raster_12_bytes() -> Vec<u8> {
    (1..=12).collect()
}

#[test]
fn yazi_first_chunk_control_string_parses_to_expected_fields() {
    let command = parse_control_data(b"q=2,a=T,z=-1,C=1,f=24,s=2,v=2,m=1").unwrap();
    assert_eq!(command.quiet, 2);
    assert_eq!(command.action, KittyAction::TransmitAndDisplay);
    assert_eq!(command.z_index, -1);
    assert!(command.suppress_cursor_movement);
    assert_eq!(command.format, KittyFormat::Rgb24);
    assert_eq!(command.pixel_width, Some(2));
    assert_eq!(command.pixel_height, Some(2));
    assert!(command.more_chunks);
    assert_eq!(command.medium, KittyMedium::Direct);
    assert_eq!(command.image_id, None);
    assert_eq!(command.image_number, None);
    assert_eq!(command.placement_id, None);
    assert!(!command.compressed);
    assert_eq!(command.delete_specifier, None);
    assert_eq!(command.image, None);
    assert_eq!(command.source_x, 0);
    assert_eq!(command.source_y, 0);
    assert_eq!(command.source_w, 0);
    assert_eq!(command.source_h, 0);
    assert_eq!(command.columns, 0);
    assert_eq!(command.rows, 0);
    assert_eq!(command.cell_offset_x, 0);
    assert_eq!(command.cell_offset_y, 0);
}

#[test]
fn three_chunk_upload_concatenates_payloads_and_completes_on_m0() {
    let raster = raster_12_bytes();
    let full_b64 = b64(&raster);
    assert_eq!(full_b64.len(), 16);
    let chunk_one = &full_b64[..8];
    let chunk_two = &full_b64[8..12];
    let chunk_three = &full_b64[12..];
    let mut parser = KittyCommandParser::new();
    assert!(parser
        .parse(format!("a=T,f=24,s=2,v=2,i=7,m=1;{}", chunk_one).as_bytes())
        .is_none());
    assert!(parser
        .parse(format!("m=1;{}", chunk_two).as_bytes())
        .is_none());
    let command = parse_one(&mut parser, format!("m=0;{}", chunk_three).as_bytes()).unwrap();
    let image = command.image.unwrap();
    assert_eq!(image.bytes, raster);
    assert_eq!(image.width, 2);
    assert_eq!(image.height, 2);
    assert!(!command.more_chunks);
    assert_eq!(command.image_id, Some(7));
    assert!(!parser.has_pending());
}

#[test]
fn delete_between_chunks_aborts_upload_and_returns_the_delete() {
    let raster = raster_12_bytes();
    let full_b64 = b64(&raster);
    let chunk_one = &full_b64[..8];
    let chunk_rest = &full_b64[8..];
    let mut parser = KittyCommandParser::new();
    assert!(parser
        .parse(format!("a=T,f=24,s=2,v=2,m=1;{}", chunk_one).as_bytes())
        .is_none());
    assert!(parser.has_pending());
    let delete = parse_one(&mut parser, b"a=d,d=A").unwrap();
    assert_eq!(delete.action, KittyAction::Delete);
    assert_eq!(delete.delete_specifier, Some('A'));
    assert!(!parser.has_pending());
    let err = parse_one(&mut parser, format!("m=0;{}", chunk_rest).as_bytes()).unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Einval);
}

#[test]
fn zlib_compressed_raster_inflates_to_original_bytes() {
    let raster = raster_12_bytes();
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&raster).unwrap();
    let compressed = encoder.finish().unwrap();
    let mut parser = KittyCommandParser::new();
    let command = parse_one(&mut parser, &raw_cmd("a=T,f=24,s=2,v=2,o=z", &compressed)).unwrap();
    let image = command.image.unwrap();
    assert_eq!(image.bytes, raster);
    assert_eq!(image.format, KittyFormat::Rgb24);
}

#[test]
fn embedded_png_fixture_decodes_to_expected_rgba() {
    let mut parser = KittyCommandParser::new();
    let command = parse_one(&mut parser, &raw_cmd("a=T,f=100", &PNG_2X2_RGBA)).unwrap();
    assert_eq!(
        command.image,
        Some(DecodedImage {
            bytes: PNG_2X2_EXPECTED_RGBA.to_vec(),
            width: 2,
            height: 2,
            format: KittyFormat::Rgba32,
        })
    );
}

#[test]
fn non_png_payload_with_png_format_is_einval() {
    let mut parser = KittyCommandParser::new();
    let err = parse_one(&mut parser, &raw_cmd("a=T,f=100", b"this is not a png")).unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Einval);
}

#[test]
fn raw_payload_with_insufficient_bytes_is_enodata() {
    let mut parser = KittyCommandParser::new();
    let payload: Vec<u8> = (1..=11).collect();
    let err = parse_one(&mut parser, &raw_cmd("a=T,f=24,s=2,v=2", &payload)).unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Enodata);
}

#[test]
fn raw_payload_with_excess_bytes_is_truncated() {
    let mut parser = KittyCommandParser::new();
    let payload: Vec<u8> = (1..=20).collect();
    let command = parse_one(&mut parser, &raw_cmd("a=T,f=24,s=2,v=2", &payload)).unwrap();
    let image = command.image.unwrap();
    assert_eq!(image.bytes.len(), 12);
    assert_eq!(image.bytes, (1..=12).collect::<Vec<u8>>());
}

#[test]
fn raw_format_without_dimensions_is_einval() {
    let mut parser = KittyCommandParser::new();
    let err = parse_one(&mut parser, &raw_cmd("a=T,f=24", &raster_12_bytes())).unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Einval);
}

#[test]
fn icat_direct_probe_with_data_size_parses_ok() {
    let mut parser = KittyCommandParser::new();
    let command = parse_one(
        &mut parser,
        &raw_cmd("t=d,a=q,i=1,s=1,v=1,f=24,S=3", b"123"),
    )
    .unwrap();
    assert_eq!(command.action, KittyAction::Query);
    assert_eq!(command.image_id, Some(1));
    assert_eq!(command.data_size, Some(3));
    assert_eq!(command.image.unwrap().bytes, b"123".to_vec());
}

#[test]
fn data_offset_and_size_window_file_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("windowed.bin");
    let bytes: Vec<u8> = (1..=10).collect();
    std::fs::write(&path, &bytes).unwrap();
    let mut parser = KittyCommandParser::new();
    let err = parse_one(
        &mut parser,
        &raw_cmd(
            "a=T,t=f,f=24,s=1,v=1,O=4,S=3",
            path.to_str().unwrap().as_bytes(),
        ),
    )
    .unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Enotsupported);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn unknown_key_is_silently_ignored() {
    let mut parser = KittyCommandParser::new();
    let command = parse_one(&mut parser, b"a=T,i=5,q=1,Z=9,f=24,s=1,v=1;AAAA").unwrap();
    assert_eq!(command.image_id, Some(5));
    assert_eq!(command.quiet, 1);
    assert_eq!(command.image.unwrap().width, 1);
}

#[test]
fn known_but_unsupported_keys_do_not_break_valid_command() {
    let mut parser = KittyCommandParser::new();
    let command = parse_one(
        &mut parser,
        b"a=T,i=5,N=1,P=2,Q=3,H=4,V=5,f=24,s=1,v=1;AAAA",
    )
    .unwrap();
    assert_eq!(command.image_id, Some(5));
    assert!(command.image.is_some());
}

#[test]
fn unicode_placeholder_key_is_enotsupported() {
    let mut parser = KittyCommandParser::new();
    let err = parse_one(&mut parser, b"a=T,U=1,f=24,s=1,v=1;AAAA").unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Enotsupported);
}

#[test]
fn shared_memory_medium_is_enotsupported() {
    let mut parser = KittyCommandParser::new();
    let err = parse_one(&mut parser, &raw_cmd("a=T,t=s,s=1,v=1", b"/x")).unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Enotsupported);
}

#[test]
fn animation_actions_are_enotsupported() {
    for control in [b"a=f".as_slice(), b"a=a".as_slice(), b"a=c".as_slice()] {
        let err = parse_control_data(control).unwrap_err();
        assert_eq!(err.code, KittyErrorCode::Enotsupported);
    }
}

#[test]
fn oversized_dimensions_are_einval() {
    let mut parser = KittyCommandParser::new();
    let err = parse_one(&mut parser, b"a=T,f=24,s=10001,v=1;AAAA").unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Einval);
    let err = parse_one(&mut parser, b"a=T,f=32,s=10000,v=10000;AAAA").unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Einval);
}

#[test]
fn file_medium_outside_temp_proc_sys_and_dev_is_enotsupported() {
    let raster = raster_12_bytes();
    let dir = manifest_dir_tempdir();
    let path = dir.path().join("img.bin");
    std::fs::write(&path, &raster).unwrap();
    let canonical = std::fs::canonicalize(&path).unwrap();
    assert!(!canonical.starts_with("/tmp"));
    assert!(!canonical.starts_with("/proc"));
    assert!(!canonical.starts_with("/sys"));
    assert!(!canonical.starts_with("/dev"));
    let mut parser = KittyCommandParser::new();
    let err = parse_one(
        &mut parser,
        &raw_cmd("a=T,f=24,s=2,v=2,t=f", path.to_str().unwrap().as_bytes()),
    )
    .unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Enotsupported);
    assert!(path.exists());
    assert_eq!(std::fs::read(&path).unwrap(), raster);
}

#[test]
fn file_medium_nonexistent_path_is_ebadf() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("does-not-exist.bin");
    let mut parser = KittyCommandParser::new();
    let err = parse_one(
        &mut parser,
        &raw_cmd("a=T,f=24,s=2,v=2,t=f", path.to_str().unwrap().as_bytes()),
    )
    .unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Enotsupported);
    assert!(!path.exists());
}

// Creates a file directly in the system temp directory, which is where the
// protocol allows `t=t` files to be deleted from.
fn temp_dir_file(prefix: &str, contents: &[u8]) -> PathBuf {
    let mut file = tempfile::Builder::new()
        .prefix(prefix)
        .tempfile_in(std::env::temp_dir())
        .unwrap();
    file.write_all(contents).unwrap();
    let (_file, path) = file.keep().unwrap();
    path
}

fn manifest_dir_tempdir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("kitty-parser-test")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap()
}

fn transmit_temp(path: &Path) -> Result<KittyCommand, KittyError> {
    let mut parser = KittyCommandParser::new();
    parse_one(
        &mut parser,
        &raw_cmd("a=T,f=24,s=2,v=2,t=t", path.to_str().unwrap().as_bytes()),
    )
}

#[test]
fn temp_file_medium_inside_temp_dir_is_deleted_after_read() {
    let raster = raster_12_bytes();
    let path = temp_dir_file("tty-graphics-protocol-", &raster);
    let result = transmit_temp(&path);
    let existed = path.exists();
    let bytes = std::fs::read(&path).ok();
    let _ = std::fs::remove_file(&path);
    assert_eq!(result.unwrap_err().code, KittyErrorCode::Enotsupported);
    assert!(existed, "temp file with the protocol marker was deleted");
    assert_eq!(bytes.unwrap(), raster);
}

#[test]
fn temp_file_medium_in_temp_dir_without_marker_is_read_but_kept() {
    let raster = raster_12_bytes();
    let path = temp_dir_file("kitty-parser-test-", &raster);
    let result = transmit_temp(&path);
    let existed = path.exists();
    let bytes = std::fs::read(&path).ok();
    let _ = std::fs::remove_file(&path);
    assert_eq!(result.unwrap_err().code, KittyErrorCode::Enotsupported);
    assert!(existed);
    assert_eq!(bytes.unwrap(), raster);
}

#[test]
fn temp_file_medium_in_temp_subdirectory_is_not_deleted() {
    let raster = raster_12_bytes();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tty-graphics-protocol-img.bin");
    std::fs::write(&path, &raster).unwrap();
    let err = transmit_temp(&path).unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Enotsupported);
    assert!(path.exists());
    assert_eq!(std::fs::read(&path).unwrap(), raster);
}

#[test]
fn temp_file_medium_dotdot_traversal_out_of_temp_dir_is_not_deleted() {
    let raster = raster_12_bytes();
    let dir = manifest_dir_tempdir();
    let victim = dir.path().join("tty-graphics-protocol-victim.bin");
    std::fs::write(&victim, &raster).unwrap();
    // starts with the real temp dir, climbs out of it (extra `..` at the root
    // are no-ops) and comes back down to the victim
    let temp_dir = std::env::temp_dir();
    let mut traversal = temp_dir.clone();
    for _ in temp_dir.components() {
        traversal.push("..");
    }
    for component in victim.components() {
        if let std::path::Component::Normal(name) = component {
            traversal.push(name);
        }
    }
    assert!(traversal.starts_with(&temp_dir));
    let result = transmit_temp(&traversal);
    assert!(victim.exists(), "file outside the temp dir was deleted");
    assert_eq!(std::fs::read(&victim).unwrap(), raster);
    assert_eq!(result.unwrap_err().code, KittyErrorCode::Enotsupported);
}

#[cfg(unix)]
#[test]
fn temp_file_medium_symlink_to_outside_file_deletes_nothing() {
    let raster = raster_12_bytes();
    let dir = manifest_dir_tempdir();
    let victim = dir.path().join("victim.bin");
    std::fs::write(&victim, &raster).unwrap();
    let link =
        std::env::temp_dir().join(format!("tty-graphics-protocol-link-{}", std::process::id()));
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&victim, &link).unwrap();
    let result = transmit_temp(&link);
    let link_exists = std::fs::symlink_metadata(&link).is_ok();
    let _ = std::fs::remove_file(&link);
    assert!(victim.exists(), "symlink target was deleted");
    assert_eq!(std::fs::read(&victim).unwrap(), raster);
    assert!(
        link_exists,
        "symlink was removed although target is not a temp file"
    );
    assert_eq!(result.unwrap_err().code, KittyErrorCode::Enotsupported);
}

#[cfg(unix)]
#[test]
fn temp_file_medium_through_symlinked_directory_is_not_deleted() {
    let raster = raster_12_bytes();
    let dir = manifest_dir_tempdir();
    let victim = dir.path().join("tty-graphics-protocol-victim.bin");
    std::fs::write(&victim, &raster).unwrap();
    let link_dir =
        std::env::temp_dir().join(format!("tty-graphics-protocol-dir-{}", std::process::id()));
    let _ = std::fs::remove_file(&link_dir);
    std::os::unix::fs::symlink(dir.path(), &link_dir).unwrap();
    let result = transmit_temp(&link_dir.join("tty-graphics-protocol-victim.bin"));
    let _ = std::fs::remove_file(&link_dir);
    assert!(
        victim.exists(),
        "file behind a symlinked directory was deleted"
    );
    assert_eq!(std::fs::read(&victim).unwrap(), raster);
    assert_eq!(result.unwrap_err().code, KittyErrorCode::Enotsupported);
}

#[test]
fn temp_file_medium_failed_read_does_not_delete() {
    let path = temp_dir_file("tty-graphics-protocol-", &raster_12_bytes());
    let mut parser = KittyCommandParser::new();
    let err = parse_one(
        &mut parser,
        &raw_cmd(
            "a=T,f=24,s=2,v=2,t=t,O=64",
            path.to_str().unwrap().as_bytes(),
        ),
    )
    .unwrap_err();
    let existed = path.exists();
    let bytes = std::fs::read(&path).ok();
    let _ = std::fs::remove_file(&path);
    assert_eq!(err.code, KittyErrorCode::Enotsupported);
    assert!(existed, "file was deleted although the read failed");
    assert_eq!(bytes.unwrap(), raster_12_bytes());
}

#[cfg(unix)]
#[test]
fn opening_refuses_a_symlinked_intermediate_directory() {
    let raster = raster_12_bytes();
    let dir = manifest_dir_tempdir();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let real = base.join("real");
    std::fs::create_dir(&real).unwrap();
    std::fs::write(real.join("img.bin"), &raster).unwrap();
    // a directory swapped for a symlink after the path had been resolved
    let link = base.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(open_without_following_symlinks(&real.join("img.bin")).is_ok());
    assert!(open_without_following_symlinks(&link.join("img.bin")).is_err());
    // and so is a symlink in place of the file
    std::os::unix::fs::symlink(real.join("img.bin"), base.join("img-link.bin")).unwrap();
    assert!(open_without_following_symlinks(&base.join("img-link.bin")).is_err());
}

#[cfg(unix)]
#[test]
fn deleting_removes_the_file_that_was_read() {
    let raster = raster_12_bytes();
    let path = temp_dir_file("tty-graphics-protocol-", &raster);
    let mut opened = open_graphics_file(&path).unwrap();
    let command = KittyCommand::default();
    let bytes = read_file_range(
        &mut opened.file,
        opened.len,
        &command,
        &EchoFields::from_command(&command),
    )
    .unwrap();
    assert_eq!(bytes, raster);
    assert!(should_delete_temp_file(&opened));
    delete_opened_file(&opened);
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn deleting_is_skipped_when_the_name_points_to_a_different_file() {
    let path = temp_dir_file("tty-graphics-protocol-", &raster_12_bytes());
    let opened = open_graphics_file(&path).unwrap();
    // the original inode stays allocated while `opened` holds it, so the
    // replacement cannot reuse its number
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, b"someone else's file").unwrap();
    delete_opened_file(&opened);
    let survived = path.exists();
    let _ = std::fs::remove_file(&path);
    assert!(survived, "a different file under the same name was deleted");
}

#[cfg(unix)]
#[test]
fn deleting_is_skipped_when_the_name_became_a_symlink() {
    let dir = manifest_dir_tempdir();
    let victim = dir.path().join("victim.bin");
    std::fs::write(&victim, b"victim").unwrap();
    let path = temp_dir_file("tty-graphics-protocol-", &raster_12_bytes());
    let opened = open_graphics_file(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&victim, &path).unwrap();
    delete_opened_file(&opened);
    let link_survived = std::fs::symlink_metadata(&path).is_ok();
    let _ = std::fs::remove_file(&path);
    assert!(victim.exists());
    assert!(link_survived, "the symlink under the name was deleted");
}

#[cfg(unix)]
#[test]
fn file_mediums_refuse_proc_sys_and_dev() {
    for medium in ["f", "t"] {
        for path in [
            "/proc/self/cmdline",
            "/proc/self/status",
            "/sys/kernel/uevent_seqnum",
            "/dev/null",
            "/dev/zero",
        ] {
            let existed = std::path::Path::new(path).exists();
            let mut parser = KittyCommandParser::new();
            let err = parse_one(
                &mut parser,
                &raw_cmd(&format!("a=T,f=24,s=1,v=1,t={}", medium), path.as_bytes()),
            )
            .unwrap_err();
            assert_eq!(
                err.code,
                KittyErrorCode::Enotsupported,
                "{} t={}",
                path,
                medium
            );
            assert_eq!(std::path::Path::new(path).exists(), existed, "{}", path);
        }
    }
}

#[cfg(unix)]
#[test]
fn file_medium_follows_symlink_to_regular_file() {
    let raster = raster_12_bytes();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.bin");
    let link = dir.path().join("link.bin");
    std::fs::write(&target, &raster).unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let mut parser = KittyCommandParser::new();
    let err = parse_one(
        &mut parser,
        &raw_cmd("a=T,f=24,s=2,v=2,t=f", link.to_str().unwrap().as_bytes()),
    )
    .unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Enotsupported);
    assert!(target.exists());
    assert_eq!(std::fs::read(&target).unwrap(), raster);
    assert!(std::fs::symlink_metadata(&link).is_ok());
}

#[test]
fn temp_file_medium_outside_temp_dir_is_not_deleted() {
    let raster = raster_12_bytes();
    let dir = tempfile::Builder::new()
        .prefix("kitty-parser-test")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap();
    let path = dir.path().join("img.bin");
    std::fs::write(&path, &raster).unwrap();
    let mut parser = KittyCommandParser::new();
    let err = parse_one(
        &mut parser,
        &raw_cmd("a=T,f=24,s=2,v=2,t=t", path.to_str().unwrap().as_bytes()),
    )
    .unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Enotsupported);
    assert!(path.exists());
    assert_eq!(std::fs::read(&path).unwrap(), raster);
}

#[test]
fn file_medium_larger_than_decode_cap_is_einval() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("huge.bin");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_DECODED_BYTES as u64 + 1).unwrap();
    drop(file);
    let mut parser = KittyCommandParser::new();
    let err = parse_one(
        &mut parser,
        &raw_cmd("a=T,f=24,s=2,v=2,t=f", path.to_str().unwrap().as_bytes()),
    )
    .unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Enotsupported);
    assert!(path.exists());
}

#[test]
fn file_medium_window_within_oversized_file_reads_only_requested_range() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("huge_windowed.bin");
    let mut file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_DECODED_BYTES as u64 + 1024).unwrap();
    file.seek(std::io::SeekFrom::Start(64)).unwrap();
    file.write_all(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12])
        .unwrap();
    drop(file);
    let mut parser = KittyCommandParser::new();
    let err = parse_one(
        &mut parser,
        &raw_cmd(
            "a=T,t=f,f=24,s=2,v=2,O=64,S=12",
            path.to_str().unwrap().as_bytes(),
        ),
    )
    .unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Enotsupported);
    assert!(path.exists());
}

#[test]
fn file_medium_offset_beyond_eof_is_ebadf() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("small.bin");
    std::fs::write(&path, raster_12_bytes()).unwrap();
    let mut parser = KittyCommandParser::new();
    let err = parse_one(
        &mut parser,
        &raw_cmd(
            "a=T,t=f,f=24,s=2,v=2,O=64,S=12",
            path.to_str().unwrap().as_bytes(),
        ),
    )
    .unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Enotsupported);
    assert!(path.exists());
    assert_eq!(std::fs::read(&path).unwrap(), raster_12_bytes());
}

#[test]
fn temp_file_outside_temp_dir_with_magic_name_is_not_deleted() {
    let raster = raster_12_bytes();
    let dir = manifest_dir_tempdir();
    let path = dir.path().join("tty-graphics-protocol-img.bin");
    std::fs::write(&path, &raster).unwrap();
    let err = transmit_temp(&path).unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Enotsupported);
    assert!(path.exists());
    assert_eq!(std::fs::read(&path).unwrap(), raster);
}

#[test]
fn temp_file_inside_temp_dir_without_magic_name_is_not_deleted() {
    let raster = raster_12_bytes();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("img.bin");
    std::fs::write(&path, &raster).unwrap();
    let mut parser = KittyCommandParser::new();
    let command = parse_one(
        &mut parser,
        &raw_cmd("a=T,f=24,s=2,v=2,t=t", path.to_str().unwrap().as_bytes()),
    )
    .unwrap();
    assert_eq!(command.image.unwrap().bytes, raster);
    assert!(path.exists());
}

#[cfg(unix)]
#[test]
fn temp_file_path_escaping_temp_dir_is_not_deleted() {
    let raster = raster_12_bytes();
    let dir = tempfile::Builder::new()
        .prefix("kitty-parser-test")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap();
    let path = dir.path().join("tty-graphics-protocol-victim.bin");
    std::fs::write(&path, &raster).unwrap();
    let temp_root = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    let victim = std::fs::canonicalize(&path).unwrap();
    let mut escaping = temp_root.clone();
    for _ in temp_root.components().skip(1) {
        escaping.push("..");
    }
    escaping.push(victim.strip_prefix("/").unwrap());
    let mut parser = KittyCommandParser::new();
    let command = parse_one(
        &mut parser,
        &raw_cmd(
            "a=T,f=24,s=2,v=2,t=t",
            escaping.to_str().unwrap().as_bytes(),
        ),
    )
    .unwrap();
    assert_eq!(command.image.unwrap().bytes, raster);
    assert!(path.exists());
}

#[test]
fn chunked_upload_exceeding_payload_cap_is_rejected() {
    let mut parser = KittyCommandParser::new();
    let chunk = vec![b'A'; 1 << 20];
    assert!(parser
        .parse(&raw_cmd("a=t,f=32,s=1,v=1,i=7,m=1", b"AAAA"))
        .is_none());
    let mut result = None;
    for _ in 0..200 {
        result = parser.parse(&raw_cmd("m=1", &chunk));
        if result.is_some() {
            break;
        }
    }
    let err = result.unwrap().unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Einval);
    assert!(!parser.has_pending());
}

fn chunk_raw(control: &str, payload_len: usize) -> Vec<u8> {
    let mut raw = format!("{};", control).into_bytes();
    raw.resize(raw.len() + payload_len, b'A');
    raw
}

#[test]
fn chunked_upload_beyond_the_decoded_limit_is_dropped_and_fails_on_the_last_chunk() {
    let mut parser = KittyCommandParser::new();
    let chunk_len = MAX_PAYLOAD_B64_BYTES / 3 * 2;
    assert!(parser
        .parse(&chunk_raw("a=T,f=24,s=1,v=1,i=9,m=1", chunk_len))
        .is_none());
    assert_eq!(
        parser.pending.as_ref().unwrap().payload_b64.len(),
        chunk_len
    );
    // the second chunk crosses the limit: what was assembled is dropped
    assert!(parser.parse(&chunk_raw("m=1", chunk_len)).is_none());
    assert!(parser.pending.as_ref().unwrap().payload_b64.is_empty());
    // further chunks are consumed without being buffered
    assert!(parser.parse(&chunk_raw("m=1", 1024)).is_none());
    assert!(parser.pending.as_ref().unwrap().payload_b64.is_empty());
    let err = parser.parse(&chunk_raw("m=0", 4)).unwrap().unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Einval);
    assert_eq!(err.image_id, Some(9));
    assert!(!parser.has_pending());
    // the parser is usable again afterwards
    let command = parse_one(&mut parser, b"a=T,i=5,f=24,s=1,v=1;AAAA").unwrap();
    assert!(command.image.is_some());
}

#[test]
fn chunked_upload_at_the_limit_is_still_assembled() {
    let mut parser = KittyCommandParser::new();
    let chunk_len = MAX_PAYLOAD_B64_BYTES / 2;
    assert!(parser
        .parse(&chunk_raw("a=T,f=24,s=1,v=1,m=1", chunk_len))
        .is_none());
    let command = parser
        .parse(&chunk_raw("m=0", MAX_PAYLOAD_B64_BYTES - chunk_len))
        .unwrap()
        .unwrap();
    assert_eq!(command.image.unwrap().bytes, vec![0, 0, 0]);
}

#[test]
fn direct_payload_beyond_the_decoded_limit_is_einval() {
    let command = KittyCommand {
        format: KittyFormat::Rgb24,
        pixel_width: Some(1),
        pixel_height: Some(1),
        ..Default::default()
    };
    let payload = vec![b'A'; MAX_PAYLOAD_B64_BYTES + 4];
    let err = run_payload_pipeline(command, &payload).unwrap_err();
    assert_eq!(err.code, KittyErrorCode::Einval);
}
