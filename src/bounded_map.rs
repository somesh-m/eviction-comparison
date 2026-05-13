use hashbrown::HashMap;
use hashbrown::hash_map::Entry;

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
        }
    }

    pub fn get(&mut self, key: &str) -> Option<&str> {
        let loc = *self.index_map.get(key)?;
        match loc {
            Location::Probation(idx) => {
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

            // 2. If there is a victim, remove it from the map FIRST
        if let Some(v_key) = victim_key {
            // Only remove if it's not the same key we are currently upserting
            if v_key != key {
                self.index_map.remove(&v_key);
            }
        }

        match self.index_map.entry(key) {
            Entry::Occupied(mut entry) => {
                match *entry.get() {
                    Location::Probation(idx) => {
                        if let Some(mut item) = self.probation_pool[idx].take() {
                            item.value = value;
                            item.visited = true;
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
                }
            }
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
        if self.protected_used_count > self.protected_eviction_trigger {
            let mut evicted = 0;
            let start = self.protected_hand;
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
                if self.protected_hand == start || evicted >= self.protected_eviction_budget {
                    break;
                }
            }
        }
    }
}
