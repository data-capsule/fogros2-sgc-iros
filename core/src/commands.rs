extern crate tokio;
extern crate tokio_core;



use crate::network::tcp::tcp_listener;
use crate::remote_rib;
use crate::structs::{GDPPacket, GDPChannel};
use futures::future;

use tokio::sync::mpsc::{self, Sender, Receiver};
use utils::app_config::AppConfig;
use utils::error::Result;

use crate::connection_rib::connection_router;
use crate::network::dtls::{dtls_listener, dtls_test_client, connect_target};

use local_ip_address::local_ip;


const DTLS_ADDR: &'static str = "127.0.0.1:9232";
const RIB_DTLS_ADDR: &'static str = "127.0.0.1:9233";

/// inspired by https://stackoverflow.com/questions/71314504/how-do-i-simultaneously-read-messages-from-multiple-tokio-channels-in-a-single-t
/// TODO: later put to another file
#[tokio::main]
async fn router_async_loop(target_rib_addr: Option<String>, remote_rib_name: Option<u32>) {
    // rib_rx <GDPPacket = [u8]>: forward gdppacket to rib
    let (rib_tx, rib_rx) = mpsc::channel(32);
    // channel_tx <GDPChannel = <gdp_name, sender>>: forward channel maping to rib
    let (channel_tx, channel_rx) = mpsc::channel(32);

    // connect to the target_rib with dTLS if specified in argument
    if target_rib_addr.is_some() && remote_rib_name.is_some() {
        let dtls_target_addr = target_rib_addr.unwrap();
        let remote_rib_name = remote_rib_name.unwrap();
        AppConfig::set("remote_rib_name", &remote_rib_name.to_string()).expect("Unable to set config item: remote_rib_name");
        tokio::spawn(connect_target(
            remote_rib_name,
            dtls_target_addr,
            rib_tx.clone(),
            channel_tx.clone()
        ));
    }
    

    let tcp_sender_handle = tokio::spawn(tcp_listener(
        "127.0.0.1:9997",
        rib_tx.clone(),
        channel_tx.clone(),
    ));

    let dtls_sender_handle =
        tokio::spawn(dtls_listener(DTLS_ADDR, rib_tx.clone(), channel_tx.clone(), false));
    let rib_handle = tokio::spawn(connection_router(rib_rx, channel_rx, AppConfig::get::<u32>("router_name").expect("Unable to get Config item: router_name")));

    future::join_all([tcp_sender_handle, rib_handle, dtls_sender_handle]).await;
    //join!(foo_sender_handle, bar_sender_handle, receive_handle);
}




#[tokio::main]
pub async fn rib_aynsc_loop() {
    
    // rib_rx <GDPPacket = [u8]>: forward gdppacket to rib
    let (rib_tx, rib_rx): (Sender<GDPPacket>, Receiver<GDPPacket>) = mpsc::channel(32);
    // channel_tx <GDPChannel = <gdp_name, sender>>: forward channel maping to rib
    let (channel_tx, channel_rx): (Sender<GDPChannel>, Receiver<GDPChannel>) = mpsc::channel(32);
    // spawn dtls listener and handler dispatcher loop
    let dtls_sender_handle = tokio::spawn(dtls_listener(RIB_DTLS_ADDR, rib_tx.clone(), channel_tx.clone(), true));
    // spawn dtls processing loop
    let request_proc_handle = tokio::spawn(remote_rib::process_rib_request(rib_rx, channel_rx));

    futures::future::join_all([dtls_sender_handle, request_proc_handle]).await;
        
}

/// Show the configuration file
pub fn router(router_name: u32, target_rib: Option<String>, remote_rib_name: Option<u32>) -> Result<()> {
    warn!("router is started!");
    
    if target_rib.is_some() && remote_rib_name.is_some() {
        println!("router will connect to domain rib with name {:?} at {:?}", remote_rib_name.as_ref().unwrap(), target_rib.as_ref().unwrap());
    } else {
        dbg!("Current router is not connected to a domain rib");
    }
    AppConfig::set("router_name", &router_name.to_string()).expect("Unable to set config item: router_name");
    let local_ip = local_ip().unwrap().to_string();
    AppConfig::set("local_ip", &local_ip).expect("Unable to set config item: local_ip");
    router_async_loop(target_rib, remote_rib_name);

    Ok(())
}

// Start the remote rib
pub fn rib(target_rib: Option<String>) -> Result<()> {
    
    if target_rib.is_some() {
        println!("rib will connect to parent rib at {:?}:9232", target_rib.unwrap());
    } else {
        println!("target_rib is none, this is considered as top-tier RIB");
    }
    // todo: add hierarchical RIB
    rib_aynsc_loop();

    Ok(())
}

/// Show the configuration file
pub fn config() -> Result<()> {
    let config = AppConfig::fetch()?;
    println!("{:#?}", config);

    Ok(())
}

/// Simulate an error
pub fn simulate_error() -> Result<()> {
    // Log this Error simulation
    info!("We are simulating an error");
    // test_cert();
    dtls_test_client(DTLS_ADDR).expect("DLTS Client error");
    Ok(())
}
