use core::fmt;
use std::borrow::Cow;

use bitcoin::Address;
use candid::types::TypeInner;
use candid::CandidType;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Copy)]
pub struct Account([u8; 100], u8);
impl CandidType for Account {
    fn _ty() -> candid::types::Type {
        candid::types::Type(TypeInner::Text.into())
    }

    fn idl_serialize<S>(&self, serializer: S) -> Result<(), S::Error>
    where
        S: candid::types::Serializer,
    {
        let string = self.to_string();
        serializer.serialize_text(&string)
    }
}

impl From<String> for Account {
    fn from(value: String) -> Self {
        let bytes = value.as_bytes();
        if bytes.len() > 100 {
            panic!("Data to large")
        }
        let mut array = [0u8; 100];
        array[..bytes.len()].copy_from_slice(bytes);
        Account(array, value.len() as u8)
    }
}

impl From<Address> for Account {
    fn from(value: Address) -> Self {
        Self::from(value.to_string())
    }
}

impl ToString for Account {
    fn to_string(&self) -> String {
        String::from_utf8_lossy(&self.0).to_string()[..self.1 as usize].to_string()
    }
}

impl Serialize for Account {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let len = self.1 as usize;
        let str = &self.0[..len];
        serializer.serialize_bytes(str)
    }
}

impl<'de> Deserialize<'de> for Account {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AccountVisitor;

        impl<'de> Visitor<'de> for AccountVisitor {
            type Value = Account;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a valid Bitcoin address string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let bytes = v.as_bytes();
                let len: u8 = v.len() as u8;

                // Use up to 100 byes for encoding addresses
                if bytes.len() > 100 {
                    return Err(de::Error::invalid_length(bytes.len(), &self));
                }
                let mut array = [0u8; 100];
                array[..bytes.len()].copy_from_slice(bytes);
                Ok(Account(array, len))
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let bytes = v;
                let len: u8 = v.len() as u8;

                // Use up to 100 byes for encoding addresses
                if bytes.len() > 100 {
                    return Err(de::Error::invalid_length(bytes.len(), &self));
                }
                let mut array = [0u8; 100];
                array[..bytes.len()].copy_from_slice(bytes);
                Ok(Account(array, len))
            }
        }

        // Use the AccountVisitor to deserialize
        deserializer.deserialize_any(AccountVisitor)
    }
}

impl Storable for Account {
    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        ciborium::ser::into_writer(&self, &mut buf).unwrap();
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        let data: Vec<u8> = ciborium::de::from_reader(bytes.as_ref()).unwrap();
        let len: u8 = data.len() as u8;

        let mut array = [0u8; 100];
        array[..data.len()].copy_from_slice(&data);
        Self(array, len)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 101u32,
        is_fixed_size: false,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_from_string() {
        let account = Account::from("bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string());
        assert_eq!(
            account.to_string(),
            "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh"
        );
    }

    #[test]
    #[should_panic(expected = "Data to large")]
    #[allow(unused_must_use)]
    fn test_account_from_string_invalid_length() {
        Account::from("a_long_string_exceeding_the_limit_of_100_characters_____________________________________________________".to_string());
    }

    #[test]
    fn test_account_to_bytes() {
        let account = Account::from("bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string());
        let bytes = account.to_bytes();
        let deserialized_account = Account::from_bytes(bytes);
        assert_eq!(account, deserialized_account);
    }

    #[test]
    fn test_account_ordering() {
        let account1 = Account::from("bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string());
        let account2 = Account::from("bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlj".to_string());
        assert!(account1 < account2);
    }
}
