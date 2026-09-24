use crate::font::FaceStyle;
use crate::platform::Platform;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    pub families: Vec<String>,
    pub monospaced: bool,
    pub bold: bool,
    pub italic: bool,
}

pub fn preferred_fallbacks(platform: Platform) -> &'static [&'static str] {
    match platform {
        Platform::MacOs => &[
            "Menlo",
            "SF Mono",
            "Monaco",
            "PingFang SC",
            "Hiragino Sans",
            "Apple SD Gothic Neo",
            "Apple Symbols",
            "Apple Color Emoji",
        ],
        Platform::Windows => &[
            "Cascadia Mono",
            "Consolas",
            "Microsoft YaHei",
            "Yu Gothic",
            "Malgun Gothic",
            "Segoe UI Symbol",
            "Segoe UI Emoji",
        ],
        Platform::Linux => &[],
    }
}

fn named(face: &Face, family: &str) -> bool {
    face.families
        .iter()
        .any(|name| name.eq_ignore_ascii_case(family))
}

fn style_distance(face: &Face, style: FaceStyle) -> u8 {
    2 * u8::from(face.bold != style.is_bold()) + u8::from(face.italic != style.is_italic())
}

fn preference_rank(face: &Face, preferred: &[&str]) -> usize {
    preferred
        .iter()
        .position(|family| named(face, family))
        .unwrap_or(preferred.len())
}

pub fn fallback_order(faces: &[Face], preferred: &[&str], style: FaceStyle) -> Vec<usize> {
    let mut order: Vec<usize> = (0..faces.len()).collect();
    order.sort_by_key(|index| {
        let face = &faces[*index];
        (
            preference_rank(face, preferred),
            !face.monospaced,
            style_distance(face, style),
            *index,
        )
    });
    order
}

pub fn family_choice(faces: &[Face], family: &str, style: FaceStyle) -> Option<usize> {
    (0..faces.len())
        .filter(|index| named(&faces[*index], family))
        .min_by_key(|index| (style_distance(&faces[*index], style), *index))
}

#[cfg(any(target_os = "macos", windows))]
pub use scanner::Scanner;

#[cfg(any(target_os = "macos", windows))]
mod scanner {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use super::{fallback_order, family_choice, preferred_fallbacks, Face};
    use crate::font::FaceStyle;
    use crate::platform::Platform;

    const BOLD_WEIGHT: u16 = 600;

    pub struct Scanner {
        database: fontdb::Database,
        ids: Vec<fontdb::ID>,
        paths: Vec<PathBuf>,
        faces: Vec<Face>,
        orders: [Vec<usize>; 4],
    }

    impl Scanner {
        pub fn load() -> Option<Mutex<Self>> {
            let mut database = fontdb::Database::new();
            database.load_system_fonts();
            let scanner = Self::from_database(database);
            if scanner.is_none() {
                eprintln!(
                    "zellij-window: no system fonts were found; falling back to the embedded fonts"
                );
            }
            scanner.map(Mutex::new)
        }

        #[cfg(test)]
        pub fn synthetic(directory: &std::path::Path) -> Option<Self> {
            let mut database = fontdb::Database::new();
            database.load_fonts_dir(directory);
            Self::from_database(database)
        }

        fn from_database(database: fontdb::Database) -> Option<Self> {
            let mut ids = Vec::new();
            let mut paths = Vec::new();
            let mut faces = Vec::new();
            for info in database.faces() {
                let path = match &info.source {
                    fontdb::Source::File(path) => path.clone(),
                    fontdb::Source::SharedFile(path, _) => path.clone(),
                    fontdb::Source::Binary(_) => continue,
                };
                ids.push(info.id);
                paths.push(path);
                faces.push(Face {
                    families: info.families.iter().map(|(name, _)| name.clone()).collect(),
                    monospaced: info.monospaced,
                    bold: info.weight.0 >= BOLD_WEIGHT,
                    italic: info.style != fontdb::Style::Normal,
                });
            }
            if faces.is_empty() {
                return None;
            }
            let preferred = preferred_fallbacks(Platform::current());
            let orders = [
                FaceStyle::Regular,
                FaceStyle::Bold,
                FaceStyle::Italic,
                FaceStyle::BoldItalic,
            ]
            .map(|style| fallback_order(&faces, preferred, style));
            Some(Self {
                database,
                ids,
                paths,
                faces,
                orders,
            })
        }

        fn located(&self, index: usize) -> Option<(PathBuf, u32)> {
            let info = self.database.face(self.ids[index])?;
            Some((self.paths[index].clone(), info.index))
        }

        fn covers(&self, index: usize, character: char) -> bool {
            self.database
                .with_face_data(self.ids[index], |data, face| {
                    swash::FontRef::from_index(data, face as usize)
                        .is_some_and(|font| font.charmap().map(character) != 0)
                })
                .unwrap_or(false)
        }

        pub fn match_codepoint(&self, style: FaceStyle, character: char) -> Option<(PathBuf, u32)> {
            self.orders[style.index()]
                .iter()
                .find(|index| self.covers(**index, character))
                .and_then(|index| self.located(*index))
        }

        pub fn match_family(&self, family: &str, style: FaceStyle) -> Option<(PathBuf, u32)> {
            family_choice(&self.faces, family, style).and_then(|index| self.located(index))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(family: &str, monospaced: bool, bold: bool, italic: bool) -> Face {
        Face {
            families: vec![family.to_owned()],
            monospaced,
            bold,
            italic,
        }
    }

    #[test]
    fn preferred_families_come_first_in_the_order_they_are_listed() {
        let faces = vec![
            face("Some Sans", false, false, false),
            face("Emoji", false, false, false),
            face("Mono", true, false, false),
            face("CJK", false, false, false),
        ];
        assert_eq!(
            fallback_order(&faces, &["mono", "CJK", "Emoji"], FaceStyle::Regular),
            vec![2, 3, 1, 0]
        );
    }

    #[test]
    fn among_the_rest_monospaced_faces_come_before_proportional_ones() {
        let faces = vec![
            face("Proportional", false, false, false),
            face("Fixed", true, false, false),
            face("Other Proportional", false, false, false),
            face("Other Fixed", true, false, false),
        ];
        assert_eq!(
            fallback_order(&faces, &[], FaceStyle::Regular),
            vec![1, 3, 0, 2]
        );
    }

    #[test]
    fn within_a_family_the_requested_style_wins_and_weight_matters_more_than_slant() {
        let faces = vec![
            face("Mono", true, false, false),
            face("Mono", true, true, false),
            face("Mono", true, false, true),
            face("Mono", true, true, true),
        ];
        assert_eq!(
            fallback_order(&faces, &["Mono"], FaceStyle::Bold),
            vec![1, 3, 0, 2]
        );
        assert_eq!(
            fallback_order(&faces, &["Mono"], FaceStyle::Italic),
            vec![2, 0, 3, 1]
        );
    }

    #[test]
    fn a_family_is_found_by_name_regardless_of_case_and_its_closest_style_is_chosen() {
        let faces = vec![
            face("Other", true, true, false),
            face("Iosevka Term", true, false, false),
            face("Iosevka Term", true, true, false),
        ];
        assert_eq!(
            family_choice(&faces, "iosevka term", FaceStyle::Bold),
            Some(2)
        );
        assert_eq!(
            family_choice(&faces, "Iosevka Term", FaceStyle::BoldItalic),
            Some(2)
        );
        assert_eq!(
            family_choice(&faces, "Iosevka Term", FaceStyle::Italic),
            Some(1)
        );
        assert_eq!(family_choice(&faces, "Missing", FaceStyle::Regular), None);
    }

    #[test]
    fn each_desktop_platform_names_monospace_cjk_and_emoji_fallbacks_and_linux_defers_to_fontconfig(
    ) {
        assert!(preferred_fallbacks(Platform::Linux).is_empty());
        for (platform, monospace, cjk, emoji) in [
            (Platform::MacOs, "Menlo", "PingFang SC", "Apple Color Emoji"),
            (
                Platform::Windows,
                "Consolas",
                "Microsoft YaHei",
                "Segoe UI Emoji",
            ),
        ] {
            let preferred = preferred_fallbacks(platform);
            for family in [monospace, cjk, emoji] {
                assert!(preferred.contains(&family), "{:?} {}", platform, family);
            }
            let position = |family| preferred.iter().position(|name| *name == family);
            assert!(position(monospace) < position(cjk));
            assert!(position(cjk) < position(emoji));
        }
    }
}
