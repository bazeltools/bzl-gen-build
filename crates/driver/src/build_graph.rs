use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

use crate::{async_read_json_file, read_json_file, write_json_file, BuildGraphArgs, Opt};

use anyhow::{anyhow, Result};
use bzl_gen_build_shared_types::{
    directive::{BinaryRefAndPath, BinaryRefConfig, EntityDirectiveConfig, ManualRefConfig},
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
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, Eq)]
pub struct GraphNodeMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binary_refs: Vec<BinaryRefAndPath>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_refs: Vec<ManualRefConfig>,
}
impl GraphNodeMetadata {
    pub fn is_empty(&self) -> bool {
        self.binary_refs.is_empty() && self.manual_refs.is_empty()
    }
}

impl<'a> From<&'a NodeExternalState> for GraphNodeMetadata {
    fn from(nes: &'a NodeExternalState) -> Self {
        Self {
            binary_refs: nes.binary_refs.clone(),
            manual_refs: nes.manual_refs.clone(),
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
}
impl NodeExternalState {
    pub fn empty(name: Arc<String>, node_type: NodeType) -> Self {
        Self {
            name,
            node_type,
            binary_refs: Default::default(),
            manual_refs: Default::default(),
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
                        "We cannot find a parent dir for {},{:#?}",
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
        let consumed_nodes = self.consumed_nodes.clone();
        for (k, values) in consumed_nodes.iter() {
            let lst: Vec<&str> = values
                .iter()
                .flat_map(|e| self.get_node_label(e).into_iter())
                .collect();
            let ky = &self.get_node_label(k).unwrap();

            let mut entries: HashSet<String> = HashSet::default();

            for ele in lst.iter() {
                match ele.strip_prefix(ky) {
                    None => return Err(anyhow!("Element in equiv set doesn't share the same prefix: target: {} , parent: {}", ele, ky)),
                    Some(remainder) => {
                        let mut previous: Option<String> = None;
                        for next_section in remainder.split('/').filter(|e| !e.is_empty()) {
                            let l = if let Some(p) = previous {
                                format!("{}/{}", p, next_section)
                            } else {
                                format!("{}/{}", ky, next_section)
                            };
                                entries.insert(l.clone());
                                previous = Some(l);
                            }
                        }
                    }
            }

            let to_collapse: Vec<usize> = entries
                .iter()
                .filter_map(|ele| {
                    let al = Arc::new((*ele).clone());

                    if let Some(id) = self.forward_map.get(&al) {
                        let id = *id;
                        if self.node_is_live(id) {
                            Some(id)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            if !to_collapse.is_empty() {
                self.merge_node(*k, &to_collapse)?;
            }
        }
        Ok(())
    }

    fn merge_node<'a, T>(&mut self, destination: usize, src: T) -> Result<()>
    where
        T: IntoIterator<Item = &'a usize> + Copy,
    {
        fn consume_edges<'a, T>(
            m: &mut HashMap<usize, HashSet<usize>>,
            destination: usize,
            error_on_missing: bool,
            src: T,
        ) -> Result<()>
        where
            T: IntoIterator<Item = &'a usize> + Copy,
        {
            let mut parent = m.remove(&destination).unwrap_or_default();

            for e in src.into_iter() {
                if destination != *e {
                    if let Some(old) = m.remove(e) {
                        parent.extend(old.into_iter());
                    } else if error_on_missing {
                        return Err(anyhow!(
                            "Tried to squash a node {} into another node, but it didn't exist?",
                            e
                        ));
                    }
                }
            }
            parent.remove(&destination);

            m.insert(destination, parent);
            for (_k, v) in m.iter_mut() {
                for e in src.into_iter() {
                    if destination != *e && v.remove(e) {
                        v.insert(destination);
                    }
                }
                v.remove(_k);
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

        consume_edges(&mut self.compile_edges, destination, true, src)?;
        consume_edges(&mut self.runtime_edges, destination, false, src)?;

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

    fn collapse_loop(&mut self, element: usize) -> Result<Option<usize>> {
        let mut reverse_steps: HashMap<usize, usize> = HashMap::default();
        let mut to_visit: Vec<usize> = vec![element];

        'outer_loop: while let Some(v) = to_visit.pop() {
            for p in self.get_all_outbound_nodes(v) {
                let previously_present = reverse_steps.insert(p, v);
                if p == element {
                    break 'outer_loop;
                } else if previously_present.is_none() {
                    to_visit.push(p);
                }
            }
        }
        let mut to_collapse: HashSet<usize> = HashSet::default();
        to_collapse.insert(element);

        let mut current = reverse_steps.get(&element);

        while let Some(nxt) = current {
            if *nxt == element || to_collapse.contains(nxt) {
                break;
            } else {
                to_collapse.insert(*nxt);
                current = reverse_steps.get(nxt);
            }
        }
        if to_collapse.len() > 1 {
            let target = self.find_or_create_common_ancestor(&to_collapse)?;
            self.merge_node(target, &to_collapse)?;
            Ok(Some(target))
        } else {
            Ok(None)
        }
    }

    fn in_cycle(&self, node: usize) -> bool {
        let mut to_visit = vec![node];
        let mut inner_visited: HashSet<usize> = HashSet::default();

        while let Some(nxt) = to_visit.pop() {
            if inner_visited.contains(&nxt) {
                continue;
            }
            inner_visited.insert(nxt);
            for outbound_edge in self.get_all_outbound_nodes(nxt) {
                to_visit.push(outbound_edge);
                if outbound_edge == node {
                    return true;
                }
            }
        }
        false
    }

    pub fn collapse(&mut self) -> Result<()> {
        let mut no_loops: HashSet<usize> = HashSet::default();
        loop {
            let mut incompress = None;
            'outer_loop: for (node, _) in self
                .compile_edges
                .iter()
                .chain(self.runtime_edges.iter())
                .filter(|(_k, v)| !v.is_empty())
            {
                if no_loops.contains(node) {
                    continue;
                }
                if self.in_cycle(*node) {
                    incompress = Some(*node);
                    break 'outer_loop;
                } else {
                    no_loops.insert(*node);
                }
            }
            if let Some(i) = incompress {
                if let Some(target) = self.collapse_loop(i)? {
                    // Since we can merge to a 3rd node not in the dependency graph, that node needs to be re-added to the
                    // to visit graph, since it may have been eliminated before.
                    no_loops.remove(&target);
                };
                self.common_ancestor()?;
            } else {
                return Ok(());
            }
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
            ),
        ) = li.await??;

        let m = Arc::new(k);

        forward_map.insert(m.clone(), idx);
        let node_external_state = NodeExternalState {
            name: m.clone(),
            node_type: NodeType::RealNode,
            binary_refs: binary_ref_directives,
            manual_refs: manual_ref_directives,
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
            all_defs.entry(Arc::new(p.clone())).or_insert(ref_idx);
            ref_idx += 1;
        }
    }

    let all_defs = Arc::new(all_defs);

    let mut configured_entity_directives: Vec<directive::EntityDirectiveConfig> = Vec::default();

    for directives in project_conf.path_directives.iter() {
        match directives.directives().as_ref() {
            Ok(parsed_directives) => {
                for d in parsed_directives {
                    match d {
                        Directive::BinaryRef(_) => (),    // handled elsewhere
                        Directive::SrcDirective(_) => (), // handled elsewhere
                        Directive::ManualRef(_) => (),    // handled elsewhere
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
        all_defs.clone(),
        concurrent_io_operations,
    )
    .await?;

    info!(
        "Graph initial state loaded after {:?} , have {} nodes initially",
        st.elapsed(),
        graph.compile_edges.len()
    );
    let st = Instant::now();
    graph.collapse()?;
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
                output_node.child_nodes.insert(
                    node_state.name.as_str().to_string(),
                    node_state.as_ref().into(),
                );
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
        output_node.node_type = node_state.node_type;
        output_node.node_metadata = node_state.as_ref().into();
        output_node.runtime_dependencies.sort();
    }

    let out = GraphMapping { build_mapping };
    write_json_file(extract.graph_out.as_path(), &out)?;

    Ok(())
}

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
            .collapse()
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
            .collapse()
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
            .collapse()
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
    fn test_simple_graph_no_collapse() {
        let mut graph = GraphState::default();

        let foo_bar_baz = graph.add_node("com/foo/bar/baz".to_string(), NodeType::RealNode);
        let foo_bar_ba2 = graph.add_node("com/foo/bar/ba2".to_string(), NodeType::RealNode);
        let foo_bar_ba3 = graph.add_node("com/foo/bar/ba3".to_string(), NodeType::RealNode);
        graph.add_compile_edge(foo_bar_baz, foo_bar_ba2);
        graph.add_compile_edge(foo_bar_ba2, foo_bar_ba3);

        graph
            .collapse()
            .expect("Should be able to collapse the graph");

        assert_eq!(graph.node_count(), 3);

        assert!(graph.node_exists(foo_bar_baz));
        assert!(graph.node_exists(foo_bar_ba2));

        assert!(graph.consumed_nodes.is_empty());
    }
}
