//! Matching SGML tokens and fragments and extracting key parts.
//!
//! This is mainly based on <https://www.w3.org/MarkUp/SGML/productions.html>.

use nom::branch::alt;
use nom::bytes::complete::{is_not, tag, take_until, take_while};
use nom::character::complete::{char, none_of, one_of, satisfy};
use nom::combinator::{cut, map, not, opt, peek, recognize, verify};
use nom::error::{context, ContextError, ParseError};
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
            context(r##"close comment ("-->")"##, cut(char('>'))),
        ))),
    )(input)
}

pub fn comment<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    preceded(
        tag("--"),
        take_until_terminated(r##"close comment ("-->")"##, "--"),
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
                is_not(">\"'[-"),
            )))),
            cut(char('>')),
        ))),
    )(input)
}

pub fn marked_section<'a, E>(input: &'a str) -> IResult<&str, (&str, &str), E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    context(
        "marked section",
        preceded(
            tag("<!["),
            cut(tuple((
                terminated(
                    map(opt(is_not("[]<>!")), |s: Option<&str>| {
                        s.unwrap_or_default().trim_matches(is_sgml_whitespace)
                    }),
                    char('['),
                ),
                take_until_terminated(r##"marked section end ("]]>")"##, "]]>"),
            ))),
        ),
    )(input)
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

pub fn data<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    verify(
        recognize(tuple((
            many0_count(tuple((
                take_until("<"),
                tag("<"),
                not(alt((
                    tag("?"),
                    tag("!-"),
                    tag("!["),
                    preceded(tag("!"), name_start),
                    // Minimally matching start/end tags: "<>", "</>", "<x", or "</x"
                    preceded(opt(tag("/")), alt((name_start, tag(">")))),
                ))),
            ))),
            take_while(|c| c != '<'),
            // take_until("<"),
            // is_not("<"),
        ))),
        |s: &str| !s.is_empty(),
    )(input)
}

pub fn open_start_tag<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    preceded(char('<'), name)(input)
}

pub fn close_start_tag<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    recognize(char('>'))(input)
}

pub fn xml_close_empty_element<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    tag("/>")(input)
}

pub fn empty_start_tag<'a, E>(input: &'a str) -> IResult<&str, &str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    tag("<>")(input)
}

pub fn attribute<'a, E>(input: &'a str) -> IResult<&str, (&str, Option<&str>), E>
where
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
                    cut(alt((unquoted_attribute_value, quoted_attribute_value))),
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

    type E<'a> = nom::error::Error<&'a str>;

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
    fn test_marked_section() {
        assert_eq!(
            marked_section::<E>("<![IGNORE [ lkjsdflkj sdflkj sdflkj  ]]>"),
            Ok(("", ("IGNORE", " lkjsdflkj sdflkj sdflkj  ")))
        );
        assert_eq!(
            marked_section::<E>("<![ %Some.Condition[<x></x>]]>"),
            Ok(("", ("%Some.Condition", "<x></x>")))
        );
        assert_eq!(
            marked_section::<E>("<![CDATA[Hello]] world]]>"),
            Ok(("", ("CDATA", "Hello]] world")))
        );
        assert_eq!(
            marked_section::<E>("<![ %cond;[ ]]>"),
            Ok(("", ("%cond;", " ")))
        );
        assert_eq!(
            marked_section::<E>("<![ RCDATA TEMP []]>"),
            Ok(("", ("RCDATA TEMP", "")))
        );
        assert_eq!(
            marked_section::<E>("<![INCLUDE []]]>"),
            Ok(("", ("INCLUDE", "]")))
        );
        const BODY: &str = r##"
            <!ENTITY % reserved
                "datasrc     %URI;          #IMPLIED  -- a single or tabular Data Source --
                 datafld     CDATA          #IMPLIED  -- the property or column name --
                 dataformatas (plaintext|html) plaintext -- text or html --"
                 >
        "##;
        assert_eq!(
            marked_section::<E>(&format!("<![ %HTML.Reserved; [{}]]>", BODY)),
            Ok(("", ("%HTML.Reserved;", BODY)))
        );
        assert_eq!(marked_section::<E>("<![[]>]]]]]>"), Ok(("", ("", "]>]]]"))));
        assert_eq!(marked_section::<E>("<![ [abc]]>"), Ok(("", ("", "abc"))));
        assert_eq!(marked_section::<E>("<![ []]>"), Ok(("", ("", ""))));
        assert_eq!(marked_section::<E>("<![[abc]]>"), Ok(("", ("", "abc"))));
        assert_eq!(marked_section::<E>("<![[]]>"), Ok(("", ("", ""))));
        marked_section::<E>("<![ IGNORE >").unwrap_err();
        marked_section::<E>("<![ IGNORE []>").unwrap_err();
        marked_section::<E>("<![ IGNORE ]]>").unwrap_err();
        marked_section::<E>("<![ IGNORE [] ]>").unwrap_err();
        marked_section::<E>("<![ IGNORE []] >").unwrap_err();
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
    fn test_data() {
        assert_eq!(data::<E>("foo"), Ok(("", "foo")));
        assert_eq!(data::<E>("foo>"), Ok(("", "foo>")));

        assert_eq!(data::<E>("foo<x"), Ok(("<x", "foo")));
        assert_eq!(data::<E>("foo<bar>"), Ok(("<bar>", "foo")));
        assert_eq!(data::<E>("foo<>"), Ok(("<>", "foo")));
        assert_eq!(data::<E>("foo</x"), Ok(("</x", "foo")));
        assert_eq!(data::<E>("foo</>"), Ok(("</>", "foo")));

        assert_eq!(data::<E>("foo<"), Ok(("", "foo<")));
        assert_eq!(data::<E>("foo< "), Ok(("", "foo< ")));
        assert_eq!(data::<E>("foo<3"), Ok(("", "foo<3")));
        assert_eq!(data::<E>("foo< x"), Ok(("", "foo< x")));

        assert_eq!(data::<E>("foo<<"), Ok(("", "foo<<")));
        assert_eq!(data::<E>("foo<<<"), Ok(("", "foo<<<")));
        assert_eq!(data::<E>("foo<<<x"), Ok(("<x", "foo<<")));
        assert_eq!(data::<E>("foo<1<2<three<4"), Ok(("<three<4", "foo<1<2")));
        assert_eq!(data::<E>("foo<<x<"), Ok(("<x<", "foo<")));
        assert_eq!(data::<E>("<123"), Ok(("", "<123")));
        assert_eq!(data::<E>("<123</x"), Ok(("</x", "<123")));

        assert_eq!(data::<E>("foo<!"), Ok(("", "foo<!")));
        assert_eq!(data::<E>("foo<! "), Ok(("", "foo<! ")));
        assert_eq!(data::<E>("foo<!-"), Ok(("<!-", "foo")));
        assert_eq!(data::<E>("foo<!x"), Ok(("<!x", "foo")));
        assert_eq!(data::<E>("foo<!["), Ok(("<![", "foo")));
        assert_eq!(data::<E>("foo<! x"), Ok(("", "foo<! x")));
        assert_eq!(data::<E>("foo<! -"), Ok(("", "foo<! -")));
        assert_eq!(data::<E>("foo<! ["), Ok(("", "foo<! [")));

        data::<E>("<foo").unwrap_err();
        data::<E>("</foo>").unwrap_err();
    }
}
