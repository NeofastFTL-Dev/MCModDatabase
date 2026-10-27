#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    std::panic::set_hook(Box::new(|info| {
        mc_mod_database::util::log_error(&format!("panic: {info}"));
    }));

    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|s| s.as_str()) == Some("agent") {
        #[cfg(windows)]
        attach_console_for_cli();
        if let Err(e) = mc_mod_database::agent::run(&args[2..]) {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return;
    }

    if let Err(e) = mc_mod_database::gui::run_gui() {
        mc_mod_database::util::log_error(&format!("GUI error: {e}"));
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn attach_console_for_cli() {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileType, FILE_TYPE_DISK, FILE_TYPE_PIPE,
    };
    use windows_sys::Win32::System::Console::{
        AllocConsole, AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE,
    };

    unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        let redirected = !h.is_null()
            && h != INVALID_HANDLE_VALUE
            && {
                let ft = GetFileType(h);
                ft == FILE_TYPE_DISK || ft == FILE_TYPE_PIPE
            };
        if redirected {
            // Keep PowerShell/cmd redirection; do not freopen onto CONOUT$.
            return;
        }

        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            let _ = AllocConsole();
        }

        extern "C" {
            fn freopen(
                filename: *const i8,
                mode: *const i8,
                stream: *mut std::ffi::c_void,
            ) -> *mut std::ffi::c_void;
            fn __acrt_iob_func(idx: u32) -> *mut std::ffi::c_void;
        }
        let conin = b"CONIN$\0";
        let conout = b"CONOUT$\0";
        let r = b"r\0";
        let w = b"w\0";
        freopen(
            conin.as_ptr() as *const i8,
            r.as_ptr() as *const i8,
            __acrt_iob_func(0),
        );
        freopen(
            conout.as_ptr() as *const i8,
            w.as_ptr() as *const i8,
            __acrt_iob_func(1),
        );
        freopen(
            conout.as_ptr() as *const i8,
            w.as_ptr() as *const i8,
            __acrt_iob_func(2),
        );
    }
}
