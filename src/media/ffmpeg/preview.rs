use std::{ffi::c_void, path::Path, ptr};

use super::{
    ffi,
    types::{DecodedFrame, RgbaFrame},
};

pub struct PreviewSession {
    raw: *mut ffi::GcPreviewSession,
}

unsafe impl Send for PreviewSession {}

impl Drop for PreviewSession {
    fn drop(&mut self) {
        unsafe {
            ffi::gopcomposer_preview_session_free(self.raw);
        }
    }
}

pub fn decode_rgba_frame(path: &Path, frame_index: i64) -> Result<RgbaFrame, String> {
    let (_path_text, input) = ffi::path_to_cstring(path)?;
    let mut raw = ffi::empty_rgba_frame();

    let code = unsafe { ffi::gopcomposer_decode_rgba_frame(input.as_ptr(), frame_index, &mut raw) };
    let result = if code < 0 {
        Err(ffi::c_string_lossy(raw.error)
            .unwrap_or_else(|| format!("FFmpeg decode failed with code {code}")))
    } else {
        copy_raw_frame(&raw)
    };

    unsafe {
        ffi::gopcomposer_free_rgba_frame(&mut raw);
    }
    result
}

pub fn decode_rgba_window(
    path: &Path,
    anchor_frame: i64,
    anchor_pts: i64,
    start_frame: i64,
    frame_count: i32,
) -> Result<Vec<DecodedFrame>, String> {
    let (_path_text, input) = ffi::path_to_cstring(path)?;
    let mut raw = ffi::empty_rgba_window();

    let code = unsafe {
        ffi::gopcomposer_decode_rgba_window(
            input.as_ptr(),
            anchor_frame,
            anchor_pts,
            start_frame,
            frame_count,
            &mut raw,
        )
    };
    let result = rgba_window_result(
        code,
        &raw,
        |code| format!("FFmpeg decode window failed with code {code}"),
        "decoder returned an empty window",
    );

    unsafe {
        ffi::gopcomposer_free_rgba_window(&mut raw);
    }
    result
}

pub fn open_preview_session(path: &Path) -> Result<PreviewSession, String> {
    let (_path_text, input) = ffi::path_to_cstring(path)?;
    let mut session = ptr::null_mut();
    let mut error = ffi::empty_rgba_window();

    let code =
        unsafe { ffi::gopcomposer_preview_session_open(input.as_ptr(), &mut session, &mut error) };
    let result = if code < 0 {
        Err(ffi::c_string_lossy(error.error)
            .unwrap_or_else(|| format!("FFmpeg preview session open failed with code {code}")))
    } else if session.is_null() {
        Err("FFmpeg preview session open returned null".to_owned())
    } else {
        Ok(PreviewSession { raw: session })
    };

    unsafe {
        ffi::gopcomposer_free_rgba_window(&mut error);
    }
    result
}

impl PreviewSession {
    pub(crate) fn raw_handle(&mut self) -> *mut c_void {
        self.raw.cast()
    }

    pub fn accel_name(&self) -> String {
        let ptr = unsafe { ffi::gopcomposer_preview_session_accel(self.raw) };
        ffi::c_string_lossy(ptr).unwrap_or_else(|| "software".to_owned())
    }

    pub fn decode_window(
        &mut self,
        anchor_frame: i64,
        anchor_pts: i64,
        start_frame: i64,
        frame_count: i32,
    ) -> Result<Vec<DecodedFrame>, String> {
        let mut raw = ffi::empty_rgba_window();

        let code = unsafe {
            ffi::gopcomposer_preview_session_decode_window(
                self.raw,
                anchor_frame,
                anchor_pts,
                start_frame,
                frame_count,
                &mut raw,
            )
        };
        let result = rgba_window_result(
            code,
            &raw,
            |code| format!("FFmpeg preview session decode failed with code {code}"),
            "decoder returned an empty window",
        );

        unsafe {
            ffi::gopcomposer_free_rgba_window(&mut raw);
        }
        result
    }

    pub fn prepare_dx(
        &mut self,
        anchor_frame: i64,
        anchor_pts: i64,
        target_frame: i64,
    ) -> Result<(), String> {
        let mut error = ptr::null_mut();
        let code = unsafe {
            ffi::gopcomposer_preview_session_prepare_dx(
                self.raw,
                anchor_frame,
                anchor_pts,
                target_frame,
                &mut error,
            )
        };
        let result = if code < 0 {
            Err(ffi::c_string_lossy(error)
                .unwrap_or_else(|| format!("DX preview prepare failed with code {code}")))
        } else {
            Ok(())
        };
        unsafe {
            ffi::gopcomposer_free_string(error);
        }
        result
    }
}

fn rgba_window_result(
    code: i32,
    raw: &ffi::GcRgbaWindow,
    fallback_error: impl FnOnce(i32) -> String,
    empty_error: &str,
) -> Result<Vec<DecodedFrame>, String> {
    if code < 0 {
        return Err(ffi::c_string_lossy(raw.error).unwrap_or_else(|| fallback_error(code)));
    }
    if raw.frames.is_null() || raw.frame_count == 0 {
        return Err(empty_error.to_owned());
    }

    let frames = unsafe { std::slice::from_raw_parts(raw.frames, raw.frame_count as usize) };
    let mut output = Vec::with_capacity(frames.len());
    for item in frames {
        output.push(DecodedFrame {
            frame_index: item.frame_index,
            frame: copy_raw_frame(&item.frame)?,
        });
    }
    Ok(output)
}

fn copy_raw_frame(raw: &ffi::GcRgbaFrame) -> Result<RgbaFrame, String> {
    if raw.pixels.is_null() || raw.width <= 0 || raw.height <= 0 || raw.stride < raw.width * 4 {
        return Err("decoder returned an empty frame".to_owned());
    }

    let width = raw.width as usize;
    let height = raw.height as usize;
    let stride = raw.stride as usize;
    let mut pixels = vec![0u8; width * height * 4];
    let source = unsafe { std::slice::from_raw_parts(raw.pixels, stride * height) };
    for row in 0..height {
        let src = &source[row * stride..row * stride + width * 4];
        let dst = &mut pixels[row * width * 4..(row + 1) * width * 4];
        dst.copy_from_slice(src);
    }
    Ok(RgbaFrame {
        width,
        height,
        pixels,
    })
}
