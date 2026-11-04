use crate::render::graphics::ShaderDialect;
use crate::render::renderer::bytemuck_cast;
use crate::render::renderer::Pass;
use crate::render::renderer::GLYPH_STRIDE;
use crate::render::renderer::SOLID_STRIDE;
use anyhow::{anyhow, Result};
use glow::HasContext;

pub(crate) const SOLID_VERTEX: &str = r#"uniform vec2 u_viewport;
uniform vec2 u_origin;
in vec2 a_position;
in vec3 a_color;
out vec3 v_color;
void main() {
    v_color = a_color;
    vec2 position = a_position + u_origin;
    vec2 ndc = vec2(position.x / u_viewport.x * 2.0 - 1.0,
                    1.0 - position.y / u_viewport.y * 2.0);
    gl_Position = vec4(ndc, 0.0, 1.0);
}
"#;

pub(crate) const SOLID_FRAGMENT: &str = r#"in vec3 v_color;
out vec4 o_color;
void main() {
    o_color = vec4(v_color, 1.0);
}
"#;

pub(crate) const GLYPH_VERTEX: &str = r#"uniform vec2 u_viewport;
uniform vec2 u_origin;
in vec2 a_position;
in vec2 a_texcoord;
in vec3 a_color;
out vec2 v_texcoord;
out vec3 v_color;
void main() {
    v_texcoord = a_texcoord;
    v_color = a_color;
    vec2 position = a_position + u_origin;
    vec2 ndc = vec2(position.x / u_viewport.x * 2.0 - 1.0,
                    1.0 - position.y / u_viewport.y * 2.0);
    gl_Position = vec4(ndc, 0.0, 1.0);
}
"#;

pub(crate) const GLYPH_FRAGMENT: &str = r#"uniform sampler2D u_atlas;
in vec2 v_texcoord;
in vec3 v_color;
out vec4 o_color;
void main() {
    float coverage = texture(u_atlas, v_texcoord).r;
    o_color = vec4(v_color * coverage, coverage);
}
"#;

pub(crate) const COLOR_FRAGMENT: &str = r#"uniform sampler2D u_atlas;
in vec2 v_texcoord;
in vec3 v_color;
out vec4 o_color;
void main() {
    vec4 texel = texture(u_atlas, v_texcoord);
    vec3 tinted = texel.rgb * v_color;
    o_color = vec4(tinted * texel.a, texel.a);
}
"#;

pub(super) unsafe fn premultiplied_blending(gl: &glow::Context) {
    gl.blend_func_separate(
        glow::ONE,
        glow::ONE_MINUS_SRC_ALPHA,
        glow::ONE,
        glow::ONE_MINUS_SRC_ALPHA,
    );
}

pub(super) unsafe fn solid_pass(gl: &glow::Context, program: glow::Program) -> Result<Pass> {
    let pass = Pass {
        viewport: gl.get_uniform_location(program, "u_viewport"),
        origin: gl.get_uniform_location(program, "u_origin"),
        program,
        vertex_array: gl.create_vertex_array().map_err(|e| anyhow!(e))?,
        buffer: gl.create_buffer().map_err(|e| anyhow!(e))?,
    };
    gl.bind_vertex_array(Some(pass.vertex_array));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(pass.buffer));
    attribute(gl, program, "a_position", 2, SOLID_STRIDE, 0);
    attribute(gl, program, "a_color", 3, SOLID_STRIDE, 2 * 4);
    Ok(pass)
}

pub(super) unsafe fn textured_pass(gl: &glow::Context, program: glow::Program) -> Result<Pass> {
    let pass = Pass {
        viewport: gl.get_uniform_location(program, "u_viewport"),
        origin: gl.get_uniform_location(program, "u_origin"),
        program,
        vertex_array: gl.create_vertex_array().map_err(|e| anyhow!(e))?,
        buffer: gl.create_buffer().map_err(|e| anyhow!(e))?,
    };
    gl.bind_vertex_array(Some(pass.vertex_array));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(pass.buffer));
    attribute(gl, program, "a_position", 2, GLYPH_STRIDE, 0);
    attribute(gl, program, "a_texcoord", 2, GLYPH_STRIDE, 2 * 4);
    attribute(gl, program, "a_color", 3, GLYPH_STRIDE, 4 * 4);
    Ok(pass)
}

pub(super) unsafe fn draw_pass(
    gl: &glow::Context,
    pass: &Pass,
    vertices: &[f32],
    stride: i32,
    viewport: (f32, f32),
    origin: (f32, f32),
) {
    if vertices.is_empty() {
        return;
    }
    gl.use_program(Some(pass.program));
    gl.uniform_2_f32(pass.viewport.as_ref(), viewport.0, viewport.1);
    gl.uniform_2_f32(pass.origin.as_ref(), origin.0, origin.1);
    gl.bind_vertex_array(Some(pass.vertex_array));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(pass.buffer));
    gl.buffer_data_u8_slice(
        glow::ARRAY_BUFFER,
        bytemuck_cast(vertices),
        glow::DYNAMIC_DRAW,
    );
    let count = vertices.len() as i32 / (stride / 4);
    gl.draw_arrays(glow::TRIANGLES, 0, count);
}

pub(super) unsafe fn attribute(
    gl: &glow::Context,
    program: glow::Program,
    name: &str,
    size: i32,
    stride: i32,
    offset: i32,
) {
    let location = gl.get_attrib_location(program, name);
    if let Some(location) = location {
        gl.enable_vertex_attrib_array(location);
        gl.vertex_attrib_pointer_f32(location, size, glow::FLOAT, false, stride, offset);
    }
}

pub(super) unsafe fn link(
    gl: &glow::Context,
    dialect: ShaderDialect,
    vertex: &str,
    fragment: &str,
) -> Result<glow::Program> {
    let program = gl.create_program().map_err(|e| anyhow!(e))?;
    let mut shaders = Vec::new();

    for (kind, source) in [
        (glow::VERTEX_SHADER, vertex),
        (glow::FRAGMENT_SHADER, fragment),
    ] {
        let shader = gl.create_shader(kind).map_err(|e| anyhow!(e))?;
        gl.shader_source(shader, &dialect.source(source));
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            return Err(anyhow!(
                "shader compilation failed: {}",
                gl.get_shader_info_log(shader)
            ));
        }
        gl.attach_shader(program, shader);
        shaders.push(shader);
    }

    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        return Err(anyhow!(
            "program link failed: {}",
            gl.get_program_info_log(program)
        ));
    }
    for shader in shaders {
        gl.detach_shader(program, shader);
        gl.delete_shader(shader);
    }

    Ok(program)
}
