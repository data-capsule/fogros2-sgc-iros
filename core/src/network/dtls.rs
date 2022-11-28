use crate::network::udpstream::{UdpListener, UdpStream};
use crate::pipeline::{proc_gdp_packet, proc_rib_packet, populate_gdp_struct};
use std::{net::SocketAddr, pin::Pin, str::FromStr};

use futures::future::{join_all, join};
use futures::join;
use openssl::{
    pkey::PKey,
    ssl::{Ssl, SslAcceptor, SslConnector, SslContext, SslMethod, SslVerifyMode},
    x509::X509,
};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use utils::app_config::AppConfig;
use crate::structs::{GDPChannel, GDPPacket, GDPName, GdpAction};
use tokio::sync::mpsc::{self, Sender};

use async_recursion::async_recursion;

const UDP_BUFFER_SIZE: usize = 4096; // 17kb

static SERVER_CERT: &'static [u8] = include_bytes!("../../resources/router.pem");
static SERVER_KEY: &'static [u8] = include_bytes!("../../resources/router-private.pem");
const SERVER_DOMAIN: &'static str = "pourali.com";

/// helper function of SSL
fn ssl_acceptor(certificate: &[u8], private_key: &[u8]) -> std::io::Result<SslContext> {
    let mut acceptor_builder = SslAcceptor::mozilla_intermediate(SslMethod::dtls())?;
    acceptor_builder.set_certificate(&&X509::from_pem(certificate)?)?;
    acceptor_builder.set_private_key(&&PKey::private_key_from_pem(private_key)?)?;
    acceptor_builder.check_private_key()?;
    let acceptor = acceptor_builder.build();
    Ok(acceptor.into_context())
}

/// handle one single session of dtls
/// 1. init and advertise the mpsc channel to connection rib
/// 2. select between
///         incoming dtls packets -> receive and send to rib
///         incomine packets from rib -> send to the tcp session
async fn handle_dtls_stream(
    socket: UdpStream, acceptor: SslContext, rib_tx: &Sender<GDPPacket>,
    channel_tx: &Sender<GDPChannel>, is_remote_rib: bool
) {
    let (m_tx, mut m_rx) = mpsc::channel(32);
    let ssl = Ssl::new(&acceptor).unwrap();
    let mut stream = tokio_openssl::SslStream::new(ssl, socket).unwrap();
    Pin::new(&mut stream).accept().await.unwrap();

    println!("handling new dtls stream");

    loop {
        // TODO:
        // Question: what's the bahavior here, will it keep allocating memory?
        let mut buf = vec![0u8; UDP_BUFFER_SIZE];
        // Wait for the UDP socket to be readable
        // or new data to be sent
        tokio::select! {
            Some(pkt_to_forward) = m_rx.recv() => {
                let packet: &GDPPacket = &pkt_to_forward;
                stream.write_all(&packet.payload[..packet.payload.len()]).await.unwrap();
            }
            // _ = do_stuff_async()
            // async read is cancellation safe
            _ = stream.read(&mut buf) => {
                // NOTE: if we want real time system bound
                // let n = match timeout(Duration::from_millis(UDP_TIMEOUT), stream.read(&mut buf))
                if is_remote_rib {
                    proc_rib_packet(buf.to_vec(),
                        rib_tx,
                        channel_tx,
                        &m_tx
                    ).await
                } else {
                    proc_gdp_packet(buf.to_vec(),  // packet
                        rib_tx,  //used to send packet to rib
                        channel_tx, // used to send GDPChannel to rib
                        &m_tx //the sending handle of this connection
                    ).await;
                }
                
            },
        }
    }
}

pub async fn dtls_listener(
    addr: String, rib_tx: Sender<GDPPacket>, channel_tx: Sender<GDPChannel>, is_remote_rib: bool
) {
    let listener = UdpListener::bind(SocketAddr::from_str(&addr).unwrap())
        .await
        .unwrap();
    let acceptor = ssl_acceptor(SERVER_CERT, SERVER_KEY).unwrap();
    // tokio::spawn(async {
    //     dtls_test_client3("127.0.0.1:9234").await;
    // });
    
    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let rib_tx = rib_tx.clone();
        let channel_tx = channel_tx.clone();
        let acceptor = acceptor.clone();
        tokio::spawn(
            async move { handle_dtls_stream(socket, acceptor, &rib_tx, &channel_tx, is_remote_rib).await },
        );
    }
}

#[tokio::main]
pub async fn dtls_test_client(addr: &str) -> std::io::Result<SslContext> {
    // let stream = UdpStream::connect(SocketAddr::from_str(addr).unwrap()).await?;

    // // setup ssl
    // let mut connector_builder = SslConnector::builder(SslMethod::dtls())?;
    // connector_builder.set_verify(SslVerifyMode::NONE);
    // let connector = connector_builder.build().configure().unwrap();
    // let ssl = connector.into_ssl(SERVER_DOMAIN).unwrap();
    // let mut stream = tokio_openssl::SslStream::new(ssl, stream).unwrap();
    // Pin::new(&mut stream).connect().await.unwrap();

    // // split the stream into read half and write half
    // let (mut rd, mut wr) = tokio::io::split(stream);

    // // read: separate thread
    // let _dtls_sender_handle = tokio::spawn(async move {
    //     loop {
    //         let mut buf = vec![0u8; 1024];
    //         let n = rd.read(&mut buf).await.unwrap();
    //         print!("-> {}", String::from_utf8_lossy(&buf[..n]));
    //     }
    // });

    // loop {
    //     let mut buffer = String::new();
    //     std::io::stdin().read_line(&mut buffer)?;
    //     wr.write_all(buffer.as_bytes()).await?;
    
    // }
    dtls_test_client2(addr).await
}

pub async fn dtls_test_client2(addr: &str) -> std::io::Result<SslContext> {
    let stream = UdpStream::connect(SocketAddr::from_str(addr).unwrap()).await?;

    // setup ssl
    let mut connector_builder = SslConnector::builder(SslMethod::dtls())?;
    connector_builder.set_verify(SslVerifyMode::NONE);
    let connector = connector_builder.build().configure().unwrap();
    let ssl = connector.into_ssl(SERVER_DOMAIN).unwrap();
    let mut stream = tokio_openssl::SslStream::new(ssl, stream).unwrap();
    Pin::new(&mut stream).connect().await.unwrap();

    // split the stream into read half and write half
    let (mut rd, mut wr) = tokio::io::split(stream);

    // read: separate thread
    let _dtls_sender_handle = tokio::spawn(async move {
        loop {
            let mut buf = vec![0u8; 1024];
            let n = rd.read(&mut buf).await.unwrap();
            print!("-> {}", String::from_utf8_lossy(&buf[..n]));
        }
    });

    loop {
        let mut buffer = String::new();
        std::io::stdin().read_line(&mut buffer)?;
        // wr.write_all(buffer.as_bytes()).await?;
        let add_clone = addr.to_string();
        tokio::spawn(async move {
            dtls_test_client3(&add_clone).await;
        });
    }
}


pub async fn dtls_test_client3(addr: &str) -> std::io::Result<SslContext> {
    let stream = UdpStream::connect(SocketAddr::from_str(addr).unwrap()).await?;

    // setup ssl
    let mut connector_builder = SslConnector::builder(SslMethod::dtls())?;
    connector_builder.set_verify(SslVerifyMode::NONE);
    let connector = connector_builder.build().configure().unwrap();
    let ssl = connector.into_ssl(SERVER_DOMAIN).unwrap();
    let mut stream = tokio_openssl::SslStream::new(ssl, stream).unwrap();
    Pin::new(&mut stream).connect().await.unwrap();

    // split the stream into read half and write half
    let (mut rd, mut wr) = tokio::io::split(stream);

    // read: separate thread
    let _dtls_sender_handle = tokio::spawn(async move {
        loop {
            let mut buf = vec![0u8; 1024];
            let n = rd.read(&mut buf).await.unwrap();
            print!("-> {}", String::from_utf8_lossy(&buf[..n]));
        }
    });

    let _writer_handle = tokio::spawn(async move {
        loop {
            let mut buffer = String::new();
            std::io::stdin().read_line(&mut buffer).unwrap();
            wr.write_all(buffer.as_bytes()).await.unwrap();
        }
    });
    
    join_all([_dtls_sender_handle, _writer_handle]).await;
    unreachable!();
}

// Connect to a target rib with dTLS.
// todo: change gdpname from u32 to real gdpname
pub async fn connect_rib(gdpname: u32, address: String, rib_tx: Sender<GDPPacket>, channel_tx: Sender<GDPChannel>){
    let stream = UdpStream::connect(SocketAddr::from_str(&address).unwrap()).await.unwrap();

    let mut connector_builder = SslConnector::builder(SslMethod::dtls()).unwrap();
    connector_builder.set_verify(SslVerifyMode::NONE);
    let connector = connector_builder.build().configure().unwrap();
    let ssl = connector.into_ssl(SERVER_DOMAIN).unwrap();
    let mut stream = tokio_openssl::SslStream::new(ssl, stream).unwrap();
    Pin::new(&mut stream).connect().await.unwrap();

    // println!("Code go pass Pin Stream");

    // split the stream into read half and write half
    let (mut rd, mut wr) = tokio::io::split(stream);

    let (tx, mut rx) = mpsc::channel(32);


    // todo: replace with real GDPName
    let m_gdp_name = match gdpname {
        1 => GDPName([1, 1, 1, 1]),
        2 => GDPName([2, 2, 2, 2]),
        3 => GDPName([3, 3, 3, 3]),
        4 => GDPName([4, 4, 4, 4]),
        5 => GDPName([5, 5, 5, 5]),
        6 => GDPName([6, 6, 6, 6]),
        _ => GDPName([0, 0, 0, 0]),
    };

    let send_channel = GDPChannel {
        gdpname: m_gdp_name, // All GDP control-plane messages use the default route
        channel: tx.clone(),
    };

    println!("Sent channel of gdpname {:?} from connect_target", gdpname);

    channel_tx.send(send_channel).await.expect("channel_tx channel closed!");


    // Send a ADV message to the target in order to register self
    let local_rib_name = AppConfig::get::<u32>("router_name").expect("Cannot advertise current router. Reason: no gdpname assigned");
    // todo: 1. when hello to parent rib, it's ok to treat self as a client to the parent rib. (current version)
    // todo: 2. when hello to peer routers, need to provide querying client's gdpname instead of router gdpname, so that two-way e2e connection is established
    let config_dtls_addr = AppConfig::get::<String>("DTLS_ADDR").unwrap();
    let config_dtls_port: &str = config_dtls_addr.split(':').collect::<Vec<&str>>()[1];
    let mut local_ip_address = AppConfig::get::<String>("local_ip").unwrap();
    local_ip_address.push_str(&format!(":{}", config_dtls_port));
    let buffer = format!("ADV,{},{}", local_rib_name, config_dtls_addr).as_bytes().to_vec();
    wr.write_all(&buffer).await.unwrap();
    
    // Listen for any to-be-send message
    let write_handle = tokio::spawn( async move {
        loop {
            let control_message = rx.recv().await.unwrap();

            // write the update message to buffer and flush the buffer
            let buffer = control_message.payload;

            wr.write_all(&buffer).await;
        }
    });

    // read from target rib dlts connection
    let read_handle = tokio::spawn(async move {
        loop {
            dbg!("connect_rib: read_handle loop once! ");

            let mut buf = vec![0u8; 64];
            let n = rd.read(&mut buf).await.unwrap();
            
            // let gdp_packet = populate_gdp_struct(buf);
            // proc_gdp_packet(buf.to_vec(), &rib_tx,&channel_tx, &tx.clone()).await;
            // rib_tx.send(gdp_packet).await.expect("rib_tx channel closed!");
            let gdp_packet = populate_gdp_struct(buf);
            let action = gdp_packet.action;
            let gdp_name = gdp_packet.gdpname;
            
            if action == GdpAction::RibReply {
                println!("Received RiBReply");
                let received_str: Vec<&str> = std::str::from_utf8(&gdp_packet.payload)
                    .unwrap()
                    .trim()
                    .split(",")
                    .collect();
                // format = {REPLY, 127.0.0.1:9232}
                let ip_address = received_str[1].trim().trim_end_matches('\0').to_string();
        
                let clone_rib_tx = rib_tx.clone();
                let clone_channel_tx = channel_tx.clone();
                let conn_handle = tokio::spawn(async move {
                    // connect_a_router(gdp_name.0[0].into(), ip_address, clone_rib_tx, clone_channel_tx).await;
                    dtls_test_client3(&ip_address).await;
                });
                // join!(conn_handle);
            }
            
        }
    });

    join_all([write_handle, read_handle]).await;
    unreachable!()

}


pub async fn connect_a_router(gdpname: u32, address: String, rib_tx: Sender<GDPPacket>, channel_tx: Sender<GDPChannel>){
    println!("Code enters connect_target");
    let stream = UdpStream::connect(SocketAddr::from_str(&address).unwrap()).await.unwrap();
    println!("Code established UDP connection with {:?}", address);
    // setup ssl
    let mut connector_builder = SslConnector::builder(SslMethod::dtls()).unwrap();
    connector_builder.set_verify(SslVerifyMode::NONE);
    let connector = connector_builder.build().configure().unwrap();
    let ssl = connector.into_ssl(SERVER_DOMAIN).unwrap();
    let mut stream = tokio_openssl::SslStream::new(ssl, stream).unwrap();
    Pin::new(&mut stream).connect().await.unwrap();

    println!("Code go pass Pin Stream");

    // split the stream into read half and write half
    let (mut rd, mut wr) = tokio::io::split(stream);

    let (tx, mut rx) = mpsc::channel(32);


    // todo: replace with real GDPName
    let m_gdp_name = match gdpname {
        1 => GDPName([1, 1, 1, 1]),
        2 => GDPName([2, 2, 2, 2]),
        3 => GDPName([3, 3, 3, 3]),
        4 => GDPName([4, 4, 4, 4]),
        5 => GDPName([5, 5, 5, 5]),
        6 => GDPName([6, 6, 6, 6]),
        _ => GDPName([0, 0, 0, 0]),
    };

    let send_channel = GDPChannel {
        gdpname: m_gdp_name, // All GDP control-plane messages use the default route
        channel: tx.clone(),
    };

    println!("Sent channel of gdpname {:?} from connect_target", gdpname);

    channel_tx.send(send_channel).await.expect("channel_tx channel closed!");

    
    

    // Send a ADV message to the target in order to register self
    let local_rib_name = AppConfig::get::<u32>("router_name").expect("Cannot advertise current router. Reason: no gdpname assigned");
    // todo: 1. when hello to parent rib, it's ok to treat self as a client to the parent rib. (current version)
    // todo: 2. when hello to peer routers, need to provide querying client's gdpname instead of router gdpname, so that two-way e2e connection is established
    let config_dtls_addr = AppConfig::get::<String>("DTLS_ADDR").unwrap();
    let config_dtls_port: &str = config_dtls_addr.split(':').collect::<Vec<&str>>()[1];
    let mut local_ip_address = AppConfig::get::<String>("local_ip").unwrap();
    local_ip_address.push_str(&format!(":{}", config_dtls_port));
    let buffer = format!("ADV,{},{}", local_rib_name, config_dtls_addr).as_bytes().to_vec();
    wr.write_all(&buffer).await.unwrap();
    
    // Listen for any to-be-send message
    let write_handle = tokio::spawn( async move {
        loop {
            let control_message = rx.recv().await.unwrap();

            // write the update message to buffer and flush the buffer
            let buffer = control_message.payload;

            wr.write_all(&buffer).await;
        }
    });

    // read from target rib dlts connection
    let read_handle = tokio::spawn(async move {
        loop {
            let mut buf = vec![0u8; 64];
            let n = rd.read(&mut buf).await.unwrap();
            
            let gdp_packet = populate_gdp_struct(buf);
            println!("Got a packet from peer. Packet = {:?}", gdp_packet.action);
            // proc_gdp_packet(buf.to_vec(), &rib_tx,&channel_tx, &tx.clone()).await;
            // rib_tx.send(gdp_packet).await.expect("rib_tx channel closed!");
            
        }
    });
    join_all([write_handle, read_handle]).await;
    
    
    unreachable!()

}

// // ! Solution for recursive spawn tested here: https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=73006606a3d76de1a49aeb2138e17bbe
// pub async fn spawn_connect_target(gdpname: u32, address: String, rib_tx: Sender<GDPPacket>, channel_tx: Sender<GDPChannel>) {
    
//     tokio::spawn(async move {
//         println!("Trying to pair with {:?} using dtls", address);
//         connect_rib(gdpname, address, rib_tx, channel_tx).await.expect("Unable to connect to target!");
//     }).await.unwrap();
// }


