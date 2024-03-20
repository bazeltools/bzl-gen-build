use std::borrow::Cow;

use ast::{Arguments, Stmt};
use rustpython_parser::ast;

use crate::PythonProgram;

impl std::fmt::Display for PythonProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", program_to_string(&self.body))
    }
}

struct WritingBuffer<'a> {
    buf: Vec<Cow<'a, str>>,
    indent: usize,
    in_line: bool,
}

impl<'a> WritingBuffer<'a> {
    pub fn new() -> Self {
        WritingBuffer {
            buf: Vec::default(),
            indent: 0,
            in_line: true,
        }
    }
    pub fn indent(&mut self) -> &mut Self {
        self.indent += 1;
        self
    }

    pub fn deindent(&mut self) -> &mut Self {
        self.indent += 1;
        self
    }

    pub fn push(&mut self, other: Cow<'a, str>) {
        if !self.in_line {
            self.buf.push(Cow::Borrowed("\n"));
            for _ in 0..self.indent {
                self.buf.push(Cow::Borrowed("  "))
            }
        }
        self.in_line = true;
        self.buf.push(other);
    }

    pub fn finish_line(&mut self) {
        self.in_line = false
    }

    pub fn finish(self) -> String {
        self.buf.join("")
    }
}

fn emit_args<'a>(args: &'a Arguments, str_buffer: &mut WritingBuffer<'a>) {
    let mut first_arg = true;

    for positional_arg in args.args.iter() {
        if !first_arg {
            str_buffer.push(Cow::Borrowed(", "));
        }
        first_arg = false;
        let positional_arg = &positional_arg.def;
        if positional_arg.annotation.is_some() {
            panic!("Annotation printing not supported {:#?}", positional_arg);
        }
        str_buffer.push(Cow::Borrowed(positional_arg.arg.as_str()));
    }

    if args.kwarg.is_some() {
        panic!("KW arg printing not supported {:#?}", args);
    }
    if !args.kwonlyargs.is_empty() {
        panic!("kwonlyargs arg printing not supported {:#?}", args);
    }
}
fn emit_body<'a>(body: &'a [Stmt], str_buffer: &mut WritingBuffer<'a>) {
    for stmt in body.iter() {
        match &stmt {
            Stmt::Import(ast::StmtImport { range: _, names: _ }) => todo!(),
            Stmt::FunctionDef(ast::StmtFunctionDef {
                range: _,
                name,
                args,
                body,
                decorator_list,
                returns: _,
                type_comment: _,
                type_params: _,
            }) => {
                if !decorator_list.is_empty() {
                    panic!(
                        "Have not implemented how to print function with decorators: {:?}",
                        stmt
                    )
                }
                str_buffer.push(Cow::Borrowed("def "));
                str_buffer.push(Cow::Borrowed(name.as_str()));
                str_buffer.push(Cow::Borrowed("("));
                emit_args(args.as_ref(), str_buffer);
                str_buffer.push(Cow::Borrowed("):"));
                str_buffer.finish_line();
                str_buffer.indent();

                emit_body(body, str_buffer);
                str_buffer.deindent();
                str_buffer.finish_line();
            }

            Stmt::ImportFrom(ast::StmtImportFrom {
                range: _,
                module,
                names,
                level,
            }) => {
                if level.is_some() && level != &Some(ast::Int::new(0)) {
                    panic!(
                        "Have not implemented how to print: {:?}, {:?}, {:?}",
                        module, names, level
                    )
                }
                if !names.is_empty() {
                    let module = module
                        .as_ref()
                        .expect("Should be able to get the module if we are importing names");
                    str_buffer.push(Cow::Borrowed("from "));
                    str_buffer.push(Cow::Borrowed(module.as_str()));
                    str_buffer.push(Cow::Borrowed(" import "));

                    let mut first = true;
                    for nme in names.iter() {
                        if !first {
                            str_buffer.push(Cow::Borrowed(","));
                        }
                        first = false;
                        if let Some(as_name) = &nme.asname {
                            str_buffer.push(Cow::Borrowed(nme.name.as_str()));
                            str_buffer.push(Cow::Borrowed(" as "));
                            str_buffer.push(Cow::Borrowed(as_name.as_str()));
                        } else {
                            str_buffer.push(Cow::Borrowed(nme.name.as_str()));
                        }
                    }
                }

                str_buffer.finish_line();
            }
            Stmt::Pass(ast::StmtPass { range: _ }) => {
                str_buffer.push(Cow::Borrowed("pass"));
                str_buffer.finish_line()
            }

            Stmt::Expr(ast::StmtExpr { range: _, value }) => {
                str_buffer.push(Cow::Owned(format!("{}", value)));
                str_buffer.finish_line()
            }
            _ => {
                panic!("Have not implemented how to print: {:?}", stmt)
            }
        }
    }
}

fn program_to_string(program: &[Stmt]) -> String {
    let mut write_buf = WritingBuffer::new();
    emit_body(program, &mut write_buf);
    write_buf.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_parsing() {
        let python_source = r#"from tensorflow import foo
def cust_fn():
  pass"#;

        let parsed = PythonProgram::parse(python_source, "tmp.py").unwrap();
        let printed_parsed = format!("{}", parsed);

        assert_eq!(
            python_source, printed_parsed,
            "\n\nPrinted parsed was: {}\n\n",
            printed_parsed
        );
    }
}
