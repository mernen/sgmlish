use std::fmt;

use crate::SgmlEvent;

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
        self.events.iter().try_for_each(|event| {
            if let SgmlEvent::Attribute { .. } = event {
                f.write_str(" ")?;
            }
            fmt::Display::fmt(event, f)
        })
    }
}
