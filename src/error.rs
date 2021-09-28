//! When things don't go as planned.

/// Alias for a `Result` with the error type [`sgmlish::Error`](Error)
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for all parsing and deserialization errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An error occurred when parsing SGML data.
    ///
    /// Parsing errors are at this point simplified, so that this error type
    /// has no dependencies on transient state.
    /// If you wish to capture more details from the parser, see
    /// [`Parser::parse_with_detailed_errors`](crate::parser::Parser::parse_with_detailed_errors).
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
    #[error("invalid marked section keyword: {0}")]
    InvalidMarkedSectionKeyword(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Error>();
    }

    #[test]
    /// Ensure all the necessary bounds are met for downcasting errors
    fn test_error_dyn_cast() {
        let err: Box<dyn std::error::Error> = Box::new(Error::ParseError("".to_owned()));
        assert!(err.is::<Error>());
    }
}
