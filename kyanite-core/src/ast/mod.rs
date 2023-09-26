use inkwell::{types::BasicTypeEnum, AddressSpace};
use serde::{Deserialize, Serialize};

use crate::{
    codegen::Ir,
    parse::Parser,
    token::{Token, TokenStream},
    PipelineError, Source,
};
use std::fmt;

pub mod node;

#[derive(Debug, Serialize, Deserialize)]
pub struct Ast {
    pub file: File,
}

impl Ast {
    pub fn from_source(source: Source) -> Result<Self, PipelineError> {
        let stream = TokenStream::new(source).map_err(|_| PipelineError::InvalidUtf8)?;
        Self::new(stream)
    }

    fn new(stream: TokenStream) -> Result<Self, PipelineError> {
        let errors = stream.errors.len();
        if errors > 0 {
            return Err(PipelineError::LexError(errors));
        }
        let mut parser = Parser::new(stream.source, stream.tokens);
        let file = parser.parse();
        let errors = parser.errors.len();
        if errors > 0 {
            return Err(PipelineError::ParseError(errors));
        }
        Ok(Self { file })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct File {
    pub nodes: Vec<Node>,
}

impl File {
    pub fn new(nodes: Vec<Node>) -> Self {
        Self { nodes }
    }
}

impl fmt::Display for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for item in &self.nodes {
            writeln!(f, "{}", item)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Node {
    FuncDecl(node::FuncDecl),
    Assign(node::Assign),
    ConstantDecl(node::ConstantDecl),
    VarDecl(node::VarDecl),
    Call(node::Call),
    Return(node::Return),
    Binary(node::Binary),
    Unary(node::Unary),
    Ident(node::Ident),
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    #[allow(dead_code)]
    Void,
}

impl Node {
    pub fn func(name: Token, params: Vec<Param>, ty: Option<Token>, body: Vec<Node>) -> Self {
        Self::FuncDecl(node::FuncDecl::new(name, params, ty, body))
    }

    pub fn assign(target: Node, expr: Node) -> Self {
        Self::Assign(node::Assign::new(Box::new(target), Box::new(expr)))
    }

    pub fn var(name: Token, ty: Token, init: Node) -> Self {
        Self::VarDecl(node::VarDecl::new(name, ty, Box::new(init)))
    }

    pub fn constant(name: Token, ty: Token, init: Node) -> Self {
        Self::ConstantDecl(node::ConstantDecl::new(name, ty, Box::new(init)))
    }

    pub fn call(
        left: Node,
        args: Vec<Node>,
        parens: (Token, Token),
        delimiters: Vec<Token>,
    ) -> Self {
        Self::Call(node::Call::new(Box::new(left), args, parens, delimiters))
    }

    pub fn ret(expr: Node, token: Token) -> Self {
        Self::Return(node::Return::new(Box::new(expr), token))
    }

    pub fn unary(op: Token, right: Node) -> Self {
        Self::Unary(node::Unary::new(op, Box::new(right)))
    }

    pub fn binary(left: Node, op: Token, right: Node) -> Self {
        Self::Binary(node::Binary::new(Box::new(left), op, Box::new(right)))
    }

    pub fn ident(name: Token) -> Self {
        Self::Ident(node::Ident::new(name))
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Node::FuncDecl(func) => write!(f, "{}", func),
            Node::Assign(assign) => write!(f, "{}", assign),
            Node::VarDecl(var) => write!(f, "{}", var),
            Node::ConstantDecl(constant) => write!(f, "{}", constant),
            Node::Return(ret) => write!(f, "{}", ret),
            Node::Binary(binary) => write!(f, "{}", binary),
            Node::Unary(unary) => write!(f, "{}", unary),
            Node::Call(call) => write!(f, "{}", call),
            Node::Ident(id) => write!(f, "{}", id),
            Node::Float(n) => write!(f, "{}", n),
            Node::Int(i) => write!(f, "{}", i),
            Node::Str(s) => write!(f, "{}", s),
            Node::Bool(b) => write!(f, "{}", b),
            Node::Void => write!(f, "void"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Str,
    Int,
    Float,
    Bool,
    Void,
}

impl Type {
    pub fn as_llvm_basic_type<'a, 'ctx>(&'a self, ir: &Ir<'a, 'ctx>) -> BasicTypeEnum<'ctx> {
        match self {
            Type::Int => ir.context.i64_type().into(),
            Type::Float => ir.context.f64_type().into(),
            Type::Str => ir
                .context
                .i8_type()
                .ptr_type(AddressSpace::default())
                .into(),
            Type::Bool => ir.context.bool_type().into(),
            Type::Void => unimplemented!("void does not implement `BasicTypeEnum`"),
        }
    }
}

impl From<&Token> for Type {
    fn from(value: &Token) -> Self {
        match value
            .lexeme
            .clone()
            .expect("token should have lexeme")
            .as_str()
        {
            "str" => Self::Str,
            "int" => Self::Int,
            "float" => Self::Float,
            "bool" => Self::Bool,
            "void" => Self::Void,
            _ => unreachable!("type lexeme must be one of `str`, `int`, `float`, `bool`, `void`"),
        }
    }
}

impl From<Option<&Token>> for Type {
    fn from(token: Option<&Token>) -> Self {
        match token {
            Some(token) => Self::from(token),
            None => Self::Void,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Param {
    pub name: Token,
    pub ty: Token,
}

impl Param {
    pub fn new(name: Token, ty: Token) -> Self {
        Self { name, ty }
    }
}

macro_rules! assert_ast {
    ($($path:expr => $name:ident),*) => {
        #[cfg(test)]
        mod tests {
            use crate::{Source, ast};

            $(
                #[test]
                fn $name() -> Result<(), Box<dyn std::error::Error>> {
                    let ast = ast::Ast::from_source(Source::new($path)?)?;
                    insta::with_settings!({snapshot_path => "../../snapshots"}, {
                        insta::assert_yaml_snapshot!(ast);
                    });

                    Ok(())
                }
            )*
        }
    };
}

assert_ast!(
    "test-cases/hello.kya" => hello_world,
    "test-cases/expr.kya" => expr,
    "test-cases/calls.kya" => calls,
    "test-cases/empty.kya" => empty,
    "test-cases/access.kya" => access,
    "test-cases/mixed.kya" => mixed
);