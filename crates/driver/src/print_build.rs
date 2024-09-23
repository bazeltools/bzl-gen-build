use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    async_read_json_file,
    build_graph::{GraphMapping, GraphNode, GraphNodeMetadata},
    extract_defrefs::{self, path_is_match},
    to_directory, Opt, PrintBuildArgs,
};
use anyhow::{anyhow, Context, Result};
use ast::{Expr, Stmt};
use bzl_gen_build_python_utilities::{ast_builder, PythonProgram};
use bzl_gen_build_shared_types::{
    build_config::{SourceConfig, TargetNameStrategy, WriteMode},
    module_config::ModuleConfig,
    *,
};
use futures::{stream, StreamExt};
use ignore::WalkBuilder;
use rustpython_parser::ast;

use futures::Future;
use tokio::{io::AsyncWriteExt, sync::Semaphore};

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
    fn sort_like_buildifier<E>(kwargs: &mut Vec<(Arc<String>, E)>) {
        lazy_static::lazy_static! {
            static ref PRIORITY_MAP: HashMap<&'static str, i32> = {
                let mut m = HashMap::new();
                // this is from: https://github.com/bazelbuild/buildtools/blob/be1c24cc9a44b4fd2410ec5356e4e21926dd206a/tables/tables.go#L177C34-L210C2
                m.insert("name", -99);
                m.insert("archive_override.module_name", -99);
                m.insert("git_override.module_name", -99);
                m.insert("local_path_override.module_name", -99);
                m.insert("multiple_version_override.module_name", -99);
                m.insert("single_version_override.module_name", -99);
                m.insert("bazel_dep.version", -98);
                m.insert("module.version", -98);
                m.insert("gwt_name", -98);
                m.insert("package_name", -97);
                m.insert("visible_node_name", -96);
                m.insert("size", -95);
                m.insert("timeout", -94);
                m.insert("testonly", -93);
                m.insert("src", -92);
                m.insert("srcdir", -91);
                m.insert("srcs", -90);
                m.insert("out", -89);
                m.insert("outs", -88);
                m.insert("hdrs", -87);
                m.insert("has_services", -86);
                m.insert("include", -85);
                m.insert("of", -84);
                m.insert("baseline", -83);
                // All others sort here, at 0.
                m.insert("destdir", 1);
                m.insert("exports", 2);
                m.insert("runtime_deps", 3);
                m.insert("deps", 4);
                m.insert("implementation", 5);
                m.insert("implements", 6);
                m.insert("alwayslink", 7);
                m
            };
        }

        kwargs.sort_by(|(a_key_arc, _), (b_key_arc, _)| {
            let a_key = a_key_arc.as_str();
            let b_key = b_key_arc.as_str();

            let a_priority = *PRIORITY_MAP.get(a_key).unwrap_or(&0);
            let b_priority = *PRIORITY_MAP.get(b_key).unwrap_or(&0);

            match a_priority.cmp(&b_priority) {
                std::cmp::Ordering::Equal => a_key.cmp(b_key),
                other => other,
            }
        });
    }

    pub fn emit_build_function_call(&self) -> Result<Stmt> {
        let mut kw_args: Vec<(Arc<String>, Expr)> = Default::default();

        kw_args.push((
            Arc::new("name".to_string()),
            ast_builder::with_constant_str(self.name.clone()),
        ));

        if let Some(srcs) = &self.srcs {
            kw_args.push((Arc::new("srcs".to_string()), srcs.to_statement()));
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

        for (k, v) in &self.extra_k_strs {
            kw_args.push((
                Arc::new(k.clone()),
                ast_builder::with_constant_str(v.to_string()),
            ));
        }

        Self::sort_like_buildifier(&mut kw_args);
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
    pub fn to_statement(&self) -> Expr {
        match self {
            SrcType::Glob { include, exclude } => {
                let mut kw_args: Vec<(Arc<String>, Expr)> = Default::default();

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

#[derive(Debug, Default)]
struct TargetEntries {
    pub entries: Vec<TargetEntry>,
}

impl TargetEntries {
    // Helper
    fn load_statement(from: Arc<String>, methods: Vec<Arc<String>>) -> Stmt {
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
        Ok(format!(
            "# ---- BEGIN BZL_GEN_BUILD_GENERATED_CODE ---- no_hash
{}
# ---- END BZL_GEN_BUILD_GENERATED_CODE ---- no_hash
",
            &program
        ))
    }

    pub fn to_ast(&self) -> Result<PythonProgram> {
        let mut program: Vec<Stmt> = Vec::default();
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

    fn combine(t1: TargetEntries, t2: TargetEntries) -> TargetEntries {
        let mut entries: Vec<TargetEntry> = Vec::default();
        let mut names: HashSet<String> = HashSet::default();
        for entry in t1.entries {
            names.insert(entry.name.clone());
            entries.push(entry);
        }
        for entry in t2.entries {
            if !names.contains(&entry.name) {
                entries.push(entry);
            }
        }
        TargetEntries { entries: entries }
    }
}

async fn generate_targets<F, R>(
    opt: &'static Opt,
    project_conf: &'static ProjectConf,
    source_conf: SourceConfig,
    graph_nodes: &Vec<GraphNode>,
    element: &String,
    emitted_files: &mut Vec<PathBuf>,
    on_child: F,
) -> Result<TargetEntries>
where
    F: Fn(PathBuf, TargetEntries) -> R,
    R: Future<Output = Result<i32>> + Send + 'static,
{
    let mut module_config: Option<&ModuleConfig> = None;
    for (_k, v) in project_conf.configurations.iter() {
        let paths = if source_conf == SourceConfig::Main {
            v.main_roots.clone()
        } else {
            v.test_roots.clone()
        };
        let matched_paths: Vec<String> = paths
            .into_iter()
            .filter(|p| element.starts_with(p.as_str()))
            .take(2)
            .collect();

        // This configuration doesn't match, but others might.
        if matched_paths.is_empty() {
            continue;
        }
        if matched_paths.len() > 1 {
            return Err(anyhow::anyhow!(
                "Found too many paths for {}, at least: {:?}",
                element,
                matched_paths
            ));
        }
        if module_config.is_none() {
            module_config = Some(v);
        } else {
            return Err(anyhow::anyhow!("Multiple configurations matched for {}, at least: {:?}; module config was before: {:?}", element, matched_paths, module_config));
        }
    }
    let module_config = if let Some(a) = module_config {
        a
    } else {
        return Ok(Default::default());
    };

    let target_folder = opt.working_directory.join(&element);
    let base_name = to_file_name(&target_folder);
    let mut t: TargetEntries = Default::default();
    let test_globs = &module_config.test_globs;
    let globset = extract_defrefs::to_globset(test_globs)?;
    for graph_node in graph_nodes {
        let node_file = opt.working_directory.join(&graph_node.node_label);
        let node_file_name = to_file_name(&node_file);
        let relative_path = node_file
            .as_path()
            .strip_prefix(opt.working_directory.as_path())?
            .to_path_buf();
        let mut extra_kv_pairs: HashMap<String, Vec<String>> = HashMap::default();
        let (build_config, use_rglob) = if source_conf == SourceConfig::Test {
            (&module_config.build_config.test, !opt.no_aggregate_source)
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

        let target_name_strategy = build_config.target_name_strategy;
        let target_name = if !opt.no_aggregate_source {
            base_name.clone()
        } else {
            to_name_from_file_name(&node_file_name, target_name_strategy)?
        };

        fn add_non_empty(
            opt: &'static Opt,
            key: &str,
            labels: &Vec<String>,
            extra_kv_pairs: &mut HashMap<String, Vec<String>>,
            target_name_strategy: TargetNameStrategy,
        ) {
            if !labels.is_empty() {
                let vals = labels
                    .iter()
                    .map(|e| to_label(&opt, e, target_name_strategy))
                    .collect();
                extra_kv_pairs.insert(key.to_string(), vals);
            }
        }

        add_non_empty(
            opt,
            "deps",
            &graph_node.dependencies,
            &mut extra_kv_pairs,
            target_name_strategy,
        );
        add_non_empty(
            opt,
            "runtime_deps",
            &graph_node.runtime_dependencies,
            &mut extra_kv_pairs,
            target_name_strategy,
        );

        for (k, lst) in build_config.extra_key_to_list.iter() {
            append_key_values(&mut extra_kv_pairs, k.clone(), &lst);
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
                                append_key_values(
                                    &mut extra_kv_pairs,
                                    attr.attr_name.clone(),
                                    &attr.values,
                                );
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

        let filegroup_target_name = format!("{}_files", target_name);

        let mut parent_include_src = Vec::default();

        apply_binaries(
            &mut t,
            &graph_node.node_metadata,
            module_config,
            &target_name,
        )?;
        apply_manual_refs(&mut extra_kv_pairs, &graph_node.node_metadata);
        apply_attr_string_lists(&mut extra_kv_pairs, &graph_node.node_metadata);
        // before we give extra_kv_pairs away to make the main target,
        // we need to clone deps here for a later use in secondaries.
        let deps = extra_kv_pairs.get("deps").cloned().unwrap_or_else(Vec::new);
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
                    if !opt.no_aggregate_source {
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
                    } else {
                        if path_is_match(&relative_path, test_globs, &globset, &source_conf) {
                            parent_include_src.push(format!("{}", node_file_name));
                        } else {
                            continue; // continue for graph_nodes
                        }
                    }
                }
            }

            let mut child_nodes: HashMap<String, Vec<GraphNodeMetadata>> = HashMap::default();
            for (entry, metadata) in graph_node.child_nodes.iter() {
                let metadata = metadata.clone();
                let element = if !opt.no_aggregate_source {
                    entry.clone()
                } else {
                    to_directory(entry.to_string())
                };
                let v = child_nodes.entry(element).or_default();
                v.push(metadata);
            }
            for (directory, metadatas) in child_nodes.iter() {
                if let Some(folder_name) = directory.split('/').filter(|e| !e.is_empty()).last() {
                    parent_include_src.push(format!("//{}:{}_files", directory, folder_name));

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
                    let mut t = TargetEntries {
                        entries: vec![filegroup_target],
                    };

                    for metadata in metadatas {
                        apply_manual_refs(&mut extra_kv_pairs, metadata);
                        apply_attr_string_lists(&mut extra_kv_pairs, metadata);
                        apply_binaries(&mut t, metadata, module_config, &directory)?;
                    }

                    let sub_target = opt.working_directory.join(directory).join("BUILD.bazel");
                    emitted_files.push(sub_target.clone());
                    on_child(sub_target, t).await?;
                } else {
                    return Err(anyhow!(
                        "Unable to extract folder name for node: {}",
                        directory
                    ));
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
                srcs: Some(SrcType::List(parent_include_src.clone())),
                target_type: Arc::new(build_config.function_name.clone()),
                extra_k_strs: Vec::default(),
            };

            t.entries.push(target);
        }

        apply_secondary_rules(
            &mut t,
            module_config,
            &target_name,
            &parent_include_src,
            &deps,
        );
    } // end for graph_nodes

    fn to_label(
        opt: &'static Opt,
        entry: &str,
        target_name_strategy: TargetNameStrategy,
    ) -> String {
        if entry.starts_with('@') {
            entry.to_string()
        } else {
            if !opt.no_aggregate_source {
                format!("//{}", entry)
            } else {
                let directory = to_directory(entry.to_string());
                let full_file = opt.working_directory.join(entry);
                let file_name = to_file_name(&full_file);
                // Result<String> fails only if the file_name fails to get a file stem, so it's not likely
                let name =
                    to_name_from_file_name(&file_name, target_name_strategy).unwrap_or(file_name);
                format!("//{}:{}", directory, name)
            }
        }
    }

    fn to_name_from_file_name(
        file_name: &str,
        target_name_strategy: TargetNameStrategy,
    ) -> Result<String> {
        match target_name_strategy {
            TargetNameStrategy::SourceFileStem => match Path::new(&file_name).file_stem() {
                Some(s) => Ok(s.to_string_lossy().to_string()),
                None => Err(anyhow!("can't get file_stem of {}", file_name)),
            },
            TargetNameStrategy::Auto => Ok(file_name.replace(".", "_")),
        }
    }

    fn to_file_name(path: &Path) -> String {
        path.file_name().unwrap().to_str().unwrap().to_string()
    }

    fn apply_binaries(
        target_entries: &mut TargetEntries,
        node_metadata: &GraphNodeMetadata,
        module_config: &ModuleConfig,
        lib_target: &str,
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

                if let Some(tv) = &bin.binary_refs.target_value {
                    k_strs.push(("binary_refs_value".to_string(), tv.to_string()));
                }

                k_strs.push((
                    "owning_library".to_string(),
                    format!(":{}", lib_target),
                ));

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

    fn append_key_values(
        extra_kv_pairs: &mut HashMap<String, Vec<String>>,
        key: String,
        values: &Vec<String>,
    ) {
        extra_kv_pairs
            .entry(key)
            .or_default()
            .extend(values.iter().cloned());
    }

    fn apply_attr_string_lists(
        extra_kv_pairs: &mut HashMap<String, Vec<String>>,
        node_metadata: &GraphNodeMetadata,
    ) {
        for attr in node_metadata.attr_string_lists.iter() {
            append_key_values(extra_kv_pairs, attr.attr_name.clone(), &attr.values);
        }
    }

    fn apply_secondary_rules(
        target_entries: &mut TargetEntries,
        module_config: &ModuleConfig,
        parent_target_name: &str,
        parent_include_src: &Vec<String>,
        parent_deps: &Vec<String>,
    ) {
        for (k, build_config) in module_config.build_config.secondary_rules.iter() {
            let sec_target_name = format!("{}_{}", parent_target_name, k);
            let mut required_load = HashMap::default();
            let mut srcs = Option::default();
            for h in build_config.headers.iter() {
                required_load.insert(
                    Arc::new(h.load_from.clone()),
                    vec![Arc::new(h.load_value.clone())],
                );
            }
            let mut extra_kv_pairs: HashMap<String, Vec<String>> = HashMap::default();
            for (k, lst) in &build_config.extra_key_to_list {
                let vs = lst
                    .iter()
                    .flat_map(|v| {
                        eval_extra_var(v, parent_target_name, parent_include_src, parent_deps)
                    })
                    .collect::<Vec<_>>();
                match k.as_str() {
                    "srcs" => srcs = Some(SrcType::List(vs)),
                    _ => append_key_values(&mut extra_kv_pairs, k.clone(), &vs),
                }
            }
            target_entries.entries.push(TargetEntry {
                name: sec_target_name.clone(),
                extra_kv_pairs: extra_kv_pairs
                    .into_iter()
                    .map(|(k, mut v)| {
                        v.sort();
                        v.dedup();
                        (k, v)
                    })
                    .collect(),
                extra_k_strs: build_config
                    .extra_key_to_value
                    .clone()
                    .into_iter()
                    .map(|(k, v)| (k, v))
                    .collect(),
                required_load: required_load.clone(),
                visibility: None,
                srcs: srcs,
                target_type: Arc::new(build_config.function_name.clone()),
            });
        }
    }

    // This expands `${name}` etc appearing inside of the extra_key_to_list value
    // with the name of the parent target.
    fn eval_extra_var(
        value: &String,
        parent_target_name: &str,
        parent_include_src: &Vec<String>,
        parent_deps: &Vec<String>,
    ) -> Vec<String> {
        if value.contains("${name}") {
            vec![value.replace("${name}", parent_target_name)]
        } else if value.contains("${srcs}") {
            parent_include_src
                .clone()
                .into_iter()
                .map(|v| value.replace("${srcs}", &v))
                .collect()
        } else if value.contains("${deps}") {
            parent_deps
                .clone()
                .into_iter()
                .map(|v| value.replace("${deps}", &fully_qualified_label(&v)))
                .collect()
        } else {
            vec![value.to_string()]
        }
    }

    fn fully_qualified_label(value: &String) -> String {
        if value.starts_with("//") {
            if value.contains(":") {
                value.to_string()
            } else {
                format!("{}:{}", value, value.split("/").last().unwrap()).to_string()
            }
        } else {
            value.to_string()
        }
    }

    Ok(t)
}

// Performs the side effect of writing BUILD file
async fn print_file(
    opt: &'static Opt,
    project_conf: &'static ProjectConf,
    mut graph_nodes: Vec<GraphNode>,
    concurrent_io_operations: &'static Semaphore,
    element: String,
) -> Result<Vec<PathBuf>> {
    graph_nodes.sort_by(|a, b| a.node_label.cmp(&b.node_label));
    let mut emitted_files: Vec<PathBuf> = Vec::default();
    let target_folder = opt.working_directory.join(&element);
    let target_file = target_folder.join("BUILD.bazel");
    emitted_files.push(target_file.clone());
    let t1 = generate_targets(
        opt,
        project_conf,
        SourceConfig::Main,
        &graph_nodes,
        &element,
        &mut emitted_files,
        |sub_target: PathBuf, t: TargetEntries| async move {
            let _handle = concurrent_io_operations.acquire().await?;
            tokio::fs::write(sub_target.clone(), t.emit_build_file()?)
                .await
                .with_context(|| format!("Attempting to write file data to {:?}", sub_target))?;
            Ok(0)
        },
    )
    .await?;
    let t2 = generate_targets(
        opt,
        project_conf,
        SourceConfig::Test,
        &graph_nodes,
        &element,
        &mut emitted_files,
        |sub_target: PathBuf, t: TargetEntries| async move {
            let _handle = concurrent_io_operations.acquire().await?;
            tokio::fs::write(sub_target.clone(), t.emit_build_file()?)
                .await
                .with_context(|| format!("Attempting to write file data to {:?}", sub_target))?;
            Ok(0)
        },
    )
    .await?;
    let t = TargetEntries::combine(t1, t2);
    let handle = concurrent_io_operations.acquire().await?;
    let write_mode = WriteMode::new(opt.append);
    match write_mode {
        WriteMode::Append => {
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .append(true)
                .open(&target_file)
                .await?;

            if !t.entries.is_empty() {
                file.write(t.emit_build_file()?.as_bytes())
                    .await
                    .with_context(|| {
                        format!("Attempting to write file data to {:?}", target_file)
                    })?;
            }
        }
        WriteMode::Overwrite => {
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&target_file)
                .await?;
            if !t.entries.is_empty() {
                file.write_all(t.emit_build_file()?.as_bytes())
                    .await
                    .with_context(|| {
                        format!("Attempting to write file data to {:?}", target_file)
                    })?;
            }
        }
    }
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

    let mut graph_nodes: HashMap<String, Vec<GraphNode>> = HashMap::default();
    for (entry, graph_node) in graph_data
        .build_mapping
        .into_iter()
        .filter(|(k, _v)| !k.starts_with('@'))
    {
        let element = if !opt.no_aggregate_source {
            entry
        } else {
            to_directory(entry.to_string())
        };
        let v = graph_nodes.entry(element).or_default();
        v.push(graph_node);
    }

    let mut res = Vec::default();
    for (element, nodes) in graph_nodes {
        res.push(tokio::spawn(async move {
            print_file(opt, project_conf, nodes, concurrent_io_operations, element).await
        }));
    }

    while let Some(nxt) = res.pop() {
        let added_files = nxt.await??;
        for f in added_files.iter() {
            current_files.remove(f);
        }
    }

    // These files are old and not updated..
    let write_mode = WriteMode::new(opt.append);
    if write_mode == WriteMode::Overwrite {
        for f in current_files {
            println!("Deleting no longer used build file of: {:?}", f);
            std::fs::remove_file(&f)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_config::{BuildConfig, BuildLoad, GrpBuildConfig};
    use crate::build_graph::NodeType;
    use crate::Commands::PrintBuild;
    use std::collections::BTreeMap;

    fn example_opt(no_aggregate_source: bool, write_mode: WriteMode) -> Opt {
        Opt {
            input_path: PathBuf::new(),
            working_directory: PathBuf::new(),
            concurrent_io_operations: 8,
            cache_path: PathBuf::new(),
            no_aggregate_source: no_aggregate_source,
            append: write_mode == WriteMode::Append,
            command: PrintBuild(PrintBuildArgs {
                graph_data: PathBuf::new(),
            }),
        }
    }

    fn example_project_conf() -> ProjectConf {
        ProjectConf {
            configurations: HashMap::from([(
                "protos".to_string(),
                ModuleConfig {
                    file_extensions: vec!["proto".to_string()],
                    build_config: BuildConfig {
                        main: Some(GrpBuildConfig {
                            headers: vec![BuildLoad {
                                load_from: "@rules_proto//proto:defs.bzl".to_string(),
                                load_value: "proto_library".to_string(),
                            }],
                            function_name: "proto_library".to_string(),
                            target_name_strategy: TargetNameStrategy::SourceFileStem,
                            extra_key_to_list: HashMap::default(),
                            extra_key_to_value: HashMap::default(),
                        }),
                        test: None,
                        binary_application: None,
                        secondary_rules: BTreeMap::default(),
                    },
                    main_roots: vec!["src/main/protos".to_string()],
                    test_roots: vec!["src/test/protos".to_string()],
                    test_globs: vec![],
                },
            )]),
            includes: vec![],
            path_directives: vec![],
        }
    }

    fn example_project_conf_with_secondaries() -> ProjectConf {
        ProjectConf {
            configurations: HashMap::from([(
                "protos".to_string(),
                ModuleConfig {
                    file_extensions: vec!["proto".to_string()],
                    build_config: BuildConfig {
                        main: Some(GrpBuildConfig {
                            headers: vec![BuildLoad {
                                load_from: "@rules_proto//proto:defs.bzl".to_string(),
                                load_value: "proto_library".to_string(),
                            }],
                            function_name: "proto_library".to_string(),
                            target_name_strategy: TargetNameStrategy::Auto,
                            extra_key_to_list: HashMap::default(),
                            extra_key_to_value: HashMap::default(),
                        }),
                        test: None,
                        binary_application: None,
                        secondary_rules: BTreeMap::from([
                            (
                                "java".to_string(),
                                GrpBuildConfig {
                                    headers: vec![],
                                    function_name: "java_proto_library".to_string(),
                                    target_name_strategy: TargetNameStrategy::Auto,
                                    extra_key_to_list: HashMap::from([(
                                        "deps".to_string(),
                                        vec![":${name}".to_string()],
                                    )]),
                                    extra_key_to_value: HashMap::default(),
                                },
                            ),
                            (
                                "py".to_string(),
                                GrpBuildConfig {
                                    headers: vec![BuildLoad {
                                        load_from: "@com_google_protobuf//:protobuf.bzl"
                                            .to_string(),
                                        load_value: "py_proto_library".to_string(),
                                    }],
                                    function_name: "py_proto_library".to_string(),
                                    target_name_strategy: TargetNameStrategy::Auto,
                                    extra_key_to_list: HashMap::from([
                                        ("srcs".to_string(), vec!["${srcs}".to_string()]),
                                        ("deps".to_string(), vec!["${deps}_py".to_string()]),
                                    ]),
                                    extra_key_to_value: HashMap::default(),
                                },
                            ),
                        ]),
                    },
                    main_roots: vec!["src/main/protos".to_string()],
                    test_roots: vec!["src/test/protos".to_string()],
                    test_globs: vec![],
                },
            )]),
            includes: vec![],
            path_directives: vec![],
        }
    }

    #[tokio::test]
    async fn test_generate_targets() -> Result<(), Box<dyn std::error::Error>> {
        let mut build_graph = GraphNode::default();
        build_graph.node_type = NodeType::RealNode;
        build_graph.node_label = "src/main/protos".to_string();
        test_generate_targets_base(
            example_project_conf(),
            vec![build_graph],
            "src/main/protos".to_string(),
            2,
            r#"load('@rules_proto//proto:defs.bzl', 'proto_library')

filegroup(
    name='protos_files',
    srcs=glob(include=['**/*.proto']),
    visibility=['//visibility:public']
)

proto_library(
    name='protos',
    srcs=[':protos_files'],
    visibility=['//visibility:public']
)
        "#,
            false,
            WriteMode::Overwrite,
        )
        .await
    }

    #[tokio::test]
    async fn test_generate_targets_with_secondaries() -> Result<(), Box<dyn std::error::Error>> {
        let mut node1 = GraphNode::default();
        node1.node_label = "src/main/protos/a.proto".to_string();
        node1.node_type = NodeType::RealNode;

        let mut node2 = GraphNode::default();
        node2.node_label = "src/main/protos/b.proto".to_string();
        node2.node_type = NodeType::RealNode;
        node2
            .dependencies
            .push("src/main/protos/a.proto".to_string());
        test_generate_targets_base(
            example_project_conf_with_secondaries(),
            vec![node1, node2],
            "src/main/protos".to_string(),
            6,
            r#"load('@com_google_protobuf//:protobuf.bzl', 'py_proto_library')
load('@rules_proto//proto:defs.bzl', 'proto_library')

proto_library(
    name='a_proto',
    srcs=['a.proto'],
    visibility=['//visibility:public']
)

java_proto_library(
    name='a_proto_java',
    visibility=['//visibility:public'],
    deps=[':a_proto'],
)

py_proto_library(
    name='a_proto_py',
    srcs=['a.proto'],
    visibility=['//visibility:public'],
    deps=[],
)

proto_library(
    name='b_proto',
    srcs=['b.proto'],
    visibility=['//visibility:public'],
    deps=['//src/main/protos:a_proto'],
)

java_proto_library(
    name='b_proto_java',
    visibility=['//visibility:public'],
    deps=[':b_proto'],
)

py_proto_library(
    name='b_proto_py',
    srcs=['b.proto'],
    visibility=['//visibility:public'],
    deps=['//src/main/protos:a_proto_py'],
)
        "#,
            true,
            WriteMode::Overwrite,
        )
        .await
    }

    async fn test_generate_targets_base(
        project_conf: ProjectConf,
        build_graph: Vec<GraphNode>,
        element: String,
        expected_target_count: usize,
        expected_build_file: &str,
        no_aggregate_source: bool,
        write_mode: WriteMode,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut emitted_files: Vec<PathBuf> = Vec::default();
        let opt = Box::leak(Box::new(example_opt(no_aggregate_source, write_mode)));
        let boxed_project_conf = Box::leak(Box::new(project_conf));
        let target_entries = generate_targets(
            opt,
            boxed_project_conf,
            SourceConfig::Main,
            &build_graph,
            &element,
            &mut emitted_files,
            |_sub_target: PathBuf, _t: TargetEntries| async move { Ok(0) },
        )
        .await?;
        assert_eq!(target_entries.entries.len(), expected_target_count);
        let generated_s = target_entries.emit_build_file()?;
        let actual_parsed = PythonProgram::parse(generated_s.as_str(), "tmp.py")?;
        let expected_parsed = {
            let parsed = PythonProgram::parse(expected_build_file, "tmp.py").unwrap();
            PythonProgram::parse(format!("{}", parsed).as_str(), "tmp.py").unwrap()
        };
        assert_eq!(
            expected_parsed.to_string(),
            actual_parsed.to_string(),
            "\n\nexpected:\n{}\n\nactual:\n{}\n",
            expected_parsed,
            actual_parsed
        );
        Ok(())
    }

    #[test]
    fn test_simple_target_entry() {
        let python_source = r#"load("//build_tools/lang_support/scala/test:scalatest.bzl", "scala_tests")
scala_tests(
    name = "scala_extractor",
    srcs = glob(include =  ["*.scala"]),
    visibility = ["//visibility:public"],
    deps = [
        "//src/main/scala/com/example/scala_extractor",
        "@jvm__io_circe__circe_core//:jar",
        "@jvm__org_scalacheck__scalacheck//:jar",
    ],
)
        "#;

        let parsed_from_embed_string = {
            let parsed = PythonProgram::parse(python_source, "tmp.py").unwrap();
            PythonProgram::parse(format!("{}", parsed).as_str(), "tmp.py").unwrap()
        };

        let mut entries = Vec::default();
        entries.push(make_target_entry("scala_extractor"));
        let target_entries = TargetEntries { entries };

        let generated_s = target_entries.emit_build_file().unwrap();

        let parsed_from_generated_string =
            PythonProgram::parse(generated_s.as_str(), "tmp.py").unwrap();

        assert_eq!(
            parsed_from_embed_string.to_string(),
            parsed_from_generated_string.to_string(),
            "\n\nExpected:\n{}\n\nGenerated:\n{}\n",
            parsed_from_embed_string,
            parsed_from_generated_string
        );
    }

    fn make_target_entry(name: &str) -> TargetEntry {
        let mut required_load = HashMap::new();
        required_load.insert(
            Arc::new("//build_tools/lang_support/scala/test:scalatest.bzl".to_string()),
            vec![Arc::new("scala_tests".to_string())],
        );
        TargetEntry {
            name: name.to_string(),
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
        }
    }

    #[test]
    fn test_combine() {
        let mut e1 = Vec::default();
        e1.push(make_target_entry("t1"));
        let ts1 = TargetEntries { entries: e1 };

        let mut e2 = Vec::default();
        e2.push(make_target_entry("t2"));
        let ts2 = TargetEntries { entries: e2 };

        let mut e3 = Vec::default();
        e3.push(make_target_entry("t1"));
        let ts3 = TargetEntries { entries: e3 };

        let mut e4 = Vec::default();
        e4.push(make_target_entry("t1"));
        let ts4 = TargetEntries { entries: e4 };

        let actual1 = TargetEntries::combine(ts1, ts2);
        assert_eq!(actual1.entries.len(), 2);

        let actual2 = TargetEntries::combine(ts3, ts4);
        assert_eq!(actual2.entries.len(), 1);
    }
}
