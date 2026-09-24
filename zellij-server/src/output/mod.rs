use std::collections::VecDeque;

use base64::engine::general_purpose::STANDARD as BASE64_ENCODER;
use base64::engine::Engine as _;

use crate::panes::Row;

use crate::panes::Selection;
use crate::{
    panes::kitty_graphics::store::{InternalImageId, KittyImageStore},
    panes::sixel::SixelImageStore,
    panes::terminal_character::{AnsiCode, AnsiStyledUnderline, CharacterStyles, NamedColor},
    panes::{LinkHandler, PaneId, TerminalCharacter, DEFAULT_STYLES, EMPTY_TERMINAL_CHARACTER},
    ClientId,
};
use std::cell::RefCell;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    str,
};
use zellij_utils::data::{HighlightLayer, PaneContents, PaneRenderReport};
use zellij_utils::errors::prelude::*;
use zellij_utils::pane_size::SizeInPixels;
use zellij_utils::pane_size::{PaneGeom, Size};
use zellij_utils::structured_render::{
    CursorState, FrameBuilder, GeometryRecord, GraphicsMedium, GraphicsRecord, LinkEntry,
    LinkRecord, PaneRect, PendingOverlay, WireCell, WireColor, ATTR_BOLD, ATTR_DIM,
    ATTR_FAST_BLINK, ATTR_HIDDEN, ATTR_ITALIC, ATTR_REVERSE, ATTR_SLOW_BLINK, ATTR_STRIKE,
    CURSOR_BLINKING, CURSOR_SHAPE_BEAM, CURSOR_SHAPE_BLOCK, CURSOR_SHAPE_DEFAULT,
    CURSOR_SHAPE_UNDERLINE, GRAPHICS_FORMAT_RGBA8, LINK_NONE, MAX_LINK_ID, MAX_LINK_URI_LEN,
    UNDERLINE_CURLY, UNDERLINE_DASHED, UNDERLINE_DOTTED, UNDERLINE_DOUBLE, UNDERLINE_NONE,
    UNDERLINE_SHIFT, UNDERLINE_STRAIGHT,
};

const KITTY_MEDIA_PREFIX: &str = "zellij-tty-graphics-protocol";
const SHM_DIR: &str = "/dev/shm";
const MAX_TRACKED_MEDIA_FILES: usize = 512;
static NEXT_MEDIA_FILE: AtomicU64 = AtomicU64::new(0);

fn vte_goto_instruction(x_coords: usize, y_coords: usize, vte_output: &mut String) -> Result<()> {
    write!(
        vte_output,
        "\u{1b}[{};{}H\u{1b}[m",
        y_coords + 1, // + 1 because VTE is 1 indexed
        x_coords + 1,
    )
    .with_context(|| {
        format!(
            "failed to execute VTE instruction to go to ({}, {})",
            x_coords, y_coords
        )
    })
}

fn vte_hide_cursor_instruction(vte_output: &mut String) -> Result<()> {
    write!(vte_output, "\u{1b}[?25l").context("failed to execute VTE instruction to hide cursor")
}

/// A selection region with associated styling for highlights and text selection.
#[derive(Debug, Clone, Copy)]
pub struct HighlightSelection {
    pub selection: Selection,
    pub bg: Option<AnsiCode>,
    pub fg: Option<AnsiCode>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub layer: HighlightLayer,
}

fn adjust_styles_for_possible_selection(
    chunk_selection_and_colors: &[HighlightSelection],
    character_styles: CharacterStyles,
    chunk_y: usize,
    chunk_width: usize,
) -> CharacterStyles {
    chunk_selection_and_colors
        .iter()
        .find(|hs| hs.selection.contains(chunk_y, chunk_width))
        .map(|hs| {
            let mut styles = character_styles;
            if let Some(bg) = hs.bg {
                styles = styles.background(Some(bg));
            }
            if let Some(fg) = hs.fg {
                styles = styles.foreground(Some(fg));
            }
            if hs.bold {
                styles = styles.bold(Some(AnsiCode::On));
            }
            if hs.italic {
                styles = styles.italic(Some(AnsiCode::On));
            }
            if hs.underline {
                styles = styles.underline(Some(AnsiCode::Underline(None)));
            }
            styles
        })
        .unwrap_or(character_styles)
}

fn adjust_styles_for_custom_bg_fg(
    character_styles: CharacterStyles,
    pane_default_fg: Option<AnsiCode>,
    pane_default_bg: Option<AnsiCode>,
) -> CharacterStyles {
    let mut character_styles = character_styles;
    if character_styles.foreground.is_none() || character_styles.foreground == Some(AnsiCode::Reset)
    {
        if let Some(fg) = pane_default_fg {
            character_styles.foreground = Some(fg);
        }
    }
    if character_styles.background.is_none() || character_styles.background == Some(AnsiCode::Reset)
    {
        if let Some(bg) = pane_default_bg {
            character_styles.background = Some(bg);
        }
    }
    character_styles
}

fn write_changed_styles(
    character_styles: &mut CharacterStyles,
    current_character_styles: CharacterStyles,
    chunk_changed_colors: Option<[Option<AnsiCode>; 256]>,
    link_handler: Option<&std::cell::Ref<LinkHandler>>,
    osc8_hyperlinks: bool,
    vte_output: &mut String,
) -> Result<()> {
    let err_context = "failed to format changed styles to VTE string";

    if let Some(new_styles) =
        character_styles.update_and_return_diff(&current_character_styles, chunk_changed_colors)
    {
        if osc8_hyperlinks {
            if let Some(osc8_link) =
                link_handler.and_then(|l_h| l_h.output_osc8(new_styles.link_anchor))
            {
                write!(vte_output, "{}{}", new_styles, osc8_link).context(err_context)?;
            } else {
                write!(vte_output, "{}", new_styles).context(err_context)?;
            }
        } else {
            write!(vte_output, "{}", new_styles).context(err_context)?;
        }
    }
    Ok(())
}

fn serialize_chunks_with_newlines(
    character_chunks: Vec<CharacterChunk>,
    chunk_starts_a_line: &[bool],
    _sixel_chunks: Option<&Vec<SixelImageChunk>>, // TODO: fix this sometime
    link_handler: Option<&mut Rc<RefCell<LinkHandler>>>,
    styled_underlines: bool,
    osc8_hyperlinks: bool,
    max_size: Option<Size>,
) -> Result<String> {
    let err_context = || "failed to serialize input chunks".to_string();

    let mut vte_output = String::new();
    let link_handler = link_handler.map(|l_h| l_h.borrow());
    for (chunk_index, character_chunk) in character_chunks.into_iter().enumerate() {
        // Skip chunks that are completely outside the size bounds
        if let Some(size) = max_size {
            if character_chunk.y >= size.rows {
                continue; // Chunk is below visible area
            }
            if character_chunk.x >= size.cols {
                continue; // Chunk starts outside visible area
            }
        }

        let chunk_changed_colors = character_chunk.changed_colors();
        let pane_default_fg = character_chunk.pane_default_fg;
        let pane_default_bg = character_chunk.pane_default_bg;
        let mut character_styles = DEFAULT_STYLES.enable_styled_underlines(styled_underlines);
        if chunk_starts_a_line
            .get(chunk_index)
            .copied()
            .unwrap_or(true)
        {
            vte_output.push_str("\n\r");
        }
        let mut chunk_width = character_chunk.x;
        for t_character in character_chunk.terminal_characters.iter() {
            // Stop rendering if the next character would exceed max_size.cols
            if let Some(size) = max_size {
                if chunk_width + t_character.width() > size.cols {
                    break; // Stop rendering this chunk
                }
            }

            let current_character_styles = adjust_styles_for_custom_bg_fg(
                adjust_styles_for_possible_selection(
                    character_chunk.selection_and_colors(),
                    *t_character.styles,
                    character_chunk.y,
                    chunk_width,
                ),
                pane_default_fg,
                pane_default_bg,
            );
            write_changed_styles(
                &mut character_styles,
                current_character_styles,
                chunk_changed_colors,
                link_handler.as_ref(),
                osc8_hyperlinks,
                &mut vte_output,
            )
            .with_context(err_context)?;
            chunk_width += t_character.width();
            vte_output.push(t_character.character);
        }
    }
    Ok(vte_output)
}
fn write_shared_media(rgba: &[u8], host_state: &mut HostKittyState) -> Option<PathBuf> {
    let serial = NEXT_MEDIA_FILE.fetch_add(1, Ordering::Relaxed);
    let file_name = format!("{}-{}-{}", KITTY_MEDIA_PREFIX, std::process::id(), serial);
    let directory = match Path::new(SHM_DIR).is_dir() {
        true => PathBuf::from(SHM_DIR),
        false => std::env::temp_dir(),
    };
    let path = directory.join(file_name);
    if let Err(e) = fs::write(&path, rgba) {
        log::warn!(
            "failed to write structured graphics media to {:?}, carrying it inline: {}",
            path,
            e
        );
        let _ = fs::remove_file(&path);
        return None;
    }
    host_state.track_media(path.clone());
    Some(path)
}

fn residency_record(
    host_image_id: u32,
    width: usize,
    height: usize,
    rgba: &[u8],
    image_key: (InternalImageId, Option<(u16, u16)>),
    local_media: bool,
    host_state: &mut HostKittyState,
) -> GraphicsRecord {
    let shared = local_media
        .then(|| write_shared_media(rgba, host_state))
        .flatten();
    match shared {
        Some(path) => {
            host_state.track_media_for_image(image_key, path.clone());
            GraphicsRecord::Residency {
                id: host_image_id as u64,
                width: width as u32,
                height: height as u32,
                format: GRAPHICS_FORMAT_RGBA8,
                medium: GraphicsMedium::SharedFile,
                byte_len: rgba.len() as u32,
                data: path.to_string_lossy().into_owned().into_bytes(),
            }
        },
        None => GraphicsRecord::Residency {
            id: host_image_id as u64,
            width: width as u32,
            height: height as u32,
            format: GRAPHICS_FORMAT_RGBA8,
            medium: GraphicsMedium::Inline,
            byte_len: rgba.len() as u32,
            data: rgba.to_vec(),
        },
    }
}

fn flatten_kitty_frame(kitty_input: KittyFrameInput, out: &mut Vec<GraphicsRecord>) {
    let KittyFrameInput {
        mut chunks_by_pane,
        rendered_panes,
        visible_panes,
        kitty_image_store,
        host_state,
        host_display_cleared,
        local_media,
    } = kitty_input;

    if host_display_cleared {
        host_state.live_placements.clear();
        for image_key in host_state.transmitted.keys().copied().collect::<Vec<_>>() {
            host_state.release_media_for_image(&image_key);
        }
        host_state.transmitted.clear();
        out.push(GraphicsRecord::Clear);
    }

    let mut freed: Vec<((InternalImageId, Option<(u16, u16)>), u32)> = host_state
        .transmitted
        .iter()
        .filter(|(image_key, _)| kitty_image_store.get(image_key.0).is_none())
        .map(|(image_key, host_id)| (*image_key, *host_id))
        .collect();
    freed.sort_by_key(|(_, host_id)| *host_id);
    for (image_key, host_id) in freed {
        out.push(GraphicsRecord::DeleteImage { id: host_id as u64 });
        host_state.transmitted.remove(&image_key);
        host_state.release_media_for_image(&image_key);
        host_state
            .live_placements
            .retain(|_, record| record.image_key != image_key);
    }

    if let Some(visible_panes) = visible_panes {
        let mut to_delete: Vec<(KittyChunkKey, u32, u32)> = host_state
            .live_placements
            .iter()
            .filter(|(key, _)| !visible_panes.contains(&key.pane_id))
            .map(|(key, record)| (key.clone(), record.host_image_id, record.host_placement_id))
            .collect();
        to_delete.sort_by_key(|(key, _, _)| {
            (pane_sort_key(key.pane_id), key.placement_uid, key.sub_index)
        });
        for (key, host_image_id, host_placement_id) in to_delete {
            out.push(GraphicsRecord::DeletePlacement {
                image_id: host_image_id as u64,
                placement_id: host_placement_id as u64,
            });
            host_state.live_placements.remove(&key);
        }
    }

    let mut rendered: Vec<PaneId> = rendered_panes.into_iter().collect();
    rendered.sort_by_key(|pane_id| pane_sort_key(*pane_id));
    for pane_id in rendered {
        let chunks = chunks_by_pane.remove(&pane_id).unwrap_or_default();
        let mut by_uid: BTreeMap<u64, Vec<KittyImageChunk>> = BTreeMap::new();
        for chunk in chunks {
            by_uid.entry(chunk.placement_uid).or_default().push(chunk);
        }
        let mut current: BTreeMap<KittyChunkKey, KittyImageChunk> = BTreeMap::new();
        for (placement_uid, mut group) in by_uid {
            group.sort_by_key(|chunk| (chunk.cell_y, chunk.cell_x));
            for (sub_index, chunk) in group.into_iter().enumerate() {
                current.insert(
                    KittyChunkKey {
                        pane_id,
                        placement_uid,
                        sub_index: sub_index as u32,
                    },
                    chunk,
                );
            }
        }
        let mut stale: Vec<(KittyChunkKey, u32, u32)> = host_state
            .live_placements
            .iter()
            .filter(|(key, _)| key.pane_id == pane_id && !current.contains_key(key))
            .map(|(key, record)| (key.clone(), record.host_image_id, record.host_placement_id))
            .collect();
        stale.sort_by_key(|(key, _, _)| (key.placement_uid, key.sub_index));
        for (key, host_image_id, host_placement_id) in stale {
            out.push(GraphicsRecord::DeletePlacement {
                image_id: host_image_id as u64,
                placement_id: host_placement_id as u64,
            });
            host_state.live_placements.remove(&key);
        }

        for (key, chunk) in current {
            let variant = if chunk.scaled_px.is_some() {
                Some(chunk.dest_cells)
            } else {
                None
            };
            let image_key = (chunk.internal_image_id, variant);
            let host_image_id = match host_state.transmitted.get(&image_key) {
                Some(host_image_id) => *host_image_id,
                None => {
                    let raster_dims = match variant {
                        Some(cells) => {
                            match kitty_image_store.scaled_variant(chunk.internal_image_id, cells) {
                                Some(_) => chunk.scaled_px,
                                None => None,
                            }
                        },
                        None => kitty_image_store
                            .get(chunk.internal_image_id)
                            .map(|image| (image.width as usize, image.height as usize)),
                    };
                    let Some((width, height)) = raster_dims else {
                        continue;
                    };
                    let rgba = match variant {
                        Some(cells) => {
                            kitty_image_store.scaled_variant(chunk.internal_image_id, cells)
                        },
                        None => kitty_image_store
                            .get(chunk.internal_image_id)
                            .map(|image| image.rgba.as_slice()),
                    };
                    let Some(rgba) = rgba else {
                        continue;
                    };
                    let host_image_id = host_state.next_host_image_id;
                    out.push(residency_record(
                        host_image_id,
                        width,
                        height,
                        rgba,
                        image_key,
                        local_media,
                        host_state,
                    ));
                    host_state.next_host_image_id += 1;
                    host_state.transmitted.insert(image_key, host_image_id);
                    host_image_id
                },
            };
            let existing = host_state.live_placements.get(&key).map(|record| {
                (
                    record.host_placement_id,
                    record.geometry,
                    record.host_image_id,
                )
            });
            let host_placement_id = match existing {
                Some((_, geometry, existing_host_image_id))
                    if geometry == chunk && existing_host_image_id == host_image_id =>
                {
                    continue;
                },
                Some((host_placement_id, _, _)) => host_placement_id,
                None => {
                    let host_placement_id = host_state.next_host_placement_id;
                    host_state.next_host_placement_id += 1;
                    host_placement_id
                },
            };
            out.push(GraphicsRecord::Placement {
                image_id: host_image_id as u64,
                placement_id: host_placement_id as u64,
                cell_x: chunk.cell_x as u32,
                cell_y: chunk.cell_y as u32,
                offset_x: chunk.cell_offset_x as u32,
                offset_y: chunk.cell_offset_y as u32,
                source_x: chunk.source_px_x as u32,
                source_y: chunk.source_px_y as u32,
                source_width: chunk.source_px_width as u32,
                source_height: chunk.source_px_height as u32,
                z: chunk.z_index,
            });
            host_state.live_placements.insert(
                key,
                HostPlacementRecord {
                    host_image_id,
                    host_placement_id,
                    image_key,
                    geometry: chunk,
                },
            );
        }
    }
}

fn wire_color(code: Option<AnsiCode>) -> u32 {
    match code {
        Some(AnsiCode::NamedColor(named)) => WireColor::Named(named_color_index(named)).pack(),
        Some(AnsiCode::ColorIndex(index)) => WireColor::Indexed(index).pack(),
        Some(AnsiCode::RgbCode((r, g, b))) => WireColor::Rgb(r, g, b).pack(),
        _ => WireColor::Default.pack(),
    }
}

fn named_color_index(named: NamedColor) -> u8 {
    match named {
        NamedColor::Black => 0,
        NamedColor::Red => 1,
        NamedColor::Green => 2,
        NamedColor::Yellow => 3,
        NamedColor::Blue => 4,
        NamedColor::Magenta => 5,
        NamedColor::Cyan => 6,
        NamedColor::White => 7,
        NamedColor::BrightBlack => 8,
        NamedColor::BrightRed => 9,
        NamedColor::BrightGreen => 10,
        NamedColor::BrightYellow => 11,
        NamedColor::BrightBlue => 12,
        NamedColor::BrightMagenta => 13,
        NamedColor::BrightCyan => 14,
        NamedColor::BrightWhite => 15,
    }
}

fn substitute_changed_color(
    code: Option<AnsiCode>,
    changed_colors: Option<[Option<AnsiCode>; 256]>,
) -> Option<AnsiCode> {
    match (code, changed_colors) {
        (Some(AnsiCode::ColorIndex(index)), Some(changed_colors)) => {
            changed_colors[index as usize].or(code)
        },
        _ => code,
    }
}

fn wire_attrs(styles: &CharacterStyles, styled_underlines: bool) -> u16 {
    let mut attrs = 0u16;
    for (code, bit) in [
        (styles.bold, ATTR_BOLD),
        (styles.dim, ATTR_DIM),
        (styles.italic, ATTR_ITALIC),
        (styles.reverse, ATTR_REVERSE),
        (styles.hidden, ATTR_HIDDEN),
        (styles.strike, ATTR_STRIKE),
        (styles.slow_blink, ATTR_SLOW_BLINK),
        (styles.fast_blink, ATTR_FAST_BLINK),
    ] {
        if matches!(code, Some(AnsiCode::On)) {
            attrs |= bit;
        }
    }
    let underline = match styles.underline {
        Some(AnsiCode::Underline(None)) => UNDERLINE_STRAIGHT,
        Some(AnsiCode::Underline(Some(styled))) if styled_underlines => match styled {
            AnsiStyledUnderline::Double => UNDERLINE_DOUBLE,
            AnsiStyledUnderline::Undercurl => UNDERLINE_CURLY,
            AnsiStyledUnderline::Underdotted => UNDERLINE_DOTTED,
            AnsiStyledUnderline::Underdashed => UNDERLINE_DASHED,
        },
        Some(AnsiCode::Underline(Some(_))) => UNDERLINE_STRAIGHT,
        _ => UNDERLINE_NONE,
    };
    attrs |= underline << UNDERLINE_SHIFT;
    attrs
}

fn wire_cell(
    t_character: &TerminalCharacter,
    styles: CharacterStyles,
    chunk_changed_colors: Option<[Option<AnsiCode>; 256]>,
    styled_underlines: bool,
) -> WireCell {
    WireCell {
        ch: t_character.character as u32,
        fg: wire_color(substitute_changed_color(
            styles.foreground,
            chunk_changed_colors,
        )),
        bg: wire_color(substitute_changed_color(
            styles.background,
            chunk_changed_colors,
        )),
        underline_color: if styled_underlines {
            wire_color(styles.underline_color)
        } else {
            WireColor::Default.pack()
        },
        attrs: wire_attrs(&styles, styled_underlines),
        width: t_character.width().min(2) as u8,
        link: LINK_NONE,
    }
}

pub struct LinkFlatten<'a> {
    pub handler: &'a LinkHandler,
    pub table: &'a mut LinkTable,
}

#[allow(clippy::too_many_arguments)]
fn flatten_chunks(
    character_chunks: Vec<CharacterChunk>,
    sixel_chunks: Option<&Vec<SixelImageChunk>>,
    sixel_image_store: Option<&mut SixelImageStore>,
    styled_underlines: bool,
    kitty_input: Option<KittyFrameInput>,
    mut links: Option<&mut LinkFlatten<'_>>,
    builder: &mut FrameBuilder,
) -> Result<()> {
    let err_context = || "failed to flatten input chunks".to_string();

    let cols = builder.cols() as usize;
    let rows = builder.rows() as usize;
    let mut run: Vec<WireCell> = Vec::new();
    for character_chunk in character_chunks {
        if character_chunk.y >= rows || character_chunk.x >= cols {
            continue;
        }
        let chunk_changed_colors = character_chunk.changed_colors();
        let pane_default_fg = character_chunk.pane_default_fg;
        let pane_default_bg = character_chunk.pane_default_bg;
        run.clear();
        let mut chunk_width = character_chunk.x;
        for t_character in character_chunk.terminal_characters.iter() {
            if chunk_width + t_character.width() > cols {
                break;
            }
            let current_character_styles = adjust_styles_for_custom_bg_fg(
                adjust_styles_for_possible_selection(
                    character_chunk.selection_and_colors(),
                    *t_character.styles,
                    character_chunk.y,
                    chunk_width,
                ),
                pane_default_fg,
                pane_default_bg,
            );
            let mut cell = wire_cell(
                t_character,
                current_character_styles,
                chunk_changed_colors,
                styled_underlines,
            );
            if let Some(links) = links.as_deref_mut() {
                if let Some(uri) = links.handler.uri(t_character.styles.link_anchor) {
                    cell.link = links.table.id_for(uri, cols, rows);
                }
            }
            chunk_width += t_character.width();
            run.push(cell);
            for _ in 1..t_character.width() {
                run.push(WireCell {
                    ch: ' ' as u32,
                    width: 1,
                    ..cell
                });
            }
        }
        builder.push_row(character_chunk.y as u16, character_chunk.x as u16, &run);
        if let Some(links) = links.as_deref_mut() {
            let width = run.len().min(cols - character_chunk.x);
            links
                .table
                .note_row(character_chunk.y, character_chunk.x, &run[..width]);
        }
    }

    if let Some(links) = links.as_deref_mut() {
        builder.push_links(&links.table.take_fresh());
        links.table.reclaim();
    }

    let mut records: Vec<GraphicsRecord> = Vec::new();
    if let Some(kitty_input) = kitty_input {
        flatten_kitty_frame(kitty_input, &mut records);
    }
    if let Some(sixel_image_store) = sixel_image_store {
        if let Some(sixel_chunks) = sixel_chunks {
            for sixel_chunk in sixel_chunks {
                if sixel_chunk.cell_y >= rows || sixel_chunk.cell_x >= cols {
                    continue;
                }
                let serialized_sixel_image = sixel_image_store.serialize_image(
                    sixel_chunk.sixel_image_id,
                    sixel_chunk.sixel_image_pixel_x,
                    sixel_chunk.sixel_image_pixel_y,
                    sixel_chunk.sixel_image_pixel_width,
                    sixel_chunk.sixel_image_pixel_height,
                );
                if let Some(serialized_sixel_image) = serialized_sixel_image {
                    records.push(GraphicsRecord::SixelChunk {
                        cell_x: sixel_chunk.cell_x as u32,
                        cell_y: sixel_chunk.cell_y as u32,
                        pixel_x: sixel_chunk.sixel_image_pixel_x as u32,
                        pixel_y: sixel_chunk.sixel_image_pixel_y as u32,
                        pixel_width: sixel_chunk.sixel_image_pixel_width as u32,
                        pixel_height: sixel_chunk.sixel_image_pixel_height as u32,
                        payload: serialized_sixel_image.into_bytes(),
                    });
                }
            }
        }
    }
    builder.extend_graphics(records.iter());
    let _ = err_context;
    Ok(())
}

fn serialize_chunks(
    character_chunks: Vec<CharacterChunk>,
    sixel_chunks: Option<&Vec<SixelImageChunk>>,
    link_handler: Option<&mut Rc<RefCell<LinkHandler>>>,
    sixel_image_store: Option<&mut SixelImageStore>,
    styled_underlines: bool,
    osc8_hyperlinks: bool,
    max_size: Option<Size>,
    kitty_input: Option<KittyFrameInput>,
) -> Result<String> {
    let err_context = || "failed to serialize input chunks".to_string();

    let mut vte_output = String::new();
    let mut sixel_vte: Option<String> = None;
    let link_handler = link_handler.map(|l_h| l_h.borrow());
    for character_chunk in character_chunks {
        // Skip chunks that are completely outside the size bounds
        if let Some(size) = max_size {
            if character_chunk.y >= size.rows {
                continue; // Chunk is below visible area
            }
            if character_chunk.x >= size.cols {
                continue; // Chunk starts outside visible area
            }
        }

        let chunk_changed_colors = character_chunk.changed_colors();
        let pane_default_fg = character_chunk.pane_default_fg;
        let pane_default_bg = character_chunk.pane_default_bg;
        let mut character_styles = DEFAULT_STYLES.enable_styled_underlines(styled_underlines);
        vte_goto_instruction(character_chunk.x, character_chunk.y, &mut vte_output)
            .with_context(err_context)?;
        let mut chunk_width = character_chunk.x;
        for t_character in character_chunk.terminal_characters.iter() {
            // Stop rendering if the next character would exceed max_size.cols
            if let Some(size) = max_size {
                if chunk_width + t_character.width() > size.cols {
                    break; // Stop rendering this chunk
                }
            }

            let current_character_styles = adjust_styles_for_custom_bg_fg(
                adjust_styles_for_possible_selection(
                    character_chunk.selection_and_colors(),
                    *t_character.styles,
                    character_chunk.y,
                    chunk_width,
                ),
                pane_default_fg,
                pane_default_bg,
            );
            write_changed_styles(
                &mut character_styles,
                current_character_styles,
                chunk_changed_colors,
                link_handler.as_ref(),
                osc8_hyperlinks,
                &mut vte_output,
            )
            .with_context(err_context)?;
            chunk_width += t_character.width();
            vte_output.push(t_character.character);
        }
    }
    if let Some(sixel_image_store) = sixel_image_store {
        if let Some(sixel_chunks) = sixel_chunks {
            for sixel_chunk in sixel_chunks {
                // Skip sixel chunks that are completely outside the size bounds
                if let Some(size) = max_size {
                    if sixel_chunk.cell_y >= size.rows {
                        continue; // Sixel chunk is below visible area
                    }
                    if sixel_chunk.cell_x >= size.cols {
                        continue; // Sixel chunk starts outside visible area
                    }
                }

                let serialized_sixel_image = sixel_image_store.serialize_image(
                    sixel_chunk.sixel_image_id,
                    sixel_chunk.sixel_image_pixel_x,
                    sixel_chunk.sixel_image_pixel_y,
                    sixel_chunk.sixel_image_pixel_width,
                    sixel_chunk.sixel_image_pixel_height,
                );
                if let Some(serialized_sixel_image) = serialized_sixel_image {
                    let sixel_vte = sixel_vte.get_or_insert_with(String::new);
                    vte_goto_instruction(sixel_chunk.cell_x, sixel_chunk.cell_y, sixel_vte)
                        .with_context(err_context)?;
                    sixel_vte.push_str(&serialized_sixel_image);
                }
            }
        }
    }
    if let Some(ref sixel_vte) = sixel_vte {
        // we do this at the end because of the implied z-index,
        // images should be above text unless the text was explicitly inserted after them (the
        // latter being a case we handle in our own internal state and not in the output)
        let save_cursor_position = "\u{1b}[s";
        let restore_cursor_position = "\u{1b}[u";
        vte_output.push_str(save_cursor_position);
        vte_output.push_str(sixel_vte);
        vte_output.push_str(restore_cursor_position);
    }
    if let Some(kitty_input) = kitty_input {
        let kitty_vte = serialize_kitty_frame(kitty_input).with_context(err_context)?;
        if !kitty_vte.is_empty() {
            vte_output.push_str("\u{1b}[s");
            vte_output.push_str(&kitty_vte);
            vte_output.push_str("\u{1b}[u");
        }
    }
    Ok(vte_output)
}

type AbsoluteMiddleStart = usize;
type AbsoluteMiddleEnd = usize;
type PadLeftEndBy = usize;
type PadRightStartBy = usize;
fn adjust_middle_segment_for_wide_chars(
    middle_start: usize,
    middle_end: usize,
    terminal_characters: &[TerminalCharacter],
) -> Result<(
    AbsoluteMiddleStart,
    AbsoluteMiddleEnd,
    PadLeftEndBy,
    PadRightStartBy,
)> {
    let err_context = || {
        format!(
            "failed to adjust middle segment (from {} to {}) for wide chars: '{:?}'",
            middle_start, middle_end, terminal_characters
        )
    };

    let mut absolute_middle_start_index = None;
    let mut absolute_middle_end_index = None;
    let mut current_x = 0;
    let mut pad_left_end_by = 0;
    let mut pad_right_start_by = 0;
    for (absolute_index, t_character) in terminal_characters.iter().enumerate() {
        current_x += t_character.width();
        if current_x >= middle_start && absolute_middle_start_index.is_none() {
            if current_x > middle_start {
                pad_left_end_by = current_x - middle_start;
                absolute_middle_start_index = Some(absolute_index);
            } else {
                absolute_middle_start_index = Some(absolute_index + 1);
            }
        }
        if current_x >= middle_end && absolute_middle_end_index.is_none() {
            absolute_middle_end_index = Some(absolute_index + 1);
            if current_x > middle_end {
                pad_right_start_by = current_x - middle_end;
            }
        }
    }
    Ok((
        absolute_middle_start_index.with_context(err_context)?,
        absolute_middle_end_index.with_context(err_context)?,
        pad_left_end_by,
        pad_right_start_by,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KittyChunkKey {
    pub pane_id: PaneId,
    pub placement_uid: u64,
    pub sub_index: u32,
}

#[derive(Debug, Clone)]
pub struct HostPlacementRecord {
    pub host_image_id: u32,
    pub host_placement_id: u32,
    pub image_key: (InternalImageId, Option<(u16, u16)>),
    pub geometry: KittyImageChunk,
}

#[derive(Debug, Clone)]
pub struct HostKittyState {
    pub transmitted: HashMap<(InternalImageId, Option<(u16, u16)>), u32>,
    pub live_placements: HashMap<KittyChunkKey, HostPlacementRecord>,
    media_files: BTreeSet<PathBuf>,
    media_by_image: HashMap<(InternalImageId, Option<(u16, u16)>), PathBuf>,
    next_host_image_id: u32,
    next_host_placement_id: u32,
}

impl Default for HostKittyState {
    fn default() -> Self {
        HostKittyState {
            transmitted: HashMap::new(),
            live_placements: HashMap::new(),
            media_files: BTreeSet::new(),
            media_by_image: HashMap::new(),
            next_host_image_id: 2_000_000_000,
            next_host_placement_id: 1,
        }
    }
}

impl HostKittyState {
    pub fn release_media(&mut self) {
        self.media_by_image.clear();
        for path in std::mem::take(&mut self.media_files) {
            let _ = fs::remove_file(path);
        }
    }

    fn track_media(&mut self, path: PathBuf) {
        self.media_files.insert(path);
        if self.media_files.len() > MAX_TRACKED_MEDIA_FILES {
            self.media_files.retain(|path| path.exists());
        }
    }

    fn track_media_for_image(
        &mut self,
        image_key: (InternalImageId, Option<(u16, u16)>),
        path: PathBuf,
    ) {
        self.media_by_image.insert(image_key, path.clone());
        self.track_media(path);
    }

    fn release_media_for_image(&mut self, image_key: &(InternalImageId, Option<(u16, u16)>)) {
        if let Some(path) = self.media_by_image.remove(image_key) {
            let _ = fs::remove_file(&path);
            self.media_files.remove(&path);
        }
    }
}

pub struct KittyFrameInput<'a> {
    pub chunks_by_pane: HashMap<PaneId, Vec<KittyImageChunk>>,
    pub rendered_panes: HashSet<PaneId>,
    pub visible_panes: Option<&'a HashSet<PaneId>>,
    pub kitty_image_store: &'a mut KittyImageStore,
    pub host_state: &'a mut HostKittyState,
    pub host_display_cleared: bool,
    pub local_media: bool,
}

fn pane_sort_key(pane_id: PaneId) -> (u8, u32) {
    match pane_id {
        PaneId::Terminal(id) => (0, id),
        PaneId::Plugin(id) => (1, id),
    }
}

fn emit_kitty_transmit(
    out: &mut String,
    host_image_id: u32,
    width: usize,
    height: usize,
    b64: &str,
) -> Result<()> {
    let err_context = "failed to serialize kitty transmit";
    let mut parts: Vec<&str> = vec![];
    let mut index = 0;
    while index < b64.len() {
        let end = std::cmp::min(index + 4096, b64.len());
        parts.push(&b64[index..end]);
        index = end;
    }
    if parts.is_empty() {
        parts.push("");
    }
    let last = parts.len() - 1;
    for (part_index, part) in parts.iter().enumerate() {
        if part_index == 0 {
            write!(
                out,
                "\u{1b}_Ga=t,q=2,f=32,t=d,i={},s={},v={},m={};{}\u{1b}\\",
                host_image_id,
                width,
                height,
                if last == 0 { 0 } else { 1 },
                part
            )
            .context(err_context)?;
        } else {
            write!(
                out,
                "\u{1b}_Gq=2,m={};{}\u{1b}\\",
                if part_index == last { 0 } else { 1 },
                part
            )
            .context(err_context)?;
        }
    }
    Ok(())
}

fn kitty_media_target() -> (u8, String, PathBuf) {
    let serial = NEXT_MEDIA_FILE.fetch_add(1, Ordering::Relaxed);
    let file_name = format!("{}-{}-{}", KITTY_MEDIA_PREFIX, std::process::id(), serial);
    let shm = Path::new(SHM_DIR);
    if shm.is_dir() {
        let path = shm.join(&file_name);
        return (b's', format!("/{}", file_name), path);
    }
    let path = std::env::temp_dir().join(&file_name);
    let name = path.to_string_lossy().into_owned();
    (b't', name, path)
}

fn emit_kitty_local_media(
    out: &mut String,
    host_image_id: u32,
    width: usize,
    height: usize,
    rgba: &[u8],
    image_key: (InternalImageId, Option<(u16, u16)>),
    host_state: &mut HostKittyState,
) -> Result<bool> {
    let (medium, name, path) = kitty_media_target();
    if let Err(e) = fs::write(&path, rgba) {
        log::warn!(
            "failed to write kitty media to {:?}, falling back to base64: {}",
            path,
            e
        );
        let _ = fs::remove_file(&path);
        return Ok(false);
    }
    host_state.track_media_for_image(image_key, path);
    write!(
        out,
        "\u{1b}_Ga=t,q=2,f=32,t={},i={},s={},v={},S={},m=0;{}\u{1b}\\",
        medium as char,
        host_image_id,
        width,
        height,
        rgba.len(),
        BASE64_ENCODER.encode(name.as_bytes()),
    )
    .context("failed to serialize kitty local media transmit")?;
    Ok(true)
}

fn serialize_kitty_frame(kitty_input: KittyFrameInput) -> Result<String> {
    let err_context = "failed to serialize kitty frame";
    let KittyFrameInput {
        mut chunks_by_pane,
        rendered_panes,
        visible_panes,
        kitty_image_store,
        host_state,
        host_display_cleared,
        local_media,
    } = kitty_input;
    let mut out = String::new();
    if host_display_cleared {
        host_state.live_placements.clear();
        for image_key in host_state.transmitted.keys().copied().collect::<Vec<_>>() {
            host_state.release_media_for_image(&image_key);
        }
        host_state.transmitted.clear();
    }
    let mut freed: Vec<((InternalImageId, Option<(u16, u16)>), u32)> = host_state
        .transmitted
        .iter()
        .filter(|(image_key, _)| kitty_image_store.get(image_key.0).is_none())
        .map(|(image_key, host_id)| (*image_key, *host_id))
        .collect();
    freed.sort_by_key(|(_, host_id)| *host_id);
    for (image_key, host_id) in freed {
        write!(out, "\u{1b}_Ga=d,q=2,d=I,i={}\u{1b}\\", host_id).context(err_context)?;
        host_state.transmitted.remove(&image_key);
        host_state.release_media_for_image(&image_key);
        host_state
            .live_placements
            .retain(|_, record| record.image_key != image_key);
    }
    if let Some(visible_panes) = visible_panes {
        let mut to_delete: Vec<(KittyChunkKey, u32, u32)> = host_state
            .live_placements
            .iter()
            .filter(|(key, _)| !visible_panes.contains(&key.pane_id))
            .map(|(key, record)| (key.clone(), record.host_image_id, record.host_placement_id))
            .collect();
        to_delete.sort_by_key(|(key, _, _)| {
            (pane_sort_key(key.pane_id), key.placement_uid, key.sub_index)
        });
        for (key, host_image_id, host_placement_id) in to_delete {
            write!(
                out,
                "\u{1b}_Ga=d,q=2,d=i,i={},p={}\u{1b}\\",
                host_image_id, host_placement_id
            )
            .context(err_context)?;
            host_state.live_placements.remove(&key);
        }
    }
    let mut rendered: Vec<PaneId> = rendered_panes.into_iter().collect();
    rendered.sort_by_key(|pane_id| pane_sort_key(*pane_id));
    for pane_id in rendered {
        let chunks = chunks_by_pane.remove(&pane_id).unwrap_or_default();
        let mut by_uid: BTreeMap<u64, Vec<KittyImageChunk>> = BTreeMap::new();
        for chunk in chunks {
            by_uid.entry(chunk.placement_uid).or_default().push(chunk);
        }
        let mut current: BTreeMap<KittyChunkKey, KittyImageChunk> = BTreeMap::new();
        for (placement_uid, mut group) in by_uid {
            group.sort_by_key(|chunk| (chunk.cell_y, chunk.cell_x));
            for (sub_index, chunk) in group.into_iter().enumerate() {
                current.insert(
                    KittyChunkKey {
                        pane_id,
                        placement_uid,
                        sub_index: sub_index as u32,
                    },
                    chunk,
                );
            }
        }
        let mut stale: Vec<(KittyChunkKey, u32, u32)> = host_state
            .live_placements
            .iter()
            .filter(|(key, _)| key.pane_id == pane_id && !current.contains_key(key))
            .map(|(key, record)| (key.clone(), record.host_image_id, record.host_placement_id))
            .collect();
        stale.sort_by_key(|(key, _, _)| (key.placement_uid, key.sub_index));
        for (key, host_image_id, host_placement_id) in stale {
            write!(
                out,
                "\u{1b}_Ga=d,q=2,d=i,i={},p={}\u{1b}\\",
                host_image_id, host_placement_id
            )
            .context(err_context)?;
            host_state.live_placements.remove(&key);
        }
        for (key, chunk) in current {
            let variant = if chunk.scaled_px.is_some() {
                Some(chunk.dest_cells)
            } else {
                None
            };
            let image_key = (chunk.internal_image_id, variant);
            let host_image_id = match host_state.transmitted.get(&image_key) {
                Some(host_image_id) => *host_image_id,
                None => {
                    let raster_dims = match variant {
                        Some(cells) => {
                            match kitty_image_store.scaled_variant(chunk.internal_image_id, cells) {
                                Some(_) => chunk.scaled_px,
                                None => None,
                            }
                        },
                        None => kitty_image_store
                            .get(chunk.internal_image_id)
                            .map(|image| (image.width as usize, image.height as usize)),
                    };
                    let (width, height) = match raster_dims {
                        Some(dims) => dims,
                        None => continue,
                    };
                    let host_image_id = host_state.next_host_image_id;
                    let mut emitted = false;
                    if local_media {
                        let rgba = match variant {
                            Some(cells) => {
                                kitty_image_store.scaled_variant(chunk.internal_image_id, cells)
                            },
                            None => kitty_image_store
                                .get(chunk.internal_image_id)
                                .map(|image| image.rgba.as_slice()),
                        };
                        if let Some(rgba) = rgba {
                            emitted = emit_kitty_local_media(
                                &mut out,
                                host_image_id,
                                width,
                                height,
                                rgba,
                                image_key,
                                host_state,
                            )?;
                        }
                    }
                    if !emitted {
                        let b64 =
                            match kitty_image_store.base64_for(chunk.internal_image_id, variant) {
                                Some(b64) => b64,
                                None => continue,
                            };
                        emit_kitty_transmit(&mut out, host_image_id, width, height, &b64)?;
                    }
                    host_state.next_host_image_id += 1;
                    host_state.transmitted.insert(image_key, host_image_id);
                    host_image_id
                },
            };
            let existing = host_state.live_placements.get(&key).map(|record| {
                (
                    record.host_placement_id,
                    record.geometry,
                    record.host_image_id,
                )
            });
            let host_placement_id = match existing {
                Some((_, geometry, existing_host_image_id))
                    if geometry == chunk && existing_host_image_id == host_image_id =>
                {
                    continue;
                },
                Some((host_placement_id, _, _)) => host_placement_id,
                None => {
                    let host_placement_id = host_state.next_host_placement_id;
                    host_state.next_host_placement_id += 1;
                    host_placement_id
                },
            };
            vte_goto_instruction(chunk.cell_x, chunk.cell_y, &mut out).context(err_context)?;
            write!(
                out,
                "\u{1b}_Ga=p,q=2,i={},p={},x={},y={},w={},h={},X={},Y={},z={},C=1\u{1b}\\",
                host_image_id,
                host_placement_id,
                chunk.source_px_x,
                chunk.source_px_y,
                chunk.source_px_width,
                chunk.source_px_height,
                chunk.cell_offset_x,
                chunk.cell_offset_y,
                chunk.z_index
            )
            .context(err_context)?;
            host_state.live_placements.insert(
                key,
                HostPlacementRecord {
                    host_image_id,
                    host_placement_id,
                    image_key,
                    geometry: chunk,
                },
            );
        }
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq)]
pub enum RenderPayload {
    Ansi(String),
    Frame(Vec<u8>),
}

impl RenderPayload {
    pub fn ansi(&self) -> &str {
        match self {
            RenderPayload::Ansi(content) => content,
            RenderPayload::Frame(_) => "",
        }
    }

    pub fn frame(&self) -> Option<&[u8]> {
        match self {
            RenderPayload::Ansi(_) => None,
            RenderPayload::Frame(frame) => Some(frame),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            RenderPayload::Ansi(content) => content.is_empty(),
            RenderPayload::Frame(frame) => frame.is_empty(),
        }
    }
}

pub const STRUCTURED_RENDER_DEADLINE: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct StructuredClientState {
    pub enabled: bool,
    pub size: Size,
    pub next_seq: u64,
    pub last_dims: Option<(u16, u16)>,
    pub force_full_repaint: bool,
    pub last_cursor: CursorState,
    pub in_flight: Option<u64>,
    pub in_flight_since: Option<Instant>,
    pub overlay: PendingOverlay,
    pub last_geometry: Option<GeometryRecord>,
    pub links: LinkTable,
}

#[derive(Debug, Default)]
pub struct LinkTable {
    ids: HashMap<String, u8>,
    uris: HashMap<u8, String>,
    shadow: Vec<u8>,
    cols: usize,
    fresh: Vec<LinkEntry>,
}

impl LinkTable {
    pub fn reset(&mut self) {
        self.ids.clear();
        self.uris.clear();
        self.shadow.clear();
        self.cols = 0;
        self.fresh.clear();
    }

    fn open(&mut self, cols: usize, rows: usize) {
        if self.cols == cols && self.shadow.len() == cols * rows {
            return;
        }
        self.reset();
        self.cols = cols;
        self.shadow = vec![LINK_NONE; cols * rows];
    }

    fn id_for(&mut self, uri: &str, cols: usize, rows: usize) -> u8 {
        if uri.is_empty() || uri.len() > MAX_LINK_URI_LEN {
            return LINK_NONE;
        }
        if let Some(id) = self.ids.get(uri) {
            return *id;
        }
        self.open(cols, rows);
        let Some(id) = (1..=MAX_LINK_ID).find(|id| !self.uris.contains_key(id)) else {
            return LINK_NONE;
        };
        self.ids.insert(uri.to_owned(), id);
        self.uris.insert(id, uri.to_owned());
        self.fresh.push(LinkEntry {
            id,
            uri: uri.to_owned(),
        });
        id
    }

    fn note_row(&mut self, y: usize, x0: usize, cells: &[WireCell]) {
        if self.shadow.is_empty() {
            return;
        }
        let start = y * self.cols + x0;
        for (offset, cell) in cells.iter().enumerate() {
            let Some(held) = self.shadow.get_mut(start + offset) else {
                return;
            };
            *held = cell.link;
        }
    }

    fn take_fresh(&mut self) -> LinkRecord {
        LinkRecord {
            entries: std::mem::take(&mut self.fresh),
        }
    }

    fn reclaim(&mut self) {
        if self.shadow.is_empty() || self.uris.is_empty() {
            return;
        }
        let mut live = [false; 256];
        for id in &self.shadow {
            live[*id as usize] = true;
        }
        self.uris.retain(|id, uri| {
            if live[*id as usize] {
                return true;
            }
            self.ids.remove(uri.as_str());
            false
        });
    }
}

impl Default for StructuredClientState {
    fn default() -> Self {
        StructuredClientState {
            enabled: false,
            size: Size::default(),
            next_seq: 0,
            last_dims: None,
            force_full_repaint: false,
            last_cursor: CursorState::default(),
            in_flight: None,
            in_flight_since: None,
            overlay: PendingOverlay::new(0, 0),
            last_geometry: None,
            links: LinkTable::default(),
        }
    }
}

impl StructuredClientState {
    pub fn reset_viewport(&mut self, size: Size) {
        self.size = size;
        self.force_full_repaint = true;
        self.in_flight = None;
        self.in_flight_since = None;
        self.overlay.resize(size.cols, size.rows);
        self.last_geometry = None;
        self.links.reset();
    }

    pub fn arm(&mut self, seq: u64) {
        self.in_flight = Some(seq);
        self.in_flight_since = Some(Instant::now());
    }

    pub fn disarm(&mut self) {
        self.in_flight = None;
        self.in_flight_since = None;
    }

    pub fn delivery_deadline_expired(&self) -> bool {
        match (self.in_flight, self.in_flight_since) {
            (Some(_), Some(since)) => since.elapsed() >= STRUCTURED_RENDER_DEADLINE,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    pub fn allocate_seq(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
    }
}

fn wire_cursor_shape(csi: &str) -> u8 {
    match csi {
        "\u{1b}[1 q" => CURSOR_SHAPE_BLOCK | CURSOR_BLINKING,
        "\u{1b}[2 q" => CURSOR_SHAPE_BLOCK,
        "\u{1b}[3 q" => CURSOR_SHAPE_UNDERLINE | CURSOR_BLINKING,
        "\u{1b}[4 q" => CURSOR_SHAPE_UNDERLINE,
        "\u{1b}[5 q" => CURSOR_SHAPE_BEAM | CURSOR_BLINKING,
        "\u{1b}[6 q" => CURSOR_SHAPE_BEAM,
        _ => CURSOR_SHAPE_DEFAULT,
    }
}

fn frame_builder_for(
    clients: &Rc<RefCell<HashMap<ClientId, StructuredClientState>>>,
    client_id: ClientId,
    cursor: Option<CursorState>,
) -> Option<FrameBuilder> {
    let mut clients = clients.borrow_mut();
    let state = clients.get_mut(&client_id)?;
    let cols = state.size.cols.min(u16::MAX as usize) as u16;
    let rows = state.size.rows.min(u16::MAX as usize) as u16;
    let seq = state.allocate_seq();
    let dims_changed = state.last_dims != Some((cols, rows));
    let full_repaint = state.force_full_repaint || dims_changed;
    if full_repaint {
        state.links.reset();
    }
    state.force_full_repaint = false;
    state.last_dims = Some((cols, rows));
    let cursor = cursor.unwrap_or(state.last_cursor);
    state.last_cursor = cursor;
    let mut builder = FrameBuilder::new(cols, rows, seq);
    builder.set_full_repaint(full_repaint);
    builder.set_clear(full_repaint);
    builder.set_cursor(cursor);
    Some(builder)
}

fn link_flatten<'a>(
    handler: Option<&'a LinkHandler>,
    state: Option<&'a mut StructuredClientState>,
) -> Option<LinkFlatten<'a>> {
    match (handler, state) {
        (Some(handler), Some(state)) => Some(LinkFlatten {
            handler,
            table: &mut state.links,
        }),
        _ => None,
    }
}

fn push_geometry_if_changed(
    clients: &Rc<RefCell<HashMap<ClientId, StructuredClientState>>>,
    client_id: ClientId,
    geometry: GeometryRecord,
    builder: &mut FrameBuilder,
) {
    let mut clients = clients.borrow_mut();
    let Some(state) = clients.get_mut(&client_id) else {
        return;
    };
    if state.last_geometry.as_ref() == Some(&geometry) {
        return;
    }
    builder.push_geometry(&geometry);
    state.last_geometry = Some(geometry);
}

fn is_subsumed_by_frame_semantics(instruction: &str) -> bool {
    if matches!(
        instruction,
        "\u{1b}[?25h" | "\u{1b}[?25l" | "\u{1b}[m\u{1b}[2J"
    ) {
        return true;
    }
    let Some(rest) = instruction.strip_prefix("\u{1b}[") else {
        return false;
    };
    let Some(position_end) = rest.find('H') else {
        return false;
    };
    let (position, tail) = rest.split_at(position_end);
    let mut coordinates = position.split(';');
    let is_bare_goto = matches!(
        (coordinates.next(), coordinates.next(), coordinates.next()),
        (Some(row), Some(column), None)
            if !row.is_empty()
                && !column.is_empty()
                && row.bytes().all(|byte| byte.is_ascii_digit())
                && column.bytes().all(|byte| byte.is_ascii_digit())
    );
    if !is_bare_goto {
        return false;
    }
    let tail = &tail[1..];
    const SHAPE_TAIL_LEN: usize = "\u{1b}[m\u{1b}[0 q".len();
    tail.is_empty()
        || tail == "\u{1b}[m"
        || (tail.len() == SHAPE_TAIL_LEN
            && tail.starts_with("\u{1b}[m\u{1b}[")
            && tail.ends_with(" q"))
}

fn append_sideband_instruction(sideband: &mut Vec<u8>, instruction: &str) {
    if is_subsumed_by_frame_semantics(instruction) {
        return;
    }
    sideband.extend_from_slice(instruction.as_bytes());
}

#[derive(Clone, Debug, Default)]
pub struct Output {
    pre_vte_instructions: HashMap<ClientId, Vec<String>>,
    post_vte_instructions: HashMap<ClientId, Vec<String>>,
    client_character_chunks: HashMap<ClientId, Vec<CharacterChunk>>,
    client_pane_rects: HashMap<ClientId, Vec<PaneRect>>,
    sixel_chunks: HashMap<ClientId, Vec<SixelImageChunk>>,
    client_kitty_chunks: HashMap<ClientId, HashMap<PaneId, Vec<KittyImageChunk>>>,
    client_rendered_kitty_panes: HashMap<ClientId, HashSet<PaneId>>,
    client_kitty_visible_panes: HashMap<ClientId, HashSet<PaneId>>,
    clients_with_cleared_host_display: HashSet<ClientId>,
    link_handler: Option<Rc<RefCell<LinkHandler>>>,
    sixel_image_store: Rc<RefCell<SixelImageStore>>,
    kitty_image_store: Rc<RefCell<KittyImageStore>>,
    kitty_host_capabilities: Rc<RefCell<HashMap<ClientId, bool>>>,
    kitty_local_media: Rc<RefCell<HashMap<ClientId, bool>>>,
    kitty_host_state: Rc<RefCell<HashMap<ClientId, HostKittyState>>>,
    sixel_host_capabilities: Rc<RefCell<HashMap<ClientId, bool>>>,
    structured_render_clients: Rc<RefCell<HashMap<ClientId, StructuredClientState>>>,
    character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
    floating_panes_stack: Option<FloatingPanesStack>,
    styled_underlines: bool,
    osc8_hyperlinks: bool,
    pane_render_report: PaneRenderReport,
    pub collect_ansi_pane_contents: bool,
    cursor_coordinates: Option<(usize, usize)>,
    client_cursor: HashMap<ClientId, CursorState>,
}

impl Output {
    pub fn new(
        sixel_image_store: Rc<RefCell<SixelImageStore>>,
        character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
        styled_underlines: bool,
        osc8_hyperlinks: bool,
        kitty_image_store: Rc<RefCell<KittyImageStore>>,
        kitty_host_capabilities: Rc<RefCell<HashMap<ClientId, bool>>>,
        kitty_local_media: Rc<RefCell<HashMap<ClientId, bool>>>,
        kitty_host_state: Rc<RefCell<HashMap<ClientId, HostKittyState>>>,
        sixel_host_capabilities: Rc<RefCell<HashMap<ClientId, bool>>>,
        structured_render_clients: Rc<RefCell<HashMap<ClientId, StructuredClientState>>>,
    ) -> Self {
        Output {
            sixel_image_store,
            character_cell_size,
            styled_underlines,
            osc8_hyperlinks,
            kitty_image_store,
            kitty_host_capabilities,
            kitty_local_media,
            kitty_host_state,
            sixel_host_capabilities,
            structured_render_clients,
            ..Default::default()
        }
    }

    pub fn add_clients(
        &mut self,
        client_ids: &HashSet<ClientId>,
        link_handler: Rc<RefCell<LinkHandler>>,
        floating_panes_stack: Option<FloatingPanesStack>,
    ) {
        self.link_handler = Some(link_handler);
        self.floating_panes_stack = floating_panes_stack;
        for client_id in client_ids {
            self.client_character_chunks.insert(*client_id, vec![]);
            self.client_pane_rects.insert(*client_id, vec![]);
        }
    }
    pub fn add_pane_rect(&mut self, client_id: ClientId, pane_rect: PaneRect) {
        if let Some(pane_rects) = self.client_pane_rects.get_mut(&client_id) {
            pane_rects.push(pane_rect);
        }
    }
    pub fn add_character_chunks_to_client(
        &mut self,
        client_id: ClientId,
        mut character_chunks: Vec<CharacterChunk>,
        z_index: Option<usize>,
    ) -> Result<()> {
        if let Some(client_character_chunks) = self.client_character_chunks.get_mut(&client_id) {
            if let Some(floating_panes_stack) = &self.floating_panes_stack {
                let mut visible_character_chunks = floating_panes_stack
                    .visible_character_chunks(character_chunks, z_index)
                    .with_context(|| {
                        format!("failed to add character chunks for client {}", client_id)
                    })?;
                client_character_chunks.append(&mut visible_character_chunks);
            } else {
                client_character_chunks.append(&mut character_chunks);
            }
        }
        Ok(())
    }
    pub fn add_character_chunks_to_multiple_clients(
        &mut self,
        character_chunks: Vec<CharacterChunk>,
        client_ids: impl Iterator<Item = ClientId>,
        z_index: Option<usize>,
    ) -> Result<()> {
        for client_id in client_ids {
            self.add_character_chunks_to_client(client_id, character_chunks.clone(), z_index)
                .context("failed to add character chunks for multiple clients")?;
            // TODO: forgo clone by adding an all_clients thing?
        }
        Ok(())
    }
    pub fn add_post_vte_instruction_to_multiple_clients(
        &mut self,
        client_ids: impl Iterator<Item = ClientId>,
        vte_instruction: &str,
    ) {
        for client_id in client_ids {
            let entry = self
                .post_vte_instructions
                .entry(client_id)
                .or_insert_with(Vec::new);
            entry.push(String::from(vte_instruction));
        }
    }
    pub fn add_pre_vte_instruction_to_multiple_clients(
        &mut self,
        client_ids: impl Iterator<Item = ClientId>,
        vte_instruction: &str,
    ) {
        for client_id in client_ids {
            let entry = self
                .pre_vte_instructions
                .entry(client_id)
                .or_insert_with(Vec::new);
            entry.push(String::from(vte_instruction));
        }
    }
    pub fn mark_host_display_cleared_for_clients(
        &mut self,
        client_ids: impl Iterator<Item = ClientId>,
    ) {
        for client_id in client_ids {
            self.clients_with_cleared_host_display.insert(client_id);
        }
    }
    pub fn mark_host_display_cleared_for_client(&mut self, client_id: ClientId) {
        self.clients_with_cleared_host_display.insert(client_id);
    }
    pub fn add_post_vte_instruction_to_client(
        &mut self,
        client_id: ClientId,
        vte_instruction: &str,
    ) {
        let entry = self
            .post_vte_instructions
            .entry(client_id)
            .or_insert_with(Vec::new);
        entry.push(String::from(vte_instruction));
    }
    pub fn add_pre_vte_instruction_to_client(
        &mut self,
        client_id: ClientId,
        vte_instruction: &str,
    ) {
        let entry = self
            .pre_vte_instructions
            .entry(client_id)
            .or_insert_with(Vec::new);
        entry.push(String::from(vte_instruction));
    }
    pub fn add_sixel_image_chunks_to_client(
        &mut self,
        client_id: ClientId,
        sixel_image_chunks: Vec<SixelImageChunk>,
        z_index: Option<usize>,
    ) {
        if let Some(character_cell_size) = *self.character_cell_size.borrow() {
            let mut sixel_chunks = if let Some(floating_panes_stack) = &self.floating_panes_stack {
                floating_panes_stack.visible_sixel_image_chunks(
                    sixel_image_chunks,
                    z_index,
                    &character_cell_size,
                )
            } else {
                sixel_image_chunks
            };
            let entry = self.sixel_chunks.entry(client_id).or_insert_with(Vec::new);
            entry.append(&mut sixel_chunks);
        }
    }
    pub fn add_sixel_image_chunks_to_multiple_clients(
        &mut self,
        sixel_image_chunks: Vec<SixelImageChunk>,
        client_ids: impl Iterator<Item = ClientId>,
        z_index: Option<usize>,
    ) {
        if let Some(character_cell_size) = *self.character_cell_size.borrow() {
            let sixel_chunks = if let Some(floating_panes_stack) = &self.floating_panes_stack {
                floating_panes_stack.visible_sixel_image_chunks(
                    sixel_image_chunks,
                    z_index,
                    &character_cell_size,
                )
            } else {
                sixel_image_chunks
            };
            for client_id in client_ids {
                let entry = self.sixel_chunks.entry(client_id).or_insert_with(Vec::new);
                entry.append(&mut sixel_chunks.clone());
            }
        }
    }
    pub fn add_kitty_image_chunks_to_client(
        &mut self,
        client_id: ClientId,
        pane_id: PaneId,
        kitty_image_chunks: Vec<KittyImageChunk>,
        z_index: Option<usize>,
    ) {
        self.client_rendered_kitty_panes
            .entry(client_id)
            .or_insert_with(HashSet::new)
            .insert(pane_id);
        let mut kitty_chunks = match (
            *self.character_cell_size.borrow(),
            &self.floating_panes_stack,
        ) {
            (Some(character_cell_size), Some(floating_panes_stack)) => floating_panes_stack
                .visible_kitty_image_chunks(kitty_image_chunks, z_index, &character_cell_size),
            _ => kitty_image_chunks,
        };
        self.client_kitty_chunks
            .entry(client_id)
            .or_insert_with(HashMap::new)
            .entry(pane_id)
            .or_insert_with(Vec::new)
            .append(&mut kitty_chunks);
    }
    pub fn add_kitty_image_chunks_to_multiple_clients(
        &mut self,
        pane_id: PaneId,
        kitty_image_chunks: Vec<KittyImageChunk>,
        client_ids: impl Iterator<Item = ClientId>,
        z_index: Option<usize>,
    ) {
        let kitty_chunks = match (
            *self.character_cell_size.borrow(),
            &self.floating_panes_stack,
        ) {
            (Some(character_cell_size), Some(floating_panes_stack)) => floating_panes_stack
                .visible_kitty_image_chunks(kitty_image_chunks, z_index, &character_cell_size),
            _ => kitty_image_chunks,
        };
        for client_id in client_ids {
            self.client_rendered_kitty_panes
                .entry(client_id)
                .or_insert_with(HashSet::new)
                .insert(pane_id);
            self.client_kitty_chunks
                .entry(client_id)
                .or_insert_with(HashMap::new)
                .entry(pane_id)
                .or_insert_with(Vec::new)
                .append(&mut kitty_chunks.clone());
        }
    }
    pub fn set_kitty_visible_panes(&mut self, client_id: ClientId, pane_ids: HashSet<PaneId>) {
        self.client_kitty_visible_panes.insert(client_id, pane_ids);
    }
    pub fn serialize(&mut self) -> Result<HashMap<ClientId, RenderPayload>> {
        let err_context = || "failed to serialize output to clients".to_string();

        let mut serialized_render_instructions = HashMap::new();

        for (client_id, client_character_chunks) in self.client_character_chunks.drain() {
            let structured = self
                .structured_render_clients
                .borrow()
                .get(&client_id)
                .map(|state| state.enabled)
                .unwrap_or(false);
            let mut client_serialized_render_instructions = String::new();

            // append pre-vte instructions for this client
            let host_display_cleared = self.clients_with_cleared_host_display.remove(&client_id);
            let pre_vte_instructions_for_client = self
                .pre_vte_instructions
                .remove(&client_id)
                .unwrap_or_default();
            if !structured {
                for vte_instruction in &pre_vte_instructions_for_client {
                    client_serialized_render_instructions.push_str(vte_instruction);
                }
            }

            let kitty_chunks_by_pane = self.client_kitty_chunks.remove(&client_id);
            let kitty_rendered_panes = self.client_rendered_kitty_panes.remove(&client_id);
            let kitty_visible_panes = self.client_kitty_visible_panes.remove(&client_id);
            let client_host_is_kitty_capable = self
                .kitty_host_capabilities
                .borrow()
                .get(&client_id)
                .copied()
                .unwrap_or(false);
            let client_host_is_sixel_capable = self
                .sixel_host_capabilities
                .borrow()
                .get(&client_id)
                .copied()
                .unwrap_or(false);
            let client_reads_local_media = self
                .kitty_local_media
                .borrow()
                .get(&client_id)
                .copied()
                .unwrap_or(false);
            let mut kitty_image_store;
            let mut kitty_host_state_map;
            let kitty_input = if client_host_is_kitty_capable {
                kitty_image_store = self.kitty_image_store.borrow_mut();
                kitty_host_state_map = self.kitty_host_state.borrow_mut();
                Some(KittyFrameInput {
                    chunks_by_pane: kitty_chunks_by_pane.unwrap_or_default(),
                    rendered_panes: kitty_rendered_panes.unwrap_or_default(),
                    visible_panes: kitty_visible_panes.as_ref(),
                    kitty_image_store: &mut *kitty_image_store,
                    host_state: kitty_host_state_map.entry(client_id).or_default(),
                    host_display_cleared,
                    local_media: client_reads_local_media,
                })
            } else {
                None
            };

            // append the actual vte
            let sixel_chunks_for_client = if client_host_is_sixel_capable {
                self.sixel_chunks.get(&client_id)
            } else {
                None
            };

            if structured {
                let mut builder = frame_builder_for(
                    &self.structured_render_clients,
                    client_id,
                    self.client_cursor.get(&client_id).copied(),
                )
                .with_context(|| {
                    format!(
                        "client {} is registered for structured rendering without a record",
                        client_id
                    )
                })
                .with_context(err_context)?;
                if host_display_cleared {
                    builder.set_clear(true);
                }
                push_geometry_if_changed(
                    &self.structured_render_clients,
                    client_id,
                    GeometryRecord {
                        panes: self
                            .client_pane_rects
                            .remove(&client_id)
                            .unwrap_or_default(),
                    },
                    &mut builder,
                );
                for vte_instruction in &pre_vte_instructions_for_client {
                    append_sideband_instruction(builder.sideband_mut(), vte_instruction);
                }
                let link_handler = self.link_handler.as_ref().map(|handler| handler.borrow());
                let mut structured_clients = self.structured_render_clients.borrow_mut();
                let mut links = link_flatten(
                    link_handler.as_deref(),
                    structured_clients.get_mut(&client_id),
                );
                flatten_chunks(
                    client_character_chunks,
                    sixel_chunks_for_client,
                    Some(&mut self.sixel_image_store.borrow_mut()),
                    self.styled_underlines,
                    kitty_input,
                    links.as_mut(),
                    &mut builder,
                )
                .with_context(err_context)?;
                drop(links);
                drop(structured_clients);
                drop(link_handler);
                if let Some(post_vte_instructions_for_client) =
                    self.post_vte_instructions.remove(&client_id)
                {
                    for vte_instruction in post_vte_instructions_for_client {
                        append_sideband_instruction(builder.sideband_mut(), &vte_instruction);
                    }
                }
                serialized_render_instructions
                    .insert(client_id, RenderPayload::Frame(builder.finish()));
                continue;
            }

            client_serialized_render_instructions.push_str(
                &serialize_chunks(
                    client_character_chunks,
                    sixel_chunks_for_client,
                    self.link_handler.as_mut(),
                    Some(&mut self.sixel_image_store.borrow_mut()),
                    self.styled_underlines,
                    self.osc8_hyperlinks,
                    None, // No size constraints for regular rendering
                    kitty_input,
                )
                .with_context(err_context)?,
            ); // TODO: less allocations?

            // append post-vte instructions for this client
            if let Some(post_vte_instructions_for_client) =
                self.post_vte_instructions.remove(&client_id)
            {
                for vte_instruction in post_vte_instructions_for_client {
                    client_serialized_render_instructions.push_str(&vte_instruction);
                }
            }
            serialized_render_instructions.insert(
                client_id,
                RenderPayload::Ansi(client_serialized_render_instructions),
            );
        }
        Ok(serialized_render_instructions)
    }

    pub fn serialize_watcher_frame(
        &mut self,
        followed_client_id: ClientId,
        watcher_id: ClientId,
    ) -> Result<Vec<u8>> {
        let err_context = || {
            format!(
                "failed to serialize a frame for watcher {} following client {}",
                watcher_id, followed_client_id
            )
        };
        let missing_record = || {
            format!(
                "watcher {} is registered for structured rendering without a record",
                watcher_id
            )
        };

        let watcher_size = self
            .structured_render_clients
            .borrow()
            .get(&watcher_id)
            .map(|state| state.size)
            .with_context(missing_record)
            .with_context(err_context)?;

        let cursor = self
            .client_cursor
            .get(&followed_client_id)
            .copied()
            .map(|mut cursor| {
                if cursor.y as usize >= watcher_size.rows || cursor.x as usize >= watcher_size.cols
                {
                    cursor.visible = false;
                }
                cursor
            });

        let mut builder = frame_builder_for(&self.structured_render_clients, watcher_id, cursor)
            .with_context(missing_record)
            .with_context(err_context)?;

        if self
            .clients_with_cleared_host_display
            .remove(&followed_client_id)
        {
            builder.set_clear(true);
        }
        let followed_pane_rects = self
            .client_pane_rects
            .get(&followed_client_id)
            .cloned()
            .unwrap_or_default();
        push_geometry_if_changed(
            &self.structured_render_clients,
            watcher_id,
            GeometryRecord {
                panes: followed_pane_rects,
            }
            .clip_to(
                watcher_size.cols.min(u16::MAX as usize) as u16,
                watcher_size.rows.min(u16::MAX as usize) as u16,
            ),
            &mut builder,
        );
        for vte_instruction in self
            .pre_vte_instructions
            .remove(&followed_client_id)
            .unwrap_or_default()
        {
            append_sideband_instruction(builder.sideband_mut(), &vte_instruction);
        }

        let character_chunks = self
            .client_character_chunks
            .remove(&followed_client_id)
            .unwrap_or_default();
        let followed_host_is_sixel_capable = self
            .sixel_host_capabilities
            .borrow()
            .get(&followed_client_id)
            .copied()
            .unwrap_or(false);
        let sixel_chunks_for_client = if followed_host_is_sixel_capable {
            self.sixel_chunks.get(&followed_client_id)
        } else {
            None
        };
        let link_handler = self.link_handler.as_ref().map(|handler| handler.borrow());
        let mut structured_clients = self.structured_render_clients.borrow_mut();
        let mut links = link_flatten(
            link_handler.as_deref(),
            structured_clients.get_mut(&watcher_id),
        );
        flatten_chunks(
            character_chunks,
            sixel_chunks_for_client,
            Some(&mut self.sixel_image_store.borrow_mut()),
            self.styled_underlines,
            None,
            links.as_mut(),
            &mut builder,
        )
        .with_context(err_context)?;
        drop(links);
        drop(structured_clients);
        drop(link_handler);

        for vte_instruction in self
            .post_vte_instructions
            .remove(&followed_client_id)
            .unwrap_or_default()
        {
            append_sideband_instruction(builder.sideband_mut(), &vte_instruction);
        }

        Ok(builder.finish())
    }
    pub fn set_client_cursor(
        &mut self,
        client_id: ClientId,
        x: usize,
        y: usize,
        visible: bool,
        shape_csi: &str,
    ) {
        self.client_cursor.insert(
            client_id,
            CursorState {
                x: x.min(u16::MAX as usize) as u16,
                y: y.min(u16::MAX as usize) as u16,
                shape: wire_cursor_shape(shape_csi),
                visible,
            },
        );
    }
    pub fn serialize_with_size(
        &mut self,
        max_size: Option<Size>,
        content_size: Option<Size>,
    ) -> Result<HashMap<ClientId, String>> {
        let err_context =
            || "failed to serialize output to clients with size constraints".to_string();

        let mut serialized_render_instructions = HashMap::new();

        for (client_id, client_character_chunks) in self.client_character_chunks.drain() {
            let mut client_serialized_render_instructions = String::new();

            // append pre-vte instructions for this client
            if let Some(pre_vte_instructions_for_client) =
                self.pre_vte_instructions.remove(&client_id)
            {
                for vte_instruction in pre_vte_instructions_for_client {
                    client_serialized_render_instructions.push_str(&vte_instruction);
                }
            }

            // Add padding instructions if max_size is larger than content_size
            if let (Some(max_size), Some(content_size)) = (max_size, content_size) {
                if max_size.rows > content_size.rows || max_size.cols > content_size.cols {
                    // Clear each line from the end of rendered content to the end of the watcher's line
                    for y in 0..content_size.rows {
                        let padding_instruction = format!(
                            "\u{1b}[{};{}H\u{1b}[m\u{1b}[K",
                            y + 1,
                            content_size.cols + 1
                        );
                        client_serialized_render_instructions.push_str(&padding_instruction);
                    }

                    // Clear all content below the last rendered line
                    let clear_below_instruction =
                        format!("\u{1b}[{};{}H\u{1b}[m\u{1b}[J", content_size.rows + 1, 1);
                    client_serialized_render_instructions.push_str(&clear_below_instruction);
                }
            }

            // append the actual vte with size constraints
            let client_host_is_sixel_capable = self
                .sixel_host_capabilities
                .borrow()
                .get(&client_id)
                .copied()
                .unwrap_or(false);
            let sixel_chunks_for_client = if client_host_is_sixel_capable {
                self.sixel_chunks.get(&client_id)
            } else {
                None
            };
            client_serialized_render_instructions.push_str(
                &serialize_chunks(
                    client_character_chunks,
                    sixel_chunks_for_client,
                    self.link_handler.as_mut(),
                    Some(&mut self.sixel_image_store.borrow_mut()),
                    self.styled_underlines,
                    self.osc8_hyperlinks,
                    max_size,
                    None,
                )
                .with_context(err_context)?,
            );

            // append post-vte instructions for this client
            if let Some(post_vte_instructions_for_client) =
                self.post_vte_instructions.remove(&client_id)
            {
                for vte_instruction in post_vte_instructions_for_client {
                    client_serialized_render_instructions.push_str(&vte_instruction);
                }
            }

            // Check if cursor was cropped and hide it if necessary
            if let (Some(max_size), Some((cursor_x, cursor_y))) =
                (max_size, self.cursor_coordinates)
            {
                let cursor_was_cropped = cursor_y >= max_size.rows || cursor_x >= max_size.cols;
                if cursor_was_cropped {
                    vte_hide_cursor_instruction(&mut client_serialized_render_instructions)
                        .with_context(err_context)?;
                }
            }

            serialized_render_instructions.insert(client_id, client_serialized_render_instructions);
        }
        Ok(serialized_render_instructions)
    }
    pub fn is_dirty(&self) -> bool {
        !self.pre_vte_instructions.is_empty()
            || !self.post_vte_instructions.is_empty()
            || self.client_character_chunks.values().any(|c| !c.is_empty())
            || self.sixel_chunks.values().any(|c| !c.is_empty())
            || self
                .client_kitty_chunks
                .values()
                .any(|chunks_by_pane| chunks_by_pane.values().any(|c| !c.is_empty()))
            || self.has_pending_kitty_host_deletes()
    }
    pub fn has_pending_kitty_host_deletes(&self) -> bool {
        let kitty_image_store = self.kitty_image_store.borrow();
        self.kitty_host_state.borrow().values().any(|host_state| {
            host_state
                .transmitted
                .keys()
                .any(|(internal_id, _)| kitty_image_store.get(*internal_id).is_none())
        })
    }
    pub fn has_rendered_assets(&self) -> bool {
        // pre_vte and post_vte are not considered rendered assets as they should not be visible
        self.client_character_chunks.values().any(|c| !c.is_empty())
            || self.sixel_chunks.values().any(|c| !c.is_empty())
            || self
                .client_kitty_chunks
                .values()
                .any(|chunks_by_pane| chunks_by_pane.values().any(|c| !c.is_empty()))
    }
    pub fn cursor_is_visible(
        &mut self,
        cursor_x: usize,
        cursor_y: usize,
        z_index: Option<usize>,
    ) -> bool {
        self.cursor_coordinates = Some((cursor_x, cursor_y));
        self.floating_panes_stack
            .as_ref()
            .map(|s| s.cursor_is_visible(cursor_x, cursor_y, z_index))
            .unwrap_or(true)
    }
    pub fn add_pane_contents(
        &mut self,
        client_ids: &[ClientId],
        pane_id: PaneId,
        pane_contents: PaneContents,
    ) {
        self.pane_render_report
            .add_pane_contents(client_ids, pane_id.into(), pane_contents);
    }
    pub fn add_pane_contents_with_ansi(
        &mut self,
        client_ids: &[ClientId],
        pane_id: PaneId,
        pane_contents: PaneContents,
    ) {
        self.pane_render_report.add_pane_contents_with_ansi(
            client_ids,
            pane_id.into(),
            pane_contents,
        );
    }
    pub fn drain_pane_render_report(&mut self) -> PaneRenderReport {
        let empty_pane_render_report = PaneRenderReport::default();
        std::mem::replace(&mut self.pane_render_report, empty_pane_render_report)
    }
}

// this struct represents the geometry of a group of floating panes
// we use it to filter out CharacterChunks who are behind these geometries
// and so would not be visible. If a chunk is partially covered, it is adjusted
// to include only the non-covered parts
#[derive(Debug, Clone, Default)]
pub struct FloatingPanesStack {
    pub layers: Vec<PaneGeom>,
}

impl FloatingPanesStack {
    pub fn visible_character_chunks(
        &self,
        mut character_chunks: Vec<CharacterChunk>,
        z_index: Option<usize>,
    ) -> Result<Vec<CharacterChunk>> {
        let err_context = || {
            format!(
                "failed to determine visible character chunks at z-index {:?}",
                z_index
            )
        };

        let z_index = z_index.unwrap_or(0);
        let mut chunks_to_check: Vec<CharacterChunk> = character_chunks.drain(..).collect();
        let mut visible_chunks = vec![];
        'chunk_loop: loop {
            match chunks_to_check.pop() {
                Some(mut c_chunk) => {
                    let panes_to_check = self.layers.iter().skip(z_index);
                    for pane_geom in panes_to_check {
                        let new_chunk_to_check = self
                            .remove_covered_parts(pane_geom, &mut c_chunk)
                            .with_context(err_context)?;
                        if let Some(new_chunk_to_check) = new_chunk_to_check {
                            // this happens when the pane covers the middle of the chunk, and so we
                            // end up with an extra chunk we need to check (eg. against panes above
                            // this one)
                            chunks_to_check.push(new_chunk_to_check);
                        }
                        if c_chunk.terminal_characters.is_empty() {
                            continue 'chunk_loop;
                        }
                    }
                    visible_chunks.push(c_chunk);
                },
                None => {
                    break 'chunk_loop;
                },
            }
        }
        Ok(visible_chunks)
    }
    pub fn visible_sixel_image_chunks(
        &self,
        mut sixel_image_chunks: Vec<SixelImageChunk>,
        z_index: Option<usize>,
        character_cell_size: &SizeInPixels,
    ) -> Vec<SixelImageChunk> {
        let z_index = z_index.unwrap_or(0);
        let mut chunks_to_check: Vec<SixelImageChunk> = sixel_image_chunks.drain(..).collect();
        let panes_to_check = self.layers.iter().skip(z_index);
        for pane_geom in panes_to_check {
            let chunks_to_check_against_this_pane: Vec<SixelImageChunk> =
                chunks_to_check.drain(..).collect();
            for s_chunk in chunks_to_check_against_this_pane {
                let mut uncovered_chunks =
                    self.remove_covered_sixel_parts(pane_geom, &s_chunk, character_cell_size);
                chunks_to_check.append(&mut uncovered_chunks);
            }
        }
        chunks_to_check
    }
    pub fn visible_kitty_image_chunks(
        &self,
        mut kitty_image_chunks: Vec<KittyImageChunk>,
        z_index: Option<usize>,
        character_cell_size: &SizeInPixels,
    ) -> Vec<KittyImageChunk> {
        let z_index = z_index.unwrap_or(0);
        let mut chunks_to_check: Vec<KittyImageChunk> = kitty_image_chunks.drain(..).collect();
        let panes_to_check = self.layers.iter().skip(z_index);
        for pane_geom in panes_to_check {
            let chunks_to_check_against_this_pane: Vec<KittyImageChunk> =
                chunks_to_check.drain(..).collect();
            for k_chunk in chunks_to_check_against_this_pane {
                let mut uncovered_chunks =
                    self.remove_covered_kitty_parts(pane_geom, &k_chunk, character_cell_size);
                chunks_to_check.append(&mut uncovered_chunks);
            }
        }
        chunks_to_check
    }
    fn remove_covered_kitty_parts(
        &self,
        pane_geom: &PaneGeom,
        k_chunk: &KittyImageChunk,
        character_cell_size: &SizeInPixels,
    ) -> Vec<KittyImageChunk> {
        let rounded_kitty_image_pixel_height =
            if k_chunk.source_px_height % character_cell_size.height > 0 {
                let modulus = k_chunk.source_px_height % character_cell_size.height;
                k_chunk.source_px_height + (character_cell_size.height - modulus)
            } else {
                k_chunk.source_px_height
            };
        let rounded_kitty_image_pixel_width =
            if k_chunk.source_px_width % character_cell_size.width > 0 {
                let modulus = k_chunk.source_px_width % character_cell_size.width;
                k_chunk.source_px_width + (character_cell_size.width - modulus)
            } else {
                k_chunk.source_px_width
            };

        let pane_top_edge = pane_geom.y * character_cell_size.height;
        let pane_left_edge = pane_geom.x * character_cell_size.width;
        let pane_bottom_edge = (pane_geom.y + pane_geom.rows.as_usize().saturating_sub(1))
            * character_cell_size.height;
        let pane_right_edge =
            (pane_geom.x + pane_geom.cols.as_usize().saturating_sub(1)) * character_cell_size.width;
        let k_chunk_top_edge = k_chunk.cell_y * character_cell_size.height;
        let k_chunk_bottom_edge = k_chunk_top_edge + rounded_kitty_image_pixel_height;
        let k_chunk_left_edge = k_chunk.cell_x * character_cell_size.width;
        let k_chunk_right_edge = k_chunk_left_edge + rounded_kitty_image_pixel_width;

        let mut uncovered_chunks = vec![];
        let pane_covers_chunk_completely = pane_top_edge <= k_chunk_top_edge
            && pane_bottom_edge >= k_chunk_bottom_edge
            && pane_left_edge <= k_chunk_left_edge
            && pane_right_edge >= k_chunk_right_edge;
        let pane_intersects_with_chunk_vertically = (pane_left_edge >= k_chunk_left_edge
            && pane_left_edge <= k_chunk_right_edge)
            || (pane_right_edge >= k_chunk_left_edge && pane_right_edge <= k_chunk_right_edge)
            || (pane_left_edge <= k_chunk_left_edge && pane_right_edge >= k_chunk_right_edge);
        let pane_intersects_with_chunk_horizontally = (pane_top_edge >= k_chunk_top_edge
            && pane_top_edge <= k_chunk_bottom_edge)
            || (pane_bottom_edge >= k_chunk_top_edge && pane_bottom_edge <= k_chunk_bottom_edge)
            || (pane_top_edge <= k_chunk_top_edge && pane_bottom_edge >= k_chunk_bottom_edge);
        if pane_covers_chunk_completely {
            return uncovered_chunks;
        }
        if pane_top_edge >= k_chunk_top_edge
            && pane_top_edge <= k_chunk_bottom_edge
            && pane_intersects_with_chunk_vertically
        {
            let top_image_chunk = KittyImageChunk {
                cell_x: k_chunk.cell_x,
                cell_y: k_chunk.cell_y,
                source_px_x: k_chunk.source_px_x,
                source_px_y: k_chunk.source_px_y,
                source_px_width: rounded_kitty_image_pixel_width,
                source_px_height: pane_top_edge - k_chunk_top_edge,
                ..*k_chunk
            };
            uncovered_chunks.push(top_image_chunk);
        }
        if pane_bottom_edge <= k_chunk_bottom_edge
            && pane_bottom_edge >= k_chunk_top_edge
            && pane_intersects_with_chunk_vertically
        {
            let bottom_image_chunk = KittyImageChunk {
                cell_x: k_chunk.cell_x,
                cell_y: (pane_bottom_edge / character_cell_size.height) + 1,
                source_px_x: k_chunk.source_px_x,
                source_px_y: k_chunk.source_px_y
                    + (pane_bottom_edge - k_chunk_top_edge)
                    + character_cell_size.height,
                source_px_width: rounded_kitty_image_pixel_width,
                source_px_height: (rounded_kitty_image_pixel_height
                    - (pane_bottom_edge - k_chunk_top_edge))
                    .saturating_sub(character_cell_size.height),
                ..*k_chunk
            };
            uncovered_chunks.push(bottom_image_chunk);
        }
        if pane_left_edge >= k_chunk_left_edge
            && pane_left_edge <= k_chunk_right_edge
            && pane_intersects_with_chunk_horizontally
        {
            let source_px_y = if k_chunk_top_edge < pane_top_edge {
                k_chunk.source_px_y + (pane_top_edge - k_chunk_top_edge)
            } else {
                k_chunk.source_px_y
            };
            let max_image_height = if k_chunk_top_edge < pane_top_edge {
                rounded_kitty_image_pixel_height.saturating_sub(pane_top_edge - k_chunk_top_edge)
            } else {
                rounded_kitty_image_pixel_height
            };
            let left_image_chunk = KittyImageChunk {
                cell_x: k_chunk.cell_x,
                cell_y: std::cmp::max(k_chunk.cell_y, pane_top_edge / character_cell_size.height),
                source_px_x: k_chunk.source_px_x,
                source_px_y,
                source_px_width: rounded_kitty_image_pixel_width
                    .saturating_sub(k_chunk_right_edge.saturating_sub(pane_left_edge)),
                source_px_height: std::cmp::min(
                    pane_bottom_edge - pane_top_edge + character_cell_size.height,
                    max_image_height,
                ),
                ..*k_chunk
            };
            uncovered_chunks.push(left_image_chunk);
        }
        if pane_right_edge <= k_chunk_right_edge
            && pane_right_edge >= k_chunk_left_edge
            && pane_intersects_with_chunk_horizontally
        {
            let source_px_y = if k_chunk_top_edge < pane_top_edge {
                k_chunk.source_px_y + (pane_top_edge - k_chunk_top_edge)
            } else {
                k_chunk.source_px_y
            };
            let max_image_height = if k_chunk_top_edge < pane_top_edge {
                rounded_kitty_image_pixel_height.saturating_sub(pane_top_edge - k_chunk_top_edge)
            } else {
                rounded_kitty_image_pixel_height
            };
            let source_px_x = k_chunk.source_px_x
                + (pane_right_edge - k_chunk_left_edge)
                + character_cell_size.width;
            let right_image_chunk = KittyImageChunk {
                cell_x: (pane_right_edge / character_cell_size.width) + 1,
                cell_y: std::cmp::max(k_chunk.cell_y, pane_top_edge / character_cell_size.height),
                source_px_x,
                source_px_y,
                source_px_width: (rounded_kitty_image_pixel_width
                    .saturating_sub(pane_right_edge - k_chunk_left_edge))
                .saturating_sub(character_cell_size.width),
                source_px_height: std::cmp::min(
                    pane_bottom_edge - pane_top_edge + character_cell_size.height,
                    max_image_height,
                ),
                ..*k_chunk
            };
            uncovered_chunks.push(right_image_chunk);
        }
        if uncovered_chunks.is_empty() {
            uncovered_chunks.push(*k_chunk);
        }
        uncovered_chunks
            .into_iter()
            .filter(|chunk| chunk.source_px_width > 0 && chunk.source_px_height > 0)
            .collect()
    }
    fn remove_covered_parts(
        &self,
        pane_geom: &PaneGeom,
        c_chunk: &mut CharacterChunk,
    ) -> Result<Option<CharacterChunk>> {
        let err_context = || {
            format!(
                "failed to remove covered parts from floating panes: {:#?}",
                self
            )
        };

        let pane_top_edge = pane_geom.y;
        let pane_left_edge = pane_geom.x;
        let pane_bottom_edge = pane_geom.y + pane_geom.rows.as_usize().saturating_sub(1);
        let pane_right_edge = pane_geom.x + pane_geom.cols.as_usize().saturating_sub(1);
        let c_chunk_left_side = c_chunk.x;
        let c_chunk_right_side = c_chunk.x + (c_chunk.width()).saturating_sub(1);
        if pane_top_edge <= c_chunk.y && pane_bottom_edge >= c_chunk.y {
            if pane_left_edge <= c_chunk_left_side && pane_right_edge >= c_chunk_right_side {
                // pane covers chunk completely
                drop(c_chunk.terminal_characters.drain(..));
                return Ok(None);
            } else if pane_right_edge >= c_chunk_left_side
                && pane_right_edge < c_chunk_right_side
                && pane_left_edge <= c_chunk_left_side
            {
                // pane covers chunk partially to the left
                let covered_part = c_chunk.drain_by_width(pane_right_edge + 1 - c_chunk_left_side);
                drop(covered_part);
                c_chunk.x = pane_right_edge + 1;
                return Ok(None);
            } else if pane_left_edge >= c_chunk_left_side
                && pane_left_edge >= c_chunk_left_side
                && pane_right_edge >= c_chunk_right_side
            {
                // pane covers chunk partially to the right
                c_chunk.retain_by_width(pane_left_edge - c_chunk_left_side);
                return Ok(None);
            } else if pane_left_edge >= c_chunk_left_side && pane_right_edge <= c_chunk_right_side {
                // pane covers chunk middle
                let (left_chunk_characters, right_chunk_characters) = c_chunk
                    .cut_middle_out(
                        pane_left_edge - c_chunk_left_side,
                        (pane_right_edge + 1) - c_chunk_left_side,
                    )
                    .with_context(err_context)?;
                let left_chunk_x = c_chunk_left_side;
                let right_chunk_x = pane_right_edge + 1;
                let mut left_chunk =
                    CharacterChunk::new(left_chunk_characters, left_chunk_x, c_chunk.y);
                left_chunk.pane_default_fg = c_chunk.pane_default_fg;
                left_chunk.pane_default_bg = c_chunk.pane_default_bg;
                left_chunk.changed_colors = c_chunk.changed_colors;
                if !c_chunk.selection_and_colors.is_empty() {
                    left_chunk.selection_and_colors = c_chunk.selection_and_colors.clone();
                }

                c_chunk.x = right_chunk_x;
                c_chunk.terminal_characters = right_chunk_characters;
                return Ok(Some(left_chunk));
            }
        };
        Ok(None)
    }
    fn remove_covered_sixel_parts(
        &self,
        pane_geom: &PaneGeom,
        s_chunk: &SixelImageChunk,
        character_cell_size: &SizeInPixels,
    ) -> Vec<SixelImageChunk> {
        // round these up to the nearest cell edge
        let rounded_sixel_image_pixel_height =
            if s_chunk.sixel_image_pixel_height % character_cell_size.height > 0 {
                let modulus = s_chunk.sixel_image_pixel_height % character_cell_size.height;
                s_chunk.sixel_image_pixel_height + (character_cell_size.height - modulus)
            } else {
                s_chunk.sixel_image_pixel_height
            };
        let rounded_sixel_image_pixel_width =
            if s_chunk.sixel_image_pixel_width % character_cell_size.width > 0 {
                let modulus = s_chunk.sixel_image_pixel_width % character_cell_size.width;
                s_chunk.sixel_image_pixel_width + (character_cell_size.width - modulus)
            } else {
                s_chunk.sixel_image_pixel_width
            };

        let pane_top_edge = pane_geom.y * character_cell_size.height;
        let pane_left_edge = pane_geom.x * character_cell_size.width;
        let pane_bottom_edge = (pane_geom.y + pane_geom.rows.as_usize().saturating_sub(1))
            * character_cell_size.height;
        let pane_right_edge =
            (pane_geom.x + pane_geom.cols.as_usize().saturating_sub(1)) * character_cell_size.width;
        let s_chunk_top_edge = s_chunk.cell_y * character_cell_size.height;
        let s_chunk_bottom_edge = s_chunk_top_edge + rounded_sixel_image_pixel_height;
        let s_chunk_left_edge = s_chunk.cell_x * character_cell_size.width;
        let s_chunk_right_edge = s_chunk_left_edge + rounded_sixel_image_pixel_width;

        let mut uncovered_chunks = vec![];
        let pane_covers_chunk_completely = pane_top_edge <= s_chunk_top_edge
            && pane_bottom_edge >= s_chunk_bottom_edge
            && pane_left_edge <= s_chunk_left_edge
            && pane_right_edge >= s_chunk_right_edge;
        let pane_intersects_with_chunk_vertically = (pane_left_edge >= s_chunk_left_edge
            && pane_left_edge <= s_chunk_right_edge)
            || (pane_right_edge >= s_chunk_left_edge && pane_right_edge <= s_chunk_right_edge)
            || (pane_left_edge <= s_chunk_left_edge && pane_right_edge >= s_chunk_right_edge);
        let pane_intersects_with_chunk_horizontally = (pane_top_edge >= s_chunk_top_edge
            && pane_top_edge <= s_chunk_bottom_edge)
            || (pane_bottom_edge >= s_chunk_top_edge && pane_bottom_edge <= s_chunk_bottom_edge)
            || (pane_top_edge <= s_chunk_top_edge && pane_bottom_edge >= s_chunk_bottom_edge);
        if pane_covers_chunk_completely {
            return uncovered_chunks;
        }
        if pane_top_edge >= s_chunk_top_edge
            && pane_top_edge <= s_chunk_bottom_edge
            && pane_intersects_with_chunk_vertically
        {
            // pane covers image bottom
            let top_image_chunk = SixelImageChunk {
                cell_x: s_chunk.cell_x,
                cell_y: s_chunk.cell_y,
                sixel_image_pixel_x: s_chunk.sixel_image_pixel_x,
                sixel_image_pixel_y: s_chunk.sixel_image_pixel_y,
                sixel_image_pixel_width: rounded_sixel_image_pixel_width,
                sixel_image_pixel_height: pane_top_edge - s_chunk_top_edge,
                sixel_image_id: s_chunk.sixel_image_id,
            };
            uncovered_chunks.push(top_image_chunk);
        }
        if pane_bottom_edge <= s_chunk_bottom_edge
            && pane_bottom_edge >= s_chunk_top_edge
            && pane_intersects_with_chunk_vertically
        {
            // pane covers image top
            let bottom_image_chunk = SixelImageChunk {
                cell_x: s_chunk.cell_x,
                cell_y: (pane_bottom_edge / character_cell_size.height) + 1,
                sixel_image_pixel_x: s_chunk.sixel_image_pixel_x,
                sixel_image_pixel_y: s_chunk.sixel_image_pixel_y
                    + (pane_bottom_edge - s_chunk_top_edge)
                    + character_cell_size.height,
                sixel_image_pixel_width: rounded_sixel_image_pixel_width,
                sixel_image_pixel_height: (rounded_sixel_image_pixel_height
                    - (pane_bottom_edge - s_chunk_top_edge))
                    .saturating_sub(character_cell_size.height),
                sixel_image_id: s_chunk.sixel_image_id,
            };
            uncovered_chunks.push(bottom_image_chunk);
        }
        if pane_left_edge >= s_chunk_left_edge
            && pane_left_edge <= s_chunk_right_edge
            && pane_intersects_with_chunk_horizontally
        {
            // pane covers image right
            let sixel_image_pixel_y = if s_chunk_top_edge < pane_top_edge {
                s_chunk.sixel_image_pixel_y + (pane_top_edge - s_chunk_top_edge)
            } else {
                s_chunk.sixel_image_pixel_y
            };
            let max_image_height = if s_chunk_top_edge < pane_top_edge {
                rounded_sixel_image_pixel_height.saturating_sub(pane_top_edge - s_chunk_top_edge)
            } else {
                rounded_sixel_image_pixel_height
            };
            let left_image_chunk = SixelImageChunk {
                cell_x: s_chunk.cell_x,
                // if the pane_top_edge is lower than the image, we want to start there, because we
                // already cut that part above when checking if the pane covered the chunk bottom
                cell_y: std::cmp::max(s_chunk.cell_y, pane_top_edge / character_cell_size.height),
                sixel_image_pixel_x: s_chunk.sixel_image_pixel_x,
                sixel_image_pixel_y,
                sixel_image_pixel_width: rounded_sixel_image_pixel_width
                    .saturating_sub(s_chunk_right_edge.saturating_sub(pane_left_edge)),
                sixel_image_pixel_height: std::cmp::min(
                    pane_bottom_edge - pane_top_edge + character_cell_size.height,
                    max_image_height,
                ),
                sixel_image_id: s_chunk.sixel_image_id,
            };
            uncovered_chunks.push(left_image_chunk);
        }
        if pane_right_edge <= s_chunk_right_edge
            && pane_right_edge >= s_chunk_left_edge
            && pane_intersects_with_chunk_horizontally
        {
            // pane covers image left
            let sixel_image_pixel_y = if s_chunk_top_edge < pane_top_edge {
                s_chunk.sixel_image_pixel_y + (pane_top_edge - s_chunk_top_edge)
            } else {
                s_chunk.sixel_image_pixel_y
            };
            let max_image_height = if s_chunk_top_edge < pane_top_edge {
                rounded_sixel_image_pixel_height.saturating_sub(pane_top_edge - s_chunk_top_edge)
            } else {
                rounded_sixel_image_pixel_height
            };
            let sixel_image_pixel_x = s_chunk.sixel_image_pixel_x
                + (pane_right_edge - s_chunk_left_edge)
                + character_cell_size.width;
            let right_image_chunk = SixelImageChunk {
                cell_x: (pane_right_edge / character_cell_size.width) + 1,
                // if the pane_top_edge is lower than the image, we want to start there, because we
                // already cut that part above when checking if the pane covered the chunk bottom
                cell_y: std::cmp::max(s_chunk.cell_y, pane_top_edge / character_cell_size.height),
                sixel_image_pixel_x,
                sixel_image_pixel_y,
                sixel_image_pixel_width: (rounded_sixel_image_pixel_width
                    .saturating_sub(pane_right_edge - s_chunk_left_edge))
                .saturating_sub(character_cell_size.width),
                sixel_image_pixel_height: std::cmp::min(
                    pane_bottom_edge - pane_top_edge + character_cell_size.height,
                    max_image_height,
                ),
                sixel_image_id: s_chunk.sixel_image_id,
            };
            uncovered_chunks.push(right_image_chunk);
        }
        if uncovered_chunks.is_empty() {
            // the pane doesn't cover the chunk at all, so we return it as is
            uncovered_chunks.push(*s_chunk);
        }
        uncovered_chunks
    }
    pub fn cursor_is_visible(
        &self,
        cursor_x: usize,
        cursor_y: usize,
        z_index: Option<usize>,
    ) -> bool {
        let z_index = z_index.map(|z| z + 1).unwrap_or(0); // +1 because we only check panes above the active pane
        let panes_to_check = self.layers.iter().skip(z_index);
        for pane_geom in panes_to_check {
            let pane_top_edge = pane_geom.y;
            let pane_left_edge = pane_geom.x;
            let pane_bottom_edge = pane_geom.y + pane_geom.rows.as_usize().saturating_sub(1);
            let pane_right_edge = pane_geom.x + pane_geom.cols.as_usize().saturating_sub(1);
            if pane_top_edge <= cursor_y
                && pane_bottom_edge >= cursor_y
                && pane_left_edge <= cursor_x
                && pane_right_edge >= cursor_x
            {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, Default)]
pub struct CharacterChunk {
    pub terminal_characters: Vec<TerminalCharacter>,
    pub x: usize,
    pub y: usize,
    pub changed_colors: Option<[Option<AnsiCode>; 256]>,
    pub pane_default_fg: Option<AnsiCode>,
    pub pane_default_bg: Option<AnsiCode>,
    selection_and_colors: Vec<HighlightSelection>,
}

pub use crate::panes::kitty_graphics::grid_state::KittyImageChunk;

#[derive(Debug, Clone, Copy, Default)]
pub struct SixelImageChunk {
    pub cell_x: usize,
    pub cell_y: usize,
    pub sixel_image_pixel_x: usize,
    pub sixel_image_pixel_y: usize,
    pub sixel_image_pixel_width: usize,
    pub sixel_image_pixel_height: usize,
    pub sixel_image_id: usize,
}

impl CharacterChunk {
    pub fn new(terminal_characters: Vec<TerminalCharacter>, x: usize, y: usize) -> Self {
        CharacterChunk {
            terminal_characters,
            x,
            y,
            ..Default::default()
        }
    }
    pub fn add_selection_and_colors(
        &mut self,
        highlight: HighlightSelection,
        offset_x: usize,
        offset_y: usize,
    ) {
        self.selection_and_colors.push(HighlightSelection {
            selection: highlight.selection.offset(offset_x, offset_y),
            ..highlight
        });
    }
    pub fn selection_and_colors(&self) -> &[HighlightSelection] {
        &self.selection_and_colors
    }
    pub fn add_changed_colors(&mut self, changed_colors: Option<[Option<AnsiCode>; 256]>) {
        self.changed_colors = changed_colors;
    }
    pub fn add_pane_defaults(&mut self, fg: Option<AnsiCode>, bg: Option<AnsiCode>) {
        self.pane_default_fg = fg;
        self.pane_default_bg = bg;
    }
    pub fn changed_colors(&self) -> Option<[Option<AnsiCode>; 256]> {
        self.changed_colors
    }
    pub fn width(&self) -> usize {
        let mut width = 0;
        for t_character in &self.terminal_characters {
            width += t_character.width()
        }
        width
    }
    pub fn drain_by_width(&mut self, x: usize) -> impl Iterator<Item = TerminalCharacter> {
        let mut drained_part: VecDeque<TerminalCharacter> = VecDeque::new();
        let mut drained_part_len = 0;
        loop {
            if self.terminal_characters.is_empty() {
                break;
            }
            let next_character = self.terminal_characters.remove(0); // TODO: consider copying self.terminal_characters into a VecDeque to make this process faster?
            if drained_part_len + next_character.width() <= x {
                drained_part_len += next_character.width();
                drained_part.push_back(next_character);
            } else {
                if drained_part_len == x {
                    self.terminal_characters.insert(0, next_character); // put it back
                } else if next_character.width() > 1 {
                    for _ in 1..next_character.width() {
                        self.terminal_characters.insert(0, EMPTY_TERMINAL_CHARACTER);
                        drained_part.push_back(EMPTY_TERMINAL_CHARACTER);
                    }
                }
                break;
            }
        }
        drained_part.into_iter()
    }
    pub fn retain_by_width(&mut self, x: usize) {
        let part_to_retain = self.drain_by_width(x);
        self.terminal_characters = part_to_retain.collect();
    }
    pub fn cut_middle_out(
        &mut self,
        middle_start: usize,
        middle_end: usize,
    ) -> Result<(Vec<TerminalCharacter>, Vec<TerminalCharacter>)> {
        let err_context = || "failed to cut middle out of character chunk".to_string();

        let (
            absolute_middle_start_index,
            absolute_middle_end_index,
            pad_left_end_by,
            pad_right_start_by,
        ) = adjust_middle_segment_for_wide_chars(
            middle_start,
            middle_end,
            &self.terminal_characters,
        )
        .with_context(err_context)?;
        let mut terminal_characters: Vec<TerminalCharacter> =
            self.terminal_characters.drain(..).collect();
        let mut characters_on_the_right: Vec<TerminalCharacter> = terminal_characters
            .drain(absolute_middle_end_index..)
            .collect();
        let mut characters_on_the_left: Vec<TerminalCharacter> = terminal_characters
            .drain(..absolute_middle_start_index)
            .collect();
        if pad_left_end_by > 0 {
            characters_on_the_left.resize(pad_left_end_by, EMPTY_TERMINAL_CHARACTER);
        }
        if pad_right_start_by > 0 {
            for _ in 0..pad_right_start_by {
                characters_on_the_right.insert(0, EMPTY_TERMINAL_CHARACTER);
            }
        }
        Ok((characters_on_the_left, characters_on_the_right))
    }
}

#[derive(Clone, Debug)]
pub struct OutputBuffer {
    pub changed_lines: HashSet<usize>, // line index
    pub should_update_all_lines: bool,
    styled_underlines: bool,
    last_changed_line: Option<usize>,
}

impl Default for OutputBuffer {
    fn default() -> Self {
        OutputBuffer {
            changed_lines: HashSet::new(),
            should_update_all_lines: true, // first time we should do a full render
            styled_underlines: true,
            last_changed_line: None,
        }
    }
}

impl OutputBuffer {
    pub fn update_line(&mut self, line_index: usize) {
        if self.should_update_all_lines || self.last_changed_line == Some(line_index) {
            return;
        }
        self.changed_lines.insert(line_index);
        self.last_changed_line = Some(line_index);
    }
    pub fn update_lines(&mut self, start: usize, end: usize) {
        if !self.should_update_all_lines {
            for idx in start..=end {
                if !self.changed_lines.contains(&idx) {
                    self.changed_lines.insert(idx);
                }
            }
            self.last_changed_line = None;
        }
    }
    pub fn update_all_lines(&mut self) {
        self.clear();
        self.should_update_all_lines = true;
    }
    pub fn clear(&mut self) {
        self.changed_lines.clear();
        self.should_update_all_lines = false;
        self.last_changed_line = None;
    }
    pub fn serialize(
        &self,
        viewport: &[Row],
        osc8_hyperlinks: bool,
        max_size: Option<Size>,
    ) -> Result<String> {
        let mut chunks = Vec::new();
        let mut chunk_starts_a_line = Vec::new();
        for (line_index, line) in viewport.iter().enumerate() {
            let terminal_characters =
                self.extract_line_from_viewport(line_index, viewport, line.width());

            let x = 0;
            let y = line_index;
            chunks.push(CharacterChunk::new(terminal_characters, x, y));
            chunk_starts_a_line.push(line.is_canonical || line_index == 0);
        }
        serialize_chunks_with_newlines(
            chunks,
            &chunk_starts_a_line,
            None,
            None,
            self.styled_underlines,
            osc8_hyperlinks,
            max_size,
        )
    }
    pub fn changed_chunks_in_viewport(
        &self,
        viewport: &[Row],
        viewport_width: usize,
        viewport_height: usize,
        x_offset: usize,
        y_offset: usize,
    ) -> Vec<CharacterChunk> {
        if self.should_update_all_lines {
            let mut changed_chunks = Vec::new();
            for line_index in 0..viewport_height {
                let terminal_characters =
                    self.extract_line_from_viewport(line_index, viewport, viewport_width);

                let x = x_offset; // right now we only buffer full lines as this doesn't seem to have a huge impact on performance, but the infra is here if we want to change this
                let y = line_index + y_offset;
                changed_chunks.push(CharacterChunk::new(terminal_characters, x, y));
            }
            changed_chunks
        } else {
            let mut line_changes: Vec<_> = self
                .changed_lines
                .iter()
                .filter(|i| *i < &viewport_height)
                .copied()
                .collect();
            line_changes.sort_unstable();
            let mut changed_chunks = Vec::new();
            for line_index in line_changes {
                let terminal_characters =
                    self.extract_line_from_viewport(line_index, viewport, viewport_width);
                let x = x_offset;
                let y = line_index + y_offset;
                changed_chunks.push(CharacterChunk::new(terminal_characters, x, y));
            }
            changed_chunks
        }
    }
    fn extract_characters_from_row(
        &self,
        row: &Row,
        viewport_width: usize,
    ) -> Vec<TerminalCharacter> {
        let mut terminal_characters: Vec<TerminalCharacter> = row.columns.iter().cloned().collect();
        // pad row
        let row_width = row.width();
        if row_width < viewport_width {
            let mut pad_character = EMPTY_TERMINAL_CHARACTER;
            if let Some(bg_color) = row.bg_color {
                pad_character
                    .styles
                    .update(|styles| styles.background = Some(bg_color));
            }
            let mut padding = vec![pad_character; viewport_width - row_width];
            terminal_characters.append(&mut padding);
        } else if row_width > viewport_width {
            let width_offset = row.excess_width_until(viewport_width);
            let truncate_position = viewport_width.saturating_sub(width_offset);
            if truncate_position < terminal_characters.len() {
                terminal_characters.truncate(truncate_position);
            }
        }
        terminal_characters
    }
    fn extract_line_from_viewport(
        &self,
        line_index: usize,
        viewport: &[Row],
        viewport_width: usize,
    ) -> Vec<TerminalCharacter> {
        match viewport.get(line_index) {
            // TODO: iterator?
            Some(row) => self.extract_characters_from_row(row, viewport_width),
            None => {
                vec![EMPTY_TERMINAL_CHARACTER; viewport_width]
            },
        }
    }
    pub fn changed_rects_in_viewport(&self, viewport_height: usize) -> HashMap<usize, usize> {
        // group the changed lines into "changed_rects", which indicate where the line starts (the
        // hashmap key) and how many lines are in there (its value)
        let mut changed_rects: HashMap<usize, usize> = HashMap::new(); // <start_line_index, line_count>
        let mut last_changed_line_index: Option<usize> = None;
        let mut changed_line_count = 0;
        let mut add_changed_line = |line_index| match last_changed_line_index.as_mut() {
            Some(changed_line_index) => {
                if *changed_line_index + changed_line_count == line_index {
                    changed_line_count += 1
                } else {
                    changed_rects.insert(*changed_line_index, changed_line_count);
                    last_changed_line_index = Some(line_index);
                    changed_line_count = 1;
                }
            },
            None => {
                last_changed_line_index = Some(line_index);
                changed_line_count = 1;
            },
        };

        // TODO: move this whole thing to output_buffer
        if self.should_update_all_lines {
            // for line_index in 0..self.viewport.len() {
            for line_index in 0..viewport_height {
                add_changed_line(line_index);
            }
        } else {
            for line_index in self.changed_lines.iter().copied() {
                add_changed_line(line_index);
            }
        }
        if let Some(changed_line_index) = last_changed_line_index {
            changed_rects.insert(changed_line_index, changed_line_count);
        }
        changed_rects
    }
}

#[cfg(test)]
mod unit;
