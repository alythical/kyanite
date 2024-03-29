#[allow(clippy::wildcard_imports)]
use crate::{
    ast::{
        node::*,
        ty::{Type, TypeParameter},
        Decl, Expr, Stmt,
    },
    error::PreciseError,
    token::{Kind, Span, Token},
    Source,
};
use std::{collections::VecDeque, rc::Rc};

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    #[error("unexpected EOF")]
    UnexpectedEof(Span),
    #[error("expected {0} but found {2}")]
    Expected(Kind, Span, Kind),
    #[error("unexpected {0}")]
    Unhandled(Kind, Span, &'static [Kind]),
}

pub struct Parser<'a> {
    source: &'a Source,
    tokens: VecDeque<Token>,
    errors: Vec<PreciseError<'a>>,
    previous: Option<Token>,
    panic: bool,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a Source, tokens: VecDeque<Token>) -> Self {
        Self {
            source,
            tokens,
            panic: false,
            errors: vec![],
            previous: None,
        }
    }

    pub fn parse(&mut self) -> Result<Vec<Decl>, &Vec<PreciseError<'a>>> {
        let mut nodes: Vec<Decl> = vec![];
        while let Ok(token) = self.peek() {
            match match token.kind {
                Kind::Class => self.class(),
                Kind::Fun => self.function(&None, false),
                Kind::Extern => self.function(&None, true),
                Kind::Const => self.constant(),
                Kind::Eof => break,
                _ => {
                    let token = self.advance().unwrap();
                    Err(ParseError::Unhandled(
                        token.kind,
                        token.span,
                        &[Kind::Fun, Kind::Const],
                    ))
                }
            } {
                Ok(node) => nodes.push(node),
                Err(e) => {
                    self.error(&e);
                    self.synchronize(false);
                }
            }
        }
        if self.errors.is_empty() {
            Ok(nodes)
        } else {
            Err(&self.errors)
        }
    }

    fn class(&mut self) -> Result<Decl, ParseError> {
        self.consume(Kind::Class)?;
        let name = self.consume(Kind::Identifier)?;
        let tp = (self.peek()?.kind == Kind::Less)
            .then(|| self.type_parameters())
            .transpose()?;
        let parent = (self.peek()?.kind == Kind::Colon)
            .then(|| {
                self.consume(Kind::Colon)?;
                self.consume(Kind::Identifier)
            })
            .transpose()?;
        self.consume(Kind::LeftBrace)?;
        let fields = self.fields()?;
        let mut methods = vec![];
        while self.peek()?.kind != Kind::RightBrace {
            methods.push(match self.function(&Some(name.clone()), false)? {
                Decl::Function(fun) => fun,
                _ => unreachable!(),
            });
        }
        self.consume(Kind::RightBrace)?;
        Ok(ClassDecl::wrapped(name, fields, methods, parent, tp))
    }

    fn type_parameters(&mut self) -> Result<Vec<TypeParameter>, ParseError> {
        let mut tp = vec![];
        self.consume(Kind::Less)?;
        while self.peek()?.kind != Kind::Greater {
            let name = self.consume(Kind::Identifier)?;
            let bound = (!matches!(self.peek()?.kind, Kind::Comma | Kind::Greater))
                .then(|| {
                    self.consume(Kind::Colon)?;
                    self.consume(Kind::Identifier)
                })
                .transpose()?;
            if self.peek()?.kind != Kind::Greater {
                self.consume(Kind::Comma)?;
            }
            tp.push(TypeParameter::new(name, bound));
        }
        self.consume(Kind::Greater)?;
        Ok(tp)
    }

    fn function(&mut self, method: &Option<Token>, external: bool) -> Result<Decl, ParseError> {
        if external {
            self.consume(Kind::Extern)?;
        }
        self.consume(Kind::Fun)?;
        let name = self.consume(Kind::Identifier)?;
        let tp = (self.peek()?.kind == Kind::Less)
            .then(|| self.type_parameters())
            .transpose()?
            .unwrap_or(vec![]);
        self.consume(Kind::LeftParen)?;
        let params = self.params(method)?;
        self.consume(Kind::RightParen)?;
        let mut ty: Option<Type> = None;
        if self.peek()?.kind == Kind::Colon {
            self.consume(Kind::Colon)?;
            ty = Some(self.ty()?);
        }
        if external {
            Ok(FuncDecl::wrapped(name, params, ty, tp, vec![], external))
        } else {
            Ok(FuncDecl::wrapped(
                name,
                params,
                ty,
                tp,
                self.block()?,
                external,
            ))
        }
    }

    fn params(&mut self, method: &Option<Token>) -> Result<Vec<Param>, ParseError> {
        let mut params: Vec<Param> = vec![];
        let mut index = 0;
        while self.peek()?.kind != Kind::RightParen {
            let name = self.consume(Kind::Identifier)?;
            let ty = if method.as_ref().is_some_and(|_| index == 0) {
                index += 1;
                Type::new(method.clone().unwrap(), vec![])
            } else {
                self.consume(Kind::Colon)?;
                self.ty()?
            };
            params.push(Param::new(name, ty));
            if self.peek()?.kind != Kind::RightParen {
                self.consume(Kind::Comma)?;
            }
        }
        Ok(params)
    }

    fn fields(&mut self) -> Result<Vec<Field>, ParseError> {
        let mut fields: Vec<Field> = vec![];
        while !matches!(self.peek()?.kind, Kind::RightBrace | Kind::Fun) {
            let name = self.consume(Kind::Identifier)?;
            self.consume(Kind::Colon)?;
            let ty = self.ty()?;
            fields.push(Field::new(name, ty));
            if !matches!(self.peek()?.kind, Kind::RightBrace | Kind::Fun) {
                self.consume(Kind::Comma)?;
            }
        }
        Ok(fields)
    }

    fn ty(&mut self) -> Result<Type, ParseError> {
        let base = self.consume(Kind::Identifier)?;
        (self.peek()?.kind == Kind::Less)
            .then(|| {
                self.consume(Kind::Less)?;
                let mut params = vec![];
                while self.peek()?.kind != Kind::Greater {
                    params.push(self.ty()?);
                    if self.peek()?.kind != Kind::Greater {
                        self.consume(Kind::Comma)?;
                    }
                }
                self.consume(Kind::Greater)?;
                Ok(params)
            })
            .transpose()
            .map(|params| Type::new(base, params.unwrap_or_default()))
    }

    fn block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.consume(Kind::LeftBrace)?;
        let mut stmts: Vec<Stmt> = vec![];
        while self.peek()?.kind != Kind::RightBrace {
            let stmt = self.statement();
            match stmt {
                Ok(stmt) => stmts.push(stmt),
                Err(e) => {
                    self.error(&e);
                    self.synchronize(true);
                }
            }
        }
        self.consume(Kind::RightBrace)?;
        Ok(stmts)
    }

    fn constant(&mut self) -> Result<Decl, ParseError> {
        self.consume(Kind::Const)?;
        let name = self.consume(Kind::Identifier)?;
        self.consume(Kind::Colon)?;
        let ty = self.ty()?;
        self.consume(Kind::Equal)?;
        let value = self.expression()?;
        self.consume(Kind::Semicolon)?;
        Ok(ConstantDecl::wrapped(name, ty, value))
    }

    fn declaration(&mut self) -> Result<Stmt, ParseError> {
        self.consume(Kind::Let)?;
        let name = self.consume(Kind::Identifier)?;
        self.consume(Kind::Colon)?;
        let ty = self.ty()?;
        self.consume(Kind::Equal)?;
        let expr = self.expression()?;
        self.consume(Kind::Semicolon)?;
        Ok(Stmt::Var(Rc::new(VarDecl { name, ty, expr })))
    }

    fn condition(&mut self) -> Result<Stmt, ParseError> {
        self.consume(Kind::If)?;
        let condition = self.expression()?;
        let is = self.block()?;
        if self.peek()?.kind == Kind::Else {
            self.consume(Kind::Else)?;
            let otherwise = self.block()?;
            Ok(If::wrapped(condition, is, otherwise))
        } else {
            Ok(If::wrapped(condition, is, vec![]))
        }
    }

    fn r#for(&mut self) -> Result<Stmt, ParseError> {
        self.consume(Kind::For)?;
        let index = self.consume(Kind::Identifier)?;
        self.consume(Kind::In)?;
        let range = self.range()?;
        let block = self.block()?;
        Ok(For::wrapped(index, range, block))
    }

    fn r#while(&mut self) -> Result<Stmt, ParseError> {
        self.consume(Kind::While)?;
        let condition = self.expression()?;
        let block = self.block()?;
        Ok(While::wrapped(condition, block))
    }

    fn statement(&mut self) -> Result<Stmt, ParseError> {
        match self.peek()?.kind {
            Kind::Let => self.declaration(),
            Kind::If => self.condition(),
            Kind::For => self.r#for(),
            Kind::While => self.r#while(),
            Kind::Return => {
                let keyword = self.consume(Kind::Return)?;
                let expr = self.expression()?;
                self.consume(Kind::Semicolon)?;
                Ok(Return::wrapped(expr, keyword))
            }
            _ => self.assignment(),
        }
    }

    fn assignment(&mut self) -> Result<Stmt, ParseError> {
        let item = self.expression()?;
        if self.peek()?.kind == Kind::Equal {
            self.consume(Kind::Equal)?;
            let right = self.expression()?;
            self.consume(Kind::Semicolon)?;
            Ok(Assign::wrapped(item, right))
        } else {
            self.consume(Kind::Semicolon)?;
            Ok(Stmt::Expr(item))
        }
    }

    fn expression(&mut self) -> Result<Expr, ParseError> {
        self.equality()
    }

    fn range(&mut self) -> Result<Expr, ParseError> {
        let left = self.consume(Kind::LeftBracket)?;
        let start = self.expression()?;
        self.consume(Kind::Comma)?;
        let end = self.expression()?;
        let right = self.consume(Kind::RightBracket)?;
        Ok(Range::wrapped(start, end, (left, right)))
    }

    fn equality(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.comparison()?;
        while matches!(self.peek()?.kind, Kind::BangEqual | Kind::EqualEqual) {
            let operator = self.advance().unwrap();
            let right = self.comparison()?;
            expr = Binary::wrapped(expr, operator, right);
        }
        Ok(expr)
    }

    fn comparison(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.term()?;
        while matches!(
            self.peek()?.kind,
            Kind::Greater | Kind::GreaterEqual | Kind::Less | Kind::LessEqual
        ) {
            let operator = self.advance().unwrap();
            let right = self.term()?;
            expr = Binary::wrapped(expr, operator, right);
        }
        Ok(expr)
    }

    fn term(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.factor()?;
        while matches!(self.peek()?.kind, Kind::Minus | Kind::Plus) {
            let operator = self.advance().unwrap();
            let right = self.factor()?;
            expr = Binary::wrapped(expr, operator, right);
        }
        Ok(expr)
    }

    fn factor(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.unary()?;
        while matches!(self.peek()?.kind, Kind::Slash | Kind::Star) {
            let operator = self.advance().unwrap();
            let right = self.unary()?;
            expr = Binary::wrapped(expr, operator, right);
        }
        Ok(expr)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        Ok(match self.peek()?.kind {
            Kind::Bang | Kind::Minus => {
                let operator = self.advance().unwrap();
                let right = self.unary()?;
                Unary::wrapped(operator, right)
            }
            _ => self.access()?,
        })
    }

    fn access(&mut self) -> Result<Expr, ParseError> {
        let item = self.call()?;
        let mut chain: Vec<Expr> = vec![];
        while self.peek()?.kind == Kind::Dot {
            self.consume(Kind::Dot)?;
            let el = self.call()?;
            chain.push(el.clone());
            if self.peek()?.kind != Kind::Dot {
                chain.insert(0, item);
                if let Expr::Call(call) = el {
                    let whole = chain.clone();
                    chain.pop();
                    return Ok(Call::wrapped(
                        Access::wrapped(whole),
                        vec![Access::wrapped(chain)]
                            .into_iter()
                            .chain(call.args.clone())
                            .collect(),
                        call.parens.clone(),
                        call.delimiters.clone(),
                    ));
                }
                return Ok(Access::wrapped(chain));
            }
        }
        Ok(item)
    }

    fn call(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.primary()?;
        if self.peek()?.kind == Kind::LeftParen {
            let open = self.consume(Kind::LeftParen)?;
            let mut args: Vec<Expr> = vec![];
            let mut delimiters: Vec<Token> = vec![];
            if self.peek()?.kind != Kind::RightParen {
                args.push(self.expression()?);
                while self.peek()?.kind == Kind::Comma {
                    delimiters.push(self.consume(Kind::Comma)?);
                    args.push(self.expression()?);
                }
            }
            let close = self.consume(Kind::RightParen)?;
            left = Call::wrapped(left, args, (open, close), delimiters);
        }
        Ok(left)
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        Ok(match self.peek()?.kind {
            Kind::LeftParen => {
                self.consume(Kind::LeftParen)?;
                let expr = self.expression()?;
                self.consume(Kind::RightParen)?;
                expr
            }
            Kind::Literal => {
                let token = self.advance().unwrap();
                let lexeme = token.lexeme.unwrap();
                match lexeme {
                    "true" | "false" => Literal::<bool>::bool(lexeme == "true", token),
                    _ if lexeme.starts_with('"') => Literal::<&str>::string(lexeme, token),
                    _ if lexeme.contains('.') => {
                        Literal::<f64>::float(lexeme.parse().unwrap(), token)
                    }
                    _ if lexeme.chars().next().unwrap().is_ascii_digit() => {
                        Literal::<i64>::int(lexeme.parse().unwrap(), token)
                    }
                    e => unreachable!("impossible lexeme `{}`", e),
                }
            }
            Kind::Identifier => {
                let name = self.advance().unwrap();
                if self.peek()?.kind == Kind::Colon {
                    self.init(name)?
                } else {
                    Ident::wrapped(name)
                }
            }
            _ => Err(ParseError::Unhandled(
                self.peek()?.kind,
                self.peek()?.span,
                &[Kind::Identifier, Kind::Literal, Kind::LeftParen],
            ))?,
        })
    }

    fn init(&mut self, name: Token) -> Result<Expr, ParseError> {
        self.consume(Kind::Colon)?;
        self.consume(Kind::Init)?;
        let left = self.consume(Kind::LeftParen)?;
        let mut initializers: Vec<Initializer> = vec![];
        while self.peek()?.kind != Kind::RightParen {
            let name = self.consume(Kind::Identifier)?;
            self.consume(Kind::Colon)?;
            let value = self.expression()?;
            initializers.push(Initializer::new(name, value));
            if self.peek()?.kind != Kind::RightParen {
                self.consume(Kind::Comma)?;
            }
        }
        let right = self.consume(Kind::RightParen)?;
        Ok(Init::wrapped(name, initializers, (left, right)))
    }

    fn consume(&mut self, kind: Kind) -> Result<Token, ParseError> {
        if self.eof() {
            return Err(ParseError::Expected(
                kind,
                self.previous.as_ref().unwrap().span,
                Kind::Eof,
            ));
        }
        let token = self.advance().unwrap();
        if token.kind == kind {
            Ok(token)
        } else {
            Err(ParseError::Expected(kind, token.span, token.kind))
        }
    }

    fn eof(&self) -> bool {
        self.tokens.is_empty()
    }

    fn peek(&self) -> Result<&Token, ParseError> {
        let span = if self.previous.is_some() {
            if self.eof() {
                return Err(ParseError::UnexpectedEof(Span::new(0, 0, 0)));
            }
            self.previous.as_ref().unwrap().span
        } else {
            Span::new(0, 0, 0)
        };
        match self.tokens.front() {
            Some(token) => Ok(token),
            None => Err(ParseError::UnexpectedEof(span)),
        }
    }

    fn error(&mut self, e: &ParseError) {
        self.panic = true;
        let span = *match &e {
            ParseError::Unhandled(_, span, _)
            | ParseError::UnexpectedEof(span)
            | ParseError::Expected(_, span, _) => span,
        };
        let detail = match e {
            ParseError::Expected(expected, _, _) => format!("expected {expected} here"),
            ParseError::Unhandled(_, _, expected) => {
                let expected = expected
                    .iter()
                    .map(|t| format!("`{t}`"))
                    .collect::<Vec<String>>()
                    .join(", ");
                format!("expected one of {expected} here")
            }
            ParseError::UnexpectedEof(_) => "unexpected end of file".into(),
        };
        let error = PreciseError::new(self.source, span, format!("{e}"), detail);
        println!("{error}");
        self.errors.push(error);
    }

    fn synchronize(&mut self, stmt: bool) {
        self.panic = false;
        if stmt {
            self.advance();
        }
        while !self.eof() {
            if self.previous.as_ref().unwrap().kind == Kind::Semicolon
                || self.previous.as_ref().unwrap().span.line < self.peek().unwrap().span.line
            {
                return;
            }
            if matches!(
                self.peek().unwrap().kind,
                Kind::Let | Kind::Fun | Kind::Const
            ) {
                return;
            }
            self.advance();
        }
    }

    fn advance(&mut self) -> Option<Token> {
        let previous = self.tokens.pop_front();
        self.previous = previous.clone();
        previous
    }
}

macro_rules! assert_parse {
    ($($path:expr => $name:ident / $valid:expr),*) => {
        #[cfg(test)]
        mod tests {
            use crate::{parse::Parser, token::{Lexer}, Source};

            $(
                #[test]
                fn $name() -> Result<(), Box<dyn std::error::Error>> {
                    let source = Source::new($path)?;
                    let lexer = Lexer::from(&source);
                    let mut parser = Parser::new(lexer.source, lexer.tokens);
                    let res = parser.parse();
                    insta::with_settings!({snapshot_path => "../snapshots"}, {
                        if $valid {
                            assert!(res.is_ok());
                        } else {
                            insta::assert_debug_snapshot!(res.unwrap_err());
                        }
                    });

                    Ok(())
                }
            )*
        }
    };
}

assert_parse! {
    "test-cases/hello.kya" => hello_world / true,
    "test-cases/expr.kya" => expr / true,
    "test-cases/calls.kya" => calls / true,
    "test-cases/empty.kya" => empty / true,
    "test-cases/access.kya" => access / true,
    "test-cases/mixed.kya" => mixed / true,

    "test-cases/parser/simple.kya" => simple / false,
    "test-cases/parser/toplevel.kya" => toplevel / false,
    "test-cases/parser/nested.kya" => nested / false
}
