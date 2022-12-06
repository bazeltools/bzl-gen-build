use ast::{Expr, Located, Location, StmtKind};
use rustpython_parser::ast;
use std::sync::Arc;

pub fn with_location<T>(t: T) -> Located<T> {
    let location = Location::new(1, 0);
    Located::new(location, location, t)
}
pub fn with_constant_str(s: String) -> Located<ast::ExprKind> {
    with_location({
        ast::ExprKind::Constant {
            value: ast::Constant::Str(s),
            kind: None,
        }
    })
}

pub fn as_py_list<U>(elements: Vec<Expr<U>>) -> Located<ast::ExprKind<U>> {
    with_location({
        ast::ExprKind::List {
            elts: elements,
            ctx: ast::ExprContext::Load,
        }
    })
}

pub fn as_stmt_expr(u: Located<ast::ExprKind>) -> Located<StmtKind> {
    with_location(StmtKind::Expr { value: Box::new(u) })
}

pub fn gen_py_function_call(
    name: Arc<String>,
    args: Vec<Located<ast::ExprKind>>,
    kw_args: Vec<(Arc<String>, Located<ast::ExprKind>)>,
) -> Located<ast::ExprKind> {
    let location = Location::new(1, 0);
    let mut kws = Vec::default();
    for (k, v) in kw_args {
        kws.push(with_location(ast::KeywordData {
            arg: Some(k.as_ref().to_owned()),
            value: Box::new(v),
        }))
    }

    with_location({
        ast::ExprKind::Call {
            func: Box::new(Expr::new(
                location,
                location,
                ast::ExprKind::Name {
                    id: name.as_ref().to_owned(),
                    ctx: ast::ExprContext::Load,
                },
            )),
            args,
            keywords: kws,
        }
    })
}
