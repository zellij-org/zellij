use std::path::PathBuf;

pub(crate) struct Digest(u64);

impl Digest {
    pub(crate) fn new() -> Self {
        Digest(0xcbf2_9ce4_8422_2325)
    }

    pub(crate) fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= byte as u64;
            self.0 = self.0.wrapping_mul(0x100_0000_01b3);
        }
    }

    pub(crate) fn number(&mut self, number: u64) {
        self.bytes(&number.to_le_bytes());
    }

    pub(crate) fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{:02x}", byte)).collect()
}

pub(crate) fn check(name: &str, lines: &[String]) {
    let path = PathBuf::from("src/unit/goldens").join(format!("{}.txt", name));
    let actual: String = lines.iter().map(|line| format!("{}\n", line)).collect();
    if std::env::var_os("ZELLIJ_UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().expect("golden dir")).expect("create golden dir");
        std::fs::write(&path, &actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| {
            panic!(
                "missing golden file {}; create it with ZELLIJ_UPDATE_GOLDENS=1",
                path.display()
            )
        })
        .replace("\r\n", "\n");
    if expected == actual {
        return;
    }
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let first = expected_lines
        .iter()
        .zip(actual_lines.iter())
        .position(|(expected, actual)| expected != actual)
        .unwrap_or(expected_lines.len().min(actual_lines.len()));
    panic!(
        "{} differs from {} at line {} ({} lines expected, {} produced)\n expected: {}\n actual:   {}\nif the change is intended, regenerate with ZELLIJ_UPDATE_GOLDENS=1 and review the diff",
        name,
        path.display(),
        first + 1,
        expected_lines.len(),
        actual_lines.len(),
        expected_lines.get(first).unwrap_or(&"<end of file>"),
        actual_lines.get(first).unwrap_or(&"<end of file>"),
    );
}
