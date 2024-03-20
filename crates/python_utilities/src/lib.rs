use ast::Stmt;
use rustpython_parser::{ast, Parse};

pub mod ast_builder;
pub mod ast_printer;

#[derive(Debug, PartialEq)]
pub struct PythonProgram {
    pub body: Vec<Stmt>,
}

impl PythonProgram {
    pub fn parse(source: &str, source_path: &str) -> anyhow::Result<Self> {
        let parsed = ast::Suite::parse(source, source_path)?;
        Ok(Self { body: parsed })
    }
}
