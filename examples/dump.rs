//! A simple utility that outputs the result of some common transforms.

use std::io::Read;
use std::process;

use sgmlish::parser::ParserConfig;
use sgmlish::transforms::Transform;
use sgmlish::Data::CData;
use sgmlish::{SgmlEvent, SgmlFragment};

fn main() {
    if let Err(err) = run() {
        eprintln!("🛑 {}", err);
        process::exit(1);
    }
}

fn run() -> Result<(), sgmlish::Error> {
    let config = ParserConfig::builder()
        .expand_marked_sections()
        .expand_entities(|entity| match entity {
            "lt" => Some("<"),
            "gt" => Some(">"),
            "amp" => Some("&"),
            "quot" => Some("\""),
            "apos" => Some("'"),
            _ => None,
        })
        .build();

    let mut sgml = String::new();
    std::io::stdin().read_to_string(&mut sgml).unwrap();

    let fragment = sgmlish::parser::parse_with(&sgml, &config)?;

    println!("ℹ️  Roundtrip:");
    println!("{}", fragment);
    println!();

    println!("ℹ️  Events:");
    for event in &fragment {
        println!("{:?}", event);
    }
    println!();

    println!("ℹ️  Normalized to lowercase:");
    let fragment = fragment.lowercase_identifiers();
    println!("{}", fragment);
    println!();

    println!("ℹ️  Attempting to fill end tags:");
    let fragment = fragment.normalize_end_tags()?;
    println!("{}", fragment);
    println!();

    println!("ℹ️  Pretty-printed:");
    let fragment = reindent(fragment);
    println!("{}", fragment);
    println!();

    Ok(())
}

fn reindent(fragment: SgmlFragment) -> SgmlFragment {
    let mut transform = Transform::new();
    let mut indent_level = 0;

    fn indent(level: usize) -> SgmlEvent<'static> {
        let mut s = "\n".to_owned();
        for _ in 0..level {
            s.push_str("  ");
        }
        SgmlEvent::Character(CData(s.into()))
    }

    for (i, event) in fragment.iter().enumerate() {
        if i == 0 {
            continue;
        }
        match event {
            SgmlEvent::OpenStartTag(_)
            | SgmlEvent::Character(_)
            | SgmlEvent::ProcessingInstruction(_)
            | SgmlEvent::MarkupDeclaration(_) => {
                transform.insert_at(i, indent(indent_level));
            }
            SgmlEvent::CloseStartTag => indent_level += 1,
            SgmlEvent::EndTag(_) => {
                indent_level -= 1;
                match fragment.as_slice().get(i - 1) {
                    Some(SgmlEvent::CloseStartTag) => {}
                    _ => {
                        transform.insert_at(i, indent(indent_level));
                    }
                }
            }
            _ => {}
        }
    }

    transform.apply(fragment)
}
