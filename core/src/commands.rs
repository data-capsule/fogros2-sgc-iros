extern crate tokio;
extern crate tokio_core;
use std::env;
use std::time::Duration;

use crate::connection_rib::connection_router;

use crate::network::dtls::{dtls_listener, dtls_test_client, dtls_to_peer};
use crate::network::tcp::{tcp_listener, tcp_to_peer};
use crate::structs::GDPStatus;
use crate::topic_manager::ros_topic_manager;
use futures::future;
use tokio::sync::mpsc::{self};

use tokio::time::sleep;
use utils::app_config::AppConfig;
use utils::error::Result;

/// inspired by https://stackoverflow.com/questions/71314504/how-do-i-simultaneously-read-messages-from-multiple-tokio-channels-in-a-single-t
/// TODO: later put to another file
#[tokio::main]
async fn router_async_loop() {
    let config = AppConfig::fetch().expect("App config unable to load");
    info!("{:#?}", config);
    let mut future_handles = Vec::new();

    let mut peer_with_gateway = config.peer_with_gateway;
    let default_gateway_ip = match env::var_os("GATEWAY_IP") {
        Some(gateway_ip) => {
            info!("override to connect gateway to be true");
            info!("Replace Gateway IP with {}", gateway_ip.to_string_lossy());
            peer_with_gateway = true;
            gateway_ip
                .into_string()
                .expect("Gateway IP is not a valid string")
        }
        None => config.default_gateway,
    };

    // initialize the address binding
    let all_addr = "0.0.0.0"; // optionally use [::0] for ipv6 address
    let tcp_bind_addr = format!("{}:{}", all_addr, config.tcp_port);
    let dtls_bind_addr = format!("{}:{}", all_addr, config.dtls_port);

    let default_gateway_addr = match config.ros_protocol.as_str() {
        "dtls" => format!("{}:{}", default_gateway_ip, config.dtls_port),
        "tcp" => format!("{}:{}", default_gateway_ip, config.tcp_port),
        _ => panic!("Unknown protocol"),
    };

    // rib_rx <GDPPacket = [u8]>: forward gdppacket to rib
    let (rib_tx, rib_rx) = mpsc::unbounded_channel();
    // channel_tx <GDPChannel = <gdp_name, sender>>: forward channel maping to rib
    let (channel_tx, channel_rx) = mpsc::unbounded_channel();
    // stat_tx <GdpUpdate proto>: any status update from other routers
    let (stat_tx, stat_rx) = mpsc::unbounded_channel();

    let tcp_sender_handle = tokio::spawn(tcp_listener(
        tcp_bind_addr,
        rib_tx.clone(),
        channel_tx.clone(),
    ));
    future_handles.push(tcp_sender_handle);

    let dtls_sender_handle = tokio::spawn(dtls_listener(
        dtls_bind_addr,
        rib_tx.clone(),
        channel_tx.clone(),
    ));
    future_handles.push(dtls_sender_handle);

    // grpc
    // TODO: uncomment for grpc
    // let psl_service = GDPService {
    //     rib_tx: rib_tx.clone(),
    //     status_tx: stat_tx,
    // };

    // let serve = Server::builder()
    //     .add_service(GlobaldataplaneServer::new(psl_service))
    //     .serve(grpc_bind_addr.parse().unwrap());
    // let manager_handle = tokio::spawn(async move {
    //     if let Err(e) = serve.await {
    //         eprintln!("Error = {:?}", e);
    //     }
    // });
    // let grpc_server_handle = manager_handle;
    // future_handles.push(grpc_server_handle);

    let rib_handle = tokio::spawn(connection_router(
        rib_rx,     // receive packets to forward
        stat_rx,    // recevie control place info, e.g. routing
        channel_rx, // receive channel information for connection rib
    ));
    future_handles.push(rib_handle);

    let ros_topic_manager_handle = tokio::spawn(ros_topic_manager(
        peer_with_gateway,
        default_gateway_addr.clone(),
        rib_tx.clone(),
        channel_tx.clone(),
    ));
    future_handles.push(ros_topic_manager_handle);

    // This connection is only used to advertise the routing information
    // to the gateway. It is not used to forward any packets.
    // the packets are forwarded by direct peer connections in ros_topic_manager
    if peer_with_gateway {
        let (m_tx, m_rx) = mpsc::unbounded_channel();
        if config.ros_protocol == "dtls" {
            let peer_advertisement = tokio::spawn(dtls_to_peer(
                default_gateway_addr.clone().into(),
                rib_tx.clone(),
                channel_tx.clone(),
                m_tx.clone(),
                m_rx,
            ));
            future_handles.push(peer_advertisement);
        } else if config.ros_protocol == "tcp" {
            let peer_advertisement = tokio::spawn(tcp_to_peer(
                default_gateway_addr.clone().into(),
                rib_tx.clone(),
                channel_tx.clone(),
                m_tx.clone(),
                m_rx,
            ));
            future_handles.push(peer_advertisement);
        }

        // only non-gateway proxy needs to advertise themselves
        // it needs the connection to be settled first, otherwise the advertisement is lost
        sleep(Duration::from_millis(1000)).await;
        info!("Flushing the RIB....");
        stat_tx
            .send(GDPStatus { sink: m_tx })
            .expect("Flush the RIB Failure");
    }

    future::join_all(future_handles).await;
}

/// Show the configuration file
pub fn router() -> Result<()> {
    warn!("router is started!");

    // NOTE: uncomment to use pnet
    // libpnet::pnet_proc_loop();
    router_async_loop();

    Ok(())
}

/// Show the configuration file
pub fn config() -> Result<()> {
    let config = AppConfig::fetch()?;
    info!("{:#?}", config);

    Ok(())
}

/// Simulate an error
pub fn simulate_error() -> Result<()> {
    let config = AppConfig::fetch().expect("App config unable to load");
    info!("{:#?}", config);
    // test_cert();
    // get address from default gateway

    // ros_sample();
    // TODO: uncomment them
    let test_router_addr = format!("{}:{}", config.default_gateway, config.dtls_port);
    println!("{}", test_router_addr);
    dtls_test_client("128.32.37.48:9232".into()).expect("DLTS Client error");
    Ok(())
}
