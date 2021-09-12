//! Simple parsing and deserialization of SGML.
//!
//! For a quick example of deserialization, see [`from_fragment`].

mod data;
pub mod entities;
pub mod error;
mod event_list;
pub mod parser;
pub mod transforms;
mod util;

use std::borrow::Cow;
use std::fmt::{self, Write};

pub use data::*;
pub use error::Error;
pub use event_list::*;
pub use parser::{parse, ParseError};
use util::make_owned;

#[cfg(feature = "deserialize")]
pub mod de;

#[cfg(feature = "deserialize")]
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
    /// A marked section, like `<![CDATA[ ... ]]>`.
    MarkedSection(Cow<'a, str>, Cow<'a, str>),
    /// The beginning of a start-element tag, e.g. `<EXAMPLE`.
    ///
    /// Empty start-elements (`<>`) are also represented by this event,
    /// with an empty slice.
    OpenStartTag(Cow<'a, str>),
    /// An attribute inside a start-element tag, e.g. `FOO="bar"`.
    Attribute(Cow<'a, str>, Option<Data<'a>>),
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
    Data(Data<'a>),
}

impl<'a> SgmlEvent<'a> {
    pub fn into_owned(self) -> SgmlEvent<'static> {
        match self {
            SgmlEvent::MarkupDeclaration(s) => SgmlEvent::MarkupDeclaration(make_owned(s)),
            SgmlEvent::ProcessingInstruction(s) => SgmlEvent::ProcessingInstruction(make_owned(s)),
            Self::MarkedSection(status_keywords, content) => {
                SgmlEvent::MarkedSection(make_owned(status_keywords), make_owned(content))
            }
            SgmlEvent::OpenStartTag(name) => SgmlEvent::OpenStartTag(make_owned(name)),
            SgmlEvent::Attribute(key, value) => {
                SgmlEvent::Attribute(make_owned(key), value.map(Data::into_owned))
            }
            SgmlEvent::CloseStartTag => SgmlEvent::CloseStartTag,
            SgmlEvent::XmlCloseEmptyElement => SgmlEvent::XmlCloseEmptyElement,
            SgmlEvent::EndTag(name) => SgmlEvent::EndTag(make_owned(name)),
            SgmlEvent::Data(data) => SgmlEvent::Data(data.into_owned()),
        }
    }
}

impl fmt::Display for SgmlEvent<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SgmlEvent::MarkupDeclaration(decl) | SgmlEvent::ProcessingInstruction(decl) => {
                f.write_str(decl)
            }
            SgmlEvent::MarkedSection(status_keywords, content) => {
                write!(f, "<![{}[{}]]>", status_keywords, content)
            }
            SgmlEvent::OpenStartTag(name) => write!(f, "<{}", name),
            SgmlEvent::Attribute(name, value) => {
                f.write_str(name)?;
                let (value, verbatim) = match value {
                    Some(data) => (data.as_str(), data.verbatim()),
                    None => return Ok(()),
                };
                let escape_ampersand = verbatim && value.contains('&');
                if !escape_ampersand && !value.contains('"') {
                    write!(f, "=\"{}\"", value)
                } else if !escape_ampersand && !value.contains('\'') {
                    write!(f, "='{}'", value)
                } else {
                    f.write_str("=\"")?;
                    value.chars().try_for_each(|c| match c {
                        '"' => f.write_str("&#34;"),
                        '&' if escape_ampersand => f.write_str("&#38;"),
                        c => f.write_char(c),
                    })?;
                    f.write_str("\"")
                }
            }
            SgmlEvent::CloseStartTag => f.write_str(">"),
            SgmlEvent::XmlCloseEmptyElement => f.write_str("/>"),
            SgmlEvent::EndTag(name) => write!(f, "</{}>", name),
            SgmlEvent::Data(value) => fmt::Display::fmt(&value.escape(), f),
        }
    }
}

/// Matches the most common definition of whitespace in SGML:
/// ASCII space, tab, newline, and carriage return. (`" \t\r\n"`)
///
/// This definition does not include other Unicode whitespace characters, and
/// it differs slightly from Rust's [`char::is_ascii_whitespace`] in that
/// U+000C FORM FEED is not considered whitespace.
///
/// # Example
///
/// Trimming whitespace according to SGML rules:
///
/// ```rust
/// # use sgmlish::is_sgml_whitespace;
/// let trimmed = "\n    Some text\n  ".trim_matches(is_sgml_whitespace);
/// assert_eq!(trimmed, "Some text");
/// ```
pub fn is_sgml_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

pub(crate) fn is_blank(s: &str) -> bool {
    s.trim_start_matches(is_sgml_whitespace).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sgml_whitespace() {
        assert!(is_sgml_whitespace(' '));
        assert!(is_sgml_whitespace('\t'));
        assert!(is_sgml_whitespace('\r'));
        assert!(is_sgml_whitespace('\n'));
        assert!(!is_sgml_whitespace('a'));
        assert!(!is_sgml_whitespace('\u{0c}'));
        assert!(!is_sgml_whitespace('\u{a0}'));
    }

    #[test]
    fn test_display_attribute() {
        assert_eq!(SgmlEvent::Attribute("key".into(), None).to_string(), "key");
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some(Data::CData("value".into()))).to_string(),
            "key=\"value\""
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some(Data::CData("va'lue".into()))).to_string(),
            "key=\"va'lue\""
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some(Data::CData("va\"lue".into()))).to_string(),
            "key='va\"lue'"
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some(Data::CData("va\"lu'e".into()))).to_string(),
            "key=\"va&#34;lu'e\""
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some(Data::RcData("va\"lu'e".into()))).to_string(),
            "key=\"va&#34;lu'e\""
        );

        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some(Data::RcData("a&o".into()))).to_string(),
            "key=\"a&o\""
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some(Data::CData("a&o".into()))).to_string(),
            "key=\"a&#38;o\""
        );

        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some(Data::RcData("a&o\"".into()))).to_string(),
            "key='a&o\"'"
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some(Data::CData("a&o\"".into()))).to_string(),
            "key=\"a&#38;o&#34;\""
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some(Data::RcData("a&o'".into()))).to_string(),
            "key=\"a&o'\""
        );
        assert_eq!(
            SgmlEvent::Attribute("key".into(), Some(Data::CData("a&o'".into()))).to_string(),
            "key=\"a&#38;o'\""
        );
    }
}
