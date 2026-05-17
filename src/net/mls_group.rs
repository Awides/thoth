//! MLS (Message Layer Security) group management using openmls.
//!
//! Real end-to-end encryption using MLS protocol:
//! - Group creation with `MlsGroup::new_with_group_id`
//! - Encryption via `MlsGroup::create_message` → `MlsMessageOut`
//! - Decryption via `MlsGroup::process_message` → `ProcessedMessage`
//! - Member addition via `MlsGroup::add_members` → (commit, welcome)
//! - Join via `StagedWelcome::new_from_welcome` → `into_group`
//!
//! Wire format: MlsMessageOut → `tls_serialize_detached()` bytes → hex for transport
//!              hex → bytes → `MlsMessageIn::tls_deserialize_exact()` → `process_message()`

use anyhow::{Result, anyhow};
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use tls_codec::{Serialize as TlsSerialize, Deserialize as TlsDeserialize};
use std::collections::HashMap;
use tracing::info;

const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519;

pub struct MlsGroupManager {
    provider: OpenMlsRustCrypto,
    groups: HashMap<String, MlsGroup>,
    signers: HashMap<String, SignatureKeyPair>,
    pending_signers: HashMap<String, SignatureKeyPair>,
}

impl MlsGroupManager {
    pub fn new() -> Self {
        Self {
            provider: OpenMlsRustCrypto::default(),
            groups: HashMap::new(),
            signers: HashMap::new(),
            pending_signers: HashMap::new(),
        }
    }

    pub fn create_group(&mut self, group_id: String, creator_identity: String) -> Result<()> {
        info!("Creating MLS group '{}' with creator '{}'", group_id, creator_identity);

        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())
            .map_err(|e| anyhow!("Failed to create signature key pair: {:?}", e))?;

        let credential = Credential::new(
            CredentialType::Basic,
            creator_identity.as_bytes().to_vec(),
        );
        let credential_with_key = CredentialWithKey {
            credential,
            signature_key: signer.to_public_vec().into(),
        };

        let group_id_bytes = group_id.as_bytes().to_vec();
        let mls_group_id = GroupId::from_slice(&group_id_bytes);

        let create_config = MlsGroupCreateConfig::builder()
            .wire_format_policy(PURE_CIPHERTEXT_WIRE_FORMAT_POLICY)
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .build();

        let group = MlsGroup::new_with_group_id(
            &self.provider,
            &signer,
            &create_config,
            mls_group_id,
            credential_with_key,
        ).map_err(|e| anyhow!("Failed to create MLS group: {:?}", e))?;

        self.groups.insert(group_id.clone(), group);
        self.signers.insert(group_id.clone(), signer);

        info!("Successfully created MLS group '{}'", group_id);
        Ok(())
    }

    pub fn generate_key_package(&mut self, identity: String) -> Result<Vec<u8>> {
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm())
            .map_err(|e| anyhow!("Failed to create signature key pair: {:?}", e))?;

        let credential = Credential::new(
            CredentialType::Basic,
            identity.as_bytes().to_vec(),
        );
        let credential_with_key = CredentialWithKey {
            credential,
            signature_key: signer.to_public_vec().into(),
        };

        let key_package_bundle = KeyPackage::builder()
            .build(
                CIPHERSUITE,
                &self.provider,
                &signer,
                credential_with_key,
            )
            .map_err(|e| anyhow!("Failed to generate key package: {:?}", e))?;

        self.pending_signers.insert(identity.clone(), signer);

        key_package_bundle.key_package()
            .tls_serialize_detached()
            .map_err(|e| anyhow!("Failed to serialize key package: {:?}", e))
    }

    pub fn add_member(
        &mut self,
        group_id: &str,
        key_package_bytes: &[u8],
        member_identity: String,
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        let group = self.groups.get_mut(group_id)
            .ok_or_else(|| anyhow!("Group not found: {}", group_id))?;
        let signer = self.signers.get(group_id)
            .ok_or_else(|| anyhow!("Signer not found for group: {}", group_id))?;

        let key_package_in = KeyPackageIn::tls_deserialize_exact(key_package_bytes)
            .map_err(|e| anyhow!("Failed to deserialize key package: {:?}", e))?;
        let key_package = key_package_in
            .validate(self.provider.crypto(), ProtocolVersion::default())
            .map_err(|e| anyhow!("Failed to validate key package: {:?}", e))?;

        info!("Adding member '{}' to group '{}'", member_identity, group_id);

        let (commit_msg, welcome_msg, _group_info) = group
            .add_members(&self.provider, signer, &[key_package])
            .map_err(|e| anyhow!("Failed to add member: {:?}", e))?;

        group.merge_pending_commit(&self.provider)
            .map_err(|e| anyhow!("Failed to merge pending commit: {:?}", e))?;

        let commit_bytes = commit_msg.to_bytes()
            .map_err(|e| anyhow!("Failed to serialize commit: {:?}", e))?;
        let welcome_bytes = welcome_msg.to_bytes()
            .map_err(|e| anyhow!("Failed to serialize welcome: {:?}", e))?;

        Ok((commit_bytes, welcome_bytes))
    }

    pub fn process_welcome(&mut self, welcome_bytes: &[u8], identity: String) -> Result<String> {
        let welcome_msg_in = MlsMessageIn::tls_deserialize_exact(welcome_bytes)
            .map_err(|e| anyhow!("Failed to deserialize welcome: {:?}", e))?;
        let welcome = match welcome_msg_in.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => return Err(anyhow!("Expected Welcome message in process_welcome")),
        };

        let join_config = MlsGroupJoinConfig::builder()
            .wire_format_policy(PURE_CIPHERTEXT_WIRE_FORMAT_POLICY)
            .use_ratchet_tree_extension(true)
            .build();

        let staged_welcome = StagedWelcome::new_from_welcome(
            &self.provider,
            &join_config,
            welcome,
            None,
        ).map_err(|e| anyhow!("Failed to process welcome: {:?}", e))?;

        let group_id_bytes = staged_welcome.group_context().group_id().as_slice();
        let group_id_str = String::from_utf8_lossy(group_id_bytes).to_string();

        let group = staged_welcome.into_group(&self.provider)
            .map_err(|e| anyhow!("Failed to join group from welcome: {:?}", e))?;

        let signer = self.pending_signers.remove(&identity)
            .ok_or_else(|| anyhow!("No pending signer for identity '{}'", identity))?;

        let gid = group_id_str.clone();
        self.groups.insert(group_id_str, group);
        self.signers.insert(gid.clone(), signer);

        info!("Joined MLS group '{}' via welcome", gid);
        Ok(gid)
    }

    pub fn encrypt(&mut self, group_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let group = self.groups.get_mut(group_id)
            .ok_or_else(|| anyhow!("Group not found: {}", group_id))?;
        let signer = self.signers.get(group_id)
            .ok_or_else(|| anyhow!("Signer not found for group: {}", group_id))?;

        info!("Encrypting message for group '{}': {} bytes", group_id, plaintext.len());

        let mls_message = group.create_message(&self.provider, signer, plaintext)
            .map_err(|e| anyhow!("MLS encrypt failed: {:?}", e))?;

        mls_message.to_bytes()
            .map_err(|e| anyhow!("Failed to serialize MLS message: {:?}", e))
    }

    pub fn decrypt(&mut self, group_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let group = self.groups.get_mut(group_id)
            .ok_or_else(|| anyhow!("Group not found: {}", group_id))?;

        let mls_message_in = MlsMessageIn::tls_deserialize_exact(ciphertext)
            .map_err(|e| anyhow!("Failed to deserialize MLS message: {:?}", e))?;

        let protocol_msg = mls_message_in.try_into_protocol_message()
            .map_err(|e| anyhow!("Message is not a protocol message: {:?}", e))?;

        let processed = group.process_message(&self.provider, protocol_msg)
            .map_err(|e| anyhow!("MLS decrypt failed: {:?}", e))?;

        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app_msg) => {
                Ok(app_msg.into_bytes())
            }
            ProcessedMessageContent::StagedCommitMessage(commit) => {
                group.merge_staged_commit(&self.provider, *commit)
                    .map_err(|e| anyhow!("Failed to merge staged commit: {:?}", e))?;
                Ok(Vec::new())
            }
            ProcessedMessageContent::ProposalMessage(proposal) => {
                group.store_pending_proposal(self.provider.storage(), *proposal)
                    .map_err(|e| anyhow!("Failed to store pending proposal: {:?}", e))?;
                Ok(Vec::new())
            }
            _ => Err(anyhow!("Unexpected message type in decrypt")),
        }
    }

    pub fn get_members(&self, group_id: &str) -> Result<Vec<String>> {
        let group = self.groups.get(group_id)
            .ok_or_else(|| anyhow!("Group not found: {}", group_id))?;

        Ok(group.members()
            .map(|m| {
                String::from_utf8_lossy(m.credential.serialized_content()).to_string()
            })
            .collect())
    }

    pub fn has_group(&self, group_id: &str) -> bool {
        self.groups.contains_key(group_id)
    }

    pub fn group_ids(&self) -> Vec<String> {
        self.groups.keys().cloned().collect()
    }
}

pub fn create_device_group(manager: &mut MlsGroupManager, device_ids: Vec<String>) -> Result<()> {
    let group_id = "devices".to_string();
    let creator = device_ids.first().unwrap_or(&"device_creator".to_string()).clone();
    manager.create_group(group_id, creator)?;
    Ok(())
}

pub fn create_user_group(manager: &mut MlsGroupManager, group_id: String, creator: String) -> Result<()> {
    manager.create_group(group_id, creator)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_group() {
        let mut manager = MlsGroupManager::new();
        manager.create_group("test".to_string(), "user1".to_string()).unwrap();
        assert_eq!(manager.get_members("test").unwrap().len(), 1);
    }

    #[test]
    fn test_create_and_encrypt() {
        let mut manager = MlsGroupManager::new();
        manager.create_group("test".to_string(), "alice".to_string()).unwrap();
        assert_eq!(manager.get_members("test").unwrap().len(), 1);

        let plaintext = b"Hello, MLS!";
        let _ciphertext = manager.encrypt("test", plaintext).unwrap();
    }

    #[test]
    fn test_two_member_roundtrip() {
        let mut alice_mgr = MlsGroupManager::new();
        let mut bob_mgr = MlsGroupManager::new();

        alice_mgr.create_group("chat".to_string(), "alice".to_string()).unwrap();

        let bob_kp = bob_mgr.generate_key_package("bob".to_string()).unwrap();

        let (_commit_bytes, welcome_bytes) = alice_mgr
            .add_member("chat", &bob_kp, "bob".to_string())
            .unwrap();

        bob_mgr.process_welcome(&welcome_bytes, "bob".to_string()).unwrap();

        let msg = b"Hello from Alice!";
        let cipher = alice_mgr.encrypt("chat", msg).unwrap();
        let plain = bob_mgr.decrypt("chat", &cipher).unwrap();
        assert_eq!(plain, msg);

        let msg2 = b"Hi from Bob!";
        let cipher2 = bob_mgr.encrypt("chat", msg2).unwrap();
        let plain2 = alice_mgr.decrypt("chat", &cipher2).unwrap();
        assert_eq!(plain2, msg2);
    }
}
