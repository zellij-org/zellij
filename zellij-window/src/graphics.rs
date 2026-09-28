use anyhow::{anyhow, Result};
use glutin::config::Config;
use glutin::context::{
    ContextApi, ContextAttributes, ContextAttributesBuilder, GlProfile, NotCurrentContext, Version,
};
use glutin::display::Display;
use glutin::prelude::*;
use winit::raw_window_handle::RawWindowHandle;

use crate::platform::Platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextRequest {
    Gles { major: u8, minor: u8 },
    Core { major: u8, minor: u8 },
}

impl ContextRequest {
    fn attributes(self, handle: RawWindowHandle) -> ContextAttributes {
        match self {
            ContextRequest::Gles { major, minor } => ContextAttributesBuilder::new()
                .with_context_api(ContextApi::Gles(Some(Version::new(major, minor))))
                .build(Some(handle)),
            ContextRequest::Core { major, minor } => ContextAttributesBuilder::new()
                .with_context_api(ContextApi::OpenGl(Some(Version::new(major, minor))))
                .with_profile(GlProfile::Core)
                .build(Some(handle)),
        }
    }

    fn describe(self) -> String {
        match self {
            ContextRequest::Gles { major, minor } => format!("OpenGL ES {}.{}", major, minor),
            ContextRequest::Core { major, minor } => format!("OpenGL {}.{} core", major, minor),
        }
    }
}

pub fn context_attempts(platform: Platform) -> &'static [ContextRequest] {
    match platform {
        Platform::Linux => &[ContextRequest::Gles { major: 3, minor: 0 }],
        Platform::MacOs => &[
            ContextRequest::Core { major: 4, minor: 1 },
            ContextRequest::Core { major: 3, minor: 3 },
        ],
        Platform::Windows => &[
            ContextRequest::Core { major: 3, minor: 3 },
            ContextRequest::Gles { major: 3, minor: 0 },
        ],
    }
}

pub fn create_context(
    display: &Display,
    config: &Config,
    handle: RawWindowHandle,
    platform: Platform,
) -> Result<NotCurrentContext> {
    let mut refusals = Vec::new();
    for request in context_attempts(platform) {
        match unsafe { display.create_context(config, &request.attributes(handle)) } {
            Ok(context) => {
                if platform != Platform::Linux {
                    eprintln!("zellij-window: drawing through {}", request.describe());
                }
                return Ok(context);
            },
            Err(e) => refusals.push(format!("{}: {}", request.describe(), e)),
        }
    }
    Err(anyhow!(
        "failed to create a graphics context ({})",
        refusals.join("; ")
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderDialect {
    Gles300,
    Glsl330,
    Glsl410,
}

impl ShaderDialect {
    pub fn for_context(embedded: bool, major: u32, minor: u32) -> Result<Self> {
        let version = (major, minor);
        match (embedded, version) {
            (true, version) if version >= (3, 0) => Ok(ShaderDialect::Gles300),
            (false, version) if version >= (4, 1) => Ok(ShaderDialect::Glsl410),
            (false, version) if version >= (3, 3) => Ok(ShaderDialect::Glsl330),
            _ => Err(anyhow!(
                "the {} {}.{} context is too old: the window needs OpenGL ES 3.0 or OpenGL 3.3 core",
                if embedded { "OpenGL ES" } else { "OpenGL" },
                major,
                minor
            )),
        }
    }

    pub fn header(self) -> &'static str {
        match self {
            ShaderDialect::Gles300 => "#version 300 es\nprecision highp float;\n",
            ShaderDialect::Glsl330 => "#version 330 core\n",
            ShaderDialect::Glsl410 => "#version 410 core\n",
        }
    }

    pub fn source(self, body: &str) -> String {
        format!("{}{}", self.header(), body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::renderer::{GLYPH_VERTEX as GLYPH, SOLID_FRAGMENT as SOLID};

    #[test]
    fn linux_asks_for_exactly_the_es_context_the_goldens_were_drawn_with() {
        assert_eq!(
            context_attempts(Platform::Linux),
            &[ContextRequest::Gles { major: 3, minor: 0 }]
        );
    }

    #[test]
    fn macos_asks_only_for_desktop_core_contexts_newest_first() {
        assert_eq!(
            context_attempts(Platform::MacOs),
            &[
                ContextRequest::Core { major: 4, minor: 1 },
                ContextRequest::Core { major: 3, minor: 3 },
            ]
        );
    }

    #[test]
    fn windows_asks_for_desktop_core_first_and_es_only_as_a_last_resort() {
        assert_eq!(
            context_attempts(Platform::Windows),
            &[
                ContextRequest::Core { major: 3, minor: 3 },
                ContextRequest::Gles { major: 3, minor: 0 },
            ]
        );
    }

    #[test]
    fn every_context_the_window_asks_for_has_a_shader_dialect() {
        for platform in [Platform::Linux, Platform::MacOs, Platform::Windows] {
            for request in context_attempts(platform) {
                let (embedded, major, minor) = match *request {
                    ContextRequest::Gles { major, minor } => (true, major, minor),
                    ContextRequest::Core { major, minor } => (false, major, minor),
                };
                assert!(
                    ShaderDialect::for_context(embedded, major as u32, minor as u32).is_ok(),
                    "{:?} on {:?}",
                    request,
                    platform
                );
            }
        }
    }

    #[test]
    fn an_es_context_gets_the_shaders_exactly_as_they_were_before_the_dialects() {
        let dialect = ShaderDialect::for_context(true, 3, 2).unwrap();
        assert_eq!(dialect, ShaderDialect::Gles300);
        assert_eq!(
            dialect.source(SOLID),
            "#version 300 es\nprecision highp float;\nin vec3 v_color;\nout vec4 o_color;\n\
             void main() {\n    o_color = vec4(v_color, 1.0);\n}\n"
        );
    }

    #[test]
    fn a_desktop_context_gets_the_core_version_it_supports() {
        assert_eq!(
            ShaderDialect::for_context(false, 3, 3).unwrap(),
            ShaderDialect::Glsl330
        );
        assert_eq!(
            ShaderDialect::for_context(false, 4, 0).unwrap(),
            ShaderDialect::Glsl330
        );
        assert_eq!(
            ShaderDialect::for_context(false, 4, 1).unwrap(),
            ShaderDialect::Glsl410
        );
        assert_eq!(
            ShaderDialect::for_context(false, 4, 6).unwrap(),
            ShaderDialect::Glsl410
        );
        assert!(ShaderDialect::Glsl330
            .source(GLYPH)
            .starts_with("#version 330 core\nuniform vec2 u_viewport;"));
        assert!(ShaderDialect::Glsl410
            .source(GLYPH)
            .starts_with("#version 410 core\nuniform vec2 u_viewport;"));
    }

    #[test]
    fn a_context_too_old_for_the_shaders_is_refused_by_name() {
        let err = ShaderDialect::for_context(false, 2, 1)
            .unwrap_err()
            .to_string();
        assert!(err.contains("OpenGL 2.1"), "{}", err);
        assert!(ShaderDialect::for_context(false, 3, 2).is_err());
        assert!(ShaderDialect::for_context(true, 2, 0).is_err());
    }
}
