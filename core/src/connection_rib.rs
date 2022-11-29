use crate::{structs::{GDPChannel, GDPName, GDPPacket, GdpAction}, network::udpstream::UdpStream};
use std::{collections::HashMap, net::SocketAddr, str::FromStr, pin::Pin, sync::{Arc, Mutex}};
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use tokio::{sync::mpsc::{self, Receiver, Sender}, io::{AsyncReadExt, AsyncWriteExt}};
use utils::app_config::AppConfig;
use tokio::sync::oneshot;


const SERVER_DOMAIN: &'static str = "pourali.com";

/// receive, check, and route GDP messages
///
/// receive from a pool of receiver connections (one per interface)
/// use a hash table to figure out the corresponding
///     hash table <gdp_name, send_tx>
///     TODO: use future if the destination is unknown
/// forward the packet to corresponding send_tx
pub async fn connection_router(
    mut rib_rx: Receiver<GDPPacket>, mut channel_rx: Receiver<GDPChannel>, rib_gdpname: u32
) {

    // TODO: currently, we only take one rx due to select! limitation
    // will use FutureUnordered Instead
    let _receive_handle = tokio::spawn(async move {
        let mut connection_rib_table: HashMap<GDPName, Sender<GDPPacket>> = HashMap::new();
        let mut waiting_entries = HashMap::new();

        // todo: Switch to real gdpname instead of integers
        let parent_rib_config = AppConfig::get::<u32>("REMOTE_RIB_NAME");
        let mut parent_rib_gdpname = None;
        if AppConfig::get::<bool>("HAS_PARENT").unwrap() == true {
            if let Ok(num) = parent_rib_config {
                parent_rib_gdpname = match num {
                    1 => Some(GDPName([1, 1, 1, 1])),
                    2 => Some(GDPName([2, 2, 2, 2])),
                    3 => Some(GDPName([3, 3, 3, 3])),
                    4 => Some(GDPName([4, 4, 4, 4])),
                    5 => Some(GDPName([5, 5, 5, 5])),
                    6 => Some(GDPName([6, 6, 6, 6])),
                    _ => Some(GDPName([0, 0, 0, 0])),
                };
            }
        }
        

        // loop polling from
        loop {
            tokio::select! {
                // GDP packet received
                // recv () -> find_where_to_route() -> route()
                Some(mut pkt) = rib_rx.recv() => {
                    println!("forwarder received: {pkt}");

                    // todo: process control message
                    
                    if pkt.action == GdpAction::RouteAdvertise {
                        // only do ROUTE_ADV when there exist parent rib
                        if let Some(gdpname) = parent_rib_gdpname {
                            let remote_rib_channel = connection_rib_table.get(&gdpname).expect("RIB: cannot find remote rib");
                            pkt.payload = format!("ROUTE_ADV,{},{}", rib_gdpname, pkt.gdpname.0[0]).as_bytes().to_vec();
                            remote_rib_channel.send(pkt).await.expect("RIB: parent RIB connection closed!");
                            println!("advertised route to parent rib");
                            continue;
                        }
                    }
                    
                    

                    // find where to route
                    match connection_rib_table.get(&pkt.gdpname) {
                        Some(routing_dst) => {
                            // println!("fwd!");
                            routing_dst.send(pkt).await.expect("RIB: remote connection closed");
                        }
                        None => {
                            
                            if let Some(gdpname) = parent_rib_gdpname {
                                
                                // Send RibGet to remote rib and wait for reply
                                let remote_rib_channel = connection_rib_table.get(&gdpname).expect("RIB: cannot find remote rib");
                                let rib_get = GDPPacket { 
                                    action: GdpAction::RibGet, 
                                    gdpname: pkt.gdpname, 
                                    payload: format!("RIBGET,{:?},{:?}",  AppConfig::get::<u32>("GDPNAME").unwrap(), pkt.gdpname.0[0]).as_bytes().to_vec()
                                };
                                let (tx, mut rx) = mpsc::channel::<Sender<GDPPacket>>(32);
                                // if !waiting_entries.contains_key(&pkt.gdpname) {
                                    waiting_entries.insert(pkt.gdpname, tx);

                                    println!("Sent RibGet packet: {:?}", &rib_get);
                                    remote_rib_channel.send(rib_get).await.expect("RIB:remote connection closed");
                                    
                                    
    
                                    tokio::spawn(async move {
                                        let channel = rx.recv().await;
                                        // println!("Got rib reply channel, sending the queued packet");
                                        match channel {
                                            Some(sender) => sender.send(pkt).await.expect("RIB: remote connection closed"),
                                            None => {},
                                        }
                                    });
                                // }
                                
                            } else {
                                println!("Failed to forward, destination not in forwarding table!");
                            }
                        }
                    }
                }

                // rib advertisement received
                Some(channel) = channel_rx.recv() => {
                    println!("channel registry received {:}", channel.gdpname);
                    if waiting_entries.contains_key(&channel.gdpname) {
                        println!("Fetched a waiting channel from higher RIB");
                        waiting_entries
                            .remove(&channel.gdpname)
                            .unwrap()
                            .send(channel.channel.clone())
                            .await.expect(&format!("Unable to send wake signal to thread waiting for {:?}", &channel.gdpname));
                    }
                    connection_rib_table.insert(
                        channel.gdpname,
                        channel.channel
                    );
                },
            }
        }
    }).await;
}
