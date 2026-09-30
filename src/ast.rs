//! Abstract syntax tree. Every node records the line and column it came
//! from, so errors at any stage can point at the exact spot in the source.

use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp { Add, Sub, Mul, Div, Mod, Eq, Ne, Lt, Le, Gt, Ge }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp { Neg, Not }

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Span {
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Number(f64),
    Str(String),
    /// Interpolated string: its pieces are concatenated at runtime.
    Interp(Vec<Expr>),
    Bool(bool),
    Nil,
    Var(String),
    List(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// `&&` (is_and = true) or `||`; separate because they short-circuit
    Logical { is_and: bool, left: Box<Expr>, right: Box<Expr> },
    Func(Rc<FnDecl>),
}

#[derive(Debug)]
pub struct FnDecl {
    pub name: Option<String>,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Let(String, Expr),
    /// `target = value`, or `target op= value` when `op` is set
    Assign { target: Expr, op: Option<BinOp>, value: Expr },
    Expr(Expr),
    If(Expr, Vec<Stmt>, Option<Vec<Stmt>>),
    While(Expr, Vec<Stmt>),
    ForRange(String, Expr, Expr, Vec<Stmt>),
    ForEach(String, Expr, Vec<Stmt>),
    Fn(Rc<FnDecl>),
    Return(Option<Expr>),
    Break,
    Continue,
    Block(Vec<Stmt>),
}
