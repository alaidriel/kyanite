use crate::{ast::Ast, codegen::Ir, Source};

use super::IrError;

pub struct Builtins {}

impl Builtins {
    pub fn new(ir: &mut Ir<'_, '_>) -> Result<Self, IrError> {
        let source = Source::in_memory(include_str!("stub.kya").to_string());
        let mut ast = Ast::from_source(&source).unwrap();
        for node in &mut ast.nodes {
            ir.decl(node)?;
        }
        Ok(Self {})
    }
}
