use anyhow::{bail, Result};
use tree_sitter::Parser;

#[derive(Debug, PartialEq)]
pub struct ProtobufSource {
    pub imports: Vec<String>
}

impl ProtobufSource {
    pub fn parse(source: &str, _source_path: &str) -> Result<ProtobufSource> {
        let mut buf = Vec::default();
        let mut parser = Parser::new();
        parser.set_language(tree_sitter_proto::language())?;
        let tree = match parser.parse(source, None) {
            Some(tree) => tree,
            None       => bail!("parse failed"),
        };
        let root_node = tree.root_node();
        let mut cursor = root_node.walk();
        let bytes = source.as_bytes();
        for child_node in root_node.children(&mut cursor) {
            match child_node.kind() {
                "import" =>
                    {
                        for path in child_node.children_by_field_name("path", &mut child_node.walk()) {
                            let mut quoted = path.utf8_text(bytes)?.chars();
                            // Remove the quotation marks
                            quoted.next();
                            quoted.next_back();
                            buf.push(quoted.as_str().to_string());
                        }
                    }
                _        => ()
            };
        }
        Ok(ProtobufSource { imports: buf })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_target_entry() -> Result<()> {
        let protobuf_source = r#"syntax = "proto3";

package page.common; // Requried to generate valid code.

// Always import protos with a full path relative to the WORKSPACE file.
import "page/common/src/proto/zip_code.proto";

message Address {
  // string city = 1;
  ZipCode zip_code = 2;
}"#;
        let parsed = ProtobufSource::parse(protobuf_source, "tmp.proto")?;
        let expected = vec!["page/common/src/proto/zip_code.proto".to_string()];
        assert_eq!(parsed.imports, expected);
        Ok(())
    }
}
