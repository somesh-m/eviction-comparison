use hashbrown::HashMap;
use tabular::{Table, Row};
use crate::Cache;

#[derive(Debug, Clone)]
pub struct LruNode {
    key: String,
    value: String,
    prev: Option<usize>,
    next: Option<usize>,
    access_count: usize,
}

#[derive(Debug, Clone)]
pub enum Location {
    Probation(usize),
    Protected(usize)
}

#[derive(Debug, Clone)]
pub enum NodeType {
    Probation,
    Protected
}

pub struct SegmentedLruCache {
    pub map_size: usize,
    pub probation_pool_size: usize,
    pub protected_pool_size: usize,
    pub index_map: HashMap<String, Location>,

    pub probation_eviction_trigger: usize,
    pub protected_eviction_trigger: usize,

    pub probation_eviction_budget: usize,
    pub protected_eviction_budget: usize,

    pub probation_head: Option<usize>,
    pub probation_tail: Option<usize>,

    pub protected_head: Option<usize>,
    pub protected_tail: Option<usize>,

    pub free_list: Vec<usize>,
    pub nodes: Vec<Option<LruNode>>,
    pub name: String,

    pub probation_eviction_count: usize,
    pub protected_eviction_count: usize,

    pub promotion_trigger: usize,

    pub protected_used_count: usize,
    pub probation_used_count: usize,
}

impl SegmentedLruCache {
    pub fn new(map_size: usize, protected_size: usize, probation_size: usize, probation_eviction_trigger: usize, protected_eviction_trigger: usize, probation_eviction_budget: usize, protected_eviction_budget: usize, promotion_trigger: usize) -> Self {
        Self {
            map_size: map_size,
            probation_pool_size: probation_size,
            protected_pool_size: protected_size,
            index_map: HashMap::with_capacity(map_size),
            probation_head: None,
            protected_head: None,
            protected_tail: None,
            probation_tail: None,
            nodes: vec![None; map_size],
            free_list: (0..map_size).rev().collect(),
            name: "Segmented LRU".to_string(),

            probation_eviction_count: 0,
            protected_eviction_count: 0,

            probation_eviction_trigger: probation_eviction_trigger,
            protected_eviction_trigger: protected_eviction_trigger,

            probation_eviction_budget: probation_eviction_budget,
            protected_eviction_budget: protected_eviction_budget,

            promotion_trigger: promotion_trigger,

            protected_used_count: 0,
            probation_used_count: 0,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn stats(&mut self) {

    }

    pub fn get(&mut self, key: &str) -> Option<&String> {
    // Step 1: Decode map variant
    if let Some(location) = self.index_map.get(key).cloned() {
        // Extract the index out of the match block so it's available below
        let node_idx = match location {
            Location::Probation(node_idx) => {
                // Update read count metrics inside the target slot
                if let Some(node) = &mut self.nodes[node_idx] {
                    node.access_count += 1;
                }

                let access_count = self.nodes[node_idx].as_ref().unwrap().access_count;

                // Evaluate if the node has proven hot enough for a promotion upgrade
                if access_count >= self.promotion_trigger {
                    // Snip out of Probation list circuits
                    self.detach_node(node_idx, NodeType::Probation);
                    self.probation_used_count -= 1;

                    // Wire into Protected list circuits
                    self.push_head(node_idx, NodeType::Protected);
                    self.protected_used_count += 1;

                    // Synchronize the enum tag location inside your indexing map
                    self.index_map.insert(key.to_string(), Location::Protected(node_idx));

                    // Check if this newly promoted entry pushes the protected pool over its limit
                    self.handle_protected_overflow();
                } else {
                    // Regular Probation hit: refresh position to front of probation tier
                    self.detach_node(node_idx, NodeType::Probation);
                    self.push_head(node_idx, NodeType::Probation);
                }

                node_idx // <-- Return index from this branch
            }
            Location::Protected(node_idx) => {
                // Item is already in the safe zone. Bump metrics and move to Protected MRU head.
                if let Some(node) = &mut self.nodes[node_idx] {
                    node.access_count += 1;
                }
                self.detach_node(node_idx, NodeType::Protected);
                self.push_head(node_idx, NodeType::Protected);

                node_idx // <-- Return index from this branch
            }
        };

        // Step 2: node_idx is now safely in scope here!
        self.nodes[node_idx].as_ref().map(|node| &node.value)
    } else {
        // Explicit Cache Miss
        None
    }
}

    pub fn upsert(&mut self, key: String, value: String) {
        // --- SCENARIO 1: CACHE HIT ---
        if let Some(location) = self.index_map.get(&key).cloned() {
            match location {
                Location::Probation(node_idx) => {
                    // 1. Mutate metadata and payload
                    if let Some(node) = &mut self.nodes[node_idx] {
                        node.value = value;
                        node.access_count += 1;
                    }

                    let access_count = self.nodes[node_idx].as_ref().unwrap().access_count;

                    // 2. Check if it qualifies for Promotion
                    if access_count >= self.promotion_trigger {
                        // Snip from Probation
                        self.detach_node(node_idx, NodeType::Probation);
                        self.probation_used_count -= 1;

                        // Add to Protected
                        self.push_head(node_idx, NodeType::Protected);
                        self.protected_used_count += 1;

                        // Update mapping variant to Protected
                        self.index_map.insert(key, Location::Protected(node_idx));

                        // Enforce Protected tier limits
                        self.handle_protected_overflow();
                    } else {
                        // Regular hit without promotion: bring to probation head
                        self.detach_node(node_idx, NodeType::Probation);
                        self.push_head(node_idx, NodeType::Probation);
                    }
                }
                Location::Protected(node_idx) => {
                    // Simple MRU update inside the safe tier
                    if let Some(node) = &mut self.nodes[node_idx] {
                        node.value = value;
                        node.access_count += 1;
                    }
                    self.detach_node(node_idx, NodeType::Protected);
                    self.push_head(node_idx, NodeType::Protected);
                }
            }
            return;
        }

        // --- SCENARIO 2: CACHE MISS (GLOBAL CAPACITY CHECK) ---
        // If our hashmap has reached maximum physical bounds, we must evict to make space
        // if self.index_map.len() >= self.probation_eviction_trigger {
        //     self.evict_probation_tail();
        // }

        // --- SCENARIO 3: FRESH INSERTION ---
        // 1. Grab a free vector index slot
        let node_idx = if let Some(recycled_idx) = self.free_list.pop() {
            recycled_idx
        } else {
            // Fallback strategy if map sizing calculation was tight
            let new_idx = self.nodes.len();
            self.nodes.push(None);
            new_idx
        };

        // 2. Build the node with initialized access metrics
        self.nodes[node_idx] = Some(LruNode {
            key: key.clone(),
            value,
            prev: None,
            next: None,
            access_count: 1, // First look counts as 1 access
        });

        // 3. Register location as Probation variant
        self.index_map.insert(key, Location::Probation(node_idx));

        // 4. Update pointer lists
        self.push_head(node_idx, NodeType::Probation);
        self.probation_used_count += 1;

        // 5. Check if probation buffer rules dictate an immediate eviction trigger
        if self.probation_used_count > self.probation_eviction_trigger {
            self.evict_probation_tail();
        }
    }

    pub fn detach_node(&mut self, idx: usize, node_type: NodeType) {
        match node_type {
            NodeType::Probation => {
                let (new_head, new_tail) = Self::detach_from_list(
                    &mut self.nodes,
                    idx,
                    self.probation_head,
                    self.probation_tail,
                );
                self.probation_head = new_head;
                self.probation_tail = new_tail;
            }
            NodeType::Protected => {
                let (new_head, new_tail) = Self::detach_from_list(
                    &mut self.nodes,
                    idx,
                    self.protected_head,
                    self.protected_tail,
                );
                self.protected_head = new_head;
                self.protected_tail = new_tail;
            }
        }
    }

    fn detach_from_list(
        nodes: &mut [Option<LruNode>],
        idx: usize,
        mut head: Option<usize>,
        mut tail: Option<usize>,
    ) -> (Option<usize>, Option<usize>) {
        let (prev_idx, next_idx) = {
            let node = nodes[idx].as_ref().unwrap();
            (node.prev, node.next)
        };

        // Fix Left Neighbor
        if let Some(p) = prev_idx {
            if let Some(prev_node) = &mut nodes[p] {
                prev_node.next = next_idx;
            }
        } else {
            head = next_idx;
        }

        // Fix Right Neighbor
        if let Some(n) = next_idx {
            if let Some(next_node) = &mut nodes[n] {
                next_node.prev = prev_idx;
            }
        } else {
            tail = prev_idx;
        }

        (head, tail)
    }

    pub fn push_head(&mut self, idx: usize, node_type: NodeType) {
        match node_type {
            NodeType::Probation => {
                let (new_head, new_tail) = Self::push_to_head_of_list(
                    &mut self.nodes,
                    idx,
                    self.probation_head,
                    self.probation_tail,
                );
                self.probation_head = new_head;
                self.probation_tail = new_tail;
            }
            NodeType::Protected => {
                let (new_head, new_tail) = Self::push_to_head_of_list(
                    &mut self.nodes,
                    idx,
                    self.protected_head,
                    self.protected_tail,
                );
                self.protected_head = new_head;
                self.protected_tail = new_tail;
            }
        }
    }

    fn push_to_head_of_list(
        nodes: &mut [Option<LruNode>],
        idx: usize,
        mut head: Option<usize>,
        mut tail: Option<usize>,
    ) -> (Option<usize>, Option<usize>) {
        if let Some(old_head_idx) = head {
            if let Some(old_head_node) = &mut nodes[old_head_idx] {
                old_head_node.prev = Some(idx);
            }
            if let Some(new_node) = &mut nodes[idx] {
                new_node.next = Some(old_head_idx);
                new_node.prev = None;
            }
            head = Some(idx);
        } else {
            if let Some(new_node) = &mut nodes[idx] {
                new_node.prev = None;
                new_node.next = None;
            }
            head = Some(idx);
            tail = Some(idx);
        }
        (head, tail)
    }

    fn handle_protected_overflow(&mut self) {
        if self.protected_used_count > self.protected_eviction_trigger {
            let mut eviction_count = 0;
            loop {
                if eviction_count >= self.protected_eviction_budget {
                    break;
                }
                if let Some(demoted_idx) = self.protected_tail {
                    // Snip from Protected
                    self.detach_node(demoted_idx, NodeType::Protected);
                    self.protected_used_count -= 1;

                    // Push into Probation
                    self.push_head(demoted_idx, NodeType::Probation);
                    self.probation_used_count += 1;

                    // Update variant type inside your index_map tracker
                    if let Some(node) = &self.nodes[demoted_idx] {
                        self.index_map.insert(node.key.clone(), Location::Probation(demoted_idx));
                    }

                    // If this demotion forces a probation threshold breach, evict down to free list
                    if self.probation_used_count > self.probation_pool_size {
                        self.evict_probation_tail();
                    }
                    eviction_count += 1;
                }
            }
            self.protected_eviction_count += eviction_count;
        }
    }

    /// Hard eviction of the oldest probation element
    fn evict_probation_tail(&mut self) {
        let mut eviction_count = 0;
        loop {
            if eviction_count >= self.probation_eviction_budget {
                break;
            }
            if let Some(evict_idx) = self.probation_tail {
                self.detach_node(evict_idx, NodeType::Probation);
                self.probation_used_count -= 1;
                // self.probation_eviction_count += 1; // Metric update

                // Pull element cleanly out of memory array
                if let Some(evicted_node) = self.nodes[evict_idx].take() {
                    self.index_map.remove(&evicted_node.key);
                }

                // Return index address right back to the recycler pool
                self.free_list.push(evict_idx);
                eviction_count += 1;
            }
        }
        self.probation_eviction_count += eviction_count;
    }
}

impl Cache for SegmentedLruCache {
    fn upsert(&mut self, key: String, value: String) {
        self.upsert(key, value);
    }

    fn get(&mut self, key: &str) -> Option<String> {
        // Converting Option<&str> to Option<String> to match trait
        self.get(key).map(|s| s.to_string())
    }

    fn name(&self) -> &str {
        self.name()
    }

    fn stats(&mut self) {
        self.stats();
    }

    fn debug_integrity(&mut self) {

    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to setup the cache with your specific parameters
    fn setup_cache() -> SegmentedLruCache {
        let map_size = 30;
        let protected_size = 10;
        let protected_trigger = 7;
        let probation_trigger = 17;
        let probation_size = 20;
        let probation_eviction_budget = 7;
        let protected_eviction_budget = 3;
        SegmentedLruCache::new(map_size, protected_size, probation_size, probation_trigger, protected_trigger, probation_eviction_budget, protected_eviction_budget, 1)
    }

    #[test]
    fn probation_protected_integrity() {
        let mut cache = setup_cache();

        // Fill probation without triggering eviction
        for i in 0..17 {
            cache.upsert(format!("key_{}", i), format!("value_{}", i));
        }

        // Assert zero eviction from probation and protected
        assert_eq!(cache.probation_eviction_count, 0, "There should be no eviction at this point from probation segment");
        assert_eq!(cache.protected_eviction_count, 0, "There should be no eviction at this point from protected segment");

        // Assert protected size is 0 at this point
        assert_eq!(cache.protected_used_count, 0, "There should be no element in protected pool at this point");

        // Assert 17 elements in probation segment
        assert_eq!(cache.probation_used_count, 17, "There should be exactly 17 element in probation pool at this point");
    }

    #[test]
    fn probation_eviction_integrity() {
        let mut cache = setup_cache();

        //Fill in the probation with 27 elements
        for i in 0..27 {
            cache.upsert(format!("key_{}", i), format!("value_{}", i));
        }

        //Ensure 7 elements are evicted
        assert_eq!(cache.probation_eviction_count, 14, "There should be 7 eviction at this point from probation segment");

        //Assert nothing is evicted from protected segment
        assert_eq!(cache.protected_eviction_count, 0, "There should be 0 eviction at this point from protected segment");
    }

    #[test]
    fn protected_promotion_integrity() {
        let mut cache = setup_cache();

        //Fill in the probation with 17 elements
        for i in 0..17 {
            cache.upsert(format!("key_{}", i), format!("value_{}", i));
        }

        //Read 5 elements
        for i in 0..5 {
            cache.get(&format!("key_{}", i));
        }

        //Ensure 5 elements are present in protected segment
        assert_eq!(cache.protected_used_count, 5, "There should be 5 elements in the protected segment");

        //Ensure 12 elements are present in probation segment
        assert_eq!(cache.probation_used_count, 12, "There should be 12 elements in the probation segment");

        //Ensure 17 elements in the index map
        assert_eq!(cache.index_map.len(), 17, "There should be 17 elements in the index map");
    }

    #[test]
    fn protected_eviction_integrity() {
        let mut cache = setup_cache();

        //Fill in the probation with 17 elements
        for i in 0..17 {
            cache.upsert(format!("key_{}", i), format!("value_{}", i));
        }

        //Read 7 elements
        for i in 0..7 {
            cache.get(&format!("key_{}", i));
        }

        //Ensure 5 elements are present in protected segment
        assert_eq!(cache.protected_used_count, 7, "There should be 5 elements in the protected segment");

        //Ensure 12 elements are present in probation segment
        assert_eq!(cache.protected_eviction_count, 0, "There should be 0 eviction from protected segment at this point");

        //Read 1 more elements
        cache.get(&format!("key_{}", 8));
        assert_eq!(cache.protected_eviction_count, 3, "There should be 3 eviction from protected segment at this point");

    }
}
