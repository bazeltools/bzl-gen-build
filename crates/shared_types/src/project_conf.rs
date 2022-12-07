use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

use crate::module_config::ModuleConfig;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectConf {
    #[serde(default, serialize_with = "crate::serde_helpers::ordered_map")]
    pub configurations: HashMap<String, ModuleConfig>,

    #[serde(default)]
    pub includes: Vec<String>,

    #[serde(default)]
    pub path_directives: Vec<DirectiveConf>,
}
impl ProjectConf {
    pub fn merge(&mut self, other: ProjectConf) {
        self.includes.extend(other.includes.into_iter());
        self.includes.sort();
        self.includes.dedup();

        self.path_directives
            .extend(other.path_directives.into_iter());
        self.path_directives.sort();
        self.path_directives.dedup();

        for (k, v) in other.configurations {
            let e = self.configurations.entry(k);
            match e {
                std::collections::hash_map::Entry::Occupied(mut o) => o.get_mut().merge(v),
                std::collections::hash_map::Entry::Vacant(vacant) => {
                    vacant.insert(v);
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DirectiveConf {
    pub prefix: String,
    #[serde(rename = "directives")]
    pub directive_strings: Vec<String>,

    #[serde(skip)]
    directive_cache: Mutex<Option<Arc<anyhow::Result<Vec<crate::Directive>>>>>,
}

impl Ord for DirectiveConf {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        PartialOrd::partial_cmp(self, other).unwrap()
    }
}
impl Eq for DirectiveConf {
    fn assert_receiver_is_total_eq(&self) {}
}
impl PartialEq for DirectiveConf {
    fn eq(&self, other: &Self) -> bool {
        self.prefix == other.prefix && self.directive_strings == other.directive_strings
    }
}
impl PartialOrd for DirectiveConf {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.prefix.partial_cmp(&other.prefix) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.directive_strings.partial_cmp(&other.directive_strings) {
            Some(core::cmp::Ordering::Equal) => Some(std::cmp::Ordering::Equal),
            ord => ord,
        }
    }
}

impl DirectiveConf {
    pub fn new(prefix: String, directive_strings: Vec<String>) -> Self {
        Self {
            prefix,
            directive_strings,
            directive_cache: Default::default(),
        }
    }

    pub fn directives(&self) -> Arc<anyhow::Result<Vec<crate::Directive>>> {
        let mut mutex = self.directive_cache.lock().unwrap();
        if let Some(r) = mutex.as_ref() {
            return r.clone();
        }
        let v = Arc::new(crate::Directive::from_strings(&self.directive_strings));
        *mutex = Some(v.clone());
        v
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathMatcher {}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        build_config::{BuildConfig, GrpBuildConfig},
        module_config::ModuleConfig,
        DirectiveConf, ProjectConf,
    };

    const SAMPLE_V: &str = r#"
    {
        "configurations": {
          "java": {
            "file_extensions": [
              "java"
            ],
            "build_config": {
              "main": {
                "headers": [],
                "function_name": "java_library"
              },
              "extra_key_to_list": {
                "plugins": [
                  "//foo/bar/bazplugin"
                ]
              }
            },
            "main_roots": [
              "src/main/python"
            ],
            "test_roots": [
              "src/test/python"
            ]
          }
        },
        "path_directives": [
          {
            "prefix": "module-a/src/test/scala/com/foo",
            "directives": [
              "runtime_ref:com.example.Bar"
            ]
          }
        ]
      }
"#;

    #[test]
    fn parsing() {
        let v: ProjectConf = serde_json::from_str(SAMPLE_V).expect("Parse");

        assert_eq!(
            v,
            ProjectConf {
                configurations: HashMap::from([(
                    "java".to_string(),
                    ModuleConfig {
                        file_extensions: vec!["java".to_string()],
                        build_config: BuildConfig {
                            main: Some(GrpBuildConfig {
                                headers: vec![],
                                function_name: "java_library".to_string(),
                                extra_key_to_list: HashMap::default(),
                                extra_key_to_value: HashMap::default()
                            }),
                            test: None,
                            binary_application: None
                        },
                        main_roots: vec!["src/main/python".to_string()],
                        test_roots: vec!["src/test/python".to_string()]
                    }
                )]),
                includes: vec![],
                path_directives: vec![DirectiveConf::new(
                    "module-a/src/test/scala/com/foo".to_string(),
                    vec!["runtime_ref:com.example.Bar".to_string()]
                )]
            }
        );
    }
}
