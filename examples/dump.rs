//! A simple utility that outputs the result of some common transforms.

use std::io::Read;
use std::{env, process};

use sgmlish::transforms::Transform;
use sgmlish::{SgmlEvent, SgmlFragment};

fn main() {
    if let Err(err) = run() {
        eprintln!("🛑 {}", err);
        process::exit(1);
    }
}

fn run() -> sgmlish::Result<()> {
    let parser = sgmlish::Parser::builder()
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

    let sgml = if let Some(path) = env::args_os().skip(1).next() {
        std::fs::read_to_string(path).unwrap()
    } else {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer).unwrap();
        buffer
    };

    let fragment = parser.parse(&sgml)?;

    println!("ℹ️  Roundtrip:");
    println!("{}", fragment);
    println!();

    println!("ℹ️  Events:");
    for event in &fragment {
        println!("{:?}", event);
    }
    println!();

    let normalized = match sgmlish::transforms::normalize_end_tags(fragment.clone()) {
        Ok(fragment) => fragment,
        Err(err) => {
            eprintln!("Error normalizing end tags:");
            return Err(err.into());
        }
    };
    if normalized != fragment {
        println!("ℹ️  Inserting implied end tags:");
        println!("{}", fragment);
        println!();
    }

    println!("ℹ️  Pretty-printed:");
    let fragment = reindent(normalized);
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
        SgmlEvent::Character(s.into())
    }

    let mut keep_same_line = 0;
    for (i, event) in fragment.iter().enumerate().skip(1) {
        if keep_same_line > 0 {
            keep_same_line -= 1;
            continue;
        }
        match event {
            SgmlEvent::OpenStartTag(_)
            | SgmlEvent::Character(_)
            | SgmlEvent::ProcessingInstruction(_)
            | SgmlEvent::MarkupDeclaration { .. }
            | SgmlEvent::MarkedSection { .. } => transform.insert_at(i, indent(indent_level)),
            SgmlEvent::CloseStartTag => match &fragment.as_slice()[i + 1..] {
                [SgmlEvent::EndTag(_), ..] => keep_same_line = 1,
                [SgmlEvent::Character(_), SgmlEvent::EndTag(_), ..] => keep_same_line = 2,
                _ => indent_level += 1,
            },
            SgmlEvent::EndTag(_) => {
                indent_level -= 1;
                transform.insert_at(i, indent(indent_level));
            }
            SgmlEvent::Attribute(..) | SgmlEvent::XmlCloseEmptyElement => {}
        }
    }

    transform.apply(fragment)
}
