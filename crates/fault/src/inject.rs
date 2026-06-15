use solana_hash::Hash;

pub const MAX_PROCESSING_AGE: u64 = 150;
const INJECTED_AGE_MARGIN: u64 = 16;

#[derive(Debug, Clone, Copy)]
pub struct ExpiredBlockhash {
    pub blockhash: Hash,
    pub apparent_age_slots: u64,
}

pub fn inject_expired_blockhash(reference: Hash) -> ExpiredBlockhash {
    let mut bytes = reference.to_bytes();
    bytes.rotate_left(1);
    bytes[0] ^= 0xA5;
    ExpiredBlockhash {
        blockhash: Hash::new_from_array(bytes),
        apparent_age_slots: MAX_PROCESSING_AGE + INJECTED_AGE_MARGIN,
    }
}
