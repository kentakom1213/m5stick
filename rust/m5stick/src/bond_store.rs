use core::ops::Range;

use embedded_storage_async::nor_flash::NorFlash;
use sequential_storage::{
    cache::NoCache,
    map::{Key, SerializationError, Value},
};
use trouble_host::prelude::*;

const BOND_START: u32 = 0x7f0000;
const BOND_END: u32 = 0x800000;

fn storage_range() -> Range<u32> {
    BOND_START..BOND_END
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredAddr(BdAddr);

impl Key for StoredAddr {
    fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize, SerializationError> {
        if buffer.len() < 6 {
            return Err(SerializationError::BufferTooSmall);
        }

        buffer[..6].copy_from_slice(self.0.raw());

        Ok(6)
    }

    fn deserialize_from(buffer: &[u8]) -> Result<(Self, usize), SerializationError> {
        if buffer.len() < 6 {
            return Err(SerializationError::BufferTooSmall);
        }

        Ok((StoredAddr(BdAddr::new(buffer[..6].try_into().unwrap())), 6))
    }
}

struct StoredBondInformation {
    ltk: LongTermKey,
    security_level: SecurityLevel,
}

impl<'a> Value<'a> for StoredBondInformation {
    fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize, SerializationError> {
        if buffer.len() < 17 {
            return Err(SerializationError::BufferTooSmall);
        }

        buffer[..16].copy_from_slice(&self.ltk.to_le_bytes());

        buffer[16] = match self.security_level {
            SecurityLevel::NoEncryption => 0,
            SecurityLevel::Encrypted => 1,
            SecurityLevel::EncryptedAuthenticated => 2,
        };

        Ok(17)
    }

    fn deserialize_from(buffer: &'a [u8]) -> Result<Self, SerializationError> {
        if buffer.len() < 17 {
            return Err(SerializationError::BufferTooSmall);
        }

        let ltk = LongTermKey::from_le_bytes(buffer[..16].try_into().unwrap());

        let security_level = match buffer[16] {
            0 => SecurityLevel::NoEncryption,
            1 => SecurityLevel::Encrypted,
            2 => SecurityLevel::EncryptedAuthenticated,
            _ => return Err(SerializationError::InvalidData),
        };

        Ok(Self {
            ltk,
            security_level,
        })
    }
}

pub async fn store<S>(
    storage: &mut S,
    info: &BondInformation,
) -> Result<(), sequential_storage::Error<S::Error>>
where
    S: NorFlash,
{
    let range = storage_range();

    sequential_storage::erase_all(storage, range.clone()).await?;

    let mut buffer = [0u8; 32];

    let key = StoredAddr(info.identity.bd_addr);

    let value = StoredBondInformation {
        ltk: info.ltk,
        security_level: info.security_level,
    };

    sequential_storage::map::store_item(
        storage,
        range,
        &mut NoCache::new(),
        &mut buffer,
        &key,
        &value,
    )
    .await?;

    Ok(())
}

pub async fn load<S>(storage: &mut S) -> Option<BondInformation>
where
    S: NorFlash,
{
    let mut buffer = [0u8; 32];
    let mut cache = NoCache::new();

    let mut iter = sequential_storage::map::fetch_all_items::<StoredAddr, _, _>(
        storage,
        storage_range(),
        &mut cache,
        &mut buffer,
    )
    .await
    .ok()?;

    while let Some((key, value)) = iter.next::<StoredBondInformation>(&mut buffer).await.ok()? {
        return Some(BondInformation {
            identity: Identity {
                bd_addr: key.0,
                irk: None,
            },
            security_level: value.security_level,
            is_bonded: true,
            ltk: value.ltk,
        });
    }

    None
}
