pub mod address;
pub mod psbt;
pub mod utils;

// Core Bitcoin functionality exports
pub use address::generate_bitcoin_address;
pub use address::get_fingerprint;
pub use address::get_xpub;
