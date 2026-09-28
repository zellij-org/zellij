use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::font::FaceStyle;

#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
mod scan;

#[cfg(not(any(target_os = "macos", windows)))]
use fontconfig::Fontconfig as Backend;
#[cfg(any(target_os = "macos", windows))]
use scan::Scanner as Backend;

static BACKEND: OnceLock<Option<Mutex<Backend>>> = OnceLock::new();

#[derive(Clone, Copy)]
pub struct Discovery(&'static Mutex<Backend>);

impl Discovery {
    pub fn open() -> Option<Self> {
        BACKEND.get_or_init(Backend::load).as_ref().map(Discovery)
    }

    pub fn match_codepoint(&self, style: FaceStyle, character: char) -> Option<(PathBuf, u32)> {
        match self.0.lock() {
            Ok(backend) => backend.match_codepoint(style, character),
            Err(_) => {
                eprintln!("zellij-window: the font discovery mutex was poisoned");
                None
            },
        }
    }

    pub fn match_family(&self, family: &str, style: FaceStyle) -> Option<(PathBuf, u32)> {
        match self.0.lock() {
            Ok(backend) => backend.match_family(family, style),
            Err(_) => {
                eprintln!("zellij-window: the font discovery mutex was poisoned");
                None
            },
        }
    }

    #[cfg(test)]
    pub fn synthetic(directory: &std::path::Path) -> Option<Self> {
        Backend::synthetic(directory)
            .map(|backend| Discovery(Box::leak(Box::new(Mutex::new(backend)))))
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod fontconfig {
    use std::ffi::{c_char, c_int, c_void, CStr, CString};
    use std::path::PathBuf;
    use std::sync::Mutex;

    use crate::font::FaceStyle;

    fn library_name() -> Option<&'static str> {
        #[cfg(target_os = "linux")]
        {
            Some("libfontconfig.so.1")
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    const FC_MATCH_PATTERN: c_int = 0;
    const FC_RESULT_MATCH: c_int = 0;
    const FC_WEIGHT_REGULAR: c_int = 80;
    const FC_WEIGHT_BOLD: c_int = 200;
    const FC_SLANT_ROMAN: c_int = 0;
    const FC_SLANT_ITALIC: c_int = 100;

    const FAMILY: &[u8] = b"family\0";
    const WEIGHT: &[u8] = b"weight\0";
    const SLANT: &[u8] = b"slant\0";
    const CHARSET: &[u8] = b"charset\0";
    const FILE: &[u8] = b"file\0";
    const INDEX: &[u8] = b"index\0";
    const MONOSPACE: &[u8] = b"monospace\0";

    const MAX_FAMILY_NAMES: c_int = 8;

    type Config = c_void;
    type Pattern = c_void;
    type CharSet = c_void;

    struct Api {
        init_load_config_and_fonts: unsafe extern "C" fn() -> *mut Config,
        pattern_create: unsafe extern "C" fn() -> *mut Pattern,
        pattern_destroy: unsafe extern "C" fn(*mut Pattern),
        pattern_add_string: unsafe extern "C" fn(*mut Pattern, *const c_char, *const u8) -> c_int,
        pattern_add_integer: unsafe extern "C" fn(*mut Pattern, *const c_char, c_int) -> c_int,
        pattern_add_charset:
            unsafe extern "C" fn(*mut Pattern, *const c_char, *const CharSet) -> c_int,
        pattern_get_string:
            unsafe extern "C" fn(*const Pattern, *const c_char, c_int, *mut *mut u8) -> c_int,
        pattern_get_integer:
            unsafe extern "C" fn(*const Pattern, *const c_char, c_int, *mut c_int) -> c_int,
        pattern_get_charset:
            unsafe extern "C" fn(*const Pattern, *const c_char, c_int, *mut *mut CharSet) -> c_int,
        charset_create: unsafe extern "C" fn() -> *mut CharSet,
        charset_destroy: unsafe extern "C" fn(*mut CharSet),
        charset_add_char: unsafe extern "C" fn(*mut CharSet, u32) -> c_int,
        charset_has_char: unsafe extern "C" fn(*const CharSet, u32) -> c_int,
        config_substitute: unsafe extern "C" fn(*mut Config, *mut Pattern, c_int) -> c_int,
        default_substitute: unsafe extern "C" fn(*mut Pattern),
        font_match: unsafe extern "C" fn(*mut Config, *mut Pattern, *mut c_int) -> *mut Pattern,
        #[cfg(test)]
        config_create: unsafe extern "C" fn() -> *mut Config,
        #[cfg(test)]
        config_app_font_add_dir: unsafe extern "C" fn(*mut Config, *const u8) -> c_int,
        _library: libloading::Library,
    }

    pub(super) struct Fontconfig {
        api: Api,
        config: *mut Config,
    }

    unsafe impl Send for Fontconfig {}

    impl Fontconfig {
        pub(super) fn load() -> Option<Mutex<Self>> {
            let Some(library) = library_name() else {
                eprintln!(
                    "zellij-window: system font discovery has no backend on this platform; \
                     falling back to the embedded fonts"
                );
                return None;
            };
            Self::load_from(library)
        }

        pub(super) fn load_from(library: &str) -> Option<Mutex<Self>> {
            let api = match unsafe { load(library) } {
                Ok(api) => api,
                Err(e) => {
                    eprintln!(
                        "zellij-window: system font discovery is unavailable ({}); \
                         falling back to the embedded fonts",
                        e
                    );
                    return None;
                },
            };

            let config = unsafe { (api.init_load_config_and_fonts)() };
            if config.is_null() {
                eprintln!("zellij-window: fontconfig produced no configuration");
                return None;
            }

            Some(Mutex::new(Self { api, config }))
        }

        #[cfg(test)]
        pub(super) fn synthetic(directory: &std::path::Path) -> Option<Self> {
            let api = unsafe { load(library_name()?) }.ok()?;
            let config = unsafe { (api.config_create)() };
            if config.is_null() {
                return None;
            }
            let directory = CString::new(directory.to_str()?).ok()?;
            let added =
                unsafe { (api.config_app_font_add_dir)(config, directory.as_ptr() as *const u8) };
            (added != 0).then_some(Self { api, config })
        }

        pub(super) fn match_codepoint(
            &self,
            style: FaceStyle,
            character: char,
        ) -> Option<(PathBuf, u32)> {
            let api = &self.api;
            unsafe {
                let charset = (api.charset_create)();
                if charset.is_null() {
                    return None;
                }
                (api.charset_add_char)(charset, character as u32);

                let pattern = (api.pattern_create)();
                if pattern.is_null() {
                    (api.charset_destroy)(charset);
                    return None;
                }
                (api.pattern_add_string)(
                    pattern,
                    FAMILY.as_ptr() as *const c_char,
                    MONOSPACE.as_ptr(),
                );
                (api.pattern_add_integer)(
                    pattern,
                    WEIGHT.as_ptr() as *const c_char,
                    if style.is_bold() {
                        FC_WEIGHT_BOLD
                    } else {
                        FC_WEIGHT_REGULAR
                    },
                );
                (api.pattern_add_integer)(
                    pattern,
                    SLANT.as_ptr() as *const c_char,
                    if style.is_italic() {
                        FC_SLANT_ITALIC
                    } else {
                        FC_SLANT_ROMAN
                    },
                );
                (api.pattern_add_charset)(pattern, CHARSET.as_ptr() as *const c_char, charset);
                (api.config_substitute)(self.config, pattern, FC_MATCH_PATTERN);
                (api.default_substitute)(pattern);

                let mut result: c_int = 0;
                let matched = (api.font_match)(self.config, pattern, &mut result);
                let found = if matched.is_null() || result != FC_RESULT_MATCH {
                    None
                } else {
                    self.describe(matched, character)
                };

                if !matched.is_null() {
                    (api.pattern_destroy)(matched);
                }
                (api.pattern_destroy)(pattern);
                (api.charset_destroy)(charset);
                found
            }
        }

        pub(super) fn match_family(
            &self,
            family: &str,
            style: FaceStyle,
        ) -> Option<(PathBuf, u32)> {
            let api = &self.api;
            let requested = CString::new(family).ok()?;
            unsafe {
                let pattern = (api.pattern_create)();
                if pattern.is_null() {
                    return None;
                }
                (api.pattern_add_string)(
                    pattern,
                    FAMILY.as_ptr() as *const c_char,
                    requested.as_ptr() as *const u8,
                );
                (api.pattern_add_integer)(
                    pattern,
                    WEIGHT.as_ptr() as *const c_char,
                    if style.is_bold() {
                        FC_WEIGHT_BOLD
                    } else {
                        FC_WEIGHT_REGULAR
                    },
                );
                (api.pattern_add_integer)(
                    pattern,
                    SLANT.as_ptr() as *const c_char,
                    if style.is_italic() {
                        FC_SLANT_ITALIC
                    } else {
                        FC_SLANT_ROMAN
                    },
                );
                (api.config_substitute)(self.config, pattern, FC_MATCH_PATTERN);
                (api.default_substitute)(pattern);

                let mut result: c_int = 0;
                let matched = (api.font_match)(self.config, pattern, &mut result);
                let found = if matched.is_null()
                    || result != FC_RESULT_MATCH
                    || !self.answers_to(matched, family)
                {
                    None
                } else {
                    self.locate(matched)
                };

                if !matched.is_null() {
                    (api.pattern_destroy)(matched);
                }
                (api.pattern_destroy)(pattern);
                found
            }
        }

        unsafe fn answers_to(&self, matched: *mut Pattern, requested: &str) -> bool {
            let api = &self.api;
            for index in 0..MAX_FAMILY_NAMES {
                let mut name: *mut u8 = std::ptr::null_mut();
                if (api.pattern_get_string)(
                    matched,
                    FAMILY.as_ptr() as *const c_char,
                    index,
                    &mut name,
                ) != FC_RESULT_MATCH
                    || name.is_null()
                {
                    return false;
                }
                if CStr::from_ptr(name as *const c_char)
                    .to_str()
                    .is_ok_and(|name| name.eq_ignore_ascii_case(requested))
                {
                    return true;
                }
            }
            false
        }

        unsafe fn describe(
            &self,
            matched: *mut Pattern,
            character: char,
        ) -> Option<(PathBuf, u32)> {
            let api = &self.api;

            let mut covered: *mut CharSet = std::ptr::null_mut();
            if (api.pattern_get_charset)(
                matched,
                CHARSET.as_ptr() as *const c_char,
                0,
                &mut covered,
            ) == FC_RESULT_MATCH
                && !covered.is_null()
                && (api.charset_has_char)(covered, character as u32) == 0
            {
                return None;
            }

            self.locate(matched)
        }

        unsafe fn locate(&self, matched: *mut Pattern) -> Option<(PathBuf, u32)> {
            let api = &self.api;

            let mut path: *mut u8 = std::ptr::null_mut();
            if (api.pattern_get_string)(matched, FILE.as_ptr() as *const c_char, 0, &mut path)
                != FC_RESULT_MATCH
                || path.is_null()
            {
                return None;
            }
            let path = CStr::from_ptr(path as *const c_char).to_str().ok()?;

            let mut index: c_int = 0;
            if (api.pattern_get_integer)(matched, INDEX.as_ptr() as *const c_char, 0, &mut index)
                != FC_RESULT_MATCH
            {
                index = 0;
            }

            Some((PathBuf::from(path), index.max(0) as u32))
        }
    }

    unsafe fn load(name: &str) -> Result<Api, libloading::Error> {
        let library = libloading::Library::new(name)?;

        macro_rules! symbol {
            ($name:literal) => {
                *library.get($name)?
            };
        }

        Ok(Api {
            init_load_config_and_fonts: symbol!(b"FcInitLoadConfigAndFonts\0"),
            pattern_create: symbol!(b"FcPatternCreate\0"),
            pattern_destroy: symbol!(b"FcPatternDestroy\0"),
            pattern_add_string: symbol!(b"FcPatternAddString\0"),
            pattern_add_integer: symbol!(b"FcPatternAddInteger\0"),
            pattern_add_charset: symbol!(b"FcPatternAddCharSet\0"),
            pattern_get_string: symbol!(b"FcPatternGetString\0"),
            pattern_get_integer: symbol!(b"FcPatternGetInteger\0"),
            pattern_get_charset: symbol!(b"FcPatternGetCharSet\0"),
            charset_create: symbol!(b"FcCharSetCreate\0"),
            charset_destroy: symbol!(b"FcCharSetDestroy\0"),
            charset_add_char: symbol!(b"FcCharSetAddChar\0"),
            charset_has_char: symbol!(b"FcCharSetHasChar\0"),
            config_substitute: symbol!(b"FcConfigSubstitute\0"),
            default_substitute: symbol!(b"FcDefaultSubstitute\0"),
            font_match: symbol!(b"FcFontMatch\0"),
            #[cfg(test)]
            config_create: symbol!(b"FcConfigCreate\0"),
            #[cfg(test)]
            config_app_font_add_dir: symbol!(b"FcConfigAppFontAddDir\0"),
            _library: library,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROBES: [char; 6] = [
        'A',
        '\u{4f60}',
        '\u{1f600}',
        '\u{2801}',
        '\u{e000}',
        '\u{10fffd}',
    ];

    fn carries(path: &std::path::Path, index: u32, character: char) -> Option<bool> {
        let data = std::fs::read(path).ok()?;
        let font = swash::FontRef::from_index(&data, index as usize)?;
        Some(font.charmap().map(character) != 0)
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    #[test]
    fn a_loader_that_resolves_nothing_falls_back_to_the_embedded_fonts() {
        assert!(
            fontconfig::Fontconfig::load_from("libfontconfig-there-is-no-such-library.so.0")
                .is_none(),
            "a soname that does not resolve produced a working fontconfig"
        );

        let mut fonts = crate::font::FontStack::embedded(crate::font::DEFAULT_FONT_SIZE).unwrap();
        assert!(
            !fonts.has_discovery() && fonts.lookup(FaceStyle::Regular, 'A').is_some(),
            "the embedded fonts stopped covering 'A' without system discovery"
        );
    }

    #[test]
    fn a_matched_font_is_a_readable_file() {
        let Some(discovery) = Discovery::open() else {
            eprintln!("fontconfig is unavailable here; discovery is untested");
            return;
        };
        let Some((path, _index)) = discovery.match_codepoint(FaceStyle::Regular, 'A') else {
            eprintln!("no installed font carries 'A'; discovery is untested");
            return;
        };
        assert!(path.is_file(), "{:?} is not a file", path);
        assert!(std::fs::read(&path).is_ok_and(|data| !data.is_empty()));
    }

    #[test]
    fn a_match_always_carries_the_codepoint_it_was_asked_for() {
        let Some(discovery) = Discovery::open() else {
            return;
        };
        for character in PROBES {
            let Some((path, index)) = discovery.match_codepoint(FaceStyle::Regular, character)
            else {
                continue;
            };
            assert_ne!(
                carries(&path, index, character),
                Some(false),
                "U+{:04X} was matched to {:?}, which does not carry it",
                character as u32,
                path
            );
        }
    }

    #[test]
    fn opening_discovery_twice_reuses_one_fontconfig() {
        let (Some(first), Some(second)) = (Discovery::open(), Discovery::open()) else {
            return;
        };
        assert!(
            std::ptr::eq(first.0, second.0),
            "fontconfig was loaded and rescanned a second time"
        );
    }

    fn embedded_font_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")
    }

    fn synthetic() -> Option<Discovery> {
        let discovery = Discovery::synthetic(&embedded_font_dir());
        if discovery.is_none() {
            eprintln!("fontconfig is unavailable here; family selection is untested");
        }
        discovery
    }

    #[test]
    fn a_requested_family_resolves_to_a_file_carrying_that_family() {
        let Some(discovery) = synthetic() else {
            return;
        };
        let (path, index) = discovery
            .match_family("Iosevka Term", FaceStyle::Regular)
            .expect("the embedded Iosevka was not found in a config holding only it");
        assert!(path.starts_with(embedded_font_dir()), "{:?}", path);
        let data = std::fs::read(&path).unwrap();
        let font = swash::FontRef::from_index(&data, index as usize).unwrap();
        assert!(
            font.localized_strings()
                .any(|string| string.to_string().eq_ignore_ascii_case("Iosevka Term")),
            "{:?} does not name itself Iosevka Term",
            path
        );
    }

    #[test]
    fn a_family_that_is_not_installed_resolves_to_nothing() {
        let Some(discovery) = synthetic() else {
            return;
        };
        assert_eq!(
            discovery.match_family("No Such Family At All", FaceStyle::Regular),
            None,
            "fontconfig's best guess was taken for the family that was asked for"
        );
    }

    #[test]
    fn a_requested_family_answers_every_style() {
        let Some(discovery) = synthetic() else {
            return;
        };
        for style in [
            FaceStyle::Regular,
            FaceStyle::Bold,
            FaceStyle::Italic,
            FaceStyle::BoldItalic,
        ] {
            assert!(
                discovery.match_family("Iosevka Term", style).is_some(),
                "{:?} did not resolve",
                style
            );
        }
    }

    #[test]
    fn every_style_is_matched_without_crashing() {
        let Some(discovery) = Discovery::open() else {
            return;
        };
        for style in [
            FaceStyle::Regular,
            FaceStyle::Bold,
            FaceStyle::Italic,
            FaceStyle::BoldItalic,
        ] {
            if let Some((path, index)) = discovery.match_codepoint(style, 'x') {
                assert_ne!(
                    carries(&path, index, 'x'),
                    Some(false),
                    "{:?} was matched to {:?}, which has no 'x'",
                    style,
                    path
                );
            }
        }
    }
}
