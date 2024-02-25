use crate::{
    backend::kyir::{
        alloc::Registers,
        arch::{ArchInstr, FlowGraphMeta, Format},
        ir::RelOp,
    },
    Frame,
};
use std::fmt;

#[non_exhaustive]
#[derive(Debug)]
pub enum A64 {
    /// (kind, value)
    Data(String, String),
    /// (name)
    Label(String),
    /// (dst, src, offset)
    LoadImmediate(String, String, i64),
    StoreImmediate(String, String, i64),
    /// (dst, addr)
    LoadEffective(String, String),
    /// (r1, r2)
    StorePair(String, String),
    /// (r1, r2)
    LoadPair(String, String),
    /// (dst, dst, src)
    Add(String, String, String),
    Sub(String, String, String),
    Mul(String, String, String),
    Div(String, String, String),
    /// (dst, src)
    Move(String, String),
    /// (label, rel)
    Branch(String, Option<RelOp>),
    /// (label)
    BranchLink(String),
    /// (extern)
    Call(String),
    /// (lhs, rhs)
    Compare(String, String),
    Ret,
}

impl ArchInstr for A64 {
    fn create_label(address: String) -> Self {
        A64::Label(address)
    }

    fn data_fragment(kind: String, value: String) -> Self {
        A64::Data(kind, value)
    }

    fn load_fragment(dst: String, label: String) -> Self {
        A64::LoadEffective(dst, label)
    }

    fn copy(dst: String, src: String) -> Self {
        A64::Move(dst, src)
    }

    fn copy_int(dst: String, value: i64) -> Self {
        A64::Move(dst, format!("#{value}"))
    }

    fn add(dst: String, src: String) -> Self {
        A64::Add(dst.clone(), dst, src)
    }

    fn sub(dst: String, src: String) -> Self {
        A64::Sub(dst.clone(), dst, src)
    }

    fn mul(dst: String, src: String) -> Self {
        A64::Mul(dst.clone(), dst, src)
    }

    fn div(dst: String, src: String) -> Self {
        A64::Div(dst.clone(), dst, src)
    }

    fn compare(lhs: String, rhs: String) -> Self {
        A64::Compare(lhs, rhs)
    }

    fn load(dst: String, src: String, offset: i64) -> Self {
        A64::LoadImmediate(dst, src, offset)
    }

    fn store(src: String, addr: String, offset: i64) -> Self {
        A64::StoreImmediate(src, addr, offset)
    }

    fn create_jump(label: String) -> Self {
        A64::Branch(label, None)
    }

    fn conditional_jump(label: String, rel: RelOp) -> Self {
        A64::Branch(label, Some(rel))
    }

    fn call(label: String) -> Self {
        A64::Call(label)
    }
}

impl FlowGraphMeta for A64 {
    fn defines(&self) -> Vec<String> {
        match self {
            A64::LoadImmediate(dst, ..)
            | A64::LoadEffective(dst, ..)
            | A64::Add(dst, ..)
            | A64::Sub(dst, ..)
            | A64::Mul(dst, ..)
            | A64::Div(dst, ..) => {
                vec![dst.clone()]
            }
            A64::LoadPair(r1, r2) => vec![r1.clone(), r2.clone()],
            A64::Move(dst, ..) => vec![dst.clone()],
            _ => vec![],
        }
    }

    fn uses(&self) -> Vec<String> {
        match self {
            A64::StoreImmediate(src, dst, ..) if dst == "x29" => vec![src.clone()],
            A64::StoreImmediate(src, dst, ..) => vec![src.clone(), dst.clone()],
            A64::LoadImmediate(dst, src, ..) if src == "x29" => vec![dst.clone()],
            A64::LoadImmediate(dst, src, ..) => vec![dst.clone(), src.clone()],
            A64::LoadEffective(.., src) | A64::Move(_, src) => {
                vec![src.clone()]
            }
            A64::StorePair(r1, r2)
            | A64::Add(_, r1, r2)
            | A64::Sub(_, r1, r2)
            | A64::Mul(_, r1, r2)
            | A64::Div(_, r1, r2) => vec![r1.clone(), r2.clone()],
            A64::Compare(lhs, rhs) => vec![lhs.clone(), rhs.clone()],
            _ => vec![],
        }
    }

    fn jump(&self) -> bool {
        matches!(self, A64::Branch(..) | A64::BranchLink(..))
    }

    fn label(&self) -> Option<String> {
        match self {
            A64::Label(label) => Some(label.clone()),
            _ => None,
        }
    }

    fn to(&self) -> Option<String> {
        match self {
            A64::BranchLink(label) | A64::Branch(label, ..) => Some(label.clone()),
            _ => None,
        }
    }
}

impl Format for A64 {
    fn format<I: ArchInstr, F: Frame<I>>(self, registers: &Registers) -> Self {
        let get = |temp: String| match &temp[..] {
            "x29" => temp.to_string(),
            _ => registers.get(temp),
        };
        match self {
            A64::LoadImmediate(dst, src, offset) => A64::LoadImmediate(get(dst), get(src), offset),
            A64::StoreImmediate(src, dst, offset) => {
                A64::StoreImmediate(get(src), get(dst), offset)
            }
            A64::LoadEffective(dst, addr) => A64::LoadEffective(get(dst), addr),
            A64::StorePair(r1, r2) => A64::StorePair(get(r1), get(r2)),
            A64::LoadPair(r1, r2) => A64::LoadPair(get(r1), get(r2)),
            A64::Add(dst, r1, r2) => A64::Add(get(dst), get(r1), get(r2)),
            A64::Sub(dst, r1, r2) => A64::Sub(get(dst), get(r1), get(r2)),
            A64::Mul(dst, r1, r2) => A64::Mul(get(dst), get(r1), get(r2)),
            A64::Div(dst, r1, r2) => A64::Div(get(dst), get(r1), get(r2)),
            A64::Move(dst, src) => A64::Move(get(dst), get(src)),
            A64::Compare(lhs, rhs) => A64::Compare(get(lhs), get(rhs)),
            _ => self,
        }
    }
}

impl fmt::Display for A64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pad = " ".repeat(8);
        match self {
            A64::Data(kind, value) => write!(f, "{pad}.{kind} {value}"),
            A64::Label(name) => write!(f, "{name}:"),
            A64::LoadImmediate(dst, src, offset) => write!(f, "{pad}ldr {dst}, [{src}, #{offset}]"),
            A64::StoreImmediate(src, dst, offset) => {
                write!(f, "{pad}str {src}, [{dst}, #{offset}]")
            }
            A64::LoadEffective(dst, addr) => write!(
                f,
                "{pad}adrp {dst}, {addr}@PAGE\n{pad}add {dst}, {dst}, {addr}@PAGEOFF"
            ),
            A64::StorePair(r1, r2) => write!(f, "{pad}stp {r1}, {r2}, [sp, #-16]!"),
            A64::LoadPair(r1, r2) => write!(f, "{pad}ldp {r1}, {r2}, [sp], #16"),
            A64::Add(dst, r1, r2) => write!(f, "{pad}add {dst}, {r1}, {r2}"),
            A64::Sub(dst, r1, r2) => write!(f, "{pad}sub {dst}, {r1}, {r2}"),
            A64::Mul(dst, r1, r2) => write!(f, "{pad}mul {dst}, {r1}, {r2}"),
            A64::Div(dst, r1, r2) => write!(f, "{pad}sdiv {dst}, {r1}, {r2}"),
            A64::Move(dst, src) => write!(f, "{pad}mov {dst}, {src}"),
            A64::Branch(label, rel) => {
                if let Some(rel) = rel {
                    write!(f, "{pad}b{rel} {label}")
                } else {
                    write!(f, "{pad}b {label}")
                }
            }
            A64::BranchLink(label) => write!(f, "{pad}bl {label}"),
            A64::Call(ext) => write!(f, "{pad}bl {ext}"),
            A64::Compare(lhs, rhs) => write!(f, "{pad}cmp {lhs}, {rhs}"),
            A64::Ret => write!(f, "{pad}ret"),
        }
    }
}
