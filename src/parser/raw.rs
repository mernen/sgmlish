//! Matching SGML tokens and fragments and extracting key parts.
//!
//! This is mainly based on <https://www.w3.org/MarkUp/SGML/productions.html>.

use nom::branch::alt;
use nom::bytes::complete::{is_not, tag, take_till};
use nom::character::complete::{char, none_of, one_of, satisfy};
use nom::combinator::{cut, map, map_parser, not, opt, peek, recognize, verify};
use nom::error::{context, ContextError, ErrorKind, ParseError};
use nom::multi::many0_count;
use nom::sequence::{delimited, pair, preceded, terminated, tuple};
use nom::IResult;

use crate::is_sgml_whitespace;

use super::util::{spaces, strip_spaces_after, strip_spaces_around, take_until_terminated};

pub fn comment_declaration<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    context(
        "comment declaration",
        recognize(tuple((
            tag("<!"),
            peek(one_of("->")),
            cut(opt(pair(comment, many0_count(pair(spaces, comment))))),
            context(r##"comment declaration close ("-->")"##, cut(char('>'))),
        ))),
    )(input)
}

pub fn comment<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    preceded(
        tag("--"),
        take_until_terminated(r##"comment declaration close ("-->")"##, "--"),
    )(input)
}

pub fn markup_declaration<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    context(
        "markup declaration",
        recognize(tuple((
            tag("<!"),
            name,
            cut(many0_count(alt((
                comment,
                quoted_attribute_value,
                declaration_subset,
                // Accept single "-"
                terminated(tag("-"), not(tag("-"))),
                is_not("<>\"'[-"),
            )))),
            cut(char('>')),
        ))),
    )(input)
}

pub fn marked_section_start_and_keyword<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    preceded(
        tag("<!["),
        cut(terminated(
            map(
                take_till(|c| matches!(c, '[' | ']' | '<' | '>' | '!')),
                |s: &str| s.trim_matches(is_sgml_whitespace),
            ),
            char('['),
        )),
    )(input)
}

/// Matches the content for `CDATA` and `RCDATA` marked sections, immediately after [`marked_section_start_and_keyword`].
///
/// These sections do nest, meaning they end on the first `]]>` found.
pub fn marked_section_body_character<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    take_until_terminated(r##"marked section end ("]]>")"##, "]]>")(input)
}

/// Matches the content for `IGNORE` marked sections, immediately after [`marked_section_start_and_keyword`].
///
/// The content of `IGNORE` marked sections will match `<![` and `]]>` pairs,
/// stopping on the first unmatched `]]>` found.
pub fn marked_section_body_ignore<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    use nom::{FindSubstring, Parser, Slice};
    const START: &str = "<![";
    const END: &str = "]]>";
    let (close_suffix, close_match) =
        take_until_terminated(r##"end of marked section ("]]>")"##, END).parse(input)?;
    match input.find_substring(START) {
        Some(n) if n < close_match.len() => {
            let (suffix_after_matched_pair, _) = context(
                "nested marked section",
                marked_section_body_ignore,
            )(input.slice(n + START.len()..))?;
            let (final_suffix, _) = marked_section_body_ignore(suffix_after_matched_pair)?;
            Ok((
                final_suffix,
                input.slice(..input.len() - final_suffix.len() - END.len()),
            ))
        }
        _ => Ok((close_suffix, close_match)),
    }
}

pub fn marked_section_end<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    tag("]]>")(input)
}

fn declaration_subset<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    context(
        "declaration subset",
        recognize(delimited(
            char('['),
            many0_count(alt((
                quoted_attribute_value,
                declaration_subset,
                markup_declaration,
                is_not("<>\"'[]"),
            ))),
            cut(char(']')),
        )),
    )(input)
}

pub fn processing_instruction<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    context(
        "processing instruction",
        recognize(preceded(
            tag("<?"),
            take_until_terminated(r#"processing instruction end (">")"#, ">"),
        )),
    )(input)
}

/// Matches character sequences.
pub fn text<'a, E>(input: &'a str, mse: MarkedSectionEndHandling) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    verify(
        recognize(tuple((
            opt(|input| plain_text(input, mse)),
            many0_count(tuple((
                tag("<"),
                not(alt((
                    tag("?"),
                    tag("!-"),
                    tag("!["),
                    preceded(tag("!"), name_start),
                    // Minimally matching start/end tags: "<>", "</>", "<x", or "</x"
                    preceded(opt(tag("/")), alt((name_start, tag(">")))),
                ))),
                opt(|input| plain_text(input, mse)),
            ))),
        ))),
        |s: &str| !s.is_empty(),
    )(input)
}

/// Matches until the first `<` (or `]]>` in [`MarkedSectionEndHandling::StopParsing`] mode).
pub fn plain_text<'a, E>(input: &'a str, mse: MarkedSectionEndHandling) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    use nom::{FindSubstring, InputTake};
    let next_tag = input.find_substring("<").unwrap_or(input.len());
    let split_pos = match mse {
        MarkedSectionEndHandling::StopParsing => input
            .find_substring("]]>")
            .unwrap_or(next_tag)
            .min(next_tag),
        MarkedSectionEndHandling::TreatAsText => next_tag,
    };
    if split_pos == 0 {
        Err(nom::Err::Error(E::from_error_kind(
            input,
            ErrorKind::TakeUntil,
        )))
    } else {
        Ok(input.take_split(split_pos))
    }
}

/// Defines how [`marked_section_end`] sequences should be handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkedSectionEndHandling {
    /// Treat occurrences of `]]>` as plain text.
    TreatAsText,
    /// Stop parsing when an occurrence of `]]>` is found.
    StopParsing,
}

/// Matches `<foo` and outputs `foo`.
pub fn open_start_tag<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    preceded(char('<'), name)(input)
}

/// Matches `>` and outputs it.
pub fn close_start_tag<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    recognize(char('>'))(input)
}

/// Matches `/>` and outputs it.
pub fn xml_close_empty_element<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    tag("/>")(input)
}

/// Matches `<>` and outputs it.
pub fn empty_start_tag<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    tag("<>")(input)
}

/// Matches an attribute key-value pair.
pub fn attribute<'a, E>(input: &'a str) -> IResult<&str, (&str, Option<&str>), E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    attribute_parse_value(input, Ok)
}

/// Matches an attribute key-value pair, parses the value (if present) with
/// the given closure, and outputs the key and parsed value.
pub fn attribute_parse_value<'a, F, T, E>(
    input: &'a str,
    mut f: F,
) -> IResult<&'a str, (&'a str, Option<T>), E>
where
    F: FnMut(&'a str) -> Result<T, nom::Err<E>>,
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    context(
        "attribute",
        pair(
            name,
            opt(preceded(
                strip_spaces_around(char('=')),
                context(
                    "attribute value",
                    cut(map_parser(attribute_value, move |input| {
                        f(input).map(|value| ("", value))
                    })),
                ),
            )),
        ),
    )(input)
}

pub fn attribute_value<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    alt((unquoted_attribute_value, quoted_attribute_value))(input)
}

pub fn unquoted_attribute_value<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    preceded(peek(none_of("\"'")), is_not("\"'> \t\r\n"))(input)
}

fn quoted_attribute_value<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    let delimited_by =
        |c, terminator, ctx| preceded(char(c), take_until_terminated(ctx, terminator));
    alt((
        delimited_by('\'', "'", "closing '"),
        delimited_by('"', "\"", "closing \""),
    ))(input)
}

pub fn end_tag<'a, E>(input: &'a str) -> IResult<&str, Option<&str>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    context(
        "end tag",
        delimited(tag("</"), opt(strip_spaces_after(name)), cut(char('>'))),
    )(input)
}

pub fn name<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    recognize(pair(name_start, many0_count(satisfy(is_name_char))))(input)
}

pub fn name_start<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    recognize(satisfy(is_name_start_char))(input)
}

pub fn is_name_start_char(c: char) -> bool {
    c.is_alphabetic()
}

pub fn is_name_char(c: char) -> bool {
    // Using LCNMCHAR and UCNMCHAR as defined by HTML4
    c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ':')
}

#[cfg(test)]
mod tests {
    use super::*;

    type E<'a> = nom::error::VerboseError<&'a str>;

    use MarkedSectionEndHandling::*;
    const MSE_MODES: [MarkedSectionEndHandling; 2] = [TreatAsText, StopParsing];

    #[test]
    fn test_comment_declaration() {
        fn accept(decl: &str) {
            assert_eq!(comment_declaration::<E>(decl), Ok(("", decl)));
        }

        accept("<!>");
        accept("<!--comment-->");
        accept("<!-- comment 1 ---- comment 2-->");
        accept("<!-- comment 1 -- \n -- comment 2-->");

        comment_declaration::<E>("<! >").unwrap_err();
        comment_declaration::<E>("<! -- comment -->").unwrap_err();
        comment_declaration::<E>("<!-- comment -- >").unwrap_err();
    }

    #[test]
    fn test_markup_declaration() {
        fn accept(decl: &str) {
            assert_eq!(markup_declaration::<E>(decl), Ok(("", decl)));
        }

        accept("<!DOCTYPE html>");
        accept(
            r#"<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01 Transitional//EN" "http://www.w3.org/TR/html4/loose.dtd">"#,
        );
        accept("<!doctype doc [ <!element p - - ANY> ]>");
        accept(
            r##"<!ENTITY % HTMLsymbol PUBLIC
                "-//W3C//ENTITIES Symbols//EN//HTML"
                "HTMLsymbol.ent">"##,
        );
        accept(r##"<!entity ccedil "ç">"##);
        accept(
            r##"<!ATTLIST (TH|TD)
                %attrs;             -- this is a "comment --
                foo %URI; #IMPLIED  -- don't match quotes here --
            >"##,
        );

        markup_declaration::<E>("<! doctype>").unwrap_err();
        markup_declaration::<E>("< !doctype>").unwrap_err();
    }

    #[test]
    fn test_marked_section_start() {
        assert_eq!(
            marked_section_start_and_keyword::<E>("<![IGNORE [ lkjsdflkj sdflkj sdflkj  ]]>"),
            Ok((" lkjsdflkj sdflkj sdflkj  ]]>", "IGNORE"))
        );
        assert_eq!(
            marked_section_start_and_keyword::<E>("<![ %Some.Condition[<x></x>]]>"),
            Ok(("<x></x>]]>", "%Some.Condition"))
        );
        assert_eq!(
            marked_section_start_and_keyword::<E>("<![CDATA[Hello]] world]]>"),
            Ok(("Hello]] world]]>", "CDATA"))
        );
        assert_eq!(
            marked_section_start_and_keyword::<E>("<![ %cond;[ ]]>"),
            Ok((" ]]>", "%cond;"))
        );
        assert_eq!(
            marked_section_start_and_keyword::<E>("<![ RCDATA TEMP [ "),
            Ok((" ", "RCDATA TEMP"))
        );
        assert_eq!(
            marked_section_start_and_keyword::<E>("<![[]>]]]]]>"),
            Ok(("]>]]]]]>", ""))
        );
        assert_eq!(
            marked_section_start_and_keyword::<E>("<![ [abc]]>"),
            Ok(("abc]]>", ""))
        );
        marked_section_start_and_keyword::<E>("<![ IGNORE <").unwrap_err();
        marked_section_start_and_keyword::<E>("<![ IGNORE >").unwrap_err();
        marked_section_start_and_keyword::<E>("<![ IGNORE ]]>").unwrap_err();
    }

    #[test]
    fn test_marked_section_body_character() {
        assert_eq!(marked_section_body_character::<E>("]]>"), Ok(("", "")));
        assert_eq!(marked_section_body_character::<E>(" ]]>"), Ok(("", " ")));
        assert_eq!(
            marked_section_body_character::<E>("hello<![CDATA[world]]>]]>"),
            Ok(("]]>", "hello<![CDATA[world")),
        );
        marked_section_body_character::<E>("").unwrap_err();
        marked_section_body_character::<E>(" ").unwrap_err();
        marked_section_body_character::<E>("]]").unwrap_err();
        marked_section_body_character::<E>("]] >").unwrap_err();
        marked_section_body_character::<E>("] ]>").unwrap_err();
    }

    #[test]
    fn test_marked_section_body_ignore() {
        assert_eq!(marked_section_body_ignore::<E>("]]>"), Ok(("", "")));
        assert_eq!(marked_section_body_ignore::<E>(" ]]>"), Ok(("", " ")));
        assert_eq!(
            marked_section_body_ignore::<E>(" hello world ]]> "),
            Ok((" ", " hello world ")),
        );
        assert_eq!(
            marked_section_body_ignore::<E>("<IMG ALT=']]>'>"),
            Ok(("'>", "<IMG ALT='")),
        );
        assert_eq!(
            marked_section_body_ignore::<E>("<!-- ]]> -->"),
            Ok((" -->", "<!-- ")),
        );
        assert_eq!(
            marked_section_body_ignore::<E>(r##"<!DOCTYPE "example]]>">"##),
            Ok(("\">", "<!DOCTYPE \"example")),
        );
        assert_eq!(
            marked_section_body_ignore::<E>("hello]]world]]>]]>"),
            Ok(("]]>", "hello]]world")),
        );
        assert_eq!(
            marked_section_body_ignore::<E>("hello]]>world]]>]]>"),
            Ok(("world]]>]]>", "hello")),
        );
        assert_eq!(
            marked_section_body_ignore::<E>("<![hello]]> world]]><![[]]>"),
            Ok(("<![[]]>", "<![hello]]> world")),
        );
        assert_eq!(
            marked_section_body_ignore::<E>(
                "<!] <![CDATA[hello]]> <![[CDATA[<![[world]]>]]>]]><![CDATA[!]]>"
            ),
            Ok((
                "<![CDATA[!]]>",
                "<!] <![CDATA[hello]]> <![[CDATA[<![[world]]>]]>",
            ))
        );
        assert_eq!(
            marked_section_body_ignore::<E>(
                "<!] <![CDATA[hello]]> <!]]><![[CDATA[<![[world]]>]]><![CDATA[!]]>"
            ),
            Ok((
                "<![[CDATA[<![[world]]>]]><![CDATA[!]]>",
                "<!] <![CDATA[hello]]> <!",
            ))
        );
        marked_section_body_ignore::<E>("").unwrap_err();
        marked_section_body_ignore::<E>("hello").unwrap_err();
        marked_section_body_ignore::<E>("]>").unwrap_err();
        marked_section_body_ignore::<E>("<![").unwrap_err();
        marked_section_body_ignore::<E>("<![]]>").unwrap_err();
        marked_section_body_ignore::<E>(
            "<!] <![CDATA[hello]]> <![[CDATA[<![[world]]>]]><![CDATA[!]]>",
        )
        .unwrap_err();
    }

    #[test]
    fn test_processing_instruction() {
        fn accept(decl: &str, rest: &str) {
            assert_eq!(
                processing_instruction::<E>(decl),
                Ok((rest, &decl[..decl.len() - rest.len()]))
            );
        }

        accept("<?> ", " ");
        accept("<?style tt = font courier>\n", "\n");
        accept("<?/experiment>", "");
        // XML-style processing instructions are not strictly SGML, but welp
        accept(
            r#"<?xml-stylesheet href="example.xslt" type="text/xsl"?>>"#,
            ">",
        );

        processing_instruction::<E>("< ?>").unwrap_err();
    }

    #[test]
    fn test_start_tag_empty() {
        assert_eq!(empty_start_tag::<E>("<> ok"), Ok((" ok", "<>")));

        empty_start_tag::<E>("< a>").unwrap_err();
    }

    #[test]
    fn test_attribute() {
        assert_eq!(attribute::<E>("foo=bar"), Ok(("", ("foo", Some("bar")))));
        assert_eq!(attribute::<E>("foo = bar"), Ok(("", ("foo", Some("bar")))));
        assert_eq!(attribute::<E>("foo = 123"), Ok(("", ("foo", Some("123")))));
        assert_eq!(
            attribute::<E>("foo= #ff0000"),
            Ok(("", ("foo", Some("#ff0000"))))
        );
        assert_eq!(attribute::<E>("checked "), Ok((" ", ("checked", None))));
        assert_eq!(attribute::<E>("usemap>"), Ok((">", ("usemap", None))));
        assert_eq!(
            attribute::<E>("foo='quoted \">'"),
            Ok(("", ("foo", Some("quoted \">"))))
        );
        assert_eq!(
            attribute::<E>("foo = \"quoted '>\""),
            Ok(("", ("foo", Some("quoted '>"))))
        );
        assert_eq!(
            attribute::<E>("foo = \"quoted \">\""),
            Ok((">\"", ("foo", Some("quoted "))))
        );
        assert_eq!(
            attribute::<E>("foo='<!-- comment' -->"),
            Ok((" -->", ("foo", Some("<!-- comment"))))
        );
        assert_eq!(
            attribute::<E>("foo='<!SGML \"ex'ample\">"),
            Ok(("ample\">", ("foo", Some("<!SGML \"ex"))))
        );
        assert_eq!(
            attribute::<E>("foo=\"<![IGNORE[x\"]]>"),
            Ok(("]]>", ("foo", Some("<![IGNORE[x"))))
        );
        assert_eq!(
            attribute::<E>("foo = <bar>"),
            Ok((">", ("foo", Some("<bar"))))
        );
        assert_eq!(
            attribute::<E>("foo = value'>"),
            Ok(("'>", ("foo", Some("value"))))
        );
        attribute::<E>("foo='value").unwrap_err();
        attribute::<E>("foo=\"value").unwrap_err();
        attribute::<E>("foo =").unwrap_err();
        attribute::<E>("foo = >").unwrap_err();
    }

    #[test]
    fn test_end_tag() {
        assert_eq!(end_tag::<E>("</x>"), Ok(("", Some("x"))));
        assert_eq!(end_tag::<E>("</foo\n>"), Ok(("", Some("foo"))));
        assert_eq!(end_tag::<E>("</>"), Ok(("", None)));
        end_tag::<E>("< /foo>").unwrap_err();
        end_tag::<E>("</ foo>").unwrap_err();
        end_tag::<E>("</ >").unwrap_err();
    }

    #[test]
    fn test_text() {
        for eom in MSE_MODES {
            assert_eq!(text::<E>("foo", eom), Ok(("", "foo")));
            assert_eq!(text::<E>("foo>", eom), Ok(("", "foo>")));

            assert_eq!(text::<E>("foo<x", eom), Ok(("<x", "foo")));
            assert_eq!(text::<E>("foo<bar>", eom), Ok(("<bar>", "foo")));
            assert_eq!(text::<E>("foo<>", eom), Ok(("<>", "foo")));
            assert_eq!(text::<E>("foo</x", eom), Ok(("</x", "foo")));
            assert_eq!(text::<E>("foo</>", eom), Ok(("</>", "foo")));

            assert_eq!(text::<E>("foo<", eom), Ok(("", "foo<")));
            assert_eq!(text::<E>("foo< ", eom), Ok(("", "foo< ")));
            assert_eq!(text::<E>("foo<3", eom), Ok(("", "foo<3")));
            assert_eq!(text::<E>("foo< x", eom), Ok(("", "foo< x")));

            assert_eq!(text::<E>("foo<<", eom), Ok(("", "foo<<")));
            assert_eq!(text::<E>("foo<<<", eom), Ok(("", "foo<<<")));
            assert_eq!(text::<E>("foo<<<x", eom), Ok(("<x", "foo<<")));
            assert_eq!(
                text::<E>("foo<1<2<three<4", eom),
                Ok(("<three<4", "foo<1<2"))
            );
            assert_eq!(text::<E>("foo<<x<", eom), Ok(("<x<", "foo<")));
            assert_eq!(text::<E>("<123", eom), Ok(("", "<123")));
            assert_eq!(text::<E>("<123</x", eom), Ok(("</x", "<123")));

            assert_eq!(text::<E>("foo<!", eom), Ok(("", "foo<!")));
            assert_eq!(text::<E>("foo<! ", eom), Ok(("", "foo<! ")));
            assert_eq!(text::<E>("foo<!-", eom), Ok(("<!-", "foo")));
            assert_eq!(text::<E>("foo<!x", eom), Ok(("<!x", "foo")));
            assert_eq!(text::<E>("foo<![", eom), Ok(("<![", "foo")));
            assert_eq!(text::<E>("foo<! x", eom), Ok(("", "foo<! x")));
            assert_eq!(text::<E>("foo<! -", eom), Ok(("", "foo<! -")));
            assert_eq!(text::<E>("foo<! [", eom), Ok(("", "foo<! [")));

            text::<E>("<foo", eom).unwrap_err();
            text::<E>("</foo>", eom).unwrap_err();
        }

        assert_eq!(
            text::<E>("foo<]]><bar>", TreatAsText),
            Ok(("<bar>", "foo<]]>"))
        );
        assert_eq!(text::<E>("]]><bar>", TreatAsText), Ok(("<bar>", "]]>")));

        assert_eq!(
            text::<E>("foo<]]><bar>", StopParsing),
            Ok(("]]><bar>", "foo<"))
        );
        text::<E>("]]><bar>", StopParsing).unwrap_err();
    }

    #[test]
    fn test_plain_text() {
        for eom in MSE_MODES {
            assert_eq!(plain_text::<E>("x<", eom), Ok(("<", "x")));
            assert_eq!(plain_text::<E>("x<foo", eom), Ok(("<foo", "x")));
            plain_text::<E>("<foo", eom).unwrap_err();
            plain_text::<E>("<#", eom).unwrap_err();
            plain_text::<E>("<", eom).unwrap_err();
            assert_eq!(plain_text::<E>("a<b]]>c", eom), Ok(("<b]]>c", "a")));
            plain_text::<E>("<]]>", eom).unwrap_err();
        }

        assert_eq!(plain_text::<E>("x]]>", TreatAsText), Ok(("", "x]]>")));
        assert_eq!(plain_text::<E>("x]]>", StopParsing), Ok(("]]>", "x")));
        assert_eq!(plain_text::<E>("a]]>b<c", TreatAsText), Ok(("<c", "a]]>b")));
        assert_eq!(plain_text::<E>("a]]>b<c", StopParsing), Ok(("]]>b<c", "a")));

        assert_eq!(plain_text::<E>("]]>", TreatAsText), Ok(("", "]]>")));
        plain_text::<E>("]]>", StopParsing).unwrap_err();
        assert_eq!(plain_text::<E>("]]><", TreatAsText), Ok(("<", "]]>")));
        plain_text::<E>("]]><", StopParsing).unwrap_err();
    }
}
