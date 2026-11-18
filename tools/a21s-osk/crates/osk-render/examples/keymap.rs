//! Print key centres for one layer: `cargo run -p osk-render --example keymap -- en-us base`
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let layout_id = args.get(1).map(String::as_str).unwrap_or("en-us");
    let layer_name = args.get(2).map(String::as_str).unwrap_or("base");
    let src = match layout_id {
        "th" | "th-kedmanee" => include_str!("../../../data/layouts/th-kedmanee.toml"),
        _ => include_str!("../../../data/layouts/en-us.toml"),
    };
    let layout = osk_core::layout::Layout::parse(src).expect("layout");
    let rows = layout.row_rects(layer_name).expect("layer");
    for row in rows {
        for (rect, pos) in row {
            let key = layout.key(layer_name, pos).unwrap();
            println!(
                "{}\t{}\t{}\t{:.0}\t{:.0}",
                key.label,
                pos.row,
                pos.col,
                rect.x + rect.w / 2.0,
                rect.y + rect.h / 2.0
            );
        }
    }
}
