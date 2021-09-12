use std::borrow::Cow;
use std::fmt::{self, Write};

use crate::util::make_owned;
use crate::{entities, is_blank, is_sgml_whitespace};

pub use Data::*;

/// A fragment of data in an SGML document. Normally found as text content of
/// elements, as attribute values, or in marked sections like `<![CDATA[...]]>`.
///
/// # Equality
///
/// Equality is strict: two `Data` instances are only considered equal if they
/// are of the same kind and have the same content.
/// That means, for example, that `CData("") != RcData("")`, even though they
/// are effectively the same in practical use.
///
/// To normalize all data fragments to `CData`, use [`Data::expand_entities`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Data<'a> {
    /// "Character data" --- content that should be understood literally.
    ///
    /// Means, for example, that `Hello&#33;` should be interpreted as a
    /// 10-character string.
    CData(Cow<'a, str>),
    /// "Replaceable character data" --- entities and character references
    /// should be expanded.
    ///
    /// Means, for example, that `Hello&#33;` should be interpreted as the
    /// 6-character string `Hello!`.
    RcData(Cow<'a, str>),
}

impl<'a> Data<'a> {
    /// Returns `true` for `CData`, meaning its contents should be taken literally,
    /// and `false` for `RcData`.
    pub fn verbatim(&self) -> bool {
        match self {
            Data::CData(_) => true,
            Data::RcData(_) => false,
        }
    }

    /// Returns a string slice with the contents of this fragment.
    pub fn as_str(&self) -> &str {
        match self {
            Data::CData(s) => s,
            Data::RcData(s) => s,
        }
    }

    #[cfg_attr(feature = "deserialize", allow(unused))]
    pub(crate) fn into_cow(self) -> Cow<'a, str> {
        match self {
            CData(s) => s,
            RcData(s) => s,
        }
    }

    /// Returns `true` if the data is empty or only contains whitespace characters.
    pub fn is_blank(&self) -> bool {
        is_blank(self.as_str())
    }

    /// Returns a new `Data` of the same kind, with leading and trailing whitespace removed.
    ///
    /// Whitespace rules are defined by [`is_sgml_whitespace`].
    pub fn trim(self) -> Data<'a> {
        fn trim_cow(cow: Cow<str>) -> Cow<str> {
            match cow {
                Cow::Borrowed(s) => Cow::Borrowed(s.trim_matches(is_sgml_whitespace)),
                Cow::Owned(s) => {
                    let trimmed = s.trim_matches(is_sgml_whitespace);
                    if trimmed.len() == s.len() {
                        Cow::Owned(s)
                    } else {
                        trimmed.to_owned().into()
                    }
                }
            }
        }

        match self {
            Data::CData(s) => Data::CData(trim_cow(s)),
            Data::RcData(s) => Data::RcData(trim_cow(s)),
        }
    }

    pub fn into_owned(self) -> Data<'static> {
        match self {
            Data::CData(s) => Data::CData(make_owned(s)),
            Data::RcData(s) => Data::RcData(make_owned(s)),
        }
    }

    /// Returns an iterator that escapes characters that cannot be represented in
    /// SGML text (`<`, `>`, `&`) using character references (`&#60;`).
    ///
    /// When escaping [`RcData`], only `<` and `>` are escaped; `&` is output as-is,
    /// as it references unprocessed character entities.
    /// When escaping [`CData`], `&` is escaped aswell.
    ///
    /// # Examples
    ///
    /// The returned value can be used with `println!` or other formatting macros:
    ///
    /// ```rust
    /// println!("Escaped: {}", sgmlish::CData("Sonic & Knuckles".into()).escape());
    /// ```
    ///
    /// To convert to a string:
    ///
    /// ```rust
    /// assert_eq!(sgmlish::CData("Sonic & Knuckles".into()).escape().to_string(), "Sonic &#38; Knuckles");
    /// ```
    ///
    /// Example of the difference between [CData] and [RcData]:
    ///
    /// ```rust
    /// assert_eq!(
    ///     sgmlish::RcData("<Sonic &amp; Knuckles>".into()).escape().to_string(),
    ///     "&#60;Sonic &amp; Knuckles&#62;"
    /// );
    /// assert_eq!(
    ///     sgmlish::CData("<Sonic &amp; Knuckles>".into()).escape().to_string(),
    ///     "&#60;Sonic &#38;amp; Knuckles&#62;"
    /// );
    /// ```
    pub fn escape(&self) -> EscapeData {
        EscapeData {
            escape_ampersand: self.verbatim(),
            chars: self.as_str().chars(),
            escape_buffer: None,
        }
    }

    /// Expands character references (`&#123;`) in `RcData`, converting it to `CData`.
    /// `CData` is returned unaltered.
    ///
    /// Fails if any entity reference (`&example;`) is present.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use sgmlish::{CData, RcData};
    /// # fn main() -> Result<(), sgmlish::Error> {
    /// let original = RcData("Hello&#44; world&#33;".into());
    /// let expanded = original.expand_character_references()?;
    /// assert_eq!(expanded, CData("Hello, world!".into()));
    /// // Expanding multiple times is a no-op, as the data is already flagged as CData
    /// let repeat = expanded.expand_character_references()?;
    /// assert_eq!(repeat, CData("Hello, world!".into()));
    /// # Ok(())
    /// # }
    /// ```
    pub fn expand_character_references(self) -> entities::Result<Self> {
        match self {
            CData(_) => Ok(self),
            RcData(s) => {
                let expanded = entities::expand_character_references(&s)?;
                if expanded == *s {
                    Ok(CData(s))
                } else {
                    Ok(CData(expanded.into_owned().into()))
                }
            }
        }
    }

    /// Expands entity references (`&example;`) as well as character references (`&#123;`)
    /// in `RcData`, converting it to `CData`. `CData` is returned unaltered.
    ///
    /// The given closure is called to expand each entity found. Fails if the closure returns `None`.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use std::collections::HashMap;
    /// # use sgmlish::{CData, RcData};
    /// # fn main() -> Result<(), sgmlish::Error> {
    /// let mut entities = HashMap::new();
    /// entities.insert("eacute", "é");
    ///
    /// let original = RcData("Caf&eacute;".into());
    /// let expanded = original.expand_entities(|entity| entities.get(entity))?;
    /// assert_eq!(expanded, CData("Café".into()));
    /// // Expanding multiple times is a no-op, as the data is already flagged as CData
    /// let repeat = expanded.expand_entities(|entity| entities.get(entity))?;
    /// assert_eq!(repeat, CData("Café".into()));
    /// # Ok(())
    /// # }
    /// ```
    pub fn expand_entities<F, T>(self, f: F) -> entities::Result<Self>
    where
        F: FnMut(&str) -> Option<T>,
        T: AsRef<str>,
    {
        match self {
            CData(_) => Ok(self),
            RcData(s) => {
                let expanded = entities::expand_entities(&s, f)?;
                if expanded == *s {
                    Ok(CData(s))
                } else {
                    Ok(CData(expanded.into_owned().into()))
                }
            }
        }
    }
}

impl Default for Data<'_> {
    fn default() -> Self {
        Data::CData(Cow::Borrowed(""))
    }
}

/// The return type of [`Data::escape`].
#[derive(Clone, Debug)]
pub struct EscapeData<'a> {
    escape_ampersand: bool,
    chars: std::str::Chars<'a>,
    escape_buffer: Option<std::slice::Iter<'static, u8>>,
}

impl<'a> Iterator for EscapeData<'a> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(buffer) = &mut self.escape_buffer {
            match buffer.next() {
                Some(c) => return Some(*c as char),
                None => self.escape_buffer = None,
            }
        }
        match self.chars.next() {
            Some('<') => {
                self.escape_buffer = Some(b"#60;".iter());
                Some('&')
            }
            Some('>') => {
                self.escape_buffer = Some(b"#62;".iter());
                Some('&')
            }
            Some('&') if self.escape_ampersand => {
                self.escape_buffer = Some(b"#38;".iter());
                Some('&')
            }
            x => x,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (min, max) = self.chars.size_hint();
        let escape_len = self
            .escape_buffer
            .as_ref()
            .map(|buf| buf.len())
            .unwrap_or(0);

        (
            min + escape_len,
            max
                // Every remaining character may convert to "&#xx;"
                .and_then(|n| n.checked_mul(5))
                .and_then(|n| n.checked_add(escape_len)),
        )
    }
}

impl<'a> fmt::Display for EscapeData<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.clone().try_for_each(|c| f.write_char(c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cdata(s: &str) -> Data {
        CData(s.into())
    }

    fn rcdata(s: &str) -> Data {
        RcData(s.into())
    }

    #[test]
    fn test_escape_data_noop() {
        assert_eq!(cdata("hello!").escape().to_string(), "hello!");
        assert_eq!(rcdata("hello!").escape().to_string(), "hello!");
    }

    #[test]
    fn test_escape_data_sequences() {
        assert_eq!(
            cdata("hello && <world>").escape().to_string(),
            "hello &#38;&#38; &#60;world&#62;"
        );
        assert_eq!(
            rcdata("hello && <world>").escape().to_string(),
            "hello && &#60;world&#62;"
        );
    }

    #[test]
    fn test_escape_data_iter() {
        let data = rcdata("wo<rld");
        let mut escape = data.escape();
        assert_eq!(escape.size_hint(), (2, Some(30)));

        assert_eq!(escape.next(), Some('w'));
        assert_eq!(escape.size_hint(), (2, Some(25)));

        assert_eq!(escape.next(), Some('o'));
        assert_eq!(escape.size_hint(), (1, Some(20)));

        assert_eq!(escape.next(), Some('&'));
        assert_eq!(escape.size_hint(), (4 + 1, Some(4 + 15)));

        assert_eq!(escape.next(), Some('#'));
        assert_eq!(escape.size_hint(), (3 + 1, Some(3 + 15)));

        assert_eq!(escape.next(), Some('6'));
        assert_eq!(escape.size_hint(), (2 + 1, Some(2 + 15)));

        assert_eq!(escape.next(), Some('0'));
        assert_eq!(escape.size_hint(), (1 + 1, Some(1 + 15)));

        assert_eq!(escape.next(), Some(';'));
        assert_eq!(escape.size_hint(), (0 + 1, Some(0 + 15)));

        assert_eq!(escape.next(), Some('r'));
        assert_eq!(escape.size_hint(), (1, Some(10)));

        assert_eq!(escape.next(), Some('l'));
        assert_eq!(escape.size_hint(), (1, Some(5)));

        assert_eq!(escape.next(), Some('d'));
        assert_eq!(escape.size_hint(), (0, Some(0)));

        assert_eq!(escape.next(), None);
        assert_eq!(escape.size_hint(), (0, Some(0)));
    }
}
