#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    std::panic::set_hook(Box::new(|info| {
        mc_mod_database::util::log_error(&format!("panic: {info}"));
    }));
    if let Err(e) = mc_mod_database::gui::run_gui() {
        mc_mod_database::util::log_error(&format!("GUI error: {e}"));
        std::process::exit(1);
    }
}
