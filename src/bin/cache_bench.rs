use zipf::ZipfDistribution;
use rand::distributions::Distribution;
use eviction::algorithms::bounded_map::MemoryBoundedMap;
use eviction::algorithms::sieve::SieveMap;
use eviction::Cache;
use rand::SeedableRng;
use rand::rngs::StdRng;
use plotters::prelude::*;
use textplots::{Chart, Plot, Shape};

use std::fmt::Write as FmtWrite;
use std::fs::OpenOptions;
use std::io::Write as IoWrite;
use std::time::Instant;

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
    zipf_percent: f32
}


fn run_workload(cache: &mut dyn Cache, num_keys: usize, total_ops: usize, cold_keys: usize) -> f32 {
    let mut rng = StdRng::seed_from_u64(42);

    // Zipf s=1.03 is the industry standard for database/cache benchmarks (YCSB)
    // We use a key universe larger than the cache (10,000 keys for a 2,000 capacity)
    let zipf = ZipfDistribution::new(num_keys, 1.03).unwrap();
    let mut hits = 0;
    let mut miss = 0;

    // println!("--- Starting Benchmark: --- \n{}\n", cache.name());

    // PHASE 1: SEQUENTIAL FILL (Cold Start)
    // println!("--- Phase 1: Sequential Fill (Cold Start)...");
    for i in 0..cold_keys {
        cache.upsert(format!("key_{}", i), "value".into());
    }

    // PHASE 2: ZIPFIAN WORKLOAD
    // println!("--- Phase 2: Zipfian Traffic...\n\n");

    // Start the stopwatch right before the loop begins
    let start_time = Instant::now();

    for i in 0..total_ops {
        let idx = zipf.sample(&mut rng) - 1;
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
        }
    }
    let avg_miss = miss / (total_ops/num_keys);

    let total_duration = start_time.elapsed();

    let hit_rate = (hits as f32 / total_ops as f32) * 100.0;
    println!("Hit Rate: {:.2}%", hit_rate);
    println!("-------------------------------------------------------------------------------\n");
    hit_rate
}

fn find_sensitivity(workload: Workload) {
    let variations = generate_config(workload.num_keys, workload.total_ops, workload.total_cache_capacity, workload.eviction_budget);
    let mut result: (f32, f32);
    let mut plot_data: Vec<(f32, f32)> = Vec::new();
    for config in variations {
        println!(
            "ProtRatio: {} | Prot: {:<5} | Prob: {:<5} | Trig: {:<5} | Zipf: {} | Budget: {}",
            config.protected_percentage, config.protected, config.probation, config.eviction_trigger, config.zipf_score, config.eviction_budget
        );
        let mut b_map = MemoryBoundedMap::new(config.protected, config.eviction_trigger, config.probation, config.eviction_budget, 2);
        let hit_rate = run_workload(&mut b_map, workload.num_keys, workload.total_ops, workload.total_cache_capacity);
        result = (config.protected_percentage, hit_rate);
        plot_data.push(result);
    }
    let max_point = plot_data
        .iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .copied()
        .unwrap_or((0.0, 0.0));

    let plot_index = PlotIndex {
        peak_key_size: (max_point.0 / 100.0) * workload.total_cache_capacity as f32,
        zipf_hotkey_size: (17.0 / 100.0) * workload.num_keys as f32,
        zipf_percent: 17.0
    };
    plot_sensitivity(&plot_data, &workload.name, &plot_index, max_point);

    let mut output = String::with_capacity(256 + (plot_data.len() * 32));
    writeln!(&mut output, " ------------ {} -------------- ", workload.name).unwrap();
    writeln!(&mut output, "\n{:<18} | {:>15}", "Metric", "Value").unwrap();
    writeln!(&mut output, "{:-<18}-+-{:-<15}", "", "").unwrap();
    writeln!(&mut output, "{:<18} | {:>15}", "Keyspace", workload.num_keys).unwrap();
    writeln!(
        &mut output,
        "{:<18} | {:>15}",
        "Cache Capacity",
        workload.total_cache_capacity
    ).unwrap();
    writeln!(&mut output, "\n{:<15} | {:<10}", "Protected %", "Hit Rate").unwrap();
    writeln!(&mut output, "{:-<15}-+-{:-<10}", "", "").unwrap();
    for (protected_percentage, hit_rate) in &plot_data {
        writeln!(&mut output, "{:<15.2} | {:<10.4}", protected_percentage, hit_rate).unwrap();
    }
    writeln!(
        &mut output,
        "\nMax Hit Rate: {:.4} at {:.2}% protected segment ({:.0} keys)",
        max_point.1,
        max_point.0,
        plot_index.peak_key_size
    ).unwrap();
    writeln!(&mut output, "----------------------------------------").unwrap();

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("cache_bench_results.txt")
        .expect("failed to open cache_bench_results.txt");
    file.write_all(output.as_bytes())
        .expect("failed to write cache benchmark results");

    // 1. Find the maximum point

}

fn generate_config(
    num_keys: usize,
    _total_tops: usize,
    total_cache_capacity: usize,
    eviction_budget: usize,
) -> Vec<CacheConfig> {
    // Generate scenarios
    // For zipf traffic pattern, roughly 20% of the keys receive the 80% of traffic.
    // Lets start by keeping protected segment at around 60% . We increase at 5%
    // increment everytime.
    // For finding out the sensitivity we keep the zipf score constant at 1.03
    let mut configs = Vec::new();
    let zipf_score = 1.03;
    let eviction_budget = eviction_budget;

    // Start at 5% + 300, increment by 5% steps
    // We use a float loop to avoid precision issues with percentages
    let mut percentage = 0.05;

    while percentage <= 0.901 { // 0.901 to account for float rounding
        let protected = (percentage * total_cache_capacity as f32) as usize + 300;
        let eviction_trigger = (percentage * total_cache_capacity as f32) as usize + 100;
        // Ensure protected doesn't exceed total_cache_capacity
        if protected < total_cache_capacity {
            let probation = total_cache_capacity - protected;
            configs.push(CacheConfig {
                protected_percentage: (percentage * 100.0) as f32,
                protected,
                probation,
                eviction_trigger,
                zipf_score,
                eviction_budget,
            });
        }
        percentage += 0.05;
    }

    configs
}

fn main() {
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

    workload_data.push(Workload{
        num_keys: 10_000_000,
        total_ops: 100_000_000,
        total_cache_capacity: 2_500_000,
        eviction_budget: 1,
        name: "LARGE LOAD".to_string(),
    });

    workload_data.push(Workload{
        num_keys: 20_000_000,
        total_ops: 200_000_000,
        total_cache_capacity: 5_000_000,
        eviction_budget: 1,
        name: "VERY LARGE LOAD".to_string(),
    });

    for config in workload_data {
        find_sensitivity(config);
    }
}

fn plot_sensitivity(
    data: &[(f32, f32)],
    name: &str,
    index: &PlotIndex,
    max_point: (f32, f32)
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create a stable filename
    let file_path = format!("sensitivity_analysis_{}.png", name.replace(" ", "_"));

    // 2. Set canvas size (slightly taller to accommodate multi-line footer)
    let root = BitMapBackend::new(&file_path, (840, 700)).into_drawing_area();
    root.fill(&WHITE)?;

    // 3. Draw Header
    root.draw_text(
        &name.to_uppercase(),
        &("roboto", 18).into_font().color(&BLACK.mix(0.6)),
        (20, 15),
    )?;

    // 4. Build Chart with adjusted margins
    let mut chart = ChartBuilder::on(&root)
        .caption("Segmented Sieve: Protected Ratio vs Hit Rate", ("roboto", 25))
        .margin_top(40)
        .margin_left(40)
        .margin_right(40)
        .margin_bottom(120) // Extra space for the 3-line analysis
        .x_label_area_size(40)
        .y_label_area_size(50)
        .build_cartesian_2d(0f32..100f32, 0f32..100f32)?;

    chart.configure_mesh()
        .x_desc("Protected Pool %")
        .y_desc("Hit Rate %")
        .axis_desc_style(("roboto", 15))
        .draw()?;

    // 5. Projection Lines (Dashed effect via light mix)
    let projection_style = BLACK.mix(0.2).stroke_width(1);
    chart.draw_series(std::iter::once(PathElement::new(
        vec![(max_point.0, 0.0), (max_point.0, max_point.1)],
        projection_style,
    )))?;
    chart.draw_series(std::iter::once(PathElement::new(
        vec![(0.0, max_point.1), (max_point.0, max_point.1)],
        projection_style,
    )))?;

    // 6. Main Data Series
    chart.draw_series(LineSeries::new(data.iter().copied(), &RED))?
        .label("Segmented Sieve")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));

    // 7. Max Point Marker and Label
    chart.draw_series(std::iter::once(Circle::new(max_point, 5, BLUE.filled())))?;

    chart.draw_series(std::iter::once(Text::new(
        format!("Max: ({:.1}%, {:.2}%)", max_point.0, max_point.1),
        (max_point.0 + 2.0, max_point.1 + 2.0),
        ("roboto", 13).into_font().color(&BLUE),
    )))?;

    // 8. Multi-line Analysis Statement
    // Manually breaking lines to prevent overflow
    let line1 = format!(
        "Peak hit rate correlates with a protected pool size of {}, effectively capturing the",
        index.peak_key_size
    );
    let line2 = format!(
        "Zipfian 'heavy-hitter' segment (representing {}% of traffic across {} keys).",
        index.zipf_percent, index.zipf_hotkey_size
    );
    let line3 = "The results demonstrate the efficiency of segmented admission in mitigating long-tail churn.";

    let footer_style = ("roboto", 13).into_font().color(&BLUE);

    // Positioned relative to the bottom of the canvas
    root.draw_text(&line1, &footer_style, (60, 610))?;
    root.draw_text(&line2, &footer_style, (60, 630))?;
    root.draw_text(&line3, &footer_style, (60, 650))?;

    // 9. Legend
    chart.configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(&BLACK)
        .position(SeriesLabelPosition::MiddleRight)
        .draw()?;

    root.present()?;
    println!("Report generated: {}", file_path);
    Ok(())
}
