use std::borrow::Cow;

use ast::{Arguments, Stmt};
use rustpython_parser::ast;

pub(crate) struct WritingBuffer<'a> {
    buf: Vec<Cow<'a, str>>,
    indent: usize,
    offset: usize,
    in_line: bool,
    in_keyword: bool,
}

impl<'a> WritingBuffer<'a> {
    pub fn new() -> Self {
        WritingBuffer {
            buf: Vec::default(),
            indent: 0,
            offset: 0,
            in_line: true,
            in_keyword: false,
        }
    }

    pub fn at_start(&self) -> bool {
        self.offset == 0
    }

    pub fn indent(&mut self) -> &mut Self {
        self.indent += 1;
        self
    }

    pub fn deindent(&mut self) -> &mut Self {
        self.indent -= 1;
        self
    }

    pub fn push(&mut self, other: &'a str) -> &mut Self {
        self.push_cow(Cow::Borrowed(other))
    }

    pub fn push_cow(&mut self, other: Cow<'a, str>) -> &mut Self {
        if !self.in_line {
            self.offset += 1;
            self.buf.push(Cow::Borrowed("\n"));
            let indent_str = Cow::Borrowed("    ");
            for _ in 0..self.indent {
                self.buf.push(indent_str.clone());
                self.offset += 4
            }
        }
        self.in_line = true;
        self.offset += other.len();
        self.buf.push(other);
        self
    }

    pub fn finish_line(&mut self) -> &mut Self {
        self.in_line = false;
        self
    }

    pub fn finish(self) -> String {
        self.buf.join("")
    }

    pub fn begin_keyword(&mut self) -> &mut Self {
        self.in_keyword = true;
        self
    }

    pub fn finish_keyword(&mut self) -> &mut Self {
        self.in_keyword = false;
        self
    }
}

fn emit_args<'a>(args: &'a Arguments, str_buffer: &mut WritingBuffer<'a>) {
    let mut first_arg = true;

    for positional_arg in args.args.iter() {
        if !first_arg {
            str_buffer.push(", ");
        }
        first_arg = false;
        let positional_arg = &positional_arg.def;
        if positional_arg.annotation.is_some() {
            panic!("Annotation printing not supported {:#?}", positional_arg);
        }
        str_buffer.push(positional_arg.arg.as_str());
    }

    if args.kwarg.is_some() {
        panic!("KW arg printing not supported {:#?}", args);
    }
    if !args.kwonlyargs.is_empty() {
        panic!("kwonlyargs arg printing not supported {:#?}", args);
    }
}
pub(crate) fn emit_body<'a>(body: &'a [Stmt], str_buffer: &mut WritingBuffer<'a>) {
    for stmt in body {
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
                str_buffer.push("def ").push(name.as_str()).push("(");
                emit_args(args.as_ref(), str_buffer);
                str_buffer.push("):").finish_line().indent();

                emit_body(body, str_buffer);
                str_buffer.deindent().finish_line();
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
                    str_buffer
                        .push("from ")
                        .push(module.as_str())
                        .push(" import ");

                    let mut first = true;
                    for nme in names.iter() {
                        if !first {
                            str_buffer.push(",");
                        }
                        first = false;
                        if let Some(as_name) = &nme.asname {
                            str_buffer
                                .push(nme.name.as_str())
                                .push(" as ")
                                .push(as_name.as_str());
                        } else {
                            str_buffer.push(nme.name.as_str());
                        }
                    }
                }

                str_buffer.finish_line();
            }
            Stmt::Pass(ast::StmtPass { range: _ }) => {
                str_buffer.push("pass").finish_line();
            }

            Stmt::Expr(ast::StmtExpr { range: _, value }) => {
                value.custom_fmt(str_buffer, false);
                str_buffer.finish_line();
            }
            _ => {
                panic!("Have not implemented how to print: {:?}", stmt)
            }
        }
    }
}

/**
 * CustomDisplay represents a mutable pretty printing.
 * WritingBuffer keeps track of the indentation state, so we can't just return string.
 * This allows, e.g. a nested list items to be double-indented.
 *
 * defer - When true, it does not push to str_buffer, and just returns the String value.
 *         This functionality is partially implemented since we need it only to pick up
 *         the function name.
 */
trait CustomDisplay {
    fn custom_fmt(&self, str_buffer: &mut WritingBuffer, defer: bool) -> String;
}

impl CustomDisplay for ast::Expr {
    fn custom_fmt(&self, str_buffer: &mut WritingBuffer, defer: bool) -> String {
        fn push(str_buffer: &mut WritingBuffer, defer: bool, s: String) -> String {
            if !defer {
                str_buffer.push_cow(Cow::Owned(s.clone()));
            }
            s
        }
        fn push_inline_list(
            str_buffer: &mut WritingBuffer,
            defer: bool,
            begin_marker: &str,
            args: &Vec<ast::Expr>,
            keywords: &Vec<ast::Keyword>,
            end_marker: &str,
        ) {
            str_buffer.push_cow(Cow::Owned(begin_marker.to_string()));
            for (idx, arg) in args.iter().enumerate() {
                arg.custom_fmt(str_buffer, defer);
                if idx < args.len() - 1 {
                    str_buffer.push(", ");
                }
            }
            for (idx, kw) in keywords.iter().enumerate() {
                str_buffer.begin_keyword();
                match &kw.arg {
                    Some(arg) => {
                        str_buffer.push_cow(Cow::Owned(format!("{} = ", arg)));
                    }
                    None => (),
                };
                kw.value.custom_fmt(str_buffer, defer);
                if idx < keywords.len() - 1 {
                    str_buffer.push(", ");
                }
                str_buffer.finish_keyword();
            }
            str_buffer.push_cow(Cow::Owned(end_marker.to_string()));
        }
        fn push_multi_line_list(
            str_buffer: &mut WritingBuffer,
            defer: bool,
            begin_marker: &str,
            args: &Vec<ast::Expr>,
            keywords: &Vec<ast::Keyword>,
            end_marker: &str,
        ) {
            str_buffer
                .push_cow(Cow::Owned(begin_marker.to_string()))
                .finish_line()
                .indent();
            for arg in args.iter() {
                arg.custom_fmt(str_buffer, defer);
                str_buffer.push(",").finish_line();
            }
            for kw in keywords.iter() {
                str_buffer.begin_keyword();
                match &kw.arg {
                    Some(arg) => {
                        str_buffer.push_cow(Cow::Owned(format!("{} = ", arg)));
                    }
                    None => (),
                };
                kw.value.custom_fmt(str_buffer, defer);
                str_buffer.push(",").finish_keyword().finish_line();
            }
            str_buffer
                .deindent()
                .push_cow(Cow::Owned(end_marker.to_string()));
        }
        fn push_list(
            str_buffer: &mut WritingBuffer,
            defer: bool,
            begin_marker: &str,
            args: &Vec<ast::Expr>,
            keywords: &Vec<ast::Keyword>,
            end_marker: &str,
        ) {
            if args.len() + keywords.len() < 2 {
                push_inline_list(str_buffer, defer, begin_marker, args, keywords, end_marker)
            } else {
                push_multi_line_list(str_buffer, defer, begin_marker, args, keywords, end_marker)
            }
        }
        match self {
            // This uses double-quotation for String literals
            ast::Expr::Constant(ast::ExprConstant { value, .. }) => match value {
                ast::Constant::Str(str) => push(str_buffer, defer, format!("\"{}\"", str)),
                _ => push(str_buffer, defer, format!("{}", self)),
            },
            ast::Expr::Call(ast::ExprCall {
                func,
                args,
                keywords,
                ..
            }) => {
                let func_expr: &ast::Expr = &*func;
                let name = func_expr.custom_fmt(str_buffer, true);
                if name == "load" {
                    // loads don't have spaces between in buildifier and are never nested
                    str_buffer.push_cow(Cow::Owned(name));
                    push_inline_list(str_buffer, defer, "(", args, keywords, ")");
                } else {
                    if !(str_buffer.in_keyword || str_buffer.at_start()) {
                        str_buffer.push("").finish_line();
                    }
                    str_buffer.push_cow(Cow::Owned(name));
                    push_list(str_buffer, defer, "(", args, keywords, ")")
                }
                "".to_string()
            }
            ast::Expr::List(ast::ExprList { elts, .. }) => {
                push_list(str_buffer, defer, "[", elts, &vec![], "]");
                "".to_string()
            }
            _ => push(str_buffer, defer, format!("{}", self)),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip_build_file() {
        assert_round_trip(
            r#"load("@rules_proto//proto:defs.bzl", "proto_library")

proto_library(
    name = "aa_proto",
    srcs = [
        "aa.proto",
        "bb.proto",
    ],
    deps = [
        "//x",
        "//y",
    ],
    nested = [
        ["foo"],
        ["bar"],
    ],
    visibility = ["//visibility:public"],
)

java_proto_library(
    name = "a_proto_java",
    visibility = ["//visibility:public"],
    deps = [":a_proto"],
)

filegroup(
    name = "example_files",
    srcs = glob(include = ["**/*.java"]),
    visibility = ["//visibility:public"],
)"#,
        )
    }

    #[test]
    fn round_trip_build_file_fg() {
        // we don't indent before filegroup
        assert_round_trip(
            r#"filegroup(
    name = "example_files",
    srcs = glob(include = ["**/*.java"]),
    visibility = ["//visibility:public"],
)"#,
        )
    }

    #[test]
    fn round_trip_python_source() {
        assert_round_trip(
            r#"from tensorflow import foo
def aa():
    pass
def cust_fn():
    pass"#,
        )
    }

    fn assert_round_trip(code: &str) {
        use crate::PythonProgram;

        let parsed = PythonProgram::parse(code, "tmp.py").unwrap();
        let printed_parsed = format!("{}", parsed);

        assert_eq!(
            code, printed_parsed,
            "\n\nPrinted parsed was: {}\n\n",
            printed_parsed
        );
    }
}
