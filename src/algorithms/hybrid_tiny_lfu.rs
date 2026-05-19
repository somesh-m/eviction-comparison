use hashbrown::HashMap;
use tabular::{Table, Row};
use crate::Cache;
use std::collections::HashSet;
use probabilistic_collections::count_min_sketch::{CountMinSketch, CountMinStrategy};

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
}

pub struct HybridTinyLFU {
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
    pub error_tolerance: f64,
    pub confidence: f64,
    pub sketch: CountMinSketch::<CountMinStrategy, String>,
}

impl HybridTinyLFU {
    pub fn new(protected_size: usize, trigger: usize, probation_size: usize, budget: usize) -> Self {
        let error_tolerance = 0.01;
        let confidence = 0.99;

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
            name: "Segmented Sieve with frequency based admission control".to_string(),
            total_eviction_count: 0,

            //Variables for count min sketch
            error_tolerance,
            confidence,
            sketch: CountMinSketch::<CountMinStrategy, String>::from_error(confidence, error_tolerance),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn update_freq_map(&mut self, key: &str) {
        self.sketch.insert(key, 1);
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
            promotion_trigger: 0,
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
            .with_cell(self.protected_eviction_trigger.to_string())
            .with_cell(self.protected_eviction_budget.to_string())
            .with_cell(self.protected_used_count.to_string())
            .with_cell(self.total_eviction_count.to_string())
            .with_cell(self.index_map.len().to_string()));

        println!("{}", table);
    }

    pub fn get(&mut self, key: &str) -> Option<&str> {
        self.update_freq_map(key);

        // Grab the location out of the map. If it doesn't exist, exit early.
        let loc = *self.index_map.get(key)?;

        match loc {
            Location::Probation(idx) => {
                // Take the item out of probation temporarily to decide its destination pool
                if let Some(mut item) = self.probation_pool[idx].take() {

                    if self.protected_used_count >= self.protected_eviction_trigger {
                        let prot_idx = self.protected_hand;

                        if let Some(mut prot_item) = self.protected_pool[prot_idx].take() {
                            // Compare the hotness using your TinyLFU sketch
                            if self.sketch.count(&item.key) > self.sketch.count(&prot_item.key) {
                                // Candidate Wins! Move `item` to Protected, demote `prot_item` to Probation
                                let item_key_clone = item.key.clone();
                                self.protected_pool[prot_idx] = Some(item);
                                self.index_map.insert(item_key_clone, Location::Protected(prot_idx));
                                self.protected_hand = (prot_idx + 1) % self.protected_pool_size;

                                let prob_idx = self.probation_hand;
                                if let Some(old_prob) = self.probation_pool[prob_idx].take() {
                                    self.index_map.remove(&old_prob.key);
                                }

                                let prot_key_clone = prot_item.key.clone();
                                self.probation_pool[prob_idx] = Some(prot_item);
                                self.index_map.insert(prot_key_clone, Location::Probation(prob_idx));
                                self.probation_hand = (prob_idx + 1) % self.probation_pool_size;

                                // Safely reference the value now that the item is back in storage
                                return self.protected_pool[prot_idx].as_ref().map(|item| item.value.as_str());
                            } else {
                                // Candidate Loses: Restore prot_item to Protected, leave item in Probation
                                let prot_key_clone = prot_item.key.clone();
                                self.protected_pool[prot_idx] = Some(prot_item);
                                self.index_map.insert(prot_key_clone, Location::Protected(prot_idx));

                                let item_key_clone = item.key.clone();
                                self.probation_pool[idx] = Some(item);
                                self.index_map.insert(item_key_clone, Location::Probation(idx));

                                return self.probation_pool[idx].as_ref().map(|item| item.value.as_str());
                            }
                        } else {
                            // Edge case safety fallback: if protected pool hand slot was unexpectedly empty
                            let item_key_clone = item.key.clone();
                            self.protected_pool[prot_idx] = Some(item);
                            self.index_map.insert(item_key_clone, Location::Protected(prot_idx));
                            return self.protected_pool[prot_idx].as_ref().map(|item| item.value.as_str());
                        }
                    } else {
                        // No eviction is needed: Promote directly to a free spot in the protected pool
                        if let Some(prot_idx) = self.protected_free_list.pop() {
                            let item_key_clone = item.key.clone();
                            self.protected_pool[prot_idx] = Some(item);
                            self.index_map.insert(item_key_clone, Location::Protected(prot_idx));
                            self.protected_used_count += 1;

                            return self.protected_pool[prot_idx].as_ref().map(|item| item.value.as_str());
                        } else {
                            // Fallback if free list lied to us: put back in probation
                            let item_key_clone = item.key.clone();
                            self.probation_pool[idx] = Some(item);
                            self.index_map.insert(item_key_clone, Location::Probation(idx));
                            return self.probation_pool[idx].as_ref().map(|item| item.value.as_str());
                        }
                    }
                } else {
                    None
                }
            }
            Location::Protected(idx) => {
                // Read hit on an item already in the Protected cache
                if let Some(item) = &self.protected_pool[idx] {
                    Some(item.value.as_str())
                } else {
                    None
                }
            }
        }
    }
    pub fn upsert(&mut self, key: String, value: String) {
        self.update_freq_map(&key);
        let existing_location = self.index_map.get(&key).copied();

        match existing_location {
            Some(Location::Probation(idx)) => {
                if let Some(mut item) = self.probation_pool[idx].take() {
                    item.value = value;

                    if self.protected_used_count >= self.protected_eviction_trigger {
                        let prot_idx = self.protected_hand;
                        if let Some(mut prot_item) = self.protected_pool[prot_idx].take() {

                            // Compare hotness using the TinyLFU Sketch
                            if self.sketch.count(&key) > self.sketch.count(&prot_item.key) {
                                // Candidate wins: Move item to Protected, demote prot_item to Probation

                                // 1. Place new item into Protected
                                let item_key_clone = item.key.clone();
                                self.protected_pool[prot_idx] = Some(item);
                                self.index_map.insert(item_key_clone, Location::Protected(prot_idx));
                                self.protected_hand = (prot_idx + 1) % self.protected_pool_size;

                                // 2. Evict/Overwite the old probation target with demoted protected item
                                let prob_idx = self.probation_hand;
                                if let Some(old_prob) = self.probation_pool[prob_idx].take() {
                                    self.index_map.remove(&old_prob.key);
                                }

                                let prot_key_clone = prot_item.key.clone();
                                self.probation_pool[prob_idx] = Some(prot_item);
                                self.index_map.insert(prot_key_clone, Location::Probation(prob_idx));
                                self.probation_hand = (prob_idx + 1) % self.probation_pool_size;
                            } else {
                                // Candidate loses: Keep prot_item in Protected, keep item in Probation

                                // 1. Put back protected item to its original slot
                                let prot_key_clone = prot_item.key.clone();
                                self.protected_pool[prot_idx] = Some(prot_item);
                                self.index_map.insert(prot_key_clone, Location::Protected(prot_idx));

                                // 2. Put item back into its original probation slot (or advance it to hand)
                                let item_key_clone = item.key.clone();
                                self.probation_pool[idx] = Some(item);
                                self.index_map.insert(item_key_clone, Location::Probation(idx));
                            }
                        }
                    } else {
                        // No eviction needed in protected pool
                        if let Some(prot_idx) = self.protected_free_list.pop() {
                            let item_key_clone = item.key.clone();
                            self.protected_pool[prot_idx] = Some(item);
                            self.index_map.insert(item_key_clone, Location::Protected(prot_idx));
                            self.protected_used_count += 1;
                        }
                    }
                }
            }

            Some(Location::Protected(idx)) => {
                if let Some(item) = &mut self.protected_pool[idx] {
                    item.value = value;
                }
            }

            None => {
                let pos = self.probation_hand;

                // TINYLFU ADMISSION WINDOW:
                // Check if incoming key is hotter than the victim currently sitting at probation_hand
                if let Some(old_item) = &self.probation_pool[pos] {
                    if self.sketch.count(&key) < self.sketch.count(&old_item.key) {
                        // Incoming key is colder than the victim. Reject admission to protect cache!
                        return;
                    }
                }

                // Evict the victim if it survived or if slot was filled
                if let Some(old_item) = self.probation_pool[pos].take() {
                    self.index_map.remove(&old_item.key);
                }

                self.probation_pool[pos] = Some(ValueMeta {
                    key: key.clone(),
                    value,
                });

                self.index_map.insert(key, Location::Probation(pos));
                self.probation_hand = (pos + 1) % self.probation_pool_size;
            }
        }
    }
}

impl Cache for HybridTinyLFU {
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
        HybridTinyLFU::debug_integrity(self);
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    // Helper to setup the cache with your specific parameters
    fn setup_cache() -> HybridTinyLFU {
        let protected_size = 10;
        let protected_trigger = 7;
        let probation_size = 20;
        let eviction_budget = 5;
        HybridTinyLFU::new(protected_size, protected_trigger, probation_size, eviction_budget)
    }

    #[test]
    fn test_promotion_demotion() {
        let mut cache = setup_cache();

        //Test 1: Fill Probation (20 elements)
        for i in 0..20 {
            cache.upsert(format!("key_{}", i), format!("value_{}", i));
        }

        assert_eq!(cache.protected_used_count, 0, "Protected pool should be empty initially");

        //Access 7 elements again to move them to protected pool
        for i in 0..7 {
            cache.get(&format!("key_{}", i));
        }

        //Make sure there are 7 elements in protected pool
        assert_eq!(cache.protected_used_count, 7, "Protectded pool must has 7 elements at this point");

        //Access one more element to trigger eviction
        cache.get(&format!("key_{}", 7));

        //assert there is no eviction as this new element has same freq as other protected elements
        assert_eq!(cache.protected_used_count, 7, "Protectded pool must has 7 elements at this point");

        //Even though the protected count is 7, we need to check the elements to make sure "key_7" is not present in protected and present in probation
        let location = cache.index_map.get("key_7").expect("key_7 should exist in the index map");
        assert!(
            matches!(location, Location::Probation(_)),
            "Expected key_7 to be in Probation, but found it in {:?}",
            location
        );

        //Access the element one more time to actually move it to protected
        cache.get(&format!("key_{}", 7));
        cache.get(&format!("key_{}", 7));
        cache.get(&format!("key_{}", 7));
        cache.get(&format!("key_{}", 7));

        //assert it is now present in protected pool
        let location = cache.index_map.get("key_7").expect("key_7 should exist in the index map");
        assert!(
            matches!(location, Location::Protected(_)),
            "Expected key_7 to be in Protected, but found it in {:?}",
            location
        );

        let location = cache.index_map.get("key_0").expect("key_0 should exist in the index map");
        assert!(
            matches!(location, Location::Probation(_)),
            "Expected key_0 to be in Protected, but found it in {:?}",
            location
        );
    }

    #[test]
    fn test_probation_and_protected_pool_integrity() {
        let mut cache = setup_cache();

        // Test 1: Fill Probation (20 elements)
        for i in 0..20 {
            cache.upsert(format!("key_{}", i), format!("value_{}", i));
        }

        assert_eq!(cache.index_map.len(), 20, "Index map should have exactly 20 elements");
        assert_eq!(cache.protected_used_count, 0, "Protected pool should be empty initially");

        //Test the frquency of each element
        for i in 0..20 {
            assert!(cache.sketch.count(&format!("key_{}", i)) >= 1, "Freq of each element should be atleast 1");
        }

        //Access an element again
        cache.get(&format!{"key_{}", 0});

        //Assert it is moved to protected pool
        assert_eq!(cache.protected_used_count, 1, "Protected pool should have exactly 1 element");
        //assert no change in index map
        assert_eq!(cache.index_map.len(), 20, "Index map should have no change in number of elements");
        //assert the frequency of this key to be greater than 1
        assert!(cache.sketch.count(&format!("key_{}", 0)) > 1, "Freq of this element should be greater than 1");
    }
}
