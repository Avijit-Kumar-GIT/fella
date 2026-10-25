// Keep the sidecar process from opening a separate console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args_os().skip(1);
    if let Some("--engine-stdio") = args
        .next()
        .and_then(|arg| arg.into_string().ok())
        .as_deref()
    {
        let Some(flag) = args.next() else {
            eprintln!("fella: --engine-stdio requires --data-dir <path>");
            std::process::exit(2);
        };
        if flag != "--data-dir" {
            eprintln!("fella: expected --data-dir after --engine-stdio");
            std::process::exit(2);
        }
        let Some(data_dir) = args.next() else {
            eprintln!("fella: --data-dir requires a path");
            std::process::exit(2);
        };
        if let Err(error) = fella_lib::run_engine_stdio(std::path::Path::new(&data_dir)) {
            eprintln!("fella engine: {error}");
            std::process::exit(1);
        }
        return;
    }
    eprintln!("fella: launch with --engine-stdio --data-dir <path>");
    std::process::exit(2);
}
