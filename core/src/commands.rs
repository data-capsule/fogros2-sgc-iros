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
use crate::network::dtls::{dtls_listener, dtls_test_client, connect_rib};

use local_ip_address::local_ip;


// const DTLS_ADDR: &'static str = "127.0.0.1:9232";
const RIB_DTLS_ADDR: &'static str = "127.0.0.1:9233";

/// inspired by https://stackoverflow.com/questions/71314504/how-do-i-simultaneously-read-messages-from-multiple-tokio-channels-in-a-single-t
/// TODO: later put to another file
#[tokio::main]
async fn router_async_loop(target_rib_addr: Option<String>, remote_rib_name: Option<u32>) {
    // This is for tokio-console, the debugger for tokio
    // if AppConfig::get::<String>("DTLS_ADDR").unwrap() == "127.0.0.1:9232" {
    //     console_subscriber::init();
    // }

    // rib_rx <GDPPacket = [u8]>: forward gdppacket to rib
    let (rib_tx, rib_rx) = mpsc::channel(32);
    // channel_tx <GDPChannel = <gdp_name, sender>>: forward channel maping to rib
    let (channel_tx, channel_rx) = mpsc::channel(32);

    // connect to the target_rib with dTLS if specified in argument
    if target_rib_addr.is_some() && remote_rib_name.is_some() {
        let dtls_target_addr = target_rib_addr.unwrap();
        let remote_rib_name = remote_rib_name.unwrap();
        
        let rib_connection_handle = tokio::spawn(connect_rib(
            remote_rib_name,
            dtls_target_addr,
            rib_tx.clone(),
            channel_tx.clone()
        ));
        let tcp_sender_handle = tokio::spawn(tcp_listener(
            AppConfig::get::<String>("TCP_ADDR").unwrap(),
            rib_tx.clone(),
            channel_tx.clone(),
        ));
    
        let dtls_sender_handle =
            tokio::spawn(dtls_listener(AppConfig::get::<String>("DTLS_ADDR").unwrap(), rib_tx.clone(), channel_tx.clone(), false));
        let rib_handle = tokio::spawn(connection_router(rib_rx, channel_rx, AppConfig::get::<u32>("GDPNAME").expect("Unable to get Config item: GDPNAME")));
    
        future::join_all([rib_connection_handle, tcp_sender_handle, rib_handle, dtls_sender_handle]).await;

    } else {
        let tcp_sender_handle = tokio::spawn(tcp_listener(
            AppConfig::get::<String>("TCP_ADDR").unwrap(),
            rib_tx.clone(),
            channel_tx.clone(),
        ));
    
        let dtls_sender_handle =
            tokio::spawn(dtls_listener(AppConfig::get::<String>("DTLS_ADDR").unwrap(), rib_tx.clone(), channel_tx.clone(), false));
        let rib_handle = tokio::spawn(connection_router(rib_rx, channel_rx, AppConfig::get::<u32>("GDPNAME").expect("Unable to get Config item: GDPNAME")));
    
        future::join_all([tcp_sender_handle, rib_handle, dtls_sender_handle]).await;
    }
    

    // let tcp_sender_handle = tokio::spawn(tcp_listener(
    //     AppConfig::get::<String>("TCP_ADDR").unwrap(),
    //     rib_tx.clone(),
    //     channel_tx.clone(),
    // ));

    // let dtls_sender_handle =
    //     tokio::spawn(dtls_listener(AppConfig::get::<String>("DTLS_ADDR").unwrap(), rib_tx.clone(), channel_tx.clone(), false));
    // let rib_handle = tokio::spawn(connection_router(rib_rx, channel_rx, AppConfig::get::<u32>("router_name").expect("Unable to get Config item: router_name")));

    // future::join_all([tcp_sender_handle, rib_handle, dtls_sender_handle]).await;
    //join!(foo_sender_handle, bar_sender_handle, receive_handle);
}




#[tokio::main]
pub async fn rib_aynsc_loop() {
    // rib_rx <GDPPacket = [u8]>: forward gdppacket to rib
    let (rib_tx, rib_rx): (Sender<GDPPacket>, Receiver<GDPPacket>) = mpsc::channel(32);
    // channel_tx <GDPChannel = <gdp_name, sender>>: forward channel maping to rib
    let (channel_tx, channel_rx): (Sender<GDPChannel>, Receiver<GDPChannel>) = mpsc::channel(32);
    // spawn dtls listener and handler dispatcher loop
    let dtls_sender_handle = tokio::spawn(dtls_listener(RIB_DTLS_ADDR.to_string(), rib_tx.clone(), channel_tx.clone(), true));
    // spawn dtls processing loop
    let request_proc_handle = tokio::spawn(remote_rib::process_rib_request(rib_rx, channel_rx));

    futures::future::join_all([dtls_sender_handle, request_proc_handle]).await;
        
}

/// Show the configuration file
pub fn router() -> Result<()> {
    warn!("router is started!");

    let has_parent = AppConfig::get::<bool>("HAS_PARENT").expect("Unable to get config item: HAS_PARENT");
    let remote_rib_address = AppConfig::get::<String>("REMOTE_RIB_ADDRESS");
    let remote_rib_name = AppConfig::get::<u32>("REMOTE_RIB_NAME");
    
    let local_ip = local_ip().unwrap().to_string();
    AppConfig::set("local_ip", &local_ip).expect("Unable to set config item: local_ip");

    dbg!(AppConfig::get::<String>("TCP_ADDR").unwrap());
    dbg!(AppConfig::get::<String>("DTLS_ADDR").unwrap());

    if has_parent {
        println!("router will connect to domain rib with name {:?} at {:?}", remote_rib_name.as_ref().unwrap(), remote_rib_address.as_ref().unwrap());
    } else {
        println!("Current router is not connected to a domain rib");
    }

    router_async_loop(remote_rib_address.ok(), remote_rib_name.ok());


    

    Ok(())
}

// Start the remote rib
pub fn rib() -> Result<()> {
    let has_parent = AppConfig::get::<bool>("HAS_PARENT").expect("Unable to get config item: HAS_PARENT");
    let remote_rib_address = AppConfig::get::<String>("REMOTE_RIB_ADDRESS");
    let remote_rib_name = AppConfig::get::<u32>("REMOTE_RIB_NAME");
    if has_parent {
        println!("rib will connect to parent rib at {:?}", remote_rib_address.unwrap());
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
    console_subscriber::init();
    // Log this Error simulation
    info!("We are simulating an error");
    // test_cert();
    dtls_test_client("127.0.0.1:9232").expect("DLTS Client error");
    Ok(())
}
