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
    payload_len: u32be,
    #[payload]
    payload: Vec<u8>
} 

impl GdpSecProtocol {
    pub fn get_header_length() -> usize {
        12
    }

    pub fn encrypt_gdp(res_gdp_sec: &mut MutableGdpSecProtocolPacket, plaintext: &[u8], key: &[u8]) -> Result<()>{
        // Description: Encrypt the plaintext and write the ciphertext into the payload of res_gdp_sec

        let key = Key::from_slice(key);
        let cipher = Aes256Gcm::new(&key);
        let nonce = rand::thread_rng().gen::<[u8; 12]>(); 
        let nonce = Nonce::from_slice(&nonce);
        res_gdp_sec.set_nonce(nonce);
        let ciphertext = cipher.encrypt(nonce, plaintext).map_err(|_| {
            println!("GDP encryption failed");
            anyhow!("GDP encryption failed")
        })?;

        res_gdp_sec.set_payload(&ciphertext);
        res_gdp_sec.set_payload_len(ciphertext.len().try_into().unwrap());

        Ok(())
    }

    pub fn decrypt_gdp(gdp_sec_packet: &mut MutableGdpSecProtocolPacket, key: &[u8]) -> Result<()> {
        // Description: Decrypt the gdp sec packet and return the gdp packet only

        let key = Key::from_slice(key);
        let cipher = Aes256Gcm::new(&key);

        let nonce = gdp_sec_packet.get_nonce();
        let nonce = Nonce::from_slice(&nonce);
        let ciphertext_length = gdp_sec_packet.get_payload_len() as usize;
        let plaintext = cipher.decrypt(nonce, &gdp_sec_packet.payload()[..ciphertext_length]).map_err(|_| {
            debug!("GDP decryption failed");
            anyhow!("GDP decryption failed")
        })?;

        gdp_sec_packet.set_payload(&plaintext);
        
        Ok(())
    }
}

