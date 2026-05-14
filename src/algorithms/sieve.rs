use hashbrown::HashMap;
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

#[derive(Debug, Clone)]
pub struct ValueMeta {
    pub key: String,
    pub value: String,
    pub visited: bool,
}

pub struct SieveMap {
    pub map_size: usize,
    pub index_map: HashMap<String, usize>,
    pub eviction_trigger: usize,
    pub eviction_budget: usize,
    pub hand: usize,
    pub free_list: Vec<usize>,
    pub entry_list: Vec<Option<ValueMeta>>,
    pub name: String,
    pub total_eviction_count: usize,
}

impl SieveMap {
    pub fn new(map_size: usize, trigger: usize, budget: usize) -> Self {
        Self {
            map_size: map_size,
            eviction_trigger: trigger,
            eviction_budget: budget,
            index_map: HashMap::new(),
            hand: 0,
            free_list: (0..map_size).rev().collect(),
            entry_list: vec![None; map_size],
            total_eviction_count: 0,
            name: "Sieve".to_string(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn stats(&mut self) {
        let mut table = Table::new("{:<} {:>} {:>} {:>} {:>} {:>}");
        // Add Header Row
        table.add_row(Row::new()
            .with_cell("Name")
            .with_cell("Map Capacity")
            .with_cell("E Trig")
            .with_cell("E Budget")
            .with_cell("Total Evict")
            .with_cell("Total Key"));

        // Add Data Row
        table.add_row(Row::new()
            .with_cell(&self.name)
            .with_cell(self.map_size.to_string())
            .with_cell(self.eviction_trigger.to_string())
            .with_cell(self.eviction_budget.to_string())
            .with_cell(self.total_eviction_count.to_string())
            .with_cell(self.index_map.len().to_string()));

        println!("{}", table);
    }

    pub fn get(&mut self, key: &str) -> Option<&str> {
        // 1. Get the index from the map
        if let Some(&idx) = self.index_map.get(key) {
            // 2. Access the actual metadata in the entry_list
            if let Some(item) = &mut self.entry_list[idx] {
                // 3. Mark as visited (the Sieve/Clock logic)
                item.visited = true;
                // 4. Return the value as a borrowed &str
                return Some(&item.value);
            }
        }
        None
    }

    pub fn upsert(&mut self, key: String, value: String) {
        if let Some(&idx) = self.index_map.get(&key) {
            if let Some(item) = &mut self.entry_list[idx] {
                item.value = value;
                item.visited = true;
            }
        } else {
            self.evict();
            if let Some(idx) = self.free_list.pop() {
                self.index_map.insert(key.clone(), idx);
                self.entry_list[idx] = Some(ValueMeta {
                    key,
                    value,
                    visited: true
                });
            }
        }
    }

    fn evict(&mut self) {
        if (self.map_size - self.free_list.len()) >= self.eviction_trigger {
            let mut evicted = 0;
            loop {
                let idx = self.hand;
                if let Some(mut item) = self.entry_list[idx].take() {
                    if item.visited {
                        item.visited = false;
                        self.entry_list[idx] = Some(item);
                    } else {
                        self.index_map.remove(&item.key);
                        self.free_list.push(idx);
                        evicted += 1;
                    }
                }
                self.hand = (idx + 1) % self.map_size;
                if evicted >= self.eviction_budget {
                    break;
                }
            }
            self.total_eviction_count += evicted;
        }
    }
}

impl Cache for SieveMap {
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

    // Helper to setup the cache
    fn setup_cache(map_size: usize, trigger: usize, budget: usize) -> SieveMap {

        SieveMap::new(map_size, trigger, budget)
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
