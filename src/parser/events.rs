//! Parsing SGML into [`SgmlEvent`]s.

use std::borrow::Cow;
use std::iter::FusedIterator;
use std::{fmt, mem};

use nom::branch::alt;
use nom::combinator::{all_consuming, cut, map, value};
use nom::error::{context, ContextError, ParseError};
use nom::multi::{many0, many0_count, many1};
use nom::sequence::{terminated, tuple};
use nom::IResult;

use crate::{Data, SgmlEvent};

use super::raw::{self, comment_declaration};
use super::util::{comments_and_spaces, strip_comments_and_spaces_after, strip_spaces_after};

pub fn document_entity<'a, E>(input: &'a str) -> IResult<&str, impl Iterator<Item = SgmlEvent>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    all_consuming(map(
        tuple((
            comments_and_spaces,
            prolog,
            context("document content", cut(content)),
            many0(strip_comments_and_spaces_after(processing_instruction)),
        )),
        |(_, declarations, content, epilogue)| {
            declarations.into_iter().chain(content).chain(epilogue)
        },
    ))(input)
}

pub fn prolog<'a, E>(input: &'a str) -> IResult<&str, Vec<SgmlEvent>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    context(
        "prolog",
        many0(strip_comments_and_spaces_after(alt((
            markup_declaration,
            marked_section,
            processing_instruction,
        )))),
    )(input)
}

pub fn markup_declaration<'a, E>(input: &'a str) -> IResult<&str, SgmlEvent, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(raw::markup_declaration, |s| {
        SgmlEvent::MarkupDeclaration(Cow::from(s))
    })(input)
}

pub fn marked_section<'a, E>(input: &'a str) -> IResult<&str, SgmlEvent, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(raw::marked_section, |(status_keywords, content)| {
        SgmlEvent::MarkedSection(Cow::from(status_keywords), Cow::from(content))
    })(input)
}

pub fn processing_instruction<'a, E>(input: &'a str) -> IResult<&str, SgmlEvent, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(raw::processing_instruction, |s| {
        SgmlEvent::ProcessingInstruction(Cow::from(s))
    })(input)
}

pub fn content<'a, E>(input: &'a str) -> IResult<&str, impl Iterator<Item = SgmlEvent>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(
        many1(terminated(content_item, many0_count(comment_declaration))),
        |items| items.into_iter().flatten(),
    )(input)
}

/// Matches a single unit of content -- a tag, text data, processing instruction, or section declaration
pub fn content_item<'a, E>(input: &'a str) -> IResult<&str, EventIter, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    alt((
        map(data, EventIter::once),
        start_tag,
        map(end_tag, EventIter::once),
        map(processing_instruction, EventIter::once),
        map(marked_section, EventIter::once),
        // When all else fails, sinalize we expected at least opening a tag
        |input| Err(nom::Err::Error(E::from_char(input, '<'))),
    ))(input)
}

pub fn start_tag<'a, E>(input: &'a str) -> IResult<&str, EventIter, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    context(
        "start tag",
        alt((
            map(
                tuple((
                    strip_spaces_after(open_start_tag),
                    many0(strip_spaces_after(attribute)),
                    cut(alt((xml_close_empty_element, close_start_tag))),
                )),
                EventIter::start_tag,
            ),
            empty_start_tag,
        )),
    )(input)
}

pub fn open_start_tag<'a, E>(input: &'a str) -> IResult<&str, SgmlEvent, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(raw::open_start_tag, |name| {
        SgmlEvent::OpenStartTag(name.into())
    })(input)
}

pub fn close_start_tag<'a, E>(input: &'a str) -> IResult<&str, SgmlEvent, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    value(SgmlEvent::CloseStartTag, raw::close_start_tag)(input)
}

pub fn xml_close_empty_element<'a, E>(input: &'a str) -> IResult<&str, SgmlEvent, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    value(
        SgmlEvent::XmlCloseEmptyElement,
        raw::xml_close_empty_element,
    )(input)
}

pub fn empty_start_tag<'a, E>(input: &'a str) -> IResult<&str, EventIter, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(raw::empty_start_tag, |_| {
        EventIter::start_tag((
            SgmlEvent::OpenStartTag("".into()),
            vec![],
            SgmlEvent::CloseStartTag,
        ))
    })(input)
}

pub fn attribute<'a, E>(input: &'a str) -> IResult<&str, SgmlEvent, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(raw::attribute, |(key, value)| {
        SgmlEvent::Attribute(key.into(), value.map(|value| Data::RcData(value.into())))
    })(input)
}

fn end_tag<'a, E>(input: &'a str) -> IResult<&str, SgmlEvent, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(raw::end_tag, |name| {
        SgmlEvent::EndTag(name.unwrap_or_default().into())
    })(input)
}

pub fn data<'a, E>(input: &'a str) -> IResult<&str, SgmlEvent, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(raw::data, |s| SgmlEvent::Data(Data::RcData(s.into())))(input)
}

/// An iterator over a sequence of events.
///
/// This struct exists to minimize the number of allocations during the
/// parsing phase.
#[derive(PartialEq)]
pub struct EventIter<'a> {
    start: Option<SgmlEvent<'a>>,
    middle: Vec<SgmlEvent<'a>>,
    end: Option<SgmlEvent<'a>>,
    middle_next: usize,
}

impl<'a> EventIter<'a> {
    fn once(event: SgmlEvent<'a>) -> Self {
        EventIter {
            start: Some(event),
            middle: Vec::new(),
            end: None,
            middle_next: 0,
        }
    }

    fn start_tag((start, middle, end): (SgmlEvent<'a>, Vec<SgmlEvent<'a>>, SgmlEvent<'a>)) -> Self {
        EventIter {
            start: Some(start),
            middle,
            end: Some(end),
            middle_next: 0,
        }
    }
}

impl<'a> Iterator for EventIter<'a> {
    type Item = SgmlEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(event) = self.start.take() {
            return Some(event);
        }

        if let Some(event) = self.middle.get_mut(self.middle_next) {
            self.middle_next += 1;
            return Some(mem::replace(event, SgmlEvent::XmlCloseEmptyElement));
        }

        if let Some(event) = self.end.take() {
            return Some(event);
        }

        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for EventIter<'_> {
    fn len(&self) -> usize {
        // Overflow: `middle_next` stops incrementing as soon as `middle_next == middle.len()`
        let middle_remaining = self.middle.len() - self.middle_next;
        self.start.is_some() as usize + middle_remaining + self.end.is_some() as usize
    }
}

impl fmt::Debug for EventIter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("EventIter(")?;
        let mut list = f.debug_list();
        if let Some(event) = &self.start {
            list.entry(event);
        }
        if let Some(events) = self.middle.get(self.middle_next..) {
            list.entries(events);
        }
        if let Some(event) = &self.end {
            list.entry(event);
        }
        list.finish()?;
        f.write_str(")")
    }
}

impl FusedIterator for EventIter<'_> {}

#[cfg(test)]
mod tests {
    use super::Data::*;
    use super::SgmlEvent::*;
    use super::*;

    type E<'a> = nom::error::Error<&'a str>;

    #[test]
    fn test_event_display() {
        assert_eq!(
            format!("{}", MarkupDeclaration("<?DOCTYPE HTML?>".into())),
            "<?DOCTYPE HTML?>"
        );
        assert_eq!(
            format!("{}", ProcessingInstruction("<?IS10744 FSIDR myurl>".into())),
            "<?IS10744 FSIDR myurl>"
        );

        assert_eq!(format!("{}", OpenStartTag("foo".into())), "<foo");
        assert_eq!(
            format!("{}", Attribute("foo".into(), Some(RcData("bar".into())))),
            "foo=\"bar\""
        );
        assert_eq!(format!("{}", Attribute("foo".into(), None)), "foo");
        assert_eq!(format!("{}", CloseStartTag), ">");
        assert_eq!(format!("{}", XmlCloseEmptyElement), "/>");
        assert_eq!(format!("{}", EndTag("foo".into())), "</foo>");
        assert_eq!(format!("{}", EndTag("".into())), "</>");

        assert_eq!(format!("{}", Data(RcData("hello".into()))), "hello");
    }

    #[test]
    fn test_document_entity() {
        const SAMPLE: &str = r#"
            <!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN"
                "http://www.w3.org/TR/html4/strict.dtd">
            <HTML>
                <HEAD>
                    <TITLE>My first HTML document</TITLE>
                </HEAD>
                <BODY>
                    <P>Hello world!
                </BODY>
            </HTML>
        "#;
        let (rest, mut events) = document_entity::<E>(SAMPLE).unwrap();
        assert!(rest.is_empty(), "rest: {:?}", rest);

        assert_eq!(
            events.next(),
            Some(MarkupDeclaration(
                "<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.01//EN\"\n                \"http://www.w3.org/TR/html4/strict.dtd\">"
                    .into()
            ))
        );

        assert_eq!(events.next(), Some(OpenStartTag("HTML".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(
            events.next(),
            Some(Data(RcData("\n                ".into())))
        );
        assert_eq!(events.next(), Some(OpenStartTag("HEAD".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(
            events.next(),
            Some(Data(RcData("\n                    ".into())))
        );
        assert_eq!(events.next(), Some(OpenStartTag("TITLE".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(
            events.next(),
            Some(Data(RcData("My first HTML document".into())))
        );
        assert_eq!(events.next(), Some(EndTag("TITLE".into())));
        assert_eq!(
            events.next(),
            Some(Data(RcData("\n                ".into())))
        );
        assert_eq!(events.next(), Some(EndTag("HEAD".into())));
        assert_eq!(
            events.next(),
            Some(Data(RcData("\n                ".into())))
        );

        assert_eq!(events.next(), Some(OpenStartTag("BODY".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(
            events.next(),
            Some(Data(RcData("\n                    ".into())))
        );

        assert_eq!(events.next(), Some(OpenStartTag("P".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(
            events.next(),
            Some(Data(RcData("Hello world!\n                ".into())))
        );

        assert_eq!(events.next(), Some(EndTag("BODY".into())));
        assert_eq!(events.next(), Some(Data(RcData("\n            ".into()))));
        assert_eq!(events.next(), Some(EndTag("HTML".into())));
        assert_eq!(events.next(), Some(Data(RcData("\n        ".into()))));
    }

    #[test]
    fn test_start_tag() {
        let (rest, mut events) =
            start_tag::<E>("<a href='test.htm' \ntarget = _blank > ok").unwrap();
        assert_eq!(rest, " ok");

        assert_eq!(events.next(), Some(OpenStartTag("a".into())));
        assert_eq!(
            events.next(),
            Some(Attribute("href".into(), Some(RcData("test.htm".into()))))
        );
        assert_eq!(
            events.next(),
            Some(Attribute("target".into(), Some(RcData("_blank".into()))))
        );
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(events.next(), None);
    }

    #[test]
    fn test_start_tag_xml_no_content() {
        let (rest, mut events) = start_tag::<E>("<br />").unwrap();
        assert_eq!(rest, "");

        assert_eq!(events.next(), Some(OpenStartTag("br".into())));
        assert_eq!(events.next(), Some(XmlCloseEmptyElement));
        assert_eq!(events.next(), None);
    }

    #[test]
    fn test_start_tag_empty() {
        let (rest, mut events) = start_tag::<E>("<> ok").unwrap();
        assert_eq!(rest, " ok");

        assert_eq!(events.next(), Some(OpenStartTag("".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(events.next(), None);
    }

    #[test]
    fn test_end_tag() {
        assert_eq!(end_tag::<E>("</x>"), Ok(("", EndTag("x".into()))));
        assert_eq!(end_tag::<E>("</foo\n>"), Ok(("", EndTag("foo".into()))));
        assert_eq!(end_tag::<E>("</>"), Ok(("", EndTag("".into()))));
    }

    #[test]
    fn test_event_iter_single_item() {
        let mut iter = EventIter::once(EndTag("foo".into()));

        assert_eq!(format!("{:?}", iter), "EventIter([EndTag(\"foo\")])");
        assert_eq!(iter.len(), 1);

        assert_eq!(iter.next(), Some(EndTag("foo".into())));
        assert_eq!(format!("{:?}", iter), "EventIter([])");
        assert_eq!(iter.len(), 0);

        assert_eq!(iter.next(), None);
        assert_eq!(iter.len(), 0);
    }

    #[test]
    fn test_event_iter_complete() {
        let mut iter = EventIter::start_tag((
            OpenStartTag("foo".into()),
            vec![
                Attribute("x".into(), Some(RcData("y".into()))),
                Attribute("z".into(), None),
            ],
            CloseStartTag,
        ));

        assert_eq!(
            format!("{:?}", iter),
            r#"EventIter([OpenStartTag("foo"), Attribute("x", Some(RcData("y"))), Attribute("z", None), CloseStartTag])"#
        );
        assert_eq!(iter.len(), 4);

        assert_eq!(iter.next(), Some(OpenStartTag("foo".into())));
        assert_eq!(
            format!("{:?}", iter),
            r#"EventIter([Attribute("x", Some(RcData("y"))), Attribute("z", None), CloseStartTag])"#
        );
        assert_eq!(iter.len(), 3);

        assert_eq!(
            iter.next(),
            Some(Attribute("x".into(), Some(RcData("y".into()))))
        );
        assert_eq!(
            format!("{:?}", iter),
            r#"EventIter([Attribute("z", None), CloseStartTag])"#
        );
        assert_eq!(iter.len(), 2);

        assert_eq!(iter.next(), Some(Attribute("z".into(), None)));
        assert_eq!(format!("{:?}", iter), "EventIter([CloseStartTag])");
        assert_eq!(iter.len(), 1);

        assert_eq!(iter.next(), Some(CloseStartTag));
        assert_eq!(format!("{:?}", iter), "EventIter([])");
        assert_eq!(iter.len(), 0);

        assert_eq!(iter.next(), None);
        assert_eq!(iter.len(), 0);
    }
}
