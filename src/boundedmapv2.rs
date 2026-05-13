use hashbrown::HashMap;

enum PoolType {
    probation,
    protected
}

#[derive(Hash, Eq, PartialEq, Debug)]
struct ValueMeta {
    key: String,
    value: String,
    pool_type: PoolType,
    visited: bool,
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
    map: HashMap<String, usize>,
    protected_pool: Vec<Option<ValueMeta>>,
    probation_pool: Vec<Option<ValueMeta>>,

    protected_used_bytes: usize,
    probation_used_bytes: usize,

    protected_max_bytes: usize,
    probation_max_bytes: usize,

    protected_eviction_trigger: usize,
    probation_eviction_trigger: usize,
}

impl MemoryBoundedMap {
    pub fn new(protected_max_bytes: usize, protected_eviction_trigger: usize, probation_max_bytes: usize, probation_eviction_trigger: usize) -> Self {
        Self {
            index_map: HashMap::new(),
            protected_pool: Vec::new(),
            probation_pool: Vec::new(),

            protected_used_bytes: 0,
            probation_used_bytes: 0,

            protected_max_bytes,
            probation_max_bytes,

            protected_eviction_trigger,
            probation_eviction_trigger,
        }
    }

    pub fn insert(&mut self, key: String, mut value: ValueMeta) {
        let new_entry_size = value.total_bytes() + key.capacity();

        // 1. Probation eviction trigger logic:
        while self.probation_used_bytes + new_entry_size > self.probation_eviction_trigger && !self.index_map.is_empty() {
            self.evict_probation();
        }

        // 2. Protected eviction trigger logic:
        while self.protected_used_bytes > self.protected_eviction_trigger && !self.index_map.is_empty() {
            self.evict_protected();
        }

        // 3. Handle overwrites
        /**
         * 3. Handle updates
         * This block finds if the index_map already has the key. If the key exists this is basically an update call.
         * Steps for updating an already existing key:
         * a. Change the pool type to protected
         * b. Update the probation_used_bytes
         * c. Move the object from probation pool to protected pool
         */
        if let Some(old_index) = self.index_map.get(&key) {
            value.pool_type = PoolType::protected;
            let temp_size = old_value.total_bytes - key.capacity();
            self.probation_used_bytes -= temp_size;
            
            self.used_bytes -= (old_value.total_bytes() - key.capacity());
        }

        /**
         * 4. Insert new key
         * When inserting a new key, it will always be inserted in the probationary segment.
         * Steps for inserting a new key:
         * a. Increment the probation_used_bytes by new_entry_size
         * b. Add the value to the probation_pool. There is no sparsity in the probationary pool so we can directly use push to push at the end of the vector.
         * c. Store the index of the entry from pool in index_map. Index can be found out using self.probation_pool.len() - 1.
         * Note: The default type for pool type is probation. In case of update or get the appropritate logic should change it to protected
         */
        self.probation_used_bytes += new_entry_size;
        self.probation_pool.push(value);
        self.index_map.insert(key, value);
        self.map.insert(key, value);
    }

    fn evict_protected(&mut self) {
        //TODO: Implement the protected segment eviction logic.
    }

    fn evict_probation(&mut self) {
        /**
        /TODO: Implement the probation segment eviction logic.
        */
    }

    pub fn get_usage(&self) -> f64 {
        (self.used_bytes as f64 / self.max_bytes as f64) * 100.0
    }
}
