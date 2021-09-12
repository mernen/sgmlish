use std::borrow::Cow;
use std::fmt::{self, Write};
use std::mem;

use never::Never;

use crate::transforms::MapDataResult;
use crate::{entities, transforms, Data, SgmlEvent};

/// A list of events from a parsed SGML document.
///
/// An event represents a certain sequence of tokens in the SGML file, like the
/// opening of a tag, or a piece of text data.
///
/// Working directly with events is not very practical; they are mainly meant
/// for applying transforms before being used for deserialization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SgmlFragment<'a> {
    events: Vec<SgmlEvent<'a>>,
}

impl<'a> SgmlFragment<'a> {
    /// Returns the number of events in the list.
    // `is_empty()` makes no sense here, since we don't expect empty event lists
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Views the event list as a slice of events.
    pub fn as_slice(&self) -> &[SgmlEvent<'a>] {
        &self.events
    }

    /// Converts the event list into a [`Vec`] of events.
    pub fn into_vec(self) -> Vec<SgmlEvent<'a>> {
        self.events
    }

    /// Returns an iterator over references to events.
    pub fn iter(&self) -> std::slice::Iter<SgmlEvent<'a>> {
        self.events.iter()
    }

    /// Returns an iterator over mutable references to events.
    pub fn iter_mut(&mut self) -> std::slice::IterMut<SgmlEvent<'a>> {
        self.events.iter_mut()
    }

    /// Detaches the event list from the source string, taking ownership of all substrings.
    pub fn into_owned(self) -> SgmlFragment<'static> {
        self.into_iter()
            .map(|event| event.into_owned())
            .collect::<Vec<_>>()
            .into()
    }

    /// Deserializes using [`serde`]. This method requires the `deserialize` feature.
    ///
    /// This is a convenience method for [`from_fragment`](crate::de::from_fragment).
    #[cfg(feature = "deserialize")]
    pub fn deserialize<T>(self) -> Result<T, crate::de::DeserializationError>
    where
        T: serde::Deserialize<'a>,
    {
        crate::de::from_fragment(self)
    }

    /// Removes leading and trailing spaces from the data events.
    /// Empty events are then removed.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), sgmlish::Error> {
    /// use sgmlish::RcData;
    /// use sgmlish::SgmlEvent::*;
    ///
    /// let indented = sgmlish::parse(r##"
    ///     <HTML>
    ///         <BODY>
    ///             <P CLASS=" intro ">
    ///                 Hello, world!
    ///             </P>
    ///         </BODY>
    ///     </HTML>
    /// "##)?;
    /// let unindented = sgmlish::parse(r##"<HTML><BODY><P CLASS=" intro ">Hello, world!</P></BODY></HTML>"##)?;
    ///
    /// assert_eq!(indented.trim_spaces(), unindented);
    /// # Ok(())
    /// # }
    /// ```
    pub fn trim_spaces(self) -> Self {
        self.map_data(|data, key| {
            if key.is_some() {
                // Attribute values are not trimmed
                Some(data)
            } else {
                let trimmed = data.trim();
                if trimmed.as_str().is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }
        })
    }

    /// Inserts omitted end tags, assuming they are only implied for text-only content.
    ///
    /// This is good enough for certain formats, like [OFX] 1.x, but not for others, e.g. [HTML].
    ///
    /// # Notes
    ///
    /// * Tag names are compared in a case-sensitive manner; if your data may mix cases,
    ///   you can apply [`lowercase_identifiers`] or [`uppercase_identifiers`] beforehand.
    /// * This transforms does not support empty start tags (`<>`) or empty end tags (`</>`).
    ///
    /// # Example
    ///
    /// Taking a fragment of (valid) OFX and inserting implied end tags:
    ///
    /// ```rust
    /// # fn main() -> Result<(), sgmlish::Error> {
    /// let end_tags_implied = sgmlish::parse(r##"
    ///     <BANKTRANLIST>
    ///         <DTSTART>20210101000000[-4:GMT]
    ///         <DTEND>20210201000000[-4:GMT]
    ///         <STMTTRN>
    ///             <TRNTYPE>DEBIT
    ///             <DTPOSTED>20210114000000[-4:GMT]
    ///             <TRNAMT>-12.34
    ///             <FITID>F1910527-5589-4110-B55F-D257F92645B8
    ///             <MEMO>Example
    ///         </STMTTRN>
    ///     </BANKTRANLIST>
    /// "##)?.trim_spaces();
    ///
    /// let normalized = sgmlish::parse(r##"
    ///     <BANKTRANLIST>
    ///         <DTSTART>20210101000000[-4:GMT]</DTSTART>
    ///         <DTEND>20210201000000[-4:GMT]</DTEND>
    ///         <STMTTRN>
    ///             <TRNTYPE>DEBIT</TRNTYPE>
    ///             <DTPOSTED>20210114000000[-4:GMT]</DTPOSTED>
    ///             <TRNAMT>-12.34</TRNAMT>
    ///             <FITID>F1910527-5589-4110-B55F-D257F92645B8</FITID>
    ///             <MEMO>Example</MEMO>
    ///         </STMTTRN>
    ///     </BANKTRANLIST>
    /// "##)?.trim_spaces();
    ///
    /// assert_eq!(end_tags_implied.normalize_end_tags()?, normalized);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [OFX]: https://en.wikipedia.org/wiki/Open_Financial_Exchange
    /// [HTML]: https://en.wikipedia.org/wiki/HTML
    /// [`lowercase_identifiers`]: SgmlFragment::lowercase_identifiers
    /// [`uppercase_identifiers`]: SgmlFragment::uppercase_identifiers
    pub fn normalize_end_tags(self) -> Result<Self, transforms::NormalizationError> {
        transforms::normalize_end_tags(self)
    }

    pub fn expand_marked_sections(self) -> Result<Self, crate::Error> {
        transforms::expand_marked_sections(self)
    }

    /// Calls a closure on every identifier (tag name and attribute key),
    /// returning a new `SgmlFragment` with the returned replacements.
    pub fn map_identifiers<F, R>(mut self, mut f: F) -> Self
    where
        F: FnMut(Cow<'a, str>) -> R,
        R: Into<Cow<'a, str>>,
    {
        let mut transform = |slot: &mut Cow<'a, str>| {
            let id = mem::take(slot);
            *slot = f(id).into();
        };

        for event in &mut self {
            match event {
                SgmlEvent::OpenStartTag(name) => transform(name),
                SgmlEvent::EndTag(name) => {
                    if !name.is_empty() {
                        transform(name);
                    }
                }
                SgmlEvent::Attribute(key, _) => transform(key),
                _ => {}
            }
        }
        self
    }

    /// Normalizes all identifiers (tag names and attribute keys) as ASCII lowercase.
    /// [`Data`], markup declarations and processing instructions are not affected.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), sgmlish::Error> {
    /// let case_insensitive = sgmlish::parse(r##"
    ///     <!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd">
    ///     <Html>
    ///         <body>
    ///             <IMG src="underconstruction.gif">
    ///         </body>
    ///     </html>
    /// "##)?;
    ///
    /// let normalized = sgmlish::parse(r##"
    ///     <!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd">
    ///     <html>
    ///         <body>
    ///             <img src="underconstruction.gif">
    ///         </body>
    ///     </html>
    /// "##)?;
    ///
    /// assert_eq!(case_insensitive.lowercase_identifiers(), normalized);
    /// # Ok(())
    /// # }
    /// ```
    pub fn lowercase_identifiers(self) -> Self {
        self.map_identifiers(|mut name| {
            name.to_mut().make_ascii_lowercase();
            name
        })
    }

    /// Normalizes all identifiers (tag names and attribute keys) as ASCII uppercase.
    /// [`Data`], markup declarations and processing instructions are not affected.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), sgmlish::Error> {
    /// let case_insensitive = sgmlish::parse(r##"
    ///     <!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd">
    ///     <Html>
    ///         <body>
    ///             <IMG src="underconstruction.gif">
    ///         </body>
    ///     </html>
    /// "##)?;
    ///
    /// let normalized = sgmlish::parse(r##"
    ///     <!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd">
    ///     <HTML>
    ///         <BODY>
    ///             <IMG SRC="underconstruction.gif">
    ///         </BODY>
    ///     </HTML>
    /// "##)?;
    ///
    /// assert_eq!(case_insensitive.uppercase_identifiers(), normalized);
    /// # Ok(())
    /// # }
    /// ```
    pub fn uppercase_identifiers(self) -> Self {
        self.map_identifiers(|mut name| {
            name.to_mut().make_ascii_uppercase();
            name
        })
    }

    /// Calls a closure on every fragment of character data (element content and attribute value),
    /// returning a new `SgmlFragment` with the returned replacements.
    ///
    /// The closure receives two parameters: the data fragment,
    /// and the attribute name (or `None` in case of element content).
    /// The return value of the closure can be either [`Data`] or `Option<Data>`,
    /// in case filtering is desired.
    ///
    /// For a version where the closure may return an error, see [`try_map_data`].
    ///
    /// [`try_map_data`]: SgmlFragment::try_map_data
    pub fn map_data<F, R>(self, f: F) -> Self
    where
        F: FnMut(Data<'a>, Option<&str>) -> R,
        R: Into<MapDataResult<'a, Never>>,
    {
        transforms::try_map_data(self, f).unwrap_or_else(|_| unreachable!())
    }

    /// Calls a closure on every fragment of character data (element content and attribute value),
    /// returning a new [`SgmlFragment`] with the returned replacements.
    ///
    /// The closure receives two parameters: the data fragment,
    /// and the attribute name (or `None` in case of element content).
    /// The return value of the closure can be either `Result<Data, E>`
    /// or `Result<Option<Data>, E>`, in case filtering is desired.
    ///
    /// For a version where error results are not needed, see [`map_data`].
    ///
    /// [`map_data`]: SgmlFragment::map_data
    pub fn try_map_data<F, R, E>(self, f: F) -> Result<Self, E>
    where
        F: FnMut(Data<'a>, Option<&str>) -> R,
        R: Into<MapDataResult<'a, E>>,
    {
        transforms::try_map_data(self, f)
    }

    /// Expands character references (`&#123;`) in the content.
    /// Any entity references are treated as errors.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use sgmlish::{CData, RcData, SgmlEvent};
    /// # fn main() -> Result<(), sgmlish::Error> {
    /// let sgml = sgmlish::parse(
    ///     "\
    ///     <example>\
    ///         This is an &#60;example&#62; element\
    ///     </example>\
    ///     ",
    /// )?;
    /// // We start with unexpanded replaceable character data (RcData)
    /// assert_eq!(
    ///     sgml.as_slice()[2],
    ///     SgmlEvent::Data(RcData("This is an &#60;example&#62; element".into()))
    /// );
    ///
    /// let expanded = sgml.expand_character_references()?;
    /// // The expanded form is now character data (CData)
    /// assert_eq!(
    ///     expanded.as_slice()[2],
    ///     SgmlEvent::Data(CData("This is an <example> element".into()))
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn expand_character_references(self) -> entities::Result<Self> {
        self.try_map_data(|text, _key| text.expand_character_references())
    }

    /// Expands entity references (`&foo;`) as well as character references (`&#123;`)
    /// in the content.
    ///
    /// The given closure is called to expand each entity found. Fails if the closure returns `None`.
    /// Character references (`&#123;`) are expanded without going through the closure.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::collections::HashMap;
    /// # use sgmlish::{CData, RcData, SgmlEvent};
    /// # fn main() -> Result<(), sgmlish::Error> {
    /// let mut entities = HashMap::new();
    /// entities.insert("lt", "<");
    /// entities.insert("gt", ">");
    /// entities.insert("amp", "&");
    ///
    /// let sgml = sgmlish::parse(
    ///     "\
    ///     <example>\
    ///         This is an &lt;example&gt; element\
    ///     </example>\
    ///     ",
    /// )?;
    /// // We start with unexpanded replaceable character data (RcData)
    /// assert_eq!(
    ///     sgml.as_slice()[2],
    ///     SgmlEvent::Data(RcData("This is an &lt;example&gt; element".into()))
    /// );
    ///
    /// let expanded = sgml.expand_entities(|entity| entities.get(entity))?;
    /// // The expanded form is now character data (CData)
    /// assert_eq!(
    ///     expanded.as_slice()[2],
    ///     SgmlEvent::Data(CData("This is an <example> element".into()))
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn expand_entities<F, T>(self, mut f: F) -> entities::Result<Self>
    where
        F: FnMut(&str) -> Option<T>,
        T: AsRef<str>,
    {
        self.try_map_data(|text, _key| text.expand_entities(&mut f))
    }
}

impl<'a> From<Vec<SgmlEvent<'a>>> for SgmlFragment<'a> {
    fn from(events: Vec<SgmlEvent<'a>>) -> Self {
        SgmlFragment { events }
    }
}

impl<'a> IntoIterator for SgmlFragment<'a> {
    type Item = SgmlEvent<'a>;

    type IntoIter = std::vec::IntoIter<SgmlEvent<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.events.into_iter()
    }
}

impl<'a, 'b> IntoIterator for &'b SgmlFragment<'a> {
    type Item = &'b SgmlEvent<'a>;

    type IntoIter = std::slice::Iter<'b, SgmlEvent<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, 'b> IntoIterator for &'b mut SgmlFragment<'a> {
    type Item = &'b mut SgmlEvent<'a>;

    type IntoIter = std::slice::IterMut<'b, SgmlEvent<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl fmt::Display for SgmlFragment<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.events.iter().try_for_each(|event| match event {
            SgmlEvent::Attribute(..) => {
                f.write_char(' ')?;
                fmt::Display::fmt(event, f)
            }
            event => fmt::Display::fmt(event, f),
        })
    }
}
