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
mod util;

use std::borrow::Cow;
use std::fmt::{self, Write};

pub use error::{Error, Result};
pub use fragment::*;
pub use parser::{parse, Parser, ParserConfig};
use util::make_owned;

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
    MarkupDeclaration(Cow<'a, str>),
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
    OpenStartTag(Cow<'a, str>),
    /// An attribute inside a start-element tag, e.g. `FOO="bar"`.
    Attribute(Cow<'a, str>, Option<Cow<'a, str>>),
    /// Closing of a start-element tag, e.g. `>`.
    CloseStartTag,
    /// XML-specific closing of empty elements, e.g. `/>`
    XmlCloseEmptyElement,
    /// An end-element tag, e.g. `</EXAMPLE>`.
    ///
    /// Empty end-element tags (`</>`) are also represented by this event,
    /// with an empty slice.
    EndTag(Cow<'a, str>),
    /// Any string of characters that is not part of a tag.
    Character(Cow<'a, str>),
}

impl<'a> SgmlEvent<'a> {
    pub fn into_owned(self) -> SgmlEvent<'static> {
        match self {
            SgmlEvent::MarkupDeclaration(s) => SgmlEvent::MarkupDeclaration(make_owned(s)),
            SgmlEvent::ProcessingInstruction(s) => SgmlEvent::ProcessingInstruction(make_owned(s)),
            Self::MarkedSection {
                status_keywords,
                section,
            } => SgmlEvent::MarkedSection {
                status_keywords: make_owned(status_keywords),
                section: make_owned(section),
            },
            SgmlEvent::OpenStartTag(name) => SgmlEvent::OpenStartTag(make_owned(name)),
            SgmlEvent::Attribute(key, value) => {
                SgmlEvent::Attribute(make_owned(key), value.map(make_owned))
            }
            SgmlEvent::CloseStartTag => SgmlEvent::CloseStartTag,
            SgmlEvent::XmlCloseEmptyElement => SgmlEvent::XmlCloseEmptyElement,
            SgmlEvent::EndTag(name) => SgmlEvent::EndTag(make_owned(name)),
            SgmlEvent::Character(text) => SgmlEvent::Character(make_owned(text)),
        }
    }
}

impl fmt::Display for SgmlEvent<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SgmlEvent::MarkupDeclaration(decl) | SgmlEvent::ProcessingInstruction(decl) => {
                f.write_str(decl)
            }
            SgmlEvent::MarkedSection {
                status_keywords,
                section,
            } => {
                write!(f, "<![{}[{}]]>", status_keywords, section)
            }
            SgmlEvent::OpenStartTag(name) => write!(f, "<{}", name),
            SgmlEvent::Attribute(name, None) => f.write_str(name),
            SgmlEvent::Attribute(name, Some(value)) => {
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
            SgmlEvent::EndTag(name) => write!(f, "</{}>", name),
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
            format!("{}", MarkupDeclaration("<?DOCTYPE HTML?>".into())),
            "<?DOCTYPE HTML?>"
        );
        assert_eq!(
            format!("{}", ProcessingInstruction("<?IS10744 FSIDR myurl>".into())),
            "<?IS10744 FSIDR myurl>"
        );

        assert_eq!(format!("{}", OpenStartTag("foo".into())), "<foo");
        assert_eq!(
            format!("{}", Attribute("foo".into(), Some("bar".into()))),
            "foo=\"bar\""
        );
        assert_eq!(format!("{}", Attribute("foo".into(), None)), "foo");
        assert_eq!(format!("{}", CloseStartTag), ">");
        assert_eq!(format!("{}", XmlCloseEmptyElement), "/>");
        assert_eq!(format!("{}", EndTag("foo".into())), "</foo>");
        assert_eq!(format!("{}", EndTag("".into())), "</>");

        assert_eq!(format!("{}", Character("hello".into())), "hello");
    }

    #[test]
    fn test_display_attribute() {
        assert_eq!(SgmlEvent::Attribute("key".into(), None).to_string(), "key");
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some("value".into())).to_string(),
            "key=\"value\""
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some("va'lue".into())).to_string(),
            "key=\"va'lue\""
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some("va\"lue".into())).to_string(),
            "key='va\"lue'"
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some("va\"lu'e".into())).to_string(),
            "key=\"va&#34;lu'e\""
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some("a&o".into())).to_string(),
            "key=\"a&#38;o\""
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some("a&o\"".into())).to_string(),
            "key=\"a&#38;o&#34;\""
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some("a&o'".into())).to_string(),
            "key=\"a&#38;o'\""
        );
    }
}
