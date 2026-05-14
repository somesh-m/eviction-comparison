// mod bounded_map;
// use bounded_map::MemoryBoundedMap;

// fn main() {
//     // Params: Protected Size=10, Trigger=7, Probation Size=20, Budget=3
//     let protected_size = 10;
//     let protected_trigger = 7;
//     let probation_size = 20;
//     let eviction_budget = 5;

//     let mut cache = MemoryBoundedMap::new(protected_size, protected_trigger, probation_size, eviction_budget);

//     println!("--- Test Config --- \n Protected Pool: {0} \n Eviction Trigger: {1} \n Probation Pool: {2} \n Eviction Budget: {3}",
//         protected_size, protected_trigger, probation_size, eviction_budget);

//     println!("--- Test 1: Fill Probation ---");
//     for i in 0..20 {
//         let key = format!("key_{}", i);
//         let value = format!("value_{}", i);
//         cache.upsert(key, value);
//     }
//     print_cache_state(&cache);

//     println!("--- Test 2: Read the values from probation ---");
//     // Added 'mut' here so we can increment it
//     let mut read_fail_count: u32 = 0;
//     for i in 0..7 {
//         let key = format!("key_{}", i);
//         let value = format!("value_{}", i);
//         // Assuming .get() returns Option<String> or Option<&String>
//         if cache.get(&key).as_deref() != Some(value.as_str()) {
//             read_fail_count += 1;
//         }
//     }

//     println!("Cache miss: {0}", read_fail_count);
//     print_cache_state(&cache);

//     println!("--- Test 3: Check eviction from protected pool---");
//     let key_7 = format!("key_{}", 7);
//     let value_7 = format!("value_{}", 7);
//     if cache.get(&key_7).as_deref() != Some(value_7.as_str()) {
//         println!("Cache miss... failing the test");
//     }

//     print_cache_state(&cache);

//     println!("--- Test 3: Check eviction from protected pool---");
//     let key_7 = format!("key_{}", 7);
//     let value_7 = format!("value_{}", 7);
//     if cache.get(&key_7).as_deref() != Some(value_7.as_str()) {
//         println!("Cache miss... failing the test");
//     }

//     print_cache_state(&cache);

//     // Resetting without 'let' or with 'let mut' to shadow
//     read_fail_count = 0;
//     for i in 5..8 {
//         let key = format!("key_{}", i);
//         let value = format!("value_{}", i);
//         if cache.get(&key).as_deref() != Some(value.as_str()) {
//             read_fail_count += 1;
//         }
//     }
//     println!("Cache miss: {0}", read_fail_count);

//     println!("--- Testing Saturation ---");
//     //ensure index map size is 23
//     for i in 0..2000 {
//         let key = format!("key_{}", i);
//         let value = format!("value_{}", i);
//         cache.upsert(key, value);
//     }

//     print_cache_state(&cache);
// }

// fn print_cache_state(map: &MemoryBoundedMap) {
//     // Note: ensure these fields are public (pub) in your bounded_map module
//     println!("Index Map Size: {}", map.index_map.len());
//     println!("Protected Count: {}", map.protected_used_count);
// }
