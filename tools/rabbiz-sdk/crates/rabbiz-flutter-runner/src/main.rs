use std::path::PathBuf;

use rabbiz_flutter_runner::{FlutterBundle, FlutterHostApp};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut check_only = false;
    let mut dir: Option<PathBuf> = None;

    for arg in args {
        if arg == "--check" {
            check_only = true;
        } else if arg == "--help" || arg == "-h" {
            println!("Usage: rabbiz-flutter-runner [--check] <bundle_dir>");
            return;
        } else {
            dir = Some(PathBuf::from(arg));
        }
    }

    let bundle_dir = dir.unwrap_or_else(|| PathBuf::from("."));
    match FlutterBundle::load(&bundle_dir) {
        Ok(bundle) => {
            if check_only {
                println!(
                    "OK: Flutter bundle '{}' ({:?}) at {}",
                    bundle.app_id,
                    bundle.mode,
                    bundle.assets_dir.display()
                );
                return;
            }
            for (k, v) in bundle.env_contract() {
                std::env::set_var(k, v);
            }
            let app = FlutterHostApp::new(bundle);
            rabbiz_app_kit::run_app(app);
        }
        Err(err) => {
            eprintln!("rabbiz-flutter-runner error: {err}");
            std::process::exit(1);
        }
    }
}
