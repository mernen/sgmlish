use std::fmt::{self, Write};
use std::iter::FusedIterator;

pub fn escape(text: &str) -> Escape {
    Escape::new(text)
}

/// The return type of [`Data::escape`].
#[derive(Clone, Debug)]
pub struct Escape<'a> {
    escape_ampersand: bool,
    chars: std::str::Chars<'a>,
    escape_buffer: Option<std::slice::Iter<'static, u8>>,
}

impl<'a> Escape<'a> {
    fn new(text: &'a str) -> Self {
        Escape {
            escape_ampersand: true,
            chars: text.chars(),
            escape_buffer: None,
        }
    }

    pub fn set_escape_ampersand(&mut self, escape_ampersand: bool) {
        self.escape_ampersand = escape_ampersand;
    }
}

impl<'a> Iterator for Escape<'a> {
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

impl FusedIterator for Escape<'_> {}

impl<'a> fmt::Display for Escape<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.clone().try_for_each(|c| f.write_char(c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_noop() {
        assert_eq!(escape("hello!").to_string(), "hello!");
    }

    #[test]
    fn test_escape_sequences() {
        assert_eq!(
            escape("hello && <world>").to_string(),
            "hello &#38;&#38; &#60;world&#62;"
        );
    }

    #[test]
    fn test_escape_disable_ampersand() {
        let mut esc = escape("hello && <world>");
        esc.set_escape_ampersand(false);
        assert_eq!(esc.to_string(), "hello && &#60;world&#62;");
    }

    #[test]
    fn test_escape_iter() {
        let mut escape = escape("wo<rld");
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
