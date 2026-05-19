use hashbrown::HashMap;
use tabular::{Table, Row};
use crate::Cache;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct AlgorithmStats {
    pub algorithm_name: String,
    pub protected_pool_size: usize,
    pub probation_pool_size: usize,
    pub protected_eviction_trigger: usize,
    pub protected_eviction_budget: usize,
    pub total_element_count: usize,
    pub total_eviction_count: usize,
    pub promotion_trigger: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum Location {
    Probation(usize),
    Protected(usize)
}

#[derive(Debug)]
pub struct ValueMeta {
    pub key: String,
    pub value: String,
    pub visited: bool,
    pub access_count: usize,
}

pub struct MemoryBoundedMap {
    pub index_map: HashMap<String, Location>,
    pub protected_pool_size: usize,
    pub probation_pool_size: usize,
    pub protected_used_count: usize,
    pub protected_eviction_trigger: usize,
    pub protected_eviction_budget: usize,
    pub protected_pool: Vec<Option<ValueMeta>>,
    pub probation_pool: Vec<Option<ValueMeta>>,
    pub protected_hand: usize,
    pub probation_hand: usize,
    pub protected_free_list: Vec<usize>,
    pub name: String,
    pub total_eviction_count: usize,
    pub promotion_trigger: usize,
}

impl MemoryBoundedMap {
    pub fn new(protected_size: usize, trigger: usize, probation_size: usize, budget: usize, promotion_trigger: usize) -> Self {
        Self {
            index_map: HashMap::new(),
            protected_pool_size: protected_size,
            probation_pool_size: probation_size,
            protected_used_count: 0,
            protected_eviction_trigger: trigger,
            protected_eviction_budget: budget,
            protected_pool: (0..protected_size).map(|_| None).collect(),
            probation_pool: (0..probation_size).map(|_| None).collect(),
            protected_hand: 0,
            probation_hand: 0,
            protected_free_list: (0..protected_size).rev().collect(),
            name: "Segmented Admission Control & Sieve Eviction".to_string(),
            total_eviction_count: 0,
            promotion_trigger: promotion_trigger,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn fetch_stats(&mut self) -> AlgorithmStats {
        AlgorithmStats {
            algorithm_name: self.name.clone(),
            protected_pool_size: self.protected_pool_size,
            probation_pool_size: self.probation_pool_size,
            protected_eviction_trigger: self.protected_eviction_trigger,
            protected_eviction_budget: self.protected_eviction_budget,
            total_element_count: self.index_map.len(),
            total_eviction_count: self.total_eviction_count,
            promotion_trigger: self.promotion_trigger,
        }
    }

    pub fn debug_integrity(&self) {
        let mut probation_live_slots = 0usize;
        let mut probation_stale_slots = 0usize;
        let mut probation_duplicate_slots = 0usize;

        let mut protected_live_slots = 0usize;
        let mut protected_stale_slots = 0usize;
        let mut protected_duplicate_slots = 0usize;

        let mut seen_keys: HashSet<&str> = HashSet::new();

        for (idx, slot) in self.probation_pool.iter().enumerate() {
            if let Some(item) = slot {
                probation_live_slots += 1;

                if !seen_keys.insert(item.key.as_str()) {
                    probation_duplicate_slots += 1;
                }

                match self.index_map.get(&item.key) {
                    Some(Location::Probation(map_idx)) if *map_idx == idx => {}
                    _ => {
                        probation_stale_slots += 1;
                    }
                }
            }
        }

        for (idx, slot) in self.protected_pool.iter().enumerate() {
            if let Some(item) = slot {
                protected_live_slots += 1;

                if !seen_keys.insert(item.key.as_str()) {
                    protected_duplicate_slots += 1;
                }

                match self.index_map.get(&item.key) {
                    Some(Location::Protected(map_idx)) if *map_idx == idx => {}
                    _ => {
                        protected_stale_slots += 1;
                    }
                }
            }
        }

        let pool_unique_keys = seen_keys.len();
        let index_keys = self.index_map.len();

        println!();
        println!("--- INTEGRITY DEBUG ---");
        println!("Index Map Keys           : {}", index_keys);
        println!("Pool Unique Keys         : {}", pool_unique_keys);
        println!("Probation Live Slots     : {}", probation_live_slots);
        println!("Probation Stale Slots    : {}", probation_stale_slots);
        println!("Probation Duplicates     : {}", probation_duplicate_slots);
        println!("Protected Live Slots     : {}", protected_live_slots);
        println!("Protected Stale Slots    : {}", protected_stale_slots);
        println!("Protected Duplicates     : {}", protected_duplicate_slots);
        println!("Protected Free List      : {}", self.protected_free_list.len());
        println!("Total Pool Live Slots    : {}", probation_live_slots + protected_live_slots);
        println!("------------------------");
        println!();
    }

    pub fn stats(&mut self) {
        let mut table = Table::new("{:<} {:>} {:>} {:>} {:>} {:>} {:>} {:>} {:>}");
        // Add Header Row
        table.add_row(Row::new()
            .with_cell("name")
            .with_cell("prot_size")
            .with_cell("prob_size")
            .with_cell("promo_trigger")
            .with_cell("e_trig")
            .with_cell("e_budget")
            .with_cell("prot_used")
            .with_cell("total_evict")
            .with_cell("total_key"));

        // Add Data Row
        table.add_row(Row::new()
            .with_cell(&self.name)
            .with_cell(self.protected_pool_size.to_string())
            .with_cell(self.probation_pool_size.to_string())
            .with_cell(self.promotion_trigger.to_string())
            .with_cell(self.protected_eviction_trigger.to_string())
            .with_cell(self.protected_eviction_budget.to_string())
            .with_cell(self.protected_used_count.to_string())
            .with_cell(self.total_eviction_count.to_string())
            .with_cell(self.index_map.len().to_string()));

        println!("{}", table);
    }

    pub fn get(&mut self, key: &str) -> Option<&str> {
        let loc = *self.index_map.get(key)?;
        match loc {
            Location::Probation(idx) => {
                // Take the item out to mutate/inspect it
                if let Some(mut item) = self.probation_pool[idx].take() {
                    // 1. Increment access count and mark visited
                    item.access_count += 1;
                    item.visited = true;

                    // 2. Check if it qualifies for promotion
                    if item.access_count >= self.promotion_trigger {
                        self.evict_protected();

                        if let Some(prot_idx) = self.protected_free_list.pop() {
                            self.protected_pool[prot_idx] = Some(item);
                            self.index_map.insert(key.to_string(), Location::Protected(prot_idx));
                            self.protected_used_count += 1;
                            self.protected_pool[prot_idx].as_ref().map(|m| m.value.as_str())
                        } else {
                            // Protected is full even after evict attempt; keep in probation
                            self.probation_pool[idx] = Some(item);
                            self.probation_pool[idx].as_ref().map(|m| m.value.as_str())
                        }
                    } else {
                        // 3. Trigger not met: put it back exactly where it was in probation
                        self.probation_pool[idx] = Some(item);
                        self.probation_pool[idx].as_ref().map(|m| m.value.as_str())
                    }
                } else {
                    None
                }
            }
            Location::Protected(idx) => {
                if let Some(item) = &mut self.protected_pool[idx] {
                    item.visited = true;
                    item.access_count += 1; // Good practice to track hits here too
                    Some(item.value.as_str())
                } else {
                    None
                }
            }
        }
    }

    pub fn upsert(&mut self, key: String, value: String) {
        let existing_location = self.index_map.get(&key).copied();

        match existing_location {
            Some(Location::Probation(idx)) => {
                if let Some(mut item) = self.probation_pool[idx].take() {
                    item.value = value;
                    // Increment access count because the key was targeted/hit
                    item.access_count += 1;
                    item.visited = true;

                    // Check if it qualifies for promotion
                    if item.access_count >= self.promotion_trigger {
                        self.evict_protected();
                        if let Some(prot_idx) = self.protected_free_list.pop() {
                            self.protected_pool[prot_idx] = Some(item);
                            self.index_map.insert(key, Location::Protected(prot_idx));
                            self.protected_used_count += 1;
                        } else {
                            // Promotion failed because protected pool has no free slot.
                            // Put the item back into probation, otherwise it is lost.
                            self.probation_pool[idx] = Some(item);
                            self.index_map.insert(key, Location::Probation(idx));
                        }
                    } else {
                        // Trigger not met: Put the updated item straight back into probation
                        self.probation_pool[idx] = Some(item);
                        self.index_map.insert(key, Location::Probation(idx));
                    }
                }
            }

            Some(Location::Protected(idx)) => {
                if let Some(item) = &mut self.protected_pool[idx] {
                    item.value = value;
                    item.visited = true;
                    item.access_count += 1; // Increment count here too to track its hotness
                }
            }

            None => {
                //No need for protected eviction as new entry first moves on to the probation pool, which as per this implementation is a circular buffer.
                let pos = self.probation_hand;

                // Only evict from probation when inserting a brand-new key.
                if let Some(old_item) = self.probation_pool[pos].take() {
                    self.index_map.remove(&old_item.key);
                }

                // Brand new keys start with an access count of 1
                self.probation_pool[pos] = Some(ValueMeta {
                    key: key.clone(),
                    value,
                    visited: false,
                    access_count: 1,
                });

                self.index_map.insert(key, Location::Probation(pos));
                self.probation_hand = (pos + 1) % self.probation_pool_size;
            }
        }
    }

    fn evict_protected(&mut self) {
        if self.protected_used_count >= self.protected_eviction_trigger {
            let mut evicted = 0;
            loop {
                let idx = self.protected_hand;
                if let Some(mut item) = self.protected_pool[idx].take() {
                    if item.visited {
                        item.visited = false;
                        self.protected_pool[idx] = Some(item);
                    } else {
                        self.index_map.remove(&item.key);
                        self.protected_free_list.push(idx);
                        self.protected_used_count -= 1;
                        evicted += 1;
                    }
                }
                self.protected_hand = (idx + 1) % self.protected_pool_size;
                if evicted >= self.protected_eviction_budget {
                    break;
                }
            }
            self.total_eviction_count += evicted;
        }
    }
}

impl Cache for MemoryBoundedMap {
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
        MemoryBoundedMap::debug_integrity(self);
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    // Helper to setup the cache with your specific parameters
    fn setup_cache() -> MemoryBoundedMap {
        let protected_size = 10;
        let protected_trigger = 7;
        let probation_size = 20;
        let eviction_budget = 5;
        MemoryBoundedMap::new(protected_size, protected_trigger, probation_size, eviction_budget, 1)
    }

    #[test]
    fn test_probation_and_promotion_and_eviction() {
        let mut cache = setup_cache();

        // Test 1: Fill Probation (20 elements)
        for i in 0..20 {
            cache.upsert(format!("key_{}", i), format!("value_{}", i));
        }

        assert_eq!(cache.index_map.len(), 20, "Index map should have exactly 20 elements");
        assert_eq!(cache.protected_used_count, 0, "Protected pool should be empty initially");

        let mut read_fail_count: u32 = 0;
        for i in 0..7 {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i);
            // Assuming .get() returns Option<String> or Option<&String>
            if cache.get(&key).as_deref() != Some(value.as_str()) {
                read_fail_count += 1;
            }
        }

        assert_eq!(read_fail_count, 0, "There should be no cache miss here.");


        //Read one non existing key to ensure protected used count is not increasing in unsafe manner
        cache.get(&format!("key_{}", 34979799)).as_deref();


        assert_eq!(cache.index_map.len(), 20, "Index map should have exactly 20 elements");
        assert_eq!(cache.protected_used_count, 7, "Protected pool should have 7 elements after reading 7 values");

        // Test 3: Access the 8th element (key_7) to trigger eviction (Budget: 5)
        let key_7 = "key_7".to_string();
        assert!(cache.get(&key_7).is_some(), "Key 7 should be accessible");

        assert_eq!(cache.protected_used_count, 3, "Protected pool should have 7 elements after reading 7 values");
        assert_eq!(cache.total_eviction_count, 5, "Total 5 elements should be evicted");

        // After eviction: 20 initial - 5 budget = 15 total elements
        assert_eq!(cache.index_map.len(), 15, "Index map should be 15 after eviction budget of 5");
    }

    #[test]
    fn test_saturation_and_cyclic_behavior() {
        let mut cache = setup_cache();

        // Test 4: Heavy insertion
        for i in 0..2000 {
            cache.upsert(format!("key_{}", i), format!("value_{}", i));
        }

        // verify state doesn't crash and respects bounds
        assert!(cache.index_map.len() <= 20, "Cache should handle saturation without crashing");
    }
}
