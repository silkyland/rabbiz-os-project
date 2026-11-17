//! `rabbiz-de` — RabbizOS compositor entry point (M0: run / probe / env).

use de_config::flags::{parse, Command};
use de_core::{event_loop, init, Run};

/// Pre-first-frame failure code — the session wrapper maps this straight to
/// the labwc fallback with no retry delay.
const EXIT_CRASH_FALLBACK: i32 = 75;

fn main() {
    // smithay/drm diagnostics go to stderr; stdout stays JSON-pure (§7.21).
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let cli = match parse(std::env::args().skip(1)) {
        Ok(c) => c,
        Err(msg) => {
            if msg.starts_with("rabbiz-de") {
                println!("{msg}");
            } else {
                eprintln!("{msg}");
            }
            std::process::exit(if msg.starts_with("rabbiz-de") { 0 } else { 2 });
        }
    };
    if cli.flags.force_fail {
        std::process::exit(EXIT_CRASH_FALLBACK);
    }

    let code = match cli.command {
        Command::Env => {
            print!("{}", de_system::env::shell_lines());
            0
        }
        Command::Probe => match de_backend::probe::run(cli.flags.require_gl) {
            Ok((json, ok)) => {
                println!("{json}");
                if ok {
                    0
                } else {
                    EXIT_CRASH_FALLBACK
                }
            }
            Err(e) => {
                eprintln!("probe: {e}");
                EXIT_CRASH_FALLBACK
            }
        },
        Command::Launch(id) => match de_apps::launch::launch(&id) {
            Ok(pid) => {
                println!("{pid}");
                0
            }
            Err(e) => {
                eprintln!("launch {id}: {e}");
                2
            }
        },
        Command::Run => {
            let mut display = match smithay::reexports::wayland_server::Display::<
                de_core::De,
            >::new() {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("display: {e}");
                    std::process::exit(EXIT_CRASH_FALLBACK);
                }
            };
            match init(&cli.flags, &mut display) {
                Ok(de) => {
                    match Run::new(
                        de,
                        cli.flags.stats_file.as_deref(),
                        cli.flags.stats_summary.clone(),
                        cli.flags.scenario.clone(),
                        cli.flags.duration.map(std::time::Duration::from_secs),
                    ) {
                        Ok(run) => match event_loop(run, display, cli.flags.stats) {
                            Ok(code) => code as i32,
                            Err(e) => {
                                eprintln!("run: {e}");
                                EXIT_CRASH_FALLBACK
                            }
                        },
                        Err(e) => {
                            eprintln!("stats: {e}");
                            EXIT_CRASH_FALLBACK
                        }
                    }
                }
                Err(e) => {
                    eprintln!("init: {e}");
                    EXIT_CRASH_FALLBACK
                }
            }
        }
    };
    std::process::exit(code);
}
