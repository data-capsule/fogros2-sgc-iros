use crate::pipeline::{populate_gdp_struct_from_bytes, proc_gdp_packet};
use crate::structs::{GDPChannel, GDPPacket, Packet, GdpAction};
use std::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, Sender, Receiver};
use std::{net::SocketAddr, pin::Pin, str::FromStr};
use crate::structs::GDPName;
use crate::pipeline::construct_gdp_advertisement_from_bytes;
use crate::structs::get_gdp_name_from_topic;
const UDP_BUFFER_SIZE: usize = 4096; // 17480 17kb TODO: make it formal
use serde::{Serialize, Deserialize};
use crate::structs::GDPPacketInTransit;
use crate::pipeline::construct_gdp_forward_from_bytes;
use rand::Rng;
use serde_bytes::ByteBuf;
fn generate_random_gdp_name_for_thread() -> GDPName{
    // u8:4
    GDPName([
        rand::thread_rng().gen(), 
        rand::thread_rng().gen(),
        rand::thread_rng().gen(),
        rand::thread_rng().gen()
    ])
}


/// handle one single session of tcpstream
/// 1. init and advertise the mpsc channel to connection rib
/// 2. select between
///         incoming tcp packets -> receive and send to rib
///         incomine packets from rib -> send to the tcp session
async fn handle_tcp_stream(
    stream: TcpStream, rib_tx: &Sender<GDPPacket>, channel_tx: &Sender<GDPChannel>,
    m_tx:Sender<GDPPacket>, mut m_rx:Receiver<GDPPacket>, 
    thread_name: GDPName
) {
    // ...
    
    // TODO: placeholder, later replace with packet parsing

    loop {
        // Wait for the TCP socket to be readable
        // or new data to be sent
        tokio::select! {
            // new stuff from TCP!
            _f = stream.readable() => {
                // Creating the buffer **after** the `await` prevents it from
                // being stored in the async task.

                let mut buf = vec![0u8; UDP_BUFFER_SIZE];
                let mut buffer_size = 0;
                // Try to read data, this may still fail with `WouldBlock`
                // if the readiness event is a false positive.
                match stream.try_read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        println!("read {} bytes", n);
                        buffer_size = n;
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(_e) => {
                        continue;
                    }
                }

                let deserialized:GDPPacketInTransit = serde_json::from_str(std::str::from_utf8(&buf[..buffer_size]).unwrap()).unwrap();
                if (deserialized.action == GdpAction::Forward) {
                    let packet = construct_gdp_forward_from_bytes(deserialized.destination, thread_name, deserialized.payload);
                    proc_gdp_packet(packet,  // packet
                        rib_tx,  //used to send packet to rib
                        channel_tx, // used to send GDPChannel to rib
                        &m_tx //the sending handle of this connection
                    ).await;
                }
                else if (deserialized.action == GdpAction::Advertise) {
                    let packet = construct_gdp_advertisement_from_bytes(deserialized.destination, thread_name);
                    proc_gdp_packet(packet,  // packet
                        rib_tx,  //used to send packet to rib
                        channel_tx, // used to send GDPChannel to rib
                        &m_tx //the sending handle of this connection
                    ).await;
                }
                else{
                    info!("TCP received a packet but did not handle: {:?}", deserialized)
                }

            },

            // new data to send to TCP!
            Some(pkt_to_forward) = m_rx.recv() => {
                // okay this may have deadlock
                stream.writable().await.expect("TCP stream is closed");

                // info!("TCP packet to forward: {:?}", pkt_to_forward);
                // let payload = match pkt_to_forward.payload {
                //     Some(payload) => {payload},  //;
                //     _ => {pkt_to_forward.get_serialized().into()}
                // };
                

                
                let payload = pkt_to_forward.to_transit();
                let serialized = serde_json::to_string(&payload).unwrap();
                info!("serialized{:?}", serialized.len());

                let s = serde_bytes::ByteBuf::from(payload.payload);
                

                // // Deserialize
                // let deserialized: GDPPacketInTransit = serde_json::from_str(&serialized).unwrap();
                            
                // Convert the Point to a JSON string.
                // Try to write data, this may still fail with `WouldBlock`
                // if the readiness event is a false positive.
                match stream.try_write(serialized.as_ref()) {
                    Ok(n) => {
                        println!("write {} bytes", n);
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        continue
                    }
                    Err(_e) => {
                        println!("Err of other kind");
                        continue
                    }
                }
            },
        }
    }
}

/// listen at @param address and process on tcp accept()
///     rib_tx: channel that send GDPPacket to rib
///     channel_tx: channel that advertise GDPChannel to rib
pub async fn tcp_listener(addr: String, rib_tx: Sender<GDPPacket>, channel_tx: Sender<GDPChannel>) {
    let listener = TcpListener::bind(&addr).await.unwrap();
    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let rib_tx = rib_tx.clone();
        let channel_tx = channel_tx.clone();

        // Process each socket concurrently.
        tokio::spawn(async move { 
            let (m_tx, mut m_rx) = mpsc::channel(32);
            handle_tcp_stream(socket, &rib_tx, &channel_tx, m_tx, m_rx, generate_random_gdp_name_for_thread()).await 
        });
    }
}



pub async fn tcp_to_peer(addr: String, 
    rib_tx: Sender<GDPPacket>,
    channel_tx: Sender<GDPChannel>) {
    
    let stream = TcpStream::connect(SocketAddr::from_str(&addr).unwrap()).await.unwrap();
    println!("{:?}", stream);

    let m_gdp_name = generate_random_gdp_name_for_thread(); 
    info!("TCP takes gdp name {:?}", m_gdp_name);

    let (m_tx, mut m_rx) = mpsc::channel(32);
    let node_advertisement = construct_gdp_advertisement_from_bytes(
        m_gdp_name, m_gdp_name
    );
    proc_gdp_packet(
        node_advertisement, // packet
        &rib_tx,            //used to send packet to rib
        &channel_tx,        // used to send GDPChannel to rib
        &m_tx,              //the sending handle of this connection
    )
    .await;
    handle_tcp_stream(stream, &rib_tx, &channel_tx, m_tx,m_rx, m_gdp_name).await;
}