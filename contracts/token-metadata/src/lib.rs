#![no_std]
//! Embed in the project token so the immutable contract ID remains its identity.
use soroban_sdk::{contracterror, contractevent, contracttype, Address, BytesN, Env, String};

#[contracttype]
enum Key {
    TokenMetadata,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub description: String,
    pub image_uri: Option<String>,
    pub image_sha256: Option<BytesN<32>>,
    pub website_uri: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataState {
    pub creator: Address,
    pub metadata: TokenMetadata,
    pub revision: u64,
    pub locked: bool,
}

#[contractevent]
pub struct MetadataUpdated {
    pub revision: u64,
}

#[contractevent]
pub struct MetadataLocked {
    pub revision: u64,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MetadataError {
    InvalidMetadata = 100,
    AlreadyInitialized = 101,
    Locked = 102,
    StaleRevision = 103,
    RevisionOverflow = 104,
    ImageDigestRequired = 105,
    InvalidArweaveLogo = 106,
}

/// A canonical Arweave object or a file within an immutable path manifest.
pub fn is_arweave_uri(value: &String) -> bool {
    let len = value.len() as usize;
    if !(48..=256).contains(&len) {
        return false;
    }
    let mut buffer = [0; 256];
    let uri = &mut buffer[..len];
    value.copy_into_slice(uri);
    let id = &uri[5..48];
    if &uri[..5] != b"ar://" || !id.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_') {
        return false;
    }
    if len == 48 {
        return true;
    }
    uri[48] == b'/' && uri[49..].split(|byte| *byte == b'/').all(|part| {
        !part.is_empty() && part != b"." && part != b".." && part.iter().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~')
        })
    })
}

pub fn validate_project_logo(metadata: &TokenMetadata) -> Result<(), MetadataError> {
    if metadata.image_uri.as_ref().is_some_and(|uri| !is_arweave_uri(uri)) {
        return Err(MetadataError::InvalidArweaveLogo);
    }
    Ok(())
}

fn nonblank(value: &String, max_bytes: u32) -> bool {
    let len = value.len();
    if len == 0 || len > max_bytes {
        return false;
    }
    let mut buffer = [0; 64];
    let bytes = &mut buffer[..len as usize];
    value.copy_into_slice(bytes);
    bytes.iter().any(|byte| !byte.is_ascii_whitespace() && !byte.is_ascii_control())
        && !bytes.iter().any(|byte| byte.is_ascii_control())
}

fn validate(metadata: &TokenMetadata) -> Result<(), MetadataError> {
    let valid_uri = |uri: &Option<String>| {
        uri.as_ref().is_none_or(|value| (1..=256).contains(&value.len()))
    };
    if !nonblank(&metadata.name, 64)
        || !nonblank(&metadata.symbol, 16)
        || metadata.description.len() > 512
        || !valid_uri(&metadata.image_uri)
        || !valid_uri(&metadata.website_uri)
    {
        return Err(MetadataError::InvalidMetadata);
    }
    Ok(())
}

fn validate_lock(metadata: &TokenMetadata) -> Result<(), MetadataError> {
    if metadata.image_uri.is_some() && metadata.image_sha256.is_none() {
        return Err(MetadataError::ImageDigestRequired);
    }
    Ok(())
}

pub fn initialize(
    env: &Env,
    creator: Address,
    metadata: TokenMetadata,
    locked: bool,
) -> Result<(), MetadataError> {
    let storage = env.storage().instance();
    if storage.has(&Key::TokenMetadata) {
        return Err(MetadataError::AlreadyInitialized);
    }
    validate(&metadata)?;
    if locked {
        validate_lock(&metadata)?;
    }
    creator.require_auth();
    storage.set(&Key::TokenMetadata, &MetadataState { creator, metadata, revision: 0, locked });
    MetadataUpdated { revision: 0 }.publish(env);
    if locked {
        MetadataLocked { revision: 0 }.publish(env);
    }
    Ok(())
}

pub fn get(env: &Env) -> MetadataState {
    env.storage().instance().get(&Key::TokenMetadata).unwrap()
}

fn editable(env: &Env, expected_revision: u64) -> Result<MetadataState, MetadataError> {
    let state = get(env);
    if state.locked {
        return Err(MetadataError::Locked);
    }
    state.creator.require_auth();
    if state.revision != expected_revision {
        return Err(MetadataError::StaleRevision);
    }
    Ok(state)
}

pub fn update(
    env: &Env,
    expected_revision: u64,
    metadata: TokenMetadata,
) -> Result<(), MetadataError> {
    let mut state = editable(env, expected_revision)?;
    validate(&metadata)?;
    state.revision = state.revision.checked_add(1).ok_or(MetadataError::RevisionOverflow)?;
    state.metadata = metadata;
    env.storage().instance().set(&Key::TokenMetadata, &state);
    MetadataUpdated { revision: state.revision }.publish(env);
    Ok(())
}

pub fn lock(env: &Env, expected_revision: u64) -> Result<(), MetadataError> {
    let mut state = editable(env, expected_revision)?;
    validate_lock(&state.metadata)?;
    state.locked = true;
    env.storage().instance().set(&Key::TokenMetadata, &state);
    MetadataLocked { revision: state.revision }.publish(env);
    Ok(())
}
