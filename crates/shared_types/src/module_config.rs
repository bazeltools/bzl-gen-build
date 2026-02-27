use serde::{Deserialize, Serialize};

use crate::build_config::BuildConfig;

#[derive(Debug, Serialize, Default, Deserialize, PartialEq, Eq)]
pub struct ModuleConfig {
    pub file_extensions: Vec<String>,

    #[serde(default)]
    pub build_config: BuildConfig,

    #[serde(default)]
    pub main_roots: Vec<String>,

    #[serde(default)]
    pub test_roots: Vec<String>,

    #[serde(default)]
    pub test_globs: Vec<String>,

    #[serde(default)]
    pub circular_dependency_allow_list: Vec<String>,

    /// When true, prepend `# buildifier: disable=format` on the first line of generated BUILD files.
    #[serde(default)]
    pub disable_format: bool,
}

/// Prepends `# buildifier: disable=format` on the first line when disable_format is true.
pub fn maybe_add_buildifier_disable(content: impl AsRef<str>, disable_format: bool) -> String {
    if !disable_format {
        return content.as_ref().to_string();
    }
    let c = content.as_ref();
    if c.starts_with("# buildifier: disable=format") {
        return c.to_string();
    }
    format!("# buildifier: disable=format\n{}", c)
}

impl ModuleConfig {
    pub fn merge(&mut self, other: ModuleConfig) {
        self.build_config.merge(other.build_config);

        self.main_roots.extend(other.main_roots.into_iter());
        self.main_roots.sort();
        self.main_roots.dedup();

        self.test_roots.extend(other.test_roots.into_iter());
        self.test_roots.sort();
        self.test_roots.dedup();
    }
}

#[cfg(test)]
mod tests {
    use super::maybe_add_buildifier_disable;

    #[test]
    fn test_maybe_add_buildifier_disable() {
        assert_eq!(maybe_add_buildifier_disable("load(...)\n", false), "load(...)\n");
        assert_eq!(
            maybe_add_buildifier_disable("load(...)\n", true),
            "# buildifier: disable=format\nload(...)\n"
        );
        assert_eq!(
            maybe_add_buildifier_disable("# buildifier: disable=format\nload(...)\n", true),
            "# buildifier: disable=format\nload(...)\n"
        );
    }
}
