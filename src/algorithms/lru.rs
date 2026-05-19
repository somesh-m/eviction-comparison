use hashbrown::HashMap;
use tabular::{Table, Row};
use crate::Cache;

#[derive(Debug, Clone)]
pub struct LruNode {
    key: String,
    value: String,
    prev: Option<usize>,
    next: Option<usize>,
}

pub struct LruCache {
    pub map_size: usize,
    pub index_map: HashMap<String, usize>,
    pub eviction_trigger: usize,
    pub eviction_budget: usize,
    pub head: Option<usize>,
    pub tail: Option<usize>,
    pub free_list: Vec<usize>,
    pub nodes: Vec<Option<LruNode>>,
    pub name: String,
    pub total_eviction_count: usize,
}

impl LruCache {
    pub fn new(map_size: usize, trigger: usize, budget: usize) -> Self{
        Self {
            map_size: map_size,
            eviction_trigger: trigger,
            eviction_budget: budget,
            index_map: HashMap::with_capacity(map_size),
            head: None,
            tail: None,
            nodes: vec![None; map_size],
            free_list: (0..map_size).rev().collect(),
            name: "LRU".to_string(),
            total_eviction_count: 0,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn stats(&mut self) {

    }

    pub fn upsert(&mut self, key: String, value: String) {
        if let Some(&idx) = self.index_map.get(&key) {
            if let Some(ref mut item) = self.nodes[idx] {
                item.value = value;
            }
            self.detach_node(idx);
            self.push_head(idx);
        } else {
            //new element insert
            self.evict_tail();
            let node_idx = if let Some(recycled_idx) = self.free_list.pop() {
                recycled_idx
            } else {
                let new_idx = self.nodes.len();
                self.nodes.push(None);
                new_idx
            };
            //Construct the object
            self.nodes[node_idx] = Some(LruNode {
                key: key.clone(),
                value,
                prev: None,
                next: None
            });
            self.index_map.insert(key, node_idx);
            self.push_head(node_idx);
        }
    }

    pub fn get(&mut self, key: &str) -> Option<&str> {
        if let Some(&node_idx) = self.index_map.get(key) {
            self.detach_node(node_idx);
            self.push_head(node_idx);
            self.nodes[node_idx].as_ref().map(|node| node.value.as_str())
        } else {
            None
        }
    }

    fn evict_tail(&mut self) {
        if self.index_map.len() >= self.eviction_trigger {
            let mut eviction_count = 0;
            loop {
                if eviction_count >= self.eviction_budget {
                    break;
                }
                if let Some(tail_idx) = self.tail {
                    self.detach_node(tail_idx);

                    if let Some(evicted_node) = self.nodes[tail_idx].take() {
                        self.index_map.remove(&evicted_node.key);
                    }
                    self.total_eviction_count += 1;
                    self.free_list.push(tail_idx);
                    eviction_count += 1;
                }
            }

        }
    }

    fn detach_node(&mut self, idx: usize) {
        // 1. Extract the neighbor indices from the target node safely.
        // We unwrap the outer Option wrapper because we know this node exists.
        let (prev_idx, next_idx) = {
            let node = self.nodes[idx].as_ref().unwrap();
            (node.prev, node.next)
        };

        // 2. Fix the Left Neighbor: Point its 'next' over to our 'next_idx'
        if let Some(p) = prev_idx {
            if let Some(prev_node) = &mut self.nodes[p] {
                prev_node.next = next_idx;
            }
        } else {
            // If there was no previous neighbor, this node was the head!
            self.head = next_idx;
        }

        // 3. Fix the Right Neighbor: Point its 'prev' back to our 'prev_idx'
        if let Some(n) = next_idx {
            if let Some(next_node) = &mut self.nodes[n] {
                next_node.prev = prev_idx;
            }
        } else {
            // If there was no next neighbor, this node was the tail!
            self.tail = prev_idx;
        }
    }

    fn push_head(&mut self, idx: usize) {
        // Case A: The list already has an existing head node
        if let Some(old_head_idx) = self.head {
            // 1. Make the old head's 'prev' point back to our new node index
            if let Some(old_head_node) = &mut self.nodes[old_head_idx] {
                old_head_node.prev = Some(idx);
            }

            // 2. Link our new node's forward pointer to that old head index
            if let Some(new_node) = &mut self.nodes[idx] {
                new_node.next = Some(old_head_idx);
                new_node.prev = None; // Heads never have a previous neighbor
            }

            // 3. Globally promote this index to the cache head
            self.head = Some(idx);
        }
        // Case B: The list is completely empty (First node ever inserted)
        else {
            if let Some(new_node) = &mut self.nodes[idx] {
                new_node.prev = None;
                new_node.next = None;
            }
            // When there is only one element, it acts as both the Head and the Tail
            self.head = Some(idx);
            self.tail = Some(idx);
        }
    }
}


impl Cache for LruCache {
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

    // Helper to setup the cache
    fn setup_cache(map_size: usize, trigger: usize, budget: usize) -> LruCache {

        LruCache::new(map_size, trigger, budget)
    }

    #[test]
    fn test_simple_set_get() {
        let map_size = 20;
        let trigger = 15;
        let budget = 3;

        let mut cache = setup_cache(map_size, trigger, budget);

        //Fill in all the 10 elements
        for i in 0..10 {
            cache.upsert(format!("key_{}", i), format!("value_{}", i));
        }

        assert_eq!(cache.index_map.len(), 10, "Index map should have exactly 20 elements");
        assert_eq!(cache.free_list.len(), cache.map_size - cache.index_map.len(), "Free list size is not correct");
        assert_eq!(cache.total_eviction_count, 0, "There should be no eviction at this point");

        let mut miss_count = 0;
        for i in 0..10 {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i);
            if cache.get(&key).as_deref() != Some(value.as_str()) {
                miss_count += 1;
            }
        }

        assert_eq!(miss_count, 0, "There should be no cache miss here");
    }

    #[test]
    fn test_eviction() {
        let map_size = 20;
        let trigger = 15;
        let budget = 3;
        let mut cache = setup_cache(map_size, trigger, budget);

        //Fill the upto trigger
        for i in 0..trigger {
            cache.upsert(format!("key_{}", i), format!("value_{}", i));
        }

        assert_eq!(cache.total_eviction_count, 0, "There should be no eviction as we hae filled only upto trigger");
        assert_eq!(cache.index_map.len(), trigger, "There should be exactly {:?} number of keys", trigger);

        // Insert more elements to trigger eviction
        cache.upsert(format!("key_{}", trigger+1), format!("value_{}", trigger+1));
        // Check the eviction count to be equal to eviction_budget
        assert_eq!(cache.total_eviction_count, budget, "Eviction count should be equal to eviction budget");
        // Check the bookkeeping
        assert_eq!(cache.index_map.len(), (trigger+1)-budget, "Invalid number of keys after eviction");
        assert_eq!(cache.free_list.len(), map_size - ((trigger+1)-budget), "Invalid number of slots in free list after eviction");
    }

    #[test]
    fn test_update() {
        let map_size = 20;
        let trigger = 15;
        let budget = 3;
        let mut cache = setup_cache(map_size, trigger, budget);

        //Fill the upto trigger
        for i in 0..trigger {
            cache.upsert(format!("key_{}", i), format!("value_{}", i));
        }

        assert_eq!(cache.total_eviction_count, 0, "There should be no eviction as we hae filled only upto trigger");
        assert_eq!(cache.index_map.len(), trigger, "There should be exactly {:?} number of keys", trigger);

        // Update the value of even index elements
        for i in 0..trigger {
            if i % 2 == 0 {
                cache.upsert(format!("key_{}", i), format!("updated_value_{}", i));
            }
        }

        // Ensure non of the bookkeeping is compromised
        assert_eq!(cache.total_eviction_count, 0, "There should be no eviction as we hae filled only upto trigger");
        assert_eq!(cache.index_map.len(), trigger, "There should be exactly {:?} number of keys", trigger);

        let mut discrepancy_count = 0;
        for i in 0..trigger {
            let key = format!("key_{}", i);
            let value = cache.get(&key);

            let expected_val = if i % 2 == 0 {
                format!("updated_value_{}", i)
            } else {
                format!("value_{}", i)
            };

            if value != Some(expected_val.as_str()) {
                discrepancy_count += 1;
            }
        }

        assert_eq!(discrepancy_count, 0, "There should be no discrepancy in the values");
    }
}

