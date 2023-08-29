use anyhow::{bail, Result};
use bzl_gen_build_shared_types::Directive;
use tree_sitter::Parser;

#[derive(Debug, PartialEq)]
pub struct ProtobufSource {
    pub imports: Vec<String>,
    pub well_known_refs: Vec<String>,
    pub bzl_gen_build_commands: Vec<String>,
}

impl ProtobufSource {
    pub fn parse(source: &str, _source_path: &str) -> Result<ProtobufSource> {
        let mut buf = Vec::default();
        let mut parser = Parser::new();
        let mut well_known_refs = Vec::default();
        let mut bzl_gen_build_commands = Vec::default();
        parser.set_language(tree_sitter_proto::language())?;
        let tree = match parser.parse(source, None) {
            Some(tree) => tree,
            None => bail!("parse failed"),
        };
        let root_node = tree.root_node();
        let mut cursor = root_node.walk();
        let bytes = source.as_bytes();
        for child_node in root_node.children(&mut cursor) {
            match child_node.kind() {
                "import" => {
                    for path in child_node.children_by_field_name("path", &mut child_node.walk()) {
                        let mut quoted = path.utf8_text(bytes)?.chars();
                        // Remove the quotation marks
                        quoted.next();
                        quoted.next_back();
                        match Self::well_known_target(&quoted.as_str()) {
                            Some(well_known) => well_known_refs.push(well_known.to_string()),
                            None => buf.push(quoted.as_str().to_string()),
                        }
                    }
                }
                "comment" => {
                    let raw_comment = child_node.utf8_text(bytes)?;
                    if let Some(bzl_command) = Directive::extract_directive(&raw_comment, "//") {
                        bzl_gen_build_commands.push(bzl_command.trim().to_string());
                    }
                }
                _ => (),
            };
        }
        Ok(ProtobufSource {
            imports: buf,
            well_known_refs: well_known_refs,
            bzl_gen_build_commands: bzl_gen_build_commands,
        })
    }

    pub fn well_known_target(path: &str) -> Option<String> {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() != 3 {
            None
        } else if parts[0] != "google" || parts[1] != "protobuf" {
            None
        } else {
            Some(format!(
                "@com_github_protocolbuffers_protobuf//:{}",
                parts[2].replace(".", "_")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_directives() -> Result<()> {
        let protobuf_source = r#"syntax = "proto3";

package page.common; // Requried to generate valid code.

// bzl_gen_build:manual_ref:aaa
import "some/aaa.proto";

message Address {
  string city = 1;
}"#;
        let parsed = ProtobufSource::parse(protobuf_source, "tmp.proto")?;
        let expected = vec!["manual_ref:aaa"];
        assert_eq!(parsed.bzl_gen_build_commands, expected);
        Ok(())
    }

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

    #[test]
    fn test_well_known_target() -> Result<()> {
        let wk = ProtobufSource::well_known_target("google/protobuf/any.proto");
        let expected = Some("@com_github_protocolbuffers_protobuf//:any_proto".to_string());
        assert_eq!(wk, expected);
        Ok(())
    }

    #[test]
    fn test_parse_well_known() -> Result<()> {
        let protobuf_source = r#"syntax = "proto3";

package page.common; // Requried to generate valid code.

import "google/protobuf/timestamp.proto";

message Address {
  string city = 1;
}"#;
        let parsed = ProtobufSource::parse(protobuf_source, "tmp.proto")?;
        assert_eq!(parsed.imports, [] as [&str; 0]);

        let expected = vec!["@com_github_protocolbuffers_protobuf//:timestamp_proto".to_string()];
        assert_eq!(parsed.well_known_refs, expected);
        Ok(())
    }
}
