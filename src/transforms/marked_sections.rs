use std::borrow::Cow;
use std::mem;

use nom::Finish;

use crate::parser::DefaultErrorType;
use crate::{is_blank, parser, ParseError};
use crate::{Data, Error, SgmlEvent, SgmlFragment};

use super::Transform;

pub(crate) fn expand_marked_sections(mut fragment: SgmlFragment) -> Result<SgmlFragment, Error> {
    let mut transform = Transform::new();

    for (i, event) in fragment.iter_mut().enumerate() {
        if let SgmlEvent::MarkedSection(keywords, content) = event {
            transform.remove_at(i);
            for e in expand_marked_section(keywords, mem::take(content))? {
                transform.insert_at(i, e);
            }
        }
    }

    Ok(transform.apply(fragment))
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MarkedSectionStatus {
    Include,
    RcData,
    CData,
    Ignore,
}

const KEYWORDS: &[(&str, MarkedSectionStatus)] = &[
    ("CDATA", MarkedSectionStatus::CData),
    ("RCDATA", MarkedSectionStatus::RcData),
    ("IGNORE", MarkedSectionStatus::Ignore),
    ("INCLUDE", MarkedSectionStatus::Include),
    ("TEMP", MarkedSectionStatus::Include),
];

impl MarkedSectionStatus {
    /// Returns the operation for the given status keyword.
    ///
    /// Returns `None` if the given string is not a valid keyword.
    pub fn from_keyword(status_keyword: &str) -> Option<Self> {
        KEYWORDS
            .iter()
            .find_map(|(kw, level)| kw.eq_ignore_ascii_case(status_keyword).then(|| *level))
    }

    /// Returns the highest-priority operation from all the given keywords.
    ///
    /// If the keyword list contains an invalid keyword, returns it as an error.
    pub fn from_keywords(status_keywords: &str) -> Result<Self, &str> {
        status_keywords
            .split_ascii_whitespace()
            .map(|keyword| MarkedSectionStatus::from_keyword(keyword).ok_or(keyword))
            .try_fold(MarkedSectionStatus::Include, |a, b| b.map(|b| a.max(b)))
    }
}

/// Expands the described marked section into a fragment.
///
/// If the status keywords list contains an `IGNORE` keyword, no events are produced.
/// If a `CDATA` or `RCDATA` keyword is present, the contents are treated as a single piece of [`Data`].
/// `INCLUDE` and `TEMP` have no practical effect.
///
/// Parameter entities are not supported in the keywords list; you must expand
/// them yourself beforehand.
/// For example, `<![%condition[ example ]]>` will return an [`UnrecognizedMarkedSectionKeyword`] error.
///
/// [`UnrecognizedMarkedSectionKeyword`]: Error::UnrecognizedMarkedSectionKeyword
pub fn expand_marked_section<'a>(
    status_keywords: &str,
    content: Cow<'a, str>,
) -> Result<SgmlFragment<'a>, Error> {
    let status = MarkedSectionStatus::from_keywords(status_keywords)
        .map_err(|keyword| Error::UnrecognizedMarkedSectionKeyword(keyword.to_owned()))?;

    fn parse(
        input: &str,
    ) -> Result<impl Iterator<Item = SgmlEvent>, ParseError<&str, DefaultErrorType<&str>>> {
        let (rest, events) = nom::combinator::all_consuming(parser::events::content)(input)
            .finish()
            .map_err(|error| ParseError::from_nom(input, error))?;
        debug_assert!(rest.is_empty(), "all_consuming failed");
        Ok(events)
    }

    match status {
        MarkedSectionStatus::Ignore => Ok(vec![].into()),
        MarkedSectionStatus::CData => Ok(vec![SgmlEvent::Data(Data::CData(content))].into()),
        MarkedSectionStatus::RcData => Ok(vec![SgmlEvent::Data(Data::RcData(content))].into()),
        MarkedSectionStatus::Include => {
            if is_blank(&content) {
                return Ok(vec![SgmlEvent::Data(Data::RcData(content))].into());
            }

            let events = match content {
                Cow::Borrowed(content) => parse(content)?.collect::<Vec<_>>(),
                Cow::Owned(content) => parse(&content)?
                    .map(|event| event.into_owned())
                    .collect::<Vec<_>>(),
            };
            Ok(events.into())
        }
    }
}

/// Returns a properly-marked [`Data`] if the list of status keywords only contains
/// a single `CDATA` or `RCDATA` keyword, or gives the `content` back otherwise.
pub fn extract_data_marked_section<'a>(
    status_keywords: &str,
    content: Cow<'a, str>,
) -> Result<Data<'a>, Cow<'a, str>> {
    if status_keywords.eq_ignore_ascii_case("CDATA") {
        Ok(Data::CData(content))
    } else if status_keywords.eq_ignore_ascii_case("RCDATA") {
        Ok(Data::RcData(content))
    } else {
        Err(content)
    }
}

#[cfg(test)]
mod tests {
    use crate::parse;

    use super::*;

    #[test]
    fn test_expand_marked_sections() {
        let events = parse(
            "\
                <TEST>\
                    Start \
                    <![[ First <FOO> item ]]>\
                    <![RCDATA IGNORE[ <BAR> text ]]>\
                    mid\
                    <![IGNORE[ Second <FOO> item ]]>\
                    <![TEMP RCDATA[ <BAR> text ]]>\
                    </FOO>\
                </TEST>\
            ",
        )
        .unwrap();

        let mut events = expand_marked_sections(events).unwrap().into_iter();

        assert_eq!(events.next(), Some(SgmlEvent::OpenStartTag("TEST".into())));
        assert_eq!(events.next(), Some(SgmlEvent::CloseStartTag));
        assert_eq!(
            events.next(),
            Some(SgmlEvent::Data(Data::RcData("Start ".into())))
        );
        assert_eq!(
            events.next(),
            Some(SgmlEvent::Data(Data::RcData(" First ".into())))
        );
        assert_eq!(events.next(), Some(SgmlEvent::OpenStartTag("FOO".into())));
        assert_eq!(events.next(), Some(SgmlEvent::CloseStartTag));
        assert_eq!(
            events.next(),
            Some(SgmlEvent::Data(Data::RcData(" item ".into())))
        );
        assert_eq!(
            events.next(),
            Some(SgmlEvent::Data(Data::RcData("mid".into())))
        );
        assert_eq!(
            events.next(),
            Some(SgmlEvent::Data(Data::RcData(" <BAR> text ".into())))
        );
        assert_eq!(events.next(), Some(SgmlEvent::EndTag("FOO".into())));
        assert_eq!(events.next(), Some(SgmlEvent::EndTag("TEST".into())));
        assert_eq!(events.next(), None);
    }

    #[test]
    fn test_expand_ignore_cdata() {
        let mut expanded = expand_marked_section("IGNORE CDATA", "Hello World".into())
            .unwrap()
            .into_iter();

        assert_eq!(expanded.next(), None);
    }

    #[test]
    fn test_expand_ignore_rcdata() {
        let mut expanded = expand_marked_section("RCDATA IGNORE", "Hello World".into())
            .unwrap()
            .into_iter();

        assert_eq!(expanded.next(), None);
    }

    #[test]
    fn test_expand_empty_cdata() {
        let mut expanded = expand_marked_section("CDATA", "".into())
            .unwrap()
            .into_iter();

        assert_eq!(
            expanded.next(),
            Some(SgmlEvent::Data(Data::CData("".into())))
        );
        assert_eq!(expanded.next(), None);
    }

    #[test]
    fn test_expand_content() {
        let mut expanded = expand_marked_section("", "<b>Hello</b>".into())
            .unwrap()
            .into_iter();

        assert_eq!(expanded.next(), Some(SgmlEvent::OpenStartTag("b".into())));
        assert_eq!(expanded.next(), Some(SgmlEvent::CloseStartTag));
        assert_eq!(
            expanded.next(),
            Some(SgmlEvent::Data(Data::RcData("Hello".into())))
        );
        assert_eq!(expanded.next(), Some(SgmlEvent::EndTag("b".into())));
        assert_eq!(expanded.next(), None);
    }

    #[test]
    fn test_expand_content_with_whitespace() {
        let mut expanded = expand_marked_section("", " <b> Hello </b> ".into())
            .unwrap()
            .into_iter();

        assert_eq!(
            expanded.next(),
            Some(SgmlEvent::Data(Data::RcData(" ".into())))
        );
        assert_eq!(expanded.next(), Some(SgmlEvent::OpenStartTag("b".into())));
        assert_eq!(expanded.next(), Some(SgmlEvent::CloseStartTag));
        assert_eq!(
            expanded.next(),
            Some(SgmlEvent::Data(Data::RcData(" Hello ".into())))
        );
        assert_eq!(expanded.next(), Some(SgmlEvent::EndTag("b".into())));
        assert_eq!(
            expanded.next(),
            Some(SgmlEvent::Data(Data::RcData(" ".into())))
        );
        assert_eq!(expanded.next(), None);
    }

    #[test]
    fn test_expand_content_only_whitespace() {
        let mut expanded = expand_marked_section("INCLUDE", "\n\n".into())
            .unwrap()
            .into_iter();

        assert_eq!(
            expanded.next(),
            Some(SgmlEvent::Data(Data::RcData("\n\n".into())))
        );
        assert_eq!(expanded.next(), None);
    }

    #[test]
    fn test_expand_content_empty() {
        let mut expanded = expand_marked_section("TEMP", "".into())
            .unwrap()
            .into_iter();

        assert_eq!(
            expanded.next(),
            Some(SgmlEvent::Data(Data::RcData("".into())))
        );
        assert_eq!(expanded.next(), None);
    }

    #[test]
    fn test_expand_content_incomplete_start_tag() {
        let err = expand_marked_section("", " <x y='z' ".into()).unwrap_err();
        assert!(matches!(err, Error::ParseError(_)));
    }

    #[test]
    fn test_expand_content_incomplete_end_tag() {
        let mut expanded = expand_marked_section("", "</ ".into()).unwrap().into_iter();

        assert_eq!(
            expanded.next(),
            Some(SgmlEvent::Data(Data::RcData("</ ".into())))
        );
        assert_eq!(expanded.next(), None);
    }

    #[test]
    fn test_extract_data_marked_section() {
        assert_eq!(
            extract_data_marked_section("CDATA", "Hello".into()),
            Ok(Data::CData("Hello".into()))
        );
        assert_eq!(
            extract_data_marked_section("RCDATA", "Hello".into()),
            Ok(Data::RcData("Hello".into()))
        );
        assert_eq!(
            extract_data_marked_section("RCDATA TEMP", "Hello".into()),
            Err("Hello".into())
        );
        assert_eq!(
            extract_data_marked_section("", "Hello".into()),
            Err("Hello".into())
        );
    }
}
