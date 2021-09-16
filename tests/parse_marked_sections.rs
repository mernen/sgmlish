use sgmlish::{Data, Parser, SgmlEvent};

const SGML: &str = r##"
    <!DOCTYPE test>
    <TEST>
        one
        <![%cond[ two <FOO> three
            <?page break>
            <![RCDATA IGNORE[ four <![INCLUDE[ <BAR> five ]]> ]]>
            <![INCLUDE[ six <BAZ PROP=" ]]> "> ]]>
            </BAZ>
        ]]>
        seven
        <![IGNORE[ eight <QUUX> nine ]]>
        <![TEMP RCDATA[ <XYZZY> <![[ ten ]]> ]]>
        end
        </FOO>
    </TEST>
"##;

#[test]
fn test_include_trim_whitespace() {
    let mut events = Parser::builder()
        .expand_marked_sections()
        .expand_parameter_entities(|entity| {
            assert_eq!(entity, "cond");
            Some("INCLUDE")
        })
        .parse(SGML)
        .unwrap()
        .into_iter();

    assert_eq!(
        events.next(),
        Some(SgmlEvent::MarkupDeclaration("<!DOCTYPE test>".into()))
    );

    assert_eq!(events.next(), Some(SgmlEvent::OpenStartTag("TEST".into())));
    assert_eq!(events.next(), Some(SgmlEvent::CloseStartTag));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("one".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("two".into())))
    );
    assert_eq!(events.next(), Some(SgmlEvent::OpenStartTag("FOO".into())));
    assert_eq!(events.next(), Some(SgmlEvent::CloseStartTag));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("three".into())))
    );

    assert_eq!(
        events.next(),
        Some(SgmlEvent::ProcessingInstruction("<?page break>".into()))
    );

    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("six".into())))
    );
    assert_eq!(events.next(), Some(SgmlEvent::OpenStartTag("BAZ".into())));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Attribute(
            "PROP".into(),
            Some(Data::CData(" ]]> ".into()))
        ))
    );
    assert_eq!(events.next(), Some(SgmlEvent::CloseStartTag));
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("BAZ".into())));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("seven".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("<XYZZY> <![[ ten".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("]]>\n        end".into())))
    );
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("FOO".into())));
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("TEST".into())));
    assert_eq!(events.next(), None);
}

#[test]
fn test_include_keep_whitespace() {
    let mut events = Parser::builder()
        .trim_whitespace(false)
        .expand_marked_sections()
        .expand_parameter_entities(|_| Some("INCLUDE"))
        .parse(SGML)
        .unwrap()
        .into_iter();

    assert_eq!(
        events.next(),
        Some(SgmlEvent::MarkupDeclaration("<!DOCTYPE test>".into()))
    );

    assert_eq!(events.next(), Some(SgmlEvent::OpenStartTag("TEST".into())));
    assert_eq!(events.next(), Some(SgmlEvent::CloseStartTag));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData(
            "\n        one\n        ".into()
        )))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData(" two ".into())))
    );
    assert_eq!(events.next(), Some(SgmlEvent::OpenStartTag("FOO".into())));
    assert_eq!(events.next(), Some(SgmlEvent::CloseStartTag));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData(
            " three\n            ".into()
        )))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::ProcessingInstruction("<?page break>".into()))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("\n            ".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("\n            ".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData(" six ".into())))
    );
    assert_eq!(events.next(), Some(SgmlEvent::OpenStartTag("BAZ".into())));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Attribute(
            "PROP".into(),
            Some(Data::CData(" ]]> ".into()))
        ))
    );
    assert_eq!(events.next(), Some(SgmlEvent::CloseStartTag));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData(" ".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("\n            ".into())))
    );
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("BAZ".into())));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("\n        ".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData(
            "\n        seven\n        ".into()
        )))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("\n        ".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData(
            " <XYZZY> <![[ ten ".into()
        )))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData(
            " ]]>\n        end\n        ".into()
        )))
    );
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("FOO".into())));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("\n    ".into())))
    );
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("TEST".into())));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("\n".into())))
    );
    assert_eq!(events.next(), None);
}

#[test]
fn test_ignore_trim_whitespace() {
    let mut events = Parser::builder()
        .expand_marked_sections()
        .expand_parameter_entities(|_| Some("IGNORE"))
        .parse(SGML)
        .unwrap()
        .into_iter();

    assert_eq!(
        events.next(),
        Some(SgmlEvent::MarkupDeclaration("<!DOCTYPE test>".into()))
    );

    assert_eq!(events.next(), Some(SgmlEvent::OpenStartTag("TEST".into())));
    assert_eq!(events.next(), Some(SgmlEvent::CloseStartTag));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("one".into())))
    );
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("BAZ".into())));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData(
            "]]>\n        seven".into()
        )))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("<XYZZY> <![[ ten".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("]]>\n        end".into())))
    );
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("FOO".into())));
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("TEST".into())));
    assert_eq!(events.next(), None);
}

#[test]
fn test_cdata_trim_whitespace() {
    let mut events = Parser::builder()
        .expand_marked_sections()
        .expand_parameter_entities(|_| Some("CDATA"))
        .parse(SGML)
        .unwrap()
        .into_iter();

    assert_eq!(
        events.next(),
        Some(SgmlEvent::MarkupDeclaration("<!DOCTYPE test>".into()))
    );

    assert_eq!(events.next(), Some(SgmlEvent::OpenStartTag("TEST".into())));
    assert_eq!(events.next(), Some(SgmlEvent::CloseStartTag));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("one".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData(
            concat!(
                "two <FOO> three\n",
                "            <?page break>\n",
                "            <![RCDATA IGNORE[ four <![INCLUDE[ <BAR> five"
            )
            .into(),
        )))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("]]>".into())))
    );

    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("six".into())))
    );
    assert_eq!(events.next(), Some(SgmlEvent::OpenStartTag("BAZ".into())));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Attribute(
            "PROP".into(),
            Some(Data::CData(" ]]> ".into()))
        ))
    );
    assert_eq!(events.next(), Some(SgmlEvent::CloseStartTag));
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("BAZ".into())));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData(
            "]]>\n        seven".into()
        )))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("<XYZZY> <![[ ten".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("]]>\n        end".into())))
    );
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("FOO".into())));
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("TEST".into())));
    assert_eq!(events.next(), None);
}

#[test]
fn test_keep_unmodified_include_trim_whitespace() {
    let mut events = Parser::builder()
        .marked_section_handling(sgmlish::parser::MarkedSectionHandling::KeepUnmodified)
        .expand_parameter_entities(|_| Some("INCLUDE"))
        .parse(SGML)
        .unwrap()
        .into_iter();

    assert_eq!(
        events.next(),
        Some(SgmlEvent::MarkupDeclaration("<!DOCTYPE test>".into()))
    );

    assert_eq!(events.next(), Some(SgmlEvent::OpenStartTag("TEST".into())));
    assert_eq!(events.next(), Some(SgmlEvent::CloseStartTag));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("one".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::MarkedSection {
            status_keywords: "INCLUDE".into(),
            section: concat!(
                " two <FOO> three\n",
                "            <?page break>\n",
                "            <![RCDATA IGNORE[ four <![INCLUDE[ <BAR> five ]]> ]]>\n",
                "            <![INCLUDE[ six <BAZ PROP=\" ]]> \"> ]]>\n",
                "            </BAZ>\n",
                "        ",
            )
            .into()
        })
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("seven".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::MarkedSection {
            status_keywords: "IGNORE".into(),
            section: " eight <QUUX> nine ".into(),
        })
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::MarkedSection {
            status_keywords: "TEMP RCDATA".into(),
            section: " <XYZZY> <![[ ten ".into(),
        })
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("]]>\n        end".into())))
    );
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("FOO".into())));
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("TEST".into())));
    assert_eq!(events.next(), None);
}

#[test]
fn test_keep_unmodified_ignore_trim_whitespace() {
    let mut events = Parser::builder()
        .marked_section_handling(sgmlish::parser::MarkedSectionHandling::KeepUnmodified)
        .expand_parameter_entities(|_| Some("IGNORE"))
        .parse(SGML)
        .unwrap()
        .into_iter();

    assert_eq!(
        events.next(),
        Some(SgmlEvent::MarkupDeclaration("<!DOCTYPE test>".into()))
    );

    assert_eq!(events.next(), Some(SgmlEvent::OpenStartTag("TEST".into())));
    assert_eq!(events.next(), Some(SgmlEvent::CloseStartTag));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("one".into())))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::MarkedSection {
            status_keywords: "IGNORE".into(),
            section: concat!(
                " two <FOO> three\n",
                "            <?page break>\n",
                "            <![RCDATA IGNORE[ four <![INCLUDE[ <BAR> five ]]> ]]>\n",
                "            <![INCLUDE[ six <BAZ PROP=\" ]]> \"> "
            )
            .into()
        })
    );
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("BAZ".into())));
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData(
            "]]>\n        seven".into()
        )))
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::MarkedSection {
            status_keywords: "IGNORE".into(),
            section: " eight <QUUX> nine ".into()
        })
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::MarkedSection {
            status_keywords: "TEMP RCDATA".into(),
            section: " <XYZZY> <![[ ten ".into()
        })
    );
    assert_eq!(
        events.next(),
        Some(SgmlEvent::Character(Data::CData("]]>\n        end".into())))
    );
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("FOO".into())));
    assert_eq!(events.next(), Some(SgmlEvent::EndTag("TEST".into())));
    assert_eq!(events.next(), None);
}
