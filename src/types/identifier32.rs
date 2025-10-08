//! Types used in the interface of the Bitcoin Canister.

use candid::types::TypeInner;
use candid::CandidType;
use hex::ToHex;
use ic_stable_structures::storable::Bound;
use ic_stable_structures::Storable;
use std::borrow::Cow;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identifier32Bytes(pub [u8; 32]);

impl CandidType for Identifier32Bytes {
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

impl AsRef<[u8]> for Identifier32Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Identifier32Bytes> for [u8; 32] {
    fn from(o_id: Identifier32Bytes) -> Self {
        o_id.0
    }
}

impl ToString for Identifier32Bytes {
    fn to_string(&self) -> String {
        self.0.encode_hex()
    }
}

impl From<String> for Identifier32Bytes {
    fn from(value: String) -> Self {
        let binding = hex::decode(&value).unwrap();
        let bytes = binding.as_slice();
        if bytes.len() > 32 {
            panic!("Data to large")
        }
        let mut array = [0u8; 32];
        array[..bytes.len()].copy_from_slice(bytes);
        Identifier32Bytes(array)
    }
}

impl serde::Serialize for Identifier32Bytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> serde::de::Deserialize<'de> for Identifier32Bytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Identifier32Bytes;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a 32-byte array")
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match TryInto::<[u8; 32]>::try_into(value) {
                    Ok(order_id) => Ok(Identifier32Bytes(order_id)),
                    Err(_) => Err(E::invalid_length(value.len(), &self)),
                }
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let bytes = hex::decode(v).expect("Could not decode");

                if bytes.len() > 32 {
                    return Err(serde::de::Error::invalid_length(bytes.len(), &self));
                }
                let mut array = [0u8; 32];
                array.copy_from_slice(&bytes[..32]);
                Ok(Identifier32Bytes(array))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                use serde::de::Error;
                if let Some(size_hint) = seq.size_hint() {
                    if size_hint != 32 {
                        return Err(A::Error::invalid_length(size_hint, &self));
                    }
                }
                let mut bytes = [0u8; 32];
                let mut i = 0;
                while let Some(byte) = seq.next_element()? {
                    if i == 32 {
                        return Err(A::Error::invalid_length(i + 1, &self));
                    }

                    bytes[i] = byte;
                    i += 1;
                }
                if i != 32 {
                    return Err(A::Error::invalid_length(i, &self));
                }
                Ok(Identifier32Bytes(bytes))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

impl From<[u8; 32]> for Identifier32Bytes {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl TryFrom<&'_ [u8]> for Identifier32Bytes {
    type Error = core::array::TryFromSliceError;
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let o_id: [u8; 32] = bytes.try_into()?;
        Ok(Identifier32Bytes(o_id))
    }
}

impl Storable for Identifier32Bytes {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(self.0.to_vec())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes[..32]);
        Identifier32Bytes(array)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 32u32,
        is_fixed_size: false,
    };
}

#[cfg(test)]
mod tests {

    use miniscript::bitcoin::hashes::hex::ToHex;
    use rand::{rng, TryRngCore};

    use super::*;

    #[test]
    fn serialization_test() {
        let mut rand = [0u8; 32];
        rng().try_fill_bytes(&mut rand).ok();

        let data = Identifier32Bytes::from(rand.to_hex());
        let stored = data.to_bytes();
        let recovered = Identifier32Bytes::from_bytes(stored);

        assert!(recovered == data);
    }
}
