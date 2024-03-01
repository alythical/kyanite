use crate::{
    ast::{node, Decl, Type},
    builtins,
};
use std::{collections::HashMap, rc::Rc};

#[derive(Debug, Clone)]
pub enum Symbol {
    Class(Rc<node::ClassDecl>),
    Function(Rc<node::FuncDecl>),
    Constant(Rc<node::ConstantDecl>),
    Variable(Rc<node::VarDecl>),
}

impl Symbol {
    pub fn class(&self) -> &node::ClassDecl {
        match self {
            Symbol::Class(cls) => cls,
            _ => panic!("called `Symbol::class()` on a non-class symbol"),
        }
    }

    pub fn function(&self) -> &node::FuncDecl {
        match self {
            Symbol::Function(fun) => fun,
            _ => panic!("called `Symbol::function()` on a non-function symbol"),
        }
    }

    pub fn ty(&self) -> Type {
        match self {
            Self::Class(cls) => Type::from(&cls.name),
            Self::Function(fun) => fun.ty.as_ref().into(),
            Self::Constant(c) => Type::from(&c.ty),
            Self::Variable(v) => Type::from(&v.ty),
        }
    }
}

crate::newtype!(SymbolTable:HashMap<String, Symbol>);

impl From<&Vec<Decl>> for SymbolTable {
    fn from(nodes: &Vec<Decl>) -> Self {
        let mut table: Self = Self(
            builtins::builtins()
                .nodes
                .iter()
                .map(|decl| match decl {
                    Decl::Function(fun) => (fun.name.to_string(), Symbol::Function(Rc::clone(fun))),
                    Decl::Constant(c) => (c.name.to_string(), Symbol::Constant(Rc::clone(c))),
                    Decl::Class(cls) => (cls.name.to_string(), Symbol::Class(Rc::clone(cls))),
                })
                .collect(),
        );
        nodes
            .iter()
            .for_each(|node| self::SymbolTableVisitor::visit(node, &mut table));
        table
    }
}

trait SymbolTableVisitor {
    fn visit(&self, table: &mut SymbolTable);
}

impl SymbolTableVisitor for Decl {
    fn visit(&self, table: &mut SymbolTable) {
        match self {
            Decl::Function(fun) => func(fun, table),
            Decl::Constant(c) => constant(c, table),
            Decl::Class(cls) => class(cls, table),
        }
    }
}

fn func(fun: &Rc<node::FuncDecl>, table: &mut SymbolTable) {
    table.insert(fun.name.to_string(), Symbol::Function(Rc::clone(fun)));
}

fn constant(c: &Rc<node::ConstantDecl>, table: &mut SymbolTable) {
    table.insert(c.name.to_string(), Symbol::Constant(Rc::clone(c)));
}

fn class(cls: &Rc<node::ClassDecl>, table: &mut SymbolTable) {
    table.insert(cls.name.to_string(), Symbol::Class(Rc::clone(cls)));
}
