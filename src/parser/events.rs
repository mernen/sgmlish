//! Parsing SGML into [`SgmlEvent`]s.

use std::borrow::Cow;
use std::iter::{FromIterator, FusedIterator};
use std::{fmt, mem};

use nom::branch::alt;
use nom::combinator::{all_consuming, cut, map, recognize, value};
use nom::error::{context, ContextError, ErrorKind, FromExternalError, ParseError};
use nom::multi::{many0, many0_count, many1};
use nom::sequence::{terminated, tuple};
use nom::IResult;

use crate::marked_sections::MarkedSectionStatus;
use crate::{Data, Error, SgmlEvent};

use super::parser::ParserConfig;
use super::raw::{self, comment_declaration, MarkedSectionEndHandling};
use super::util::{comments_and_spaces, strip_comments_and_spaces_after, strip_spaces_after};
use super::MarkedSectionHandling;

pub fn document_entity<'a, E>(
    input: &'a str,
    config: &ParserConfig,
) -> IResult<&'a str, impl Iterator<Item = SgmlEvent<'a>>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str> + FromExternalError<&'a str, Error>,
{
    all_consuming(map(
        tuple((
            comments_and_spaces,
            |input| prolog(input, config),
            context(
                "document content",
                cut(|input| content(input, config, MarkedSectionEndHandling::TreatAsText)),
            ),
            many0(strip_comments_and_spaces_after(|input| {
                processing_instruction(input, config)
            })),
        )),
        |(_, declarations, content, epilogue)| {
            declarations
                .into_iter()
                .chain(content)
                .chain(epilogue.into_iter().flatten())
        },
    ))(input)
}

pub fn prolog<'a, E>(
    input: &'a str,
    config: &ParserConfig,
) -> IResult<&'a str, Vec<SgmlEvent<'a>>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str> + FromExternalError<&'a str, Error>,
{
    context(
        "prolog",
        map(
            many0(strip_comments_and_spaces_after(alt((
                |input| markup_declaration(input, config),
                |input| marked_section(input, config),
                |input| processing_instruction(input, config),
            )))),
            |events| events.into_iter().flatten().collect(),
        ),
    )(input)
}

pub fn markup_declaration<'a, E>(
    input: &'a str,
    config: &ParserConfig,
) -> IResult<&'a str, EventIter<'a>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(raw::markup_declaration, |s| {
        EventIter::cond(!config.ignore_markup_declarations, || {
            SgmlEvent::MarkupDeclaration(Cow::from(s))
        })
    })(input)
}

pub fn marked_section<'a, E>(
    input: &'a str,
    config: &ParserConfig,
) -> IResult<&'a str, EventIter<'a>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str> + FromExternalError<&'a str, Error>,
{
    let (rest, status_keywords) = raw::marked_section_start(input)?;
    let status_keywords = config
        .parse_markup_declaration_text(status_keywords)
        .map_err(|err| {
            nom::Err::Failure(E::from_external_error(
                input,
                ErrorKind::Tag,
                Error::EntityError(err),
            ))
        })?;

    let marked_section_handling = config.marked_section_handling;
    let status = match marked_section_handling.parse_keywords(&status_keywords) {
        Some(status) => status,
        None => {
            return Err(nom::Err::Failure(E::from_external_error(
                input,
                ErrorKind::Tag,
                Error::InvalidMarkedSectionKeyword(status_keywords.into_owned()),
            )));
        }
    };

    match marked_section_handling {
        MarkedSectionHandling::KeepUnmodified => {
            let (rest, content) = match status {
                MarkedSectionStatus::Ignore => raw::marked_section_body_ignore(rest),
                MarkedSectionStatus::CData => raw::marked_section_body_character(rest),
                MarkedSectionStatus::RcData => raw::marked_section_body_character(rest),
                MarkedSectionStatus::Include => terminated(
                    recognize(|input| {
                        content(input, config, MarkedSectionEndHandling::StopParsing)
                    }),
                    raw::marked_section_end,
                )(rest),
            }?;
            Ok((
                rest,
                EventIter::once(SgmlEvent::MarkedSection(status_keywords, content.into())),
            ))
        }
        _ => match status {
            MarkedSectionStatus::Ignore => {
                map(raw::marked_section_body_ignore, |_| EventIter::empty())(rest)
            }
            MarkedSectionStatus::CData => map(raw::marked_section_body_character, |content| {
                EventIter::once(SgmlEvent::Character(Data::CData(
                    config.trim(content).into(),
                )))
            })(rest),
            MarkedSectionStatus::RcData => {
                let (rest, content) = raw::marked_section_body_character(rest)?;
                Ok((
                    rest,
                    EventIter::once(SgmlEvent::Character(
                        config.parse_rcdata(config.trim(content))?,
                    )),
                ))
            }
            MarkedSectionStatus::Include => terminated(
                map(
                    |input| content(input, config, MarkedSectionEndHandling::StopParsing),
                    EventIter::from_iter,
                ),
                raw::marked_section_end,
            )(rest),
        },
    }
}

pub fn processing_instruction<'a, E>(
    input: &'a str,
    config: &ParserConfig,
) -> IResult<&'a str, EventIter<'a>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(raw::processing_instruction, |s| {
        EventIter::cond(!config.ignore_processing_instructions, || {
            SgmlEvent::ProcessingInstruction(Cow::from(s))
        })
    })(input)
}

pub fn content<'a, E>(
    input: &'a str,
    config: &ParserConfig,
    mse: MarkedSectionEndHandling,
) -> IResult<&'a str, impl Iterator<Item = SgmlEvent<'a>>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str> + FromExternalError<&'a str, Error>,
{
    map(
        many1(terminated(
            |input| content_item(input, config, mse),
            many0_count(comment_declaration),
        )),
        |items| items.into_iter().flatten(),
    )(input)
}

/// Matches a single unit of content -- a tag, text data, processing instruction, or section declaration
pub fn content_item<'a, E>(
    input: &'a str,
    config: &ParserConfig,
    mse: MarkedSectionEndHandling,
) -> IResult<&'a str, EventIter<'a>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str> + FromExternalError<&'a str, Error>,
{
    alt((
        |input| text(input, config, mse),
        |input| start_tag(input, config),
        map(|input| end_tag(input, config), EventIter::once),
        |input| processing_instruction(input, config),
        |input| marked_section(input, config),
        // When all else fails, sinalize we expected at least opening a tag
        |input| Err(nom::Err::Error(E::from_char(input, '<'))),
    ))(input)
}

pub fn start_tag<'a, E>(input: &'a str, config: &ParserConfig) -> IResult<&'a str, EventIter<'a>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str> + FromExternalError<&'a str, Error>,
{
    context(
        "start tag",
        alt((
            map(
                tuple((
                    strip_spaces_after(|input| open_start_tag(input, config)),
                    many0(strip_spaces_after(|input| attribute(input, config))),
                    cut(alt((xml_close_empty_element, close_start_tag))),
                )),
                EventIter::start_tag,
            ),
            empty_start_tag,
        )),
    )(input)
}

pub fn open_start_tag<'a, E>(
    input: &'a str,
    config: &ParserConfig,
) -> IResult<&'a str, SgmlEvent<'a>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(raw::open_start_tag, |name| {
        SgmlEvent::OpenStartTag(config.name_normalization.normalize(name.into()))
    })(input)
}

pub fn close_start_tag<'a, E>(input: &'a str) -> IResult<&'a str, SgmlEvent<'a>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    value(SgmlEvent::CloseStartTag, raw::close_start_tag)(input)
}

pub fn xml_close_empty_element<'a, E>(input: &'a str) -> IResult<&'a str, SgmlEvent<'a>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    value(
        SgmlEvent::XmlCloseEmptyElement,
        raw::xml_close_empty_element,
    )(input)
}

pub fn empty_start_tag<'a, E>(input: &'a str) -> IResult<&'a str, EventIter<'a>, E>
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

pub fn attribute<'a, E>(input: &'a str, config: &ParserConfig) -> IResult<&'a str, SgmlEvent<'a>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str> + FromExternalError<&'a str, Error>,
{
    map(
        |input| raw::attribute_parse_value(input, |value| config.parse_rcdata(value)),
        |(key, value)| SgmlEvent::Attribute(config.name_normalization.normalize(key.into()), value),
    )(input)
}

fn end_tag<'a, E>(input: &'a str, config: &ParserConfig) -> IResult<&'a str, SgmlEvent<'a>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(raw::end_tag, |name| {
        SgmlEvent::EndTag(
            config
                .name_normalization
                .normalize(name.unwrap_or_default().into()),
        )
    })(input)
}

pub fn text<'a, E>(
    input: &'a str,
    config: &ParserConfig,
    mse: MarkedSectionEndHandling,
) -> IResult<&'a str, EventIter<'a>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str> + FromExternalError<&'a str, Error>,
{
    let (rest, text) = raw::text(input, mse)?;
    let s = config.trim(text);
    if s.is_empty() {
        return Ok((rest, EventIter::empty()));
    }
    Ok((
        rest,
        EventIter::once(SgmlEvent::Character(config.parse_rcdata(s)?)),
    ))
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
    const fn empty() -> Self {
        EventIter {
            start: None,
            middle: Vec::new(),
            end: None,
            middle_next: 0,
        }
    }

    fn once(event: SgmlEvent<'a>) -> Self {
        EventIter {
            start: Some(event),
            middle: Vec::new(),
            end: None,
            middle_next: 0,
        }
    }

    fn cond(condition: bool, event: impl FnOnce() -> SgmlEvent<'a>) -> Self {
        if condition {
            EventIter::once(event())
        } else {
            EventIter::empty()
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

impl<'a> FromIterator<SgmlEvent<'a>> for EventIter<'a> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = SgmlEvent<'a>>,
    {
        EventIter {
            start: None,
            middle: Vec::from_iter(iter),
            end: None,
            middle_next: 0,
        }
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
    use crate::parser::Parser;

    use super::Data::*;
    use super::SgmlEvent::*;
    use super::*;

    type E<'a> = nom::error::Error<&'a str>;

    #[test]
    fn test_document_entity_default_config() {
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
        let (rest, mut events) = document_entity::<E>(SAMPLE, &Default::default()).unwrap();
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
        assert_eq!(events.next(), Some(OpenStartTag("HEAD".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(events.next(), Some(OpenStartTag("TITLE".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(
            events.next(),
            Some(Character(CData("My first HTML document".into())))
        );
        assert_eq!(events.next(), Some(EndTag("TITLE".into())));
        assert_eq!(events.next(), Some(EndTag("HEAD".into())));

        assert_eq!(events.next(), Some(OpenStartTag("BODY".into())));
        assert_eq!(events.next(), Some(CloseStartTag));

        assert_eq!(events.next(), Some(OpenStartTag("P".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(events.next(), Some(Character(CData("Hello world!".into()))));

        assert_eq!(events.next(), Some(EndTag("BODY".into())));
        assert_eq!(events.next(), Some(EndTag("HTML".into())));
    }

    #[test]
    fn test_document_entity_ignore_markup_declarations_retain_whitespace() {
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

        let config = Parser::builder()
            .ignore_markup_declarations(true)
            .trim_whitespace(false)
            .into_config();
        let (rest, mut events) = document_entity::<E>(SAMPLE, &config).unwrap();
        assert!(rest.is_empty(), "rest: {:?}", rest);

        assert_eq!(events.next(), Some(OpenStartTag("HTML".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(
            events.next(),
            Some(Character(CData("\n                ".into())))
        );
        assert_eq!(events.next(), Some(OpenStartTag("HEAD".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(
            events.next(),
            Some(Character(CData("\n                    ".into())))
        );
        assert_eq!(events.next(), Some(OpenStartTag("TITLE".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(
            events.next(),
            Some(Character(CData("My first HTML document".into())))
        );
        assert_eq!(events.next(), Some(EndTag("TITLE".into())));
        assert_eq!(
            events.next(),
            Some(Character(CData("\n                ".into())))
        );
        assert_eq!(events.next(), Some(EndTag("HEAD".into())));
        assert_eq!(
            events.next(),
            Some(Character(CData("\n                ".into())))
        );

        assert_eq!(events.next(), Some(OpenStartTag("BODY".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(
            events.next(),
            Some(Character(CData("\n                    ".into())))
        );

        assert_eq!(events.next(), Some(OpenStartTag("P".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(
            events.next(),
            Some(Character(CData("Hello world!\n                ".into())))
        );

        assert_eq!(events.next(), Some(EndTag("BODY".into())));
        assert_eq!(
            events.next(),
            Some(Character(CData("\n            ".into())))
        );
        assert_eq!(events.next(), Some(EndTag("HTML".into())));
        assert_eq!(events.next(), Some(Character(CData("\n        ".into()))));
    }

    #[test]
    fn test_markup_declaration() {
        let input = r##"<!DOCTYPE HTML><!SGML>"##;

        let (rest, mut events) = markup_declaration::<E>(input, &Default::default()).unwrap();
        assert_eq!(rest, "<!SGML>");
        assert_eq!(
            events.next(),
            Some(SgmlEvent::MarkupDeclaration(r##"<!DOCTYPE HTML>"##.into()))
        );
        assert_eq!(events.next(), None);

        let config = Parser::builder()
            .ignore_markup_declarations(true)
            .into_config();
        let (rest, mut events) = markup_declaration::<E>(input, &config).unwrap();
        assert_eq!(rest, "<!SGML>");
        assert_eq!(events.next(), None);
    }

    #[test]
    fn test_processing_instruction() {
        let input = r##"<?experiment> "##;

        let (rest, mut events) = processing_instruction::<E>(input, &Default::default()).unwrap();
        assert_eq!(rest, " ");
        assert_eq!(
            events.next(),
            Some(SgmlEvent::ProcessingInstruction(
                r##"<?experiment>"##.into()
            ))
        );
        assert_eq!(events.next(), None);

        let config = Parser::builder()
            .ignore_processing_instructions(true)
            .into_config();
        let (rest, mut events) = processing_instruction::<E>(input, &config).unwrap();
        assert_eq!(rest, " ");
        assert_eq!(events.next(), None);
    }

    #[test]
    fn test_start_tag() {
        let config = Default::default();
        let (rest, mut events) =
            start_tag::<E>("<a href='test.htm' \ntarget = _blank > ok", &config).unwrap();
        assert_eq!(rest, " ok");

        assert_eq!(events.next(), Some(OpenStartTag("a".into())));
        assert_eq!(
            events.next(),
            Some(Attribute("href".into(), Some(CData("test.htm".into()))))
        );
        assert_eq!(
            events.next(),
            Some(Attribute("target".into(), Some(CData("_blank".into()))))
        );
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(events.next(), None);
    }

    #[test]
    fn test_start_tag_normalize_lowercase() {
        let config = Parser::builder().lowercase_names().into_config();
        let (rest, mut events) =
            start_tag::<E>("<A HREF='test.htm' \ntArget = _blank > ok", &config).unwrap();
        assert_eq!(rest, " ok");

        assert_eq!(events.next(), Some(OpenStartTag("a".into())));
        assert_eq!(
            events.next(),
            Some(Attribute("href".into(), Some(CData("test.htm".into()))))
        );
        assert_eq!(
            events.next(),
            Some(Attribute("target".into(), Some(CData("_blank".into()))))
        );
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(events.next(), None);
    }

    #[test]
    fn test_start_tag_normalize_uppercase() {
        let config = Parser::builder().uppercase_names().into_config();
        let (rest, mut events) =
            start_tag::<E>("<A href='test.htm' \ntArget = _blank > ok", &config).unwrap();
        assert_eq!(rest, " ok");

        assert_eq!(events.next(), Some(OpenStartTag("A".into())));
        assert_eq!(
            events.next(),
            Some(Attribute("HREF".into(), Some(CData("test.htm".into()))))
        );
        assert_eq!(
            events.next(),
            Some(Attribute("TARGET".into(), Some(CData("_blank".into()))))
        );
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(events.next(), None);
    }

    #[test]
    fn test_start_tag_trim_whitespace_does_not_affect_attributes() {
        let config = Parser::builder().trim_whitespace(true).into_config();
        let (rest, mut events) =
            start_tag::<E>("<img alt=' test ' longdesc=\" desc\">", &config).unwrap();
        assert_eq!(rest, "");

        assert_eq!(events.next(), Some(OpenStartTag("img".into())));
        assert_eq!(
            events.next(),
            Some(Attribute("alt".into(), Some(CData(" test ".into()))))
        );
        assert_eq!(
            events.next(),
            Some(Attribute("longdesc".into(), Some(CData(" desc".into()))))
        );
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(events.next(), None);
    }

    #[test]
    fn test_start_tag_xml_no_content() {
        let config = Default::default();
        let (rest, mut events) = start_tag::<E>("<br />", &config).unwrap();
        assert_eq!(rest, "");

        assert_eq!(events.next(), Some(OpenStartTag("br".into())));
        assert_eq!(events.next(), Some(XmlCloseEmptyElement));
        assert_eq!(events.next(), None);
    }

    #[test]
    fn test_start_tag_empty() {
        let config = Default::default();
        let (rest, mut events) = start_tag::<E>("<> ok", &config).unwrap();
        assert_eq!(rest, " ok");

        assert_eq!(events.next(), Some(OpenStartTag("".into())));
        assert_eq!(events.next(), Some(CloseStartTag));
        assert_eq!(events.next(), None);
    }

    #[test]
    fn test_end_tag() {
        let config = Default::default();
        assert_eq!(
            end_tag::<E>("</x>>", &config),
            Ok((">", EndTag("x".into())))
        );
        assert_eq!(
            end_tag::<E>("</Foo\n> ", &config),
            Ok((" ", EndTag("Foo".into())))
        );
        assert_eq!(end_tag::<E>("</>", &config), Ok(("", EndTag("".into()))));

        let config = Parser::builder().lowercase_names().into_config();
        assert_eq!(end_tag::<E>("</x>", &config), Ok(("", EndTag("x".into()))));
        assert_eq!(
            end_tag::<E>("</Foo\n>", &config),
            Ok(("", EndTag("foo".into())))
        );

        let config = Parser::builder().uppercase_names().into_config();
        assert_eq!(end_tag::<E>("</x>", &config), Ok(("", EndTag("X".into()))));
        assert_eq!(
            end_tag::<E>("</Foo\n>", &config),
            Ok(("", EndTag("FOO".into())))
        );
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
                Attribute("x".into(), Some(CData("y".into()))),
                Attribute("z".into(), None),
            ],
            CloseStartTag,
        ));

        assert_eq!(
            format!("{:?}", iter),
            r#"EventIter([OpenStartTag("foo"), Attribute("x", Some(CData("y"))), Attribute("z", None), CloseStartTag])"#
        );
        assert_eq!(iter.len(), 4);

        assert_eq!(iter.next(), Some(OpenStartTag("foo".into())));
        assert_eq!(
            format!("{:?}", iter),
            r#"EventIter([Attribute("x", Some(CData("y"))), Attribute("z", None), CloseStartTag])"#
        );
        assert_eq!(iter.len(), 3);

        assert_eq!(
            iter.next(),
            Some(Attribute("x".into(), Some(CData("y".into()))))
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
