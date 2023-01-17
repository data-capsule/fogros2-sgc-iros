extern crate multimap;
use multimap::MultiMap;
use redis::{Connection, Commands};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use utils::app_config::AppConfig;
use utils::conversion::str_to_ipv4;


pub struct RoutingInformationBase {
    pub routing_table: MultiMap<Vec<u8>, Ipv4Addr>,
    pub default_route: Vec<Ipv4Addr>,
}

impl RoutingInformationBase {
    pub fn new(config: &AppConfig) -> RoutingInformationBase {
        // TODO: config can populate the RIB somehow
        let m_gateway_addr = str_to_ipv4(&"localhost".to_owned());

        RoutingInformationBase {
            routing_table: MultiMap::new(),
            default_route: Vec::from([m_gateway_addr]),
        }
    }

    pub fn put(&mut self, key: Vec<u8>, value: Ipv4Addr) -> Option<()> {
        self.routing_table.insert(key, value);
        Some(())
    }

    // rust doesn't support default param.
    pub fn get_no_default(&self, key: Vec<u8>) -> Option<&Vec<Ipv4Addr>> {
        self.routing_table.get_vec(&key)
    }

    pub fn get(&self, key: Vec<u8>) -> &Vec<Ipv4Addr> {
        match self.get_no_default(key) {
            Some(v) => v,
            None => &self.default_route,
        }
    }
}


pub struct RIBClient {
    pub con: Connection,   // Connection to the remote RIB
    pub ip_address: String // Current GDP router's IP 
}

#[derive(Serialize, Deserialize)]
struct TopicMeta {
    pub_count: u64, // total number of publisher
    sub_count: u64, // total number of subscriber
    // todo: more attributes to be added. 
    // For example, a "status" attribute tells 
    //   whether this topic is currently used by someone
}

#[derive(Serialize, Deserialize)]
pub struct TopicRecord {
    pub ip_address: String,
    pub node_type: u8, // 0 represents sub node, 1 represents pub node
}

impl RIBClient {
    pub fn new(url: &str) -> redis::RedisResult<RIBClient> {
        let client = redis::Client::open(url)?;
        let con = client.get_connection()?;
        
        let ip_address: String = AppConfig::get("ip_address").expect("Failed to load ip_address from config file");

        Ok(RIBClient {
            con,
            ip_address
        })
    }

    pub fn create_pub_node(&mut self, topic_name: &str) -> redis::RedisResult<HashMap<String, String>> {
        // keys will be watched for the transaction
        // if corresponding values changed, transaction will be aborted
        // and retry in a loop until it succeed
        let key = topic_name;
        
        let result : HashMap<String, String> = redis::transaction(&mut self.con, &[key], |con, pipe| {
            let exists: i32 = con.exists(key)?;

            if exists == 0 {
                let new_topic_meta = TopicMeta {pub_count: 1, sub_count: 0};
                
                let record = serde_json::to_string(&TopicRecord {ip_address: self.ip_address.clone(), node_type: 1}).expect("TopicRecord serialization failed");
                
                let _ : Option<()> = pipe
                    .set(key, serde_json::to_string(&new_topic_meta).expect("TopicMeta serialization failed")).ignore()
                    .set(format!("{}/pub/1", key), record).ignore() 
                    .query(con)?;

                Ok(Some(HashMap::new()))

            } else {
                let topic_meta_str: String = con.get(key)?;
                let mut topic_meta: TopicMeta = serde_json::from_str(&topic_meta_str).expect("TopicMeta deserialization failed");
                topic_meta.pub_count += 1;
                let sub_node_keys =  (1..=topic_meta.sub_count).map(&|i| format!("{}/sub/{}", key, i)).collect::<Vec<String>>();

                let record = serde_json::to_string(&TopicRecord {ip_address: self.ip_address.clone(), node_type: 1}).expect("TopicRecord serialization failed"); 
                
                if topic_meta.sub_count == 0 {
                    let _ : Option<()> = pipe
                        .set(key, serde_json::to_string(&topic_meta).expect("TopicMeta serialization failed")).ignore()
                        .set(format!("{}/pub/{}", key, topic_meta.pub_count), record).ignore()
                        .query(con)?;
                    
                    Ok(Some(HashMap::new()))
                } else {
                    pipe
                    .set(key, serde_json::to_string(&topic_meta).expect("TopicMeta serialization failed")).ignore()
                    .set(format!("{}/pub/{}", key, topic_meta.pub_count), record).ignore() 
                    .mget(&sub_node_keys)
                    .query(con)
                    .map(|option: Option<Vec<Vec<String>>>| 
                        option.map(|mut outer_vec| outer_vec.pop().unwrap().into_iter()
                        .zip((sub_node_keys).into_iter()).map(|(value, key)| (key, value)).collect::<HashMap<String, String>>()
                    )
                )
            }
            }
        })?;

        println!("{:?}", result);
        Ok(result)
    }

    pub fn create_sub_node(&mut self, topic_name: &str) -> redis::RedisResult<()> {
        // keys will be watched for the transaction
        // if corresponding values changed, transaction will be aborted
        // and retry in a loop until it succeed
        let key = topic_name;

        let _result : () = redis::transaction(&mut self.con, &[key], |con, pipe| {
            let exists: i32 = con.exists(key)?;

            let record = serde_json::to_string(&TopicRecord {ip_address: self.ip_address.clone(), node_type: 0}).expect("TopicRecord serialization failed");
            
            if exists == 0 {
                let new_topic_meta = TopicMeta {pub_count: 0, sub_count: 1};
                
                pipe
                    .set(key, serde_json::to_string(&new_topic_meta).expect("TopicMeta serialization failed")).ignore()
                    .set(format!("{}/sub/1", key), record).ignore()
                    .query(con)
            } else {
                let topic_meta_str: String = con.get(key)?;
                let mut topic_meta: TopicMeta = serde_json::from_str(&topic_meta_str).expect("TopicMeta deserialization failed");
                topic_meta.sub_count += 1;
                pipe
                    .set(key, serde_json::to_string(&topic_meta).expect("TopicMeta serialization failed")).ignore()
                    .set(format!("{}/sub/{}", key, topic_meta.sub_count), record).ignore() 
                    .query(con)
            }
        })?;
        Ok(()) 
    }
}


mod tests {
    use super::*;

    // Initialize the configuration object. 
    // This is because when running test, the main entrance function is not executed,
    // but we need some information from config while running tests.
    // Thus these values are faked here
    // 
    fn config_setup() {
        AppConfig::init(Some("")).unwrap();
        AppConfig::set("ip_address", "128.32.37.82").expect("Config attribute cannot be set");
    }


    #[test]
    fn create_a_pub_node() {
        config_setup();
        let mut client = RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();
        client.create_pub_node("helloworld").unwrap();
    }

    #[test]
    fn create_a_sub_node() {
        config_setup();
        let mut client = RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();
        client.create_sub_node("GDPName([2, 2, 2, 2])").unwrap();
    }


    #[test]
    fn create_pub_nodes_multi_threads() {
        config_setup();
        let mut client = RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();

        let mut client_2= RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();
        let handle = std::thread::spawn(move || {
            for i in 0..10 {
                println!("Iteration {} from thread 2", i);
                client_2.create_pub_node("helloworld");
            }
        });

        for i in 0..10 {
            println!("Iteration {} from thread 1", i);
            client.create_pub_node("helloworld");
        } 

        handle.join().unwrap();
    }

    #[test]
    fn create_sub_nodes_multi_threads() {
        config_setup();
        let mut client = RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();

        let mut client_2= RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();
        let handle = std::thread::spawn(move || {
            for i in 0..10 {
                println!("Iteration {} from thread 2", i);
                client_2.create_sub_node("helloworld");
            }
        });

        for i in 0..10 {
            println!("Iteration {} from thread 1", i);
            client.create_sub_node("helloworld");
        } 

        handle.join().unwrap();
    }

    #[test]
    fn redis_basics() {
        config_setup();
        let mut client = RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();
        let mykey: Option<String> = client.con.get("mykey").unwrap();
        dbg!(mykey);

        let exists: i32 = client.con.exists("mykey").unwrap();
        dbg!(exists);

        let result: () = client.con.set("nihao", "shijie").unwrap();
        dbg!(result);

        let result: Vec<String> = client.con.mget(&["helloworld/sub/1", "helloworld/sub/2"]).unwrap();
        dbg!(result);
    }
}

