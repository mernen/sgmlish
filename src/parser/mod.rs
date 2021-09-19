use std::fmt;
use std::ops::Deref;

use nom::Finish;

use crate::SgmlFragment;

pub mod events;
pub mod raw;
pub mod util;

pub(crate) type DefaultErrorType<I> = nom::error::VerboseError<I>;

/// Parses the given string, yielding an [`SgmlFragment`].
///
/// This list can then be adjusted as desired, and later deserialized using [`from_fragment`].
///
/// [`from_fragment`]: crate::from_fragment
pub fn parse(input: &str) -> Result<SgmlFragment, ParseError<&str, DefaultErrorType<&str>>> {
    parse_with_error_type(input)
}

/// Parses the given string with a custom error type.
///
/// Different error types can make different tradeoffs between performance and level of detail.
pub fn parse_with_error_type<'a, E>(
    input: &'a str,
) -> Result<SgmlFragment<'a>, ParseError<&'a str, E>>
where
    E: nom::error::ParseError<&'a str> + nom::error::ContextError<&'a str> + fmt::Display,
{
    let (rest, events) = events::document_entity::<E>(input)
        .finish()
        .map_err(|error| ParseError::from_nom(input, error))?;
    debug_assert!(rest.is_empty(), "document_entity should be all_consuming");

    let events = events.collect::<Vec<_>>();

    Ok(SgmlFragment::from(events))
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

impl<I: Deref<Target = str>, E> ParseError<I, E> {
    pub(crate) fn from_nom(input: I, error: E) -> Self {
        ParseError { input, error }
    }

    /// Returns the original input for this error.
    pub fn input(&self) -> &str {
        &self.input
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
