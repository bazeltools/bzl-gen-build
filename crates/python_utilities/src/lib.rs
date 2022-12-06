use rustpython_ast::{Located, StmtKind};

pub mod ast_builder;
pub mod ast_printer;

#[derive(Debug, PartialEq)]
pub struct PythonProgram {
    pub body: Vec<Located<StmtKind>>,
}

impl PythonProgram {
    pub fn parse(source: &str, source_path: &str) -> anyhow::Result<Self> {
        let parsed = rustpython_parser::parser::parse_program(source, source_path)?;
        Ok(Self { body: parsed })
    }
}
