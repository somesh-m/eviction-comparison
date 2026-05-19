use probabilistic_collections::count_min_sketch::{CountMinSketch, CountMinStrategy};

fn main() {
    // 1. Initialize the Sketch
    // We pass our target error tolerance (how much overestimation we can accept)
    // and confidence (how certain we want to be that our error falls within that tolerance).
    let error_tolerance = 0.01; // 1% error tolerance
    let confidence = 0.99;      // 99% confidence level

    // The library automatically calculates the underlying optimal width and depth
    let mut sketch = CountMinSketch::<CountMinStrategy, String>::from_error(confidence, error_tolerance);

    println!("=== Phase 1: Inserting Elements ===");

    // Simulate hitting the cache with different frequencies
    // Let's make "user_session_42" highly active (Hot item)
    for _ in 0..15 {
        sketch.insert(&"user_session_42".to_string(), 1);
    }

    // Let's make "user_session_99" warm
    for _ in 0..5 {
        sketch.insert(&"user_session_99".to_string(), 1);
    }

    // "user_session_7" only gets a single access (Cold item / Scan pollution)
    sketch.insert(&"user_session_7".to_string(), 1);

    println!("Data successfully processed into the Count-Min Sketch.\n");

    println!("=== Phase 2: Estimating Frequencies ===");

    // 2. Querying the sketch for admission evaluation
    // This replicates what your TinyLFU admission controller will do!
    let freq_hot = sketch.count(&"user_session_42".to_string());
    let freq_warm = sketch.count(&"user_session_99".to_string());
    let freq_cold = sketch.count(&"user_session_7".to_string());
    let freq_ghost = sketch.count(&"non_existent_key".to_string());

    println!("Frequency of 'user_session_42' (Expected 15): {}", freq_hot);
    println!("Frequency of 'user_session_99' (Expected 5):  {}", freq_warm);
    println!("Frequency of 'user_session_7'  (Expected 1):  {}", freq_cold);
    println!("Frequency of 'non_existent_key' (Expected 0): {}", freq_ghost);

    println!("\n=== Phase 3: TinyLFU Decision Simulation ===");

    // Let's simulate a TinyLFU gatekeeper standoff:
    // A cold item currently sitting at your Sieve eviction tail ("user_session_7")
    // is fighting a warm incoming candidate ("user_session_99") for cache space.
    if freq_warm > freq_cold {
        println!("Decision: ADMIT 'user_session_99' and EVICT 'user_session_7'.");
    } else {
        println!("Decision: REJECT the incoming candidate.");
    }
}
