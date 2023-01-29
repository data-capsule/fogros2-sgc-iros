use crate::structs::{GDPChannel, GDPName, GDPPacket, GdpAction, PubPacket};
use tokio::sync::mpsc::Sender;

/// construct gdp struct from bytes
/// bytes is put as payload
pub fn construct_gdp_forward_from_bytes(gdp_name: GDPName, buffer: Vec<u8>) -> GDPPacket {
    GDPPacket {
        action: GdpAction::Forward,
        gdpname: gdp_name,
        payload: Some(buffer),
        proto: None,
    }
}

/// construct gdp struct from bytes
/// bytes is put as payload
pub fn construct_gdp_advertisement_from_bytes(gdp_name: GDPName) -> GDPPacket {
    GDPPacket {
        action: GdpAction::Advertise,
        gdpname: gdp_name,
        payload: None,
        proto: None,
    }
}

/// construct a gdp packet struct
/// we may want to use protobuf later
/// this part is facilitate testing only
pub fn populate_gdp_struct_from_bytes(buffer: Vec<u8>) -> GDPPacket {
    let received_str: Vec<&str> = std::str::from_utf8(&buffer)
        .unwrap()
        .trim()
        .split(",")
        .collect();
    let m_gdp_action = match received_str[0] {
        "ADV" => GdpAction::Advertise,
        "FWD" => GdpAction::Forward,
        "PUB" => GdpAction::PubAdvertise,
        "SUB" => GdpAction::SubAdvertise,
        _ => GdpAction::Noop,
    };
    if received_str.len() == 1{

        println!("{:?}", received_str);
    }
    let m_gdp_name = match &received_str[1][0..1] {
        "1" => GDPName([1, 1, 1, 1]),
        "2" => GDPName([2, 2, 2, 2]),
        "3" => GDPName([3, 3, 3, 3]),
        "4" => GDPName([4, 4, 4, 4]),
        "5" => GDPName([5, 5, 5, 5]),
        _ => GDPName([0, 0, 0, 0]),
    };

    GDPPacket {
        action: m_gdp_action,
        gdpname: m_gdp_name,
        payload: Some(buffer),
        proto: None,
    }
}

use crate::gdp_proto::GdpPacket;

pub fn populate_gdp_struct_from_proto(proto: GdpPacket) -> GDPPacket {
    // GDPPacket {
    //     action: GdpAction::try_from(proto.action as u8).unwrap(),
    //     gdpname: proto.receiver,
    //     payload: Some(buffer),
    //     proto: None,
    // }

    // TODO: currently, populate directly from payload as placeholder
    populate_gdp_struct_from_bytes(proto.payload)
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
    gdp_packet: GDPPacket,
    rib_tx: &Sender<GDPPacket>,      //used to send packet to rib
    channel_tx: &Sender<GDPChannel>, // used to send GDPChannel to rib
    m_tx: &Sender<GDPPacket>,        //the sending handle of this connection
) {
    // Vec<u8> to GDP Packet
    // let gdp_packet = populate_gdp_struct(packet);
    let action = gdp_packet.action;
    let gdp_name = gdp_packet.gdpname;

    match action {
        GdpAction::Advertise => {
            //construct and send channel to RIB
            let channel = GDPChannel {
                gdpname: gdp_name,
                channel: m_tx.clone(),
            };
            channel_tx
                .send(channel)
                .await
                .expect("channel_tx channel closed!");
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
        }

        GdpAction::PubAdvertise => {
            //send the packet to RIB
            rib_tx
                .send(gdp_packet)
                .await
                .expect("rib_tx channel closed!");
        }

        GdpAction::SubAdvertise => {
            //send the packet to RIB
            rib_tx
                .send(gdp_packet)
                .await
                .expect("rib_tx channel closed!");
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
