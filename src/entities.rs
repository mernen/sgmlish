//! Utilities for expanding entity and character references.

use std::borrow::Cow;
use std::char;

use nom::branch::alt;
use nom::bytes::complete::tag;
use nom::character::complete::{digit1, hex_digit1};
use nom::combinator::{map, opt, recognize};
use nom::sequence::{preceded, terminated};
use nom::IResult;

use crate::parser::raw::name;

/// The type returned by expansion operations.
pub type Result<T = ()> = std::result::Result<T, EntityError>;

/// The error type in the event an invalid entity or character reference is found.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum EntityError {
    /// An invalid character reference was found.
    /// For example, a character code that goes beyond the range of Unicode: `&#1234567;`
    #[error("invalid character reference")]
    InvalidCharacterRef,
    /// A reference to an unknown entity was found.
    /// That means the entity expansion closure was called, and it returned `None`.
    ///
    /// When invoking [`expand_character_references`], any entity reference is considered undefined.
    #[error("entity '{0}' is not defined")]
    UndefinedEntity(String),
}

/// Expands character references (`&#123;`) in the given text.
/// Any entity references are treated as errors.
///
/// # Example
///
/// ```rust
/// # use sgmlish::entities::expand_character_references;
/// let expanded = expand_character_references("&#60;hello&#44; world&#33;&#62;");
/// assert_eq!(expanded, Ok("<hello, world!>".into()));
/// ```
pub fn expand_character_references(text: &str) -> Result<Cow<str>> {
    expand_entities(text, |_| None::<&str>)
}

/// Expands entities in the text using the given closure as lookup.
/// Character references (`&#123;`) are also expanded, without going through the closure.
///
/// # Example
///
/// ```rust
/// # use std::collections::HashMap;
/// # use sgmlish::entities::expand_entities;
/// let mut entities = HashMap::new();
/// entities.insert("eacute", "é");
///
/// let expanded = expand_entities("caf&eacute; &#9749;", |entity| entities.get(entity));
/// assert_eq!(expanded, Ok("café ☕".into()));
/// ```
pub fn expand_entities<F, T>(text: &str, mut f: F) -> Result<Cow<str>>
where
    F: FnMut(&str) -> Option<T>,
    T: AsRef<str>,
{
    let mut parts = text.split('&');

    let first = parts.next().unwrap();
    if first.len() == text.len() {
        return Ok(text.into());
    }

    let mut out = String::new();
    out.push_str(first);

    for part in parts {
        match entity_or_char_ref(part) {
            Ok((rest, entity)) => {
                match entity {
                    EntityRef::Entity(name) => match f(name) {
                        Some(value) => out.push_str(value.as_ref()),
                        None => return Err(EntityError::UndefinedEntity(name.to_owned())),
                    },
                    EntityRef::Char(c) => out.push(c),
                    EntityRef::InvalidChar => return Err(EntityError::InvalidCharacterRef),
                }
                out.push_str(rest);
            }
            Err(_) => {
                out.push('&');
                out.push_str(part)
            }
        }
    }

    Ok(out.into())
}

fn entity_or_char_ref(input: &str) -> IResult<&str, EntityRef> {
    terminated(
        alt((
            map(
                preceded(
                    tag("#"),
                    alt((
                        map(digit1, |code: &str| code.parse().ok()),
                        // Hex escape codes are actually only valid in XML
                        preceded(
                            tag("x"),
                            map(hex_digit1, |code| u32::from_str_radix(code, 16).ok()),
                        ),
                    )),
                ),
                |code| {
                    code.and_then(char::from_u32)
                        .map(EntityRef::Char)
                        .unwrap_or(EntityRef::InvalidChar)
                },
            ),
            map(recognize(preceded(opt(tag("#")), name)), EntityRef::Entity),
        )),
        opt(tag(";")),
    )(input)
}

enum EntityRef<'a> {
    Entity(&'a str),
    Char(char),
    InvalidChar,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_refs() {
        fn assert_noop(s: &str) {
            let result = expand_character_references(s);
            assert_eq!(result, Ok(s.into()));
        }

        assert_noop("foo&");
        assert_noop("foo&&");
        assert_noop("foo&;bar");
        assert_noop("foo&&;bar");
        assert_noop("foo&#");
        assert_noop("foo&#;");
        assert_noop("foo&#;bar");
        assert_noop("foo&##bar");
    }

    #[test]
    fn test_invalid_character_ref() {
        let result = expand_character_references("foo&#x110000;bar");
        assert_eq!(result, Err(EntityError::InvalidCharacterRef));
    }

    #[test]
    fn test_expand_character_references_hex() {
        let result = expand_character_references("fo&#x6f; bar &#xFeFf;");
        assert_eq!(result, Ok("foo bar \u{feff}".into()));
    }

    #[test]
    fn test_expand_character_references_missing_semicolon() {
        let result = expand_character_references("fo&#x6f bar &#xFeFf");
        assert_eq!(result, Ok("foo bar \u{feff}".into()));
    }

    #[test]
    fn test_expand_entities_noop() {
        let result = expand_entities("this string has no references", |_| -> Option<&str> {
            unreachable!()
        });
        assert!(matches!(result.unwrap(), Cow::Borrowed(_)));
    }

    #[test]
    fn test_expand_entities_lookup() {
        let result = expand_entities("test &foo;&bar.x; &baz&qu-ux\n", |key| match key {
            "foo" => Some("x"),
            "bar.x" => Some("y"),
            "baz" => Some("z"),
            "qu-ux" => Some("w"),
            x => panic!("unexpected reference: {:?}", x),
        });
        assert_eq!(result, Ok("test xy zw\n".into()));
    }

    #[test]
    fn test_expand_entities_invalid_entity() {
        let result = expand_entities("test &foo;&bar;", |key| match key {
            "foo" => Some("x"),
            "bar" => None,
            x => panic!("unexpected reference: {:?}", x),
        });
        assert_eq!(result, Err(EntityError::UndefinedEntity("bar".to_owned())));
    }

    #[test]
    fn test_expand_entities_invalid_function() {
        let mut called = false;
        let result = expand_entities("foo&#test;bar", |x| {
            called = true;
            assert_eq!(x, "#test");
            None::<&str>
        });
        assert!(called);
        assert_eq!(
            result,
            Err(EntityError::UndefinedEntity("#test".to_owned()))
        );
    }
}
