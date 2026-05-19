use zipf::ZipfDistribution;
use rand::distributions::Distribution;
use eviction::Cache;
use rand::SeedableRng;
use rand::rngs::StdRng;
use plotters::prelude::*;
use plotters::style::text_anchor::{HPos, Pos, VPos};
use textplots::{Chart, Plot, Shape};
use tabular::{Table, Row};
use rand::Rng;

use std::fmt::Write as FmtWrite;
use std::fs::OpenOptions;
use std::io::Write as IoWrite;
use std::time::Instant;
use std::io::{self};

use eviction::algorithms::bounded_map::MemoryBoundedMap;
use eviction::algorithms::sieve::SieveMap;
use eviction::algorithms::lru::LruCache;
use eviction::algorithms::segmented_lru::SegmentedLruCache;
use eviction::algorithms::w_tiny_lfu::WTinyLfu;


#[derive(Debug)]
struct CacheConfig {
    protected_percentage: f32,
    protected: usize,
    probation: usize,
    eviction_trigger: usize,
    zipf_score: f32,
    eviction_budget: usize,
}

#[derive(Debug)]
struct Workload {
    num_keys: usize,
    total_ops: usize,
    total_cache_capacity: usize,
    eviction_budget: usize,
    name: String,
}

#[derive(Debug)]
struct PlotIndex {
    peak_key_size: f32,
    zipf_hotkey_size: f32,
    zipf_percent: f32,
}

#[derive(Debug)]
struct Performance {
    throughput: f64,
    duration: f64,
    avg_latency_ns: f64,
    hit_rate: f32
}

pub fn generate_random_key<R: Rng>(keyspace_size: usize, rng: &mut R) -> u64 {
    // gen_range is right-exclusive, so 0..keyspace_size yields [0, keyspace_size - 1]
    rng.gen_range(0..keyspace_size) as u64
}

fn run_workload(cache: &mut dyn Cache, num_keys: usize, total_ops: usize, cold_keys: usize, workload_name: String) -> Performance {
    println!(" ---------------- {} : {} =>", cache.name(), workload_name);
    let mut rng = StdRng::seed_from_u64(42);

    // Zipf s=1.03 is the industry standard for database/cache benchmarks (YCSB)
    // We use a key universe larger than the cache (10,000 keys for a 2,000 capacity)
    let zipf = ZipfDistribution::new(num_keys, 1.03).unwrap();
    let mut hits = 0;
    let mut miss = 0;

    //Sequential fill
    for i in 0..cold_keys {
        cache.upsert(format!("key_{}", i), "value".into());
    }

    //Warming up the cache
    for _ in 0..num_keys*2 {
        let idx = zipf.sample(&mut rng) - 1;
        let key = format!("key_{}", idx);

        if let Some(_) = cache.get(&key) {
        } else {
            cache.upsert(key, "value".into());
        }
    }

    // PHASE 2: ZIPFIAN WORKLOAD
    // println!("--- Phase 2: Zipfian Traffic...\n\n");

    // Start the stopwatch right before the loop begins
    let start_time = Instant::now();

    for i in 0..total_ops {
        if rng.gen_bool(0.3) { // 30% Random Writes
            let random_key = generate_random_key(num_keys, &mut rng);
            cache.upsert(random_key.to_string(), "value".into());
        } else { // 70% Zipfian Reads
            let idx = zipf.sample(&mut rng) - 1;
            let key = format!("key_{}", idx);

            if let Some(_) = cache.get(&key) {
                hits += 1;
            } else {
                miss += 1;
                cache.upsert(key, "value".into());
            }

        }

        // Print intermediate stats every 10k ops
        if (i+1) % (total_ops/10) == 0 {

            print_progress_bar(i+1, total_ops, 10);
        }

    }
    print_progress_bar(total_ops, total_ops, 10);
    let avg_miss = miss / (total_ops/num_keys);

    // Stop the stopwatch as soon as the loop ends
    let total_duration = start_time.elapsed();

    let hit_rate = (hits as f32 / total_ops as f32) * 100.0;

    // Calculate throughput metrics
    let total_secs = total_duration.as_secs_f64();
    let ops_per_second = total_ops as f64 / total_secs;
    let avg_latency_ns = total_duration.as_nanos() as f64 / total_ops as f64;
    let perf = Performance {
        throughput: ops_per_second,
        duration: total_secs,
        avg_latency_ns: avg_latency_ns,
        hit_rate: hit_rate,
    };
    println!("Hit Rate: {:.2}%", hit_rate);
    perf
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut workload_data: Vec<Workload> = Vec::new();
    workload_data.push(Workload {
        num_keys: 1_000_000,
        total_ops: 10_000_000,
        total_cache_capacity: 250_000,
        eviction_budget: 1,
        name: "SMALL LOAD".to_string(),
    });

    workload_data.push(Workload{
        num_keys: 5_000_000,
        total_ops: 50_000_000,
        total_cache_capacity: 1_250_000,
        eviction_budget: 1,
        name: "MEDIUM LOAD".to_string(),
    });

    // workload_data.push(Workload{
    //     num_keys: 10_000_000,
    //     total_ops: 100_000_000,
    //     total_cache_capacity: 2_500_000,
    //     eviction_budget: 1,
    //     name: "LARGE LOAD".to_string(),
    // });

    // workload_data.push(Workload{
    //     num_keys: 20_000_000,
    //     total_ops: 200_000_000,
    //     total_cache_capacity: 5_000_000,
    //     eviction_budget: 1,
    //     name: "VERY LARGE LOAD".to_string(),
    // });

    let protected_segment_ratio = 0.65; // Obtained by running cache_bench.rs. We took the average of 4 type of load we ran last time.

    let mut perf_segmented_sieve: Vec<(String, Performance)> = Vec::new();
    let mut perf_sieve: Vec<(String, Performance)> = Vec::new();
    let mut perf_lru: Vec<(String, Performance)> = Vec::new();
    let mut perf_segmented_lru: Vec<(String, Performance)> = Vec::new();
    let mut perf_w_tiny_lfu: Vec<(String, Performance)> = Vec::new();

    let mut hit_rate: f32;
    let mut perf;
    for config in workload_data {
        let workload_name = config.name.clone();
        // Run the workload on segmented sieve
        let protected_size = (protected_segment_ratio * config.total_cache_capacity as f32) as usize;
        let mut b_map = MemoryBoundedMap::new(protected_size, protected_size - 100, config.total_cache_capacity - protected_size, config.eviction_budget, 1);
        perf = run_workload(&mut b_map, config.num_keys, config.total_ops, config.total_cache_capacity, workload_name.clone());
        // result_segmented_sieve.push((workload_name.clone(), hit_rate));
        perf_segmented_sieve.push((workload_name.clone(), perf));

        // Run the workload on simple sieve algorithm
        let mut sieve = SieveMap::new(config.total_cache_capacity, config.total_cache_capacity - 200, config.eviction_budget);
        perf = run_workload(&mut sieve, config.num_keys, config.total_ops, config.total_cache_capacity, workload_name.clone());
        // result_sieve.push((workload_name.clone(), hit_rate));
        perf_sieve.push((workload_name.clone(), perf));


        //Run the workload on LRU eviction
        let mut lru = LruCache::new(config.total_cache_capacity, config.total_cache_capacity - 200, config.eviction_budget);
        perf = run_workload(&mut lru, config.num_keys, config.total_ops, config.total_cache_capacity, workload_name.clone());
        // result_lru.push((workload_name.clone(), hit_rate));
        perf_lru.push((workload_name.clone(), perf));


        //Run the workload on Segmented LRU eviction
        let probation_size = config.total_cache_capacity - protected_size;
        let mut segmented_lru = SegmentedLruCache::new(config.total_cache_capacity, protected_size, probation_size, probation_size - 100, protected_size - 100, config.eviction_budget, config.eviction_budget, 1);
        perf = run_workload(&mut segmented_lru, config.num_keys, config.total_ops, config.total_cache_capacity, workload_name.clone());
        // result_segmented_lru.push((workload_name.clone(), hit_rate));
        perf_segmented_lru.push((workload_name.clone(), perf));

        //Run the workload on w tiny lfu eviction
        let mut b_map = WTinyLfu::new(protected_size, protected_size - 100, config.total_cache_capacity - protected_size, config.eviction_budget);
        perf = run_workload(&mut b_map, config.num_keys, config.total_ops, config.total_cache_capacity, workload_name.clone());
        // result_segmented_sieve.push((workload_name.clone(), hit_rate));
        perf_w_tiny_lfu.push((workload_name.clone(), perf));

    }

    let performances = [
        ("Segmented Sieve", perf_segmented_sieve),
        ("Sieve", perf_sieve),
        ("LRU", perf_lru),
        ("Segmented LRU", perf_segmented_lru),
        ("W Tiny LFU", perf_w_tiny_lfu),
    ];
    plot_perf(&performances)?;
    plot_throughput(&performances)?;
    plot_average_latency(&performances)?;
    print_performance_table(&performances);
    Ok(())
}

use plotters::prelude::*;

fn plot_perf(
    algorithms: &[(&str, Vec<(String, Performance)>)],
) -> Result<(), Box<dyn std::error::Error>> {
    let file_path = "algorithm_comparison_65_2.png";
    let root = BitMapBackend::new(file_path, (840, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    if algorithms.is_empty() || algorithms[0].1.is_empty() {
        return Ok(());
    }

    // Extract categories ("SMALL LOAD", "MEDIUM LOAD", etc.)
    let labels: Vec<String> = algorithms[0].1.iter().map(|x| x.0.clone()).collect();
    let num_categories = labels.len();
    let x_min = -0.5f32;
    let x_max = if num_categories > 1 {
        num_categories as f32 - 0.5f32
    } else {
        0.5f32
    };
    let all_values: Vec<f32> = algorithms
        .iter()
        .flat_map(|(_, dataset)| dataset.iter().map(|(_, perf)| perf.hit_rate))
        .collect();
    let min_value = all_values.iter().copied().fold(f32::INFINITY, f32::min);
    let max_value = all_values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let spread = (max_value - min_value).max(1.0);
    let y_padding = (spread * 0.35).max(1.0);
    let y_min = (min_value - y_padding).max(0.0);
    let y_max = (max_value + y_padding).min(100.0);
    let label_step = (spread * 0.04).max(0.15); // Fine-tuned step size for compact ranges

    let mut chart = ChartBuilder::on(&root)
        .caption("Eviction Policy Comparison: Hit Rate vs Load Scale", ("roboto", 25))
        .margin(50)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(x_min..x_max, y_min..y_max)?;

    chart.configure_mesh()
        .disable_x_mesh()
        .y_desc("Hit Rate %")
        .x_desc("Workload Scale")
        .x_labels(num_categories)
        .x_label_formatter(&|x| {
            let idx = x.round() as usize;
            labels.get(idx).cloned().unwrap_or_default()
        })
        .draw()?;

    for (algo_idx, (algo_name, dataset)) in algorithms.iter().enumerate() {
        // FIXED: Explicitly map all 4 algorithms to 4 distinct colors
        let base_color = match algo_idx {
            0 => RED,
            1 => BLUE,
            2 => MAGENTA, // Distinct color for LRU
            _ => BLACK,   // Segmented LRU remains Black
        };
        let line_style = ShapeStyle::from(&base_color).stroke_width(1);

        // FIXED: Alternate labels above/below node points to eliminate overlap collisions
        let (v_pos, y_modifier) = if algo_idx % 2 == 0 {
            (VPos::Bottom, label_step)
        } else {
            (VPos::Top, -label_step)
        };

        // 1. Draw the connecting lines
        chart.draw_series(LineSeries::new(
            dataset
                .iter()
                .enumerate()
                .map(|(i, (_, perf))| (i as f32, perf.hit_rate)),
            line_style,
        ))?
        .label(*algo_name)
        .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], line_style));

        // 2. Draw colored dots at each node
        chart.draw_series(dataset.iter().enumerate().map(|(i, (_, perf))| {
            Circle::new((i as f32, perf.hit_rate), 5, ShapeStyle::from(&base_color).filled())
        }))?;

        // 3. Float the numeric hit-rate values right above/below the points
        chart.draw_series(dataset.iter().enumerate().map(|(i, (_, perf))| {
            Text::new(
                format!("{:.2}%", perf.hit_rate),
                (i as f32, perf.hit_rate + y_modifier),
                ("roboto", 11)
                    .into_font()
                    .color(&base_color)
                    .pos(Pos::new(HPos::Center, v_pos)),
            )
        }))?;
    }

    chart.configure_series_labels()
        .position(SeriesLabelPosition::UpperLeft)
        .background_style(WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;

    root.present()?;
    Ok(())
}

fn plot_throughput(
    performances: &[(&str, Vec<(String, Performance)>)],
) -> Result<(), Box<dyn std::error::Error>> {
    let file_path = "algorithm_throughput_comparison.png";
    let root = BitMapBackend::new(file_path, (840, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    if performances.is_empty() || performances[0].1.is_empty() {
        return Ok(());
    }

    let labels: Vec<String> = performances[0].1.iter().map(|x| x.0.clone()).collect();
    let num_categories = labels.len();
    let x_min = -0.5f64;
    let x_max = if num_categories > 1 {
        num_categories as f64 - 0.5f64
    } else {
        0.5f64
    };
    let all_values: Vec<f64> = performances
        .iter()
        .flat_map(|(_, dataset)| dataset.iter().map(|(_, perf)| perf.throughput))
        .collect();
    let min_value = all_values.iter().copied().fold(f64::INFINITY, f64::min);
    let max_value = all_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let spread = (max_value - min_value).max(1.0);
    let y_padding = (spread * 0.35).max(1.0);
    let y_min = (min_value - y_padding).max(0.0);
    let y_max = max_value + y_padding;
    let label_step = (spread * 0.04).max(1.0);

    let mut chart = ChartBuilder::on(&root)
        .caption("Eviction Policy Comparison: Throughput vs Load Scale", ("roboto", 25))
        .margin(50)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(x_min..x_max, y_min..y_max)?;

    chart.configure_mesh()
        .disable_x_mesh()
        .y_desc("Throughput (ops/sec)")
        .x_desc("Workload Scale")
        .x_labels(num_categories)
        .x_label_formatter(&|x| {
            let idx = x.round() as usize;
            labels.get(idx).cloned().unwrap_or_default()
        })
        .draw()?;

    for (algo_idx, (algo_name, dataset)) in performances.iter().enumerate() {
        // FIXED: Explicitly map all 4 algorithms to 4 distinct colors
        let base_color = match algo_idx {
            0 => RED,
            1 => BLUE,
            2 => MAGENTA,
            _ => BLACK,
        };
        let line_style = ShapeStyle::from(&base_color).stroke_width(1);

        // FIXED: Alternate labels above/below node points to eliminate overlap collisions
        let (v_pos, y_modifier) = if algo_idx % 2 == 0 {
            (VPos::Bottom, label_step)
        } else {
            (VPos::Top, -label_step)
        };

        chart.draw_series(LineSeries::new(
            dataset.iter().enumerate().map(|(i, val)| (i as f64, val.1.throughput)),
            line_style,
        ))?
        .label(*algo_name)
        .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], line_style));

        chart.draw_series(dataset.iter().enumerate().map(|(i, val)| {
            Circle::new((i as f64, val.1.throughput), 5, ShapeStyle::from(&base_color).filled())
        }))?;

        chart.draw_series(dataset.iter().enumerate().map(|(i, val)| {
            Text::new(
                format!("{:.2}", val.1.throughput),
                (i as f64, val.1.throughput + y_modifier),
                ("roboto", 11)
                    .into_font()
                    .color(&base_color)
                    .pos(Pos::new(HPos::Center, v_pos)),
            )
        }))?;
    }

    chart.configure_series_labels()
        .position(SeriesLabelPosition::UpperLeft)
        .background_style(WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;

    root.present()?;
    Ok(())
}

fn plot_average_latency(
    performances: &[(&str, Vec<(String, Performance)>)],
) -> Result<(), Box<dyn std::error::Error>> {
    let file_path = "algorithm_latency_comparison.png";
    let root = BitMapBackend::new(file_path, (840, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    if performances.is_empty() || performances[0].1.is_empty() {
        return Ok(());
    }

    let labels: Vec<String> = performances[0].1.iter().map(|x| x.0.clone()).collect();
    let num_categories = labels.len();
    let x_min = -0.5f64;
    let x_max = if num_categories > 1 {
        num_categories as f64 - 0.5f64
    } else {
        0.5f64
    };
    let all_values: Vec<f64> = performances
        .iter()
        .flat_map(|(_, dataset)| dataset.iter().map(|(_, perf)| perf.avg_latency_ns))
        .collect();
    let min_value = all_values.iter().copied().fold(f64::INFINITY, f64::min);
    let max_value = all_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let spread = (max_value - min_value).max(1.0);
    let y_padding = (spread * 0.35).max(1.0);
    let y_min = (min_value - y_padding).max(0.0);
    let y_max = max_value + y_padding;
    let label_step = (spread * 0.04).max(1.0);

    let mut chart = ChartBuilder::on(&root)
        .caption("Eviction Policy Comparison: Avg Latency vs Load Scale", ("roboto", 25))
        .margin(50)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(x_min..x_max, y_min..y_max)?;

    chart.configure_mesh()
        .disable_x_mesh()
        .y_desc("Average Latency (ns)")
        .x_desc("Workload Scale")
        .x_labels(num_categories)
        .x_label_formatter(&|x| {
            let idx = x.round() as usize;
            labels.get(idx).cloned().unwrap_or_default()
        })
        .draw()?;

    for (algo_idx, (algo_name, dataset)) in performances.iter().enumerate() {
        // FIXED: Explicitly map all 4 algorithms to 4 distinct colors
        let base_color = match algo_idx {
            0 => RED,
            1 => BLUE,
            2 => MAGENTA,
            _ => BLACK,
        };
        let line_style = ShapeStyle::from(&base_color).stroke_width(1);

        // FIXED: Alternate labels above/below node points to eliminate overlap collisions
        let (v_pos, y_modifier) = if algo_idx % 2 == 0 {
            (VPos::Bottom, label_step)
        } else {
            (VPos::Top, -label_step)
        };

        chart.draw_series(LineSeries::new(
            dataset.iter().enumerate().map(|(i, val)| (i as f64, val.1.avg_latency_ns)),
            line_style,
        ))?
        .label(*algo_name)
        .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], line_style));

        chart.draw_series(dataset.iter().enumerate().map(|(i, val)| {
            Circle::new((i as f64, val.1.avg_latency_ns), 5, ShapeStyle::from(&base_color).filled())
        }))?;

        chart.draw_series(dataset.iter().enumerate().map(|(i, val)| {
            Text::new(
                format!("{:.2}", val.1.avg_latency_ns),
                (i as f64, val.1.avg_latency_ns + y_modifier),
                ("roboto", 11)
                    .into_font()
                    .color(&base_color)
                    .pos(Pos::new(HPos::Center, v_pos)),
            )
        }))?;
    }

    chart.configure_series_labels()
        .position(SeriesLabelPosition::UpperLeft)
        .background_style(WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()?;

    root.present()?;
    Ok(())
}

/// Renders a smooth, text-based progress bar to stdout.
/// - `current`: Current loop index (e.g., current transaction)
/// - `total`: Total operations in the benchmark run
/// - `width`: Total character width of the progress bar itself (excluding brackets)
fn print_progress_bar(current: usize, total: usize, width: usize) {
    // 1. Calculate how many bar characters need to be filled
    let percentage = (current as f64 / total as f64).clamp(0.0, 1.0);
    let filled_width = (percentage * width as f64).round() as usize;

    // 2. Build the filled and empty string chunks
    let filled = "█".repeat(filled_width);
    let empty = " ".repeat(width - filled_width);

    // 3. Print using '\r' to reset the cursor back to the start of the current line
    // We use print! and flush stdout explicitly so it renders immediately.
    print!(
        "\rProcessing: [{}{}] {:>3.0}% ({}/{})",
        filled,
        empty,
        percentage * 100.0,
        current,
        total
    );
    let _ = io::stdout().flush();
}

fn print_performance_table(
    // Accepts a slice of tuples mapping the algorithm name to its performance vector
    datasets: &[(&str, Vec<(String, Performance)>)]
) {
    // Print Table Header
    println!(
        "{:<18} | {:<12} | {:<12} | {:<14} | {:<16} | {:<16}",
        "Algorithm Type", "Workload", "Hit Rate (%)", "Run Time (ms)", "Throughput (QPS)", "Avg Latency (us)"
    );
    println!("{:-<103}", ""); // Clean separator line

    for (algo_name, workload_reports) in datasets {
        for (i, (workload_size, perf)) in workload_reports.iter().enumerate() {

            // Only print the algorithm name on the very first row of its block
            let display_name = if i == 0 { *algo_name } else { "" };

            // Unit conversions for human readability
            let run_time_ms = perf.duration * 1000.0;
            let avg_latency_us = perf.avg_latency_ns / 1000.0;

            println!(
                "{:<18} | {:<12} | {:<12.2} | {:<14.2} | {:<16.2} | {:<16.2}",
                display_name,
                workload_size,
                perf.hit_rate,
                run_time_ms,
                perf.throughput,
                avg_latency_us
            );
        }

        // Visual break between algorithm blocks
        println!("{:-<103}", "");
    }
}
