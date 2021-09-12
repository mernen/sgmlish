use std::borrow::Cow;
use std::mem;

use crate::Data;

/// A `Cow` string builder that consumes its inputs, avoiding cloning or
/// reallocating when there's just one source.
#[derive(Debug)]
pub(crate) struct CowBuffer<'a>(Cow<'a, str>);

impl<'a> CowBuffer<'a> {
    pub(crate) fn new() -> Self {
        CowBuffer(Cow::from(""))
    }

    pub(crate) fn push_data(&mut self, data: &mut Data<'a>) {
        let cow = match mem::replace(data, Data::RcData("*CONSUMED*".into())) {
            Data::CData(cow) => cow,
            Data::RcData(_) => unreachable!("unexpanded RcData found"),
        };
        if cow.is_empty() {
            return;
        }
        if self.0.is_empty() {
            self.0 = cow;
        } else {
            self.0.to_mut().push_str(&cow);
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn into_cow(self) -> Cow<'a, str> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cow_buffer() {
        let mut buf = CowBuffer::new();
        assert!(buf.0.is_empty());

        buf.push_data(&mut Data::CData("Hello".into()));
        assert!(matches!(buf, CowBuffer(Cow::Borrowed("Hello"))));

        buf.push_data(&mut Data::CData(" World".into()));
        assert_eq!(buf.0, "Hello World");
    }
}
