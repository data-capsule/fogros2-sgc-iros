use crate::gdp_proto::GdpUpdate;
use crate::network::dtls::setup_dtls_connection_to;
use crate::network::tcp::setup_tcp_connection_to;
use crate::rib::{RIBClient, TopicRecord};
use crate::structs::{GDPChannel, GDPName, GDPPacket, GdpAction, PubPacket, SubPacket};
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use multimap::MultiMap;
use tokio::sync::mpsc::{Receiver, Sender};
use utils::app_config::AppConfig;

/// receive, check, and route GDP messages
///
/// receive from a pool of receiver connections (one per interface)
/// use a hash table to figure out the corresponding
///     hash table <gdp_name, send_tx>
///     TODO: use future if the destination is unknown
/// forward the packet to corresponding send_tx
pub async fn connection_router(
    mut rib_rx: Receiver<GDPPacket>, mut stat_rs: Receiver<GdpUpdate>,
    mut channel_rx: Receiver<GDPChannel>, rib_tx: Sender<GDPPacket>, channel_tx: Sender<GDPChannel>
) {
    // TODO: currently, we only take one rx due to select! limitation
    // will use FutureUnordered Instead
    let _receive_handle = tokio::spawn(async move {
        // map Host GDPName to Sending Channel
        let mut coonection_rib_table: MultiMap<GDPName, Sender<GDPPacket>> = MultiMap::new();
        // map Topic GDPName to Host GDPName
        let mut pub_nodes_creators: HashMap<GDPName, GDPName> = HashMap::new();
        // map Topic GDPName to SubscriberInfo
        let mut sub_nodes_info: HashMap<GDPName, HashMap<String, Ipv4Addr>> = HashMap::new();
        // RIBClient abstraction, all RIB interaction should use this object 
        // todo Jiachen: move password to a secure config file
        let mut rib_client = RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();
        
        // loop polling from
        loop {
            tokio::select! {
                // GDP packet received
                // recv () -> find_where_to_route() -> route()
                Some(pkt) = rib_rx.recv() => {
                    info!("forwarder received: {pkt}");

                    // Control plane packet reaction
                    // todo Jiachen: Maybe we should consider splitting the data and control pipeline for better readability
                    if pkt.action == GdpAction::PubAdvertise {
                        let pub_packet = PubPacket::from_vec_bytes(&pkt.payload.as_ref().expect("Packet payload is empty"));
                        pub_nodes_creators.insert(pub_packet.topic_name, pub_packet.creator);
                        
                        // publish the pub node to the remote RIB
                        let result = rib_client.create_pub_node(&format!("{}", pub_packet.topic_name));
                        
                        match result {
                            Err(_) => {
                                warn!("Failed to create a pub node in the remote RIB");
                            },
                            Ok(kv_map) => {
                                info!("A pub node has been created in the remote RIB");
                                for (key, value) in &kv_map {
                                    let topic_record: TopicRecord = serde_json::from_str(value).expect("TopicRecord deserialization failed");
                                    let entry = sub_nodes_info.entry(pub_packet.topic_name).or_insert(HashMap::new());
                                    entry.insert(key.to_string(), topic_record.ip_address.parse::<Ipv4Addr>().unwrap());
                                }
                            }
                        }

                        // Establish dtls connections with each unique subscribing router
                        let subscriber_info = sub_nodes_info.get(&pub_packet.topic_name);
                        let unique_IP_set = match subscriber_info {
                            Some(hashmap) => {
                                hashmap.values().cloned().collect::<HashSet<Ipv4Addr>>() // This can be slow due to the clone if we have too many sub nodes
                            },
                            None => {HashSet::new()},
                        };
                        for ip in unique_IP_set.into_iter() {
                            let rib_tx_cloned = rib_tx.clone();
                            let channel_tx_cloned = channel_tx.clone();
                            tokio::spawn(async move {
                                let mut socket_addr = ip.to_string();
                                // Connect to the target router using TCP
                                socket_addr.push_str(AppConfig::get("tcp_port").expect("No attribute tcp_port in config file"));
                                setup_tcp_connection_to(pub_packet.topic_name, socket_addr, rib_tx_cloned, channel_tx_cloned).await;
                                // !(not working right now) Uncomment to connect to the target router using DTLS
                                // socket_addr.push_str(":9232");
                                // setup_dtls_connection_to(pub_packet.topic_name, socket_addr, rib_tx_cloned, channel_tx_cloned).await;
                            });
                        }
                    } else if pkt.action == GdpAction::SubAdvertise {
                        let sub_packet = SubPacket::from_vec_bytes(&pkt.payload.as_ref().expect("Packet payload is empty"));
                        let result = rib_client.create_sub_node(&format!("{}", sub_packet.topic_name));

                        match result {
                            Err(_) => {
                                warn!("Failed to create a sub node in the remote RIB");
                            },
                            Ok(_) => {
                                info!("A sub node has been created in the remote RIB");
                            }
                        }
                    

                    
                        
                    } else {
                        // find where to route
                        match coonection_rib_table.get_vec(&pkt.gdpname) {
                            Some(routing_dsts) => {
                                for routing_dst in routing_dsts {
                                    debug!("fwd!");
                                    let pkt = pkt.clone();
                                    routing_dst.send(pkt).await.expect("RIB: remote connection closed");
                                }
                                
                            }
                            None => {
                                info!("{:} is not there, broadcasting...", pkt.gdpname);
                                for (_key, value) in coonection_rib_table.iter_all() {
                                    for dst in value {
                                        dst.send(pkt.clone()).await.expect("RIB: remote connection closed");

                                    }
                                }
                                
                            }
                        }
                    }
                }

                // connection rib advertisement received
                Some(channel) = channel_rx.recv() => {
                    info!("channel registry received {:}", channel.gdpname);
                    coonection_rib_table.insert(
                        channel.gdpname,
                        channel.channel
                    );
                },

                Some(update) = stat_rs.recv() => {
                    //TODO: update rib here
                }
            }
        }
    });
}
