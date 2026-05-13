use std::collections::HashMap;

enum PoolType {
    probation,
    protected
}

#[derive(Hash, Eq, PartialEq, Debug)]
struct ValueMeta {
    key: String,
    value: String,
    pool: PoolType
}

impl ValueMeta {
    pub fn total_bytes(&self) -> usize {
        let stack_size = mem::size_of::<Self>();

        let heap_size = self.key.capacity() + self.value.capacity();

        stack_size + heap_size
    }
}

#[derive(Hash, Eq, PartialEq, Debug)]
struct MemoryBoundedMap {
    map: HashMap<String, ValueMeta>
    used_bytes: usize,
    max_bytes: usize,
    eviction_trigger: usize,
    
}

impl MemoryBoundedMap {
    pub fn new(max_bytes: usize, eviction_trigger: usize) -> Self {
        Self {
            map: HashMap::new(),
            used_bytes: 0,
            max_bytes,
            eviction_trigger,
        }
    }

    pub fn insert(&mut self, key: String, mut value: ValueMeta) {
        let new_entry_size = value.total_bytes() + key.capacity();

        // 1. Eviction Logic:
        while self.used_bytes + new_entry_size > self.eviction_trigger && !self.map.is_empty() {
            self.evict();
        }

        // 2. Handle overwrites
        if let Some(old_value) = self.map.get(&key) {
            self.used_bytes -= (old_value.total_bytes() - key.capacity());
        }

        // 3. Insert new keys
        self.used_bytes += new_entry_size;
        self.map.insert(key, value);
    }

    fn evict(&mut self) {
        //TODO: Implement eviction algorithm here, based on the type of pool.
    }

    pub fn get_usage(&self) -> f64 {
        (self.used_bytes as f64 / self.max_bytes as f64) * 100.0
    }
}
