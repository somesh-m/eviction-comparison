mod bounded_map;
use bounded_map::MemoryBoundedMap;

fn main() {
    // Params: Protected Size=500, Trigger=300, Probation Size=2000, Budget=100
    let protected_size = 10;
    let protected_trigger = 7;
    let probation_size = 20;
    let eviction_budget = 3;
    let mut cache = MemoryBoundedMap::new(protected_size, protected_trigger, probation_size, eviction_budget);
    println!("--- Test Config --- \n Protected Pool: {0} \n Eviction Trigger: {1} \n Probation Pool: {2} \n Eviction Budget: {3}", protected_size, protected_trigger, probation_size, eviction_budget);
    println!("--- Test 1: Fill Probation ---");
    for i in 0..500 {
        // Generating unique keys: "key_0", "key_1", etc.
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);
        cache.upsert(key, value);
    }
    println("--- Test 2: Make sure there are no values currently in protected ---");
    print_cache_state(&cache);

    println!("--- Test 3: Read the values from probation ---");
    let read_fail_count: u32 = 0;
    for i in 0..500 {
        // Generating unique keys: "key_0", "key_1", etc.
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);
        if cache.get(key) != value {
            read_fail_count ++;
        }
    }
    println!("Cache miss: {0}", read_fail_count);

    println!("--- Test 4: Ensure protected count is now 500---")
    print_cache_state(&cache);

    println!("--- ")

    println!("\n--- Test 2: Promotion (Probation -> Protected) ---");
    // Accessing "A" should move it to protected
    cache.get("A");
    print_cache_state(&cache);

    println!("\n--- Test 3: Overwriting Probation (The Reaper) ---");
    // Probation is 3 slots. A was at index 0, but it moved.
    // Hand is currently at index 0 (after wrapping).
    // Upserting D, E, F to see probation recycling.
    cache.upsert("D".into(), "Val_D".into());
    cache.upsert("E".into(), "Val_E".into());
    print_cache_state(&cache);

    println!("\n--- Test 4: Sieve Eviction from Protected ---");
    // Move B to protected. Now protected is [A, B].
    cache.get("B");
    // Upsert a new item to trigger eviction logic
    // Protected count (2) > Trigger (1), so it will scan.
    // A and B both had visited=true from the 'get'.
    // Sieve will flip A to false, flip B to false, then stop (budget/rotation).
    cache.upsert("F".into(), "Val_F".into());
    println!("After potential eviction trigger:");
    print_cache_state(&cache);
}

fn print_cache_state(map: &MemoryBoundedMap) {
    println!("Index Map Size: {}", map.index_map.len());
    println!("Protected Count: {}", map.protected_used_count);

    // let mut protected_keys = vec![];
    // for slot in &map.protected_pool {
    //     if let Some(m) = slot { protected_keys.push(format!("{}(v:{})", m.key, m.visited)); }
    // }
    // println!("Protected Pool: {:?}", protected_keys);
}
