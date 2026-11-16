use crate::render::renderer::bytemuck_cast;
use crate::render::renderer::Pass;
use crate::render::renderer::SLOT_QUADS;
use crate::render::renderer::VERTICES_PER_QUAD;
use crate::render::renderer::WHOLE_BUFFER_DIVISOR;
use crate::render::scene::RowScene;
use glow::HasContext;

pub(super) struct Slot {
    pub(super) offset: usize,
    pub(super) capacity: usize,
    pub(super) len: usize,
}

pub(super) struct RowVertices {
    pub(super) stride_floats: usize,
    pub(super) data: Vec<f32>,
    pub(super) slots: Vec<Slot>,
    pub(super) uploaded: usize,
    pub(super) atlas_size: (u32, u32),
}

impl RowVertices {
    pub(super) fn new(stride: i32) -> Self {
        Self {
            stride_floats: stride as usize / 4,
            data: Vec::new(),
            slots: Vec::new(),
            uploaded: 0,
            atlas_size: (0, 0),
        }
    }

    pub(super) fn quad_floats(&self) -> usize {
        self.stride_floats * VERTICES_PER_QUAD
    }

    pub(super) fn capacity_for(&self, len: usize) -> usize {
        let quad = self.quad_floats();
        let quads = len.div_ceil(quad).max(1);
        quads.div_ceil(SLOT_QUADS) * SLOT_QUADS * quad
    }

    pub(super) fn vertices(&self) -> i32 {
        (self.data.len() / self.stride_floats) as i32
    }
}

#[derive(Clone, Copy)]
pub(super) struct RowChanges<'a> {
    pub(super) rows: &'a [RowScene],
    pub(super) rebuilt: &'a [usize],
    pub(super) everything: bool,
}

pub(super) fn sync_rows(
    gl: &glow::Context,
    pass: &Pass,
    buffer: &mut RowVertices,
    scratch: &mut Vec<f32>,
    RowChanges {
        rows,
        rebuilt,
        everything,
    }: RowChanges<'_>,
    atlas_size: (u32, u32),
    mut convert: impl FnMut(&RowScene, &mut Vec<f32>),
) {
    let relaid = everything || buffer.slots.len() != rows.len() || buffer.atlas_size != atlas_size;
    buffer.atlas_size = atlas_size;
    if relaid || rebuilt.len() * WHOLE_BUFFER_DIVISOR > rows.len() {
        lay_out(buffer, rows, &mut convert);
        upload_whole(gl, pass, buffer);
        return;
    }
    if rebuilt.is_empty() {
        return;
    }

    let mut ranges: Vec<(usize, usize)> = Vec::with_capacity(rebuilt.len());
    for row in rebuilt {
        scratch.clear();
        convert(&rows[*row], scratch);
        match write_slot(buffer, *row, scratch) {
            Some(range) => ranges.push(range),
            None => {
                lay_out(buffer, rows, &mut convert);
                upload_whole(gl, pass, buffer);
                return;
            },
        }
    }

    if buffer.uploaded != buffer.data.len() {
        upload_whole(gl, pass, buffer);
        return;
    }
    unsafe {
        gl.bind_vertex_array(Some(pass.vertex_array));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(pass.buffer));
        for (offset, len) in ranges {
            gl.buffer_sub_data_u8_slice(
                glow::ARRAY_BUFFER,
                (offset * 4) as i32,
                bytemuck_cast(&buffer.data[offset..offset + len]),
            );
        }
    }
}

pub(super) fn lay_out(
    buffer: &mut RowVertices,
    rows: &[RowScene],
    convert: &mut impl FnMut(&RowScene, &mut Vec<f32>),
) {
    let held: Vec<usize> = if buffer.slots.len() == rows.len() {
        buffer.slots.iter().map(|slot| slot.capacity).collect()
    } else {
        vec![0; rows.len()]
    };
    let mut data = std::mem::take(&mut buffer.data);
    data.clear();
    buffer.slots.clear();
    for (row, scene) in rows.iter().enumerate() {
        let offset = data.len();
        convert(scene, &mut data);
        let len = data.len() - offset;
        let capacity = buffer.capacity_for(len).max(held[row]);
        data.resize(offset + capacity, 0.0);
        buffer.slots.push(Slot {
            offset,
            capacity,
            len,
        });
    }
    buffer.data = data;
}

pub(super) fn write_slot(
    buffer: &mut RowVertices,
    row: usize,
    vertices: &[f32],
) -> Option<(usize, usize)> {
    let slot = buffer.slots.get_mut(row)?;
    if vertices.len() > slot.capacity {
        return None;
    }
    let (offset, capacity) = (slot.offset, slot.capacity);
    slot.len = vertices.len();
    buffer.data[offset..offset + vertices.len()].copy_from_slice(vertices);
    buffer.data[offset + vertices.len()..offset + capacity].fill(0.0);
    Some((offset, capacity))
}

pub(super) fn upload_whole(gl: &glow::Context, pass: &Pass, buffer: &mut RowVertices) {
    buffer.uploaded = buffer.data.len();
    if buffer.data.is_empty() {
        return;
    }
    unsafe {
        gl.bind_vertex_array(Some(pass.vertex_array));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(pass.buffer));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            bytemuck_cast(&buffer.data),
            glow::DYNAMIC_DRAW,
        );
    }
}

pub(super) unsafe fn draw_rows(
    gl: &glow::Context,
    pass: &Pass,
    buffer: &RowVertices,
    viewport: (f32, f32),
    origin: (f32, f32),
) {
    if buffer.data.is_empty() {
        return;
    }
    gl.use_program(Some(pass.program));
    gl.uniform_2_f32(pass.viewport.as_ref(), viewport.0, viewport.1);
    gl.uniform_2_f32(pass.origin.as_ref(), origin.0, origin.1);
    gl.bind_vertex_array(Some(pass.vertex_array));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(pass.buffer));
    gl.draw_arrays(glow::TRIANGLES, 0, buffer.vertices());
}
