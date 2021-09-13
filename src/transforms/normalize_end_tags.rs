use crate::transforms::Transform;
use crate::{SgmlEvent, SgmlFragment};

/// The error type in the event tag normalization fails.
///
/// This is returned by [`SgmlFragment::normalize_end_tags`].
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum NormalizationError {
    #[error("unpaired end tag: </{0}>")]
    UnpairedEndTag(String),
    #[error("empty start tags (<>) are not supported")]
    EmptyStartTagNotSupported,
    #[error("empty end tags (</>) are not supported")]
    EmptyEndTagNotSupported,
}

pub(crate) fn normalize_end_tags(
    mut fragment: SgmlFragment,
) -> Result<SgmlFragment, NormalizationError> {
    let mut transform = Transform::new();
    let mut stack = vec![];
    let mut next_insertion_point = fragment.len();
    let mut end_xml_empty_element = None;

    for (i, event) in fragment.iter_mut().enumerate().rev() {
        match event {
            SgmlEvent::OpenStartTag(name) => {
                if name.is_empty() {
                    return Err(NormalizationError::EmptyStartTagNotSupported);
                }
                let insertion_point = end_xml_empty_element.take().or_else(|| match stack.last() {
                    Some(end_name) if *end_name == name => {
                        stack.pop();
                        None
                    }
                    _ => Some(next_insertion_point),
                });
                if let Some(insertion_point) = insertion_point {
                    transform.insert_at(insertion_point, SgmlEvent::EndTag(name.clone()));
                }
                next_insertion_point = i;
            }
            SgmlEvent::XmlCloseEmptyElement => {
                *event = SgmlEvent::CloseStartTag;
                end_xml_empty_element = Some(i + 1);
            }
            SgmlEvent::EndTag(name) => {
                if name.is_empty() {
                    return Err(NormalizationError::EmptyEndTagNotSupported);
                }
                stack.push(name);
                next_insertion_point = i;
            }
            SgmlEvent::Character(data) => {
                if next_insertion_point == i + 1 && data.is_blank() {
                    next_insertion_point -= 1;
                }
            }
            _ => {}
        }
    }

    if let Some(end_name) = stack.last() {
        return Err(NormalizationError::UnpairedEndTag(str::to_owned(end_name)));
    }

    Ok(transform.apply(fragment))
}

#[cfg(test)]
mod tests {
    use crate::parse;

    use super::*;

    #[test]
    fn test_normalize_end_tags_noop() {
        let fragment = parse(
            "\
                <root>\
                    <foo>hello</foo>\
                    <bar>\
                        world<!-- -->!\
                    </bar>\
                </root>\
            ",
        )
        .unwrap();

        let result = normalize_end_tags(fragment.clone()).unwrap();
        assert_eq!(result, fragment);
    }

    #[test]
    fn test_normalize_end_tags_simple() {
        let fragment = parse(
            "\
                <root>\
                    <foo>hello\
                    <bar>world!\
                </root>\
            ",
        )
        .unwrap();

        let result = normalize_end_tags(fragment).unwrap();
        assert_eq!(
            result,
            parse(
                "\
                    <root>\
                        <foo>hello</foo>\
                        <bar>world!</bar>\
                    </root>\
                ",
            )
            .unwrap(),
        );
    }

    #[test]
    fn test_normalize_end_tags_insert_before_whitespace() {
        let fragment = parse(
            "\
                <root>
                    <foo>
                    <bar>hello
                    <baz>
                    <quux>world!<!-- -->
                    <xyzzy>
                </root>\
            ",
        )
        .unwrap();

        let result = normalize_end_tags(fragment).unwrap();
        assert_eq!(
            result,
            parse(
                "\
                <root>
                    <foo></foo>
                    <bar>hello
                    </bar><baz></baz>
                    <quux>world!</quux>
                    <xyzzy></xyzzy>
                </root>\
                ",
            )
            .unwrap(),
        );
    }

    #[test]
    fn test_normalize_end_tags_xml_empty() {
        let fragment = parse(
            "\
                <foo>\
                    <bar/>\
                    <baz>Hello\
                    <foo x='1'/>\
                </foo>\
            ",
        )
        .unwrap();

        let result = normalize_end_tags(fragment).unwrap();
        assert_eq!(
            result,
            parse(
                "\
                    <foo>\
                        <bar></bar>\
                        <baz>Hello</baz>\
                        <foo x='1'></foo>\
                    </foo>\
                "
            )
            .unwrap()
        );
    }

    #[test]
    fn test_normalize_end_tags_unpaired_end() {
        let fragment = parse(
            "\
            <foo>\
                <bar>a</bar>\
                <baz>\
                </bar>\
            </foo>
        ",
        )
        .unwrap();

        assert_eq!(
            normalize_end_tags(fragment),
            Err(NormalizationError::UnpairedEndTag("bar".to_owned()))
        );
    }
}
