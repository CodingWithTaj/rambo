//! Lexer: turns source text into tokens, each tagged with its line and column.
//!
//! String interpolation is handled here: `"hi {name}!"` becomes a single
//! `Interp` token holding literal pieces and the raw source of each `{...}`
//! expression, which the parser then parses on its own.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Number(f64),
    Str(String),
    Interp(Vec<Part>),
    Ident(String),
    // keywords
    Let, Fn, Return, If, Else, While, For, In, Break, Continue, True, False, Nil,
    // punctuation
    LParen, RParen, LBrace, RBrace, LBracket, RBracket,
    Comma, Semi, Colon, Dot, DotDot,
    Plus, Minus, Star, Slash, Percent,
    PlusEq, MinusEq, StarEq, SlashEq, PercentEq,
    Bang, BangEq, Eq, EqEq, Lt, LtEq, Gt, GtEq,
    AndAnd, OrOr,
    Eof,
}

/// A piece of an interpolated string.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Lit(String),
    /// Source of an embedded expression and where it starts in the file.
    Code { src: String, line: u32, col: u32 },
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
    pub col: u32,
    /// column just past the end of the token (on the same line)
    pub end: u32,
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub msg: String,
    pub line: u32,
    pub col: u32,
}

pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    lex_at(src, 1, 1)
}

/// Lex starting from a given position (used for code inside `{...}` in strings).
pub fn lex_at(src: &str, start_line: u32, start_col: u32) -> Result<Vec<Token>, LexError> {
    let chars: Vec<char> = src.chars().collect();
    let mut toks = Vec::new();
    let (mut i, mut line, mut col) = (0usize, start_line, start_col);

    let err = |msg: String, line: u32, col: u32| Err(LexError { msg, line, col });

    while i < chars.len() {
        let c = chars[i];
        let (sl, sc) = (line, col);
        let peek = |k: usize| chars.get(i + k).copied().unwrap_or('\0');

        // whitespace
        if c == '\n' { i += 1; line += 1; col = 1; continue; }
        if c.is_whitespace() { i += 1; col += 1; continue; }

        // comments: // to end of line, /* ... */ (may span lines)
        if c == '/' && peek(1) == '/' {
            while i < chars.len() && chars[i] != '\n' { i += 1; }
            continue;
        }
        if c == '/' && peek(1) == '*' {
            i += 2; col += 2;
            loop {
                if i >= chars.len() { return err("unterminated block comment".into(), sl, sc); }
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') { i += 2; col += 2; break; }
                if chars[i] == '\n' { line += 1; col = 1; } else { col += 1; }
                i += 1;
            }
            continue;
        }

        // numbers: 42, 3.14, 1_000_000 (but "1..5" is a range)
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') { i += 1; }
            if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') { i += 1; }
            }
            let text: String = chars[start..i].iter().filter(|c| **c != '_').collect();
            col += (i - start) as u32;
            toks.push(Token { tok: Tok::Number(text.parse().unwrap()), line: sl, col: sc, end: col });
            continue;
        }

        // identifiers and keywords
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') { i += 1; }
            let word: String = chars[start..i].iter().collect();
            col += (i - start) as u32;
            let tok = match word.as_str() {
                "let" => Tok::Let, "fn" => Tok::Fn, "return" => Tok::Return,
                "if" => Tok::If, "else" => Tok::Else, "while" => Tok::While,
                "for" => Tok::For, "in" => Tok::In, "break" => Tok::Break,
                "continue" => Tok::Continue, "true" => Tok::True,
                "false" => Tok::False, "nil" => Tok::Nil,
                _ => Tok::Ident(word),
            };
            toks.push(Token { tok, line: sl, col: sc, end: col });
            continue;
        }

        // strings, with escapes and {interpolation}
        if c == '"' {
            i += 1; col += 1;
            let mut parts: Vec<Part> = Vec::new();
            let mut lit = String::new();
            loop {
                if i >= chars.len() || chars[i] == '\n' {
                    return err("unterminated string".into(), sl, sc);
                }
                let ch = chars[i];
                if ch == '"' { i += 1; col += 1; break; }
                if ch == '\\' {
                    let esc = chars.get(i + 1).copied().unwrap_or('\0');
                    lit.push(match esc {
                        'n' => '\n', 't' => '\t', 'r' => '\r', '0' => '\0',
                        '"' => '"', '\\' => '\\', '{' => '{', '}' => '}',
                        _ => return err(format!("unknown escape '\\{}'", esc), line, col),
                    });
                    i += 2; col += 2;
                    continue;
                }
                if ch == '{' {
                    // find the matching '}', skipping over nested braces and strings
                    let (code_line, code_col) = (line, col + 1);
                    let start = i + 1;
                    let mut j = start;
                    let mut depth = 1;
                    while j < chars.len() {
                        match chars[j] {
                            '\n' => return err("unterminated '{' in string".into(), line, col),
                            '{' => depth += 1,
                            '}' => { depth -= 1; if depth == 0 { break; } }
                            '"' => {
                                j += 1;
                                while j < chars.len() && chars[j] != '"' && chars[j] != '\n' {
                                    if chars[j] == '\\' { j += 1; }
                                    j += 1;
                                }
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    if j >= chars.len() { return err("unterminated '{' in string".into(), line, col); }
                    let code: String = chars[start..j].iter().collect();
                    if code.trim().is_empty() {
                        return err("empty '{}' in string (use \\{ for a literal brace)".into(), line, col);
                    }
                    if !lit.is_empty() { parts.push(Part::Lit(std::mem::take(&mut lit))); }
                    parts.push(Part::Code { src: code, line: code_line, col: code_col });
                    col += (j + 1 - i) as u32;
                    i = j + 1;
                    continue;
                }
                lit.push(ch); i += 1; col += 1;
            }
            let tok = if parts.is_empty() {
                Tok::Str(lit)
            } else {
                if !lit.is_empty() { parts.push(Part::Lit(lit)); }
                Tok::Interp(parts)
            };
            toks.push(Token { tok, line: sl, col: sc, end: col });
            continue;
        }

        // operators and punctuation (two-character forms first)
        let two = |a: char, b: char| c == a && peek(1) == b;
        let (tok, len) = if two('.', '.') { (Tok::DotDot, 2) }
            else if two('=', '=') { (Tok::EqEq, 2) }
            else if two('!', '=') { (Tok::BangEq, 2) }
            else if two('<', '=') { (Tok::LtEq, 2) }
            else if two('>', '=') { (Tok::GtEq, 2) }
            else if two('&', '&') { (Tok::AndAnd, 2) }
            else if two('|', '|') { (Tok::OrOr, 2) }
            else if two('+', '=') { (Tok::PlusEq, 2) }
            else if two('-', '=') { (Tok::MinusEq, 2) }
            else if two('*', '=') { (Tok::StarEq, 2) }
            else if two('/', '=') { (Tok::SlashEq, 2) }
            else if two('%', '=') { (Tok::PercentEq, 2) }
            else {
                let t = match c {
                    '(' => Tok::LParen, ')' => Tok::RParen, '{' => Tok::LBrace, '}' => Tok::RBrace,
                    '[' => Tok::LBracket, ']' => Tok::RBracket, ',' => Tok::Comma, ';' => Tok::Semi,
                    ':' => Tok::Colon, '.' => Tok::Dot,
                    '+' => Tok::Plus, '-' => Tok::Minus, '*' => Tok::Star,
                    '/' => Tok::Slash, '%' => Tok::Percent, '!' => Tok::Bang, '=' => Tok::Eq,
                    '<' => Tok::Lt, '>' => Tok::Gt,
                    '&' => return err("unexpected '&' (did you mean '&&'?)".into(), sl, sc),
                    '|' => return err("unexpected '|' (did you mean '||'?)".into(), sl, sc),
                    _ => return err(format!("unexpected character '{}'", c), sl, sc),
                };
                (t, 1)
            };
        i += len; col += len as u32;
        toks.push(Token { tok, line: sl, col: sc, end: col });
    }
    toks.push(Token { tok: Tok::Eof, line, col, end: col });
    Ok(toks)
}
