//! Access to configuration and inner workings of the parser.

use std::borrow::Cow;
use std::fmt;

use crate::marked_sections::MarkedSectionStatus;
use crate::{entities, text, SgmlFragment};

mod error;
pub mod events;
pub mod raw;
pub mod util;

pub use error::*;

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

/// The parser for SGML data.
///
/// The parser is only capable of working directly with strings,
/// meaning the content must be decoded beforehand. If you want to work with
/// data in character sets other than UTF-8, you may want to have a look at the
/// [`encoding_rs`] crate.
///
/// [`encoding_rs`]: https://docs.rs/encoding_rs/
#[derive(Debug, Default)]
pub struct Parser {
    config: ParserConfig,
}

impl Parser {
    /// Creates a new parser with default settings.
    ///
    /// The default settings are:
    ///
    /// * Whitespace is automatically trimmed
    /// * Tag and attribute names are kept in original casing
    /// * Only `CDATA` and `RCDATA` marked sections are allowed;
    ///   `IGNORE` and `INCLUDE` blocks, for instance, are rejected,
    ///   as are parameter entities (`%example;`) in marked sections
    /// * Only character references (`&#33;`) are accepted; all entities (`&example;`)
    ///   are rejected
    /// * Markup declarations and processing instructions are preserved
    pub fn new() -> Self {
        Default::default()
    }

    /// Creates a new parser builder
    pub fn builder() -> ParserBuilder {
        ParserBuilder::new()
    }

    /// Parses the given input.
    ///
    /// Parse errors are flattened into a descriptive string.
    /// To capture the full error, use [`parse_with_detailed_errors`](Parser::parse_with_detailed_errors).
    pub fn parse<'a>(&self, input: &'a str) -> crate::Result<SgmlFragment<'a>> {
        self.parse_with_detailed_errors::<ContextualizedError<_>>(input)
            .map_err(|err| crate::Error::ParseError(err.describe(&input)))
    }

    /// Parses the given input, using a different error handler for parser errors,
    /// and capturing the full error type.
    ///
    /// Different [`nom`] error handlers may be used to adjust between speed and
    /// level of detail in error messages.
    pub fn parse_with_detailed_errors<'a, E>(&self, input: &'a str) -> Result<SgmlFragment<'a>, E>
    where
        E: nom::error::ParseError<&'a str>
            + nom::error::ContextError<&'a str>
            + nom::error::FromExternalError<&'a str, crate::Error>,
    {
        use nom::Finish;
        let (rest, events) = events::document_entity::<E>(input, &self.config).finish()?;
        debug_assert!(rest.is_empty(), "document_entity should be all_consuming");

        let events = events.collect::<Vec<_>>();

        Ok(SgmlFragment::from(events))
    }
}

/// The configuration for a [`Parser`].
pub struct ParserConfig {
    /// When `true`, leading and trailing whitespace from
    /// [`Character`](crate::SgmlEvent::Character) events will be trimmed.
    /// Defaults to `true`.
    pub trim_whitespace: bool,
    /// Defines how tag and attribute names should be handled.
    pub name_normalization: NameNormalization,
    pub marked_section_handling: MarkedSectionHandling,
    pub ignore_markup_declarations: bool,
    pub ignore_processing_instructions: bool,
    entity_fn: Option<EntityFn>,
    parameter_entity_fn: Option<EntityFn>,
}

type EntityFn = Box<dyn Fn(&str) -> Option<Cow<'static, str>>>;

/// How tag and attribute names should be handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NameNormalization {
    /// Keep tag and attribute names as-is.
    Unchanged,
    /// Normalize all tag and attribute names to lowercase.
    ToLowercase,
    /// Normalize all tag and attribute names to uppercase.
    ToUppercase,
}

impl Default for NameNormalization {
    fn default() -> Self {
        NameNormalization::Unchanged
    }
}

impl NameNormalization {
    pub fn normalize<'a>(&self, name: Cow<'a, str>) -> Cow<'a, str> {
        match self {
            NameNormalization::ToLowercase if name.chars().any(char::is_uppercase) => {
                name.to_lowercase().into()
            }
            NameNormalization::ToUppercase if name.chars().any(char::is_lowercase) => {
                name.to_uppercase().into()
            }
            _ => name,
        }
    }
}

/// How marked sections (`<![CDATA[example]]>`) should be handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkedSectionHandling {
    /// Keep all marked sections as raw [`MarkedSection`](crate::SgmlEvent::MarkedSection)
    /// events in the stream.
    KeepUnmodified,
    /// Expand `CDATA` and `RCDATA` sections into [`Character`][crate::SgmlEvent::Character]
    /// events, treat anything else as a parsing error.
    AcceptOnlyCharacterData,
    /// Expand also `INCLUDE` and `IGNORE` sections.
    ExpandAll,
}

impl Default for MarkedSectionHandling {
    fn default() -> Self {
        MarkedSectionHandling::AcceptOnlyCharacterData
    }
}

impl MarkedSectionHandling {
    /// Parses the status keywords in the given string according to the chosen rules.
    ///
    /// Returns `None` if any of the keywords is rejected.
    pub fn parse_keywords<'a>(
        &self,
        status_keywords: &'a str,
    ) -> Result<MarkedSectionStatus, &'a str> {
        match self {
            // In this mode, only one keyword is accepted; even combining
            // two otherwise acceptable keywords (e.g. `<![CDATA CDATA[`) is rejected
            MarkedSectionHandling::AcceptOnlyCharacterData => match status_keywords.parse() {
                Ok(status @ (MarkedSectionStatus::CData | MarkedSectionStatus::RcData)) => {
                    Ok(status)
                }
                _ => Err(status_keywords),
            },
            _ => MarkedSectionStatus::from_keywords(status_keywords),
        }
    }
}

impl ParserConfig {
    /// Trims the given text according to the configured rules.
    pub fn trim<'a>(&self, text: &'a str) -> &'a str {
        if self.trim_whitespace {
            text.trim_matches(text::is_sgml_whitespace)
        } else {
            text
        }
    }

    /// Parses the given replaceable character data, returning its final form.
    pub fn parse_rcdata<'a, E>(&self, rcdata: &'a str) -> Result<Cow<'a, str>, nom::Err<E>>
    where
        E: nom::error::ContextError<&'a str> + nom::error::FromExternalError<&'a str, crate::Error>,
    {
        let f = self.entity_fn.as_deref().unwrap_or(&|_| None);
        entities::expand_entities(rcdata, f).map_err(|err| into_nom_failure(rcdata, err))
    }

    /// Parses parameter entities in the given markup declaration text, returning its final form.
    pub fn parse_markup_declaration_text<'a, E>(
        &self,
        text: &'a str,
    ) -> Result<Cow<'a, str>, nom::Err<E>>
    where
        E: nom::error::ContextError<&'a str> + nom::error::FromExternalError<&'a str, crate::Error>,
    {
        let f = self.parameter_entity_fn.as_deref().unwrap_or(&|_| None);
        entities::expand_parameter_entities(text, f).map_err(|err| into_nom_failure(text, err))
    }
}

fn into_nom_failure<'a, E>(input: &'a str, err: entities::EntityError) -> nom::Err<E>
where
    E: nom::error::ContextError<&'a str> + nom::error::FromExternalError<&'a str, crate::Error>,
{
    use nom::Slice;
    let slice = input.slice(err.position..);
    nom::Err::Failure(E::add_context(
        slice,
        if slice.starts_with("&#") {
            "character reference"
        } else {
            "entity"
        },
        E::from_external_error(slice, nom::error::ErrorKind::MapRes, err.into()),
    ))
}

impl Default for ParserConfig {
    /// Creates a new, default `ParserConfig`. See [`Parser::new`] for the default settings.
    fn default() -> Self {
        ParserConfig {
            trim_whitespace: true,
            name_normalization: Default::default(),
            marked_section_handling: Default::default(),
            ignore_markup_declarations: false,
            ignore_processing_instructions: false,
            entity_fn: None,
            parameter_entity_fn: None,
        }
    }
}

impl fmt::Debug for ParserConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ParserConfig")
            .field("trim_whitespace", &self.trim_whitespace)
            .field("process_marked_sections", &self.marked_section_handling)
            .field("expand_entity", &omit(&self.entity_fn))
            .field("expand_parameter_entity", &omit(&self.parameter_entity_fn))
            .finish()
    }
}

/// A fluent interface for configuring parsers.
#[derive(Default, Debug)]
pub struct ParserBuilder {
    config: ParserConfig,
}

/// A builder for parser configurations.
impl ParserBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Default::default()
    }

    /// Defines whether whitespace surrounding text should be trimmed.
    pub fn trim_whitespace(mut self, trim_whitespace: bool) -> Self {
        self.config.trim_whitespace = trim_whitespace;
        self
    }

    /// Defines how tag and attribute names should be normalized.
    pub fn name_normalization(mut self, name_normalization: NameNormalization) -> Self {
        self.config.name_normalization = name_normalization;
        self
    }

    /// Normalizes all tag and attribute names to lowercase.
    pub fn lowercase_names(self) -> Self {
        self.name_normalization(NameNormalization::ToLowercase)
    }

    /// Normalizes all tag and attribute names to lowercase.
    pub fn uppercase_names(self) -> Self {
        self.name_normalization(NameNormalization::ToUppercase)
    }

    /// Defines a closure to be used to resolve entities.
    pub fn expand_entities<F, T>(mut self, f: F) -> Self
    where
        F: Fn(&str) -> Option<T> + 'static,
        T: Into<Cow<'static, str>>,
    {
        self.config.entity_fn = Some(Box::new(move |entity| f(entity).map(Into::into)));
        self
    }

    /// Defines a closure to be used to resolve entities.
    pub fn expand_parameter_entities<F, T>(mut self, f: F) -> Self
    where
        F: Fn(&str) -> Option<T> + 'static,
        T: Into<Cow<'static, str>>,
    {
        self.config.parameter_entity_fn = Some(Box::new(move |entity| f(entity).map(Into::into)));
        self
    }

    /// Changes how marked sections should be handled.
    pub fn marked_section_handling(mut self, mode: MarkedSectionHandling) -> Self {
        self.config.marked_section_handling = mode;
        self
    }

    /// Enables support for all marked sections, including `<![INCLUDE[...]]>`
    /// and `<![IGNORE[...]]>`.
    ///
    /// By default, only `CDATA` and `RCDATA` marked sections are accepted.
    pub fn expand_marked_sections(self) -> Self {
        self.marked_section_handling(MarkedSectionHandling::ExpandAll)
    }

    /// Changes whether markup declarations (`<!EXAMPLE>`) should be ignored
    /// or present in the event stream.
    pub fn ignore_markup_declarations(mut self, ignore: bool) -> Self {
        self.config.ignore_markup_declarations = ignore;
        self
    }

    /// Changes whether processing instructions (`<?example>`) should be ignored
    /// or present in the event stream.
    pub fn ignore_processing_instructions(mut self, ignore: bool) -> Self {
        self.config.ignore_processing_instructions = ignore;
        self
    }

    /// Builds a new parser from the given configuration.
    pub fn build(self) -> Parser {
        Parser {
            config: self.config,
        }
    }

    /// Parses the given input with the built parser.
    ///
    /// To reuse the same parser for multiple inputs, use [`build()`](ParserBuilder::build)
    /// then [`Parser::parse()`].
    pub fn parse(self, input: &str) -> crate::Result<SgmlFragment> {
        self.build().parse(input)
    }

    /// Returns a [`ParserConfig`] with the configuration that was built using other methods.
    pub fn into_config(self) -> ParserConfig {
        self.config
    }
}

fn omit<T>(opt: &Option<T>) -> impl fmt::Debug {
    opt.as_ref().map(|_| Ellipsis)
}

struct Ellipsis;

impl fmt::Debug for Ellipsis {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_normalization_unchanged() {
        assert!(matches!(
            NameNormalization::Unchanged.normalize("hello".into()),
            Cow::Borrowed("hello")
        ));
        assert!(matches!(
            NameNormalization::Unchanged.normalize("Hello".into()),
            Cow::Borrowed("Hello")
        ));
        assert!(matches!(
            NameNormalization::Unchanged.normalize("HELLO".into()),
            Cow::Borrowed("HELLO")
        ));
        assert!(matches!(
            NameNormalization::Unchanged.normalize("題名".into()),
            Cow::Borrowed("題名")
        ));
        assert!(matches!(
            NameNormalization::Unchanged.normalize("grüße".into()),
            Cow::Borrowed("grüße")
        ));
    }

    #[test]
    fn test_name_normalization_to_lowercase() {
        assert!(matches!(
            NameNormalization::ToLowercase.normalize("hello".into()),
            Cow::Borrowed("hello")
        ));
        assert_eq!(
            NameNormalization::ToLowercase.normalize("Hello".into()),
            "hello"
        );
        assert_eq!(
            NameNormalization::ToLowercase.normalize("HELLO".into()),
            "hello"
        );
        assert!(matches!(
            NameNormalization::ToLowercase.normalize("題名".into()),
            Cow::Borrowed("題名")
        ));
        assert!(matches!(
            NameNormalization::ToLowercase.normalize("grüße".into()),
            Cow::Borrowed("grüße")
        ));
        assert_eq!(
            NameNormalization::ToLowercase.normalize("Grüße".into()),
            "grüße"
        );
    }

    #[test]
    fn test_name_normalization_to_uppercase() {
        assert!(matches!(
            NameNormalization::ToUppercase.normalize("HELLO".into()),
            Cow::Borrowed("HELLO")
        ));
        assert_eq!(
            NameNormalization::ToUppercase.normalize("Hello".into()),
            "HELLO"
        );
        assert_eq!(
            NameNormalization::ToUppercase.normalize("hello".into()),
            "HELLO"
        );
        assert!(matches!(
            NameNormalization::ToUppercase.normalize("題名".into()),
            Cow::Borrowed("題名")
        ));
        assert!(matches!(
            NameNormalization::ToUppercase.normalize("GRÜSSE".into()),
            Cow::Borrowed("GRÜSSE")
        ));
        assert_eq!(
            NameNormalization::ToUppercase.normalize("grüße".into()),
            "GRÜSSE"
        );
    }
}
