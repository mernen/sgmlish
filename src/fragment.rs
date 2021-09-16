use std::fmt::{self, Write};

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
    // `is_empty()` makes no sense here, since we don't expect empty fragments
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Views the fragment as a slice of events.
    pub fn as_slice(&self) -> &[SgmlEvent<'a>] {
        &self.events
    }

    /// Converts the fragment into a [`Vec`] of events.
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

    /// Detaches the fragment from the source string, taking ownership of all substrings.
    pub fn into_owned(self) -> SgmlFragment<'static> {
        self.into_iter()
            .map(|event| event.into_owned())
            .collect::<Vec<_>>()
            .into()
    }

    /// Deserializes using [`serde`]. This method requires the `serde` feature.
    ///
    /// This is a convenience method for [`from_fragment`](crate::de::from_fragment).
    #[cfg(feature = "serde")]
    pub fn deserialize<T>(self) -> Result<T, crate::de::DeserializationError>
    where
        T: serde::Deserialize<'a>,
    {
        crate::de::from_fragment(self)
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
    /// ```rust,no_run
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
    ///     SgmlEvent::Character(RcData("This is an &#60;example&#62; element".into()))
    /// );
    ///
    /// let expanded = sgml.expand_character_references()?;
    /// // The expanded form is now character data (CData)
    /// assert_eq!(
    ///     expanded.as_slice()[2],
    ///     SgmlEvent::Character(CData("This is an <example> element".into()))
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
    /// ```rust,no_run
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
    ///     SgmlEvent::Character(RcData("This is an &lt;example&gt; element".into()))
    /// );
    ///
    /// let expanded = sgml.expand_entities(|entity| entities.get(entity))?;
    /// // The expanded form is now character data (CData)
    /// assert_eq!(
    ///     expanded.as_slice()[2],
    ///     SgmlEvent::Character(CData("This is an <example> element".into()))
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
