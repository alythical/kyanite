use crate::{
    ast::{Expr, Field, Initializer, Param, Stmt},
    token::Token,
};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, PartialEq, Eq)]
pub struct RecordDecl {
    pub name: Token,
    pub fields: Vec<Field>,
}

impl RecordDecl {
    pub fn new(name: Token, fields: Vec<Field>) -> Self {
        Self { name, fields }
    }
}

#[derive(Debug)]
pub struct FuncDecl {
    pub name: Token,
    pub params: Vec<Param>,
    pub ty: Option<Token>,
    pub body: Vec<Stmt>,
    pub external: bool,
    pub id: usize,
}

impl FuncDecl {
    pub fn new(
        name: Token,
        params: Vec<Param>,
        ty: Option<Token>,
        body: Vec<Stmt>,
        external: bool,
    ) -> Self {
        static ID: AtomicUsize = AtomicUsize::new(0);
        let id = ID.fetch_add(1, Ordering::SeqCst);
        Self {
            name,
            params,
            ty,
            body,
            external,
            id,
        }
    }
}

#[derive(Debug)]
pub struct If {
    pub condition: Expr,
    pub is: Vec<Stmt>,
    pub otherwise: Vec<Stmt>,
}

impl If {
    pub fn new(condition: Expr, is: Vec<Stmt>, otherwise: Vec<Stmt>) -> Self {
        Self {
            condition,
            is,
            otherwise,
        }
    }
}

#[derive(Debug)]
pub struct While {
    pub condition: Expr,
    pub body: Vec<Stmt>,
}

impl While {
    pub fn new(condition: Expr, body: Vec<Stmt>) -> Self {
        Self { condition, body }
    }
}

#[derive(Debug)]
pub struct Assign {
    pub target: Expr,
    pub expr: Expr,
}

impl Assign {
    pub fn new(target: Expr, expr: Expr) -> Self {
        Self { target, expr }
    }
}

#[derive(Debug)]
pub struct VarDecl {
    pub name: Token,
    pub ty: Token,
    pub expr: Expr,
}

impl VarDecl {
    pub fn new(name: Token, ty: Token, expr: Expr) -> Self {
        Self { name, ty, expr }
    }
}

#[derive(Debug)]
pub struct ConstantDecl {
    pub name: Token,
    pub ty: Token,
    pub expr: Expr,
}

impl ConstantDecl {
    pub fn new(name: Token, ty: Token, expr: Expr) -> Self {
        Self { name, ty, expr }
    }
}

#[derive(Debug)]
pub struct Init {
    pub name: Token,
    pub initializers: Vec<Initializer>,
    pub parens: (Token, Token),
}

impl Init {
    pub fn new(name: Token, initializers: Vec<Initializer>, parens: (Token, Token)) -> Self {
        Self {
            name,
            initializers,
            parens,
        }
    }
}

#[derive(Debug)]
pub struct Call {
    pub left: Box<Expr>,
    pub args: Vec<Expr>,
    pub parens: (Token, Token),
    pub delimiters: Vec<Token>,
}

impl Call {
    pub fn new(
        left: Box<Expr>,
        args: Vec<Expr>,
        parens: (Token, Token),
        delimiters: Vec<Token>,
    ) -> Self {
        Self {
            left,
            args,
            parens,
            delimiters,
        }
    }
}

#[derive(Debug)]
pub struct Access {
    pub chain: Vec<Expr>,
    pub id: usize,
}

impl Access {
    pub fn new(chain: Vec<Expr>) -> Self {
        static ID: AtomicUsize = AtomicUsize::new(0);
        let id = ID.fetch_add(1, Ordering::SeqCst);
        Self { chain, id }
    }
}

#[derive(Debug)]
pub struct Return {
    pub expr: Expr,
    pub keyword: Token,
}

impl Return {
    pub fn new(expr: Expr, keyword: Token) -> Self {
        Self { expr, keyword }
    }
}

#[derive(Debug)]
pub struct Unary {
    pub op: Token,
    pub expr: Box<Expr>,
}

impl Unary {
    pub fn new(op: Token, expr: Box<Expr>) -> Self {
        Self { op, expr }
    }
}

#[derive(Debug)]
pub struct Binary {
    pub left: Box<Expr>,
    pub op: Token,
    pub right: Box<Expr>,
}

impl Binary {
    pub fn new(left: Box<Expr>, op: Token, right: Box<Expr>) -> Self {
        Self { left, op, right }
    }
}

#[derive(Debug)]
pub struct Ident {
    pub name: Token,
}

impl Ident {
    pub fn new(name: Token) -> Self {
        Self { name }
    }
}
