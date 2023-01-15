extern crate multimap;
use multimap::MultiMap;
use redis::{Connection, Commands};
use serde::{Deserialize, Serialize};
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
    pub con: Connection
}

#[derive(Serialize, Deserialize)]
struct TopicMeta {
    pub_count: u64, // total number of publisher
    sub_count: u64, // total number of subscriber
}

impl RIBClient {
    pub fn new(url: &str) -> redis::RedisResult<RIBClient> {
        let client = redis::Client::open(url)?;
        let mut con = client.get_connection()?;
        
        Ok(RIBClient {
            con
        })
    }

    pub fn create_pub_node(&mut self, topic_name: &str) -> redis::RedisResult<()> {
        let key = topic_name;
        
        let (_result,) : (Option<()>, ) = redis::transaction(&mut self.con, &[key], |con, pipe| {
            let exists: i32 = con.exists(key)?;

            if exists == 0 {
                let new_topic_meta = TopicMeta {pub_count: 1, sub_count: 0};
                
                pipe
                    .set(key, serde_json::to_string(&new_topic_meta).expect("TopicMeta serialization failed")).ignore()
                    .set(format!("{}/pub/1", key), "Placeholder").ignore() // todo Jiachen: 1. use proper serializable value record; 2. use real ip address
                    .query(con)
            } else {
                let topic_meta_str: String = con.get(key)?;
                let mut topic_meta: TopicMeta = serde_json::from_str(&topic_meta_str).expect("TopicMeta deserialization failed");
                topic_meta.pub_count += 1;
                pipe
                    .set(key, serde_json::to_string(&topic_meta).expect("TopicMeta serialization failed")).ignore()
                    .set(format!("{}/pub/{}", key, topic_meta.pub_count), "Placeholder").ignore() // todo Jiachen: 1. use proper serializable value record; 2. use real ip address 
                    .query(con)
            }
        })?;
        Ok(())
    }

    pub fn create_sub_node(&mut self, topic_name: &str) -> redis::RedisResult<()> {
        let key = topic_name;

       let (_result,) : (Option<()>, ) = redis::transaction(&mut self.con, &[key], |con, pipe| {
            let exists: i32 = con.exists(key)?;

            if exists == 0 {
                let new_topic_meta = TopicMeta {pub_count: 0, sub_count: 1};
                
                pipe
                    .set(key, serde_json::to_string(&new_topic_meta).expect("TopicMeta serialization failed")).ignore()
                    .set(format!("{}/sub/1", key), "Placeholder").ignore() // todo Jiachen: 1. use proper serializable value record; 2. use real ip address
                    .query(con)
            } else {
                let topic_meta_str: String = con.get(key)?;
                let mut topic_meta: TopicMeta = serde_json::from_str(&topic_meta_str).expect("TopicMeta deserialization failed");
                topic_meta.sub_count += 1;
                pipe
                    .set(key, serde_json::to_string(&topic_meta).expect("TopicMeta serialization failed")).ignore()
                    .set(format!("{}/sub/{}", key, topic_meta.sub_count), "Placeholder").ignore() // todo Jiachen: 1. use proper serializable value record; 2. use real ip address 
                    .query(con)
            }
        })?;
        Ok(()) 
    }
}


mod tests {
    use super::*;

    #[test]
    fn publish_a_topic() {
        let mut client = RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();
        client.create_pub_node("helloworld");
    }

    #[test]
    fn create_pub_nodes_multi_threads() {
        let mut client = RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();

        let mut client_2= RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();
        let handle = std::thread::spawn(move || {
            for i in 0..1000 {
                println!("Iteration {} from thread 2", i);
                client_2.create_pub_node("helloworld");
            }
        });

        for i in 0..1000 {
            println!("Iteration {} from thread 1", i);
            client.create_pub_node("helloworld");
        } 

        handle.join().unwrap();
    }

    #[test]
    fn create_sub_nodes_multi_threads() {
        let mut client = RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();

        let mut client_2= RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();
        let handle = std::thread::spawn(move || {
            for i in 0..1000 {
                println!("Iteration {} from thread 2", i);
                client_2.create_sub_node("helloworld");
            }
        });

        for i in 0..1000 {
            println!("Iteration {} from thread 1", i);
            client.create_sub_node("helloworld");
        } 

        handle.join().unwrap();
    }

    #[test]
    fn redis_basics() {
        let mut client = RIBClient::new("redis://default:fogrobotics@128.32.37.41/").unwrap();
        let mykey: Option<String> = client.con.get("mykey").unwrap();
        dbg!(mykey);

        let exists: i32 = client.con.exists("mykey").unwrap();
        dbg!(exists);

        let result: () = client.con.set("nihao", "shijie").unwrap();
        dbg!(result);
    }
}

