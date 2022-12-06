use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use crate::{
    async_read_json_file,
    build_graph::{GraphMapping, GraphNode},
    Opt, PrintBuildArgs,
};
use anyhow::{anyhow, Context, Result};
use ast::{Located, StmtKind};
use bzl_gen_build_python_utilities::{ast_builder, PythonProgram};
use bzl_gen_build_shared_types::{module_config::ModuleConfig, *};
use futures::{stream, StreamExt};
use ignore::WalkBuilder;
use rustpython_parser::ast;

use tokio::sync::Semaphore;

lazy_static::lazy_static! {
    static ref BUILD_BAZEL: std::ffi::OsString = std::ffi::OsString::from("BUILD.bazel");
    static ref BUILD_NO_EXT: std::ffi::OsString = std::ffi::OsString::from("BUILD");
}

#[derive(Debug)]
struct TargetEntry {
    pub name: String,
    pub required_load: HashMap<Arc<String>, Vec<Arc<String>>>,
    pub visibility: Option<Arc<String>>,
    pub srcs: SrcType,
    pub target_type: Arc<String>,
    pub extra_kv_pairs: Vec<(String, Vec<String>)>,
}

impl TargetEntry {
    pub fn emit_build_function_call(&self) -> Result<Located<StmtKind>> {
        let mut kw_args: Vec<(Arc<String>, Located<ast::ExprKind>)> = Default::default();

        kw_args.push((
            Arc::new("name".to_string()),
            ast_builder::with_constant_str(self.name.clone()),
        ));

        kw_args.push((Arc::new("srcs".to_string()), self.srcs.to_statement()));

        for (k, v) in self.extra_kv_pairs.iter() {
            kw_args.push((
                Arc::new(k.clone()),
                ast_builder::as_py_list(
                    v.iter()
                        .map(|d| ast_builder::with_constant_str(d.to_string()))
                        .collect(),
                ),
            ));
        }

        let visibility = self
            .visibility
            .as_ref()
            .map(|e| e.as_ref().as_str())
            .unwrap_or("//visibility:public");

        kw_args.push((
            Arc::new("visibility".to_string()),
            ast_builder::as_py_list(vec![ast_builder::with_constant_str(visibility.to_string())]),
        ));

        Ok(ast_builder::as_stmt_expr(
            ast_builder::gen_py_function_call(self.target_type.clone(), Vec::default(), kw_args),
        ))
    }
}

#[derive(Debug)]
enum SrcType {
    Glob {
        include: Vec<String>,
        exclude: Vec<String>,
    },
    #[allow(dead_code)]
    Files(Vec<String>),
}
impl SrcType {
    pub fn to_statement(&self) -> Located<ast::ExprKind> {
        match self {
            SrcType::Glob { include, exclude } => {
                let mut kw_args: Vec<(Arc<String>, Located<ast::ExprKind>)> = Default::default();

                kw_args.push((
                    Arc::new("include".to_string()),
                    ast_builder::as_py_list(
                        include
                            .iter()
                            .map(|e| ast_builder::with_constant_str(e.clone()))
                            .collect(),
                    ),
                ));

                if !exclude.is_empty() {
                    kw_args.push((
                        Arc::new("exclude".to_string()),
                        ast_builder::as_py_list(
                            exclude
                                .iter()
                                .map(|e| ast_builder::with_constant_str(e.clone()))
                                .collect(),
                        ),
                    ));
                }

                ast_builder::gen_py_function_call(
                    Arc::new("glob".to_string()),
                    Vec::default(),
                    kw_args,
                )
            }

            SrcType::Files(files) => ast_builder::as_py_list(
                files
                    .iter()
                    .map(|e| ast_builder::with_constant_str(e.clone()))
                    .collect(),
            ),
        }
    }
}

struct TargetEntries {
    pub entries: Vec<TargetEntry>,
}

impl TargetEntries {
    // Helper
    fn load_statement(from: Arc<String>, methods: Vec<Arc<String>>) -> Located<StmtKind> {
        let mut fn_args = Vec::default();
        fn_args.push(ast_builder::with_constant_str(from.as_ref().to_owned()));
        fn_args.extend(
            methods
                .into_iter()
                .map(|e| e.as_ref().to_owned())
                .map(ast_builder::with_constant_str),
        );

        ast_builder::as_stmt_expr(ast_builder::gen_py_function_call(
            Arc::new("load".to_string()),
            fn_args,
            Default::default(),
        ))
    }

    pub fn emit_build_file(&self) -> Result<String> {
        let program = self.to_ast()?;
        Ok(format!("{}", &program))
    }

    pub fn to_ast(&self) -> Result<PythonProgram> {
        let mut program: Vec<Located<StmtKind>> = Vec::default();
        let mut all_load_statements: HashMap<Arc<String>, Vec<Arc<String>>> = HashMap::default();

        for entry in self.entries.iter() {
            for (k, v) in entry.required_load.iter() {
                let e = all_load_statements.entry(k.clone()).or_default();
                e.extend(v.iter().cloned());
                e.sort();
                e.dedup();
            }
        }

        for (load_from, load_v) in all_load_statements {
            program.push(TargetEntries::load_statement(load_from, load_v));
        }

        for e in self.entries.iter() {
            program.push(e.emit_build_function_call()?);
        }

        Ok(PythonProgram { body: program })
    }
}

async fn print_file(
    opt: &'static Opt,
    project_conf: &'static ProjectConf,
    graph_node: GraphNode,
    concurrent_io_operations: &'static Semaphore,
    element: String,
) -> Result<()> {
    let mut module_config: Option<(&ModuleConfig, &'static str, &'static str)> = None;
    for (_k, v) in project_conf.configurations.iter() {
        let paths = v
            .main_roots
            .iter()
            .map(|r| ("main", r))
            .chain(v.test_roots.iter().map(|r| ("test", r)));
        let matched_paths: Vec<(&str, &String)> = paths
            .filter(|(_, p)| element.starts_with(p.as_str()))
            .take(2)
            .collect();

        // This configuration doesn't match, but others might.
        if matched_paths.is_empty() {
            continue;
        }
        if matched_paths.len() > 1 {
            return Err(anyhow::anyhow!(
                "Found two many paths for {}, at least: {:?}",
                element,
                matched_paths
            ));
        }
        if module_config.is_none() {
            let (path_type, path_prefix) = matched_paths.get(0).unwrap();
            module_config = Some((v, path_type, path_prefix));
        } else {
            return Err(anyhow::anyhow!("Multiple configurations matched for {}, at least: {:?}; module config was before: {:?}", element, matched_paths, module_config));
        }
    }
    let (module_config, path_type, _matched_prefix) = if let Some(a) = module_config {
        a
    } else {
        return Err(anyhow!(
            "Unable to find any matching configuration for {}",
            element
        ));
    };

    let target_folder = opt.working_directory.join(&element);
    let target_file = target_folder.join("BUILD.bazel");
    let target_name = target_folder
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let mut extra_kv_pairs: HashMap<String, Vec<String>> = HashMap::default();

    if !graph_node.dependencies.is_empty() {
        let deps: Vec<String> = graph_node
            .dependencies
            .iter()
            .map(|e| {
                if e.starts_with('@') {
                    e.clone()
                } else {
                    format!("//{}", e)
                }
            })
            .collect();
        extra_kv_pairs.insert("deps".to_string(), deps);
    }

    if !graph_node.runtime_dependencies.is_empty() {
        let deps: Vec<String> = graph_node
            .runtime_dependencies
            .iter()
            .map(|e| {
                if e.starts_with('@') {
                    e.clone()
                } else {
                    format!("//{}", e)
                }
            })
            .collect();
        extra_kv_pairs.insert("runtime_deps".to_string(), deps);
    }

    let build_config = if path_type == "test" {
        &module_config.build_config.test
    } else {
        &module_config.build_config.main
    };

    let build_config = if let Some(bc) = build_config {
        bc
    } else {
        return Err(anyhow!(
            "unable to find build configuration for {:?}",
            graph_node
        ));
    };

    for (k, lst) in build_config.extra_key_to_list.iter() {
        extra_kv_pairs
            .entry(k.clone())
            .or_default()
            .extend(lst.iter().cloned())
    }

    for directive in module_config
        .path_directives
        .iter()
        .filter(|e| element.starts_with(&e.prefix))
    {
        match directive.directives().as_ref() {
            Ok(loaded) => {
                for d in loaded {
                    match d {
                        // Other directive types are actioned much earlier in the pipeline.
                        Directive::SrcDirective(_) => (), // no op.
                        Directive::EntityDirective(_) => (), // no op
                        Directive::ManualRef(manual_ref) => match manual_ref.command {
                            directive::ManualRefDirective::RuntimeRef => {
                                let t = extra_kv_pairs
                                    .entry("runtime_deps".to_string())
                                    .or_default();
                                t.push(manual_ref.target_value.clone())
                            }
                            directive::ManualRefDirective::Ref => {
                                let t = extra_kv_pairs.entry("deps".to_string()).or_default();
                                t.push(manual_ref.target_value.clone())
                            }
                        },
                    }
                }
            }
            Err(err) => return Err(anyhow!("{:#?}", err)),
        }
    }

    let mut required_load = HashMap::default();

    for h in build_config.headers.iter() {
        required_load.insert(
            Arc::new(h.load_from.clone()),
            vec![Arc::new(h.load_value.clone())],
        );
    }

    let primary_extension = if let Some(e) = module_config.file_extensions.first() {
        e
    } else {
        return Err(anyhow!(
            "No configured primary extension in {:?}",
            module_config
        ));
    };

    let mut include: Vec<String> = vec![format!("*.{}", primary_extension)];
    for node in graph_node.child_nodes.iter() {
        if let Some(p) = node.strip_prefix(&element) {
            include.push(format!("{}/*.{}", &p[1..], primary_extension));
        } else {
            return Err(anyhow!("Child node {} doesn't seem to be a child of the parent {}", node, element));
        }
    }

    let target = TargetEntry {
        name: target_name.clone(),
        extra_kv_pairs: extra_kv_pairs
            .into_iter()
            .map(|(k, mut v)| {
                v.sort();
                v.dedup();
                (k, v)
            })
            .collect(),
        required_load,
        visibility: None,
        srcs: SrcType::Glob {
            include,
            exclude: Vec::default(),
        },
        target_type: Arc::new(build_config.function_name.clone()),
    };

    let t = TargetEntries {
        entries: vec![target],
    };

    let handle = concurrent_io_operations.acquire().await?;

    tokio::fs::write(&target_file, t.emit_build_file()?)
        .await
        .with_context(|| format!("Attempting to write file data to {:?}", target_file))?;
    drop(handle);

    Ok(())
}

async fn async_find_all_build_files(
    opt: &'static Opt,
    project_conf: &'static ProjectConf,
) -> Result<HashSet<PathBuf>> {
    let mut results: HashSet<PathBuf> = Default::default();

    let i = project_conf
        .configurations
        .iter()
        .flat_map(|(_k, v)| v.main_roots.iter().chain(v.test_roots.iter()))
        .map(|e| opt.working_directory.join(&e))
        .map(|path| {
            tokio::spawn(async move {
                let mut local_results: HashSet<PathBuf> = Default::default();

                for entry in WalkBuilder::new(path)
                    .build()
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    if entry.file_type().map(|e| e.is_file()).unwrap_or(false)
                        && (entry.file_name() == BUILD_NO_EXT.as_os_str()
                            || entry.file_name() == BUILD_BAZEL.as_os_str())
                    {
                        local_results.insert(entry.into_path());
                    }
                }
                local_results
            })
        });

    let mut async_iter = stream::iter(i).buffer_unordered(6);

    while let Some(r) = async_iter.next().await {
        results.extend(r?)
    }

    Ok(results)
}

pub async fn print_build(
    opt: &'static Opt,
    print_build_args: &'static PrintBuildArgs,
    project_conf: &'static ProjectConf,
    concurrent_io_operations: &'static Semaphore,
) -> Result<()> {
    let graph_data: GraphMapping = async_read_json_file(&print_build_args.graph_data)
        .await
        .with_context(|| "Attempting to load graph data")?;

    let mut current_files = async_find_all_build_files(opt, project_conf)
        .await
        .with_context(|| "Finding all build files")?;

    let mut res = Vec::default();
    for (element, graph_node) in graph_data
        .build_mapping
        .into_iter()
        .filter(|(k, _v)| !k.starts_with('@'))
    {
        let target_folder = opt.working_directory.join(&element).join("BUILD.bazel");
        current_files.remove(&target_folder);
        let graph_node = graph_node.clone();
        res.push(tokio::spawn(async move {
            print_file(
                opt,
                project_conf,
                graph_node,
                concurrent_io_operations,
                element,
            )
            .await
        }));
    }

    while let Some(nxt) = res.pop() {
        nxt.await??
    }

    // These files are old and not updated..
    for f in current_files {
        println!("Deleting no longer used build file of: {:?}", f);
        std::fs::remove_file(&f)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_target_entry() {
        let python_source = r#"load("//build_tools/lang_support/scala/test:scalatest.bzl", "scala_tests")
scala_tests(
    name = "scala_extractor",
    srcs = glob(include =  ["*.scala"]),
    deps = [
        "//src/main/scala/com/example/scala_extractor",
        "@jvm__io_circe__circe_core//:jar",
        "@jvm__org_scalacheck__scalacheck//:jar",
    ],
    visibility = ["//visibility:public"],
)
        "#;

        let parsed_from_embed_string = {
            let parsed = PythonProgram::parse(python_source, "tmp.py").unwrap();
            PythonProgram::parse(format!("{}", parsed).as_str(), "tmp.py").unwrap()
        };

        let mut entries = Vec::default();

        let mut required_load = HashMap::new();
        required_load.insert(
            Arc::new("//build_tools/lang_support/scala/test:scalatest.bzl".to_string()),
            vec![Arc::new("scala_tests".to_string())],
        );

        entries.push(TargetEntry {
            name: "scala_extractor".to_string(),
            extra_kv_pairs: vec![(
                "deps".to_string(),
                vec![
                    "//src/main/scala/com/example/scala_extractor".to_string(),
                    "@jvm__io_circe__circe_core//:jar".to_string(),
                    "@jvm__org_scalacheck__scalacheck//:jar".to_string(),
                ],
            )],
            required_load,
            visibility: None,
            srcs: SrcType::Glob {
                include: vec!["*.scala".to_string()],
                exclude: Vec::default(),
            },
            target_type: Arc::new("scala_tests".to_string()),
        });

        let target_entries = TargetEntries { entries };

        let generated_s = target_entries.emit_build_file().unwrap();

        let parsed_from_generated_string =
            PythonProgram::parse(generated_s.as_str(), "tmp.py").unwrap();

        assert_eq!(
            parsed_from_embed_string, parsed_from_generated_string,
            "\n\nExpected:\n{}\n\nGenerated:\n{}\n",
            parsed_from_embed_string, parsed_from_generated_string
        );
    }
}
