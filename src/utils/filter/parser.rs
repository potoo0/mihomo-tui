#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    pub field: Option<String>,
    pub pattern: String,
    pub quoted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenizeError {
    UnterminatedQuote { pos: usize },
    InvalidEscape { pos: usize, ch: char },
}

pub fn parse_filter_expr(input: &str) -> Result<Vec<Term>, TokenizeError> {
    Tokenizer::new(input.trim()).tokenize()?.into_iter().map(parse_token).collect()
}

fn parse_token(token: Token) -> Result<Term, TokenizeError> {
    match token.colon {
        Some(colon) if colon > 0 => Ok(Term {
            field: Some(token.text[..colon].to_owned()),
            pattern: token.text[colon + 1..].to_owned(),
            quoted: token.quoted,
        }),
        Some(0) => {
            Ok(Term { field: None, pattern: token.text[1..].to_owned(), quoted: token.quoted })
        }
        _ => Ok(Term { field: None, pattern: token.text, quoted: token.quoted }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    /// Byte offset where this token starts in the trimmed input.
    pos: usize,
    /// First colon outside quotes, stored as a byte offset in `text`.
    colon: Option<usize>,
    quoted: bool,
}

struct Tokenizer<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Self { chars: input.char_indices().peekable() }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, TokenizeError> {
        let mut tokens = Vec::new();

        loop {
            while matches!(self.chars.peek(), Some((_, ch)) if ch.is_whitespace()) {
                self.chars.next();
            }

            let Some((pos, _)) = self.chars.peek().copied() else {
                break;
            };

            tokens.push(self.read_token(pos)?);
        }

        Ok(tokens)
    }

    fn read_token(&mut self, token_pos: usize) -> Result<Token, TokenizeError> {
        let mut token = Token { text: String::new(), pos: token_pos, colon: None, quoted: false };

        while let Some((pos, ch)) = self.chars.next() {
            if ch.is_whitespace() {
                break;
            }

            self.push_token_char(&mut token, pos, ch)?;
        }

        Ok(token)
    }

    fn push_token_char(
        &mut self,
        token: &mut Token,
        pos: usize,
        ch: char,
    ) -> Result<(), TokenizeError> {
        match ch {
            '"' => self.read_quoted(token, pos),

            ':' => {
                if token.colon.is_none() {
                    token.colon = Some(token.text.len());
                }

                token.text.push(':');
                Ok(())
            }

            ch => {
                token.text.push(ch);
                Ok(())
            }
        }
    }

    fn read_quoted(&mut self, token: &mut Token, quote_pos: usize) -> Result<(), TokenizeError> {
        token.quoted = true;
        while let Some((_, ch)) = self.chars.next() {
            match ch {
                '"' => return Ok(()),

                '\\' => {
                    // Quoted values support a small, explicit escape set.
                    let Some((escape_pos, escaped)) = self.chars.next() else {
                        return Err(TokenizeError::UnterminatedQuote { pos: quote_pos });
                    };

                    match escaped {
                        '"' => token.text.push('"'),
                        '\\' => token.text.push('\\'),
                        'n' => token.text.push('\n'),
                        'r' => token.text.push('\r'),
                        't' => token.text.push('\t'),
                        ch => {
                            return Err(TokenizeError::InvalidEscape { pos: escape_pos, ch });
                        }
                    }
                }

                ch => token.text.push(ch),
            }
        }

        Err(TokenizeError::UnterminatedQuote { pos: quote_pos })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_filter_expr_cases() {
        let cases = [
            (
                r#"field1:expr1 field2:"expr 2" global expr"#,
                vec![
                    term(Some("field1"), "expr1"),
                    quoted_term(Some("field2"), "expr 2"),
                    term(None, "global"),
                    term(None, "expr"),
                ],
            ),
            (r#""global expr" tail"#, vec![quoted_term(None, "global expr"), term(None, "tail")]),
            (
                r#"hello world field:value rust tokio"#,
                vec![
                    term(None, "hello"),
                    term(None, "world"),
                    term(Some("field"), "value"),
                    term(None, "rust"),
                    term(None, "tokio"),
                ],
            ),
            (
                r#"msg:"hello \"rust\"\nnext""#,
                vec![quoted_term(Some("msg"), "hello \"rust\"\nnext")],
            ),
            (
                r#"name:^rust status:!'debug"#,
                vec![term(Some("name"), "^rust"), term(Some("status"), "!'debug")],
            ),
            (r#"http://a "10:30""#, vec![term(Some("http"), "//a"), quoted_term(None, "10:30")]),
            (
                r#""http://a" "10:30""#,
                vec![quoted_term(None, "http://a"), quoted_term(None, "10:30")],
            ),
            (r#"field:"10:30""#, vec![quoted_term(Some("field"), "10:30")]),
            (r#"field:foo" bar""#, vec![quoted_term(Some("field"), "foo bar")]),
            (r#":value field:"#, vec![term(None, "value"), term(Some("field"), "")]),
            (
                r#"  field1:expr1 global  "#,
                vec![term(Some("field1"), "expr1"), term(None, "global")],
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(parse_filter_expr(input), Ok(expected), "input: {input:?}",);
        }
    }

    #[test]
    fn parse_filter_expr_error_cases() {
        let cases = [
            (r#"field:"abc"#, TokenizeError::UnterminatedQuote { pos: 6 }),
            (r#"field:"\x""#, TokenizeError::InvalidEscape { pos: 8, ch: 'x' }),
        ];

        for (input, expected) in cases {
            assert_eq!(parse_filter_expr(input), Err(expected), "input: {input:?}",);
        }
    }

    fn term(field: Option<&str>, pattern: &str) -> Term {
        Term { field: field.map(str::to_owned), pattern: pattern.to_owned(), quoted: false }
    }

    fn quoted_term(field: Option<&str>, pattern: &str) -> Term {
        Term { field: field.map(str::to_owned), pattern: pattern.to_owned(), quoted: true }
    }
}
