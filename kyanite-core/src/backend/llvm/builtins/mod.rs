use crate::{
    ast::Ast,
    backend::llvm::{Ir, IrError},
    Source,
};

pub struct Builtins;

impl Builtins {
    pub fn inject(ir: &mut Ir<'_, '_>) -> Result<(), IrError> {
        let source = Source::in_memory(include_str!("stub.kya").to_string());
        let mut ast = Ast::try_from(&source).unwrap();
        for node in &mut ast.nodes {
            ir.decl(node)?;
        }
        Ok(())
    }
}