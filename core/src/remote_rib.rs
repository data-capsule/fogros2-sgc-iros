use multimap::MultiMap;
use tokio::sync::mpsc::{Sender, Receiver};

use std::{collections::HashMap};

use crate::{structs::{GDPName, GdpAction, GDPPacket, GDPChannel}};



pub async fn process_rib_request(
    mut rib_rx: Receiver<GDPPacket>, mut channel_rx: Receiver<GDPChannel>
) {
    let _ = tokio::spawn(async move {
        // GDPName to router's IP address
        let mut ip_table = HashMap::new();
        // Multimap from host GDPName to a list of router's GDPName
        let mut host_table: MultiMap<GDPName, GDPName> = MultiMap::new();
        // Hashmap from router/lower-level remote RIB GDPName to their channel
        let mut connection_rib_table: HashMap<GDPName, Sender<GDPPacket>> = HashMap::new();

        loop {
            tokio::select! {
                
                Some(pkt) = rib_rx.recv() => {
                    println!("Remote RIB received: {:?}", pkt.action);

                    // Processing control messages
                    if pkt.action == GdpAction::ClientAdvertise {
                        let received_str: Vec<&str> = std::str::from_utf8(&pkt.payload)
                            .unwrap()
                            .trim()
                            .split(",")
                            .collect();
                        let v = received_str[2].to_string();
                        let v = v.trim_end_matches('\0').to_string();
                        dbg!(&v);
                        ip_table.insert(pkt.gdpname, v);

                        continue;

                    } else if pkt.action == GdpAction::RouteAdvertise {
                        let received_str: Vec<&str> = std::str::from_utf8(&pkt.payload)
                            .unwrap()
                            .trim()
                            .split(",")
                            .collect();
                        let router_gdpname = match &received_str[1][0..1] {
                            "1" => GDPName([1, 1, 1, 1]),
                            "2" => GDPName([2, 2, 2, 2]),
                            "3" => GDPName([3, 3, 3, 3]),
                            "4" => GDPName([4, 4, 4, 4]),
                            "5" => GDPName([5, 5, 5, 5]),
                            "6" => GDPName([6, 6, 6, 6]),
                            _ => GDPName([0, 0, 0, 0]),
                        };
                        let host_gdpname = match &received_str[2][0..1] {
                            "1" => GDPName([1, 1, 1, 1]),
                            "2" => GDPName([2, 2, 2, 2]),
                            "3" => GDPName([3, 3, 3, 3]),
                            "4" => GDPName([4, 4, 4, 4]),
                            "5" => GDPName([5, 5, 5, 5]),
                            "6" => GDPName([6, 6, 6, 6]),
                            _ => GDPName([0, 0, 0, 0]),
                        };
                        host_table.insert(host_gdpname, router_gdpname);

                        println!("{{RouteAdvertise}}, host's gdpname: {:?}, delegated router's gdpname: {:?}", host_gdpname, router_gdpname);

                        continue;
                    } else if pkt.action == GdpAction::RibGet {
                        let received_str: Vec<&str> = std::str::from_utf8(&pkt.payload)
                            .unwrap()
                            .trim()
                            .split(",")
                            .collect();
                        
                        let queried_gdpname = match &received_str[2][0..1] {
                            "1" => GDPName([1, 1, 1, 1]),
                            "2" => GDPName([2, 2, 2, 2]),
                            "3" => GDPName([3, 3, 3, 3]),
                            "4" => GDPName([4, 4, 4, 4]),
                            "5" => GDPName([5, 5, 5, 5]),
                            "6" => GDPName([6, 6, 6, 6]),
                            _ => GDPName([0, 0, 0, 0]),
                        };
                        println!("{{RibGet}}, queries gdpname is = {:?}", queried_gdpname);
                        if let Some(router_gdpname) = host_table.get(&queried_gdpname) {
                            if let Some(router_ip) = ip_table.get(&router_gdpname) {
                                let rib_reply_pkt = GDPPacket { 
                                    action: GdpAction::RibReply, 
                                    gdpname: queried_gdpname, 
                                    payload: format!("REPLY, {}, {}", queried_gdpname.0[0], router_ip).as_bytes().to_vec() 
                                };
                                match connection_rib_table.get(&pkt.gdpname) {
                                    Some(dst) => {
                                        dst.send(rib_reply_pkt).await.unwrap();
                                    }
                                    None => {
                                        println!("{:} is not there.", pkt.gdpname);
                                    }
                                }
                            }
                        }
                        continue;
                    }
                }

                // rib advertisement received
                Some(channel) = channel_rx.recv() => {
                    println!("channel registry received {:}", channel.gdpname);
                    connection_rib_table.insert(
                        channel.gdpname,
                        channel.channel
                    );
                }
            }
        }
    }).await;
    
}



