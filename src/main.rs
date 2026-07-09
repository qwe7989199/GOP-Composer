#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod cli;
mod media;
mod preview;
mod ui;
mod util;

use app::GopComposerApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let _console = init_windows_console(args.iter().any(|arg| arg == "--debug"));
    if let Some(result) = cli::try_handle(&args) {
        return result;
    }

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("GOP Composer")
            .with_decorations(false)
            .with_min_inner_size([960.0, 620.0])
            .with_inner_size([1280.0, 820.0]),
        ..Default::default()
    };

    eframe::run_native(
        "GOP Composer",
        options,
        Box::new(|cc| {
            ui::fonts::install_system_fonts(&cc.egui_ctx);
            ui::style::install_hard_style(&cc.egui_ctx);
            Ok(Box::<GopComposerApp>::default())
        }),
    )
}

#[cfg(target_os = "windows")]
fn init_windows_console(debug: bool) -> bool {
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
    const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const OPEN_EXISTING: u32 = 3;
    const FILE_TYPE_UNKNOWN: u32 = 0;

    unsafe extern "system" {
        fn AttachConsole(dw_process_id: u32) -> i32;
        fn AllocConsole() -> i32;
        fn GetStdHandle(std_handle: u32) -> isize;
        fn GetFileType(file: isize) -> u32;
        fn CreateFileA(
            lp_file_name: *const std::ffi::c_char,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *const u8,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: isize,
        ) -> isize;
        fn SetStdHandle(std_handle: u32, handle: isize) -> i32;
        fn SetConsoleOutputCP(code_page_id: u32) -> i32;
    }

    unsafe {
        let stdout = GetStdHandle(STD_OUTPUT_HANDLE);
        let stderr = GetStdHandle(STD_ERROR_HANDLE);
        if stdout != 0
            && stdout != -1isize
            && stderr != 0
            && stderr != -1isize
            && GetFileType(stdout) != FILE_TYPE_UNKNOWN
            && GetFileType(stderr) != FILE_TYPE_UNKNOWN
        {
            return true;
        }

        let attached_parent = AttachConsole(ATTACH_PARENT_PROCESS) != 0;
        let console_ready = attached_parent || (debug && AllocConsole() != 0);
        if !console_ready {
            return false;
        }

        SetConsoleOutputCP(65001);
        let output = CreateFileA(
            c"CONOUT$".as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            0,
        );
        if output != -1isize {
            SetStdHandle(STD_OUTPUT_HANDLE, output);
            SetStdHandle(STD_ERROR_HANDLE, output);
        }
        true
    }
}

#[cfg(not(target_os = "windows"))]
fn init_windows_console(_debug: bool) -> bool {
    true
}
