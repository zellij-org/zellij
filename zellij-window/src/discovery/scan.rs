use crate::font::FaceRequest;
use crate::platform::Platform;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    pub families: Vec<String>,
    pub monospaced: bool,
    pub weight: u16,
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

const PIVOT_WEIGHT: u16 = 400;

fn weight_distance(weight: u16, target: u16) -> (u16, bool) {
    let distance = weight.abs_diff(target);
    let wrong_side = if target >= PIVOT_WEIGHT {
        weight < target
    } else {
        weight > target
    };
    (distance, wrong_side)
}

fn style_distance(face: &Face, request: FaceRequest) -> ((u16, bool), bool) {
    (
        weight_distance(face.weight, request.weight),
        face.italic != request.italic,
    )
}

fn preference_rank(face: &Face, preferred: &[&str]) -> usize {
    preferred
        .iter()
        .position(|family| named(face, family))
        .unwrap_or(preferred.len())
}

pub fn fallback_order(faces: &[Face], preferred: &[&str], request: FaceRequest) -> Vec<usize> {
    let mut order: Vec<usize> = (0..faces.len()).collect();
    order.sort_by_key(|index| {
        let face = &faces[*index];
        (
            preference_rank(face, preferred),
            !face.monospaced,
            style_distance(face, request),
            *index,
        )
    });
    order
}

pub fn monospace_families(faces: &[Face]) -> Vec<String> {
    let mut families: Vec<String> = faces
        .iter()
        .filter(|face| face.monospaced)
        .filter_map(|face| face.families.first().cloned())
        .collect();
    sort_families(&mut families);
    families
}

pub fn sort_families(families: &mut Vec<String>) {
    families.retain(|family| !family.trim().is_empty() && !family.starts_with('.'));
    families.sort_by_key(|family| family.to_lowercase());
    families.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
}

pub fn family_choice(faces: &[Face], family: &str, request: FaceRequest) -> Option<usize> {
    (0..faces.len())
        .filter(|index| named(&faces[*index], family))
        .min_by_key(|index| (style_distance(&faces[*index], request), *index))
}

#[cfg(any(target_os = "macos", windows))]
pub use scanner::Scanner;

#[cfg(any(target_os = "macos", windows))]
mod scanner {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use std::collections::HashMap;

    use super::{fallback_order, family_choice, monospace_families, preferred_fallbacks, Face};
    use crate::font::FaceRequest;
    use crate::platform::Platform;

    pub struct Scanner {
        database: fontdb::Database,
        ids: Vec<fontdb::ID>,
        paths: Vec<PathBuf>,
        faces: Vec<Face>,
        orders: Mutex<HashMap<FaceRequest, Vec<usize>>>,
    }

    impl Scanner {
        pub fn load() -> Option<Mutex<Self>> {
            let mut database = fontdb::Database::new();
            database.load_system_fonts();
            let scanner = Self::from_database(database);
            if scanner.is_none() {
                report!("no system fonts were found; falling back to the embedded fonts");
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
                    weight: info.weight.0,
                    italic: info.style != fontdb::Style::Normal,
                });
            }
            if faces.is_empty() {
                return None;
            }
            Some(Self {
                database,
                ids,
                paths,
                faces,
                orders: Mutex::new(HashMap::new()),
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

        fn order(&self, request: FaceRequest) -> Vec<usize> {
            let mut orders = match self.orders.lock() {
                Ok(orders) => orders,
                Err(poisoned) => poisoned.into_inner(),
            };
            orders
                .entry(request)
                .or_insert_with(|| {
                    fallback_order(
                        &self.faces,
                        preferred_fallbacks(Platform::current()),
                        request,
                    )
                })
                .clone()
        }

        pub fn match_codepoint(
            &self,
            request: FaceRequest,
            character: char,
        ) -> Option<(PathBuf, u32)> {
            self.order(request)
                .iter()
                .find(|index| self.covers(**index, character))
                .and_then(|index| self.located(*index))
        }

        pub fn match_family(&self, family: &str, request: FaceRequest) -> Option<(PathBuf, u32)> {
            family_choice(&self.faces, family, request).and_then(|index| self.located(index))
        }

        pub fn monospace_families(&self) -> Vec<String> {
            monospace_families(&self.faces)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::FaceStyle;

    fn face(family: &str, monospaced: bool, bold: bool, italic: bool) -> Face {
        Face {
            families: vec![family.to_owned()],
            monospaced,
            weight: if bold { 700 } else { 400 },
            italic,
        }
    }

    fn weighted(family: &str, weight: u16) -> Face {
        Face {
            families: vec![family.to_owned()],
            monospaced: true,
            weight,
            italic: false,
        }
    }

    fn upright(weight: u16) -> FaceRequest {
        FaceRequest {
            weight,
            italic: false,
        }
    }

    fn weights() -> Vec<Face> {
        vec![
            weighted("Mono", 100),
            weighted("Mono", 300),
            weighted("Mono", 400),
            weighted("Mono", 500),
            weighted("Mono", 700),
        ]
    }

    #[test]
    fn only_monospaced_families_are_listed_once_each_in_order() {
        let faces = vec![
            face("Zed Mono", true, false, false),
            face("Arial", false, false, false),
            face("iosevka term", true, false, false),
            face("Iosevka Term", true, true, false),
            face(".Hidden Mono", true, false, false),
        ];
        assert_eq!(
            monospace_families(&faces),
            vec!["iosevka term".to_owned(), "Zed Mono".to_owned()]
        );
    }

    #[test]
    fn the_default_request_picks_regular_over_thin_light_and_medium() {
        assert_eq!(
            family_choice(&weights(), "Mono", FaceStyle::Regular.into()),
            Some(2)
        );
        let mut shuffled = weights();
        shuffled.rotate_left(3);
        assert_eq!(
            shuffled[family_choice(&shuffled, "Mono", FaceStyle::Regular.into()).unwrap()].weight,
            400
        );
    }

    #[test]
    fn a_medium_request_picks_medium() {
        assert_eq!(family_choice(&weights(), "Mono", upright(500)), Some(3));
    }

    #[test]
    fn bold_picks_the_heavier_face() {
        assert_eq!(
            family_choice(&weights(), "Mono", FaceStyle::Bold.into()),
            Some(4)
        );
        assert_eq!(
            family_choice(
                &weights(),
                "Mono",
                FaceRequest::weighted(FaceStyle::Bold, 500)
            ),
            Some(4)
        );
    }

    #[test]
    fn ties_go_heavier_from_regular_up_and_lighter_below_it() {
        let faces = vec![
            weighted("Mono", 300),
            weighted("Mono", 500),
            weighted("Mono", 200),
            weighted("Mono", 400),
        ];
        assert_eq!(family_choice(&faces, "Mono", upright(450)), Some(1));
        assert_eq!(family_choice(&faces, "Mono", upright(250)), Some(2));
        assert_eq!(
            family_choice(&faces[..2], "Mono", upright(400)),
            Some(1),
            "with no exact match a regular request leans heavier"
        );
        assert_eq!(
            family_choice(&faces[..3], "Mono", upright(250)),
            Some(2),
            "below regular a request leans lighter"
        );
    }

    #[test]
    fn a_light_request_takes_the_nearest_face_and_the_lighter_of_two_equals() {
        let faces = vec![weighted("Mono", 400), weighted("Mono", 100)];
        assert_eq!(family_choice(&faces, "Mono", upright(300)), Some(0));
        let faces = vec![weighted("Mono", 400), weighted("Mono", 200)];
        assert_eq!(family_choice(&faces, "Mono", upright(300)), Some(1));
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
            fallback_order(&faces, &["mono", "CJK", "Emoji"], FaceStyle::Regular.into()),
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
            fallback_order(&faces, &[], FaceStyle::Regular.into()),
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
            fallback_order(&faces, &["Mono"], FaceStyle::Bold.into()),
            vec![1, 3, 0, 2]
        );
        assert_eq!(
            fallback_order(&faces, &["Mono"], FaceStyle::Italic.into()),
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
            family_choice(&faces, "iosevka term", FaceStyle::Bold.into()),
            Some(2)
        );
        assert_eq!(
            family_choice(&faces, "Iosevka Term", FaceStyle::BoldItalic.into()),
            Some(2)
        );
        assert_eq!(
            family_choice(&faces, "Iosevka Term", FaceStyle::Italic.into()),
            Some(1)
        );
        assert_eq!(
            family_choice(&faces, "Missing", FaceStyle::Regular.into()),
            None
        );
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
