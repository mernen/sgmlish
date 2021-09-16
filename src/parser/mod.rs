//! Access to configuration and inner workings of the parser.

use std::fmt;
use std::ops::Deref;

use crate::SgmlFragment;

pub use self::parser::*;

pub mod events;
mod parser;
pub mod raw;
pub mod util;

pub(crate) type DefaultErrorType<I> = nom::error::VerboseError<I>;

/// Parses the given string using a [`Parser`] with default settings,
/// then yielding an [`SgmlFragment`].
///
/// After inserting implied end tags (if necessary), use [`from_fragment`]
/// to deserialize into a specific type.
///
/// [`from_fragment`]: crate::from_fragment
pub fn parse(input: &str) -> crate::Result<SgmlFragment> {
    Parser::new().parse(input)
}

/// The error type for parse errors.
///
/// This error contains a reference to the original input string;
/// when converted to the more general [`Error`] type, this link is lost,
/// and only a description of the original error is kept.
///
/// [`Error`]: crate::Error
#[derive(Debug)]
pub struct ParseError<I, E> {
    input: I,
    error: E,
}

impl<I, E> ParseError<I, E>
where
    I: Deref<Target = str>,
    E: nom::error::ParseError<I>,
{
    pub(crate) fn from_nom(input: I, error: E) -> Self {
        ParseError { input, error }
    }

    /// Returns the original input for this error.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Returns the internal error produced by the parsing functions.
    pub fn into_inner(self) -> E {
        self.error
    }
}

impl fmt::Display for ParseError<&str, DefaultErrorType<&str>> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&nom::error::convert_error(self.input, self.error.clone()))
    }
}

// impl<I: Deref<Target = str>, E: fmt::Display> fmt::Display for ParseError<I, E> {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         fmt::Display::fmt(&self.error, f)
//     }
// }
