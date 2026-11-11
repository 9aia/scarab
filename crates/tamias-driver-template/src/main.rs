//! tamias-driver-template — argv surface.
//!
//! Tamias verbs always arrive as `<verb> --dir <payload> --name <pkg>
//! [--config <vars>] [key=value …]`; `capabilities`/`config-schema`
//! are the discovery probes. No pavo surface in v1 — a `manifest`/
//! `serve` pair lands when a view consumer exists.

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("capabilities") => {
            println!(
                "{}",
                serde_json::to_string_pretty(&tamias_driver_template::capabilities())
                    .unwrap_or_default()
            );
        }
        Some("config-schema") => {
            println!(
                "{}",
                serde_json::to_string_pretty(&tamias_driver_template::config_schema())
                    .unwrap_or_default()
            );
        }
        Some(verb @ ("install" | "deploy" | "uninstall")) => {
            std::process::exit(tamias_driver_template::run(verb, &args.collect::<Vec<_>>()));
        }
        _ => {
            eprintln!(
                "usage: tamias-driver-template <install|deploy|uninstall|config-schema|capabilities> --dir <d> --name <n> [--config <vars>] [--target <dir>] [--dry-run] [key=value …]"
            );
            std::process::exit(2);
        }
    }
}
