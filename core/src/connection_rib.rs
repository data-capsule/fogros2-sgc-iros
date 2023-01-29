use crate::gdp_proto::GdpUpdate;
use crate::network::dtls::setup_dtls_connection_to;
use crate::rib::{RIBClient, TopicRecord};
use crate::structs::{
    GDPChannel, GDPName, GDPPacket, GdpAction, PubPacket, SubPacket, SubscriberInfo,
};
use multimap::MultiMap;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH, Instant};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, Duration};
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
    mut channel_rx: Receiver<GDPChannel>, rib_tx: Sender<GDPPacket>,
    channel_tx: Sender<GDPChannel>,start_time: Instant
) {
    // TODO: currently, we only take one rx due to select! limitation
    // will use FutureUnordered Instead
    let _receive_handle = tokio::spawn(async move {
        // map Host GDPName to Sending Channel
        let mut connection_rib_table: MultiMap<GDPName, Sender<GDPPacket>> = MultiMap::new();
        // map Topic GDPName to Host GDPName
        let mut sub_nodes_creators: MultiMap<GDPName, GDPName> = MultiMap::new();
        // map Topic GDPName to SubscriberInfo
        let sub_nodes_info: Arc<RwLock<HashMap<GDPName, SubscriberInfo>>> =
            Arc::new(RwLock::new(HashMap::new()));
        // RIBClient abstraction, all RIB interaction should use this object
        // todo Jiachen: move password to a secure config file
        let rib_client = Arc::new(Mutex::new(
            RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap(),
        ));

        let (sub_update_tx, mut sub_update_rx) = mpsc::channel::<(GDPName, Vec<Ipv4Addr>)>(100);

        // let start = Instant::now();
        
        let duration = start_time.elapsed();
        warn!("starting the router takes {:?}", duration);

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

                        // publish the pub node to the remote RIB
                        let result = rib_client.lock().await.create_pub_node(&format!("{}", pub_packet.topic_name));

                        // a signal represents whether there are new sub nodes for this topic
                        let mut more_unique_ip = false; 

                        match result {
                            Err(_) => {
                                warn!("Failed to create a pub node in the remote RIB");
                            },
                            Ok(kv_map) => {

                                // ! for benchmark purpose
                                let host_channel = connection_rib_table.get(&pub_packet.creator).unwrap();
                                let res_packet = GDPPacket { action: GdpAction::Noop, gdpname: GDPName([0,0,0,0]), payload: Some("topic_pub_ack\n".as_bytes().to_vec()), proto: None };
                                host_channel.send(res_packet).await.unwrap();


                                info!("A pub node has been created in the remote RIB");
                                let mut guard = sub_nodes_info.write().await;
                                let entry = guard.entry(pub_packet.topic_name).or_insert(SubscriberInfo::new());
                                for (_key, value) in &kv_map {
                                    let topic_record: TopicRecord = serde_json::from_str(value).expect("TopicRecord deserialization failed");
                                    entry.incr_num_sub_nodes();
                                    let (_, unique) =  entry.insert_and_keep_unique_ip_vec(&[topic_record.ip_address.parse::<Ipv4Addr>().unwrap()]);
                                    more_unique_ip |= unique;
                                }
                            }
                        }
                        
                        // Only initiate new connections when there are new sub nodes. No need to create duplicate connections.
                        if more_unique_ip {
                            if let Some(SubscriberInfo(_, ref set)) = sub_nodes_info.read().await.get(&pub_packet.topic_name) {
                                for ip in set {
                                    let rib_tx_cloned = rib_tx.clone();
                                    let channel_tx_cloned = channel_tx.clone();
                                    let mut socket_addr = ip.to_string();
                                    tokio::spawn(async move {
                                        // Connect to the target router using dTLS
                                        let dtls_port: String = AppConfig::get("dtls_port").expect("No attribute dtls_port in config file");
                                        socket_addr.push_str(&format!(":{}", dtls_port));
                                        setup_dtls_connection_to(pub_packet.topic_name, socket_addr, rib_tx_cloned, channel_tx_cloned).await;
                                    });
                                }
                            }
                        }
                            
                        // Create a polling task, which is responsible for periodically fetching new subscriber nodes' information
                        let sub_nodes_info_cloned = sub_nodes_info.clone();
                        let rib_client_cloned = rib_client.clone();
                        let sub_update_tx_cloned = sub_update_tx.clone();
                        tokio::spawn(async move {
                            loop {
                                // poll every 5 seconds if there is new sub nodes corresponding to this topic_name
                                sleep(Duration::from_millis(5000)).await;
                                let curr_sub_count = sub_nodes_info_cloned.read().await.get(&pub_packet.topic_name)
                                            .expect(&format!("topic name {} not found in cache", pub_packet.topic_name))
                                            .get_num_sub_nodes();
                                let result = rib_client_cloned.lock().await.fetch_new_sub_node_records(&format!("{}", pub_packet.topic_name), curr_sub_count);
                                match result {
                                    Err(_) => {
                                        warn!("Failed to fetch new subscriber records");
                                    },
                                    Ok(records) => {
                                        let ip_vec: Vec<Ipv4Addr> = records.iter().map(|record| record.ip_address.parse::<Ipv4Addr>().unwrap()).collect();
                                        let msg = (pub_packet.topic_name, ip_vec);
                                        sub_update_tx_cloned.send(msg).await.expect("sub_update channal closed!");
                                    }
                                }

                            }

                        });

                    } else if pkt.action == GdpAction::SubAdvertise {
                        let sub_packet = SubPacket::from_vec_bytes(&pkt.payload.as_ref().expect("Packet payload is empty"));
                        sub_nodes_creators.insert(sub_packet.topic_name, sub_packet.creator);

                        // publish the sub node to the remote RIB
                        let result = rib_client.lock().await.create_sub_node(&format!("{}", sub_packet.topic_name));

                        match result {
                            Err(_) => {
                                warn!("Failed to create a sub node in the remote RIB");
                            },
                            Ok(_) => {
                                // ! for benchmark purpose
                                let host_channel = connection_rib_table.get(&sub_packet.creator).unwrap();
                                let res_packet = GDPPacket { action: GdpAction::Noop, gdpname: GDPName([0,0,0,0]), payload: Some("topic_sub_ack\n".as_bytes().to_vec()), proto: None };
                                host_channel.send(res_packet).await.unwrap();
                                info!("A sub node has been created in the remote RIB");
                            }
                        }

                    } else {
                        // find where to route
                        match connection_rib_table.get_vec(&pkt.gdpname) {
                            Some(routing_dsts) => {
                                for routing_dst in routing_dsts {
                                    debug!("fwd!");
                                    let pkt = pkt.clone();
                                    routing_dst.send(pkt).await.expect("RIB: remote connection closed");
                                }

                            }
                            None => {
                                match sub_nodes_creators.get_vec(&pkt.gdpname) {
                                    Some(creators) => {
                                        // entering this match branch means the gdpname might be a topic gdpname instead of a host gdpname directly
                                        for creator in creators {
                                            for routing_dsts in connection_rib_table.get_vec(creator) {
                                                for routing_dst in routing_dsts {
                                                    debug!("fwd!");
                                                    let pkt = pkt.clone();
                                                    routing_dst.send(pkt).await.expect("RIB: remote connection closed");
                                                }
                                            }
                                        }

                                    },
                                    None => {
                                        warn!("{:?} is neither a host name nor a topic name, ignoring...", pkt.gdpname);
                                    },
                                }
                                // info!("{:} is not there, broadcasting...", pkt.gdpname);
                                // for (_key, value) in connection_rib_table.iter_all() {
                                //     for dst in value {
                                //         dst.send(pkt.clone()).await.expect("RIB: remote connection closed");

                                //     }
                                // }

                            }
                        }
                    }
                }

                // connection rib advertisement received
                Some(channel) = channel_rx.recv() => {
                    info!("channel registry received {:}", channel.gdpname);
                    
                    // ! for benchmark purpose, assume the host always use 1,1,1,1 as gdpname in benchmarking
                    if channel.gdpname == GDPName([1,1,1,1]) {
                        let res_packet = GDPPacket { action: GdpAction::Noop, gdpname: GDPName([0,0,0,0]), payload: Some("adv_ack\n".as_bytes().to_vec()), proto: None };
                        channel.channel.send(res_packet).await.unwrap();
                    }

                    connection_rib_table.insert(
                        channel.gdpname,
                        channel.channel
                    );         
                },

                Some(update) = stat_rs.recv() => {
                    //TODO: update rib here
                },

                // update with new information from the polling task
                Some(sub_info_update) = sub_update_rx.recv() => {
                    // insert only unique ip into sub_nodes_info and update states such as this topic's number of sub nodes
                    let (gdpname, ip_vec) = sub_info_update;
                    if ip_vec.len() > 0 {
                        let mut guard = sub_nodes_info.write().await;
                        let subscriber_info = guard.get_mut(&gdpname).unwrap();
                        let num_new_nodes = ip_vec.len() as u64;
                        let (unique_ips, more_unique_ip) = subscriber_info.insert_and_keep_unique_ip_vec(&ip_vec);
                        let curr_node_count = subscriber_info.get_num_sub_nodes();
                        subscriber_info.set_num_sub_nodes(curr_node_count + num_new_nodes);
                        println!("{:?} has {} sub nodes remotely", gdpname, curr_node_count + num_new_nodes );
                        if more_unique_ip {
                            for ip in unique_ips {
                                let rib_tx_cloned = rib_tx.clone();
                                let channel_tx_cloned = channel_tx.clone();
                                let mut socket_addr = ip.to_string();
                                tokio::spawn(async move {
                                    // Connect to the target router using dTLS
                                    let dtls_port: String = AppConfig::get("dtls_port").expect("No attribute dtls_port in config file");
                                    socket_addr.push_str(&format!(":{}", dtls_port));
                                    setup_dtls_connection_to(gdpname, socket_addr, rib_tx_cloned, channel_tx_cloned).await;
                                });
                            }
                        }
                    }


                }


            }
        }
    });
    
}
