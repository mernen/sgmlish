//! Parser combinators for spaces and comments.

use nom::branch::alt;
use nom::bytes::complete::{tag, take_until};
use nom::character::complete::{multispace0, multispace1};
use nom::combinator::recognize;
use nom::error::{ContextError, ParseError};
use nom::multi::many0_count;
use nom::sequence::{delimited, terminated};
use nom::{IResult, Parser};

use super::raw;

/// Outputs all characters until the given delimiter is found,
/// and also consumes the delimiter itself.
///
/// If the delimiter is not found, fails the parser, preventing recovery.
pub fn take_until_terminated<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    ctx: &'static str,
    delimiter: &'static str,
) -> impl FnMut(&'a str) -> IResult<&'a str, &'a str, E> {
    let fail = move |input: &'a str| {
        // On multi-character delimiters, like "]]>", if a partial match occurs
        // (for example the input ends in "]]"), try to identify the largest partial match
        let partial_delimiter_len = (1..delimiter.len())
            .rev()
            .find(|&prefix_len| input.ends_with(&delimiter[..prefix_len]))
            .unwrap_or(0);
        Err(nom::Err::Failure(E::add_context(
            &input[input.len() - partial_delimiter_len..],
            ctx,
            E::from_char(
                &input[input.len()..],
                delimiter.chars().nth(partial_delimiter_len).unwrap(),
            ),
        )))
    };
    alt((terminated(take_until(delimiter), tag(delimiter)), fail))
}

/// Matches zero or more space characters.
pub fn spaces<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&str, &str, E> {
    multispace0(input)
}

/// Matches zero or more comments and spaces.
pub fn comments_and_spaces<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    input: &'a str,
) -> IResult<&str, &str, E> {
    recognize(many0_count(alt((raw::comment_declaration, multispace1))))(input)
}

/// Applies the given parser, then skips spaces that follow.
pub fn strip_spaces_after<'a, O, E: ParseError<&'a str>, F>(f: F) -> impl Parser<&'a str, O, E>
where
    F: Parser<&'a str, O, E>,
{
    terminated(f, spaces)
}

/// Skips spaces before and after the given parser.
pub fn strip_spaces_around<'a, O, E: ParseError<&'a str>, F>(f: F) -> impl Parser<&'a str, O, E>
where
    F: Parser<&'a str, O, E>,
{
    delimited(spaces, f, spaces)
}

/// Applies the given parser, then skips spaces and comments that follow.
pub fn strip_comments_and_spaces_after<'a, O, E: ParseError<&'a str> + ContextError<&'a str>, F>(
    f: F,
) -> impl Parser<&'a str, O, E>
where
    F: Parser<&'a str, O, E>,
{
    terminated(f, comments_and_spaces)
}

#[cfg(test)]
mod tests {
    use nom::bytes::complete::tag;

    use super::*;

    type E<'a> = nom::error::Error<&'a str>;

    #[test]
    fn test_strip_space_after() {
        assert_eq!(
            strip_spaces_after::<_, E, _>(tag("foo")).parse("foo \n bar"),
            Ok(("bar", "foo"))
        );
        assert_eq!(
            strip_spaces_after::<_, E, _>(tag("foo")).parse("foo\t"),
            Ok(("", "foo"))
        );
        assert_eq!(
            strip_spaces_after::<_, E, _>(tag("foo")).parse("foobar"),
            Ok(("bar", "foo"))
        );
    }

    #[test]
    fn test_strip_comments_after() {
        assert_eq!(
            strip_comments_and_spaces_after::<_, E, _>(tag("foo")).parse("foo<!-- comment -->bar"),
            Ok(("bar", "foo"))
        );
        assert_eq!(
            strip_comments_and_spaces_after::<_, E, _>(tag("foo"))
                .parse("foo<!-- a --> <!-- b1 -- -- b2 --><!-- c --> bar"),
            Ok(("bar", "foo"))
        );
        assert_eq!(
            strip_comments_and_spaces_after::<_, E, _>(tag("foo")).parse("foo\t<!-- bar -->"),
            Ok(("", "foo"))
        );
        assert_eq!(
            strip_comments_and_spaces_after::<_, E, _>(tag("foo")).parse("foo \n "),
            Ok(("", "foo"))
        );
        assert_eq!(
            strip_comments_and_spaces_after::<_, E, _>(tag("foo")).parse("foobar"),
            Ok(("bar", "foo"))
        );
    }
}
