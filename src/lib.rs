//! Simple parsing and deserialization of SGML.
//!
//! For a quick example of deserialization, see [`from_fragment`].

pub mod entities;
pub mod error;
mod fragment;
pub mod marked_sections;
pub mod parser;
pub mod text;
pub mod transforms;

use std::borrow::Cow;
use std::fmt::{self, Write};

pub use error::{Error, Result};
pub use fragment::*;
pub use parser::{parse, Parser, ParserConfig};

#[cfg(feature = "serde")]
pub mod de;

#[cfg(feature = "serde")]
pub use de::from_fragment;

/// Represents a relevant occurrence in an SGML document.
///
/// Some aspects to keep in mind when working with events:
///
/// * Start tags are represented by *two or more* events:
///   one event for the opening of the tag (`<A`),
///   optionally followed by one event for each attribute (`HREF="example"`),
///   and finally one event for the closing of the tag (`>`).
/// * End tags (`</A>`), however, are single-event occurrences.
/// * Comments are *ignored*, and do not show up as events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SgmlEvent<'a> {
    /// A markup declaration, like `<!SGML ...>` or `<!DOCTYPE ...>`.
    ///
    /// Markup declarations that are purely comments are ignored.
    MarkupDeclaration {
        keyword: Cow<'a, str>,
        body: Cow<'a, str>,
    },
    /// A processing instruction, e.g. `<?EXAMPLE>`
    ProcessingInstruction(Cow<'a, str>),
    /// A marked section, like `<![IGNORE[...]]>`.
    MarkedSection {
        status_keywords: Cow<'a, str>,
        section: Cow<'a, str>,
    },
    /// The beginning of a start-element tag, e.g. `<EXAMPLE`.
    ///
    /// Empty start-elements (`<>`) are also represented by this event,
    /// with an empty slice.
    OpenStartTag { name: Cow<'a, str> },
    /// An attribute inside a start-element tag, e.g. `FOO="bar"`.
    Attribute {
        name: Cow<'a, str>,
        value: Option<Cow<'a, str>>,
    },
    /// Closing of a start-element tag, e.g. `>`.
    CloseStartTag,
    /// XML-specific closing of empty elements, e.g. `/>`
    XmlCloseEmptyElement,
    /// An end-element tag, e.g. `</EXAMPLE>`.
    ///
    /// Empty end-element tags (`</>`) are also represented by this event,
    /// with an empty slice.
    EndTag { name: Cow<'a, str> },
    /// Any string of characters that is not part of a tag.
    Character(Cow<'a, str>),
}

impl<'a> SgmlEvent<'a> {
    pub fn into_owned(self) -> SgmlEvent<'static> {
        match self {
            SgmlEvent::MarkupDeclaration { keyword, body } => SgmlEvent::MarkupDeclaration {
                keyword: make_owned(keyword),
                body: make_owned(body),
            },
            SgmlEvent::ProcessingInstruction(s) => SgmlEvent::ProcessingInstruction(make_owned(s)),
            Self::MarkedSection {
                status_keywords,
                section,
            } => SgmlEvent::MarkedSection {
                status_keywords: make_owned(status_keywords),
                section: make_owned(section),
            },
            SgmlEvent::OpenStartTag { name } => SgmlEvent::OpenStartTag {
                name: make_owned(name),
            },
            SgmlEvent::Attribute { name, value } => SgmlEvent::Attribute {
                name: make_owned(name),
                value: value.map(make_owned),
            },
            SgmlEvent::CloseStartTag => SgmlEvent::CloseStartTag,
            SgmlEvent::XmlCloseEmptyElement => SgmlEvent::XmlCloseEmptyElement,
            SgmlEvent::EndTag { name } => SgmlEvent::EndTag {
                name: make_owned(name),
            },
            SgmlEvent::Character(text) => SgmlEvent::Character(make_owned(text)),
        }
    }
}

fn make_owned<T: ?Sized + ToOwned>(cow: Cow<T>) -> Cow<'static, T> {
    match cow {
        Cow::Borrowed(x) => Cow::Owned(x.to_owned()),
        Cow::Owned(x) => Cow::Owned(x),
    }
}

impl fmt::Display for SgmlEvent<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SgmlEvent::MarkupDeclaration { keyword, body } => {
                write!(f, "<!{}", keyword)?;
                if !body.is_empty() {
                    write!(f, " {}", body)?;
                }
                f.write_str(">")
            }
            SgmlEvent::ProcessingInstruction(decl) => f.write_str(decl),
            SgmlEvent::MarkedSection {
                status_keywords,
                section,
            } => {
                write!(f, "<![{}[{}]]>", status_keywords, section)
            }
            SgmlEvent::OpenStartTag { name } => write!(f, "<{}", name),
            SgmlEvent::Attribute { name, value: None } => f.write_str(name),
            SgmlEvent::Attribute {
                name,
                value: Some(value),
            } => {
                f.write_str(name)?;
                let escape_ampersand = value.contains('&');
                if !escape_ampersand && !value.contains('"') {
                    write!(f, "=\"{}\"", value)
                } else if !escape_ampersand && !value.contains('\'') {
                    write!(f, "='{}'", value)
                } else {
                    f.write_str("=\"")?;
                    value.chars().try_for_each(|c| match c {
                        '"' => f.write_str("&#34;"),
                        '&' => f.write_str("&#38;"),
                        c => f.write_char(c),
                    })?;
                    f.write_str("\"")
                }
            }
            SgmlEvent::CloseStartTag => f.write_str(">"),
            SgmlEvent::XmlCloseEmptyElement => f.write_str("/>"),
            SgmlEvent::EndTag { name } => write!(f, "</{}>", name),
            SgmlEvent::Character(value) => fmt::Display::fmt(&text::escape(value), f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_display() {
        use super::SgmlEvent::*;
        assert_eq!(
            format!(
                "{}",
                MarkupDeclaration {
                    keyword: "DOCTYPE".into(),
                    body: "HTML".into(),
                },
            ),
            "<!DOCTYPE HTML>"
        );
        assert_eq!(
            format!(
                "{}",
                MarkupDeclaration {
                    keyword: "foo".into(),
                    body: "".into(),
                },
            ),
            "<!foo>"
        );
        assert_eq!(
            format!("{}", ProcessingInstruction("<?IS10744 FSIDR myurl>".into())),
            "<?IS10744 FSIDR myurl>"
        );

        assert_eq!(format!("{}", OpenStartTag { name: "foo".into() }), "<foo");
        assert_eq!(
            format!(
                "{}",
                Attribute {
                    name: "foo".into(),
                    value: Some("bar".into()),
                }
            ),
            "foo=\"bar\""
        );
        assert_eq!(
            format!(
                "{}",
                Attribute {
                    name: "foo".into(),
                    value: None,
                }
            ),
            "foo"
        );
        assert_eq!(format!("{}", CloseStartTag), ">");
        assert_eq!(format!("{}", XmlCloseEmptyElement), "/>");
        assert_eq!(format!("{}", EndTag { name: "foo".into() }), "</foo>");
        assert_eq!(format!("{}", EndTag { name: "".into() }), "</>");

        assert_eq!(format!("{}", Character("hello".into())), "hello");
    }

    #[test]
    fn test_display_attribute() {
        assert_eq!(
            SgmlEvent::Attribute {
                name: "key".into(),
                value: None
            }
            .to_string(),
            "key"
        );
        assert_eq!(
            SgmlEvent::Attribute {
                name: "key".into(),
                value: Some("value".into()),
            }
            .to_string(),
            "key=\"value\""
        );
        assert_eq!(
            SgmlEvent::Attribute {
                name: "key".into(),
                value: Some("va'lue".into()),
            }
            .to_string(),
            "key=\"va'lue\""
        );
        assert_eq!(
            SgmlEvent::Attribute {
                name: "key".into(),
                value: Some("va\"lue".into()),
            }
            .to_string(),
            "key='va\"lue'"
        );
        assert_eq!(
            SgmlEvent::Attribute {
                name: "key".into(),
                value: Some("va\"lu'e".into()),
            }
            .to_string(),
            "key=\"va&#34;lu'e\""
        );
        assert_eq!(
            SgmlEvent::Attribute {
                name: "key".into(),
                value: Some("a&o".into()),
            }
            .to_string(),
            "key=\"a&#38;o\""
        );
        assert_eq!(
            SgmlEvent::Attribute {
                name: "key".into(),
                value: Some("a&o\"".into()),
            }
            .to_string(),
            "key=\"a&#38;o&#34;\""
        );
        assert_eq!(
            SgmlEvent::Attribute {
                name: "key".into(),
                value: Some("a&o'".into()),
            }
            .to_string(),
            "key=\"a&#38;o'\""
        );
    }
}
