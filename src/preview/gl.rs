use std::{
    ffi::{CStr, c_char, c_void},
    path::{Path, PathBuf},
    ptr,
    sync::{Arc, Mutex},
};

use eframe::{egui, egui_glow, glow};
use glow::HasContext;

use crate::{
    media::ffmpeg::{self, PreviewSession, ProbeSummary},
    util::anchor_gop_for_frame,
};

#[repr(C)]
struct GcGlPreview {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn gopcomposer_gl_preview_create(
        gl_texture: u32,
        preview_out: *mut *mut GcGlPreview,
        error_out: *mut *mut c_char,
    ) -> i32;
    fn gopcomposer_gl_preview_free(preview: *mut GcGlPreview);
    fn gopcomposer_preview_session_render_gl(
        session: *mut c_void,
        preview: *mut GcGlPreview,
        anchor_frame: i64,
        anchor_pts: i64,
        target_frame: i64,
        error_out: *mut *mut c_char,
    ) -> i32;
    fn gopcomposer_free_string(text: *mut c_char);
}

#[derive(Clone)]
pub(crate) struct GlPreviewState {
    inner: Arc<Mutex<GlPreviewInner>>,
}

impl Default for GlPreviewState {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(GlPreviewInner::default())),
        }
    }
}

#[derive(Clone)]
pub(crate) struct GlPreviewRequest {
    pub(crate) width: i32,
    pub(crate) height: i32,
    path: PathBuf,
    anchor_frame: i64,
    anchor_pts: i64,
    target_frame: i64,
}

#[derive(Default)]
struct GlPreviewInner {
    resources: Option<GlowResources>,
    session: Option<PreviewSession>,
    preloaded: Option<GlPreloadedFrame>,
    preload_failed: Option<(PathBuf, i64)>,
    path: Option<PathBuf>,
    last_frame: Option<i64>,
    failed: bool,
    status: String,
}

struct GlPreloadedFrame {
    session: PreviewSession,
    path: PathBuf,
    frame: i64,
}

struct GlInteropPreview {
    raw: *mut GcGlPreview,
}

unsafe impl Send for GlInteropPreview {}

impl Drop for GlInteropPreview {
    fn drop(&mut self) {
        unsafe {
            gopcomposer_gl_preview_free(self.raw);
        }
    }
}

struct GlowResources {
    slots: Vec<GlowInteropTexture>,
    active_slot: Option<usize>,
    program: glow::NativeProgram,
    vao: glow::NativeVertexArray,
    vbo: glow::NativeBuffer,
}

struct GlowInteropTexture {
    texture: glow::NativeTexture,
    interop: GlInteropPreview,
}

unsafe impl Send for GlowResources {}

impl GlPreviewState {
    pub(crate) fn reset(&mut self) {
        let (resources, session, preloaded) = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            let resources = inner.resources.take();
            let session = inner.session.take();
            let preloaded = inner.preloaded.take();
            inner.preload_failed = None;
            inner.path = None;
            inner.last_frame = None;
            inner.failed = false;
            inner.status.clear();
            (resources, session, preloaded)
        };
        drop((resources, session, preloaded));
    }

    pub(crate) fn suspend(&mut self) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        inner.preload_failed = None;
        inner.path = None;
        inner.last_frame = None;
        inner.status.clear();
    }

    pub(crate) fn clear_media(&mut self) {
        let (session, preloaded) = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            let session = inner.session.take();
            let preloaded = inner.preloaded.take();
            inner.preload_failed = None;
            inner.path = None;
            inner.last_frame = None;
            inner.failed = false;
            inner.status.clear();
            (session, preloaded)
        };
        drop((session, preloaded));
    }

    pub(crate) fn should_prefer_native(&self) -> bool {
        if !cfg!(target_os = "windows") {
            return false;
        }
        self.inner.lock().is_ok_and(|inner| !inner.failed)
    }

    pub(crate) fn last_status(&self) -> String {
        self.inner
            .lock()
            .map(|inner| inner.status.clone())
            .unwrap_or_default()
    }

    pub(crate) fn is_rendered(&self, path: &Path, target_frame: i64) -> bool {
        self.inner.lock().is_ok_and(|inner| {
            inner.path.as_deref() == Some(path) && inner.last_frame == Some(target_frame)
        })
    }

    pub(crate) fn rendered_frame(&self, path: &Path) -> Option<i64> {
        self.inner.lock().ok().and_then(|inner| {
            (inner.path.as_deref() == Some(path))
                .then_some(inner.last_frame)
                .flatten()
        })
    }

    pub(crate) fn request_for(
        path: &Path,
        probe: &ProbeSummary,
        target_frame: i64,
    ) -> Option<GlPreviewRequest> {
        let anchor = anchor_gop_for_frame(probe, target_frame)?;
        Some(GlPreviewRequest {
            path: path.to_path_buf(),
            width: probe.width,
            height: probe.height,
            anchor_frame: anchor.start_frame,
            anchor_pts: anchor.pts,
            target_frame,
        })
    }

    pub(crate) fn paint(&mut self, ui: &mut egui::Ui, rect: egui::Rect, request: GlPreviewRequest) {
        let inner = Arc::clone(&self.inner);
        ui.painter().add(egui::PaintCallback {
            rect,
            callback: Arc::new(egui_glow::CallbackFn::new(move |_info, painter| {
                let Ok(mut inner) = inner.lock() else {
                    return;
                };
                inner.paint(painter.gl(), &request);
            })),
        });
    }

    pub(crate) fn preload(
        &mut self,
        path: &Path,
        probe: &ProbeSummary,
        target_frame: i64,
    ) -> Result<(), String> {
        let Some(request) = Self::request_for(path, probe, target_frame) else {
            return Ok(());
        };
        let Ok(mut inner) = self.inner.lock() else {
            return Ok(());
        };
        inner.preload(request)
    }
}

impl GlPreviewInner {
    fn preload(&mut self, request: GlPreviewRequest) -> Result<(), String> {
        if self.failed || self.is_preloaded(&request.path, request.target_frame) {
            return Ok(());
        }
        if self
            .preload_failed
            .as_ref()
            .is_some_and(|(path, frame)| path == &request.path && *frame == request.target_frame)
        {
            return Ok(());
        }
        if self.path.as_deref() == Some(request.path.as_path())
            && self.last_frame == Some(request.target_frame)
        {
            return Ok(());
        }

        let mut session = match ffmpeg::open_preview_session(&request.path) {
            Ok(session) => session,
            Err(error) => {
                self.preload_failed = Some((request.path, request.target_frame));
                return Err(error);
            }
        };
        if let Err(error) = session.prepare_dx(
            request.anchor_frame,
            request.anchor_pts,
            request.target_frame,
        ) {
            self.preload_failed = Some((request.path, request.target_frame));
            return Err(error);
        }
        self.preload_failed = None;
        self.preloaded = Some(GlPreloadedFrame {
            session,
            path: request.path,
            frame: request.target_frame,
        });
        Ok(())
    }

    fn paint(&mut self, gl: &glow::Context, request: &GlPreviewRequest) {
        if self.failed {
            return;
        }
        if self.resources.is_none() {
            match unsafe { GlowResources::new(gl) } {
                Ok(resources) => {
                    self.resources = Some(resources);
                }
                Err(error) => {
                    self.failed = true;
                    self.status = format!("GL preview disabled: {error}");
                    return;
                }
            }
        }
        let had_frame = self
            .resources
            .as_ref()
            .is_some_and(GlowResources::has_active_frame);
        let result = self.render_request(request);
        match result {
            Ok(()) => {
                self.status = format!("GL/D3D preview frame {}", request.target_frame);
                if let Some(resources) = self.resources.as_ref() {
                    unsafe {
                        resources.draw(gl);
                    }
                }
            }
            Err(error) => {
                if had_frame {
                    self.status = format!("GL preview holding previous frame: {error}");
                    if let Some(resources) = self.resources.as_ref() {
                        unsafe {
                            resources.draw(gl);
                        }
                    }
                } else {
                    self.failed = true;
                    self.status = format!("GL preview disabled: {error}");
                }
            }
        }
    }

    fn render_request(&mut self, request: &GlPreviewRequest) -> Result<(), String> {
        if self.path.as_deref() == Some(request.path.as_path())
            && self.last_frame == Some(request.target_frame)
        {
            return Ok(());
        }
        self.promote_preloaded(request);
        if self.path.as_deref() != Some(request.path.as_path()) {
            self.session = Some(ffmpeg::open_preview_session(&request.path)?);
            self.path = Some(request.path.clone());
            self.last_frame = None;
        }

        let Some(session) = self.session.as_mut() else {
            return Err("GL preview session is not open".to_owned());
        };
        let Some(resources) = self.resources.as_mut() else {
            return Err("GL preview resources are not ready".to_owned());
        };
        resources.render_next(
            session,
            request.anchor_frame,
            request.anchor_pts,
            request.target_frame,
        )?;
        self.path = Some(request.path.clone());
        self.last_frame = Some(request.target_frame);
        Ok(())
    }

    fn promote_preloaded(&mut self, request: &GlPreviewRequest) {
        let Some(preloaded) = self.preloaded.take() else {
            return;
        };
        if preloaded.path == request.path && preloaded.frame == request.target_frame {
            self.session = Some(preloaded.session);
            self.path = Some(preloaded.path);
            self.last_frame = None;
        } else {
            self.preloaded = Some(preloaded);
        }
    }

    fn is_preloaded(&self, path: &Path, frame: i64) -> bool {
        self.preloaded
            .as_ref()
            .is_some_and(|preloaded| preloaded.path == path && preloaded.frame == frame)
    }
}

impl GlInteropPreview {
    unsafe fn new(gl_texture: glow::NativeTexture) -> Result<Self, String> {
        let mut raw = ptr::null_mut();
        let mut error = ptr::null_mut();
        let code =
            unsafe { gopcomposer_gl_preview_create(gl_texture.0.get(), &mut raw, &mut error) };
        let result = if code < 0 || raw.is_null() {
            Err(ffi_error("GL preview creation failed", code, error))
        } else {
            Ok(Self { raw })
        };
        unsafe {
            gopcomposer_free_string(error);
        }
        result
    }

    fn render(
        &mut self,
        session: &mut PreviewSession,
        anchor_frame: i64,
        anchor_pts: i64,
        target_frame: i64,
    ) -> Result<(), String> {
        let mut error = ptr::null_mut();
        let code = unsafe {
            gopcomposer_preview_session_render_gl(
                session.raw_handle(),
                self.raw,
                anchor_frame,
                anchor_pts,
                target_frame,
                &mut error,
            )
        };
        let result = if code < 0 {
            Err(ffi_error("GL preview render failed", code, error))
        } else {
            Ok(())
        };
        unsafe {
            gopcomposer_free_string(error);
        }
        result
    }
}

impl GlowResources {
    unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let mut slots = Vec::with_capacity(GL_INTEROP_SLOT_COUNT);
        for _ in 0..GL_INTEROP_SLOT_COUNT {
            slots.push(unsafe { GlowInteropTexture::new(gl)? });
        }
        let program = unsafe { create_program(gl)? };
        let vao = unsafe { gl.create_vertex_array() }.map_err(|error| error.to_string())?;
        let vbo = unsafe { gl.create_buffer() }.map_err(|error| error.to_string())?;
        unsafe {
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            let vertices = quad_vertices();
            let vertex_bytes = std::slice::from_raw_parts(
                vertices.as_ptr().cast::<u8>(),
                std::mem::size_of_val(&vertices),
            );
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::STATIC_DRAW);
            let stride = (4 * std::mem::size_of::<f32>()) as i32;
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(
                1,
                2,
                glow::FLOAT,
                false,
                stride,
                (2 * std::mem::size_of::<f32>()) as i32,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_vertex_array(None);
        }
        Ok(Self {
            slots,
            active_slot: None,
            program,
            vao,
            vbo,
        })
    }

    fn render_next(
        &mut self,
        session: &mut PreviewSession,
        anchor_frame: i64,
        anchor_pts: i64,
        target_frame: i64,
    ) -> Result<(), String> {
        let slot = self
            .active_slot
            .map(|index| (index + 1) % self.slots.len())
            .unwrap_or(0);
        self.slots[slot]
            .interop
            .render(session, anchor_frame, anchor_pts, target_frame)?;
        self.active_slot = Some(slot);
        Ok(())
    }

    fn has_active_frame(&self) -> bool {
        self.active_slot.is_some()
    }

    unsafe fn draw(&self, gl: &glow::Context) {
        let Some(active_slot) = self.active_slot else {
            return;
        };
        unsafe {
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::BLEND);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.slots[active_slot].texture));
            gl.use_program(Some(self.program));
            if let Some(location) = gl.get_uniform_location(self.program, "u_texture") {
                gl.uniform_1_i32(Some(&location), 0);
            }
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }
    }
}

impl GlowInteropTexture {
    unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        let texture = unsafe { gl.create_texture() }.map_err(|error| error.to_string())?;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
        let interop = unsafe { GlInteropPreview::new(texture)? };
        Ok(Self { texture, interop })
    }
}

fn quad_vertices() -> [f32; 16] {
    [
        -1.0, -1.0, 0.0, 1.0, //
        1.0, -1.0, 1.0, 1.0, //
        -1.0, 1.0, 0.0, 0.0, //
        1.0, 1.0, 1.0, 0.0,
    ]
}

unsafe fn create_program(gl: &glow::Context) -> Result<glow::NativeProgram, String> {
    let program = unsafe { gl.create_program() }.map_err(|error| error.to_string())?;
    let vertex = unsafe { compile_shader(gl, glow::VERTEX_SHADER, VERTEX_SHADER) }?;
    let fragment = unsafe { compile_shader(gl, glow::FRAGMENT_SHADER, FRAGMENT_SHADER) }?;
    unsafe {
        gl.attach_shader(program, vertex);
        gl.attach_shader(program, fragment);
        gl.link_program(program);
        gl.detach_shader(program, vertex);
        gl.detach_shader(program, fragment);
        gl.delete_shader(vertex);
        gl.delete_shader(fragment);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            return Err(format!("GL preview shader link failed: {log}"));
        }
    }
    Ok(program)
}

unsafe fn compile_shader(
    gl: &glow::Context,
    shader_type: u32,
    source: &str,
) -> Result<glow::NativeShader, String> {
    let shader = unsafe { gl.create_shader(shader_type) }.map_err(|error| error.to_string())?;
    unsafe {
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(format!("GL preview shader compile failed: {log}"));
        }
    }
    Ok(shader)
}

fn ffi_error(prefix: &str, code: i32, error: *mut c_char) -> String {
    if error.is_null() {
        format!("{prefix} with code {code}")
    } else {
        let message = unsafe { CStr::from_ptr(error).to_string_lossy().into_owned() };
        format!("{prefix}: {message}")
    }
}

pub(crate) fn fit_rect(rect: egui::Rect, width: i32, height: i32) -> egui::Rect {
    if width <= 0 || height <= 0 || rect.width() <= 1.0 || rect.height() <= 1.0 {
        return rect;
    }
    let source = egui::vec2(width as f32, height as f32);
    let scale = (rect.width() / source.x)
        .min(rect.height() / source.y)
        .max(0.01);
    egui::Rect::from_center_size(rect.center(), source * scale)
}

const VERTEX_SHADER: &str = r#"#version 330 core
layout (location = 0) in vec2 a_pos;
layout (location = 1) in vec2 a_uv;
out vec2 v_uv;
void main() {
    v_uv = a_uv;
    gl_Position = vec4(a_pos, 0.0, 1.0);
}
"#;

const GL_INTEROP_SLOT_COUNT: usize = 1;

const FRAGMENT_SHADER: &str = r#"#version 330 core
in vec2 v_uv;
uniform sampler2D u_texture;
out vec4 out_color;
void main() {
    vec4 color = texture(u_texture, v_uv);
    out_color = vec4(color.rgb, 1.0);
}
"#;
