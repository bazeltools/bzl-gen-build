pub fn extract(python_src: &str) -> Vec<String> {
    let mut buf = Vec::default();
    for ln in python_src.lines() {
        if let Some(comment_line) = ln.trim_start().strip_prefix('#') {
            if let Some(matching) = comment_line.trim_start().strip_prefix("bzl_gen_build") {
                if let Some(bzl_command) = matching.trim_start().strip_prefix(':') {
                    buf.push(bzl_command.trim().to_string());
                }
            }
        }
    }
    buf.sort();
    buf.dedup();

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_directives() {
        let python_source = r#"import tensorflow as tf
from tensorflow.keras.layers import MultiHeadAttention, Concatenate, Add, Dense

from src.main.python.foo.bar.baz import NumericalEmbedding

#bzl_gen_build: runtime_ref: tensorflow.keras
# bzl_gen_build: runtime_ref: src.main.python.foo.bar.baz.NumericalEmbedding
#bzl_gen_build : manual_runtime_ref: //:build_properties.jar

def my_fn():
  from x.y.z import a as p
  #bzl_gen_build : manual_runtime_ref: //:build_properties2.jar
        "#;

        let mut expected = vec![
            "runtime_ref: tensorflow.keras".to_string(),
            "runtime_ref: src.main.python.foo.bar.baz.NumericalEmbedding".to_string(),
            "manual_runtime_ref: //:build_properties.jar".to_string(),
            "manual_runtime_ref: //:build_properties2.jar".to_string(),
        ];
        expected.sort();
        expected.dedup();
        assert_eq!(extract(python_source), expected)
    }
}
