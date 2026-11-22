//! Render a caption for a quick visual check: `cargo run --example caption -- in.jpg out.jpg`
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let picture = chirp_core::Picture {
        id: "x".into(),
        source: "bing".into(),
        market: "de-DE".into(),
        date: "2026-11-22".into(),
        title: "Kapelle im Fels".into(),
        caption: "Die Kapelle und Einsiedelei von Santa Justa in Kantabrien, Spanien".into(),
        copyright: "© Tramont_ana/Getty Images".into(),
        description: None,
        info_url: None,
        image_url: String::new(),
        fallback_url: None,
        thumb_url: None,
        note: None,
    };
    chirp_core::caption::render(
        args[1].as_ref(),
        args[2].as_ref(),
        &picture,
        chirp_core::Corner::BottomRight,
    )
    .unwrap();
}
