use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use crate::{
    async_read_json_file,
    build_graph::{GraphMapping, GraphNode, GraphNodeMetadata},
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
    pub srcs: Option<SrcType>,
    pub target_type: Arc<String>,
    pub extra_kv_pairs: Vec<(String, Vec<String>)>,
    pub extra_k_strs: Vec<(String, String)>,
}

impl TargetEntry {
    pub fn emit_build_function_call(&self) -> Result<Located<StmtKind>> {
        let mut kw_args: Vec<(Arc<String>, Located<ast::ExprKind>)> = Default::default();

        kw_args.push((
            Arc::new("name".to_string()),
            ast_builder::with_constant_str(self.name.clone()),
        ));

        if let Some(srcs) = &self.srcs {
            kw_args.push((Arc::new("srcs".to_string()), srcs.to_statement()));
        }

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

        for (k, v) in self.extra_k_strs.iter() {
            kw_args.push((
                Arc::new(k.clone()),
                ast_builder::with_constant_str(v.to_string()),
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

    List(Vec<String>),
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

            SrcType::List(files) => ast_builder::as_py_list(
                files
                    .iter()
                    .map(|e| ast_builder::with_constant_str(e.clone()))
                    .collect(),
            ),
        }
    }
}

#[derive(Debug)]
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

        let mut all_load_statements: Vec<(Arc<String>, Vec<Arc<String>>)> =
            all_load_statements.into_iter().collect();
        all_load_statements.sort();

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
) -> Result<Vec<PathBuf>> {
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
    let mut emitted_files = Vec::default();

    let target_folder = opt.working_directory.join(&element);
    let target_file = target_folder.join("BUILD.bazel");
    emitted_files.push(target_file.clone());
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

    let (build_config, use_rglob) = if path_type == "test" {
        (&module_config.build_config.test, true)
    } else {
        (&module_config.build_config.main, false)
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
        append_key_values(&mut extra_kv_pairs, &k, &lst);
    }

    for directive in project_conf
        .path_directives
        .iter()
        .filter(|e| element.starts_with(&e.prefix))
    {
        match directive.directives().as_ref() {
            Ok(loaded) => {
                for d in loaded {
                    match d {
                        Directive::BinaryRef(_) => todo!(),
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
                            directive::ManualRefDirective::DataRef => {
                                let t = extra_kv_pairs.entry("data".to_string()).or_default();
                                t.push(manual_ref.target_value.clone())
                            }
                        },
                        Directive::AttrStringList(attr) => {
                            append_key_values(&mut extra_kv_pairs, &attr.attr_name, &attr.values);
                        }
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

    let mut t = TargetEntries {
        entries: Vec::default(),
    };
    let filegroup_target_name = format!("{}_files", target_name);

    let mut parent_include_src = Vec::default();

    fn apply_binaries(
        target_entries: &mut TargetEntries,
        node_metadata: &GraphNodeMetadata,
        module_config: &ModuleConfig,
        target_path: &str,
    ) -> Result<()> {
        if !node_metadata.binary_refs.is_empty() {
            let build_config = match &module_config.build_config.binary_application {
                Some(bc) => bc,
                None => return Err(anyhow!("No binary config specified")),
            };
            let mut required_load = HashMap::default();

            for h in build_config.headers.iter() {
                required_load.insert(
                    Arc::new(h.load_from.clone()),
                    vec![Arc::new(h.load_value.clone())],
                );
            }

            for bin in node_metadata.binary_refs.iter() {
                match bin.binary_refs.command {
                    directive::BinaryRefDirective::GenerateBinary => (),
                };

                let mut k_strs: Vec<(String, String)> = Default::default();

                if let Some(ep) = &bin.entity_path {
                    k_strs.push(("entity_path".to_string(), ep.to_string()));
                }

                if let Some(ep) = &bin.binary_refs.target_value {
                    k_strs.push(("binary_refs_value".to_string(), ep.to_string()));
                }

                k_strs.push(("owning_library".to_string(), format!("//{}", target_path)));

                target_entries.entries.push(TargetEntry {
                    name: bin.binary_refs.binary_name.clone(),
                    extra_kv_pairs: Vec::default(),
                    extra_k_strs: k_strs,
                    required_load: required_load.clone(),
                    visibility: None,
                    srcs: None,
                    target_type: Arc::new(build_config.function_name.clone()),
                });
            }
        }
        Ok(())
    }

    apply_binaries(&mut t, &graph_node.node_metadata, module_config, &element)?;

    fn apply_manual_refs(
        extra_kv_pairs: &mut HashMap<String, Vec<String>>,
        node_metadata: &GraphNodeMetadata,
    ) {
        for manual_ref in node_metadata.manual_refs.iter() {
            match &manual_ref.command {
                directive::ManualRefDirective::RuntimeRef => {
                    extra_kv_pairs
                        .entry("runtime_deps".to_string())
                        .or_default()
                        .push(manual_ref.target_value.clone());
                }
                directive::ManualRefDirective::Ref => {
                    extra_kv_pairs
                        .entry("deps".to_string())
                        .or_default()
                        .push(manual_ref.target_value.clone());
                }
                directive::ManualRefDirective::DataRef => {
                    extra_kv_pairs
                        .entry("data".to_string())
                        .or_default()
                        .push(manual_ref.target_value.clone());
                }
            }
        }
    }

    apply_manual_refs(&mut extra_kv_pairs, &graph_node.node_metadata);

    fn append_key_values(
        extra_kv_pairs: &mut HashMap<String, Vec<String>>,
        key: &String,
        values: &Vec<String>,
    ) {
        extra_kv_pairs
            .entry(key.clone())
            .or_default()
            .extend(values.iter().cloned());
    }

    fn apply_attr_string_lists(
        extra_kv_pairs: &mut HashMap<String, Vec<String>>,
        node_metadata: &GraphNodeMetadata,
    ) {
        for attr in node_metadata.attr_string_lists.iter() {
            append_key_values(extra_kv_pairs, &attr.attr_name, &attr.values);
        }
    }

    apply_attr_string_lists(&mut extra_kv_pairs, &graph_node.node_metadata);

    if use_rglob {
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
            srcs: Some(SrcType::Glob {
                include: vec![format!("**/*.{}", primary_extension)],
                exclude: Vec::default(),
            }),
            target_type: Arc::new(build_config.function_name.clone()),
            extra_k_strs: Vec::default(),
        };

        t.entries.push(target);
    } else {
        match graph_node.node_type {
            crate::build_graph::NodeType::Synthetic => {}
            crate::build_graph::NodeType::RealNode => {
                parent_include_src.push(format!(":{}", filegroup_target_name));

                let filegroup_target = TargetEntry {
                    name: filegroup_target_name.clone(),
                    extra_kv_pairs: Vec::default(),
                    required_load: HashMap::default(),
                    visibility: None,
                    srcs: Some(SrcType::Glob {
                        include: vec![format!("**/*.{}", primary_extension)],
                        exclude: Vec::default(),
                    }),
                    target_type: Arc::new("filegroup".to_string()),
                    extra_k_strs: Vec::default(),
                };
                t.entries.push(filegroup_target);
            }
        }

        for (node, metadata) in graph_node.child_nodes.iter() {
            if let Some(folder_name) = node.split('/').filter(|e| !e.is_empty()).last() {
                parent_include_src.push(format!("//{}:{}_files", node, folder_name));

                let filegroup_target = TargetEntry {
                    name: format!("{}_files", folder_name),
                    extra_kv_pairs: Vec::default(),
                    required_load: HashMap::default(),
                    visibility: None,
                    srcs: Some(SrcType::Glob {
                        include: vec![format!("**/*.{}", primary_extension)],
                        exclude: Vec::default(),
                    }),
                    target_type: Arc::new("filegroup".to_string()),
                    extra_k_strs: Vec::default(),
                };

                apply_manual_refs(&mut extra_kv_pairs, metadata);
                apply_attr_string_lists(&mut extra_kv_pairs, metadata);

                let mut t = TargetEntries {
                    entries: vec![filegroup_target],
                };

                apply_binaries(&mut t, metadata, module_config, &element)?;

                let _handle = concurrent_io_operations.acquire().await?;
                let sub_target = opt.working_directory.join(node).join("BUILD.bazel");
                emitted_files.push(sub_target.clone());
                tokio::fs::write(&sub_target, t.emit_build_file()?)
                    .await
                    .with_context(|| {
                        format!("Attempting to write file data to {:?}", target_file)
                    })?;
            } else {
                return Err(anyhow!("Unable to extract folder name for node: {}", node));
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
            srcs: Some(SrcType::List(parent_include_src)),
            target_type: Arc::new(build_config.function_name.clone()),
            extra_k_strs: Vec::default(),
        };

        t.entries.push(target);
    }

    let handle = concurrent_io_operations.acquire().await?;

    tokio::fs::write(&target_file, t.emit_build_file()?)
        .await
        .with_context(|| format!("Attempting to write file data to {:?}", target_file))?;
    drop(handle);

    Ok(emitted_files)
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
        let added_files = nxt.await??;
        for f in added_files.iter() {
            current_files.remove(f);
        }
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
            srcs: Some(SrcType::Glob {
                include: vec!["*.scala".to_string()],
                exclude: Vec::default(),
            }),
            target_type: Arc::new("scala_tests".to_string()),
            extra_k_strs: Vec::default(),
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
