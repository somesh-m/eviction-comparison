use hashbrown::HashMap;
use tabular::{Table, Row};
use crate::Cache;
use std::collections::{HashSet, VecDeque};
use probabilistic_collections::count_min_sketch::{CountMinSketch, CountMinStrategy};

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
pub enum Location {
    Window,
    Probation,
    Protected,
}

#[derive(Debug, Clone)]
pub struct ValueMeta {
    pub key: String,
    pub value: String,
}

pub struct WTinyLfu {
    pub index_map: HashMap<String, Location>,

    pub window_max: usize,
    pub probation_max: usize,
    pub protected_max: usize,

    pub window_queue: VecDeque<ValueMeta>,
    pub probation_queue: VecDeque<ValueMeta>,
    pub protected_queue: VecDeque<ValueMeta>,

    pub name: String,
    pub total_eviction_count: usize,
    pub error_tolerance: f64,
    pub confidence: f64,
    pub sketch: CountMinSketch::<CountMinStrategy, String>,
}

impl WTinyLfu {
    pub fn new(total_capacity: usize) -> Self {
        // Enforce a strict partitioning layout: 10% Window, 18% Probation, 72% Protected
        // ensuring even low capacities have workable segment boundaries.
        let window_max = std::cmp::max(1, (total_capacity as f64 * 0.10) as usize);
        let remaining = total_capacity - window_max;
        let probation_max = std::cmp::max(1, (remaining as f64 * 0.20) as usize);
        let protected_max = remaining - probation_max;

        let error_tolerance = 0.01;
        let confidence = 0.99;

        Self {
            index_map: HashMap::new(),
            window_max,
            probation_max,
            protected_max,
            window_queue: VecDeque::with_capacity(window_max),
            probation_queue: VecDeque::with_capacity(probation_max),
            protected_queue: VecDeque::with_capacity(protected_max),
            name: "Original W-TinyLFU (Window LRU + Segmented Main LRU)".to_string(),
            total_eviction_count: 0,
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

    fn touch_node(&mut self, key: &str, current_loc: Location) {
        match current_loc {
            Location::Window => {
                if let Some(idx) = self.window_queue.iter().position(|x| x.key == key) {
                    if let Some(item) = self.window_queue.remove(idx) {
                        self.window_queue.push_back(item);
                    }
                }
            }
            Location::Probation => {
                if let Some(idx) = self.probation_queue.iter().position(|x| x.key == key) {
                    if let Some(item) = self.probation_queue.remove(idx) {
                        self.promote_to_protected(item);
                    }
                }
            }
            Location::Protected => {
                if let Some(idx) = self.protected_queue.iter().position(|x| x.key == key) {
                    if let Some(item) = self.protected_queue.remove(idx) {
                        self.protected_queue.push_back(item);
                    }
                }
            }
        }
    }

    fn promote_to_protected(&mut self, item: ValueMeta) {
        if self.protected_queue.len() >= self.protected_max {
            if let Some(demoted) = self.protected_queue.pop_front() {
                self.index_map.insert(demoted.key.clone(), Location::Probation);
                self.probation_queue.push_back(demoted);

                if self.probation_queue.len() > self.probation_max {
                    self.evict_from_probation();
                }
            }
        }
        self.index_map.insert(item.key.clone(), Location::Protected);
        self.protected_queue.push_back(item);
    }

    fn evict_from_probation(&mut self) {
        if let Some(evicted) = self.probation_queue.pop_front() {
            self.index_map.remove(&evicted.key);
            self.total_eviction_count += 1;
        }
    }

    pub fn get(&mut self, key: &str) -> Option<&str> {
        self.update_freq_map(key);
        let loc = *self.index_map.get(key)?;

        self.touch_node(key, loc);

        let final_loc = self.index_map.get(key)?;
        match final_loc {
            Location::Window => self.window_queue.iter().find(|x| x.key == key).map(|x| x.value.as_str()),
            Location::Probation => self.probation_queue.iter().find(|x| x.key == key).map(|x| x.value.as_str()),
            Location::Protected => self.protected_queue.iter().find(|x| x.key == key).map(|x| x.value.as_str()),
        }
    }

    pub fn upsert(&mut self, key: String, value: String) {
        self.update_freq_map(&key);

        if let Some(&loc) = self.index_map.get(&key) {
            match loc {
                Location::Window => {
                    if let Some(item) = self.window_queue.iter_mut().find(|x| x.key == key) { item.value = value; }
                }
                Location::Probation => {
                    if let Some(item) = self.probation_queue.iter_mut().find(|x| x.key == key) { item.value = value; }
                }
                Location::Protected => {
                    if let Some(item) = self.protected_queue.iter_mut().find(|x| x.key == key) { item.value = value; }
                }
            }
            self.touch_node(&key, loc);
            return;
        }

        let new_item = ValueMeta { key: key.clone(), value };
        self.window_queue.push_back(new_item);
        self.index_map.insert(key, Location::Window);

        if self.window_queue.len() > self.window_max {
            if let Some(window_evictee) = self.window_queue.pop_front() {
                self.admit_to_main_cache(window_evictee);
            }
        }
    }

    fn admit_to_main_cache(&mut self, candidate: ValueMeta) {
        if self.probation_queue.len() < self.probation_max {
            self.index_map.insert(candidate.key.clone(), Location::Probation);
            self.probation_queue.push_back(candidate);
            return;
        }

        if let Some(probation_victim) = self.probation_queue.front() {
            let candidate_freq = self.sketch.count(&candidate.key);
            let victim_freq = self.sketch.count(&probation_victim.key);

            if candidate_freq > victim_freq {
                if let Some(evicted) = self.probation_queue.pop_front() {
                    self.index_map.remove(&evicted.key);
                    self.total_eviction_count += 1;
                }
                self.index_map.insert(candidate.key.clone(), Location::Probation);
                self.probation_queue.push_back(candidate);
            } else {
                self.index_map.remove(&candidate.key);
                self.total_eviction_count += 1;
            }
        }
    }

    pub fn stats(&mut self) {
        let mut table = Table::new("{:<} {:>} {:>} {:>} {:>} {:>}");
        table.add_row(Row::new()
            .with_cell("name")
            .with_cell("win_size")
            .with_cell("prob_size")
            .with_cell("prot_size")
            .with_cell("total_evict")
            .with_cell("total_keys"));

        table.add_row(Row::new()
            .with_cell(&self.name)
            .with_cell(self.window_queue.len().to_string())
            .with_cell(self.probation_queue.len().to_string())
            .with_cell(self.protected_queue.len().to_string())
            .with_cell(self.total_eviction_count.to_string())
            .with_cell(self.index_map.len().to_string()));

        println!("{}", table);
    }

    pub fn debug_integrity(&self) {
        let mut seen_keys = HashSet::new();

        for item in &self.window_queue {
            assert!(seen_keys.insert(&item.key), "Duplicate key found in window_queue");
            assert_eq!(self.index_map.get(&item.key), Some(&Location::Window));
        }
        for item in &self.probation_queue {
            assert!(seen_keys.insert(&item.key), "Duplicate key found in probation_queue");
            assert_eq!(self.index_map.get(&item.key), Some(&Location::Probation));
        }
        for item in &self.protected_queue {
            assert!(seen_keys.insert(&item.key), "Duplicate key found in protected_queue");
            assert_eq!(self.index_map.get(&item.key), Some(&Location::Protected));
        }
        assert_eq!(seen_keys.len(), self.index_map.len(), "Index map and queue sizes misaligned");
    }
}

impl Cache for WTinyLfu {
    fn upsert(&mut self, key: String, value: String) { self.upsert(key, value); }
    fn get(&mut self, key: &str) -> Option<String> { self.get(key).map(|s| s.to_string()) }
    fn name(&self) -> &str { self.name() }
    fn stats(&mut self) { self.stats(); }
    fn debug_integrity(&mut self) { (self as &WTinyLfu).debug_integrity(); }
}


#[cfg(test)]
mod tests {
    use super::*;

    // A capacity of 10 results in:
    // Window capacity = 1
    // Main capacity = 9 -> Probation (20% of 9) = 1, Protected (remaining) = 8
    fn setup_small_cache() -> WTinyLfu {
        WTinyLfu::new(10)
    }

    #[test]
    fn test_unconditional_window_admission() {
        let mut cache = setup_small_cache();

        // 1. Insert first item. It must be admitted into the Window unconditionally.
        cache.upsert("key_1".to_string(), "val_1".to_string());
        assert_eq!(cache.index_map.get("key_1"), Some(&Location::Window));
        assert_eq!(cache.window_queue.len(), 1);
        cache.debug_integrity();
    }

    #[test]
    fn test_window_overflow_to_empty_probation() {
        let mut cache = setup_small_cache();

        // 1. Fill window (size 1)
        cache.upsert("key_1".to_string(), "val_1".to_string());

        // 2. Insert second item. "key_1" overflows window.
        // Since Probation is empty (max 1), "key_1" moves to Probation without dueling.
        cache.upsert("key_2".to_string(), "val_2".to_string());

        assert_eq!(cache.index_map.get("key_2"), Some(&Location::Window));
        assert_eq!(cache.index_map.get("key_1"), Some(&Location::Probation));
        assert_eq!(cache.probation_queue.len(), 1);
        // cache.debug_integrity();
    }

    #[test]
    fn test_admission_duel_rejection() {
        let mut cache = setup_small_cache();

        cache.upsert("key_1".to_string(), "val_1".to_string()); // Moves to Probation eventually
        cache.upsert("key_2".to_string(), "val_2".to_string()); // Now in Window, pushes key_1 to Probation
        cache.upsert("key_3".to_string(), "val_3".to_string()); // Now in Window, pushes key_2 to Duel

        // Boost historical frequency of probation resident ("key_1")
        cache.update_freq_map("key_1");
        cache.update_freq_map("key_1");

        // "key_2" will be evicted from Window and duel "key_1".
        // "key_2" has lower frequency than "key_1", so "key_2" is fully rejected/evicted.
        assert_eq!(cache.index_map.get("key_2"), None);
        assert_eq!(cache.total_eviction_count, 1);
        // cache.debug_integrity();
    }

    #[test]
    fn test_admission_duel_success() {
        let mut cache = setup_small_cache();

        cache.upsert("key_1".to_string(), "val_1".to_string());
        cache.upsert("key_2".to_string(), "val_2".to_string());

        // Artificially make the incoming window evictee ("key_2") hotter than probation resident ("key_1")
        cache.update_freq_map("key_2");
        cache.update_freq_map("key_2");

        // Triggering another insert pushes key_2 out of Window to duel key_1 in Probation
        cache.upsert("key_3".to_string(), "val_3".to_string());

        // "key_2" wins the duel! "key_1" gets permanently evicted out of Probation.
        assert_eq!(cache.index_map.get("key_1"), None);
        assert_eq!(cache.index_map.get("key_2"), Some(&Location::Probation));
        assert_eq!(cache.index_map.get("key_3"), Some(&Location::Window));
        // cache.debug_integrity();
    }

    #[test]
    fn test_probation_to_protected_promotion() {
        let mut cache = setup_small_cache();

        cache.upsert("key_1".to_string(), "val_1".to_string());
        cache.upsert("key_2".to_string(), "val_2".to_string()); // Pushes key_1 into Probation

        assert_eq!(cache.index_map.get("key_1"), Some(&Location::Probation));

        // Reading "key_1" while it lives in Probation must promote it to Protected
        let val = cache.get("key_1");
        assert_eq!(val, Some("val_1"));
        assert_eq!(cache.index_map.get("key_1"), Some(&Location::Protected));
        assert_eq!(cache.protected_queue.len(), 1);
        cache.debug_integrity();
    }

    #[test]
    fn test_protected_demotion_cascade() {
        let mut cache = setup_small_cache(); // Protected size = 8, Probation = 1, Window = 1

        // 1. Fill up the entire Protected segment (8 slots) via probation promotions
        for i in 0..8 {
            let k = format!("hot_{}", i);
            cache.upsert(k.clone(), format!("val_{}", i));
            cache.upsert("stub".to_string(), "stub".to_string()); // Overflows window to move 'hot_i' out
            cache.get(&k); // Promote 'hot_i' to Protected
        }
        assert_eq!(cache.protected_queue.len(), 8);

        // 2. Put a marked item at the LRU position (head of Protected queue)
        // By reading "hot_0" first, it was pushed back. "hot_0" is currently the oldest item.
        assert_eq!(cache.protected_queue.front().unwrap().key, "hot_0");

        // 3. Promote a new item into Protected, triggering a demotion cascade
        cache.upsert("new_hot".to_string(), "new_val".to_string());
        cache.upsert("stub2".to_string(), "stub2".to_string()); // move out of window
        cache.get("new_hot"); // Promote to Protected

        // "hot_0" should be demoted from Protected to Probation
        assert_eq!(cache.index_map.get("hot_0"), Some(&Location::Probation));
        assert_eq!(cache.protected_queue.len(), 8); // Still maxed out
        cache.debug_integrity();
    }
}
