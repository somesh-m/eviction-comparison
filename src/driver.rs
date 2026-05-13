mod bounded_map;
use bounded_map::MemoryBoundedMap;

fn main() {
    // Params: Protected Size=2, Trigger=1, Probation Size=3, Budget=1
    let mut cache = MemoryBoundedMap::new(2, 1, 3, 1);

    println!("--- Test 1: Fill Probation ---");
    cache.upsert("A".into(), "Val_A".into());
    cache.upsert("B".into(), "Val_B".into());
    cache.upsert("C".into(), "Val_C".into());
    print_cache_state(&cache);

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

    let mut protected_keys = vec![];
    for slot in &map.protected_pool {
        if let Some(m) = slot { protected_keys.push(format!("{}(v:{})", m.key, m.visited)); }
    }
    println!("Protected Pool: {:?}", protected_keys);
}
