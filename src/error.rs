//! When things don't go as planned.

use core::fmt;

use crate::ParseError;

/// Alias for a `Result` with the error type [`sgmlish::Error`](Error)
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for all parsing and deserialization errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An error occurred when parsing SGML data.
    #[error("{0}")]
    ParseError(String),
    /// An error occurred when deseralizing.
    #[cfg(feature = "serde")]
    #[error(transparent)]
    DeserializationError(#[from] crate::de::DeserializationError),
    /// An error occurred when normalizing end tags.
    #[error(transparent)]
    NormalizationError(#[from] crate::transforms::NormalizationError),
    /// An error occurred when decoding an entity reference.
    #[error(transparent)]
    EntityError(#[from] crate::entities::EntityError),
    /// An error ocurred when processing a marked section.
    #[error("unrecognized marked section keyword: {0}")]
    InvalidMarkedSectionKeyword(String),
}

impl<I: std::ops::Deref<Target = str>, E> From<ParseError<I, E>> for Error
where
    ParseError<I, E>: fmt::Display,
{
    fn from(err: ParseError<I, E>) -> Self {
        Error::ParseError(err.to_string())
    }
}
