use bzl_gen_build_python_utilities::PythonProgram;
use rustpython_ast::{Located, StmtKind};

pub fn extract(program: &PythonProgram) -> Vec<String> {
    let mut buf = Vec::default();
    extract_from_body(&program.body, &mut buf);

    let mut buf: Vec<String> = buf
        .into_iter()
        .flat_map(|b| {
            let elements = b.split('.');
            let mut cur_b = String::default();
            let mut res = Vec::default();
            for e in elements {
                let is_e = cur_b.is_empty();
                if is_e {
                    cur_b = e.to_string()
                } else {
                    cur_b = format!("{}.{}", cur_b, e);
                }
                res.push(cur_b.clone());
            }
            res.into_iter()
        })
        .collect();
    buf.sort();
    buf.dedup();

    buf
}

fn extract_from_body(body: &Vec<Located<StmtKind>>, buf: &mut Vec<String>) {
    for element in body.iter() {
        let element = &element.node;
        match element {
            StmtKind::FunctionDef { body, .. } => extract_from_body(&body, buf),
            StmtKind::AsyncFunctionDef { body, .. } => extract_from_body(&body, buf),
            StmtKind::ClassDef {
                name: _,
                bases: _,
                keywords: _,
                body,
                decorator_list: _,
            } => extract_from_body(&body, buf),

            StmtKind::For { body, orelse, .. } => {
                extract_from_body(&body, buf);
                extract_from_body(&orelse, buf);
            }
            StmtKind::AsyncFor { body, orelse, .. } => {
                extract_from_body(&body, buf);
                extract_from_body(&orelse, buf);
            }
            StmtKind::While { body, orelse, .. } => {
                extract_from_body(&body, buf);
                extract_from_body(&orelse, buf);
            }
            StmtKind::If { body, orelse, .. } => {
                extract_from_body(&body, buf);
                extract_from_body(&orelse, buf);
            }
            StmtKind::With { body, .. } => extract_from_body(&body, buf),
            StmtKind::AsyncWith { body, .. } => extract_from_body(&body, buf),
            StmtKind::Match { cases, .. } => {
                for case in cases.iter() {
                    extract_from_body(&case.body, buf);
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                for handler in handlers.iter() {
                    match &handler.node {
                        rustpython_ast::ExcepthandlerKind::ExceptHandler { body, .. } => {
                            extract_from_body(&body, buf);
                        }
                    }
                }
                extract_from_body(&body, buf);
                extract_from_body(&orelse, buf);
                extract_from_body(&finalbody, buf);
            }
            StmtKind::Import { names } => {
                for nme in names.iter() {
                    buf.push(nme.node.name.clone());
                }
            }
            StmtKind::ImportFrom {
                module,
                names,
                level: _,
            } => {
                for nme in names.iter() {
                    if let Some(module) = module.as_ref() {
                        buf.push(format!("{}.{}", module, nme.node.name));
                    } else {
                        buf.push(nme.node.name.clone());
                    }
                }
            }
            _ => (),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_target_entry() {
        let python_source = r#"import tensorflow as tf
from tensorflow.keras.layers import MultiHeadAttention, Concatenate, Add, Dense

from src.main.python.foo.bar.baz import NumericalEmbedding
# relative imports should be resolved relative to the file path
from .field import field
def my_fn():
  from x.y.z import a as p
        "#;

        let parsed = PythonProgram::parse(python_source, "foo/tmp.py").unwrap();
        let mut expected = vec![
            "tensorflow".to_string(),
            "tensorflow.keras".to_string(),
            "tensorflow.keras.layers".to_string(),
            "tensorflow.keras.layers.MultiHeadAttention".to_string(),
            "tensorflow.keras.layers.Concatenate".to_string(),
            "tensorflow.keras.layers.Add".to_string(),
            "tensorflow.keras.layers.Dense".to_string(),
            "src".to_string(),
            "src.main".to_string(),
            "src.main.python".to_string(),
            "src.main.python.foo".to_string(),
            "src.main.python.foo.bar".to_string(),
            "src.main.python.foo.bar.baz".to_string(),
            "src.main.python.foo.bar.baz.NumericalEmbedding".to_string(),
            "foo.field".to_string(),
            "foo.field.field".to_string(),
            "x".to_string(),
            "x.y".to_string(),
            "x.y.z".to_string(),
            "x.y.z.a".to_string(),
        ];
        expected.sort();
        expected.dedup();
        assert_eq!(extract(&parsed), expected)
    }
}
