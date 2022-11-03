
extern crate pnet;
extern crate pnet_macros_support;

use pnet::packet::{Packet, MutablePacket};
use pnet::datalink::{self, NetworkInterface, DataLinkSender};
use pnet::datalink::Channel::Ethernet;
use pnet::packet::ethernet::{EtherTypes, EthernetPacket, MutableEthernetPacket};
use pnet::packet::ip::{IpNextHeaderProtocol, IpNextHeaderProtocols};
use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::udp::UdpPacket;
use pnet::util::MacAddr;
use pnet_packet::ipv4::{MutableIpv4Packet, checksum};
use pnet_packet::udp::MutableUdpPacket;
use std::net::Ipv4Addr;

use utils::app_config::AppConfig;
use utils::conversion::str_to_ipv4;
use crate::protocol::GDP_protocol::{GdpProtocolPacket, MutableGdpProtocolPacket};
use crate::rib::RoutingInformationBase;
use crate::structs::GdpAction;

// Ethernet frame is 1500 bytes payload + 14 bytes header 
// cannot go beyond this size
const MAX_ETH_FRAME_SIZE : usize = 1514;  
const LEFT: Ipv4Addr = Ipv4Addr::new(128, 32, 37, 69);
const RIGHT: Ipv4Addr = Ipv4Addr::new(128, 32, 37, 41);

fn handle_gdp_packet(
    packet: &[u8], 
    res_gdp: &mut MutableGdpProtocolPacket,
    ) -> Option<()> {
    let gdp_protocol_packet = GdpProtocolPacket::new(packet);
    
    
    if let Some(gdp) = gdp_protocol_packet {
        println!("Received GDP Packet: {:?}\n", gdp);

        // create new gdp packet
        // let mut vec: Vec<u8> = vec![0; packet.len()];
        // let mut new_packet = MutableUdpPacket::new(&mut vec[..]).unwrap();
        // new_packet.clone_from(udp);
        res_gdp.clone_from(&gdp);
        res_gdp.set_src_gdpname(&gdp.get_dst_gdpname());
        res_gdp.set_dst_gdpname(&gdp.get_src_gdpname());
        println!("Received buffer is {:?}", String::from_utf8(res_gdp.payload().to_vec()));
        let payload:String = "t".to_string();
        res_gdp.set_data_len(payload.len() as u16);
        res_gdp.set_payload((payload.to_owned()).as_bytes());
        //res_gdp.set_payload(Vec::from())
        
        // println!("{:?}", String::from_utf8(res_gdp.payload().to_vec()));
        println!("The constructed gdp packet is = {:?}\n", res_gdp);
        // println!("The buffer for the above packet is = {:?}\n", vec);
        Some(())
    } else {
        println!("Malformed GDP Packet");
        None
    }
}

fn handle_udp_packet(
    packet: &[u8], 
    res_udp:&mut MutableUdpPacket,
    config: &AppConfig) -> Option<()>
    {
    let udp = UdpPacket::new(packet);

    if let Some(udp) = udp {
        let mut res_gdp  = MutableGdpProtocolPacket::new(&mut res_udp.payload_mut()[..]).unwrap();
        let res = handle_gdp_packet(udp.payload(), &mut res_gdp);
        if let Some(payload) = res {
            res_udp.clone_from(&udp);
            res_udp.set_checksum(res_udp.to_immutable().get_checksum());
            println!("Constructed UDP packet = {:?}", res_udp);
            Some(())
        } else {None}
    } else {
        println!("Malformed UDP Packet");
        None
    }
}


fn handle_ipv4_packet( 
    ethernet: &EthernetPacket, 
    res_ipv4: &mut MutableIpv4Packet,
    destination_ip: Ipv4Addr,
    config: &AppConfig) -> Option<()>{
    let header = Ipv4Packet::new(ethernet.payload());
    if let Some(header) = header {

        let mut res_udp  = MutableUdpPacket::new(&mut res_ipv4.payload_mut()[..]).unwrap();

        let res = handle_udp_packet(
            header.payload(),
            &mut res_udp,
            config,
        );
        if let Some(payload) = res {
            let udp_packet_size = res_udp.get_length() as u16;
            let ipv4_header_size = header.get_header_length() as u16;
            let packet_size = (( udp_packet_size + ipv4_header_size ) as usize)*4;
            res_ipv4.set_total_length((packet_size.try_into().unwrap()));
            
            // Simple forwarding based on configuration
            res_ipv4.clone_from(&header);
            res_ipv4.set_destination(destination_ip);
            res_ipv4.set_source(header.get_destination());
            res_ipv4.set_checksum(checksum(&res_ipv4.to_immutable()));
            
            println!("Constructed IP packet = {:?}", res_ipv4);
            Some(())

        } else {None}
    } else {
        println!(" Malformed IPv4 Packet");
        None
    }
}

pub fn gdp_pipeline(
    packet : &[u8], 
    gdp_rib: &mut RoutingInformationBase, 
    interface: &NetworkInterface , 
    tx: &mut Box<dyn DataLinkSender>, 
    config: &AppConfig
) -> Option<()>
{
    //TODO: later we are going to separate the send logic as well 

    let ethernet = EthernetPacket::new(packet).unwrap(); 

    // Packet filtering 
    // reasoning having a monolithic body here: 
    // this part is supposed to be a quick filter embedded anywhere our networking logic belongs to 
    // for example, embeded in the kernel
    // as such, it has to be standalone to other components of the code, e.g. routing logic 
    // this also allows zero copy of the write buffer 

    // check ipv4 
    if ethernet.get_ethertype() != EtherTypes::Ipv4 {
       return None;
    }
    // check ipv4 has a payload
    let ipv4_packet = match Ipv4Packet::new(ethernet.payload()) {
        Some (packet) => packet, 
        _ => return None
    }; 

    let m_ip = str_to_ipv4(&config.ip_local); 
    //check ipv4 destination
    if ipv4_packet.get_destination() != m_ip {
        return None;
    }

    // check udp header
    if ipv4_packet.get_next_level_protocol() != IpNextHeaderProtocols::Udp{
        return None;
    }

    // check udp packet is included as payload 
    let udp = match UdpPacket::new(ipv4_packet.payload()) {
        Some(packet) => packet, 
        _ => return None
    };

    // check port goes to our predefined port 
    if udp.get_destination() != config.router_port {
        return None;
    }

    // check gdp packet is included as payload 
    let gdp_protocol_packet = match GdpProtocolPacket::new(udp.payload()) {
        Some(packet) => packet, 
        _ => return None
    };
    
    let gdp_action = GdpAction::try_from(gdp_protocol_packet.get_action()).unwrap();

    match gdp_action {
        GdpAction::RibGet => {
            // handle rib query by responding with the RIB item
            let dst_gdp_name = gdp_protocol_packet.get_dst_gdpname().clone(); 
            gdp_rib.get(Vec::from([7, 1, 2, 3]));
        }, 
        GdpAction::RibReply => {
            // update local rib with the rib reply
            // below is simply an example
            let dst_gdp_name = gdp_protocol_packet.get_dst_gdpname().clone(); 
            gdp_rib.put(Vec::from([7, 1, 2, 3]), LEFT); //just an example
        }, 
        GdpAction::Forward => {
            // forward the data to the next hop
            let destination_gdp_name = gdp_protocol_packet.get_dst_gdpname();
            let destination_ips = gdp_rib.get(destination_gdp_name);
            let mut mcast_counter = 0;

            // Note: num_packets in build_and_send allows to rewrite the same buffer num_packets times (first param)
            // we can use this to implement mcast and constantly rewrite the write buffer
            match tx.build_and_send(destination_ips.len(), MAX_ETH_FRAME_SIZE,
                &mut |mut res_ether| {
                    println!("Now sending to Address = {:}", destination_ips[mcast_counter]);

                    // res_ether is the write buffer that finally goes to the network
                    let mut res_ether = MutableEthernetPacket::new(&mut res_ether).unwrap();
                    res_ether.clone_from(&ethernet);

                    // res_ipv4 is the pointer of res_ether pointing to the ethernet payload (i.e. start of the ip address)
                    let mut res_ipv4 = MutableIpv4Packet::new(&mut res_ether.payload_mut()[..]).unwrap();
                    let res = handle_ipv4_packet(&ethernet, &mut res_ipv4, destination_ips[mcast_counter], &config);
                    
                    //res_ether.set_payload(&payload);
                    res_ether.set_destination(MacAddr::broadcast());
                    res_ether.set_source(interface.mac.unwrap());
                    println!("Constructed Ethernet packet = {:?}", res_ether);
                    mcast_counter+= 1;
            }) 
            {
                Some(result) => match result {
                    Ok(_) => {}, 
                    Err(err_msg) => {println!("send error with {:?}", err_msg) }
                }, 
                None => {println!("Err: send function return none!!!"); }
            }
            
        }, 
        GdpAction::Noop => {
            // nothing to be done here
            println!("GDP Action Noop Detected, Make sure to update to 5 for forwarding");
        }, 
        _ =>{
            // control, put, get 
            println!("{:?} is not implemented yet", gdp_action)
        }
    }
    
    Some(())
}
