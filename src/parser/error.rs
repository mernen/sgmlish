use std::fmt;
use std::ops::Deref;

/// A [`nom`]-compatible error type that captures relevant information
/// for the SGML parser.
#[derive(Debug)]
pub struct ContextualizedError<I> {
    /// The remaining input when the error occurred.
    pub input: I,
    /// Was a certain character expected?
    pub char: Option<char>,
    pub error: Option<crate::Error>,
    /// The collected context.
    pub context: Vec<(I, &'static str)>,
}

impl<I: Deref<Target = str>> ContextualizedError<I> {
    /// Returns a string describing this error.
    pub fn describe(&self, input: &I) -> String {
        let mut out = String::new();
        self.describe_to(input, &mut out).unwrap();
        out
    }

    /// Writes the detailed description of this error to the given output.
    pub fn describe_to<W: fmt::Write>(&self, input: &I, mut f: W) -> fmt::Result {
        if input.is_empty() {
            return f.write_str("parse error: input is empty");
        }

        let mut context = self
            .context
            .iter()
            .map(|(substring, ctx)| (ctx, LocatedLine::locate(input, substring)))
            .peekable();

        let location = LocatedLine::locate(input, &self.input);
        write!(f, "parse error ")?;
        if let Some((ctx, ..)) = context.next_if(|(_, ctxloc)| *ctxloc == location) {
            write!(f, "in {}, ", ctx)?;
        }
        write!(f, "at line {}:", location.line_number)?;
        if let Some(err) = &self.error {
            write!(f, " {}", err)?;
        }
        if let Some(c) = self.char {
            write!(f, " expected '{}', got ", c)?;
            match self.input.chars().next() {
                Some(' ' | '\t') => write!(f, "whitespace")?,
                Some('\r' | '\n') => write!(f, "end of line")?,
                Some(c) => write!(f, "'{}'", c.escape_default())?,
                None => write!(f, "end of input")?,
            }
        }
        writeln!(f, "\n{}", location)?;

        let mut last_loc = location;
        for (ctx, ctxloc) in context {
            if ctxloc == last_loc {
                // Avoid pointing multiple times to the same location
                continue;
            }
            writeln!(
                f,
                "From {ctx}, started at {lineref}line {number}:\n{line}",
                ctx = ctx,
                lineref = if ctxloc.line_number == last_loc.line_number {
                    "the same "
                } else {
                    ""
                },
                number = ctxloc.line_number,
                line = ctxloc,
            )?;
            last_loc = ctxloc;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LocatedLine<'a> {
    // The contents of the line, without trailing newline characters.
    line: &'a str,
    // Line number, starting at 1.
    line_number: usize,
    // Column number, starting at 1.
    column_number: usize,
}

impl<'a> LocatedLine<'a> {
    fn locate(input: &'a str, substring: &'a str) -> Self {
        use nom::Offset;

        let offset = input.offset(substring);
        let input_before = &input[..offset];

        let line_start_offset = input_before.rfind('\n').map(|n| n + 1).unwrap_or(0);
        let line_number = input_before.split('\n').count();
        let column_number = offset - line_start_offset + 1;

        LocatedLine {
            line: input[line_start_offset..].lines().next().unwrap_or(""),
            line_number,
            column_number,
        }
    }
}

impl<'a> fmt::Display for LocatedLine<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let skip_line_start = self.column_number.saturating_sub(40);
        let mut max_len = 80;

        let mut indices = self.line.char_indices().map(|(index, _)| index);
        let mut display_range = 0..self.line.len();
        if skip_line_start > 0 {
            display_range.start = indices.nth(skip_line_start + 3).unwrap();
            max_len -= 3;
        } else {
            indices.next();
        }
        if let Some(cut_pos) = indices.nth(max_len - 4) {
            if indices.nth(2).is_some() {
                display_range.end = cut_pos;
            }
        }

        if display_range.start > 0 {
            f.write_str("...")?;
        }
        f.write_str(&self.line[display_range.clone()])?;
        if display_range.end < self.line.len() {
            f.write_str("...")?;
        }

        write!(
            f,
            "\n{caret:>col$}",
            caret = "^",
            col = self.column_number - skip_line_start
        )
    }
}

impl<I> nom::error::ParseError<I> for ContextualizedError<I> {
    fn from_error_kind(input: I, _kind: nom::error::ErrorKind) -> Self {
        ContextualizedError {
            input,
            char: None,
            error: None,
            context: vec![],
        }
    }

    fn append(_input: I, _kind: nom::error::ErrorKind, other: Self) -> Self {
        other
    }

    fn from_char(input: I, c: char) -> Self {
        ContextualizedError {
            input,
            char: Some(c),
            error: None,
            context: vec![],
        }
    }
}

impl<I> nom::error::ContextError<I> for ContextualizedError<I> {
    fn add_context(input: I, ctx: &'static str, mut other: Self) -> Self {
        other.context.push((input, ctx));
        other
    }
}

impl<I> nom::error::FromExternalError<I, crate::Error> for ContextualizedError<I> {
    fn from_external_error(input: I, _kind: nom::error::ErrorKind, e: crate::Error) -> Self {
        ContextualizedError {
            input,
            char: None,
            error: Some(e),
            context: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locate() {
        let input = "hello\nworld\n";
        assert_eq!(
            LocatedLine::locate(input, &input[..]),
            LocatedLine {
                line: "hello",
                line_number: 1,
                column_number: 1
            }
        );
        assert_eq!(
            LocatedLine::locate(input, &input[1..]),
            LocatedLine {
                line: "hello",
                line_number: 1,
                column_number: 2
            }
        );
        assert_eq!(
            LocatedLine::locate(input, &input[4..]),
            LocatedLine {
                line: "hello",
                line_number: 1,
                column_number: 5
            }
        );
        assert_eq!(
            LocatedLine::locate(input, &input[5..]),
            LocatedLine {
                line: "hello",
                line_number: 1,
                column_number: 6
            }
        );
        assert_eq!(
            LocatedLine::locate(input, &input[6..]),
            LocatedLine {
                line: "world",
                line_number: 2,
                column_number: 1
            }
        );
        assert_eq!(
            LocatedLine::locate(input, &input[7..]),
            LocatedLine {
                line: "world",
                line_number: 2,
                column_number: 2
            }
        );
        assert_eq!(
            LocatedLine::locate(input, &input[10..]),
            LocatedLine {
                line: "world",
                line_number: 2,
                column_number: 5
            }
        );
        assert_eq!(
            LocatedLine::locate(input, &input[11..]),
            LocatedLine {
                line: "world",
                line_number: 2,
                column_number: 6
            }
        );
        assert_eq!(
            LocatedLine::locate(input, &input[12..]),
            LocatedLine {
                line: "",
                line_number: 3,
                column_number: 1
            }
        );
    }

    #[test]
    fn test_located_line_display_short() {
        let line = "hello";

        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 1,
            }
            .to_string(),
            "hello\n^"
        );
        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 2,
            }
            .to_string(),
            "hello\n ^"
        );
        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 6,
            }
            .to_string(),
            "hello\n     ^"
        );
    }

    #[test]
    fn test_display_long_prefix() {
        let line = "thîs line is fáirly long, and we may not want to output it from the start";

        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 1,
            }
            .to_string(),
            format!("{}\n^", line)
        );
        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 10,
            }
            .to_string(),
            format!("{}\n         ^", line)
        );
        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 40,
            }
            .to_string(),
            concat!(
                "thîs line is fáirly long, and we may not want to output it from the start\n",
                "                                       ^",
            )
        );

        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 41,
            }
            .to_string(),
            concat!(
                "... line is fáirly long, and we may not want to output it from the start\n",
                "                                       ^",
            )
        );

        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 52,
            }
            .to_string(),
            concat!(
                "...irly long, and we may not want to output it from the start\n",
                "                                       ^",
            )
        );
    }

    #[test]
    fn test_display_long_suffix() {
        let line =
            "this line has precisely eighty characters. It should be printed in its entirety.";
        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 15,
            }
            .to_string(),
            format!("{}\n{:>15}", line, "^")
        );

        let line =
            "this line has exactly eighty-one characters. ¡Demás! Should be clamped to eighty.";
        assert_eq!(
            LocatedLine{line, line_number:1, column_number: 15,}.to_string(),
            concat!(
                "this line has exactly eighty-one characters. ¡Demás! Should be clamped to eig...\n",
                "              ^",
            )
        );
    }

    #[test]
    fn test_display_long_prefix_suffix() {
        let line = "this line is quite lóng, and printing too many characters after the point of interest may not be very useful";

        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 1,
            }
            .to_string(),
            concat!(
                "this line is quite lóng, and printing too many characters after the point of ...\n",
                "^",
            )
        );

        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 40,
            }
            .to_string(),
            concat!(
                "this line is quite lóng, and printing too many characters after the point of ...\n",
                "                                       ^",
            )
        );

        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 41,
            }
            .to_string(),
            concat!(
                "... line is quite lóng, and printing too many characters after the point of i...\n",
                "                                       ^",
            )
        );

        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 66,
            }
            .to_string(),
            concat!(
                "...printing too many characters after the point of interest may not be very u...\n",
                "                                       ^",
            )
        );

        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 67,
            }
            .to_string(),
                concat!(
                "...rinting too many characters after the point of interest may not be very us...\n",
                "                                       ^",
            )
        );

        assert_eq!(
            LocatedLine {
                line,
                line_number: 1,
                column_number: 68,
            }
            .to_string(),
            concat!(
                "...inting too many characters after the point of interest may not be very useful\n",
                "                                       ^",
            )
        );
    }
}
