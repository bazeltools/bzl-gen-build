use rustpython_parser::ast;
use ast::{text_size::TextRange, Expr, Stmt, TextSize};
use std::sync::Arc;

fn empty_range() -> TextRange {
    TextRange::empty(TextSize::new(0))
}

pub fn with_constant_str(s: String) -> Expr {
    Expr::Constant(ast::ExprConstant {
        range: empty_range(),
        value: ast::Constant::Str(s),
        kind: None,
    })
}

pub fn as_py_list(elements: Vec<Expr>) -> Expr {
    Expr::List(ast::ExprList {
        range: empty_range(),
        elts: elements,
        ctx: ast::ExprContext::Load,
    })
}

pub fn as_stmt_expr(u: Expr) -> Stmt {
    Stmt::Expr(ast::StmtExpr {
        range: empty_range(),
        value: Box::new(u),
    })
}

pub fn gen_py_function_call(
    name: Arc<String>,
    args: Vec<Expr>,
    kw_args: Vec<(Arc<String>, Expr)>,
) -> Expr {
    let location = empty_range();
    let mut kws = Vec::default();
    for (k, v) in kw_args {
        kws.push(ast::Keyword {
            range: location,
            arg: Some(ast::Identifier::new(k.to_string())),
            value: v,
        })
    }

    Expr::Call(ast::ExprCall {
        range: location,
        func: Box::new(Expr::Name(
            ast::ExprName {
                range: location,
                id: ast::Identifier::new(name.to_string()),
                ctx: ast::ExprContext::Load,
            },
        )),
        args,
        keywords: kws,
    })
}
