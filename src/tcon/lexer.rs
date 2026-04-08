use crate::model::Span;
use crate::tcon::diagnostic::format_source_error;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Import,
    From,
    Export,
    Const,
    True,
    False,
    Null,
    Ident(String),
    String(String),
    Number(String),
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Dot,
    Colon,
    Comma,
    Semi,
    Eq,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn lex(src: &str, file_name: &str) -> Result<Vec<Token>, String> {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut tokens = Vec::new();

    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        if c == '/' && i + 1 < bytes.len() && bytes[i + 1] as char == '/' {
            i += 2;
            while i < bytes.len() && bytes[i] as char != '\n' {
                i += 1;
            }
            continue;
        }

        if c == '/' && i + 1 < bytes.len() && bytes[i + 1] as char == '*' {
            let comment_start = i;
            i += 2;
            let mut closed = false;
            while i + 1 < bytes.len() {
                if bytes[i] as char == '*' && bytes[i + 1] as char == '/' {
                    i += 2;
                    closed = true;
                    break;
                }
                i += 1;
            }
            if !closed {
                return Err(format_source_error(
                    file_name,
                    src,
                    comment_start,
                    "unterminated block comment",
                ));
            }
            continue;
        }

        let start = i;
        let tk = match c {
            '{' => {
                i += 1;
                TokenKind::LBrace
            }
            '}' => {
                i += 1;
                TokenKind::RBrace
            }
            '[' => {
                i += 1;
                TokenKind::LBracket
            }
            ']' => {
                i += 1;
                TokenKind::RBracket
            }
            '(' => {
                i += 1;
                TokenKind::LParen
            }
            ')' => {
                i += 1;
                TokenKind::RParen
            }
            '.' => {
                i += 1;
                TokenKind::Dot
            }
            ':' => {
                i += 1;
                TokenKind::Colon
            }
            ',' => {
                i += 1;
                TokenKind::Comma
            }
            ';' => {
                i += 1;
                TokenKind::Semi
            }
            '=' => {
                i += 1;
                TokenKind::Eq
            }
            '"' => {
                i += 1;
                let mut out = String::new();
                let mut closed = false;
                while i < bytes.len() {
                    let ch = bytes[i] as char;
                    if ch == '"' {
                        i += 1;
                        closed = true;
                        break;
                    }
                    if ch == '\\' {
                        i += 1;
                        if i >= bytes.len() {
                            return Err(format_source_error(
                                file_name,
                                src,
                                start,
                                "unterminated string literal",
                            ));
                        }
                        let esc = bytes[i] as char;
                        let mapped = match esc {
                            '"' => '"',
                            '\\' => '\\',
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            other => other,
                        };
                        out.push(mapped);
                        i += 1;
                        continue;
                    }
                    out.push(ch);
                    i += 1;
                }
                if !closed {
                    return Err(format_source_error(
                        file_name,
                        src,
                        start,
                        "unterminated string literal",
                    ));
                }
                TokenKind::String(out)
            }
            '-' => {
                if i + 1 >= bytes.len() {
                    return Err(format_source_error(
                        file_name,
                        src,
                        i,
                        "unexpected character '-' (negative numbers must include digits, e.g. -1)",
                    ));
                }
                let next = bytes[i + 1] as char;
                if !next.is_ascii_digit() {
                    return Err(format_source_error(
                        file_name,
                        src,
                        i,
                        "unexpected character '-' (negative numbers must include digits, e.g. -1)",
                    ));
                }
                i += 1;
                let mut dot_seen = false;
                while i < bytes.len() {
                    let ch = bytes[i] as char;
                    if ch.is_ascii_digit() {
                        i += 1;
                    } else if ch == '.' {
                        if dot_seen {
                            return Err(format_source_error(
                                file_name,
                                src,
                                i,
                                "invalid number literal: multiple decimal points",
                            ));
                        }
                        dot_seen = true;
                        i += 1;
                    } else {
                        break;
                    }
                }
                TokenKind::Number(src[start..i].to_string())
            }
            '0'..='9' => {
                i += 1;
                let mut dot_seen = false;
                while i < bytes.len() {
                    let ch = bytes[i] as char;
                    if ch.is_ascii_digit() {
                        i += 1;
                    } else if ch == '.' {
                        if dot_seen {
                            return Err(format_source_error(
                                file_name,
                                src,
                                i,
                                "invalid number literal: multiple decimal points",
                            ));
                        }
                        dot_seen = true;
                        i += 1;
                    } else {
                        break;
                    }
                }
                let text = &src[start..i];
                TokenKind::Number(text.to_string())
            }
            _ => {
                if c.is_ascii_alphabetic() || c == '_' {
                    i += 1;
                    while i < bytes.len() {
                        let ch = bytes[i] as char;
                        if ch.is_ascii_alphanumeric() || ch == '_' {
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    let text = &src[start..i];
                    match text {
                        "import" => TokenKind::Import,
                        "from" => TokenKind::From,
                        "export" => TokenKind::Export,
                        "const" => TokenKind::Const,
                        "true" => TokenKind::True,
                        "false" => TokenKind::False,
                        "null" => TokenKind::Null,
                        _ => TokenKind::Ident(text.to_string()),
                    }
                } else {
                    return Err(format_source_error(
                        file_name,
                        src,
                        i,
                        &format!("unexpected character '{c}'"),
                    ));
                }
            }
        };

        tokens.push(Token {
            kind: tk,
            span: Span::new(start, i),
        });
    }

    Ok(tokens)
}
