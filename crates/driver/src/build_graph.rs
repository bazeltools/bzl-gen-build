use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

use crate::{async_read_json_file, read_json_file, write_json_file, BuildGraphArgs, Opt};
use anyhow::{anyhow, Result};
use bzl_gen_build_shared_types::{
    directive::{
        AttrStringListConfig, BinaryRefAndPath, BinaryRefConfig, EntityDirectiveConfig,
        ManualRefConfig,
    },
    internal_types::tree_node::TreeNode,
    *,
};

use log::{debug, info};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use super::extract_defrefs::ExtractedMappings;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, Eq)]
pub struct GraphNode {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "HashMap::is_empty",
        serialize_with = "bzl_gen_build_shared_types::serde_helpers::ordered_map"
    )]
    pub child_nodes: HashMap<String, GraphNodeMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_dependencies: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_ref_configs: Vec<ManualRefConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binary_ref_configs: Vec<BinaryRefConfig>,
    #[serde(default, skip_serializing_if = "GraphNodeMetadata::is_empty")]
    pub node_metadata: GraphNodeMetadata,
    pub node_type: NodeType,
    pub node_label: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, Eq)]
pub struct GraphNodeMetadata {
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binary_refs: Vec<BinaryRefAndPath>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_refs: Vec<ManualRefConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attr_string_lists: Vec<AttrStringListConfig>,
}
impl GraphNodeMetadata {
    pub fn is_empty(&self) -> bool {
        self.binary_refs.is_empty()
            && self.manual_refs.is_empty()
            && self.attr_string_lists.is_empty()
    }
}

impl<'a> From<&'a NodeExternalState> for GraphNodeMetadata {
    fn from(nes: &'a NodeExternalState) -> Self {
        Self {
            name: nes.name.to_string(),
            binary_refs: nes.binary_refs.clone(),
            manual_refs: nes.manual_refs.clone(),
            attr_string_lists: nes.attr_string_lists.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct GraphMapping {
    #[serde(serialize_with = "bzl_gen_build_shared_types::serde_helpers::ordered_map")]
    pub build_mapping: HashMap<String, GraphNode>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DefinedBy {
    #[serde(serialize_with = "bzl_gen_build_shared_types::serde_helpers::ordered_map")]
    pub defined_by: HashMap<String, Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PathToDefs {
    pub relative_path_to_defs: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct DefsData {
    pub defs: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum NodeType {
    Synthetic,
    RealNode,
}
impl Default for NodeType {
    fn default() -> Self {
        Self::Synthetic
    }
}

#[derive(Debug)]
struct NodeExternalState {
    pub name: Arc<String>,
    pub node_type: NodeType,
    pub binary_refs: Vec<BinaryRefAndPath>,
    pub manual_refs: Vec<ManualRefConfig>,
    pub attr_string_lists: Vec<AttrStringListConfig>,
}
impl NodeExternalState {
    pub fn empty(name: Arc<String>, node_type: NodeType) -> Self {
        Self {
            name,
            node_type,
            binary_refs: Default::default(),
            manual_refs: Default::default(),
            attr_string_lists: Default::default(),
        }
    }
}

#[derive(Debug, Default)]
struct GraphState {
    pub forward_map: HashMap<Arc<String>, usize>,
    pub reverse_map: HashMap<usize, Arc<NodeExternalState>>,
    compile_edges: HashMap<usize, HashSet<usize>>,
    pub runtime_edges: HashMap<usize, HashSet<usize>>,
    pub consumed_nodes: HashMap<usize, HashSet<usize>>,
    node_counter: usize,
    // These are used/populated for debugging purposes, maybe add a feature flag to turn off and see if perf matters?
    def_to_id: Arc<HashMap<Arc<String>, u64>>,
    owns_map: HashMap<u64, HashSet<usize>>,
    node_to_defs_cache: Option<HashMap<usize, Arc<HashSet<Arc<String>>>>>,
    // Reverse of compile_edges/runtime_edges (target -> sources). Only populated while
    // `collapse` runs, so that merging a node costs its degree rather than the graph size.
    compile_in: HashMap<usize, HashSet<usize>>,
    runtime_in: HashMap<usize, HashSet<usize>>,
}

impl GraphState {
    #[cfg(test)]
    pub fn get_compile_edges<'a>(&'a self, node_id: usize) -> Option<&HashSet<usize>> {
        self.compile_edges.get(&node_id)
    }

    pub fn get_metadata<'a>(&'a self, node_id: &usize) -> Option<&'a NodeExternalState> {
        self.reverse_map.get(node_id).map(|e| e.as_ref())
    }

    pub fn get_node_label<'a>(&'a self, node_id: &usize) -> Option<&'a str> {
        self.get_metadata(node_id).map(|e| e.name.as_str())
    }

    #[cfg(test)]
    pub fn node_count(&self) -> usize {
        self.compile_edges.len()
    }
    #[cfg(test)]
    pub fn node_exists(&self, node: usize) -> bool {
        self.compile_edges.contains_key(&node)
    }

    // Mostly used for debugging, so allow unused.
    #[allow(dead_code)]
    pub fn node_to_defs(&mut self, node_id: usize) -> Result<Arc<HashSet<Arc<String>>>> {
        fn populate_cache(graph_state: &mut GraphState) -> Result<()> {
            let mut node_to_defs: HashMap<usize, HashSet<Arc<String>>> = HashMap::default();
            let reversed_map: HashMap<u64, Arc<String>> = graph_state
                .def_to_id
                .iter()
                .map(|(k, v)| (*v, k.clone()))
                .collect();

            for (idx, owners) in graph_state.owns_map.iter() {
                for v in owners.iter() {
                    let e = node_to_defs.entry(*v).or_default();
                    match reversed_map.get(idx) {
                        Some(s) => {
                            e.insert(s.clone());
                        }
                        None => {
                            return Err(anyhow!(
                                "Should not happen, was unable to find {} as a def map entry",
                                idx
                            ))
                        }
                    }
                }
            }
            graph_state.node_to_defs_cache = Some(
                node_to_defs
                    .into_iter()
                    .map(|(k, v)| (k, Arc::new(v)))
                    .collect(),
            );
            Ok(())
        }

        if self.node_to_defs_cache.is_none() {
            populate_cache(self)?;
        }

        if let Some(node_to_def) = &self.node_to_defs_cache {
            return Ok(match node_to_def.get(&node_id) {
                None => Arc::new(HashSet::default()),
                Some(x) => x.clone(),
            });
        }

        Err(anyhow!("Unreachable!"))
    }

    pub fn add_node(&mut self, node: String, node_type: NodeType) -> usize {
        match self.forward_map.get(&node) {
            Some(idx) => *idx,
            None => {
                let nxt_id = self.node_counter;
                self.node_counter += 1;
                let v = Arc::new(node);
                self.forward_map.insert(v.clone(), nxt_id);
                self.reverse_map
                    .insert(nxt_id, Arc::new(NodeExternalState::empty(v, node_type)));
                self.compile_edges.insert(nxt_id, HashSet::default());
                nxt_id
            }
        }
    }

    #[allow(dead_code)]
    pub fn debug_node(&self, node_id: usize) {
        let outbound_compile_edges: Vec<&str> = self
            .compile_edges
            .get(&node_id)
            .map(|f| f.iter().map(|n| self.get_node_label(n).unwrap()).collect())
            .unwrap_or_default();

        let outbound_runtime_edges: Vec<&str> = self
            .runtime_edges
            .get(&node_id)
            .map(|f| f.iter().map(|n| self.get_node_label(n).unwrap()).collect())
            .unwrap_or_default();

        let self_name = self.get_node_label(&node_id).unwrap();
        println!(
            "
        Debugging node: {}
        Depends at compile time on :
            {:#?}

        Depends at runtime time on :
            {:#?}
        ",
            self_name, outbound_compile_edges, outbound_runtime_edges
        )
    }

    // only used in tests today
    #[cfg(test)]
    pub fn add_compile_edge(&mut self, from: usize, dest: usize) {
        let s = self.compile_edges.entry(from).or_default();
        s.insert(dest);
    }

    #[cfg(test)]
    pub fn add_runtime_edge(&mut self, from: usize, dest: usize) {
        let s = self.runtime_edges.entry(from).or_default();
        s.insert(dest);
    }

    fn find_or_create_common_ancestor<'a, T>(&mut self, candidates: T) -> Result<usize>
    where
        T: IntoIterator<Item = &'a usize> + Copy + std::fmt::Debug,
    {
        let str_vals: Vec<&str> = candidates
            .into_iter()
            .map(|e| self.get_node_label(e).unwrap())
            .collect();

        let first_c = str_vals.first().unwrap().to_string();
        let mut cur_tst = first_c.as_str();

        loop {
            if str_vals.iter().all(|e| match e.strip_prefix(cur_tst) {
                None => false,
                Some(rem) => rem.is_empty() || rem.starts_with('/'),
            }) {
                return Ok(self.add_node(cur_tst.to_string(), NodeType::Synthetic));
            } else {
                let nxt_dir = parent_dir(cur_tst);
                if nxt_dir == cur_tst {
                    return Err(anyhow::anyhow!(
                        "We cannot find a parent dir for {},{:#?}. This likely means there is an import cycle",
                        nxt_dir,
                        str_vals,
                    ));
                } else {
                    cur_tst = nxt_dir;
                }
            }
        }
    }

    pub fn node_is_live(&self, node: usize) -> bool {
        self.compile_edges.contains_key(&node)
    }

    pub fn common_ancestor(&mut self) -> Result<()> {
        // Only the keys are snapshotted; the consumed sets are read live below.
        let mut keys: Vec<usize> = self.consumed_nodes.keys().copied().collect();
        keys.sort_unstable();
        for k in keys {
            // Consumed by an earlier merge in this pass, so no longer a merge target.
            let Some(values) = self.consumed_nodes.get(&k) else {
                continue;
            };
            let ky = self.get_node_label(&k).unwrap();

            // Every live node whose label lies on a path below `ky` towards a consumed node.
            let mut to_collapse: HashSet<usize> = HashSet::default();
            let mut path = String::new();
            for ele in values.iter().flat_map(|e| self.get_node_label(e)) {
                let Some(remainder) = ele.strip_prefix(ky) else {
                    return Err(anyhow!("Element in equiv set doesn't share the same prefix: target: {} , parent: {}", ele, ky));
                };
                path.clear();
                path.push_str(ky);
                for next_section in remainder.split('/').filter(|e| !e.is_empty()) {
                    path.push('/');
                    path.push_str(next_section);
                    if let Some(id) = self.forward_map.get(&path) {
                        if self.node_is_live(*id) {
                            to_collapse.insert(*id);
                        }
                    }
                }
            }

            if !to_collapse.is_empty() {
                self.merge_node(k, &to_collapse)?;
            }
        }
        Ok(())
    }

    fn merge_node<'a, T>(&mut self, destination: usize, src: T) -> Result<()>
    where
        T: IntoIterator<Item = &'a usize> + Copy,
    {
        // Only valid while `collapse` maintains the incoming indexes.
        fn consume_edges(
            out: &mut HashMap<usize, HashSet<usize>>,
            incoming: &mut HashMap<usize, HashSet<usize>>,
            destination: usize,
            error_on_missing: bool,
            src: &HashSet<usize>,
        ) -> Result<()> {
            // The merged node points at everything its members pointed at, except themselves.
            let mut parent = out.remove(&destination).unwrap_or_default();
            for e in src.iter() {
                match out.remove(e) {
                    Some(old) => {
                        for t in old.iter() {
                            if let Some(sources) = incoming.get_mut(t) {
                                sources.remove(e);
                            }
                        }
                        parent.extend(old);
                    }
                    None if error_on_missing => {
                        return Err(anyhow!(
                            "Tried to squash a node {} into another node, but it didn't exist?",
                            e
                        ));
                    }
                    None => {}
                }
            }
            parent.remove(&destination);
            parent.retain(|t| !src.contains(t));
            for t in parent.iter() {
                incoming.entry(*t).or_default().insert(destination);
            }
            out.insert(destination, parent);

            // Whatever pointed at a consumed node now points at the destination.
            for e in src.iter() {
                for source in incoming.remove(e).into_iter().flatten() {
                    if source == destination || src.contains(&source) {
                        continue;
                    }
                    if let Some(edges) = out.get_mut(&source) {
                        edges.remove(e);
                        edges.insert(destination);
                    }
                    incoming.entry(destination).or_default().insert(source);
                }
            }
            if let Some(sources) = incoming.get_mut(&destination) {
                sources.retain(|s| *s != destination && !src.contains(s));
            }
            Ok(())
        }

        let mut consumed_nodes = self.consumed_nodes.remove(&destination).unwrap_or_default();
        consumed_nodes.extend(src.into_iter().copied());
        for x in src.into_iter() {
            if let Some(c) = self.consumed_nodes.remove(&x) {
                consumed_nodes.extend(c.into_iter());
            }
        }

        consumed_nodes.remove(&destination);
        self.consumed_nodes.insert(destination, consumed_nodes);

        let src: HashSet<usize> = src.into_iter().copied().filter(|e| *e != destination).collect();
        consume_edges(
            &mut self.compile_edges,
            &mut self.compile_in,
            destination,
            true,
            &src,
        )?;
        consume_edges(
            &mut self.runtime_edges,
            &mut self.runtime_in,
            destination,
            false,
            &src,
        )?;

        Ok(())
    }

    fn get_all_outbound_nodes<'a>(&'a self, node: usize) -> impl Iterator<Item = usize> + 'a {
        let c_edges = self
            .compile_edges
            .get(&node)
            .into_iter()
            .flat_map(|e| e.iter())
            .copied();
        let r_edges = self
            .runtime_edges
            .get(&node)
            .into_iter()
            .flat_map(|e| e.iter())
            .copied();

        c_edges.chain(r_edges)
    }

    /// Strongly connected components with more than one node over compile and runtime
    /// edges, i.e. every import cycle, found in a single iterative Tarjan pass.
    /// Members are sorted, and components are ordered by their smallest member, so the
    /// result doesn't depend on hash iteration order.
    fn cyclic_sccs(&self) -> Vec<Vec<usize>> {
        #[derive(Default)]
        struct Tarjan {
            index: HashMap<usize, usize>,
            low: HashMap<usize, usize>,
            on_stack: HashSet<usize>,
            stack: Vec<usize>,
        }
        impl Tarjan {
            fn discover(&mut self, node: usize) {
                let i = self.index.len();
                self.index.insert(node, i);
                self.low.insert(node, i);
                self.stack.push(node);
                self.on_stack.insert(node);
            }
        }

        let mut roots: Vec<usize> = self
            .compile_edges
            .keys()
            .chain(self.runtime_edges.keys())
            .copied()
            .collect();
        roots.sort_unstable();
        roots.dedup();

        let mut t = Tarjan::default();
        let mut sccs: Vec<Vec<usize>> = Vec::default();
        // (node, its outbound edges, next edge to visit): an explicit stack, since
        // dependency chains can be deep enough to overflow native recursion.
        let mut work: Vec<(usize, Vec<usize>, usize)> = Vec::default();

        for root in roots {
            if t.index.contains_key(&root) {
                continue;
            }
            t.discover(root);
            work.push((root, self.get_all_outbound_nodes(root).collect(), 0));

            while let Some(frame) = work.last_mut() {
                let v = frame.0;
                let next_edge = frame.1.get(frame.2).copied();
                frame.2 += 1;

                match next_edge {
                    Some(w) => {
                        if !t.index.contains_key(&w) {
                            t.discover(w);
                            work.push((w, self.get_all_outbound_nodes(w).collect(), 0));
                        } else if t.on_stack.contains(&w) {
                            let wi = t.index[&w];
                            let lv = t.low.get_mut(&v).unwrap();
                            *lv = (*lv).min(wi);
                        }
                    }
                    None => {
                        work.pop();
                        let v_low = t.low[&v];
                        if v_low == t.index[&v] {
                            let mut component = Vec::default();
                            while let Some(n) = t.stack.pop() {
                                t.on_stack.remove(&n);
                                component.push(n);
                                if n == v {
                                    break;
                                }
                            }
                            if component.len() > 1 {
                                component.sort_unstable();
                                sccs.push(component);
                            }
                        }
                        if let Some((parent, _, _)) = work.last() {
                            let lp = t.low.get_mut(parent).unwrap();
                            *lp = (*lp).min(v_low);
                        }
                    }
                }
            }
        }
        sccs.sort_unstable_by_key(|c| c[0]);
        sccs
    }

    fn check_cycle_allowed(
        &self,
        cycle: &[usize],
        circular_dependency_allow_list: &Vec<String>,
    ) -> Result<()> {
        for node in cycle {
            let node_label = self.get_node_label(node).unwrap_or_default();
            if !circular_dependency_allow_list
                .iter()
                .any(|p| node_label.starts_with(p))
            {
                return Err(anyhow::anyhow!("Circular dependency found in the package {}.
  {}
  Resolve the cycle, or opt in to collapsing this into a higher-level build target by adding circular_dependency_allow_list in the module config JSON.
  Note that creating such aggregate target across multiple directories will slow down the build anytime you make source change in the area.",
                    node_label,
                    cycle.iter().map(|n| self.get_node_label(n).unwrap_or_default()).collect::<Vec<_>>().join(", ")
                ));
            }
        }
        Ok(())
    }

    pub fn collapse(&mut self, circular_dependency_allow_list: &Vec<String>) -> Result<()> {
        fn reverse(edges: &HashMap<usize, HashSet<usize>>) -> HashMap<usize, HashSet<usize>> {
            let mut incoming: HashMap<usize, HashSet<usize>> = HashMap::default();
            for (source, targets) in edges.iter() {
                for t in targets.iter() {
                    incoming.entry(*t).or_default().insert(*source);
                }
            }
            incoming
        }
        self.compile_in = reverse(&self.compile_edges);
        self.runtime_in = reverse(&self.runtime_edges);
        let result = self.collapse_cycles(circular_dependency_allow_list);
        self.compile_in = Default::default();
        self.runtime_in = Default::default();
        result
    }

    fn collapse_cycles(&mut self, circular_dependency_allow_list: &Vec<String>) -> Result<()> {
        loop {
            let sccs = self.cyclic_sccs();
            if sccs.is_empty() {
                return Ok(());
            }
            for cycle in sccs {
                // Merging one component can consume or redirect nodes of another (the common
                // ancestor may be an existing node). Anything stale is found again next round.
                if !cycle.iter().all(|n| self.node_is_live(*n)) {
                    continue;
                }
                self.check_cycle_allowed(&cycle, circular_dependency_allow_list)?;
                let target = self.find_or_create_common_ancestor(&cycle)?;
                self.merge_node(target, &cycle)?;
            }
            self.common_ancestor()?;
        }
    }
}

fn parent_dir(k: &str) -> &str {
    if let Some(idx) = k.rfind('/') {
        k.split_at(idx).0
    } else {
        k
    }
}
async fn load_initial_graph(
    extracted_mappings: &ExtractedMappings,
    config_entity_directives: &Vec<directive::EntityDirectiveConfig>,
    all_defs: Arc<HashMap<Arc<String>, u64>>,
    concurrent_io_operations: &'static Semaphore,
) -> Result<GraphState> {
    let mut load_i = Vec::with_capacity(extracted_mappings.relative_path_to_extractmapping.len());

    for (_k, p) in extracted_mappings.relative_path_to_extractmapping.iter() {
        let pb = PathBuf::from(&p.path);
        let all_defs = all_defs.clone();
        load_i.push(tokio::spawn(async move {
            let c = concurrent_io_operations.acquire().await.unwrap();
            let r = async_read_json_file::<TreeNode>(&pb).await;
            drop(c);
            r.map(|e| {
                let runtime_refs: HashSet<u64> = e
                    .runtime_refs
                    .iter()
                    .filter_map(|k| all_defs.get(k))
                    .copied()
                    .collect();

                let refs: HashSet<u64> = e
                    .refs
                    .iter()
                    .filter_map(|k| all_defs.get(k))
                    .copied()
                    .collect();

                let defs: HashSet<u64> = e
                    .defs
                    .iter()
                    .filter_map(|k| all_defs.get(k))
                    .copied()
                    .collect();

                (
                    e.label_or_repo_path,
                    (
                        refs,
                        defs,
                        runtime_refs,
                        e.entity_directives,
                        e.binary_ref_directives,
                        e.manual_ref_directives,
                        e.attr_string_list_directives,
                    ),
                )
            })
        }));
    }

    let mut forward_map: HashMap<Arc<String>, usize> = HashMap::default();
    let mut reverse_map: HashMap<usize, Arc<NodeExternalState>> = HashMap::default();

    let mut owns_map: HashMap<u64, HashSet<usize>> = HashMap::default();
    struct TargetRefs {
        compile_time_refs: HashSet<u64>,
        runtime_refs: HashSet<u64>,
    }
    let mut refs_map: Vec<(usize, TargetRefs)> = Vec::default();

    #[derive(Default)]
    struct EntityLinksMaps {
        add_link_map: HashMap<u64, HashSet<u64>>,
    }
    impl EntityLinksMaps {
        fn add_directive(
            &mut self,
            d: &EntityDirectiveConfig,
            all_defs: &HashMap<Arc<String>, u64>,
        ) {
            if let Some(aon) = all_defs.get(&d.act_on) {
                let target = d
                    .pointing_at
                    .iter()
                    .flat_map(|d| all_defs.get(d).into_iter())
                    .copied();
                match d.command {
                    EntityDirective::Link => {
                        self.add_link_map.entry(*aon).or_default().extend(target);
                    }
                }
            }
        }
        // This is to ensure that the order of links being visited doesn't matter
        // and if we say A -> B
        // and B -> C
        // Then A -> [B, C]
        fn expand_out(&mut self) {
            let mut next_to_visit: Vec<u64> = self.add_link_map.keys().copied().collect();
            while !next_to_visit.is_empty() {
                let cur_visit: Vec<u64> = std::mem::take(&mut next_to_visit);
                for k in cur_visit.iter() {
                    if let Some(rem) = self.add_link_map.remove(&k) {
                        let updated_rem: HashSet<u64> = rem
                            .iter()
                            .flat_map(|e| {
                                self.add_link_map
                                    .get(e)
                                    .into_iter()
                                    .flat_map(|e| e.iter())
                                    .copied()
                                    .chain(std::iter::once(*e))
                            })
                            .collect();
                        if rem.len() != updated_rem.len() {
                            next_to_visit.push(*k);
                        }
                        if updated_rem.len() < rem.len() {
                            panic!("Invalid condition, should never get smaller");
                        }
                        self.add_link_map.insert(*k, updated_rem);
                    }
                }
            }
        }
    }
    let mut entity_links: EntityLinksMaps = Default::default();

    for ed in config_entity_directives.iter() {
        entity_links.add_directive(&ed, all_defs.as_ref());
    }

    let mut idx: usize = 0;
    for li in load_i {
        let (
            k,
            (
                compile_refs,
                defs,
                runtime_refs,
                entity_directives,
                binary_ref_directives,
                manual_ref_directives,
                attr_string_list_directives,
            ),
        ) = li.await??;

        let m = Arc::new(k);

        forward_map.insert(m.clone(), idx);
        let node_external_state = NodeExternalState {
            name: m,
            node_type: NodeType::RealNode,
            binary_refs: binary_ref_directives,
            manual_refs: manual_ref_directives,
            attr_string_lists: attr_string_list_directives,
        };
        reverse_map.insert(idx, Arc::new(node_external_state));

        for e in defs.iter() {
            match owns_map.entry(*e) {
                std::collections::hash_map::Entry::Occupied(mut o) => {
                    o.get_mut().insert(idx);
                    if log::log_enabled!(log::Level::Debug) {
                        let entries: Vec<Arc<String>> = o
                            .get()
                            .iter()
                            .flat_map(|l| reverse_map.get(l).into_iter())
                            .map(|e| &e.name)
                            .cloned()
                            .collect();
                        let d = all_defs.iter().find(|(_k, v)| *v == e).unwrap();
                        debug!(
                            "Detected duplicate def insertion., For entity {}, found {:?}",
                            d.0, entries
                        );
                    }
                }
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(HashSet::from([idx]));
                }
            }
        }

        refs_map.push((
            idx,
            TargetRefs {
                compile_time_refs: compile_refs,
                runtime_refs,
            },
        ));

        // Honor the entity directives
        for d in entity_directives {
            entity_links.add_directive(&d, all_defs.as_ref());
        }
        idx += 1;
    }

    entity_links.expand_out();

    fn update_from_entity_links(targerefs: &mut TargetRefs, entity_links: &EntityLinksMaps) {
        fn update_map(m: &mut HashSet<u64>, entity_links: &EntityLinksMaps) {
            for (k, v) in entity_links.add_link_map.iter() {
                if m.contains(k) {
                    m.extend(v.iter().copied());
                }
            }
        }
        update_map(&mut targerefs.compile_time_refs, entity_links);
        update_map(&mut targerefs.runtime_refs, entity_links);
    }

    let mut compile_edges: HashMap<usize, HashSet<usize>> = HashMap::default();
    let mut runtime_edges: HashMap<usize, HashSet<usize>> = HashMap::default();
    for (e, _) in reverse_map.iter() {
        compile_edges.insert(*e, HashSet::default());
    }

    for (node, mut target_refs) in refs_map.into_iter() {
        update_from_entity_links(&mut target_refs, &entity_links);
        {
            let v = compile_edges.get_mut(&node).unwrap();

            for ele in target_refs.compile_time_refs.iter() {
                match owns_map.get(ele) {
                    None => panic!("Unable to find owns_map map stuff {}", ele),
                    Some(owner) => {
                        v.extend(owner.iter());
                    }
                }
            }
            v.remove(&node);
        }

        if !target_refs.runtime_refs.is_empty() {
            let v = runtime_edges.entry(node).or_default();
            for ele in target_refs.runtime_refs.iter() {
                match owns_map.get(ele) {
                    None => panic!("Unable to find owns_map map stuff {}", ele),
                    Some(owner) => {
                        v.extend(owner.iter());
                    }
                }
            }
            v.remove(&node);
        }
    }

    // Initial node counter is the number of nodes in the edges graph before we remove any.
    let node_counter = compile_edges.len();
    Ok(GraphState {
        forward_map,
        reverse_map,
        compile_edges,
        runtime_edges,
        def_to_id: all_defs,
        owns_map,
        node_counter,
        consumed_nodes: HashMap::default(),
        node_to_defs_cache: Default::default(),
        compile_in: Default::default(),
        runtime_in: Default::default(),
    })
}

pub async fn build_graph(
    _opt: &'static Opt,
    extract: &'static BuildGraphArgs,
    project_conf: &'static ProjectConf,
    concurrent_io_operations: &'static Semaphore,
) -> Result<()> {
    let st = Instant::now();
    let extracted_mappings: ExtractedMappings = read_json_file(&extract.extracted_mappings)?;
    let path_to_defs: PathToDefs = read_json_file(&extract.extracted_defs)?;
    let mut load_i = Vec::with_capacity(path_to_defs.relative_path_to_defs.len());

    for (_, p) in path_to_defs.relative_path_to_defs.iter() {
        let pb = PathBuf::from(p);
        load_i.push(tokio::spawn(async move {
            let c = concurrent_io_operations.acquire().await?;
            let r = async_read_json_file::<DefsData>(&pb).await;
            drop(c);
            r
        }));
    }

    let mut all_defs: HashMap<Arc<String>, u64> = HashMap::with_capacity(400000);
    let mut ref_idx: u64 = 0;

    for li in load_i {
        let li = li.await??;
        for p in li.defs.into_iter() {
            all_defs.entry(Arc::new(p)).or_insert(ref_idx);
            ref_idx += 1;
        }
    }

    let all_defs = Arc::new(all_defs);

    let mut configured_entity_directives: Vec<directive::EntityDirectiveConfig> = Vec::default();

    let mut circular_allow_list: Vec<String> = vec![];
    for (_k, v) in project_conf.configurations.iter() {        
        circular_allow_list.extend(v.circular_dependency_allow_list.iter().cloned());
    }
    for directives in project_conf.path_directives.iter() {
        match directives.directives().as_ref() {
            Ok(parsed_directives) => {
                for d in parsed_directives {
                    match d {
                        Directive::BinaryRef(_) => (),      // handled elsewhere
                        Directive::SrcDirective(_) => (),   // handled elsewhere
                        Directive::ManualRef(_) => (),      // handled elsewhere
                        Directive::AttrStringList(_) => (), // handled elsewhere
                        Directive::EntityDirective(ed) => {
                            configured_entity_directives.push(ed.clone())
                        }
                    }
                }
            }
            Err(e) => return Err(anyhow!("{:#?}", e)),
        }
    }

    info!("Prelim load complete {:?}", st.elapsed());

    let mut graph = load_initial_graph(
        &extracted_mappings,
        &configured_entity_directives,
        all_defs,
        concurrent_io_operations,
    )
    .await?;

    info!(
        "Graph initial state loaded after {:?} , have {} nodes initially",
        st.elapsed(),
        graph.compile_edges.len()
    );
    let st = Instant::now();
    graph.collapse(&circular_allow_list)?;
    info!(
        "Graph iteration complete after {:?}, have {} nodes after processing",
        st.elapsed(),
        graph.compile_edges.len()
    );

    let mut build_mapping: HashMap<String, GraphNode> = HashMap::default();

    for (node, outbound_compile_edges) in graph.compile_edges.iter() {
        let node_state = graph.reverse_map.get(node).unwrap();

        let mut child_nodes = Vec::default();
        let mut to_visit = vec![*node];
        while let Some(v) = to_visit.pop() {
            if let Some(consumed) = graph.consumed_nodes.get(&v) {
                for c in consumed.iter() {
                    child_nodes.push(*c);
                    to_visit.push(*c);
                }
            }
        }
        let runtime_refs = graph.runtime_edges.get(node);
        let k_name = graph.reverse_map.get(node).map(|e| e.name.clone()).unwrap();

        let output_node = build_mapping
            .entry(k_name.as_ref().clone())
            .or_insert_with(|| GraphNode::default());

        for child_node in child_nodes.into_iter() {
            let node_state = graph
                .reverse_map
                .get(&child_node)
                .expect("Graph invalid if missing");
            if node_state.node_type == NodeType::RealNode {
                output_node
                    .child_nodes
                    .insert(node_state.name.as_ref().clone(), node_state.as_ref().into());
            }
        }

        for outbound_edge in outbound_compile_edges.iter() {
            if let Some(t) = graph.get_node_label(outbound_edge) {
                output_node.dependencies.push(t.to_owned());
            } else {
                panic!(
                    "Unable to find outbound edge {} in the reverse map",
                    outbound_edge
                );
            }
        }
        output_node.dependencies.sort();

        for outbound_runtime_edge in runtime_refs.as_ref().into_iter().flat_map(|e| e.iter()) {
            if let Some(t) = graph.get_node_label(outbound_runtime_edge) {
                output_node.runtime_dependencies.push(t.to_owned());
            } else {
                panic!(
                    "Unable to find outbound edge {} in the reverse map",
                    outbound_runtime_edge
                );
            }
        }
        output_node.node_label = k_name.to_string();
        output_node.node_type = node_state.node_type;
        output_node.node_metadata = node_state.as_ref().into();
        output_node.runtime_dependencies.sort();
    }

    let out = GraphMapping { build_mapping };
    write_json_file(extract.graph_out.as_path(), &out)?;

    Ok(())
}

#[cfg(test)]
#[path = "build_graph_reference_test.rs"]
mod reference;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_graph() {
        let mut graph = GraphState::default();

        let foo_bar_baz = graph.add_node("com/foo/bar/baz".to_string(), NodeType::RealNode);
        let foo_bar_baz_boot =
            graph.add_node("com/foo/bar/baz/boot".to_string(), NodeType::RealNode);
        let foo_bar_ba3 = graph.add_node("com/foo/bar/ba3".to_string(), NodeType::RealNode);
        graph.add_compile_edge(foo_bar_baz, foo_bar_baz_boot);
        graph.add_compile_edge(foo_bar_baz_boot, foo_bar_baz);
        graph.add_compile_edge(foo_bar_baz_boot, foo_bar_ba3);

        graph
            .collapse(&vec!["com/foo/".to_string()])
            .expect("Should be able to collapse the graph");

        assert_eq!(graph.node_count(), 2);

        assert!(graph.node_exists(foo_bar_baz));
        assert!(!graph.node_exists(foo_bar_baz_boot));
        // node untouched, so still present
        assert!(graph.node_exists(foo_bar_ba3));

        assert!(graph.consumed_nodes.contains_key(&foo_bar_baz));
        let nodes_consumed = graph.consumed_nodes.get(&foo_bar_baz).unwrap();
        assert!(nodes_consumed.contains(&foo_bar_baz_boot));

        let mut hs = HashSet::default();
        hs.insert(foo_bar_ba3);

        assert_eq!(graph.get_compile_edges(foo_bar_baz), Some(&hs));
    }

    #[test]
    fn test_simple_graph_runtime_edges() {
        let mut graph = GraphState::default();

        let foo_bar_baz = graph.add_node("com/foo/bar/baz".to_string(), NodeType::RealNode);
        let foo_bar_baz_boot =
            graph.add_node("com/foo/bar/baz/boot".to_string(), NodeType::RealNode);
        let foo_bar_ba3 = graph.add_node("com/foo/bar/ba3".to_string(), NodeType::RealNode);
        graph.add_compile_edge(foo_bar_baz, foo_bar_baz_boot);
        graph.add_runtime_edge(foo_bar_baz_boot, foo_bar_baz);
        graph.add_compile_edge(foo_bar_baz_boot, foo_bar_ba3);

        graph
            .collapse(&vec!["com/foo/".to_string()])
            .expect("Should be able to collapse the graph");

        assert_eq!(graph.node_count(), 2);

        assert!(graph.node_exists(foo_bar_baz));
        assert!(!graph.node_exists(foo_bar_baz_boot));
        // node untouched, so still present
        assert!(graph.node_exists(foo_bar_ba3));

        assert!(graph.consumed_nodes.contains_key(&foo_bar_baz));
        let nodes_consumed = graph.consumed_nodes.get(&foo_bar_baz).unwrap();
        assert!(nodes_consumed.contains(&foo_bar_baz_boot));

        let mut hs = HashSet::default();
        hs.insert(foo_bar_ba3);

        assert_eq!(graph.get_compile_edges(foo_bar_baz), Some(&hs));
    }

    #[test]
    fn test_simple_graph_collapse() {
        let mut graph = GraphState::default();

        let foo_bar_baz = graph.add_node("com/foo/bar/baz".to_string(), NodeType::RealNode);
        let foo_bar_ba2 = graph.add_node("com/foo/bar/ba2".to_string(), NodeType::RealNode);
        let foo_bar_ba3 = graph.add_node("com/foo/bar/ba3".to_string(), NodeType::RealNode);
        graph.add_compile_edge(foo_bar_baz, foo_bar_ba2);
        graph.add_compile_edge(foo_bar_ba2, foo_bar_baz);
        graph.add_compile_edge(foo_bar_ba2, foo_bar_ba3);

        graph
            .collapse(&vec!["com/foo/".to_string()])
            .expect("Should be able to collapse the graph");

        assert_eq!(graph.node_count(), 2);

        assert!(!graph.node_exists(foo_bar_baz));
        assert!(!graph.node_exists(foo_bar_ba2));
        assert!(graph.node_exists(foo_bar_ba3));

        assert_eq!(graph.consumed_nodes.len(), 1);

        let (consumed_into, consumed_nodes) = graph
            .consumed_nodes
            .iter()
            .map(|(a, b)| (*a, b.clone()))
            .next()
            .unwrap();

        // consumed into must generate a new node, so it cannot be any of these
        assert!(consumed_into != foo_bar_baz);
        assert!(consumed_into != foo_bar_ba2);
        assert!(consumed_into != foo_bar_ba3);

        assert!(consumed_nodes.contains(&foo_bar_baz));
        assert!(consumed_nodes.contains(&foo_bar_ba2));
        assert!(!consumed_nodes.contains(&foo_bar_ba3));
    }

    #[test]
    fn test_simple_graph_collapse_fail() {
        let mut graph = GraphState::default();

        let foo_bar_baz = graph.add_node("com/foo/bar/baz".to_string(), NodeType::RealNode);
        let foo_bar_ba2 = graph.add_node("com/foo/bar/ba2".to_string(), NodeType::RealNode);
        let foo_bar_ba3 = graph.add_node("com/foo/bar/ba3".to_string(), NodeType::RealNode);
        graph.add_compile_edge(foo_bar_baz, foo_bar_ba2);
        graph.add_compile_edge(foo_bar_ba2, foo_bar_baz);
        graph.add_compile_edge(foo_bar_ba2, foo_bar_ba3);

        let e = graph
            .collapse(&vec![])
            .expect_err("Should be fail to collapse the graph without kitchen sink prefixes");
        assert!(e.to_string().contains("com/foo/bar/baz, com/foo/bar/ba2"))
    }

    #[test]
    fn test_simple_graph_no_collapse() {
        let mut graph = GraphState::default();

        let foo_bar_baz = graph.add_node("com/foo/bar/baz".to_string(), NodeType::RealNode);
        let foo_bar_ba2 = graph.add_node("com/foo/bar/ba2".to_string(), NodeType::RealNode);
        let foo_bar_ba3 = graph.add_node("com/foo/bar/ba3".to_string(), NodeType::RealNode);
        graph.add_compile_edge(foo_bar_baz, foo_bar_ba2);
        graph.add_compile_edge(foo_bar_ba2, foo_bar_ba3);

        graph
            .collapse(&vec![])
            .expect("Should be able to collapse the graph");

        assert_eq!(graph.node_count(), 3);

        assert!(graph.node_exists(foo_bar_baz));
        assert!(graph.node_exists(foo_bar_ba2));

        assert!(graph.consumed_nodes.is_empty());
    }

    // ---- helpers for the cycle collapsing tests below ----

    fn real(graph: &mut GraphState, label: &str) -> usize {
        graph.add_node(label.to_string(), NodeType::RealNode)
    }

    fn id_of(graph: &GraphState, label: &str) -> usize {
        *graph
            .forward_map
            .get(&label.to_string())
            .unwrap_or_else(|| panic!("no node {}", label))
    }

    fn labels_of<'a>(graph: &GraphState, ids: impl IntoIterator<Item = &'a usize>) -> Vec<String> {
        let mut v: Vec<String> = ids
            .into_iter()
            .map(|i| graph.get_node_label(i).unwrap().to_string())
            .collect();
        v.sort();
        v
    }

    fn compile_labels(graph: &GraphState, label: &str) -> Vec<String> {
        labels_of(graph, &graph.compile_edges[&id_of(graph, label)])
    }

    fn live_labels(graph: &GraphState) -> Vec<String> {
        labels_of(graph, graph.compile_edges.keys())
    }

    /// (compile edges, runtime edges, consumed real nodes) by label, for every live node.
    type Snapshot = std::collections::BTreeMap<String, (Vec<String>, Vec<String>, Vec<String>)>;

    fn snapshot(graph: &GraphState) -> Snapshot {
        graph
            .compile_edges
            .iter()
            .map(|(id, compile)| {
                (
                    graph.get_node_label(id).unwrap().to_string(),
                    (
                        labels_of(graph, compile),
                        labels_of(graph, graph.runtime_edges.get(id).into_iter().flatten()),
                        // Synthetic nodes are never emitted, and the original algorithm creates
                        // (hash-order dependent) intermediate ones that get absorbed again.
                        labels_of(
                            graph,
                            graph
                                .consumed_nodes
                                .get(id)
                                .into_iter()
                                .flatten()
                                .filter(|c| {
                                    graph.get_metadata(c).map(|m| m.node_type)
                                        == Some(NodeType::RealNode)
                                }),
                        ),
                    ),
                )
            })
            .collect()
    }

    /// No live node may also be consumed by another, and no node may be consumed twice.
    fn is_consistent(graph: &GraphState) -> bool {
        let mut seen: HashSet<usize> = HashSet::default();
        graph
            .consumed_nodes
            .values()
            .flatten()
            .all(|c| !graph.compile_edges.contains_key(c) && seen.insert(*c))
    }

    fn allow_all() -> Vec<String> {
        vec![String::new()]
    }

    // ---- cycle collapsing ----

    #[test]
    fn test_independent_cycles_collapse_together() {
        let mut graph = GraphState::default();
        let ax = real(&mut graph, "com/a/x");
        let ay = real(&mut graph, "com/a/y");
        let bp = real(&mut graph, "com/b/p");
        let bq = real(&mut graph, "com/b/q");
        let cz = real(&mut graph, "com/c/z");
        graph.add_compile_edge(ax, ay);
        graph.add_compile_edge(ay, ax);
        graph.add_compile_edge(bp, bq);
        graph.add_compile_edge(bq, bp);
        graph.add_compile_edge(ax, cz);
        graph.add_compile_edge(bq, cz);
        graph.add_compile_edge(ay, bp);

        graph.collapse(&allow_all()).unwrap();

        assert_eq!(live_labels(&graph), vec!["com/a", "com/b", "com/c/z"]);
        assert_eq!(compile_labels(&graph, "com/a"), vec!["com/b", "com/c/z"]);
        assert_eq!(compile_labels(&graph, "com/b"), vec!["com/c/z"]);
        let a = id_of(&graph, "com/a");
        assert_eq!(labels_of(&graph, &graph.consumed_nodes[&a]), vec!["com/a/x", "com/a/y"]);
    }

    #[test]
    fn test_three_node_cycle_across_dirs_uses_common_ancestor() {
        let mut graph = GraphState::default();
        let x = real(&mut graph, "com/a/x");
        let y = real(&mut graph, "com/a/y");
        let z = real(&mut graph, "com/b/z");
        let w = real(&mut graph, "com/d/w");
        graph.add_compile_edge(x, y);
        graph.add_compile_edge(y, z);
        graph.add_runtime_edge(z, x);
        // Inbound edge from outside must be redirected to the merged node.
        graph.add_compile_edge(w, x);

        graph.collapse(&allow_all()).unwrap();

        assert_eq!(live_labels(&graph), vec!["com", "com/d/w"]);
        assert_eq!(compile_labels(&graph, "com/d/w"), vec!["com"]);
        let com = id_of(&graph, "com");
        assert_eq!(
            graph.get_metadata(&com).map(|m| m.node_type),
            Some(NodeType::Synthetic)
        );
        assert_eq!(
            labels_of(&graph, &graph.consumed_nodes[&com]),
            vec!["com/a/x", "com/a/y", "com/b/z"]
        );
    }

    #[test]
    fn test_collapse_sweeps_live_nodes_on_the_path_into_the_merged_node() {
        let mut graph = GraphState::default();
        let x1 = real(&mut graph, "com/a/x/1");
        let y1 = real(&mut graph, "com/a/y/1");
        // A live node that is a parent directory of a consumed node.
        let x = real(&mut graph, "com/a/x");
        let ext = real(&mut graph, "com/e/ext");
        graph.add_compile_edge(x1, y1);
        graph.add_compile_edge(y1, x1);
        graph.add_compile_edge(x, ext);

        graph.collapse(&allow_all()).unwrap();

        assert_eq!(live_labels(&graph), vec!["com/a", "com/e/ext"]);
        assert_eq!(compile_labels(&graph, "com/a"), vec!["com/e/ext"]);
        let a = id_of(&graph, "com/a");
        assert_eq!(
            labels_of(&graph, &graph.consumed_nodes[&a]),
            vec!["com/a/x", "com/a/x/1", "com/a/y/1"]
        );
    }

    #[test]
    fn test_merging_creates_a_new_cycle_that_is_collapsed_too() {
        // x <-> y, and y -> z -> parent-dir node which depends back on x: after x,y are merged
        // the merged node, z and the parent form a new cycle.
        let mut graph = GraphState::default();
        let x = real(&mut graph, "com/a/x");
        let y = real(&mut graph, "com/a/y");
        let z = real(&mut graph, "com/b/z");
        graph.add_compile_edge(x, y);
        graph.add_compile_edge(y, x);
        graph.add_compile_edge(y, z);
        graph.add_compile_edge(z, y);

        graph.collapse(&allow_all()).unwrap();

        assert_eq!(live_labels(&graph), vec!["com"]);
        assert!(graph.compile_edges[&id_of(&graph, "com")].is_empty());
    }

    #[test]
    fn test_self_edge_is_not_a_cycle() {
        let mut graph = GraphState::default();
        let a = real(&mut graph, "com/a");
        let b = real(&mut graph, "com/b");
        graph.add_compile_edge(a, a);
        graph.add_compile_edge(a, b);

        graph.collapse(&vec![]).unwrap();

        assert_eq!(live_labels(&graph), vec!["com/a", "com/b"]);
        assert!(graph.consumed_nodes.is_empty());
    }

    #[test]
    fn test_cycle_outside_allow_list_is_an_error_listing_the_cycle() {
        let mut graph = GraphState::default();
        let a = real(&mut graph, "com/a/x");
        let b = real(&mut graph, "com/a/y");
        let c = real(&mut graph, "com/a/downstream");
        graph.add_compile_edge(a, b);
        graph.add_compile_edge(b, a);
        graph.add_compile_edge(b, c);

        let err = graph
            .collapse(&vec!["com/other/".to_string()])
            .expect_err("cycle is not allow-listed");
        let msg = err.to_string();
        assert!(msg.contains("com/a/x, com/a/y"), "{}", msg);
        // Only the cycle is listed, not nodes merely reachable from it.
        assert!(!msg.contains("downstream"), "{}", msg);
    }

    #[test]
    fn test_cycle_partly_outside_allow_list_is_an_error() {
        let mut graph = GraphState::default();
        let a = real(&mut graph, "com/ok/x");
        let b = real(&mut graph, "com/bad/y");
        graph.add_compile_edge(a, b);
        graph.add_compile_edge(b, a);

        graph
            .collapse(&vec!["com/ok/".to_string()])
            .expect_err("com/bad/y is not allow-listed");
    }

    #[test]
    fn test_no_common_prefix_is_an_error() {
        let mut graph = GraphState::default();
        let a = real(&mut graph, "left/x");
        let b = real(&mut graph, "right/y");
        graph.add_compile_edge(a, b);
        graph.add_compile_edge(b, a);

        let err = graph.collapse(&allow_all()).expect_err("no shared parent");
        assert!(err.to_string().contains("cannot find a parent dir"));
    }

    #[test]
    fn test_deep_acyclic_chain_is_left_alone() {
        // Deep enough that a recursive traversal would overflow the stack.
        const N: usize = 200_000;
        let mut graph = GraphState::default();
        let ids: Vec<usize> = (0..N).map(|i| real(&mut graph, &format!("com/p/n{}", i))).collect();
        for w in ids.windows(2) {
            graph.add_compile_edge(w[0], w[1]);
        }

        graph.collapse(&vec![]).unwrap();

        assert_eq!(graph.node_count(), N);
        assert!(graph.consumed_nodes.is_empty());
    }

    #[test]
    fn test_large_ring_collapses_to_one_node() {
        const N: usize = 5_000;
        let mut graph = GraphState::default();
        let ids: Vec<usize> = (0..N).map(|i| real(&mut graph, &format!("com/pkg/n{}", i))).collect();
        for i in 0..N {
            graph.add_compile_edge(ids[i], ids[(i + 1) % N]);
        }

        graph.collapse(&allow_all()).unwrap();

        assert_eq!(live_labels(&graph), vec!["com/pkg"]);
        let pkg = id_of(&graph, "com/pkg");
        assert_eq!(graph.consumed_nodes[&pkg].len(), N);
    }

    #[test]
    fn test_many_small_cycles_in_a_chain() {
        // d_i/a <-> d_i/b, and each pair depends on the next pair.
        const N: usize = 3_000;
        let mut graph = GraphState::default();
        for i in 0..N {
            let a = real(&mut graph, &format!("com/d{}/a", i));
            let b = real(&mut graph, &format!("com/d{}/b", i));
            graph.add_compile_edge(a, b);
            graph.add_compile_edge(b, a);
            if i > 0 {
                let prev = id_of(&graph, &format!("com/d{}/a", i - 1));
                graph.add_compile_edge(prev, a);
            }
        }

        graph.collapse(&allow_all()).unwrap();

        assert_eq!(graph.node_count(), N);
        assert_eq!(compile_labels(&graph, "com/d0"), vec!["com/d1"]);
        assert!(compile_labels(&graph, &format!("com/d{}", N - 1)).is_empty());
    }

    // ---- differential tests against the original implementation ----

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    struct Spec {
        labels: Vec<String>,
        compile: Vec<(usize, usize)>,
        runtime: Vec<(usize, usize)>,
    }

    fn random_spec(seed: u64, max_nodes: usize, label_pool: &[String]) -> Spec {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut pool: Vec<String> = label_pool.to_vec();
        let n = 2 + rng.below(max_nodes.min(pool.len()) - 1);
        let mut labels = Vec::new();
        for _ in 0..n {
            labels.push(pool.swap_remove(rng.below(pool.len())));
        }
        // Edge density varies per case so that we see everything from forests to dense SCCs.
        let density = 1 + rng.below(4);
        let mut compile = Vec::new();
        let mut runtime = Vec::new();
        for from in 0..n {
            for to in 0..n {
                if from != to && rng.below(n * 2) < density {
                    if rng.below(5) == 0 {
                        runtime.push((from, to));
                    } else {
                        compile.push((from, to));
                    }
                }
            }
        }
        Spec { labels, compile, runtime }
    }

    fn build(spec: &Spec) -> GraphState {
        let mut graph = GraphState::default();
        for l in spec.labels.iter() {
            real(&mut graph, l);
        }
        for (a, b) in spec.compile.iter() {
            graph.add_compile_edge(*a, *b);
        }
        for (a, b) in spec.runtime.iter() {
            graph.add_runtime_edge(*a, *b);
        }
        graph
    }

    /// `com`, `com/{a,b,c}`, `com/{a,b,c}/{x,y}`, and `com/{a,b,c}/{x,y}/{p,q}`: small enough
    /// that labels collide as parent directories of one another, which exercises the
    /// common ancestor logic and the sweeping of live nodes on the path to a merged node.
    fn nested_label_pool() -> Vec<String> {
        let mut v = vec!["com".to_string()];
        for a in ["a", "b", "c"] {
            v.push(format!("com/{}", a));
            for x in ["x", "y"] {
                v.push(format!("com/{}/{}", a, x));
                for p in ["p", "q"] {
                    v.push(format!("com/{}/{}/{}", a, x, p));
                }
            }
        }
        v
    }

    #[test]
    fn test_matches_reference_implementation_on_random_graphs() {
        let pool = nested_label_pool();
        let mut cyclic_cases = 0;
        let mut reference_inconsistent = 0;
        for seed in 0..3_000u64 {
            let spec = random_spec(seed, 14, &pool);

            let mut expected_graph = build(&spec);
            let mut actual_graph = build(&spec);
            let expected = expected_graph.ref_collapse(&allow_all());
            let actual = actual_graph.collapse(&allow_all());

            assert_eq!(
                expected.is_ok(),
                actual.is_ok(),
                "seed {} disagreed on failure: expected {:?}, got {:?}",
                seed,
                expected.err(),
                actual.err()
            );
            if expected.is_ok() {
                assert!(is_consistent(&actual_graph), "seed {} is inconsistent", seed);
                // The original can resurrect a node it already consumed (it iterates a stale
                // snapshot in `common_ancestor`), leaving it both live and consumed. That is a
                // bug rather than behaviour to preserve, so only compare sane results.
                // It can also stop with a cycle left behind: its `no_loops` memo goes stale after
                // merges create new cycles through nodes already checked.
                if !is_consistent(&expected_graph) || !expected_graph.cyclic_sccs().is_empty() {
                    reference_inconsistent += 1;
                    continue;
                }
                assert_eq!(
                    snapshot(&actual_graph),
                    snapshot(&expected_graph),
                    "seed {} produced a different graph",
                    seed
                );
                if !expected_graph.consumed_nodes.is_empty() {
                    cyclic_cases += 1;
                }
            }
        }
        // Make sure the generator is actually producing cycles to collapse.
        assert!(cyclic_cases > 500, "only {} cyclic cases", cyclic_cases);
        eprintln!(
            "{} cyclic cases matched, {} skipped as the reference was inconsistent or cyclic",
            cyclic_cases, reference_inconsistent
        );
    }

    #[test]
    fn test_collapse_is_deterministic() {
        let pool = nested_label_pool();
        for seed in 0..300u64 {
            let spec = random_spec(seed, 14, &pool);
            let mut first = build(&spec);
            let mut second = build(&spec);
            let r1 = first.collapse(&allow_all());
            let r2 = second.collapse(&allow_all());
            assert_eq!(r1.is_ok(), r2.is_ok());
            if r1.is_ok() {
                assert_eq!(snapshot(&first), snapshot(&second), "seed {}", seed);
            }
        }
    }

    #[test]
    fn test_collapsed_graph_is_acyclic() {
        let pool = nested_label_pool();
        for seed in 0..1_000u64 {
            let spec = random_spec(seed, 21, &pool);
            let mut graph = build(&spec);
            if graph.collapse(&allow_all()).is_ok() {
                assert!(graph.cyclic_sccs().is_empty(), "seed {} left a cycle", seed);
                assert!(is_consistent(&graph), "seed {} is inconsistent", seed);
            }
        }
    }
}
