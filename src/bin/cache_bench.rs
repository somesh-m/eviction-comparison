use zipf::ZipfDistribution;
use rand::distributions::Distribution;
use eviction::algorithms::bounded_map::MemoryBoundedMap;
use eviction::algorithms::sieve::SieveMap;
use eviction::Cache;

fn run_workload(cache: &mut dyn Cache, num_keys: usize, total_ops: usize, cold_keys: usize) {
    let mut rng = rand::thread_rng();

    // Zipf s=1.03 is the industry standard for database/cache benchmarks (YCSB)
    // We use a key universe larger than the cache (10,000 keys for a 2,000 capacity)
    let key_universe = num_keys * 4;
    let zipf = ZipfDistribution::new(num_keys, 1.03).unwrap();
    let mut hits = 0;
    let mut miss = 0;

    println!("--- Starting Benchmark: --- \n{}\n", cache.name());

    // PHASE 1: SEQUENTIAL FILL (Cold Start)
    println!("--- Phase 1: Sequential Fill (Cold Start)...");
    for i in 0..cold_keys {
        cache.upsert(format!("key_{}", i), "value".into());
    }

    // PHASE 2: ZIPFIAN WORKLOAD
    // We run total_ops number of operations.
    println!("--- Phase 2: Zipfian Traffic...\n\n");
    for i in 0..total_ops {
        let idx = zipf.sample(&mut rng);
        let key = format!("key_{}", idx);

        if let Some(_) = cache.get(&key) {
            hits += 1;
        } else {
            miss += 1;
            cache.upsert(key, "value".into());
        }

        // Print intermediate stats every 10k ops
        if (i+1) % (total_ops/5) == 0 {
            println!(" --- Progress {:.0}% ...", ((i+1) as f64 / total_ops as f64) * 100.0);
            println!(" --- Hits => {0} Miss => {1}", hits, miss);
        }
    }

    let hit_rate = (hits as f64 / total_ops as f64) * 100.0;
    println!("\n -- RESULTS ---");
    println!("Hit Rate: {:.2}%\n", hit_rate);
    cache.stats();
    println!("-------------------------------------------------------------------------------\n");
}

fn main() {
    let num_keys = 25_000;
    let total_ops = 10_000_000;

    let total_cache_capacity = 10_000;

    let protected_size = 2_500;
    let protected_trigger = 2_250;
    let probation_size = 7_500;
    let eviction_budget = 100;

    // Add your different implementations to this list
    let mut b_map = MemoryBoundedMap::new(protected_size, protected_trigger, probation_size, eviction_budget);
    let total_size = protected_size + probation_size;
    let mut sieve = SieveMap::new(total_size, total_size - 1000, eviction_budget);

    run_workload(&mut b_map, num_keys, total_ops, probation_size);

    run_workload(&mut sieve, num_keys, total_ops, probation_size);

}
