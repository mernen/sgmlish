use core::fmt;

use crate::ParseError;

/// Alias for a `Result` with the error type [`sgmlish::Error`](Error)
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    ParseError(String),
    #[cfg(feature = "deserialize")]
    #[error(transparent)]
    DeserializationError(#[from] crate::de::DeserializationError),
    #[error(transparent)]
    NormalizationError(#[from] crate::transforms::NormalizationError),
    #[error(transparent)]
    EntityError(#[from] crate::entities::EntityError),
    #[error("unrecognized marked section keyword: {0}")]
    UnrecognizedMarkedSectionKeyword(String),
}

impl<I: std::ops::Deref<Target = str>, E> From<ParseError<I, E>> for Error
where
    ParseError<I, E>: fmt::Display,
{
    fn from(err: ParseError<I, E>) -> Self {
        Error::ParseError(err.to_string())
    }
}
