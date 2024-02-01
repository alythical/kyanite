mod alloc;
pub mod arch;
mod ir;
mod opcode;
mod translate;

use crate::{
    ast::Decl,
    backend::kyir::{
        alloc::Registers,
        arch::Frame,
        ir::{Binary, CJump, Call, Const, Expr, Jump, Label, Mem, Move, RelOp, Seq, Stmt, Temp},
        opcode::Opcode,
        translate::Translator,
    },
    pass::{AccessMap, SymbolTable},
};
use std::{
    collections::HashMap,
    fmt::{Display, Write},
    sync::atomic::{AtomicUsize, Ordering},
};

pub fn asm<F: Frame>(ast: &[Decl], symbols: &SymbolTable, accesses: &AccessMap) -> String {
    let mut translator: Translator<F> = Translator::new(accesses, symbols);
    let naive = translator.translate(ast);
    let ir = translate::canonicalize(naive);
    let mut codegen: Codegen<F> = Codegen::new(translator.functions(), ast);
    let instrs = codegen.assembly(ir);
    let registers = alloc::registers::<F>(instrs);
    codegen.format(&registers)
}

#[derive(Debug)]
pub struct Codegen<F: Frame> {
    asm: Vec<AsmInstr>,
    functions: HashMap<usize, F>,
    stack: Vec<usize>,
    idents: HashMap<String, usize>,
    call: HashMap<usize, bool>,
}

impl<F: Frame> Codegen<F> {
    fn new(functions: HashMap<usize, F>, ast: &[Decl]) -> Self {
        Self {
            idents: ast
                .iter()
                .filter_map(|decl| {
                    if let Decl::Function(decl) = decl {
                        Some((decl.name.to_string(), decl.id))
                    } else {
                        None
                    }
                })
                .collect(),
            asm: Vec::new(),
            call: HashMap::new(),
            stack: vec![],
            functions,
        }
    }

    #[must_use]
    fn assembly(&mut self, ir: Vec<Stmt>) -> &Vec<AsmInstr> {
        ir.into_iter().for_each(|stmt| stmt.assembly(self, false));
        F::passes(self);
        self.epilogues();
        &self.asm
    }

    fn epilogues(&mut self) {
        let mut instrs = vec![];
        for (name, id) in &self.idents {
            let function = self.functions.get(id).unwrap();
            instrs.push(AsmInstr::new(Instr::Oper {
                opcode: Opcode::Label(format!("{name}.epilogue")),
                src: String::new(),
                dst: String::new(),
                jump: None,
            }));
            for instr in function.epilogue() {
                instrs.push(AsmInstr::new(instr));
            }
        }
        self.asm.append(&mut instrs);
    }

    fn access(mem: &Expr) -> (String, i64) {
        let bin = match mem {
            Expr::Mem(mem) => mem.expr.binary().unwrap(),
            Expr::Binary(bin) => bin,
            _ => panic!("Expected `Expr::Mem` or `Expr::Binary`"),
        };
        (bin.left.temp().unwrap(), bin.right.int().unwrap())
    }

    fn format(self, registers: &Registers) -> String {
        let mut asm = String::new();
        for mut instr in self.asm {
            if let Instr::Oper { dst, src, .. } = &mut instr.inner {
                *dst = registers.get::<F>(dst);
                *src = registers.get::<F>(src);
            };
            writeln!(asm, "{instr}").unwrap();
        }
        asm
    }

    fn emit(&mut self, instr: Instr) {
        self.asm.push(AsmInstr::new(instr));
    }
}

trait Assembly<R> {
    fn assembly<F: Frame>(&self, codegen: &mut Codegen<F>, swap: bool) -> R;
}

impl Assembly<()> for Stmt {
    fn assembly<F: Frame>(&self, codegen: &mut Codegen<F>, _: bool) {
        match self {
            Self::Jump(jmp) => jmp.assembly(codegen, false),
            Self::Label(label) => label.assembly(codegen, false),
            Self::Move(m) => m.assembly(codegen, false),
            Self::CJump(cjmp) => cjmp.assembly(codegen, false),
            Self::Seq(seq) => seq.assembly(codegen, false),
            Self::Expr(e) => {
                e.assembly(codegen, false);
            }
            Self::Noop => {}
        }
    }
}

impl Assembly<String> for Expr {
    fn assembly<F: Frame>(&self, codegen: &mut Codegen<F>, swap: bool) -> String {
        match self {
            Self::Binary(bin) => bin.assembly(codegen, swap),
            Self::ConstInt(i) => i.assembly(codegen, swap),
            Self::Mem(mem) => mem.assembly(codegen, swap),
            Self::Call(call) => call.assembly(codegen, swap),
            Self::Temp(t) => t.name.clone(),
            Self::ConstFloat(_) => todo!(),
            Self::ESeq { .. } => panic!("`Expr::ESeq` not removed by canonicalization"),
        }
    }
}

impl Assembly<String> for Const<i64> {
    fn assembly<F: Frame>(&self, codegen: &mut Codegen<F>, _: bool) -> String {
        let name = Temp::next();
        codegen.emit(Instr::Oper {
            opcode: Opcode::Move,
            dst: name.clone(),
            src: format!("${}", self.value),
            jump: None,
        });
        name
    }
}

impl Assembly<String> for Binary {
    fn assembly<F: Frame>(&self, codegen: &mut Codegen<F>, _: bool) -> String {
        let right = self.right.assembly(codegen, false);
        let left = self.left.assembly(codegen, true);
        codegen.emit(Instr::Oper {
            opcode: Opcode::from(self.op),
            dst: left.clone(),
            src: right,
            jump: None,
        });
        left
    }
}

impl Assembly<String> for Mem {
    fn assembly<F: Frame>(&self, codegen: &mut Codegen<F>, swap: bool) -> String {
        let name = Temp::next();
        let src = name.clone();
        let (rbp, offset) = Codegen::<F>::access(&self.expr);
        let dst = format!("{offset}(%{rbp})");
        let mut oper = Instr::Oper {
            opcode: Opcode::Move,
            dst,
            src,
            jump: None,
        };
        if swap {
            if let Instr::Oper {
                ref mut dst,
                ref mut src,
                ..
            } = oper
            {
                std::mem::swap(dst, src);
            } else {
                panic!("Expected `Instr::Oper`");
            }
        }
        codegen.emit(oper);
        name
    }
}

impl Assembly<String> for Call {
    fn assembly<F: Frame>(&self, codegen: &mut Codegen<F>, _: bool) -> String {
        if let Some(&id) = codegen.stack.last() {
            codegen.call.insert(id, true);
        }
        let args: Vec<_> = self
            .args
            .iter()
            .map(|arg| arg.assembly(codegen, true))
            .enumerate()
            .map(|(i, arg)| Instr::Oper {
                opcode: Opcode::Move,
                dst: F::registers().argument[i].into(),
                src: arg,
                jump: None,
            })
            .collect();
        args.into_iter().for_each(|arg| codegen.emit(arg));
        codegen.emit(Instr::Call {
            name: self.name.clone(),
        });
        format!("%{}", F::registers().ret.value)
    }
}

impl Assembly<()> for Jump {
    fn assembly<F: Frame>(&self, codegen: &mut Codegen<F>, _: bool) {
        if self.target.ends_with("epilogue") {
            codegen.stack.pop();
        }
        codegen.emit(Instr::Oper {
            opcode: Opcode::Jump,
            dst: String::new(),
            src: String::new(),
            jump: Some(self.target.clone()),
        });
    }
}

impl Assembly<()> for Label {
    fn assembly<F: Frame>(&self, codegen: &mut Codegen<F>, _: bool) {
        let id = codegen.idents.get(&self.name).copied();
        codegen.emit(Instr::Oper {
            opcode: Opcode::Label(self.name.clone()),
            dst: String::new(),
            src: String::new(),
            jump: None,
        });
        if let Some(id) = id {
            codegen.stack.push(id);
            let function = codegen.functions.get(&id).unwrap();
            for instr in function.prologue() {
                codegen.emit(instr);
            }
        }
    }
}

impl Assembly<()> for Move {
    fn assembly<F: Frame>(&self, codegen: &mut Codegen<F>, _: bool) {
        if let Expr::Binary(ref bin) = *self.expr {
            if bin.left.temp().is_some_and(|t| t == "rbp") {
                let (rbp, offset) = Codegen::<F>::access(&self.expr);
                codegen.emit(Instr::Oper {
                    opcode: Opcode::Move,
                    dst: self.target.temp().unwrap(),
                    src: format!("{offset}(%{rbp})"),
                    jump: None,
                });
                return;
            }
        }
        let src = match *self.expr {
            Expr::Mem(_) => {
                let temp = Temp::next();
                let (rbp, offset) = Codegen::<F>::access(&self.expr);
                codegen.emit(Instr::Oper {
                    opcode: Opcode::Move,
                    dst: temp.clone(),
                    src: format!("{offset}(%{rbp})"),
                    jump: None,
                });
                temp
            }
            _ => self.expr.assembly(codegen, false),
        };
        let dst = match *self.target {
            Expr::Mem(_) | Expr::Binary { .. } => {
                let (rbp, offset) = Codegen::<F>::access(&self.target);
                format!("{offset}(%{rbp})")
            }
            _ => self.target.temp().unwrap(),
        };
        codegen.emit(Instr::Oper {
            opcode: Opcode::Move,
            dst,
            src,
            jump: None,
        });
    }
}

impl Assembly<()> for CJump {
    fn assembly<F: Frame>(&self, codegen: &mut Codegen<F>, _: bool) {
        let tmp = self.condition.assembly(codegen, false);
        if let Expr::ConstInt(_) = *self.condition {
            let one = Temp::next();
            codegen.emit(Instr::oper(Opcode::Move, one.clone(), "$1".into(), None));
            codegen.emit(Instr::oper(Opcode::Cmp(RelOp::Equal), tmp, one, None));
            codegen.emit(Instr::oper(
                Opcode::CJump(self.op.into()),
                String::new(),
                String::new(),
                Some(self.t.clone()),
            ));
        } else {
            codegen.emit(Instr::Oper {
                opcode: Opcode::CJump(self.op.into()),
                dst: String::new(),
                src: String::new(),
                jump: Some(self.t.clone()),
            });
        }
    }
}

impl Assembly<()> for Seq {
    fn assembly<F: Frame>(&self, codegen: &mut Codegen<F>, _: bool) {
        self.left.assembly(codegen, false);
        if let Some(right) = &self.right {
            right.assembly(codegen, false);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Instr {
    Oper {
        opcode: Opcode,
        dst: String,
        src: String,
        jump: Option<String>,
    },
    Call {
        name: String,
    },
}

impl Instr {
    fn oper(opcode: Opcode, dst: String, src: String, jump: Option<String>) -> Self {
        Self::Oper {
            opcode,
            dst,
            src,
            jump,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct AsmInstr {
    inner: Instr,
    id: usize,
}

impl AsmInstr {
    fn new(instr: Instr) -> Self {
        static ID: AtomicUsize = AtomicUsize::new(0);
        let id = ID.fetch_add(1, Ordering::SeqCst);
        Self { inner: instr, id }
    }

    fn operands(&self) -> usize {
        match &self.inner {
            Instr::Oper { opcode, .. } => match opcode {
                Opcode::Jump | Opcode::CJump(_) | Opcode::Push | Opcode::Ret | Opcode::Pop => 1,
                Opcode::Label(_) => 0,
                _ => 2,
            },
            Instr::Call { .. } => 0,
        }
    }

    fn to(&self) -> Option<String> {
        match &self.inner {
            Instr::Oper { jump, .. } => jump.as_ref().cloned(),
            Instr::Call { .. } => None,
        }
    }

    fn jump(&self) -> bool {
        matches!(
            self.inner,
            Instr::Oper {
                opcode: Opcode::Jump,
                ..
            }
        )
    }

    fn label(&self) -> Option<String> {
        match &self.inner {
            Instr::Oper {
                opcode: Opcode::Label(label),
                ..
            } => Some(label.clone()),
            _ => None,
        }
    }
}

impl Display for AsmInstr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            Instr::Oper {
                opcode,
                dst,
                src,
                jump,
            } => {
                match opcode {
                    Opcode::Jump => write!(f, "{opcode} {}", jump.as_ref().unwrap())?,
                    Opcode::CJump(_) => write!(f, "{opcode} {}", jump.as_ref().unwrap())?,
                    _ => write!(
                        f,
                        "{opcode} {src}{} {dst}",
                        if self.operands() == 2 { "," } else { "" }
                    )?,
                }
                Ok(())
            }
            Instr::Call { name } => {
                write!(f, "callq {name}")?;
                Ok(())
            }
        }
    }
}
