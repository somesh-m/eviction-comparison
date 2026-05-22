use crate::Cache;
use hashbrown::HashMap;
use probabilistic_collections::count_min_sketch::{CountMinSketch, CountMinStrategy};
use std::collections::HashSet;
use tabular::{Row, Table};

#[derive(Debug, Clone)]
pub struct AlgorithmStats {
    pub algorithm_name: String,
    pub window_size: usize,
    pub probation_size: usize,
    pub protected_size: usize,
    pub total_element_count: usize,
    pub total_eviction_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NodeRef {
    idx: usize,
    location: Location,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Window,
    Probation,
    Protected,
}

#[derive(Debug, Clone)]
pub struct ValueMeta {
    pub key: String,
    pub value: String,
    prev: Option<usize>,
    next: Option<usize>,
    location: Location,
}

#[derive(Debug, Clone, Copy)]
struct SegmentState {
    head: Option<usize>,
    tail: Option<usize>,
    len: usize,
    max: usize,
}

impl SegmentState {
    fn new(max: usize) -> Self {
        Self {
            head: None,
            tail: None,
            len: 0,
            max,
        }
    }
}

pub struct WTinyLfu {
    index_map: HashMap<String, NodeRef>,

    pub window_max: usize,
    pub probation_max: usize,
    pub protected_max: usize,

    window: SegmentState,
    probation: SegmentState,
    protected: SegmentState,

    nodes: Vec<Option<ValueMeta>>,
    free_list: Vec<usize>,

    pub name: String,
    pub total_eviction_count: usize,
    pub error_tolerance: f64,
    pub confidence: f64,
    pub sketch: CountMinSketch<CountMinStrategy, String>,
}

impl WTinyLfu {
    pub fn new(total_capacity: usize) -> Self {
        let window_max = if total_capacity == 0 {
            0
        } else {
            std::cmp::max(1, (total_capacity as f64 * 0.10) as usize)
        };
        let remaining = total_capacity.saturating_sub(window_max);
        let probation_max = if remaining == 0 {
            0
        } else {
            std::cmp::max(1, (remaining as f64 * 0.20) as usize)
        };
        let protected_max = remaining - probation_max;

        let error_tolerance = 0.01;
        let confidence = 0.99;

        Self {
            index_map: HashMap::with_capacity(total_capacity),
            window_max,
            probation_max,
            protected_max,
            window: SegmentState::new(window_max),
            probation: SegmentState::new(probation_max),
            protected: SegmentState::new(protected_max),
            nodes: vec![None; total_capacity],
            free_list: (0..total_capacity).rev().collect(),
            name: "Original W-TinyLFU (Window LRU + Segmented Main LRU)".to_string(),
            total_eviction_count: 0,
            error_tolerance,
            confidence,
            sketch: CountMinSketch::<CountMinStrategy, String>::from_error(
                confidence,
                error_tolerance,
            ),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn update_freq_map(&mut self, key: &str) {
        self.sketch.insert(key, 1);
    }

    fn alloc_node(&mut self, key: String, value: String) -> usize {
        let idx = if let Some(idx) = self.free_list.pop() {
            idx
        } else {
            let idx = self.nodes.len();
            self.nodes.push(None);
            idx
        };

        self.nodes[idx] = Some(ValueMeta {
            key: key.clone(),
            value,
            prev: None,
            next: None,
            location: Location::Window,
        });
        self.index_map.insert(
            key,
            NodeRef {
                idx,
                location: Location::Window,
            },
        );
        idx
    }

    fn segment(&self, location: Location) -> &SegmentState {
        match location {
            Location::Window => &self.window,
            Location::Probation => &self.probation,
            Location::Protected => &self.protected,
        }
    }

    fn segment_mut(&mut self, location: Location) -> &mut SegmentState {
        match location {
            Location::Window => &mut self.window,
            Location::Probation => &mut self.probation,
            Location::Protected => &mut self.protected,
        }
    }

    fn key_for_idx(&self, idx: usize) -> &str {
        self.nodes[idx].as_ref().unwrap().key.as_str()
    }

    fn value_for_idx(&self, idx: usize) -> &str {
        self.nodes[idx].as_ref().unwrap().value.as_str()
    }

    fn location_for_idx(&self, idx: usize) -> Location {
        self.nodes[idx].as_ref().unwrap().location
    }

    fn detach(&mut self, idx: usize) {
        let (prev_idx, next_idx, location) = {
            let node = self.nodes[idx].as_ref().unwrap();
            (node.prev, node.next, node.location)
        };

        if let Some(prev_idx) = prev_idx {
            if let Some(prev_node) = &mut self.nodes[prev_idx] {
                prev_node.next = next_idx;
            }
        } else {
            self.segment_mut(location).head = next_idx;
        }

        if let Some(next_idx) = next_idx {
            if let Some(next_node) = &mut self.nodes[next_idx] {
                next_node.prev = prev_idx;
            }
        } else {
            self.segment_mut(location).tail = prev_idx;
        }

        if let Some(node) = &mut self.nodes[idx] {
            node.prev = None;
            node.next = None;
        }

        self.segment_mut(location).len -= 1;
    }

    fn push_back(&mut self, idx: usize, location: Location) {
        let old_tail = self.segment(location).tail;

        if let Some(node) = &mut self.nodes[idx] {
            node.location = location;
            node.prev = old_tail;
            node.next = None;
        }

        if let Some(old_tail) = old_tail {
            if let Some(tail_node) = &mut self.nodes[old_tail] {
                tail_node.next = Some(idx);
            }
        } else {
            self.segment_mut(location).head = Some(idx);
        }

        self.segment_mut(location).tail = Some(idx);
        self.segment_mut(location).len += 1;

        let key = self.key_for_idx(idx);
        let current = self.index_map.get(key).copied();
        if current != Some(NodeRef { idx, location }) {
            self.index_map
                .insert(key.to_string(), NodeRef { idx, location });
        }
    }

    fn pop_front(&mut self, location: Location) -> Option<usize> {
        let head = self.segment(location).head?;
        self.detach(head);
        Some(head)
    }

    fn move_to_back(&mut self, idx: usize) {
        let location = self.location_for_idx(idx);
        if self.segment(location).tail == Some(idx) {
            return;
        }
        self.detach(idx);
        self.push_back(idx, location);
    }

    fn remove_node(&mut self, idx: usize) {
        let node = self.nodes[idx].take().unwrap();
        self.index_map.remove(node.key.as_str());
        self.free_list.push(idx);
        self.total_eviction_count += 1;
    }

    fn remove_detached_node(&mut self, idx: usize) {
        self.remove_node(idx);
    }

    fn promote_to_protected(&mut self, idx: usize) {
        self.detach(idx);

        if self.protected_max == 0 {
            self.push_back(idx, Location::Probation);
            return;
        }

        if self.protected.len >= self.protected.max {
            if let Some(demoted_idx) = self.pop_front(Location::Protected) {
                if self.probation_max == 0 {
                    self.remove_detached_node(demoted_idx);
                } else {
                    self.push_back(demoted_idx, Location::Probation);
                    if self.probation.len > self.probation.max {
                        if let Some(evicted_idx) = self.pop_front(Location::Probation) {
                            self.remove_detached_node(evicted_idx);
                        }
                    }
                }
            }
        }

        self.push_back(idx, Location::Protected);
    }

    pub fn get(&mut self, key: &str) -> Option<&str> {
        self.update_freq_map(key);

        let entry = *self.index_map.get(key)?;
        match entry.location {
            Location::Window => self.move_to_back(entry.idx),
            Location::Probation => self.promote_to_protected(entry.idx),
            Location::Protected => self.move_to_back(entry.idx),
        }

        let idx = self.index_map.get(key)?.idx;
        Some(self.value_for_idx(idx))
    }

    pub fn upsert(&mut self, key: String, value: String) {
        if self.window_max == 0 && self.probation_max == 0 && self.protected_max == 0 {
            self.total_eviction_count += 1;
            return;
        }

        self.update_freq_map(&key);

        if let Some(entry) = self.index_map.get(&key).copied() {
            if let Some(node) = &mut self.nodes[entry.idx] {
                node.value = value;
            }

            match entry.location {
                Location::Window => self.move_to_back(entry.idx),
                Location::Probation => self.promote_to_protected(entry.idx),
                Location::Protected => self.move_to_back(entry.idx),
            }
            return;
        }

        let idx = self.alloc_node(key, value);

        if self.window_max > 0 {
            self.push_back(idx, Location::Window);
            if self.window.len > self.window.max {
                if let Some(window_evictee) = self.pop_front(Location::Window) {
                    self.admit_to_main_cache(window_evictee);
                }
            }
        } else {
            self.admit_to_main_cache(idx);
        }
    }

    fn admit_to_main_cache(&mut self, candidate_idx: usize) {
        if self.probation_max == 0 {
            if self.protected_max == 0 {
                self.remove_detached_node(candidate_idx);
                return;
            }

            if self.protected.len >= self.protected.max {
                if let Some(evicted_idx) = self.pop_front(Location::Protected) {
                    self.remove_detached_node(evicted_idx);
                }
            }

            self.push_back(candidate_idx, Location::Protected);
            return;
        }

        if self.probation.len < self.probation.max {
            self.push_back(candidate_idx, Location::Probation);
            return;
        }

        if let Some(probation_victim_idx) = self.probation.head {
            let candidate_freq = self.sketch.count(self.key_for_idx(candidate_idx));
            let victim_freq = self.sketch.count(self.key_for_idx(probation_victim_idx));

            if candidate_freq > victim_freq {
                if let Some(evicted_idx) = self.pop_front(Location::Probation) {
                    self.remove_detached_node(evicted_idx);
                }
                self.push_back(candidate_idx, Location::Probation);
            } else {
                self.remove_detached_node(candidate_idx);
            }
        }
    }

    pub fn stats(&mut self) {
        let mut table = Table::new("{:<} {:>} {:>} {:>} {:>} {:>}");
        table.add_row(
            Row::new()
                .with_cell("name")
                .with_cell("win_size")
                .with_cell("prob_size")
                .with_cell("prot_size")
                .with_cell("total_evict")
                .with_cell("total_keys"),
        );

        table.add_row(
            Row::new()
                .with_cell(&self.name)
                .with_cell(self.window.len.to_string())
                .with_cell(self.probation.len.to_string())
                .with_cell(self.protected.len.to_string())
                .with_cell(self.total_eviction_count.to_string())
                .with_cell(self.index_map.len().to_string()),
        );

        println!("{}", table);
    }

    pub fn debug_integrity(&self) {
        let mut seen_keys = HashSet::new();

        for location in [Location::Window, Location::Probation, Location::Protected] {
            let mut count = 0usize;
            let mut cursor = self.segment(location).head;
            let mut prev = None;

            while let Some(idx) = cursor {
                let node = self.nodes[idx].as_ref().unwrap();

                assert_eq!(node.location, location, "Node stored in wrong segment");
                assert_eq!(node.prev, prev, "Broken prev link");
                assert_eq!(
                    self.index_map.get(&node.key),
                    Some(&NodeRef { idx, location })
                );
                assert!(seen_keys.insert(node.key.as_str()), "Duplicate key found in segments");

                prev = Some(idx);
                cursor = node.next;
                count += 1;
            }

            assert_eq!(prev, self.segment(location).tail, "Broken tail pointer");
            assert_eq!(count, self.segment(location).len, "Segment length mismatch");
            assert!(
                self.segment(location).len <= self.segment(location).max,
                "Segment exceeds configured capacity"
            );
        }

        assert_eq!(seen_keys.len(), self.index_map.len(), "Index map size mismatch");
    }
    #[cfg(test)]
    fn segment_keys(&self, location: Location) -> Vec<&str> {
        let mut keys = Vec::new();
        let mut cursor = self.segment(location).head;

        while let Some(idx) = cursor {
            let node = self.nodes[idx].as_ref().unwrap();
            keys.push(node.key.as_str());
            cursor = node.next;
        }

        keys
    }

}

impl Cache for WTinyLfu {
    fn upsert(&mut self, key: String, value: String) {
        self.upsert(key, value);
    }

    fn get(&mut self, key: &str) -> Option<String> {
        self.get(key).map(|s| s.to_string())
    }

    fn name(&self) -> &str {
        self.name()
    }

    fn stats(&mut self) {
        self.stats();
    }

    fn debug_integrity(&mut self) {
        (self as &WTinyLfu).debug_integrity();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_small_cache() -> WTinyLfu {
        WTinyLfu::new(10)
    }

    fn location_of(cache: &WTinyLfu, key: &str) -> Option<Location> {
        cache.index_map.get(key).map(|entry| entry.location)
    }

    #[test]
    fn test_unconditional_window_admission() {
        let mut cache = setup_small_cache();

        cache.upsert("key_1".to_string(), "val_1".to_string());
        assert_eq!(location_of(&cache, "key_1"), Some(Location::Window));
        assert_eq!(cache.window.len, 1);
        cache.debug_integrity();
    }

    #[test]
    fn test_window_overflow_to_empty_probation() {
        let mut cache = setup_small_cache();

        cache.upsert("key_1".to_string(), "val_1".to_string());
        cache.upsert("key_2".to_string(), "val_2".to_string());

        assert_eq!(location_of(&cache, "key_2"), Some(Location::Window));
        assert_eq!(location_of(&cache, "key_1"), Some(Location::Probation));
        assert_eq!(cache.probation.len, 1);
        cache.debug_integrity();
    }

    #[test]
    fn test_admission_duel_rejection() {
        let mut cache = setup_small_cache();

        cache.upsert("key_1".to_string(), "val_1".to_string());
        cache.upsert("key_2".to_string(), "val_2".to_string());
        cache.upsert("key_3".to_string(), "val_3".to_string());

        cache.update_freq_map("key_1");
        cache.update_freq_map("key_1");

        assert_eq!(cache.index_map.get("key_2"), None);
        assert_eq!(cache.total_eviction_count, 1);
        cache.debug_integrity();
    }

    #[test]
    fn test_admission_duel_success() {
        let mut cache = setup_small_cache();

        cache.upsert("key_1".to_string(), "val_1".to_string());
        cache.upsert("key_2".to_string(), "val_2".to_string());

        cache.update_freq_map("key_2");
        cache.update_freq_map("key_2");

        cache.upsert("key_3".to_string(), "val_3".to_string());

        assert_eq!(location_of(&cache, "key_1"), None);
        assert_eq!(location_of(&cache, "key_2"), Some(Location::Probation));
        assert_eq!(location_of(&cache, "key_3"), Some(Location::Window));
        cache.debug_integrity();
    }

    #[test]
    fn test_probation_to_protected_promotion() {
        let mut cache = setup_small_cache();

        cache.upsert("key_1".to_string(), "val_1".to_string());
        cache.upsert("key_2".to_string(), "val_2".to_string());

        assert_eq!(location_of(&cache, "key_1"), Some(Location::Probation));

        let val = cache.get("key_1");
        assert_eq!(val, Some("val_1"));
        assert_eq!(location_of(&cache, "key_1"), Some(Location::Protected));
        assert_eq!(cache.protected.len, 1);
        cache.debug_integrity();
    }

    #[test]
    fn test_protected_demotion_cascade() {
        let mut cache = setup_small_cache();

        for i in 0..8 {
            let hot_key = format!("hot_{}", i);
            let idx = cache.alloc_node(hot_key.clone(), format!("val_{}", i));
            cache.push_back(idx, Location::Protected);
        }

        let candidate_idx = cache.alloc_node("new_hot".to_string(), "new_val".to_string());
        cache.push_back(candidate_idx, Location::Probation);

        assert_eq!(cache.protected.len, 8);
        assert_eq!(
            cache.segment_keys(Location::Protected).first().copied(),
            Some("hot_0")
        );

        cache.get("new_hot");

        assert_eq!(location_of(&cache, "hot_0"), Some(Location::Probation));
        assert_eq!(location_of(&cache, "new_hot"), Some(Location::Protected));
        assert_eq!(cache.protected.len, 8);
        cache.debug_integrity();
    }

    #[test]
    fn test_tiny_capacities_do_not_underflow() {
        let mut cache = WTinyLfu::new(1);

        cache.upsert("only".to_string(), "value".to_string());
        assert_eq!(cache.get("only"), Some("value"));
        cache.debug_integrity();

        let zero_cache = WTinyLfu::new(0);
        assert_eq!(zero_cache.window_max, 0);
        assert_eq!(zero_cache.probation_max, 0);
        assert_eq!(zero_cache.protected_max, 0);
    }
}
