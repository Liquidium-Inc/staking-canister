//! Storage functionality tests
use crate::state::{
    get_pool_address, get_stored_exchange_rate, set_exchange_rate_components, set_pool_address,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Test pool address storage and retrieval
    #[test]
    fn test_pool_storage() {
        let test_address = "test_pool_address".to_string();
        let pool_index = 1u32;

        // Store pool address
        set_pool_address(test_address.clone(), pool_index);

        // Verify storage works
        assert!(!get_pool_address(pool_index).is_empty());
        let retrieved_address = get_pool_address(pool_index);
        assert!(!retrieved_address.is_empty());
        assert_eq!(retrieved_address, test_address);
    }

    /// Test exchange rate storage and calculation
    #[test]
    fn test_exchange_rate_storage() {
        let circulating_supply = 1000000u128;
        let balance = 1500000u128;

        // Store exchange rate components
        set_exchange_rate_components(circulating_supply, balance);

        // Verify exchange rate calculation works
        let stored_rate = get_stored_exchange_rate();
        assert!(stored_rate.is_some());
        assert!(stored_rate.unwrap() > 0.0);
    }
}
