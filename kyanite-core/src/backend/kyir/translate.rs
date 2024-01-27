use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    ast::Expr as AstExpr,
    ast::{node::RecordDecl, Decl as AstDecl, Initializer, Stmt as AstStmt, Type},
    backend::kyir::arch::Frame,
    pass::{AccessMap, SymbolTable},
    token::Kind,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BinOp {
    Plus,
    Minus,
    Mul,
    Div,
    Xor,
    Cmp(RelOp),
}

impl From<Kind> for BinOp {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Plus => BinOp::Plus,
            Kind::Minus => BinOp::Minus,
            Kind::Star => BinOp::Mul,
            Kind::Slash => BinOp::Div,
            Kind::BangEqual => BinOp::Cmp(RelOp::NotEqual),
            Kind::EqualEqual => BinOp::Cmp(RelOp::Equal),
            Kind::Greater => BinOp::Cmp(RelOp::Greater),
            Kind::GreaterEqual => BinOp::Cmp(RelOp::GreaterEqual),
            Kind::Less => BinOp::Cmp(RelOp::Less),
            Kind::LessEqual => BinOp::Cmp(RelOp::LessEqual),
            _ => unreachable!("not a valid binary operator"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum RelOp {
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
}

pub struct Temp;
pub struct Label;

impl Temp {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> String {
        static ID: AtomicUsize = AtomicUsize::new(0);
        format!("T{}", ID.fetch_add(1, Ordering::SeqCst))
    }
}

impl Label {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> String {
        static ID: AtomicUsize = AtomicUsize::new(0);
        format!("L{}", ID.fetch_add(1, Ordering::SeqCst))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    ConstInt(i64),
    ConstFloat(f64),
    Temp(String),
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Mem(Box<Expr>),
    Call(String, Vec<Expr>),
    ESeq {
        stmt: Box<Stmt>,
        expr: Box<Expr>,
        id: usize,
    },
}

impl Expr {
    pub fn eseq(stmt: Box<Stmt>, expr: Box<Expr>) -> Self {
        static ID: AtomicUsize = AtomicUsize::new(0);
        let id = ID.fetch_add(1, Ordering::SeqCst);
        Self::ESeq { stmt, expr, id }
    }

    pub fn condition(&self) -> Option<RelOp> {
        match self {
            Expr::Binary {
                op: BinOp::Cmp(rel),
                ..
            } => Some(*rel),
            Expr::ConstInt(_) => Some(RelOp::Equal),
            _ => None,
        }
    }

    pub fn temp(&self) -> Option<String> {
        match self {
            Expr::Temp(t) => Some(t.clone()),
            _ => None,
        }
    }

    pub fn int(&self) -> Option<i64> {
        match self {
            Expr::ConstInt(i) => Some(*i),
            _ => None,
        }
    }

    pub fn binary(self) -> Option<(BinOp, Box<Self>, Box<Self>)> {
        match self {
            Expr::Binary { op, left, right } => Some((op, left, right)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// Moves `expr` into the location specified by `target` (either a temporary or a memory offset in the frame)
    Move {
        target: Box<Expr>,
        expr: Box<Expr>,
    },
    Expr(Box<Expr>),
    Label(String),
    /// Evaluates `left`, then evaluates `right`
    Seq {
        left: Box<Stmt>,
        right: Option<Box<Stmt>>,
    },
    Jump(String),
    Noop,
    CJump {
        op: BinOp,
        condition: Box<Expr>,
        t: String,
        f: String,
    },
}

fn fix(expr: Box<Expr>) -> Expr {
    // Refuse to handle moves from memory address to memory address because unsupported
    let temp = Temp::new();
    Expr::eseq(
        Box::new(Stmt::Move {
            target: Box::new(Expr::Temp(temp.clone())),
            expr,
        }),
        Box::new(Expr::Temp(temp)),
    )
}

impl Expr {
    fn checked_binary(op: BinOp, left: Box<Expr>, right: Expr) -> Self {
        let right = match (&*left, right) {
            (Expr::Mem(_), Expr::Mem(expr)) => fix(expr),
            (Expr::Mem(_), Expr::ESeq { stmt, expr, id }) => {
                if matches!(*expr, Expr::Mem(_)) {
                    fix(expr)
                } else {
                    Expr::ESeq { stmt, expr, id }
                }
            }
            (_, right) => right,
        };
        Self::Binary {
            op,
            left,
            right: Box::new(right),
        }
    }
}

impl Stmt {
    fn checked_move(target: Box<Expr>, expr: Expr) -> Self {
        let expr = match (&*target, expr) {
            (Expr::Mem(_), Expr::Mem(expr)) => fix(expr),
            (Expr::Mem(_), Expr::ESeq { stmt, expr, id }) => {
                if matches!(*expr, Expr::Mem(_)) {
                    fix(expr)
                } else {
                    Expr::ESeq { stmt, expr, id }
                }
            }
            (_, expr) => expr,
        };
        Self::Move {
            target,
            expr: Box::new(expr),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Stmt::Seq { left, .. } => left.label(),
            Stmt::Label(label) => label.clone(),
            _ => panic!("called `Stmt::label()` on a non-label statement"),
        }
    }
}

trait Flatten<R, A> {
    fn flatten(&self, aux: A) -> R;
}

impl<F: Frame> Flatten<Vec<Expr>, &'_ mut Translator<'_, F>> for Vec<Initializer> {
    fn flatten(&self, translator: &'_ mut Translator<'_, F>) -> Vec<Expr> {
        self.iter()
            .flat_map(|init| match &init.expr {
                AstExpr::Init(nested) => nested.initializers.flatten(translator),
                _ => vec![init.expr.translate(translator)],
            })
            .collect()
    }
}

impl Flatten<Vec<(String, Type)>, &'_ SymbolTable> for RecordDecl {
    fn flatten(&self, symbols: &'_ SymbolTable) -> Vec<(String, Type)> {
        self.fields
            .iter()
            .flat_map(|field| {
                let ty = Type::from(&field.ty);
                match ty {
                    Type::UserDefined(name) => symbols
                        .get(&name.to_string())
                        .unwrap()
                        .as_record()
                        .flatten(symbols),
                    _ => vec![(field.name.to_string(), ty)],
                }
            })
            .collect()
    }
}

impl From<&[Stmt]> for Stmt {
    fn from(stmts: &[Stmt]) -> Self {
        match stmts.len() {
            0 => Stmt::Noop,
            1 => stmts[0].clone(),
            _ => Stmt::Seq {
                left: Box::new(stmts[0].clone()),
                right: Some(Box::new(Stmt::from(&stmts[1..]))),
            },
        }
    }
}

pub struct Translator<'a, F: Frame> {
    pub functions: HashMap<usize, F>,
    function: Option<usize>,
    accesses: &'a AccessMap,
    symbols: &'a SymbolTable,
}

impl<'a, F: Frame> Translator<'a, F> {
    pub fn new(accesses: &'a AccessMap, symbols: &'a SymbolTable) -> Self {
        Self {
            functions: HashMap::new(),
            function: None,
            accesses,
            symbols,
        }
    }

    pub fn translate(&mut self, ast: &[AstDecl]) -> Vec<Stmt> {
        ast.iter().map(|decl| decl.translate(self)).collect()
    }

    fn frame(&self) -> &F {
        let id: usize = self.function.unwrap();
        self.functions.get(&id).unwrap()
    }
}

trait Translate<R> {
    fn translate<F: Frame>(&self, translator: &mut Translator<F>) -> R;
}

impl Translate<Expr> for AstExpr {
    fn translate<F: Frame>(&self, translator: &mut Translator<F>) -> Expr {
        match self {
            AstExpr::Bool(b, _) => Expr::ConstInt((*b).into()),
            AstExpr::Float(f, _) => Expr::ConstFloat(*f),
            AstExpr::Int(i, _) => Expr::ConstInt(*i),
            AstExpr::Str(..) => todo!(),
            AstExpr::Binary(binary) => Expr::checked_binary(
                binary.op.kind.into(),
                Box::new(binary.left.translate(translator)),
                binary.right.translate(translator),
            ),
            AstExpr::Call(call) => {
                let name = match *call.left {
                    AstExpr::Ident(ref ident) => ident.name.to_string(),
                    AstExpr::Access(_) => todo!(),
                    _ => panic!("Expected either `AstExpr::Ident` or `AstExpr::Access` on left side of call expression"),
                };
                Expr::Call(
                    name,
                    call.args
                        .iter()
                        .map(|arg| arg.translate(translator))
                        .collect(),
                )
            }
            AstExpr::Ident(ident) => translator.frame().get(&ident.name.to_string(), None, None),
            AstExpr::Unary(unary) => match unary.op.kind {
                Kind::Minus => Expr::Binary {
                    op: BinOp::Minus,
                    left: Box::new(Expr::ConstInt(0)),
                    right: Box::new(unary.expr.translate(translator)),
                },
                Kind::Bang => Expr::Binary {
                    op: BinOp::Xor,
                    left: Box::new(unary.expr.translate(translator)),
                    right: Box::new(Expr::ConstInt(1)),
                },
                _ => unreachable!("not a valid unary operator"),
            },
            AstExpr::Access(access) => {
                let temp = Temp::new();
                let frame = translator.frame();
                let aux = translator.accesses.get(&access.id).unwrap();
                let rec = aux.symbols.first().unwrap().as_record();
                let flat = rec.flatten(translator.symbols);
                let parent = access.chain.first().unwrap().as_ident().name.to_string();
                let last = access.chain.last().unwrap().as_ident().name.to_string();
                let (index, _) = flat
                    .iter()
                    .enumerate()
                    .find(|(_, (name, _))| name == &last)
                    .unwrap();
                frame.get(&parent, Some(temp), Some(index))
            }
            AstExpr::Init(init) => {
                let registers = F::registers();
                let ty = Type::from(&init.name);
                let initializers = init.initializers.flatten(translator);
                let name = Temp::new();
                let id = translator.function.unwrap();
                let frame = translator.functions.get_mut(&id).unwrap();
                frame.allocate(translator.symbols, &name, Some(&ty));
                let begin = frame.get_offset(&name);
                let end = begin - i64::try_from((initializers.len() - 1) * F::word_size()).unwrap();
                let stmts: Vec<Stmt> = initializers
                    .into_iter()
                    .enumerate()
                    .map(|(index, expr)| {
                        Stmt::checked_move(
                            Box::new(Expr::Binary {
                                op: BinOp::Plus,
                                left: Box::new(Expr::Temp(registers.frame.to_string())),
                                right: Box::new(Expr::ConstInt(
                                    end + i64::try_from(index * F::word_size()).unwrap(),
                                )),
                            }),
                            expr,
                        )
                    })
                    .collect();
                // Evaluate the initializers, then return start address of initialized memory for record
                Expr::ESeq {
                    stmt: Box::new(Stmt::from(&stmts[..])),
                    expr: Box::new(Expr::ConstInt(begin)),
                    id,
                }
            }
        }
    }
}

impl Translate<Stmt> for AstStmt {
    #[allow(clippy::too_many_lines)]
    fn translate<F: Frame>(&self, translator: &mut Translator<F>) -> Stmt {
        let registers = F::registers();
        match self {
            AstStmt::If(c) => {
                let condition = match &c.condition {
                    AstExpr::Int(i, _) => Expr::Binary {
                        op: BinOp::Cmp(RelOp::Equal),
                        left: Box::new(Expr::ConstInt(*i)),
                        right: Box::new(Expr::ConstInt(0)),
                    },
                    c => c.translate(translator),
                };
                let t = Label::new();
                let f = Label::new();
                let done = Label::new();
                let is: Vec<Stmt> = c.is.iter().map(|stmt| stmt.translate(translator)).collect();
                let otherwise: Vec<Stmt> = c
                    .otherwise
                    .iter()
                    .map(|stmt| stmt.translate(translator))
                    .collect();
                let is = Stmt::from(&is[..]);
                let otherwise = Stmt::from(&otherwise[..]);
                Stmt::Seq {
                    left: Box::new(Stmt::Seq {
                        left: Box::new(Stmt::Seq {
                            left: Box::new(Stmt::Seq {
                                left: Box::new(Stmt::CJump {
                                    op: BinOp::Cmp(condition.condition().unwrap()),
                                    condition: Box::new(condition),
                                    t: t.clone(),
                                    f: f.clone(),
                                }),
                                right: Some(Box::new(Stmt::Seq {
                                    left: Box::new(Stmt::Label(t.clone())),
                                    right: Some(Box::new(is)),
                                })),
                            }),
                            right: Some(Box::new(Stmt::Jump(done.clone()))),
                        }),
                        right: Some(Box::new(Stmt::Seq {
                            left: Box::new(Stmt::Label(f)),
                            right: Some(Box::new(otherwise)),
                        })),
                    }),
                    right: Some(Box::new(Stmt::Label(done))),
                }
            }
            AstStmt::While(c) => {
                let condition = match &c.condition {
                    AstExpr::Int(i, _) => Expr::Binary {
                        op: BinOp::Cmp(RelOp::Equal),
                        left: Box::new(Expr::ConstInt(*i)),
                        right: Box::new(Expr::ConstInt(0)),
                    },
                    c => c.translate(translator),
                };
                let t = Label::new();
                let f = Label::new();
                let test = Label::new();
                let mut body: Vec<Stmt> = c
                    .body
                    .iter()
                    .map(|stmt| stmt.translate(translator))
                    .collect();
                body.push(Stmt::Jump(test.clone()));
                let body = Stmt::from(&body[..]);
                Stmt::Seq {
                    left: Box::new(Stmt::Seq {
                        left: Box::new(Stmt::Seq {
                            left: Box::new(Stmt::Label(test)),
                            right: Some(Box::new(Stmt::CJump {
                                op: BinOp::Cmp(condition.condition().unwrap()),
                                condition: Box::new(condition),
                                t: t.clone(),
                                f: f.clone(),
                            })),
                        }),
                        right: Some(Box::new(Stmt::Seq {
                            left: Box::new(Stmt::Label(t)),
                            right: Some(Box::new(body)),
                        })),
                    }),
                    right: Some(Box::new(Stmt::Label(f))),
                }
            }
            AstStmt::Assign(assign) => Stmt::checked_move(
                Box::new(assign.target.translate(translator)),
                assign.expr.translate(translator),
            ),
            AstStmt::Expr(e) => Stmt::Expr(Box::new(e.translate(translator))),
            AstStmt::Return(ret) => Stmt::checked_move(
                Box::new(Expr::Temp(registers.ret.value.to_string())),
                ret.expr.translate(translator),
            ),
            AstStmt::Var(var) => {
                let id = translator.function.unwrap();
                let name = var.name.to_string();
                let frame = translator.functions.get_mut(&id).unwrap();
                // No matter what, variables are always F::word_size() (either pointer to first element or the value itself)
                let target = frame.allocate(translator.symbols, &name, None);
                let expr = var.expr.translate(translator);
                Stmt::checked_move(Box::new(target), expr)
            }
        }
    }
}

impl Translate<Stmt> for AstDecl {
    fn translate<F: Frame>(&self, translator: &mut Translator<F>) -> Stmt {
        match self {
            AstDecl::Function(function) => {
                // Allocate a new frame for the function
                let frame = F::new(function);
                translator.functions.insert(function.id, frame);
                translator.function = Some(function.id);
                // Translate the body of the function
                let stmts: Vec<Stmt> = vec![Stmt::Label(function.name.to_string())]
                    .into_iter()
                    .chain(function.body.iter().map(|stmt| stmt.translate(translator)))
                    .collect();
                Stmt::from(&stmts[..])
            }
            AstDecl::Record(_) => Stmt::Noop,
            AstDecl::Constant(_) => todo!(),
        }
    }
}

// FIXME: omit because serialized labels and temporaries may differ between runs
// macro_rules! assert_ir {
//     ($($path:expr => $name:ident),*) => {
//         #[cfg(test)]
//         mod tests {
//             use std::collections::HashMap;

//             use crate::{
//                 ast,
//                 kyir::Translator,
//                 pass::{SymbolTable, TypeCheckPass},
//                 PipelineError, Source,
//             };

//             use super::arch::amd64::Amd64;

//             $(
//                 #[test]
//                 fn $name() -> Result<(), Box<dyn std::error::Error>> {
//                     let source = Source::new($path)?;
//                     let ast = ast::Ast::from_source(&source)?;
//                     let symbols = SymbolTable::from(&ast.nodes);
//                     let mut accesses = HashMap::new();
//                     let mut pass = TypeCheckPass::new(&symbols, &mut accesses, source, &ast.nodes);
//                     pass.run().map_err(PipelineError::TypeError)?;
//                     let mut translator: Translator<Amd64> = Translator::new(&accesses, &symbols);
//                     let res = translator.translate(&ast.nodes);
//                     insta::with_settings!({snapshot_path => "../../snapshots"}, {
//                         insta::assert_debug_snapshot!(&res);
//                     });

//                     Ok(())
//                 }
//             )*
//         }
//     };
// }

// assert_ir!(
//     "test-cases/kyir/varied.kya" => varied,
//     "test-cases/kyir/nested-calls.kya" => nested_calls
// );
