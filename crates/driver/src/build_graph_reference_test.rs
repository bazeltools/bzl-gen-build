//! Verbatim copy of the original (pre-Tarjan) cycle collapsing, kept only as a test oracle.
use super::*;

impl GraphState {
    pub fn ref_common_ancestor(&mut self) -> Result<()> {
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
                self.ref_merge_node(*k, &to_collapse)?;
            }
        }
        Ok(())
    }

    fn ref_merge_node<'a, T>(&mut self, destination: usize, src: T) -> Result<()>
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

    fn ref_collapse_loop(&mut self, element: usize) -> Result<Option<usize>> {
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
            self.ref_merge_node(target, &to_collapse)?;
            Ok(Some(target))
        } else {
            Ok(None)
        }
    }

    fn ref_in_cycle(&self, node: usize, circular_dependency_allow_list: &Vec<String>) -> Result<bool> {
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
                    if let Some(node_label) = self.get_node_label(&node) {
                        if !circular_dependency_allow_list
                            .iter()
                            .any(|p| node_label.starts_with(p))
                        {
                            let mut cycle_nodes: Vec<_> = inner_visited.iter().cloned().collect();
                            cycle_nodes.sort();

                            return Err(anyhow::anyhow!("Circular dependency found in the package {}.
  {}
  Resolve the cycle, or opt in to collapsing this into a higher-level build target by adding circular_dependency_allow_list in the module config JSON.
  Note that creating such aggregate target across multiple directories will slow down the build anytime you make source change in the area.",
                                node_label,
                                cycle_nodes.iter().map(|n| self.get_node_label(n).unwrap_or_default()).collect::<Vec<_>>().join(", ")
                            ));
                        }
                    }
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub fn ref_collapse(&mut self, circular_dependency_allow_list: &Vec<String>) -> Result<()> {
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
                if self.ref_in_cycle(*node, circular_dependency_allow_list)? {
                    incompress = Some(*node);
                    break 'outer_loop;
                } else {
                    no_loops.insert(*node);
                }
            }
            if let Some(i) = incompress {
                if let Some(target) = self.ref_collapse_loop(i)? {
                    // Since we can merge to a 3rd node not in the dependency graph, that node needs to be re-added to the
                    // to visit graph, since it may have been eliminated before.
                    no_loops.remove(&target);
                };
                self.ref_common_ancestor()?;
            } else {
                return Ok(());
            }
        }
    }
}
