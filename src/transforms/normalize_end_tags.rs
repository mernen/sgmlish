use crate::transforms::Transform;
use crate::{text, SgmlEvent, SgmlFragment};

/// The error type in the event tag normalization fails.
///
/// This is returned by [`normalize_end_tags`].
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NormalizationError {
    #[error("unpaired end tag: </{0}>")]
    UnpairedEndTag(String),
    #[error("empty tags (<> and </>) are not supported")]
    EmptyTagNotSupported,
}

/// Inserts omitted end tags, assuming they are only implied for text-only content.
///
/// This is good enough for certain formats, like [OFX] 1.x, but not for others, e.g. [HTML].
///
/// # Notes
///
/// * Tag names are compared in a case-sensitive manner; if your data may mix cases,
///   you can configure your parser with [`lowercase_names`] or [`uppercase_names`] beforehand.
/// * This transform does not support empty start tags (`<>`) or empty end tags (`</>`).
///
/// # Example
///
/// Taking a fragment of (valid) OFX and inserting implied end tags:
///
/// ```rust
/// # use sgmlish::transforms::normalize_end_tags;
/// # fn main() -> sgmlish::Result<()> {
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
/// "##)?;
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
/// "##)?;
///
/// assert_eq!(normalize_end_tags(end_tags_implied)?, normalized);
/// # Ok(())
/// # }
/// ```
///
/// [OFX]: https://en.wikipedia.org/wiki/Open_Financial_Exchange
/// [HTML]: https://en.wikipedia.org/wiki/HTML
/// [`lowercase_names`]: crate::parser::ParserBuilder::lowercase_names
/// [`uppercase_names`]: crate::parser::ParserBuilder::uppercase_names
pub fn normalize_end_tags(mut fragment: SgmlFragment) -> Result<SgmlFragment, NormalizationError> {
    let mut transform = Transform::new();
    let mut stack = vec![];
    let mut next_insertion_point = fragment.len();
    let mut end_xml_empty_element = None;

    for (i, event) in fragment.iter_mut().enumerate().rev() {
        match event {
            SgmlEvent::OpenStartTag { name } | SgmlEvent::EndTag { name } if name.is_empty() => {
                return Err(NormalizationError::EmptyTagNotSupported);
            }
            SgmlEvent::OpenStartTag { name } => {
                let insertion_point = end_xml_empty_element.take().or_else(|| match stack.last() {
                    Some(end_name) if *end_name == name => {
                        stack.pop();
                        None
                    }
                    _ => Some(next_insertion_point),
                });
                if let Some(insertion_point) = insertion_point {
                    transform.insert_at(insertion_point, SgmlEvent::EndTag { name: name.clone() });
                }
                next_insertion_point = i;
            }
            SgmlEvent::XmlCloseEmptyElement => {
                *event = SgmlEvent::CloseStartTag;
                end_xml_empty_element = Some(i + 1);
            }
            SgmlEvent::EndTag { name } => {
                stack.push(name);
                next_insertion_point = i;
            }
            SgmlEvent::Character(text) => {
                if next_insertion_point == i + 1 && text::is_blank(text) {
                    next_insertion_point = i;
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
            r##"
                <root>
                    <foo>hello</foo>
                    <bar>
                        world<!-- -->!
                    </bar>
                </root>
            "##,
        )
        .unwrap();

        let result = normalize_end_tags(fragment.clone()).unwrap();
        assert_eq!(result, fragment);
    }

    #[test]
    fn test_normalize_end_tags_simple() {
        let fragment = parse(
            r##"
                <root>
                    <foo>hello
                    <bar>world!
                </root>
            "##,
        )
        .unwrap();

        let result = normalize_end_tags(fragment).unwrap();
        assert_eq!(
            result,
            parse(
                r##"
                    <root>
                        <foo>hello</foo>
                        <bar>world!</bar>
                    </root>
                "##,
            )
            .unwrap(),
        );
    }

    #[test]
    fn test_normalize_end_tags_insert_before_whitespace() {
        let parser = crate::Parser::builder().trim_whitespace(false).build();
        let fragment = parser
            .parse(
                r##"
                    <root>
                        <foo>
                        <bar>hello
                        <baz>
                        <quux>world!<!-- -->
                        <xyzzy>
                    </root>
                "##,
            )
            .unwrap();

        let expected = parser
            .parse(
                r##"
                    <root>
                        <foo></foo>
                        <bar>hello
                        </bar><baz></baz>
                        <quux>world!</quux>
                        <xyzzy></xyzzy>
                    </root>
                "##,
            )
            .unwrap();
        let result = normalize_end_tags(fragment).unwrap();
        assert_eq!(result, expected,);
    }

    #[test]
    fn test_normalize_end_tags_xml_empty() {
        let fragment = parse(
            r##"
                <foo>
                    <bar/>
                    <baz>Hello
                    <foo x='1'/>
                </foo>
            "##,
        )
        .unwrap();

        let result = normalize_end_tags(fragment).unwrap();
        assert_eq!(
            result,
            parse(
                r##"
                    <foo>
                        <bar></bar>
                        <baz>Hello</baz>
                        <foo x='1'></foo>
                    </foo>
                "##
            )
            .unwrap()
        );
    }

    #[test]
    fn test_normalize_end_tags_unpaired_end() {
        let fragment = parse(
            r##"
                <foo>
                    <bar>a</bar>
                    <baz>
                    </bar>
                </foo>
            "##,
        )
        .unwrap();

        assert_eq!(
            normalize_end_tags(fragment),
            Err(NormalizationError::UnpairedEndTag("bar".to_owned()))
        );
    }
}
