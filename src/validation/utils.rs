use bitcoin::{Address, AddressType};


pub fn decode_output_address(
    psbt: &bitcoin::psbt::PartiallySignedTransaction,
    btc_network: bitcoin::Network,
    index: usize,
) -> Result<(String, AddressType), String> {
    // Decode scriptPubKey -> Address
    if let Ok(addr) =
        Address::from_script(&psbt.unsigned_tx.output[index].script_pubkey, btc_network)
    {
        let addr_type = addr
            .address_type()
            .ok_or("Malformed address".to_string())?;

        Ok((addr.to_string(), addr_type))
    } else {
        Err("Malformed address".to_string())
    }
}
