use zellij_tile::prelude::*;

pub const PREVIEWED_THEME_SETTINGS: [SettingKey; 3] =
    [SettingKey::Theme, SettingKey::ThemeDark, SettingKey::ThemeLight];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewAction {
    Set(SettingKey, String),
    Unset(SettingKey),
    Revert(SettingKey),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Preview {
    key: SettingKey,
    original: Option<String>,
    original_was_unsaved: bool,
    shown_originally: String,
    shown: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThemePreview {
    active: Option<Preview>,
}

impl ThemePreview {
    pub fn active_key(&self) -> Option<SettingKey> {
        self.active.as_ref().map(|preview| preview.key)
    }
    pub fn is_previewing(&self, key: SettingKey) -> bool {
        self.active_key() == Some(key)
    }
    fn restore_action(preview: &Preview) -> Option<PreviewAction> {
        if preview.shown.as_deref() == Some(preview.shown_originally.as_str()) {
            return None;
        }
        Some(if preview.original_was_unsaved {
            match &preview.original {
                Some(original) => PreviewAction::Set(preview.key, original.clone()),
                None => PreviewAction::Unset(preview.key),
            }
        } else {
            PreviewAction::Revert(preview.key)
        })
    }
    pub fn highlight(
        &mut self,
        key: SettingKey,
        highlighted: &str,
        shown_originally: &str,
        state: Option<&ConfigSettingState>,
        unset_label: &str,
    ) -> Vec<PreviewAction> {
        let mut actions = vec![];
        if self.active_key().map(|active| active != key).unwrap_or(false) {
            actions.extend(self.restore());
        }
        let preview = self.active.get_or_insert_with(|| Preview {
            key,
            original: state.and_then(|state| state.current_value.clone()),
            original_was_unsaved: state.map(|state| state.is_unsaved()).unwrap_or(false),
            shown_originally: shown_originally.to_owned(),
            shown: Some(shown_originally.to_owned()),
        });
        if preview.shown.as_deref() == Some(highlighted) {
            return actions;
        }
        if highlighted == preview.shown_originally {
            actions.extend(Self::restore_action(preview));
        } else if highlighted == unset_label {
            actions.push(PreviewAction::Unset(key));
        } else {
            actions.push(PreviewAction::Set(key, highlighted.to_owned()));
        }
        preview.shown = Some(highlighted.to_owned());
        actions
    }
    pub fn restore(&mut self) -> Option<PreviewAction> {
        self.active
            .take()
            .and_then(|preview| Self::restore_action(&preview))
    }
    pub fn commit(&mut self, key: SettingKey) {
        if self.is_previewing(key) {
            self.active = None;
        }
    }
    pub fn file_replaced(&mut self, dropped: &[SettingKey]) {
        if let Some(preview) = self.active.as_mut() {
            if dropped.contains(&preview.key) {
                preview.original_was_unsaved = false;
                preview.original = None;
                preview.shown = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNSET: &str = "(not set)";

    fn state(saved: Option<&str>, current: Option<&str>) -> ConfigSettingState {
        ConfigSettingState {
            key: SettingKey::Theme,
            saved_value: saved.map(|v| v.to_owned()),
            current_value: current.map(|v| v.to_owned()),
            set_in_file: saved.is_some(),
        }
    }

    #[test]
    fn highlighting_applies_each_new_theme_once() {
        let mut preview = ThemePreview::default();
        let saved = state(Some("nord"), Some("nord"));
        assert_eq!(
            preview.highlight(SettingKey::Theme, "nord", "nord", Some(&saved), UNSET),
            vec![]
        );
        assert_eq!(
            preview.highlight(SettingKey::Theme, "dracula", "nord", Some(&saved), UNSET),
            vec![PreviewAction::Set(SettingKey::Theme, "dracula".to_owned())]
        );
        assert_eq!(
            preview.highlight(SettingKey::Theme, "dracula", "nord", Some(&saved), UNSET),
            vec![]
        );
        assert!(preview.is_previewing(SettingKey::Theme));
    }

    #[test]
    fn a_saved_original_is_restored_by_reverting_to_the_saved_value() {
        let mut preview = ThemePreview::default();
        let saved = state(Some("nord"), Some("nord"));
        preview.highlight(SettingKey::Theme, "dracula", "nord", Some(&saved), UNSET);
        assert_eq!(
            preview.restore(),
            Some(PreviewAction::Revert(SettingKey::Theme))
        );
        assert_eq!(preview.active_key(), None);
        assert_eq!(preview.restore(), None);
    }

    #[test]
    fn an_unset_original_is_never_written_as_default() {
        let mut preview = ThemePreview::default();
        let unset = state(None, None);
        preview.highlight(SettingKey::Theme, "dracula", "default", Some(&unset), UNSET);
        assert_eq!(
            preview.highlight(SettingKey::Theme, "default", "default", Some(&unset), UNSET),
            vec![PreviewAction::Revert(SettingKey::Theme)]
        );
        assert_eq!(preview.restore(), None);
        let mut preview = ThemePreview::default();
        preview.highlight(SettingKey::Theme, "dracula", "default", Some(&unset), UNSET);
        assert_eq!(
            preview.restore(),
            Some(PreviewAction::Revert(SettingKey::Theme))
        );
    }

    #[test]
    fn an_unsaved_original_is_set_again_and_an_unsaved_unset_is_unset_again() {
        let mut preview = ThemePreview::default();
        let unsaved = state(Some("nord"), Some("gruvbox-dark"));
        preview.highlight(
            SettingKey::Theme,
            "dracula",
            "gruvbox-dark",
            Some(&unsaved),
            UNSET,
        );
        assert_eq!(
            preview.restore(),
            Some(PreviewAction::Set(
                SettingKey::Theme,
                "gruvbox-dark".to_owned()
            ))
        );
        let mut preview = ThemePreview::default();
        let unsaved_unset = ConfigSettingState {
            key: SettingKey::ThemeDark,
            saved_value: Some("nord".to_owned()),
            current_value: None,
            set_in_file: true,
        };
        assert_eq!(
            preview.highlight(
                SettingKey::ThemeDark,
                "dracula",
                UNSET,
                Some(&unsaved_unset),
                UNSET
            ),
            vec![PreviewAction::Set(SettingKey::ThemeDark, "dracula".to_owned())]
        );
        assert_eq!(
            preview.restore(),
            Some(PreviewAction::Unset(SettingKey::ThemeDark))
        );
    }

    #[test]
    fn highlighting_the_unset_choice_previews_an_unset_theme() {
        let mut preview = ThemePreview::default();
        let saved = ConfigSettingState {
            key: SettingKey::ThemeLight,
            saved_value: Some("nord".to_owned()),
            current_value: Some("nord".to_owned()),
            set_in_file: true,
        };
        assert_eq!(
            preview.highlight(SettingKey::ThemeLight, UNSET, "nord", Some(&saved), UNSET),
            vec![PreviewAction::Unset(SettingKey::ThemeLight)]
        );
        assert_eq!(
            preview.restore(),
            Some(PreviewAction::Revert(SettingKey::ThemeLight))
        );
    }

    #[test]
    fn a_file_change_that_replaced_the_theme_restores_the_new_saved_value() {
        let mut preview = ThemePreview::default();
        let unsaved = state(Some("nord"), Some("gruvbox-dark"));
        preview.highlight(
            SettingKey::Theme,
            "dracula",
            "gruvbox-dark",
            Some(&unsaved),
            UNSET,
        );
        preview.file_replaced(&[SettingKey::MouseMode]);
        assert!(preview.is_previewing(SettingKey::Theme));
        preview.file_replaced(&[SettingKey::Theme]);
        assert_eq!(
            preview.restore(),
            Some(PreviewAction::Revert(SettingKey::Theme))
        );
    }

    #[test]
    fn a_file_change_during_preview_reapplies_the_highlighted_theme() {
        let mut preview = ThemePreview::default();
        let saved = state(Some("nord"), Some("nord"));
        preview.highlight(SettingKey::Theme, "dracula", "nord", Some(&saved), UNSET);
        preview.file_replaced(&[SettingKey::Theme]);
        assert_eq!(
            preview.highlight(SettingKey::Theme, "dracula", "nord", Some(&saved), UNSET),
            vec![PreviewAction::Set(SettingKey::Theme, "dracula".to_owned())]
        );
    }

    #[test]
    fn choosing_ends_the_preview_without_restoring() {
        let mut preview = ThemePreview::default();
        let saved = state(Some("nord"), Some("nord"));
        preview.highlight(SettingKey::Theme, "dracula", "nord", Some(&saved), UNSET);
        preview.commit(SettingKey::Theme);
        assert_eq!(preview.restore(), None);
    }

    #[test]
    fn previewing_another_setting_restores_the_first_one() {
        let mut preview = ThemePreview::default();
        let saved = state(Some("nord"), Some("nord"));
        preview.highlight(SettingKey::Theme, "dracula", "nord", Some(&saved), UNSET);
        let dark = ConfigSettingState {
            key: SettingKey::ThemeDark,
            saved_value: None,
            current_value: None,
            set_in_file: false,
        };
        assert_eq!(
            preview.highlight(SettingKey::ThemeDark, "nord", UNSET, Some(&dark), UNSET),
            vec![
                PreviewAction::Revert(SettingKey::Theme),
                PreviewAction::Set(SettingKey::ThemeDark, "nord".to_owned())
            ]
        );
    }
}
