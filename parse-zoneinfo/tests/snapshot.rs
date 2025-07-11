use std::error::Error;

use insta::assert_debug_snapshot;

use parse_zoneinfo::line::Line;
use parse_zoneinfo::FILES;

#[ignore]
#[test]
fn test_parse() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let mut files = vec![];
    for file in FILES {
        let path = format!("../chrono-tz/tz/{file}");
        let text = std::fs::read_to_string(&path)?;
        let mut lines = vec![];
        for ln in text.lines() {
            dbg!(ln);
            match Line::new(ln)? {
                Line::Space => continue,
                ln => lines.push(format!("{ln:?}")),
            }
        }

        files.push((file, lines));
    }

    assert_debug_snapshot!(files);
    Ok(())
}
