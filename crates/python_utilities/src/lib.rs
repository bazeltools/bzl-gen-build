use ast::Stmt;
use rustpython_parser::{ast, Parse};

pub mod ast_builder;
pub mod ast_printer;
use ast_printer::{emit_body, WritingBuffer};

#[derive(PartialEq)]
pub struct PythonProgram {
    pub body: Vec<Stmt>,
}

impl std::fmt::Display for PythonProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", program_to_string(&self.body))
    }
}

impl PythonProgram {
    pub fn parse(source: &str, source_path: &str) -> anyhow::Result<Self> {
        let parsed = ast::Suite::parse(source, source_path)?;
        Ok(Self { body: parsed })
    }
}

fn program_to_string(program: &[Stmt]) -> String {
    let mut write_buf = WritingBuffer::new();
    emit_body(program, &mut write_buf);
    write_buf.finish()
}
