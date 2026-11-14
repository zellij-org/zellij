use crate::render::atlas::GlyphAtlas;
use anyhow::{anyhow, Result};
use glow::HasContext;

pub(super) struct AtlasTexture {
    pub(super) texture: glow::Texture,
    pub(super) size: (u32, u32),
    pub(super) uploaded: Option<crate::render::atlas::AtlasStamp>,
    pub(super) format: u32,
    pub(super) internal_format: i32,
}

pub(super) unsafe fn atlas_texture(
    gl: &glow::Context,
    format: u32,
    internal_format: i32,
) -> Result<AtlasTexture> {
    let texture = gl.create_texture().map_err(|e| anyhow!(e))?;
    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
    for (parameter, value) in [
        (glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32),
        (glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32),
        (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32),
        (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32),
    ] {
        gl.tex_parameter_i32(glow::TEXTURE_2D, parameter, value);
    }

    Ok(AtlasTexture {
        texture,
        size: (0, 0),
        uploaded: None,
        format,
        internal_format,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AtlasUpload {
    Nothing,
    Rows { top: u32, bottom: u32 },
    Whole,
}

pub(super) fn plan_atlas_upload(
    uploaded: Option<crate::render::atlas::AtlasStamp>,
    uploaded_size: (u32, u32),
    atlas: &GlyphAtlas,
) -> AtlasUpload {
    let size = (atlas.width(), atlas.height());
    if size.1 == 0 {
        return AtlasUpload::Nothing;
    }
    let Some(uploaded) = uploaded else {
        return AtlasUpload::Whole;
    };
    if size != uploaded_size || uploaded.identity != atlas.stamp().identity {
        return AtlasUpload::Whole;
    }
    let revision = uploaded.revision;
    if revision == atlas.revision() {
        return AtlasUpload::Nothing;
    }
    match atlas.dirty_rows_since(revision) {
        Some((top, bottom)) if bottom > top => AtlasUpload::Rows { top, bottom },
        Some(_) => AtlasUpload::Nothing,
        None => AtlasUpload::Rows {
            top: 0,
            bottom: size.1,
        },
    }
}

pub(super) fn upload_atlas(gl: &glow::Context, target: &mut AtlasTexture, atlas: &GlyphAtlas) {
    let size = (atlas.width(), atlas.height());
    let plan = plan_atlas_upload(target.uploaded, target.size, atlas);
    unsafe {
        match plan {
            AtlasUpload::Nothing => {},
            AtlasUpload::Rows { top, bottom } => {
                gl.bind_texture(glow::TEXTURE_2D, Some(target.texture));
                let row_bytes = (size.0 * atlas.channels()) as usize;
                let rows = &atlas.coverage()[top as usize * row_bytes..bottom as usize * row_bytes];
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    0,
                    top as i32,
                    size.0 as i32,
                    (bottom - top) as i32,
                    target.format,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(rows)),
                );
            },
            AtlasUpload::Whole => {
                gl.bind_texture(glow::TEXTURE_2D, Some(target.texture));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    target.internal_format,
                    size.0 as i32,
                    size.1 as i32,
                    0,
                    target.format,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(atlas.coverage())),
                );
            },
        }
    }
    target.size = size;
    target.uploaded = Some(atlas.stamp());
}
