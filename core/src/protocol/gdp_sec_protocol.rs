use pnet_macros::packet;
use pnet_macros_support::types::*;
use anyhow::{Result, anyhow};
use aes_gcm::{
    aead::{Aead, NewAead},
    Aes256Gcm, Nonce, Key 
};
use pnet_packet::Packet;
use rand::Rng;



/// Documentation for GdpSecProtocol
#[packet]
pub struct GdpSecProtocol {
    #[length = "12"]
    nonce: Vec<u8>, // 96-bits; unique per message
    #[payload]
    payload: Vec<u8>
} 

impl GdpSecProtocol {
    pub fn get_header_length() -> usize {
        12
    }

    pub fn encrypt_gdp(gdp_sec_packet: &mut MutableGdpSecProtocolPacket, key: &[u8]) -> Result<Vec<u8>>{
        // Description: Encrypt the gdp packet embedded in gdp_sec_packet and return the entire result gdp sec packet as byte slice

        let key = Key::from_slice(key);
        let cipher = Aes256Gcm::new(&key);
        let nonce = rand::thread_rng().gen::<[u8; 12]>(); 
        let nonce = Nonce::from_slice(&nonce);
        gdp_sec_packet.set_nonce(nonce);
        let ciphertext = cipher.encrypt(nonce, gdp_sec_packet.payload()).map_err(|_| {
            println!("GDP encryption failed");
            anyhow!("GDP encryption failed")
        })?;
        // gdp_sec_packet.set_payload(&ciphertext);
        let mut vec: Vec<u8> = vec![0; GdpSecProtocol::get_header_length()+ciphertext.len()];
        let mut res_gdp_sec = MutableGdpSecProtocolPacket::new(&mut vec[..]).unwrap();
        res_gdp_sec.set_nonce(nonce);
        res_gdp_sec.set_payload(&ciphertext);

        Ok(vec)
    }

    pub fn decrypt_gdp(gdp_sec_packet: &GdpSecProtocolPacket, key: &[u8]) -> Result<Vec<u8>> {
        // Description: Decrypt the gdp sec packet and return the gdp packet only

        let key = Key::from_slice(key);
        let cipher = Aes256Gcm::new(&key);

        let nonce = gdp_sec_packet.get_nonce();
        let nonce = Nonce::from_slice(&nonce);
        let plaintext = cipher.decrypt(nonce, gdp_sec_packet.payload()).map_err(|_| {
            debug!("GDP decryption failed");
            anyhow!("GDP decryption failed")
        })?;
        
        Ok(plaintext)
    }
}

