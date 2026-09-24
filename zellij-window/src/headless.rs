use anyhow::{anyhow, Context, Result};
use glow::HasContext;
use khronos_egl as egl;

use crate::atlas::Atlases;
use crate::image_io::Image;
use crate::renderer::Renderer;
use crate::scene::Scene;

type Egl = egl::DynamicInstance<egl::EGL1_4>;

const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;
const MAX_TARGET_SIDE: u32 = 16384;

pub struct Headless {
    renderer: Renderer,
    target: Option<Target>,
    egl: Egl,
    display: egl::Display,
    context: egl::Context,
    read_back: Option<ReadBack>,
    scratch: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReadBack {
    Rgba,
    Bgra,
}

struct Target {
    framebuffer: glow::Framebuffer,
    texture: glow::Texture,
    width: u32,
    height: u32,
}

#[cfg(target_os = "linux")]
const EGL_LIBRARY: &str = "libEGL.so.1";

fn load_egl_library() -> Result<libloading::Library> {
    #[cfg(target_os = "linux")]
    {
        unsafe { libloading::Library::new(EGL_LIBRARY) }
            .with_context(|| format!("{} is required for offscreen rendering", EGL_LIBRARY))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(anyhow!(
            "headless rendering is implemented for Linux only: it loads libEGL.so.1 by soname and \
             drives a surfaceless EGL display, neither of which has a counterpart wired up for \
             this platform"
        ))
    }
}

#[cfg(test)]
pub const SKIP_ENV: &str = "ZELLIJ_WINDOW_NO_GPU_TESTS";
#[cfg(test)]
const PROBE_ENV: &str = "ZELLIJ_WINDOW_GPU_PROBE";
#[cfg(test)]
const PROBE_TEST: &str = "headless::tests::the_gpu_probe_this_binary_runs_on_itself";
#[cfg(test)]
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
#[cfg(test)]
const PROBE_POLL: std::time::Duration = std::time::Duration::from_millis(50);

#[cfg(test)]
struct Shared(Headless);

#[cfg(test)]
unsafe impl Send for Shared {}

#[cfg(test)]
static SHARED: std::sync::OnceLock<std::sync::Mutex<Option<Shared>>> = std::sync::OnceLock::new();

#[cfg(test)]
pub struct GpuLease(std::sync::MutexGuard<'static, Option<Shared>>);

#[cfg(test)]
impl GpuLease {
    pub fn get(&mut self) -> &mut Headless {
        &mut self.0.as_mut().expect("a leased context").0
    }
}

#[cfg(test)]
impl Drop for GpuLease {
    fn drop(&mut self) {
        HOLDS_LEASE.with(|held| held.set(false));
        if let Some(shared) = self.0.as_ref() {
            shared.0.release_current();
        }
    }
}

#[cfg(test)]
static BUILDING_SHARED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
thread_local! {
    static HOLDS_LEASE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn assert_the_caller_holds_the_lease() {
    let exempt = BUILDING_SHARED.load(std::sync::atomic::Ordering::SeqCst)
        || std::env::var_os(PROBE_ENV).is_some();
    assert!(
        exempt || HOLDS_LEASE.with(|held| held.get()),
        "a render test built a context without holding Headless::exclusive()"
    );
}

impl Headless {
    pub fn new() -> Result<Self> {
        #[cfg(test)]
        assert_the_caller_holds_the_lease();

        let library = load_egl_library()?;
        let egl = unsafe { Egl::load_required_from(library) }
            .map_err(|e| anyhow!("failed to load EGL entry points: {}", e))?;

        let display = surfaceless_display(&egl)?;
        egl.initialize(display)
            .context("failed to initialize the surfaceless EGL display")?;
        egl.bind_api(egl::OPENGL_ES_API)
            .context("failed to select the OpenGL ES API")?;

        let config = egl
            .choose_first_config(
                display,
                &[
                    egl::SURFACE_TYPE,
                    egl::PBUFFER_BIT,
                    egl::RENDERABLE_TYPE,
                    egl::OPENGL_ES2_BIT,
                    egl::RED_SIZE,
                    8,
                    egl::GREEN_SIZE,
                    8,
                    egl::BLUE_SIZE,
                    8,
                    egl::ALPHA_SIZE,
                    8,
                    egl::NONE,
                ],
            )
            .context("failed to query EGL configs")?
            .ok_or_else(|| anyhow!("no EGL config supports 8-bit RGBA rendering"))?;

        let context = egl
            .create_context(
                display,
                config,
                None,
                &[egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE],
            )
            .context("failed to create an OpenGL ES 3 context")?;
        egl.make_current(display, None, None, Some(context))
            .context("failed to make the surfaceless context current")?;

        let gl = unsafe {
            glow::Context::from_loader_function(|symbol| {
                egl.get_proc_address(symbol)
                    .map(|address| address as *const std::ffi::c_void)
                    .unwrap_or(std::ptr::null())
            })
        };

        Ok(Self {
            renderer: Renderer::new(gl)?,
            target: None,
            egl,
            display,
            context,
            read_back: None,
            scratch: Vec::new(),
        })
    }

    pub fn render(&mut self, scene: &Scene, atlases: Atlases<'_>) -> Result<Image> {
        self.bind_target(scene.width, scene.height)?;
        self.renderer
            .draw(scene, atlases, (scene.width, scene.height));
        Ok(self.read_back(scene.width, scene.height))
    }

    fn bind_target(&mut self, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 {
            return Err(anyhow!("cannot render a {}x{} scene", width, height));
        }
        if width > MAX_TARGET_SIDE || height > MAX_TARGET_SIDE {
            return Err(anyhow!(
                "scene of {}x{} exceeds the {} px render target limit",
                width,
                height,
                MAX_TARGET_SIDE
            ));
        }

        if let Some(target) = &self.target {
            if target.width == width && target.height == height {
                unsafe {
                    self.renderer
                        .gl()
                        .bind_framebuffer(glow::FRAMEBUFFER, Some(target.framebuffer))
                };
                return Ok(());
            }
        }

        let gl = self.renderer.gl();
        unsafe {
            if let Some(previous) = self.target.take() {
                gl.delete_framebuffer(previous.framebuffer);
                gl.delete_texture(previous.texture);
            }

            let texture = gl.create_texture().map_err(|e| anyhow!(e))?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                width as i32,
                height as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );

            let framebuffer = gl.create_framebuffer().map_err(|e| anyhow!(e))?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                return Err(anyhow!(
                    "the {}x{} render target is incomplete",
                    width,
                    height
                ));
            }

            self.target = Some(Target {
                framebuffer,
                texture,
                width,
                height,
            });
        }

        Ok(())
    }

    fn read_back(&mut self, width: u32, height: u32) -> Image {
        let len = (width * height * 4) as usize;
        if self.scratch.len() < len {
            self.scratch.resize(len, 0);
        }
        let read_back = *self
            .read_back
            .get_or_insert_with(|| preferred_read_back(self.renderer.gl()));
        let format = match read_back {
            ReadBack::Rgba => glow::RGBA,
            ReadBack::Bgra => glow::BGRA,
        };
        unsafe {
            self.renderer.gl().read_pixels(
                0,
                0,
                width as i32,
                height as i32,
                format,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut self.scratch[..len])),
            );
        }

        let stride = (width * 4) as usize;
        let mut pixels = Vec::with_capacity(len);
        for row in (0..height as usize).rev() {
            pixels.extend_from_slice(&self.scratch[row * stride..(row + 1) * stride]);
        }
        if read_back == ReadBack::Bgra {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }

        Image {
            width,
            height,
            pixels,
        }
    }
}

#[cfg(test)]
impl Headless {
    pub fn render_retained(
        &mut self,
        scene: &crate::retained::RetainedScene,
        atlases: Atlases<'_>,
    ) -> Result<Image> {
        let (width, height) = (scene.width(), scene.height());
        self.bind_target(width, height)?;
        self.renderer.draw_retained(scene, atlases, (width, height));
        Ok(self.read_back(width, height))
    }

    pub fn available() -> bool {
        static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *AVAILABLE.get_or_init(|| {
            if std::env::var_os(SKIP_ENV).is_some() {
                eprintln!(
                    "zellij-window: {} is set, skipping the render tests",
                    SKIP_ENV
                );
                return false;
            }
            if std::env::var_os(PROBE_ENV).is_some() {
                return true;
            }
            match probe_in_a_child_process() {
                Ok(true) => true,
                Ok(false) => {
                    eprintln!(
                        "zellij-window: this host cannot bring up an offscreen GPU context, skipping the render tests"
                    );
                    false
                },
                Err(e) => {
                    eprintln!(
                        "zellij-window: the GPU probe did not answer ({}), skipping the render tests; set {}=1 to skip it outright",
                        e, SKIP_ENV
                    );
                    false
                },
            }
        })
    }

    pub fn exclusive() -> Option<GpuLease> {
        if !Self::available() {
            return None;
        }

        let mut guard = SHARED
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        if guard.is_none() {
            BUILDING_SHARED.store(true, std::sync::atomic::Ordering::SeqCst);
            let built = Self::new();
            BUILDING_SHARED.store(false, std::sync::atomic::Ordering::SeqCst);
            match built {
                Ok(headless) => {
                    *guard = Some(Shared(headless));
                },
                Err(e) => {
                    eprintln!(
                        "zellij-window: could not build the shared render context: {:#}",
                        e
                    );
                    return None;
                },
            }
        }

        let shared = guard.as_mut().expect("a shared context");
        shared
            .0
            .make_current_here()
            .expect("the shared render context must bind to the thread that leased it");
        shared.0.forget_uploads();
        HOLDS_LEASE.with(|held| held.set(true));
        Some(GpuLease(guard))
    }

    fn make_current_here(&self) -> Result<()> {
        self.egl
            .make_current(self.display, None, None, Some(self.context))
            .context("failed to bind the render context to this thread")
    }

    fn release_current(&self) {
        let _ = self.egl.make_current(self.display, None, None, None);
    }

    pub fn image_textures(&self) -> usize {
        self.renderer.image_textures()
    }

    pub fn read_the_next_frame_as_bgra(&mut self) {
        self.read_back = Some(ReadBack::Bgra);
    }

    pub fn driver_rejected_the_read(&self) -> bool {
        unsafe { self.renderer.gl().get_error() == glow::INVALID_OPERATION }
    }

    fn forget_uploads(&mut self) {
        self.renderer.forget_uploads();
        self.read_back = None;
    }
}

impl Drop for Headless {
    fn drop(&mut self) {
        let gl = self.renderer.gl();
        if let Some(target) = self.target.take() {
            unsafe {
                gl.delete_framebuffer(target.framebuffer);
                gl.delete_texture(target.texture);
            }
        }
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_context(self.display, self.context);
    }
}

#[cfg(test)]
fn probe_in_a_child_process() -> Result<bool> {
    use std::process::{Command, Stdio};
    use std::time::Instant;

    let exe = std::env::current_exe().context("failed to locate the test binary")?;
    let mut child = Command::new(exe)
        .args(["--exact", PROBE_TEST, "--test-threads=1"])
        .env(PROBE_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to start the GPU probe")?;

    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait().context("failed to poll the GPU probe")? {
            Some(status) => return Ok(status.success()),
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(anyhow!("it was still running after {:?}", PROBE_TIMEOUT));
                }
                std::thread::sleep(PROBE_POLL);
            },
        }
    }
}

fn preferred_read_back(gl: &glow::Context) -> ReadBack {
    let (format, kind) = unsafe {
        (
            gl.get_parameter_i32(glow::IMPLEMENTATION_COLOR_READ_FORMAT) as u32,
            gl.get_parameter_i32(glow::IMPLEMENTATION_COLOR_READ_TYPE) as u32,
        )
    };
    if format == glow::BGRA && kind == glow::UNSIGNED_BYTE {
        ReadBack::Bgra
    } else {
        ReadBack::Rgba
    }
}

fn surfaceless_display(egl: &Egl) -> Result<egl::Display> {
    let address = egl
        .get_proc_address("eglGetPlatformDisplay")
        .or_else(|| egl.get_proc_address("eglGetPlatformDisplayEXT"))
        .ok_or_else(|| anyhow!("this EGL implementation has no platform display entry point"))?;

    let raw = unsafe {
        let get_platform_display: unsafe extern "C" fn(
            egl::Enum,
            *mut std::ffi::c_void,
            *const egl::Attrib,
        ) -> egl::EGLDisplay = std::mem::transmute(address);
        get_platform_display(
            PLATFORM_SURFACELESS_MESA,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    if raw.is_null() {
        return Err(anyhow!("the EGL surfaceless platform is unavailable"));
    }

    Ok(unsafe { egl::Display::from_ptr(raw) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas::GlyphCache;
    use crate::color;
    use crate::font::{FontStack, DEFAULT_FONT_SIZE};
    use crate::scene;
    use crate::screen_buffer::painter::{self, Painter, Place};
    use crate::terminal::TerminalState;
    use zellij_utils::structured_render::{GraphicsRecord, WireCell, WireColor};

    fn painted(rows: usize, cols: usize, paint: impl FnOnce(&mut Painter)) -> TerminalState {
        Painter::state(rows, cols, paint)
    }

    fn painted_with_graphics(
        rows: usize,
        cols: usize,
        records: &[GraphicsRecord],
        paint: impl FnOnce(&mut Painter),
    ) -> TerminalState {
        Painter::state(rows, cols, |painter| {
            paint(painter);
            painter.graphics(records);
        })
    }

    fn apply_graphics(state: &mut TerminalState, records: &[GraphicsRecord]) {
        Painter::apply(state, |painter| painter.graphics(records));
    }

    fn draw(gpu: &mut GpuLease, state: &TerminalState) -> (Image, crate::font::CellMetrics) {
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let built = scene::build(state, &mut cache);
        let metrics = cache.metrics();
        (gpu.get().render(&built, cache.atlases()).unwrap(), metrics)
    }

    fn background(index: u8) -> impl Fn(&mut WireCell) {
        move |cell: &mut WireCell| cell.bg = WireColor::Named(index).pack()
    }

    #[test]
    fn a_bgra_read_back_lands_the_same_pixels_as_an_rgba_one() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let paint = |painter: &mut Painter| {
            painter.styled(0, 0, "M", background(1));
            painter.styled(0, 1, "M", background(2));
        };
        let (rgba, _) = draw(&mut gpu, &painted(2, 4, paint));

        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let state = painted(2, 4, paint);
        let built = scene::build(&state, &mut cache);
        let headless = gpu.get();
        headless.read_the_next_frame_as_bgra();
        let bgra = headless.render(&built, cache.atlases()).unwrap();
        if headless.driver_rejected_the_read() {
            eprintln!("zellij-window: this driver cannot read BGRA, skipping the swizzle check");
            return;
        }

        assert_eq!(
            rgba.pixels, bgra.pixels,
            "the BGRA read path must swizzle back to the same image the RGBA path produces"
        );
    }

    #[test]
    fn a_rebuilt_glyph_cache_never_reuses_the_texture_the_previous_one_uploaded() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let paint = |painter: &mut Painter| painter.styled(0, 0, "M", background(1));
        let state = painted(1, 2, paint);

        let mut first = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let built = scene::build(&state, &mut first);
        gpu.get().render(&built, first.atlases()).unwrap();

        let zoomed = FontStack::embedded(DEFAULT_FONT_SIZE * 1.1).unwrap();
        let mut second = GlyphCache::new(zoomed);
        let built = scene::build(&state, &mut second);
        let after_rebuild = gpu.get().render(&built, second.atlases()).unwrap();

        gpu.get().forget_uploads();
        let mut third = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE * 1.1).unwrap());
        let built = scene::build(&state, &mut third);
        let truthful = gpu.get().render(&built, third.atlases()).unwrap();

        assert_eq!(
            after_rebuild.pixels, truthful.pixels,
            "a cache rebuilt to a new size drew from the texture the previous cache uploaded"
        );
    }

    #[test]
    fn two_atlases_never_present_the_same_revision() {
        let mut first = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let mut second = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE * 1.1).unwrap());
        let state = painted(1, 2, |painter: &mut Painter| {
            painter.styled(0, 0, "M", background(1))
        });
        scene::build(&state, &mut first);
        scene::build(&state, &mut second);
        assert_ne!(
            first.atlases().mask.revision(),
            second.atlases().mask.revision(),
            "identical work on two atlases produced one revision, so an upload can be skipped"
        );
    }

    #[test]
    fn the_gpu_probe_this_binary_runs_on_itself() {
        if std::env::var_os(PROBE_ENV).is_none() {
            return;
        }
        Headless::new().expect("this host cannot bring up an offscreen GPU context");
    }

    #[test]
    fn an_empty_grid_renders_as_the_background_color() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let (image, _) = draw(&mut gpu, &painted(2, 4, |_| {}));
        assert_eq!(image.width, 32);
        assert_eq!(image.height, 40);
        let [r, g, b] = color::DEFAULT_BACKGROUND;
        assert!(
            image.pixels.chunks(4).all(|px| px == [r, g, b, 255]),
            "the frame is not uniformly background"
        );
    }

    #[test]
    fn a_background_rectangle_lands_at_the_top_left_cell() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let (image, metrics) = draw(
            &mut gpu,
            &painted(2, 4, |painter| painter.styled(0, 0, " ", background(1))),
        );
        let [r, g, b] = color::ANSI_16[1];
        assert_eq!(image.pixel(0, 0), [r, g, b, 255]);
        assert_eq!(
            image.pixel(metrics.width - 1, metrics.height - 1),
            [r, g, b, 255]
        );
        assert_eq!(
            image.pixel(metrics.width, 0),
            [
                color::DEFAULT_BACKGROUND[0],
                color::DEFAULT_BACKGROUND[1],
                color::DEFAULT_BACKGROUND[2],
                255
            ]
        );
    }

    #[test]
    fn a_glyph_marks_pixels_inside_its_own_cell_only() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let (image, metrics) = draw(&mut gpu, &painted(1, 4, |painter| painter.text(0, 0, "M")));
        let lit = |cell: u32| {
            (0..metrics.height).any(|y| {
                (0..metrics.width)
                    .any(|x| image.pixel(cell * metrics.width + x, y) != [0, 0, 0, 255])
            })
        };
        assert!(lit(0), "the glyph cell is blank");
        assert!(!lit(1), "the glyph bled into the next cell");
    }

    #[test]
    fn the_image_is_top_down_with_row_zero_at_the_top() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let (image, metrics) = draw(
            &mut gpu,
            &painted(2, 2, |painter| painter.styled(1, 0, " ", background(2))),
        );
        let [r, g, b] = color::ANSI_16[2];
        assert_eq!(image.pixel(0, metrics.height), [r, g, b, 255]);
        assert_eq!(
            image.pixel(0, 0),
            [
                color::DEFAULT_BACKGROUND[0],
                color::DEFAULT_BACKGROUND[1],
                color::DEFAULT_BACKGROUND[2],
                255
            ]
        );
    }

    #[test]
    fn a_color_glyph_reaches_the_frame_with_its_own_colors() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let (image, metrics) = draw(
            &mut gpu,
            &painted(1, 4, |painter| painter.wide(0, 0, '\u{1f600}')),
        );
        let colored = (0..metrics.height)
            .flat_map(|y| (0..metrics.width * 2).map(move |x| (x, y)))
            .map(|(x, y)| image.pixel(x, y))
            .filter(|[r, g, b, _]| r != g || g != b)
            .count();
        assert!(
            colored > 20,
            "the emoji rendered without color: {} colored pixels",
            colored
        );
    }

    #[test]
    fn a_mask_glyph_is_painted_in_its_cell_color_alone() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let (image, metrics) = draw(
            &mut gpu,
            &painted(1, 4, |painter| {
                painter.styled(0, 0, "M", |cell| cell.fg = WireColor::Named(1).pack());
            }),
        );
        let [red, green, blue] = color::ANSI_16[1];
        let lit = (0..metrics.height)
            .flat_map(|y| (0..metrics.width).map(move |x| (x, y)))
            .map(|(x, y)| image.pixel(x, y))
            .filter(|pixel| *pixel == [red, green, blue, 255])
            .count();
        assert!(lit > 0, "the glyph was not painted in its foreground color");
    }

    fn kitty_image(fill: [u8; 4], width: u32, height: u32) -> GraphicsRecord {
        painter::residency(1, width, height, fill)
    }

    #[test]
    fn a_kitty_placement_paints_its_pixels_into_the_frame() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let state = painted_with_graphics(
            2,
            4,
            &[
                kitty_image([10, 200, 30, 255], 8, 20),
                Place::of(1, 8, 20).at(1, 1).record(),
            ],
            |_| {},
        );
        let (image, metrics) = draw(&mut gpu, &state);
        assert_eq!(
            image.pixel(metrics.width, metrics.height),
            [10, 200, 30, 255]
        );
        assert_eq!(
            image.pixel(metrics.width * 2 - 1, metrics.height * 2 - 1),
            [10, 200, 30, 255]
        );
        assert_eq!(
            image.pixel(0, 0),
            [
                color::DEFAULT_BACKGROUND[0],
                color::DEFAULT_BACKGROUND[1],
                color::DEFAULT_BACKGROUND[2],
                255
            ]
        );
    }

    #[test]
    fn a_cropped_placement_paints_only_the_named_extent() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let state = painted_with_graphics(
            2,
            4,
            &[
                kitty_image([0, 0, 255, 255], 8, 20),
                Place::of(1, 8, 20).cropped(0, 0, 4, 6).record(),
            ],
            |_| {},
        );
        let (image, _) = draw(&mut gpu, &state);
        assert_eq!(image.pixel(3, 5), [0, 0, 255, 255]);
        assert_eq!(
            image.pixel(4, 5),
            [
                color::DEFAULT_BACKGROUND[0],
                color::DEFAULT_BACKGROUND[1],
                color::DEFAULT_BACKGROUND[2],
                255
            ]
        );
    }

    #[test]
    fn a_transparent_placement_lets_the_cell_behind_it_through() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let state = painted_with_graphics(
            1,
            4,
            &[
                kitty_image([0, 0, 0, 0], 8, 20),
                Place::of(1, 8, 20).record(),
            ],
            |painter| painter.styled(0, 0, "    ", background(1)),
        );
        let (image, _) = draw(&mut gpu, &state);
        let [r, g, b] = color::ANSI_16[1];
        assert_eq!(image.pixel(2, 2), [r, g, b, 255]);
    }

    #[test]
    fn a_deleted_image_leaves_the_frame_on_the_next_draw() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let headless = gpu.get();
        let mut state = painted_with_graphics(
            1,
            2,
            &[
                kitty_image([200, 10, 10, 255], 8, 20),
                Place::of(1, 8, 20).record(),
            ],
            |_| {},
        );
        let painted_image = headless
            .render(&scene::build(&state, &mut cache), cache.atlases())
            .unwrap();
        assert_eq!(painted_image.pixel(0, 0), [200, 10, 10, 255]);
        assert_eq!(headless.image_textures(), 1);

        apply_graphics(&mut state, &[GraphicsRecord::DeleteImage { id: 1 }]);
        let cleared = headless
            .render(&scene::build(&state, &mut cache), cache.atlases())
            .unwrap();
        let [r, g, b] = color::DEFAULT_BACKGROUND;
        assert_eq!(cleared.pixel(0, 0), [r, g, b, 255]);
        assert_eq!(
            headless.image_textures(),
            0,
            "the texture outlived the image it held"
        );
    }

    #[test]
    fn an_image_kept_resident_without_a_placement_keeps_its_texture() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let headless = gpu.get();
        let mut state = painted_with_graphics(
            1,
            2,
            &[
                kitty_image([1, 2, 3, 255], 8, 20),
                Place::of(1, 8, 20).record(),
            ],
            |_| {},
        );
        headless
            .render(&scene::build(&state, &mut cache), cache.atlases())
            .unwrap();

        apply_graphics(
            &mut state,
            &[GraphicsRecord::DeletePlacement {
                image_id: 1,
                placement_id: 1,
            }],
        );
        headless
            .render(&scene::build(&state, &mut cache), cache.atlases())
            .unwrap();
        assert_eq!(
            headless.image_textures(),
            1,
            "a resident image lost its texture when its placement went away"
        );
    }

    #[test]
    fn a_full_repaint_releases_every_texture() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let headless = gpu.get();
        let mut state = painted_with_graphics(
            1,
            2,
            &[
                kitty_image([1, 2, 3, 255], 8, 20),
                Place::of(1, 8, 20).record(),
            ],
            |_| {},
        );
        headless
            .render(&scene::build(&state, &mut cache), cache.atlases())
            .unwrap();
        assert_eq!(headless.image_textures(), 1);

        Painter::apply(&mut state, |painter| painter.full_repaint());
        headless
            .render(&scene::build(&state, &mut cache), cache.atlases())
            .unwrap();
        assert_eq!(headless.image_textures(), 0);
    }

    #[test]
    fn a_retransmission_under_one_id_replaces_the_pixels_on_screen() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let headless = gpu.get();
        let mut state = painted_with_graphics(
            1,
            2,
            &[
                kitty_image([1, 2, 3, 255], 8, 20),
                Place::of(1, 8, 20).record(),
            ],
            |_| {},
        );
        let first = headless
            .render(&scene::build(&state, &mut cache), cache.atlases())
            .unwrap();
        assert_eq!(first.pixel(0, 0), [1, 2, 3, 255]);

        apply_graphics(
            &mut state,
            &[
                kitty_image([4, 5, 6, 255], 8, 20),
                Place::of(1, 8, 20).record(),
            ],
        );
        let second = headless
            .render(&scene::build(&state, &mut cache), cache.atlases())
            .unwrap();
        assert_eq!(second.pixel(0, 0), [4, 5, 6, 255]);
    }

    fn sixel_payload(width: usize, bands: usize) -> String {
        let mut stream = format!("\u{1b}P0;1;0q\"1;1;{};{}", width, bands * 6);
        for _ in 0..bands {
            stream.push_str(&format!("#0;2;0;100;0!{}~-", width));
        }
        stream.push_str("\u{1b}\\");
        stream
    }

    fn sixel_state(
        rows: usize,
        cols: usize,
        records: &[GraphicsRecord],
        paint: impl FnOnce(&mut Painter),
    ) -> TerminalState {
        let mut state = TerminalState::new(rows, cols);
        state.set_cell_size(8, 20);
        Painter::apply(&mut state, |painter| {
            paint(painter);
            painter.graphics(records);
        });
        state
    }

    #[test]
    fn a_sixel_chunk_is_painted_at_the_cell_it_was_placed_on() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let headless = gpu.get();
        let state = sixel_state(
            2,
            2,
            &[painter::sixel_chunk(1, 0, 8, 6, &sixel_payload(8, 1))],
            |_| {},
        );
        let image = headless
            .render(&scene::build(&state, &mut cache), cache.atlases())
            .unwrap();

        assert_eq!(image.pixel(0, 20), [0, 255, 0, 255]);
        assert_eq!(image.pixel(7, 25), [0, 255, 0, 255]);
        let [r, g, b] = color::DEFAULT_BACKGROUND;
        assert_eq!(image.pixel(0, 19), [r, g, b, 255]);
        assert_eq!(image.pixel(8, 20), [r, g, b, 255]);
        assert_eq!(headless.image_textures(), 1);
    }

    #[test]
    fn a_sixel_chunk_is_drawn_over_the_cells_beneath_it() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let headless = gpu.get();
        let state = sixel_state(
            1,
            2,
            &[painter::sixel_chunk(0, 0, 8, 6, &sixel_payload(8, 1))],
            |painter| painter.styled(0, 0, "MM", background(1)),
        );
        let image = headless
            .render(&scene::build(&state, &mut cache), cache.atlases())
            .unwrap();
        assert_eq!(image.pixel(0, 0), [0, 255, 0, 255]);
    }

    #[test]
    fn a_kitty_image_composites_above_a_sixel_chunk_at_the_same_depth() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let headless = gpu.get();
        let state = sixel_state(
            1,
            2,
            &[
                painter::sixel_chunk(0, 0, 8, 6, &sixel_payload(8, 1)),
                kitty_image([0, 0, 200, 255], 8, 6),
                Place::of(1, 8, 6).record(),
            ],
            |_| {},
        );
        let image = headless
            .render(&scene::build(&state, &mut cache), cache.atlases())
            .unwrap();

        assert_eq!(image.pixel(0, 0), [0, 0, 200, 255], "the sixel drew on top");
        assert_eq!(headless.image_textures(), 2);
    }

    #[test]
    fn an_erased_sixel_chunk_releases_its_texture() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let headless = gpu.get();
        let mut state = sixel_state(
            2,
            2,
            &[painter::sixel_chunk(0, 0, 8, 6, &sixel_payload(8, 1))],
            |_| {},
        );
        headless
            .render(&scene::build(&state, &mut cache), cache.atlases())
            .unwrap();
        assert_eq!(headless.image_textures(), 1);

        Painter::apply(&mut state, |painter| painter.text(0, 0, "ab"));
        let cleared = headless
            .render(&scene::build(&state, &mut cache), cache.atlases())
            .unwrap();
        assert_eq!(headless.image_textures(), 0);
        assert_ne!(cleared.pixel(0, 0), [0, 255, 0, 255]);
    }

    #[test]
    fn a_negative_z_index_puts_the_image_under_the_glyph() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let at_depth = |z: i32| {
            painted_with_graphics(
                1,
                2,
                &[
                    kitty_image([0, 0, 200, 255], 8, 20),
                    Place::of(1, 8, 20).depth(z).record(),
                ],
                |painter| painter.text(0, 0, "M"),
            )
        };
        let below = draw(&mut gpu, &at_depth(-1)).0;
        let above = draw(&mut gpu, &at_depth(1)).0;

        let not_image = |image: &Image| {
            (0..20)
                .flat_map(|y| (0..8).map(move |x| (x, y)))
                .filter(|(x, y)| image.pixel(*x, *y) != [0, 0, 200, 255])
                .count()
        };
        assert!(
            not_image(&below) > 0,
            "the glyph was covered by a z=-1 image"
        );
        assert_eq!(not_image(&above), 0, "a z=1 image did not cover the glyph");
    }

    #[test]
    fn rendering_the_same_scene_twice_yields_identical_pixels() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let paint = |painter: &mut Painter| {
            painter.styled(0, 0, "hello", |cell| {
                cell.fg = WireColor::Named(3).pack();
                cell.attrs |= zellij_utils::structured_render::ATTR_BOLD;
            });
        };
        let (first, _) = draw(&mut gpu, &painted(3, 8, paint));
        let (second, _) = draw(&mut gpu, &painted(3, 8, paint));
        assert_eq!(first, second);
    }

    #[test]
    fn composing_text_is_drawn_over_cells_the_pane_still_owns() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let metrics = cache.metrics();
        let state = Painter::state(2, 8, |painter| {
            painter.text(0, 0, "ab");
            painter.text(1, 0, "second");
            painter.cursor(0, 2, crate::screen_buffer::CursorShape::Block);
        });
        let preedit = crate::composition::Preedit::new("ni", Some((2, 2)));
        let paints = color::Paints::default();
        let plain = scene::build_at(
            &state,
            &mut cache,
            scene::BlinkPhase::On,
            &paints,
            scene::CursorOptions::default(),
            None,
            None,
        );
        let composing = scene::build_at(
            &state,
            &mut cache,
            scene::BlinkPhase::On,
            &paints,
            scene::CursorOptions::default(),
            None,
            Some(&preedit),
        );

        let headless = gpu.get();
        let before = headless.render(&plain, cache.atlases()).unwrap();
        let after = headless.render(&composing, cache.atlases()).unwrap();

        let ink = |image: &Image, from: u32, to: u32, top: u32, bottom: u32| -> usize {
            let [r, g, b] = color::DEFAULT_BACKGROUND;
            let mut lit = 0;
            for y in top..bottom {
                for x in from..to {
                    if image.pixel(x, y) != [r, g, b, 255] {
                        lit += 1;
                    }
                }
            }
            lit
        };

        let cell = |image: &Image, column: u32| {
            ink(
                image,
                column * metrics.width,
                (column + 1) * metrics.width,
                0,
                metrics.height,
            )
        };
        assert_eq!(
            cell(&before, 3),
            0,
            "the second cell the composition will occupy starts empty"
        );
        assert!(
            cell(&after, 2) > 0 && cell(&after, 3) > 0,
            "the composing text was not drawn at the cursor"
        );
        assert_eq!(
            cell(&after, 4),
            (metrics.width * metrics.height) as usize,
            "the cursor did not move past the composing text"
        );
        assert_eq!(
            cell(&before, 4),
            0,
            "the cell the cursor moves onto was empty before the composition"
        );
        let second_row = |image: &Image| ink(image, 0, image.width, metrics.height, image.height);
        assert_eq!(
            second_row(&before),
            second_row(&after),
            "the pane's own cells changed beneath the composition"
        );
        assert_eq!(
            ink(&before, 0, 2 * metrics.width, 0, metrics.height),
            ink(&after, 0, 2 * metrics.width, 0, metrics.height),
            "the text left of the cursor changed"
        );
    }

    #[test]
    fn one_context_serves_scenes_of_different_sizes() {
        let Some(mut gpu) = Headless::exclusive() else {
            return;
        };
        let mut cache = GlyphCache::new(FontStack::embedded(DEFAULT_FONT_SIZE).unwrap());
        let headless = gpu.get();

        let small = painted(2, 2, |painter| painter.text(0, 0, "ab"));
        let small_scene = scene::build(&small, &mut cache);
        let small_image = headless.render(&small_scene, cache.atlases()).unwrap();

        let large = painted(4, 10, |painter| painter.text(0, 0, "cd"));
        let large_scene = scene::build(&large, &mut cache);
        let large_image = headless.render(&large_scene, cache.atlases()).unwrap();

        assert_eq!((small_image.width, small_image.height), (16, 40));
        assert_eq!((large_image.width, large_image.height), (80, 80));
    }
}
