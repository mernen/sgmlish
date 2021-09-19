//! Deserialize SGML data to a Rust data structure.

use std::borrow::Cow;
use std::rc::Rc;
use std::{fmt, mem};

use log::{debug, trace};
use serde::de::{self, IntoDeserializer, Unexpected};
use serde::Deserializer;

use crate::de::buffer::CowBuffer;
use crate::{SgmlEvent, SgmlFragment};

mod buffer;

/// Deserializes an instance of type `T` from the given [`SgmlFragment`].
///
/// Before invoking, make sure the content is *tag-valid* and consistently cased.
/// That means all start tags must have a matching end tag with identical case,
/// in a consistent hierarchy.
///
/// # Example
///
/// ```rust
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize)]
/// struct Select {
///     name: Option<String>,
///     #[serde(rename = "option")]
///     options: Vec<SelectOption>,
/// }
///
/// #[derive(Debug, Deserialize)]
/// struct SelectOption {
///     #[serde(rename = "$value")]
///     label: String,
///     value: Option<String>,
///     #[serde(default)]
///     selected: bool,
/// }
///
/// # fn main() -> sgmlish::Result<()> {
/// let sgml = r##"
///     <SELECT NAME="color">
///         <OPTION VALUE="">Choose one
///         <OPTION SELECTED>Red
///         <OPTION>Green
///         <OPTION>Blue
///     </SELECT>
/// "##;
/// let sgml = sgmlish::Parser::builder()
///     .lowercase_names()
///     .parse(sgml)?;
/// let sgml = sgmlish::transforms::normalize_end_tags(sgml)?;
/// let select = sgmlish::from_fragment::<Select>(sgml)?;
///
/// println!("Deserialized:\n{:#?}", select);
///
/// assert_eq!(select.name.as_deref(), Some("color"));
/// assert_eq!(select.options.len(), 4);
/// assert_eq!(select.options[0].label, "Choose one");
/// assert!(!select.options[0].selected);
/// assert_eq!(select.options[1].label, "Red");
/// assert!(select.options[1].selected);
/// # Ok(())
/// # }
/// ```
pub fn from_fragment<'de, T>(fragment: SgmlFragment<'de>) -> Result<T, DeserializationError>
where
    T: de::Deserialize<'de>,
{
    let mut reader = SgmlDeserializer::from_fragment(fragment)?;
    T::deserialize(&mut reader)
}

/// A deserializer for SGML content.
#[derive(Debug)]
pub struct SgmlDeserializer<'de> {
    events: std::vec::IntoIter<SgmlEvent<'de>>,
    stack: Vec<Cow<'de, str>>,
    map_key: Option<Rc<str>>,
    accumulated_text: Option<Cow<'de, str>>,
}

/// The error type for deserialization problems.
#[derive(Debug, thiserror::Error)]
pub enum DeserializationError {
    #[error("unexpected end of content")]
    UnexpectedEof,
    #[error("empty stack")]
    EmptyStack,
    #[error("expected start tag")]
    ExpectedStartTag,
    #[error("mismatched close tag: expected </{expected}>, found </{found}>")]
    MismatchedCloseTag { expected: String, found: String },
    /// An event which is not supported by deserialization was found.
    #[error("deserialization of '{0}' is not supported")]
    Unsupported(SgmlEvent<'static>),

    #[error("error parsing integer value: {source}")]
    ParseIntError {
        #[from]
        source: std::num::ParseIntError,
    },
    #[error("error parsing float value: {source}")]
    ParseFloatError {
        #[from]
        source: std::num::ParseFloatError,
    },

    #[error("{0}")]
    Message(String),
}

impl<'de> SgmlDeserializer<'de> {
    pub fn from_fragment(fragment: SgmlFragment<'de>) -> Result<Self, DeserializationError> {
        let mut reader = SgmlDeserializer {
            events: fragment.into_vec().into_iter(),
            stack: Vec::new(),
            map_key: None,
            accumulated_text: None,
        };
        reader.normalize_at_cursor()?;
        Ok(reader)
    }

    fn advance(&mut self) -> Result<SgmlEvent<'de>, DeserializationError> {
        if let Some(next) = self.events.next() {
            self.normalize_at_cursor()?;
            Ok(next)
        } else {
            Err(DeserializationError::UnexpectedEof)
        }
    }

    fn peek(&self) -> Result<&SgmlEvent<'de>, DeserializationError> {
        let current = self
            .events
            .as_slice()
            .get(0)
            .ok_or(DeserializationError::UnexpectedEof)?;
        trace!("peeked: {:?}", current);
        Ok(current)
    }

    fn peek_mut(&mut self) -> Result<&mut SgmlEvent<'de>, DeserializationError> {
        let current = self
            .events
            .as_mut_slice()
            .get_mut(0)
            .ok_or(DeserializationError::UnexpectedEof)?;
        trace!("peeked: {:?}", current);
        Ok(current)
    }

    fn peek_content_type(&self) -> Result<PeekContentType, DeserializationError> {
        let mut contains_text = false;
        let contains_child_elements = self
            .events
            .as_slice()
            .iter()
            .skip(1)
            .find_map(|event| match event {
                SgmlEvent::OpenStartTag { .. } => Some(true),
                SgmlEvent::EndTag { .. } => Some(false),
                SgmlEvent::Character(text) if !text.is_empty() => {
                    contains_text = true;
                    None
                }
                _ => None,
            })
            .ok_or(DeserializationError::UnexpectedEof)?;

        let content = PeekContentType {
            contains_child_elements,
            contains_text,
        };
        trace!("peeked content type: {:?}", content);
        Ok(content)
    }

    /// Rejects unsupported events (like empty start tags), ignores markup declarations and processing instructions,
    /// and ensures any `Data` is expanded
    fn normalize_at_cursor(&mut self) -> Result<(), DeserializationError> {
        let event = match self.events.as_mut_slice().get_mut(0) {
            Some(event) => event,
            None => return Ok(()),
        };
        match event {
            SgmlEvent::MarkupDeclaration { .. }
            | SgmlEvent::ProcessingInstruction(_)
            | SgmlEvent::MarkedSection { .. } => {}
            SgmlEvent::OpenStartTag { name } | SgmlEvent::EndTag { name } if name.is_empty() => {}
            _ => return Ok(()),
        }
        Err(DeserializationError::Unsupported(
            self.events.next().unwrap().into_owned(),
        ))
    }

    fn expect_start_tag(&self) -> Result<&Cow<'de, str>, DeserializationError> {
        match self.peek() {
            Ok(SgmlEvent::OpenStartTag { name }) => Ok(name),
            _ => Err(DeserializationError::ExpectedStartTag),
        }
    }

    /// Consumes the current event, asserting it is an open tag, and pushes it to the stack.
    fn push_elt(&mut self) -> Result<&str, DeserializationError> {
        let stag = match self.events.next() {
            Some(SgmlEvent::OpenStartTag { name }) => name,
            _ => return Err(DeserializationError::ExpectedStartTag),
        };
        debug!("push({}): {:?}", self.stack.len(), stag);
        self.stack.push(stag);
        self.normalize_at_cursor()?;
        Ok(self.stack.last().unwrap())
    }

    /// Consumes all events until the current top of the stack is popped.
    fn pop_elt(&mut self) -> Result<(), DeserializationError> {
        let stack_size = self.stack.len();
        trace!(
            "popping({}): {:?}",
            stack_size - 1,
            &self.stack[stack_size - 1]
        );
        loop {
            match self
                .events
                .next()
                .ok_or(DeserializationError::UnexpectedEof)?
            {
                SgmlEvent::XmlCloseEmptyElement => {
                    self.stack.pop();
                    return Ok(());
                }
                SgmlEvent::EndTag { name } => {
                    self.check_stack_size(stack_size);
                    let expected = self.stack.pop().unwrap();
                    if name != expected {
                        return Err(DeserializationError::MismatchedCloseTag {
                            expected: expected.to_string(),
                            found: name.to_string(),
                        });
                    }
                    debug!("popped({}): {:?}", stack_size, name);
                    return Ok(());
                }
                SgmlEvent::OpenStartTag { name } => {
                    self.stack.push(name);
                    self.pop_elt()?;
                }
                _ => {}
            };
        }
    }

    /// Skips attributes and CloseStartTag, going to the main content.
    ///
    /// Should only be used immediately after `push_elt`.
    fn advance_to_content(&mut self) -> Result<(), DeserializationError> {
        while let SgmlEvent::Attribute { .. } | SgmlEvent::CloseStartTag = self.peek()? {
            self.advance()?;
        }
        Ok(())
    }

    /// Consumes an element and returns all its text.
    ///
    /// Includes text from child elements as well.
    fn consume_text<'r, V: de::Visitor<'r>>(
        &mut self,
    ) -> Result<Cow<'de, str>, DeserializationError> {
        if let Some(accumulated_text) = self.accumulated_text.take() {
            debug!("consume_text accumulated");
            return Ok(accumulated_text);
        }

        debug!("consume_text");
        if let SgmlEvent::Attribute { name, value } = self.peek_mut()? {
            let value = mem::take(value);
            debug!("consumed text from attribute({}): {:?}", name, value);
            self.advance()?;
            return Ok(value.unwrap_or_default());
        }

        let starting_stack_size = self.stack.len();
        self.push_elt()?;

        let mut text = CowBuffer::new();

        loop {
            match self.peek_mut()? {
                SgmlEvent::OpenStartTag { .. } => {
                    self.push_elt()?;
                }
                SgmlEvent::EndTag { .. } => {
                    self.pop_elt()?;
                    if self.stack.len() == starting_stack_size {
                        break;
                    }
                }
                SgmlEvent::Character(t) => {
                    text.push_cow(mem::take(t));
                    self.advance()?;
                }
                _ => {
                    self.advance()?;
                }
            }
        }

        debug!("consumed text content: {:?}", text.as_str());
        Ok(text.into_cow())
    }

    fn do_map<'r, V>(
        &'r mut self,
        visitor: V,
        emit_value: bool,
    ) -> Result<V::Value, DeserializationError>
    where
        V: de::Visitor<'de>,
    {
        self.push_elt()?;
        let stack_size = self.stack.len();
        let value = visitor.visit_map(MapAccess::new(self, emit_value))?;
        self.check_stack_size(stack_size);
        self.pop_elt()?;

        Ok(value)
    }

    #[track_caller]
    fn check_stack_size(&self, expected_size: usize) {
        let stack = &self.stack;

        debug_assert_eq!(
            expected_size,
            stack.len(),
            "unstable stack: {action} {delta:?}",
            action = if stack.len() > expected_size {
                "added"
            } else {
                "removed"
            },
            delta = stack.iter().skip(expected_size).collect::<Vec<_>>(),
        );
    }
}

macro_rules! forward_parse {
    ($deserialize:ident => $visit:ident) => {
        fn $deserialize<V>(self, visitor: V) -> Result<V::Value, DeserializationError>
        where
            V: de::Visitor<'de>,
        {
            trace!(stringify!($deserialize));
            let value = self.consume_text::<V>()?.parse()?;
            visitor.$visit(value)
        }
    };
}

impl<'de, 'r> Deserializer<'de> for &'r mut SgmlDeserializer<'de> {
    type Error = DeserializationError;

    forward_parse!(deserialize_i8 => visit_i8);
    forward_parse!(deserialize_i16 => visit_i16);
    forward_parse!(deserialize_i32 => visit_i32);
    forward_parse!(deserialize_i64 => visit_i64);
    forward_parse!(deserialize_u8 => visit_u8);
    forward_parse!(deserialize_u16 => visit_u16);
    forward_parse!(deserialize_u32 => visit_u32);
    forward_parse!(deserialize_u64 => visit_u64);
    forward_parse!(deserialize_f32 => visit_f32);
    forward_parse!(deserialize_f64 => visit_f64);

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_bool");

        if let SgmlEvent::Attribute { name, value } = self.peek()? {
            // Treat empty values and repetitions of the key as true values
            let value = value.as_deref().unwrap_or_default();
            if value.is_empty() || value.eq_ignore_ascii_case(name) {
                self.advance()?;
                return visitor.visit_bool(true);
            }
        }

        let str = self.consume_text::<V>()?;
        if str == "1" || str.eq_ignore_ascii_case("true") {
            visitor.visit_bool(true)
        } else if str == "0" || str.eq_ignore_ascii_case("false") {
            visitor.visit_bool(false)
        } else {
            Err(de::Error::invalid_value(
                Unexpected::Str(&str),
                &"a boolean",
            ))
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_str");
        match self.consume_text::<V>()? {
            Cow::Borrowed(s) => visitor.visit_borrowed_str(s),
            Cow::Owned(s) => visitor.visit_string(s),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_string -> str");
        self.deserialize_str(visitor)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_char -> str");
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_bytes -> str");
        self.deserialize_str(visitor)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_byte_buf -> str");
        self.deserialize_str(visitor)
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_identifier -> str");
        self.deserialize_str(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        if self.accumulated_text.take().is_some() {
            trace!("deserialize_unit -> accumulated text");
            return visitor.visit_unit();
        }

        trace!("deserialize_unit");
        match self.peek()? {
            SgmlEvent::OpenStartTag { .. } => {
                self.push_elt()?;
                self.pop_elt()?;
                visitor.visit_unit()
            }
            SgmlEvent::Attribute { .. } => {
                self.advance()?;
                visitor.visit_unit()
            }
            _ => self.deserialize_any(visitor),
        }
    }

    fn deserialize_unit_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_unit_struct ({}) -> unit", name);
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_newtype_struct ({})", name);
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_seq (tag: {:?})", self.map_key);
        let stack_size = self.stack.len();

        let tag_name = self.map_key.take().map(Into::into);
        let value = visitor.visit_seq(SeqAccess::new(self, tag_name))?;

        self.check_stack_size(stack_size);

        Ok(value)
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_tuple ({} items) -> seq", len);
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_tuple_struct({}, {} items) -> seq", name, len);
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_map");
        self.do_map(visitor, false)
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_struct({}) -> map", name);
        self.do_map(visitor, fields.contains(&"$value"))
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_enum({})", name);

        let stack_size = self.stack.len();

        // If true, we have a <map-key>(enum-value)</map-key> case;
        // if false, it's (enum-value) directly
        let enum_within_element = self
            .map_key
            .as_deref()
            .and_then(|map_key| {
                self.expect_start_tag()
                    .ok()
                    .map(|start_tag| start_tag == map_key)
            })
            .unwrap_or(false);

        // If true, (enum-value) is <variant (fields)>(fields)</variant>;
        // if false, (enum-value) is just a string
        let use_tag_name_for_variant = if enum_within_element {
            if self.peek_content_type()?.contains_child_elements {
                trace!("enum within element; using content elt");
                // <key><variant (fields)>(fields)</variant></key>
                // Advance cursor to `<variant`
                self.push_elt()?;
                self.advance_to_content()?;
                true
            } else {
                trace!("enum within element; using text content");
                // <key>variant</key>
                // Keep cursor on `<key`
                false
            }
        } else {
            // No surrounding element, so it must be <variant (fields)>(fields)</variant>
            // Keep cursor on `<variant`
            true
        };

        let value = visitor.visit_enum(EnumAccess::new(self, use_tag_name_for_variant))?;
        if enum_within_element && use_tag_name_for_variant {
            self.pop_elt()?;
        }

        self.check_stack_size(stack_size);
        Ok(value)
    }

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_any");

        if self.accumulated_text.is_some() {
            return self.deserialize_str(visitor);
        }
        match self.peek()? {
            SgmlEvent::OpenStartTag { .. } => {
                let content = self.peek_content_type()?;
                if content.contains_child_elements {
                    self.deserialize_map(visitor)
                } else if content.contains_text {
                    self.deserialize_str(visitor)
                } else {
                    self.deserialize_unit(visitor)
                }
            }
            SgmlEvent::Attribute { .. } => self.deserialize_str(visitor),
            _ => Err(DeserializationError::ExpectedStartTag),
        }
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("deserialize_ignored_any -> unit");
        self.deserialize_unit(visitor)
    }
}

impl de::Error for DeserializationError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        DeserializationError::Message(msg.to_string())
    }
}

struct MapAccess<'de, 'r> {
    de: &'r mut SgmlDeserializer<'de>,
    stack_size: usize,
    map_key: Option<Rc<str>>,
    content_strategy: ContentStrategy,
    text_content: Option<CowBuffer<'de>>,
    next_entry_is_dollarvalue: bool,
}

impl<'de, 'r> MapAccess<'de, 'r> {
    fn new(de: &'r mut SgmlDeserializer<'de>, emit_value: bool) -> Self {
        let stack_size = de.stack.len();
        let content_strategy = if emit_value {
            if de
                .peek_content_type()
                .map(|content| content.contains_child_elements)
                .unwrap_or(false)
            {
                ContentStrategy::ElementsAreDollarValue
            } else {
                ContentStrategy::TextOnly
            }
        } else {
            ContentStrategy::ElementsAreMapEntries
        };
        Self {
            de,
            stack_size,
            map_key: None,
            content_strategy,
            text_content: (content_strategy == ContentStrategy::TextOnly).then(CowBuffer::new),
            next_entry_is_dollarvalue: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ContentStrategy {
    /// Element only contains text
    TextOnly,
    /// Treat element content as map entries
    ElementsAreMapEntries,
    /// Treat element content as the value for key `$value`
    ElementsAreDollarValue,
}

impl<'de, 'r> de::MapAccess<'de> for MapAccess<'de, 'r> {
    type Error = DeserializationError;

    fn next_key_seed<K: de::DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        trace!("next_key_seed");
        self.de.check_stack_size(self.stack_size);

        loop {
            break match self.de.peek_mut()? {
                SgmlEvent::EndTag { .. } | SgmlEvent::XmlCloseEmptyElement => {
                    if self.text_content.is_some() {
                        self.next_entry_is_dollarvalue = true;
                        debug!("next key: $value");
                        self.map_key = Some("$value".into());
                        seed.deserialize("$value".into_deserializer()).map(Some)
                    } else {
                        Ok(None)
                    }
                }
                SgmlEvent::Attribute { name, .. } => {
                    debug!("next key: {} (from attribute)", name);
                    seed.deserialize(name.as_ref().into_deserializer())
                        .map(Some)
                }
                SgmlEvent::CloseStartTag => {
                    self.de.advance()?;
                    continue;
                }
                SgmlEvent::OpenStartTag { name } => match self.content_strategy {
                    ContentStrategy::ElementsAreMapEntries => {
                        debug!("next key: {} (from tag name)", name);
                        self.map_key = Some(name.clone().into_owned().into());
                        seed.deserialize(name.as_ref().into_deserializer())
                            .map(Some)
                    }
                    ContentStrategy::ElementsAreDollarValue => {
                        debug!("next key: $value (for element {:?})", name);
                        seed.deserialize("$value".into_deserializer()).map(Some)
                    }
                    ContentStrategy::TextOnly => unreachable!(),
                },
                SgmlEvent::Character(text) => {
                    let text = mem::take(text);
                    self.de.advance()?;
                    if let Some(value_acc) = &mut self.text_content {
                        value_acc.push_cow(text);
                    }
                    continue;
                }
                SgmlEvent::ProcessingInstruction(_)
                | SgmlEvent::MarkupDeclaration { .. }
                | SgmlEvent::MarkedSection { .. } => unreachable!(),
            };
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        trace!("next_value_seed (key={:?})", self.map_key);
        self.de.check_stack_size(self.stack_size);

        if self.next_entry_is_dollarvalue {
            self.de.accumulated_text = Some(self.text_content.take().unwrap().into_cow());
            let value = seed.deserialize(&mut *self.de)?;
            self.de.accumulated_text = None;
            Ok(value)
        } else if let Ok(SgmlEvent::Attribute { .. }) = self.de.peek() {
            seed.deserialize(&mut *self.de)
        } else {
            self.de.map_key = self.map_key.take();
            let value = seed.deserialize(&mut *self.de)?;
            self.de.map_key = None;
            Ok(value)
        }
    }
}

struct SeqAccess<'de, 'r> {
    de: &'r mut SgmlDeserializer<'de>,
    stack_size: usize,
    tag_name: Option<Rc<str>>,
}

impl<'de, 'r> SeqAccess<'de, 'r> {
    fn new(de: &'r mut SgmlDeserializer<'de>, tag_name: Option<Rc<str>>) -> Self {
        let stack_size = de.stack.len();
        Self {
            de,
            stack_size,
            tag_name,
        }
    }
}

impl<'de, 'r> de::SeqAccess<'de> for SeqAccess<'de, 'r> {
    type Error = DeserializationError;

    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Self::Error> {
        self.de.check_stack_size(self.stack_size);

        loop {
            match self.de.peek()? {
                SgmlEvent::OpenStartTag { name } => match &self.tag_name {
                    Some(expected_tag) if name.as_ref() != expected_tag.as_ref() => {
                        return Ok(None)
                    }
                    _ => {
                        if self.de.map_key != self.tag_name {
                            self.de.map_key = self.tag_name.clone();
                        }
                        return Ok(Some(seed.deserialize(&mut *self.de)?));
                    }
                },
                SgmlEvent::Character(text) if text.is_empty() => self.de.advance()?,
                _ => return Ok(None),
            };
        }
    }
}

struct EnumAccess<'de, 'r> {
    de: &'r mut SgmlDeserializer<'de>,
    use_tag_name_for_variant: bool,
}

impl<'de, 'r> EnumAccess<'de, 'r> {
    fn new(de: &'r mut SgmlDeserializer<'de>, use_tag_name_for_variant: bool) -> Self {
        Self {
            de,
            use_tag_name_for_variant,
        }
    }
}

impl<'de, 'r> de::EnumAccess<'de> for EnumAccess<'de, 'r> {
    type Error = DeserializationError;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), DeserializationError>
    where
        V: de::DeserializeSeed<'de>,
    {
        trace!("variant_seed");
        let name = if self.use_tag_name_for_variant {
            debug!("using tag name for enum variant");
            let name = self.de.expect_start_tag()?.as_ref();
            seed.deserialize(name.into_deserializer())
        } else {
            debug!("using text content for enum variant");
            seed.deserialize(&mut *self.de)
        }?;
        Ok((name, self))
    }
}

impl<'de, 'r> de::VariantAccess<'de> for EnumAccess<'de, 'r> {
    type Error = DeserializationError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        trace!("unit_variant");
        if self.use_tag_name_for_variant {
            self.de.push_elt()?;
            self.de.pop_elt()?;
        }
        Ok(())
    }

    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<T::Value, Self::Error> {
        trace!("newtype_variant");
        seed.deserialize(self.de)
    }

    fn tuple_variant<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("tuple_variant({} items)", len);
        if self.use_tag_name_for_variant {
            self.de.map_key = Some(self.de.expect_start_tag()?.clone().into_owned().into());
        }
        self.de.deserialize_seq(visitor)
    }

    fn struct_variant<V>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        trace!("struct_variant");
        self.de.do_map(visitor, fields.contains(&"$value"))
    }
}

#[derive(Debug)]
struct PeekContentType {
    contains_text: bool,
    contains_child_elements: bool,
}
