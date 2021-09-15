use std::borrow::Cow;
use std::fmt;

use crate::marked_sections::MarkedSectionStatus;
use crate::{entities, is_sgml_whitespace, Data};

// Import used for documentation links
#[allow(unused_imports)]
use crate::SgmlEvent;

pub struct ParserConfig {
    /// When `true`, leading and trailing whitespace from [`Character`](SgmlEvent::Character) events will be trimmed.
    /// Defaults to `true`.
    pub trim_whitespace: bool,
    /// Defines how tag and attribute names should be handled.
    pub name_normalization: NameNormalization,
    pub marked_section_handling: MarkedSectionHandling,
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
    pub fn normalize<'a>(&self, mut name: Cow<'a, str>) -> Cow<'a, str> {
        match self {
            NameNormalization::ToLowercase if name.chars().any(|c| c.is_ascii_uppercase()) => {
                name.to_mut().make_ascii_lowercase();
                name
            }
            NameNormalization::ToUppercase if name.chars().any(|c| c.is_ascii_lowercase()) => {
                name.to_mut().make_ascii_uppercase();
                name
            }
            _ => name,
        }
    }
}

/// How marked sections (`<![CDATA[example]]>`) should be handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkedSectionHandling {
    /// Keep all marked sections as [`MarkedSection`](SgmlEvent::MarkedSection)
    /// events in the stream.
    KeepUnmodified,
    /// Expand `CDATA` and `RCDATA` sections into [`Character`][SgmlEvent::Character] events,
    /// treat anything else as a parse error.
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
    pub fn parse_keywords(&self, status_keywords: &str) -> Option<MarkedSectionStatus> {
        match self {
            // In this mode, only one keyword is accepted; even combining
            // two otherwise acceptable keywords (e.g. `<![CDATA CDATA[`) is rejected
            MarkedSectionHandling::AcceptOnlyCharacterData => match status_keywords.parse() {
                Ok(status @ (MarkedSectionStatus::CData | MarkedSectionStatus::RcData)) => {
                    Some(status)
                }
                _ => None,
            },
            _ => MarkedSectionStatus::from_keywords(status_keywords).ok(),
        }
    }
}

impl ParserConfig {
    /// Creates a new `ParserConfig`, with default settings.
    ///
    /// The default settings are:
    ///
    /// * Whitespace is automatically trimmed
    /// * Tag and attribute names are kept in original form
    /// * Only `CDATA` and `RCDATA` marked sections are allowed;
    ///   `IGNORE` and `INCLUDE` blocks, for instance, are rejected
    /// * Only character references (`&#33;`) are accepted; all entities (`&example;`)
    ///   are rejected
    /// * Parameter entities (`%example;`) in marked sections are rejected
    pub fn new() -> Self {
        ParserConfig {
            trim_whitespace: true,
            name_normalization: Default::default(),
            marked_section_handling: Default::default(),
            entity_fn: None,
            parameter_entity_fn: None,
        }
    }

    /// Creates a new builder, for ease of configuration.
    pub fn builder() -> ParserConfigBuilder {
        ParserConfigBuilder::new()
    }

    pub fn trim<'a>(&self, text: &'a str) -> &'a str {
        if self.trim_whitespace {
            text.trim_matches(is_sgml_whitespace)
        } else {
            text
        }
    }

    pub fn parse_rcdata<'a>(&self, rcdata: &'a str) -> crate::Result<Data<'a>> {
        let f = self.entity_fn.as_deref().unwrap_or(&|_| None);
        entities::expand_entities(rcdata, f)
            .map(Data::CData)
            .map_err(From::from)
    }

    pub fn parse_markup_declaration_text<'a>(
        &self,
        text: &'a str,
    ) -> entities::Result<Cow<'a, str>> {
        let f = self.parameter_entity_fn.as_deref().unwrap_or(&|_| None);
        entities::expand_parameter_entities(text, f).map_err(From::from)
    }
}

impl Default for ParserConfig {
    /// Creates a new, default `ParserConfig`. See [`ParserConfig::new`] for the default settings.
    fn default() -> Self {
        ParserConfig::new()
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

#[derive(Default, Debug)]
pub struct ParserConfigBuilder {
    config: ParserConfig,
}

/// A builder for parser configurations.
impl ParserConfigBuilder {
    pub fn new() -> Self {
        Default::default()
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

    /// Defines a closure to be used to resolve entities
    pub fn expand_parameter_entities<F, T>(mut self, f: F) -> Self
    where
        F: Fn(&str) -> Option<T> + 'static,
        T: Into<Cow<'static, str>>,
    {
        self.config.parameter_entity_fn = Some(Box::new(move |entity| f(entity).map(Into::into)));
        self
    }

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

    pub fn trim_whitespace(mut self, trim_whitespace: bool) -> Self {
        self.config.trim_whitespace = trim_whitespace;
        self
    }

    /// Returns a [`ParserConfig`] with the configuration that was built using other methods.
    pub fn build(self) -> ParserConfig {
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
