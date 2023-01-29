use crate::pipeline::{populate_gdp_struct_from_bytes, proc_gdp_packet};
use crate::structs::{GDPChannel, GDPName, GDPPacket, Packet};
use futures::future::join_all;
use std::io;
use tokio::io::{split, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, Sender};

const UDP_BUFFER_SIZE: usize = 4096; // 17480 17kb TODO: make it formal

/// handle one single session of tcpstream
/// 1. init and advertise the mpsc channel to connection rib
/// 2. select between
///         incoming tcp packets -> receive and send to rib
///         incomine packets from rib -> send to the tcp session
async fn handle_tcp_stream(
    stream: TcpStream, rib_tx: &Sender<GDPPacket>, channel_tx: &Sender<GDPChannel>,
) {
    // ...
    let (m_tx, mut m_rx) = mpsc::channel(32);
    // TODO: placeholder, later replace with packet parsing

    loop {
        // Wait for the TCP socket to be readable
        // or new data to be sent
        tokio::select! {
            // new stuff from TCP!
            _f = stream.readable() => {
                // Creating the buffer **after** the `await` prevents it from
                // being stored in the async task.

                let mut buf = vec![0u8; 1024];
                // Try to read data, this may still fail with `WouldBlock`
                // if the readiness event is a false positive.
                match stream.try_read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        println!("read {} bytes", n);
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(_e) => {
                        continue;
                    }
                }
                let packet = populate_gdp_struct_from_bytes(buf);
                proc_gdp_packet(packet,  // packet
                    rib_tx,  //used to send packet to rib
                    channel_tx, // used to send GDPChannel to rib
                    &m_tx //the sending handle of this connection
                ).await;

            },

            // new data to send to TCP!
            Some(pkt_to_forward) = m_rx.recv() => {
                // okay this may have deadlock
                stream.writable().await.expect("TCP stream is closed");


                let payload = pkt_to_forward.get_byte_payload().unwrap();
                // Try to write data, this may still fail with `WouldBlock`
                // if the readiness event is a false positive.
                match stream.try_write(&payload) {
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

// Establish a TCP connection with target router who is subscribing to gdpname
pub async fn setup_tcp_connection_to(
    gdpname: GDPName, address: String, rib_tx: Sender<GDPPacket>, channel_tx: Sender<GDPChannel>,
) {
    // TCP connect with peer router
    let socket = TcpStream::connect(address).await.unwrap();
    let (mut rd, mut wr) = split(socket);

    let (tx, mut rx) = mpsc::channel(32);

    let send_channel = GDPChannel {
        gdpname: gdpname,
        channel: tx.clone(),
    };

    channel_tx
        .send(send_channel)
        .await
        .expect("channel_tx channel closed!");

    let write_handle = tokio::spawn(async move {
        loop {
            let control_message = rx.recv().await.unwrap();

            // write the update message to buffer and flush the buffer
            let buffer = control_message.payload.unwrap();

            wr.write_all(&buffer).await;
        }
    });

    // read from target router tcp connection
    let read_handle = tokio::spawn(async move {
        loop {
            let mut buf = vec![0u8; UDP_BUFFER_SIZE];
            let n = rd.read(&mut buf).await.unwrap();

            // let gdp_packet = populate_gdp_struct_from_bytes(buf);
            // println!("Got a packet from peer. Packet = {:?}", gdp_packet.action);
            // proc_gdp_packet(buf.to_vec(), &rib_tx,&channel_tx, &tx.clone()).await;
            // rib_tx.send(gdp_packet).await.expect("rib_tx channel closed!");
            let rib_tx_clone = rib_tx.clone();
            let channel_tx_clone = channel_tx.clone();
            let tx_clone = tx.clone();
            tokio::spawn(async move {
                let packet = populate_gdp_struct_from_bytes(buf);
                proc_gdp_packet(packet, &rib_tx_clone, &channel_tx_clone, &tx_clone).await;
            });
        }
    });
    join_all([write_handle, read_handle]).await;
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
        tokio::spawn(async move { handle_tcp_stream(socket, &rib_tx, &channel_tx).await });
    }
}
