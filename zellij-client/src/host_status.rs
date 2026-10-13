use zellij_utils::attention::HostProgramStatus;

#[derive(Debug, Default)]
pub struct HostStatusWriter {
    supported: bool,
    latest: Option<Option<HostProgramStatus>>,
    last_written: Option<String>,
}

impl HostStatusWriter {
    pub fn set_supported(&mut self) -> Option<String> {
        self.supported = true;
        self.next_output()
    }
    pub fn update(&mut self, status: Option<HostProgramStatus>) -> Option<String> {
        self.latest = Some(status);
        if self.supported {
            self.next_output()
        } else {
            None
        }
    }
    fn next_output(&mut self) -> Option<String> {
        let status = self.latest.clone()?;
        match status {
            Some(status) => {
                let osc = status.to_osc();
                if self.last_written.as_ref() == Some(&osc) {
                    None
                } else {
                    self.last_written = Some(osc.clone());
                    Some(osc)
                }
            },
            None => self
                .last_written
                .take()
                .map(|_| HostProgramStatus::clear_osc()),
        }
    }
    pub fn teardown(&mut self) -> Option<String> {
        self.last_written
            .take()
            .map(|_| HostProgramStatus::clear_osc())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocked() -> HostProgramStatus {
        HostProgramStatus {
            state: "blocked".to_owned(),
            kind: Some("permission".to_owned()),
            progress: None,
            msg: "api › claude: needs approval".to_owned(),
        }
    }

    #[test]
    fn nothing_is_written_until_the_host_supports_the_protocol() {
        let mut writer = HostStatusWriter::default();
        assert_eq!(writer.update(Some(blocked())), None);
        let written = writer.set_supported().unwrap();
        assert!(written.starts_with("\u{1b}]7501;state=blocked:app=zellij:kind=permission:msg="));
        assert!(written.ends_with("\u{1b}\\"));
        assert_eq!(writer.teardown(), Some("\u{1b}]7501;state=clear\u{1b}\\".to_owned()));
        assert_eq!(writer.teardown(), None);
    }

    #[test]
    fn identical_records_are_not_repeated() {
        let mut writer = HostStatusWriter::default();
        writer.set_supported();
        assert!(writer.update(Some(blocked())).is_some());
        assert_eq!(writer.update(Some(blocked())), None);
    }

    #[test]
    fn turning_status_off_clears_the_record() {
        let mut writer = HostStatusWriter::default();
        writer.set_supported();
        writer.update(Some(blocked()));
        assert_eq!(
            writer.update(None),
            Some("\u{1b}]7501;state=clear\u{1b}\\".to_owned())
        );
        assert_eq!(writer.update(None), None);
        assert_eq!(writer.teardown(), None);
    }

    #[test]
    fn message_is_base64_without_control_characters() {
        let status = HostProgramStatus {
            state: "error".to_owned(),
            kind: None,
            progress: None,
            msg: "a\u{202e}b\nc".to_owned(),
        };
        assert_eq!(
            status.to_osc(),
            "\u{1b}]7501;state=error:app=zellij:msg=YWJj\u{1b}\\"
        );
    }
}
