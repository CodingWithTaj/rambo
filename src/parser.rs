//! Recursive-descent parser. Statements are parsed top-down; expressions use
//! one function per precedence level, lowest first:
//!
//!   ||  →  &&  →  == !=  →  < <= > >=  →  + -  →  * / %  →  unary ! -
//!       →  postfix: call f(x), index a[i], field a.b  →  primary

use crate::ast::*;
use crate::lexer::{self, Part, Tok, Token};
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub msg: String,
    pub line: u32,
    pub col: u32,
}

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

type PResult<T> = Result<T, ParseError>;

pub fn parse(toks: Vec<Token>) -> PResult<Vec<Stmt>> {
    let mut p = Parser { toks, pos: 0 };
    let mut stmts = Vec::new();
    while !p.check(&Tok::Eof) {
        stmts.push(p.statement()?);
    }
    Ok(stmts)
}

pub fn describe(t: &Tok) -> String {
    let sym = match t {
        Tok::Number(n) => return format!("number {}", n),
        Tok::Str(_) | Tok::Interp(_) => return "a string".into(),
        Tok::Ident(s) => return format!("'{}'", s),
        Tok::Eof => return "the end of the file".into(),
        Tok::LParen => "(", Tok::RParen => ")", Tok::LBrace => "{", Tok::RBrace => "}",
        Tok::LBracket => "[", Tok::RBracket => "]", Tok::Comma => ",", Tok::Semi => ";",
        Tok::Colon => ":", Tok::Dot => ".", Tok::DotDot => "..",
        Tok::Plus => "+", Tok::Minus => "-", Tok::Star => "*", Tok::Slash => "/", Tok::Percent => "%",
        Tok::PlusEq => "+=", Tok::MinusEq => "-=", Tok::StarEq => "*=", Tok::SlashEq => "/=", Tok::PercentEq => "%=",
        Tok::Bang => "!", Tok::BangEq => "!=", Tok::Eq => "=", Tok::EqEq => "==",
        Tok::Lt => "<", Tok::LtEq => "<=", Tok::Gt => ">", Tok::GtEq => ">=",
        Tok::AndAnd => "&&", Tok::OrOr => "||",
        kw => return format!("keyword '{}'", format!("{:?}", kw).to_lowercase()),
    };
    format!("'{}'", sym)
}

impl Parser {
    fn peek(&self) -> &Token { &self.toks[self.pos] }
    fn peek_at(&self, k: usize) -> &Tok { &self.toks[(self.pos + k).min(self.toks.len() - 1)].tok }
    fn span(&self) -> Span { let t = self.peek(); Span { line: t.line, col: t.col } }
    fn check(&self, t: &Tok) -> bool { &self.peek().tok == t }
    fn advance(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if self.pos < self.toks.len() - 1 { self.pos += 1; }
        t
    }
    fn eat(&mut self, t: &Tok) -> bool {
        if self.check(t) { self.advance(); true } else { false }
    }
    fn error<T>(&self, msg: impl Into<String>) -> PResult<T> {
        let t = self.peek();
        Err(ParseError { msg: msg.into(), line: t.line, col: t.col })
    }
    fn expect(&mut self, t: Tok, what: &str) -> PResult<Token> {
        if self.check(&t) { Ok(self.advance()) } else {
            let found = describe(&self.peek().tok);
            self.error(format!("expected {}, found {}", what, found))
        }
    }
    /// Semicolons get a friendlier message that points just after the previous token.
    fn expect_semi(&mut self, after: &str) -> PResult<()> {
        if self.eat(&Tok::Semi) { return Ok(()); }
        let prev = &self.toks[self.pos.saturating_sub(1)];
        let here = self.peek();
        let found = describe(&here.tok);
        if here.line != prev.line {
            // the semicolon is missing at the end of the previous line
            return Err(ParseError { msg: format!("missing ';' after {}", after), line: prev.line, col: prev.end });
        }
        self.error(format!("expected ';' after {}, found {}", after, found))
    }
    fn ident(&mut self, what: &str) -> PResult<String> {
        match self.peek().tok.clone() {
            Tok::Ident(name) => { self.advance(); Ok(name) }
            other => self.error(format!("expected {}, found {}", what, describe(&other))),
        }
    }

    // ---------- statements ----------

    fn statement(&mut self) -> PResult<Stmt> {
        let span = self.span();
        let kind = match self.peek().tok {
            Tok::Let => {
                self.advance();
                let name = self.ident("a variable name after 'let'")?;
                self.expect(Tok::Eq, "'=' (variables must be given a value)")?;
                let value = self.expression()?;
                self.expect_semi("the variable declaration")?;
                StmtKind::Let(name, value)
            }
            Tok::Fn if matches!(self.peek_at(1), Tok::Ident(_)) => {
                self.advance();
                let name = self.ident("function name")?;
                StmtKind::Fn(self.function_rest(Some(name), span)?)
            }
            Tok::If => { self.advance(); return self.if_rest(span); }
            Tok::While => {
                self.advance();
                let cond = self.expression()?;
                StmtKind::While(cond, self.block()?)
            }
            Tok::For => {
                self.advance();
                let var = self.ident("a loop variable after 'for'")?;
                self.expect(Tok::In, "'in' after the loop variable")?;
                let start = self.expression()?;
                if self.eat(&Tok::DotDot) {
                    let end = self.expression()?;
                    StmtKind::ForRange(var, start, end, self.block()?)
                } else {
                    StmtKind::ForEach(var, start, self.block()?)
                }
            }
            Tok::Return => {
                self.advance();
                let value = if self.check(&Tok::Semi) || self.check(&Tok::RBrace) { None } else { Some(self.expression()?) };
                self.expect_semi("return")?;
                StmtKind::Return(value)
            }
            Tok::Break => { self.advance(); self.expect_semi("break")?; StmtKind::Break }
            Tok::Continue => { self.advance(); self.expect_semi("continue")?; StmtKind::Continue }
            Tok::LBrace => StmtKind::Block(self.block()?),
            _ => {
                let expr = self.expression()?;
                let op = match self.peek().tok {
                    Tok::Eq => Some(None),
                    Tok::PlusEq => Some(Some(BinOp::Add)),
                    Tok::MinusEq => Some(Some(BinOp::Sub)),
                    Tok::StarEq => Some(Some(BinOp::Mul)),
                    Tok::SlashEq => Some(Some(BinOp::Div)),
                    Tok::PercentEq => Some(Some(BinOp::Mod)),
                    _ => None,
                };
                if let Some(op) = op {
                    if !matches!(expr.kind, ExprKind::Var(_) | ExprKind::Index(..)) {
                        return self.error("can only assign to a variable, list element, or map field");
                    }
                    self.advance();
                    let value = self.expression()?;
                    self.expect_semi("the assignment")?;
                    StmtKind::Assign { target: expr, op, value }
                } else {
                    self.expect_semi("the expression")?;
                    StmtKind::Expr(expr)
                }
            }
        };
        Ok(Stmt { kind, span })
    }

    fn if_rest(&mut self, span: Span) -> PResult<Stmt> {
        let cond = self.expression()?;
        let then = self.block()?;
        let els = if self.eat(&Tok::Else) {
            if self.check(&Tok::If) {
                let s = self.span();
                self.advance();
                Some(vec![self.if_rest(s)?]) // `else if` is sugar for else { if ... }
            } else {
                Some(self.block()?)
            }
        } else { None };
        Ok(Stmt { kind: StmtKind::If(cond, then, els), span })
    }

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect(Tok::LBrace, "'{'")?;
        let mut stmts = Vec::new();
        while !self.check(&Tok::RBrace) {
            if self.check(&Tok::Eof) { return self.error("this block is never closed: expected '}'"); }
            stmts.push(self.statement()?);
        }
        self.advance();
        Ok(stmts)
    }

    fn function_rest(&mut self, name: Option<String>, span: Span) -> PResult<Rc<FnDecl>> {
        self.expect(Tok::LParen, "'(' before the parameters")?;
        let mut params = Vec::new();
        if !self.check(&Tok::RParen) {
            loop {
                let p = self.ident("a parameter name")?;
                if params.contains(&p) { return self.error(format!("duplicate parameter '{}'", p)); }
                params.push(p);
                if !self.eat(&Tok::Comma) || self.check(&Tok::RParen) { break; }
            }
        }
        if params.len() > 255 { return self.error("too many parameters (the limit is 255)"); }
        self.expect(Tok::RParen, "')' after the parameters")?;
        let body = self.block()?;
        Ok(Rc::new(FnDecl { name, params, body, span }))
    }

    // ---------- expressions ----------

    pub fn expression(&mut self) -> PResult<Expr> { self.or() }

    fn or(&mut self) -> PResult<Expr> {
        let mut left = self.and()?;
        while self.check(&Tok::OrOr) {
            let span = self.span();
            self.advance();
            let right = self.and()?;
            left = Expr { kind: ExprKind::Logical { is_and: false, left: Box::new(left), right: Box::new(right) }, span };
        }
        Ok(left)
    }

    fn and(&mut self) -> PResult<Expr> {
        let mut left = self.equality()?;
        while self.check(&Tok::AndAnd) {
            let span = self.span();
            self.advance();
            let right = self.equality()?;
            left = Expr { kind: ExprKind::Logical { is_and: true, left: Box::new(left), right: Box::new(right) }, span };
        }
        Ok(left)
    }

    fn binary_level(&mut self, next: fn(&mut Self) -> PResult<Expr>, ops: &[(Tok, BinOp)]) -> PResult<Expr> {
        let mut left = next(self)?;
        'outer: loop {
            for (t, op) in ops {
                if self.check(t) {
                    let span = self.span();
                    self.advance();
                    let right = next(self)?;
                    left = Expr { kind: ExprKind::Binary(*op, Box::new(left), Box::new(right)), span };
                    continue 'outer;
                }
            }
            return Ok(left);
        }
    }

    fn equality(&mut self) -> PResult<Expr> {
        self.binary_level(Self::comparison, &[(Tok::EqEq, BinOp::Eq), (Tok::BangEq, BinOp::Ne)])
    }
    fn comparison(&mut self) -> PResult<Expr> {
        self.binary_level(Self::term, &[
            (Tok::Lt, BinOp::Lt), (Tok::LtEq, BinOp::Le), (Tok::Gt, BinOp::Gt), (Tok::GtEq, BinOp::Ge),
        ])
    }
    fn term(&mut self) -> PResult<Expr> {
        self.binary_level(Self::factor, &[(Tok::Plus, BinOp::Add), (Tok::Minus, BinOp::Sub)])
    }
    fn factor(&mut self) -> PResult<Expr> {
        self.binary_level(Self::unary, &[(Tok::Star, BinOp::Mul), (Tok::Slash, BinOp::Div), (Tok::Percent, BinOp::Mod)])
    }

    fn unary(&mut self) -> PResult<Expr> {
        let op = match self.peek().tok { Tok::Minus => UnOp::Neg, Tok::Bang => UnOp::Not, _ => return self.postfix() };
        let span = self.span();
        self.advance();
        let operand = self.unary()?;
        Ok(Expr { kind: ExprKind::Unary(op, Box::new(operand)), span })
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        loop {
            let span = self.span();
            if self.eat(&Tok::LParen) {
                let args = self.items(Tok::RParen, "')' after the arguments")?;
                if args.len() > 255 { return self.error("too many arguments (the limit is 255)"); }
                e = Expr { kind: ExprKind::Call(Box::new(e), args), span };
            } else if self.eat(&Tok::LBracket) {
                let idx = self.expression()?;
                self.expect(Tok::RBracket, "']' after the index")?;
                e = Expr { kind: ExprKind::Index(Box::new(e), Box::new(idx)), span };
            } else if self.eat(&Tok::Dot) {
                let field_span = self.span();
                let field = self.ident("a field name after '.'")?;
                let key = Expr { kind: ExprKind::Str(field), span: field_span };
                e = Expr { kind: ExprKind::Index(Box::new(e), Box::new(key)), span };
            } else {
                return Ok(e);
            }
        }
    }

    fn items(&mut self, close: Tok, what: &str) -> PResult<Vec<Expr>> {
        let mut items = Vec::new();
        if !self.check(&close) {
            loop {
                items.push(self.expression()?);
                if !self.eat(&Tok::Comma) || self.check(&close) { break; } // trailing comma is fine
            }
        }
        self.expect(close, what)?;
        Ok(items)
    }

    fn map_literal(&mut self) -> PResult<ExprKind> {
        let mut pairs = Vec::new();
        if !self.check(&Tok::RBrace) {
            loop {
                let span = self.span();
                // `name: v` uses the name as a string key; `[expr]: v` computes the key
                let key = match (self.peek().tok.clone(), self.peek_at(1)) {
                    (Tok::Ident(name), Tok::Colon) => { self.advance(); Expr { kind: ExprKind::Str(name), span } }
                    (Tok::LBracket, _) => {
                        self.advance();
                        let k = self.expression()?;
                        self.expect(Tok::RBracket, "']' after the computed key")?;
                        k
                    }
                    _ => self.expression()?,
                };
                self.expect(Tok::Colon, "':' between a key and its value")?;
                let value = self.expression()?;
                pairs.push((key, value));
                if !self.eat(&Tok::Comma) || self.check(&Tok::RBrace) { break; }
            }
        }
        self.expect(Tok::RBrace, "'}' to close the map")?;
        if pairs.len() > u16::MAX as usize { return self.error("map literal too large"); }
        Ok(ExprKind::Map(pairs))
    }

    fn interpolation(&mut self, parts: Vec<Part>, span: Span) -> PResult<ExprKind> {
        let mut exprs = Vec::new();
        for part in parts {
            match part {
                Part::Lit(s) => exprs.push(Expr { kind: ExprKind::Str(s), span }),
                Part::Code { src, line, col } => {
                    let toks = lexer::lex_at(&src, line, col)
                        .map_err(|e| ParseError { msg: e.msg, line: e.line, col: e.col })?;
                    let mut sub = Parser { toks, pos: 0 };
                    let e = sub.expression()?;
                    if !sub.check(&Tok::Eof) {
                        return sub.error(format!("unexpected {} inside '{{...}}'", describe(&sub.peek().tok)));
                    }
                    exprs.push(e);
                }
            }
        }
        Ok(ExprKind::Interp(exprs))
    }

    fn primary(&mut self) -> PResult<Expr> {
        let span = self.span();
        let t = self.advance();
        let kind = match t.tok {
            Tok::Number(n) => ExprKind::Number(n),
            Tok::Str(s) => ExprKind::Str(s),
            Tok::Interp(parts) => self.interpolation(parts, span)?,
            Tok::True => ExprKind::Bool(true),
            Tok::False => ExprKind::Bool(false),
            Tok::Nil => ExprKind::Nil,
            Tok::Ident(name) => ExprKind::Var(name),
            Tok::LParen => {
                let e = self.expression()?;
                self.expect(Tok::RParen, "')'")?;
                return Ok(e);
            }
            Tok::LBracket => ExprKind::List(self.items(Tok::RBracket, "']' to close the list")?),
            Tok::LBrace => self.map_literal()?,
            Tok::Fn => ExprKind::Func(self.function_rest(None, span)?),
            other => {
                return Err(ParseError { msg: format!("expected an expression, found {}", describe(&other)), line: t.line, col: t.col });
            }
        };
        Ok(Expr { kind, span })
    }
}
