use futures::Future;
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
use r2r; 
use r2r::QosProfile;
use criterion::*;
use criterion::async_executor::FuturesExecutor;
use futures::executor::LocalPool;
use futures::future;
use futures::stream::StreamExt;
use futures::task::LocalSpawnExt;

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

fn r2r_ros_publisher(c: &mut Criterion) {
    let mut group = c.benchmark_group("ros");
    let ctx = r2r::Context::create().unwrap();
    let mut node = r2r::Node::create(ctx, "node", "namespace").unwrap();
    let subscriber =
        node.subscribe::<r2r::std_msgs::msg::String>("/topic", QosProfile::default()).unwrap();
    let publisher =
        node.create_publisher::<r2r::std_msgs::msg::String>("/topic", QosProfile::default()).unwrap();
    let mut timer = node.create_wall_timer(std::time::Duration::from_millis(1000)).unwrap();

    for num in [3, 10, 20].iter() {
        
        // group.bench_with_input(
        //     BenchmarkId::new("loader_chain", num),
        //     &num,
        //     |b, request| b.to_async(FuturesExecutor).iter(|| {
        //         let msg = r2r::std_msgs::msg::String {
        //             data: format!("Hello, world! ()"),
        //         };
        //         publisher.publish(&msg).unwrap();
        //     } 
        // ));
        let x = 3;
        let s = String::from_iter(std::iter::repeat('a').take(x));

        group.bench_with_input(
            BenchmarkId::new("r2r_ros_publisher", num),
            &num,
            |b, request| b.iter(|| {
                let msg = r2r::std_msgs::msg::String {
                    data: s.clone(),
                };
                publisher.publish(&msg).unwrap();
            } 
        ));

    }

    group.finish();
}

fn r2r_ros_subcriber(c: &mut Criterion) {
    let mut group = c.benchmark_group("ros");
    for num in [3, 10, 20].iter() {
        
        let ctx = r2r::Context::create().unwrap();
        let mut node = r2r::Node::create(ctx, "node", "namespace").unwrap();
        let mut subscriber =
            node.subscribe::<r2r::std_msgs::msg::String>("/topic", QosProfile::default()).unwrap();
        let publisher =
            node.create_publisher::<r2r::std_msgs::msg::String>("/topic", QosProfile::default()).unwrap();
        let mut pool = LocalPool::new();
        let spawner = pool.spawner();
        // group.bench_with_input(
        //     BenchmarkId::new("loader_chain", num),
        //     &num,
        //     |b, request| b.to_async(FuturesExecutor).iter(|| {
        //         let msg = r2r::std_msgs::msg::String {
        //             data: format!("Hello, world! ()"),
        //         };
        //         publisher.publish(&msg).unwrap();
        //     } 
        // ));
        let x = 3;
        let s = String::from_iter(std::iter::repeat('a').take(x));
        spawner.spawn_local(async move {
            loop {
                let msg = r2r::std_msgs::msg::String {
                    data: s.clone(),
                };
                publisher.publish(&msg).unwrap();
                println!("published!");
            }
        });

        // group.bench_with_input(
        //     BenchmarkId::new("r2r_ros_subcriber", num),
        //     &num,
        //     |b, request| b.to_async(FuturesExecutor).iter(|| 
        //         {
        //         subscriber.next().await;
        //         future::ready(())
        //     } 
        // ));

        group.bench_with_input(
            BenchmarkId::new("r2r_ros_subcriber", num),
            &num,
            |b, request| b.to_async(FuturesExecutor).iter(|| 
                async move {
                    let ctx = r2r::Context::create().unwrap();
                    let mut node = r2r::Node::create(ctx, "node", "namespace").unwrap();
                    let mut subscriber =
                        node.subscribe::<r2r::std_msgs::msg::String>("/topic", QosProfile::default()).unwrap();
                    subscriber.next().await;
                }
        )
    );
    }

    group.finish();
}


// // benchmark_group!(benches, b, grpc_client, executor_call_loader); // uncomment the grpc_client
criterion_group!(name = benches;
  config =  Criterion::default().with_profiler(perf::FlamegraphProfiler::new(100));
  targets = r2r_ros_publisher,
  r2r_ros_subcriber
);
criterion_main!(benches);