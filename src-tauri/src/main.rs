// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args_os().skip(1);
    if args.next().is_some_and(|a| a == "--self-test") {
        std::process::exit(radial_lib::self_test(args.next().map(Into::into)));
    }
    radial_lib::run();
}
