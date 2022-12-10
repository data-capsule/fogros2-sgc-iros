use log::info;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::process::Command;
use tokio;
use utils::app_config::AppConfig;
extern crate criterion;
use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, Bencher, BenchmarkId, Criterion,
    Throughput,
};
mod perf;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn init_app_config() {
    ::std::env::set_var("RUST_LOG", "info");

    // Initialize Configuration
    let include_path = match env::var_os("FAAS_CONFIG") {
        Some(_) => "./src/resources/docker.toml".to_owned(),
        None => "./src/resources/native.toml".to_owned(),
    };
    info!("Using config file : {}", include_path);
    // let config_contents = include_str!(include_path);
    let config_contents = fs::read_to_string(include_path).expect("config file not found!");
    AppConfig::init(Some(&config_contents));
}

fn placeholder(c: &mut Criterion) {
    let mut group = c.benchmark_group("executor");

    for num in [3, 10, 20].iter() {
        
        group.bench_with_input(
            BenchmarkId::new("loader_chain", num + 3),
            &num,
            |b, request| b.iter(|| {} ));
    }

    group.finish();
}


// // benchmark_group!(benches, b, grpc_client, executor_call_loader); // uncomment the grpc_client
criterion_group!(name = benches;
  config =  Criterion::default().with_profiler(perf::FlamegraphProfiler::new(100));
  targets = placeholder
);
criterion_main!(benches);