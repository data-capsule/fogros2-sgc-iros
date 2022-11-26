use std::borrow::BorrowMut;

use crate::{structs::{GDPChannel, GDPName, GDPPacket, GdpAction, GDPUpdate}, network::dtls::{connect_target, spawn_connect_target}};
use tokio::sync::mpsc::Sender;

/// construct a gdp packet struct
/// we may want to use protobuf later
/// this part is facilitate testing only

pub fn populate_gdp_struct(buffer: Vec<u8>) -> GDPPacket {
    let received_str: Vec<&str> = std::str::from_utf8(&buffer)
        .unwrap()
        .trim()
        .split(",")
        .collect();
    let m_gdp_action = match received_str[0] {
        "ADV" => GdpAction::ClientAdvertise,
        "FWD" => GdpAction::Forward,
        "ROUTE_ADV" => GdpAction::RouteAdvertise,
        "REPLY" => GdpAction::RibReply,
        _ => GdpAction::Noop,
    };

    let m_gdp_name = match &received_str[1][0..1] {
        "1" => GDPName([1, 1, 1, 1]),
        "2" => GDPName([2, 2, 2, 2]),
        "3" => GDPName([3, 3, 3, 3]),
        "4" => GDPName([4, 4, 4, 4]),
        "5" => GDPName([5, 5, 5, 5]),
        "6" => GDPName([6, 6, 6, 6]),
        _ => GDPName([0, 0, 0, 0]),
    };

    let pkt = GDPPacket {
        action: m_gdp_action,
        gdpname: m_gdp_name,
        payload: buffer,
    };

    pkt
}



/// parses the processsing received GDP packets
/// match GDP action and send the packets with corresponding actual actions
///
///  proc_gdp_packet(buf,  // packet
///               rib_tx.clone(),  //used to send packet to rib
///               channel_tx.clone(), // used to send GDPChannel to rib
///               m_tx.clone() //the sending handle of this connection
///  );
///
pub async fn proc_gdp_packet(
    packet: Vec<u8>,
    rib_tx: &Sender<GDPPacket>,      //used to send packet to rib
    channel_tx: &Sender<GDPChannel>, // used to send GDPChannel to rib
    m_tx: &Sender<GDPPacket>,        //the sending handle of this connection
) {
    // Vec<u8> to GDP Packet
    let gdp_packet = populate_gdp_struct(packet);
    let action = gdp_packet.action;
    let gdp_name = gdp_packet.gdpname;

    match action {
        GdpAction::ClientAdvertise => {
            //construct and send channel to RIB
            let channel = GDPChannel {
                gdpname: gdp_name,
                channel: m_tx.clone(),
            };
            channel_tx
                .send(channel)
                .await
                .expect("channel_tx channel closed!");
            
            // It is possible this router has parent RIB
            // need to prepare RouteAdvertise
            let route_advertise = GDPPacket {
                action: GdpAction::RouteAdvertise,
                gdpname: gdp_name,
                payload: Vec::new(),
            };
            rib_tx.send(route_advertise).await.expect("rib_tx channel closed!");
        }
        GdpAction::Forward => {
            //send the packet to RIB
            rib_tx
                .send(gdp_packet)
                .await
                .expect("rib_tx channel closed!");
        }
        GdpAction::RibGet => {
            // handle rib query by responding with the RIB item
        }
        GdpAction::RibReply => {
            // update local rib with the rib reply
            let received_str: Vec<&str> = std::str::from_utf8(&gdp_packet.payload)
                .unwrap()
                .trim()
                .split(",")
                .collect();
            // format = {REPLY, 127.0.0.1:9232}
            let ip_address = received_str[1].trim();

            let clone_rib_tx = rib_tx.clone();
            let clone_channel_tx = channel_tx.clone();
            spawn_connect_target(gdp_name.0[0].into(), ip_address.to_string(), clone_rib_tx, clone_channel_tx);
            
        }

        GdpAction::Noop => {
            // nothing to be done here
            println!("GDP Action Noop Detected, Make sure to update to 5 for forwarding");
        }
        _ => {
            // control, put, get
            println!("{:?} is not implemented yet", action)
        }
    }
}



pub async fn proc_rib_packet(
    packet: Vec<u8>,
    rib_tx: &Sender<GDPPacket>,      //used to send packet to rib
    channel_tx: &Sender<GDPChannel>, // used to send GDPChannel to rib
    m_tx: &Sender<GDPPacket>,        //the sending handle of this connection
) {
    // Vec<u8> to GDP Packet
    let gdp_packet = populate_gdp_struct(packet);
    let action = gdp_packet.action;
    let gdp_name = gdp_packet.gdpname;

    match action {
        GdpAction::ClientAdvertise => {
            //construct and send channel to RIB
            let channel = GDPChannel {
                gdpname: gdp_name,
                channel: m_tx.clone(),
            };
            channel_tx
                .send(channel)
                .await
                .expect("channel_tx channel closed!");
            // RIB also needs router's IP address for future RibGet request
            rib_tx.send(gdp_packet).await.expect("rib_tx channel closed!");
            
        }
        GdpAction::RouteAdvertise => {
            // handle route advertisement
            // The format of message is `ROUTE_ADV,1,2`, where 2 is the host gdpname, and 1 is the delegated router
            rib_tx
                .send(gdp_packet)
                .await
                .expect("rib_tx channel closed!");

        }
        GdpAction::RibGet => {
            
            rib_tx
                .send(gdp_packet)
                .await
                .expect("rib_tx channel closed!");
        }
        _ => {
            // control, put, get
            println!("{:?} is not implemented yet", action)
        }
    }
}
