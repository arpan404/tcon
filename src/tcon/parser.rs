use crate::model::{ExportConst, Expr, Key, Program, Span};
use crate::tcon::lexer::{Token, TokenKind};

pub fn parse(tokens: &[Token], file_name: &str) -> Result<Program, String> {
    let mut p = Parser {
        tokens,
        i: 0,
        file_name,
    };
    p.parse_program()
}

struct Parser<'a> {
    tokens: &'a [Token],
    i: usize,
    file_name: &'a str,
}

impl<'a> Parser<'a> {
    fn parse_program(&mut self) -> Result<Program, String> {
        let mut exports = Vec::new();
        while !self.eof() {
            exports.push(self.parse_export_const()?);
        }
        Ok(Program { exports })
    }

    fn parse_export_const(&mut self) -> Result<ExportConst, String> {
        let start = self.expect_simple(TokenKind::Export)?.span.start;
        self.expect_simple(TokenKind::Const)?;
        let (name, _) = self.expect_ident()?;
        self.expect_simple(TokenKind::Eq)?;
        let expr = self.parse_expr()?;
        let end = self
            .maybe_simple(TokenKind::Semi)
            .map(|t| t.span.end)
            .unwrap_or(expr_span(&expr).end);
        Ok(ExportConst {
            name,
            expr,
            span: Span::new(start, end),
        })
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.maybe_simple(TokenKind::Dot).is_some() {
                let (name, span) = self.expect_ident()?;
                let left_span = expr_span(&expr);
                expr = Expr::Member(Box::new(expr), name, Span::new(left_span.start, span.end));
                continue;
            }
            if self.maybe_simple(TokenKind::LParen).is_some() {
                let mut args = Vec::new();
                if !self.peek_simple(TokenKind::RParen) {
                    loop {
                        args.push(self.parse_expr()?);
                        if self.maybe_simple(TokenKind::Comma).is_some() {
                            continue;
                        }
                        break;
                    }
                }
                let r = self.expect_simple(TokenKind::RParen)?;
                let left_span = expr_span(&expr);
                expr = Expr::Call(Box::new(expr), args, Span::new(left_span.start, r.span.end));
                continue;
            }
            break;
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        let tok = self.next().ok_or_else(|| self.err("unexpected EOF"))?;
        match &tok.kind {
            TokenKind::String(s) => Ok(Expr::String(s.clone(), tok.span)),
            TokenKind::Number(n) => Ok(Expr::Number(n.clone(), tok.span)),
            TokenKind::True => Ok(Expr::Bool(true, tok.span)),
            TokenKind::False => Ok(Expr::Bool(false, tok.span)),
            TokenKind::Null => Ok(Expr::Null(tok.span)),
            TokenKind::Ident(name) => Ok(Expr::Ident(name.clone(), tok.span)),
            TokenKind::LBrace => self.parse_object(tok.span.start),
            TokenKind::LBracket => self.parse_array(tok.span.start),
            _ => Err(self.err_at(tok.span.start, "unexpected token in expression")),
        }
    }

    fn parse_object(&mut self, start: usize) -> Result<Expr, String> {
        let mut fields = Vec::new();
        while !self.peek_simple(TokenKind::RBrace) {
            let key_tok = self.next().ok_or_else(|| self.err("unexpected EOF"))?;
            let key = match &key_tok.kind {
                TokenKind::Ident(k) => Key::Ident(k.clone()),
                TokenKind::String(k) => Key::String(k.clone()),
                _ => {
                    return Err(
                        self.err_at(key_tok.span.start, "object key must be identifier or string")
                    );
                }
            };
            self.expect_simple(TokenKind::Colon)?;
            let value = self.parse_expr()?;
            let span = Span::new(key_tok.span.start, expr_span(&value).end);
            fields.push((key, value, span));
            if self.maybe_simple(TokenKind::Comma).is_some() {
                if self.peek_simple(TokenKind::RBrace) {
                    break;
                }
                continue;
            }
            break;
        }
        let r = self.expect_simple(TokenKind::RBrace)?;
        Ok(Expr::Object(fields, Span::new(start, r.span.end)))
    }

    fn parse_array(&mut self, start: usize) -> Result<Expr, String> {
        let mut items = Vec::new();
        while !self.peek_simple(TokenKind::RBracket) {
            let item = self.parse_expr()?;
            let s = expr_span(&item);
            items.push((item, s));
            if self.maybe_simple(TokenKind::Comma).is_some() {
                if self.peek_simple(TokenKind::RBracket) {
                    break;
                }
                continue;
            }
            break;
        }
        let r = self.expect_simple(TokenKind::RBracket)?;
        Ok(Expr::Array(items, Span::new(start, r.span.end)))
    }

    fn expect_ident(&mut self) -> Result<(String, Span), String> {
        let tok = self.next().ok_or_else(|| self.err("unexpected EOF"))?;
        match &tok.kind {
            TokenKind::Ident(name) => Ok((name.clone(), tok.span)),
            _ => Err(self.err_at(tok.span.start, "expected identifier")),
        }
    }

    fn expect_simple(&mut self, expected: TokenKind) -> Result<Token, String> {
        let tok = self.next().ok_or_else(|| self.err("unexpected EOF"))?;
        if std::mem::discriminant(&tok.kind) == std::mem::discriminant(&expected) {
            Ok(tok.clone())
        } else {
            Err(self.err_at(tok.span.start, "unexpected token"))
        }
    }

    fn maybe_simple(&mut self, expected: TokenKind) -> Option<Token> {
        if self.peek_simple(expected) {
            self.next()
        } else {
            None
        }
    }

    fn peek_simple(&self, expected: TokenKind) -> bool {
        self.tokens
            .get(self.i)
            .map(|t| std::mem::discriminant(&t.kind) == std::mem::discriminant(&expected))
            .unwrap_or(false)
    }

    fn next(&mut self) -> Option<Token> {
        let out = self.tokens.get(self.i).cloned();
        if out.is_some() {
            self.i += 1;
        }
        out
    }

    fn eof(&self) -> bool {
        self.i >= self.tokens.len()
    }

    fn err(&self, msg: &str) -> String {
        format!("{}: {}", self.file_name, msg)
    }

    fn err_at(&self, pos: usize, msg: &str) -> String {
        format!("{}: {} at byte {}", self.file_name, msg, pos)
    }
}

fn expr_span(expr: &Expr) -> Span {
    match expr {
        Expr::Object(_, s)
        | Expr::Array(_, s)
        | Expr::String(_, s)
        | Expr::Number(_, s)
        | Expr::Bool(_, s)
        | Expr::Null(s)
        | Expr::Ident(_, s)
        | Expr::Member(_, _, s)
        | Expr::Call(_, _, s) => *s,
    }
}
