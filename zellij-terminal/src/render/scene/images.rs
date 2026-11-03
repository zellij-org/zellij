use crate::emulation::terminal::TerminalState;
use crate::render::scene::ImageKey;
use crate::render::scene::ImageQuad;
use crate::render::scene::PixelRect;
use crate::text::font::CellMetrics;
use std::rc::Rc;
use zellij_utils::structured_render::GeometryRecord;

pub fn resident_images(state: &TerminalState) -> Vec<ImageKey> {
    let mut resident: Vec<ImageKey> = state
        .sixels()
        .resident_ids()
        .into_iter()
        .map(ImageKey::Sixel)
        .collect();
    resident.extend(
        state
            .graphics()
            .resident_ids()
            .into_iter()
            .map(ImageKey::Kitty),
    );
    resident
}

pub fn image_area(image: &ImageQuad) -> PixelRect {
    PixelRect {
        left: image.x,
        top: image.y,
        right: image.x.saturating_add(image.width as i32),
        bottom: image.y.saturating_add(image.height as i32),
    }
}

pub fn clip_to_owning_pane(
    quad: ImageQuad,
    cell_x: usize,
    cell_y: usize,
    geometry: &GeometryRecord,
    metrics: CellMetrics,
) -> Option<ImageQuad> {
    let (Ok(cell_x), Ok(cell_y)) = (u16::try_from(cell_x), u16::try_from(cell_y)) else {
        return Some(quad);
    };
    let Some(pane) = geometry.content_pane_at(cell_x, cell_y) else {
        return Some(quad);
    };
    let left = pane.content_x() as i64 * metrics.width as i64;
    let top = pane.content_y() as i64 * metrics.height as i64;
    let right = left + pane.content_cols() as i64 * metrics.width as i64;
    let bottom = top + pane.content_rows() as i64 * metrics.height as i64;

    let kept_left = (quad.x as i64).max(left);
    let kept_top = (quad.y as i64).max(top);
    let kept_right = (quad.x as i64 + quad.width as i64).min(right);
    let kept_bottom = (quad.y as i64 + quad.height as i64).min(bottom);
    if kept_right <= kept_left || kept_bottom <= kept_top {
        return None;
    }
    Some(ImageQuad {
        x: kept_left as i32,
        y: kept_top as i32,
        width: (kept_right - kept_left) as u32,
        height: (kept_bottom - kept_top) as u32,
        source_x: quad.source_x + (kept_left - quad.x as i64) as u32,
        source_y: quad.source_y + (kept_top - quad.y as i64) as u32,
        ..quad
    })
}

pub fn images_of(state: &TerminalState, metrics: CellMetrics) -> Vec<ImageQuad> {
    let graphics = state.graphics();
    let geometry = state.geometry();
    let mut quads: Vec<ImageQuad> = state
        .sixels()
        .chunks()
        .filter_map(|chunk| {
            clip_to_owning_pane(
                ImageQuad {
                    key: ImageKey::Sixel(chunk.id),
                    revision: chunk.revision,
                    x: (chunk.cell_x as u32 * metrics.width) as i32,
                    y: (chunk.cell_y as u32 * metrics.height) as i32,
                    width: chunk.width,
                    height: chunk.height,
                    source_x: 0,
                    source_y: 0,
                    z: 0,
                    image: Rc::clone(&chunk.image),
                },
                chunk.cell_x,
                chunk.cell_y,
                geometry,
                metrics,
            )
        })
        .collect();
    quads.extend(graphics.placements().filter_map(|placement| {
        let image = graphics.image(placement.image_id)?;
        let revision = graphics.revision(placement.image_id)?;
        clip_to_owning_pane(
            ImageQuad {
                key: ImageKey::Kitty(placement.image_id),
                revision,
                x: (placement.cell_x as u32 * metrics.width + placement.offset_x) as i32,
                y: (placement.cell_y as u32 * metrics.height + placement.offset_y) as i32,
                width: placement.source_width,
                height: placement.source_height,
                source_x: placement.source_x,
                source_y: placement.source_y,
                z: placement.z,
                image: Rc::clone(image),
            },
            placement.cell_x,
            placement.cell_y,
            geometry,
            metrics,
        )
    }));
    quads.sort_by_key(|quad| (quad.z, quad.key));
    quads
}
