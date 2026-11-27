mod device_service;
mod keymap;
mod rfb;
mod selftest;
mod ui;
mod zrle;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--selftest") {
        std::process::exit(selftest::run(&args));
    }

    let mut host = "172.16.42.1".to_string();
    let mut autoconnect = true;
    let mut it = args.iter().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--host" => {
                if let Some(v) = it.next() {
                    host = v.clone();
                }
            }
            "--no-autoconnect" => {
                autoconnect = false;
            }
            _ => {}
        }
    }

    std::process::exit(ui::run(host, autoconnect));
}
