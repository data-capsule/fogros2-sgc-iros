use anyhow::{anyhow, Result};
use std::{fmt, net::Ipv4Addr};
use strum_macros::EnumIter;
pub const MAGIC_NUMBERS: u16 = u16::from_be_bytes([0x26, 0x2a]);

pub type GdpName = [u8; 32];

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, EnumIter)]
pub enum GdpAction {
    Noop = 0,
    Forward = 1,
    Advertise = 2,
    RibGet = 3,
    RibReply = 4,
    Nack = 5,
    Control = 6,
    PubAdvertise = 7,
    SubAdvertise = 8,
}

impl Default for GdpAction {
    fn default() -> Self {
        GdpAction::Noop
    }
}

impl TryFrom<u8> for GdpAction {
    type Error = anyhow::Error;

    fn try_from(v: u8) -> Result<Self> {
        match v {
            x if x == GdpAction::Noop as u8 => Ok(GdpAction::Noop),
            x if x == GdpAction::RibGet as u8 => Ok(GdpAction::RibGet),
            x if x == GdpAction::RibReply as u8 => Ok(GdpAction::RibReply),
            x if x == GdpAction::Forward as u8 => Ok(GdpAction::Forward),
            x if x == GdpAction::Nack as u8 => Ok(GdpAction::Nack),
            x if x == GdpAction::Control as u8 => Ok(GdpAction::Control),
            x if x == GdpAction::PubAdvertise as u8 => Ok(GdpAction::PubAdvertise),
            x if x == GdpAction::SubAdvertise as u8 => Ok(GdpAction::SubAdvertise),
            unknown => Err(anyhow!("Unknown action byte ({:?})", unknown)),
        }
    }
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Default)]
#[repr(C, packed)]
pub struct u16be(u16);

impl From<u16> for u16be {
    fn from(item: u16) -> Self {
        u16be(u16::to_be(item))
    }
}

impl From<u16be> for u16 {
    fn from(item: u16be) -> Self {
        u16::from_be(item.0)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, Default)]
pub struct GDPName(pub [u8; 4]); //256 bit destination
impl fmt::Display for GDPName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

use crate::gdp_proto::GdpPacket;
pub(crate) trait Packet {
    /// get protobuf object of the packet
    fn get_proto(&self) -> Option<&GdpPacket>;
    /// get serialized byte array of the packet
    fn get_byte_payload(&self) -> Option<&Vec<u8>>;
}

#[derive(Debug, PartialEq, Clone)]
pub struct GDPPacket {
    pub action: GdpAction,
    pub gdpname: GDPName,
    // the payload can be either (both)
    // Vec u8 bytes or protobuf
    // converting back and forth between proto and u8 is expensive
    // preferably forward directly without conversion
    pub payload: Option<Vec<u8>>,
    pub proto: Option<GdpPacket>,
}

impl Packet for GDPPacket {
    fn get_proto(&self) -> Option<&GdpPacket> {
        match &self.proto {
            Some(p) => Some(p),
            None => None, //TODO
        }
    }
    fn get_byte_payload(&self) -> Option<&Vec<u8>> {
        match &self.payload {
            Some(p) => Some(p),
            None => None, //TODO
        }
    }
}

impl fmt::Display for GDPPacket {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Write strictly the first element into the supplied output
        // stream: `f`. Returns `fmt::Result` which indicates whether the
        // operation succeeded or failed. Note that `write!` uses syntax which
        // is very similar to `println!`.
        if let Some(payload) = &self.payload {
            write!(
                f,
                "{:?}: {:?}",
                self.gdpname,
                std::str::from_utf8(&payload)
                    .expect("parsing failure")
                    .trim_matches(char::from(0))
            )
        } else if let Some(payload) = &self.proto {
            write!(f, "{:?}: {:?}", self.gdpname, payload)
        } else {
            write!(f, "{:?}: packet do not exist", self.gdpname)
        }
    }
}

use tokio::sync::mpsc::Sender;
#[derive(Debug, Clone)]
pub struct GDPChannel {
    pub gdpname: GDPName,
    pub channel: Sender<GDPPacket>,
}


pub struct PubPacket {
    pub creator: GDPName,
    pub topic_name: GDPName,
}

impl PubPacket {
    pub fn from_vec_bytes(buffer: &Vec<u8>) -> PubPacket {
        let received_str: Vec<&str> = std::str::from_utf8(&buffer)
            .unwrap()
            .trim()
            .split(",")
            .collect();
        
        // assert_eq!(buffer.len(), 3, "The buffer does not cannot be deserialized to a PubPacket");

        let creator = match &received_str[1][0..1] {
            "1" => GDPName([1, 1, 1, 1]),
            "2" => GDPName([2, 2, 2, 2]),
            "3" => GDPName([3, 3, 3, 3]),
            "4" => GDPName([4, 4, 4, 4]),
            "5" => GDPName([5, 5, 5, 5]),
            _ => GDPName([0, 0, 0, 0]),
        };

        let topic_name = match &received_str[2][0..1] {
            "1" => GDPName([1, 1, 1, 1]),
            "2" => GDPName([2, 2, 2, 2]),
            "3" => GDPName([3, 3, 3, 3]),
            "4" => GDPName([4, 4, 4, 4]),
            "5" => GDPName([5, 5, 5, 5]),
            _ => GDPName([0, 0, 0, 0]),
        };

        PubPacket {
            creator,
            topic_name
        }
    }
}

pub struct SubPacket {
    pub creator: GDPName,
    pub topic_name: GDPName,
}

impl SubPacket {
    pub fn from_vec_bytes(buffer: &Vec<u8>) -> SubPacket {
        let received_str: Vec<&str> = std::str::from_utf8(&buffer)
            .unwrap()
            .trim()
            .split(",")
            .collect();
        
        // assert_eq!(buffer.len(), 3, "The buffer does not cannot be deserialized to a PubPacket");

        let creator = match &received_str[1][0..1] {
            "1" => GDPName([1, 1, 1, 1]),
            "2" => GDPName([2, 2, 2, 2]),
            "3" => GDPName([3, 3, 3, 3]),
            "4" => GDPName([4, 4, 4, 4]),
            "5" => GDPName([5, 5, 5, 5]),
            _ => GDPName([0, 0, 0, 0]),
        };

        let topic_name = match &received_str[2][0..1] {
            "1" => GDPName([1, 1, 1, 1]),
            "2" => GDPName([2, 2, 2, 2]),
            "3" => GDPName([3, 3, 3, 3]),
            "4" => GDPName([4, 4, 4, 4]),
            "5" => GDPName([5, 5, 5, 5]),
            _ => GDPName([0, 0, 0, 0]),
        };

        SubPacket {
            creator,
            topic_name
        }
    }
}



pub type SubscriberInfo = (GDPName, Ipv4Addr);