use hashbrown::HashMap;
use hashbrown::hash_map::Entry;
use tabular::{Table, Row};
use crate::Cache;

#[derive(Debug, Clone)]
pub struct AlgorithmStats {
    pub algorithm_name: String,
    pub protected_pool_size: usize,
    pub probation_pool_size: usize,
    pub protected_eviction_trigger: usize,
    pub protected_eviction_budget: usize,
    pub probation_used_count: usize,
    pub total_element_count: usize,
    pub total_eviction_count: usize,
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
}

pub struct MemoryBoundedMap {
    pub index_map: HashMap<String, Location>,
    pub protected_pool_size: usize,
    pub probation_pool_size: usize,
    pub protected_used_count: usize,
    pub probation_used_count: usize,
    pub protected_eviction_trigger: usize,
    pub protected_eviction_budget: usize,
    pub protected_pool: Vec<Option<ValueMeta>>,
    pub probation_pool: Vec<Option<ValueMeta>>,
    pub protected_hand: usize,
    pub probation_hand: usize,
    pub protected_free_list: Vec<usize>,
    pub name: String,
    pub total_eviction_count: usize,
}

impl MemoryBoundedMap {
    pub fn new(protected_size: usize, trigger: usize, probation_size: usize, budget: usize) -> Self {
        Self {
            index_map: HashMap::new(),
            protected_pool_size: protected_size,
            probation_pool_size: probation_size,
            protected_used_count: 0,
            probation_used_count: 0,
            protected_eviction_trigger: trigger,
            protected_eviction_budget: budget,
            protected_pool: (0..protected_size).map(|_| None).collect(),
            probation_pool: (0..probation_size).map(|_| None).collect(),
            protected_hand: 0,
            probation_hand: 0,
            protected_free_list: (0..protected_size).rev().collect(),
            name: "Segmented Admission Control & Sieve Eviction".to_string(),
            total_eviction_count: 0,
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
            probation_used_count: self.probation_used_count,
            total_element_count: self.index_map.len(),
            total_eviction_count: self.total_eviction_count,
        }
    }

    pub fn stats(&mut self) {
        let mut table = Table::new("{:<} {:>} {:>} {:>} {:>} {:>} {:>} {:>}");
        // Add Header Row
        table.add_row(Row::new()
            .with_cell("Name")
            .with_cell("Prot Size")
            .with_cell("Prob Size")
            .with_cell("E Trig")
            .with_cell("E Budget")
            .with_cell("Prot Used")
            .with_cell("Total Evict")
            .with_cell("Total Key"));

        // Add Data Row
        table.add_row(Row::new()
            .with_cell(&self.name)
            .with_cell(self.protected_pool_size.to_string())
            .with_cell(self.probation_pool_size.to_string())
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
                self.evict_protected();
                if let Some(mut item) = self.probation_pool[idx].take() {
                    item.visited = true;
                    if let Some(prot_idx) = self.protected_free_list.pop() {
                        self.protected_pool[prot_idx] = Some(item);
                        self.index_map.insert(key.to_string(), Location::Protected(prot_idx));
                        self.probation_used_count -= 1;
                        self.protected_used_count += 1;
                        self.protected_pool[prot_idx].as_ref().map(|m| m.value.as_str())
                    } else {
                        // If protected is full, put it back in probation
                        self.probation_pool[idx] = Some(item);
                        self.probation_pool[idx].as_ref().map(|m| m.value.as_str())
                    }
                } else { None }
            }
            Location::Protected(idx) => {
                if let Some(item) = &mut self.protected_pool[idx] {
                    item.visited = true;
                    Some(item.value.as_str())
                } else { None }
            }
        }
    }

    pub fn upsert(&mut self, key: String, value: String) {
        self.evict_protected();

        let victim_key = self.probation_pool[self.probation_hand]
            .as_ref()
            .map(|item| item.key.clone());

        // If there is a victim, remove it from the map first.
        if let Some(v_key) = victim_key {
            // Only remove if it's not the same key we are currently upserting.
            if v_key != key {
                self.index_map.remove(&v_key);
            }
        }

        match self.index_map.entry(key) {
            Entry::Occupied(mut entry) => match *entry.get() {
                Location::Probation(idx) => {
                    if let Some(mut item) = self.probation_pool[idx].take() {
                        item.value = value;
                        item.visited = false;

                        if let Some(prot_idx) = self.protected_free_list.pop() {
                            self.protected_pool[prot_idx] = Some(item);
                            entry.insert(Location::Protected(prot_idx));

                            self.probation_used_count -= 1;
                            self.protected_used_count += 1;
                        }
                    }
                }

                Location::Protected(idx) => {
                    if let Some(item) = &mut self.protected_pool[idx] {
                        item.value = value;
                        item.visited = true;
                    }
                }
            },

            Entry::Vacant(entry) => {
                let pos = self.probation_hand;

                self.probation_pool[pos] = Some(ValueMeta {
                    key: entry.key().clone(),
                    value,
                    visited: false,
                });

                entry.insert(Location::Probation(pos));

                self.probation_hand = (pos + 1) % self.probation_pool_size;
                self.probation_used_count += 1;
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
        MemoryBoundedMap::new(protected_size, protected_trigger, probation_size, eviction_budget)
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
