//! MLS (Message Layer Security) group management via Marmot Development Kit (MDK).
//!
//! MDK handles the full MLS protocol internally:
//! - Group creation with MIP-03 compliant wire format (kind:445)
//! - Encryption via `MDK::create_message()` → ready-to-publish Nostr event
//! - Decryption via `MDK::process_message()` → MessageProcessingResult
//! - Member addition via `MDK::add_members()` → commit + welcome rumors
//! - Join via `MDK::process_welcome()` + `MDK::accept_welcome()`
//!
//! The MlsGroupManager wraps MDK and manages a name→GroupId mapping so callers
//! can work with human-readable group names while MDK uses proper GroupId types.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use mdk_core::prelude::*;
use mdk_core::key_packages::KeyPackageOptions;
use mdk_memory_storage::MdkMemoryStorage;
use nostr::{EventBuilder, JsonUtil, Keys, Kind, PublicKey, RelayUrl, UnsignedEvent};
use tracing::info;

pub struct MlsGroupManager {
    mdk: Arc<Mutex<MDK<MdkMemoryStorage>>>,
    keys: HashMap<String, Keys>,
    name_to_mls_id: HashMap<String, GroupId>,
    default_relays: Vec<RelayUrl>,
}

impl MlsGroupManager {
    pub fn new() -> Self {
        let mdk = MDK::new(MdkMemoryStorage::default());
        Self {
            mdk: Arc::new(Mutex::new(mdk)),
            keys: HashMap::new(),
            name_to_mls_id: HashMap::new(),
            default_relays: Self::default_relay_urls(),
        }
    }

    fn default_relay_urls() -> Vec<RelayUrl> {
        ["wss://relay.nostr.io", "wss://nos.lol", "wss://relay.damus.io"]
            .iter()
            .filter_map(|u| RelayUrl::parse(u).ok())
            .collect()
    }

    fn get_or_create_keys(&mut self, identity: &str) -> Result<Keys> {
        if let Some(keys) = self.keys.get(identity) {
            return Ok(keys.clone());
        }
        let keys = Keys::generate();
        self.keys.insert(identity.to_string(), keys.clone());
        Ok(keys)
    }

    fn resolve_mls_group_id(&self, group_id: &str) -> Result<GroupId> {
        self.name_to_mls_id.get(group_id).cloned()
            .ok_or_else(|| anyhow!("Group not found: {}", group_id))
    }

    fn find_group_by_name(&self, name: &str) -> Result<Option<GroupId>> {
        let mdk = self.mdk.lock().map_err(|e| anyhow!("MDK lock poisoned: {}", e))?;
        let groups = mdk.get_groups()
            .map_err(|e| anyhow!("Failed to list groups: {:?}", e))?;
        for g in &groups {
            if g.name == name {
                return Ok(Some(g.mls_group_id.clone()));
            }
        }
        Ok(None)
    }

    pub fn create_group(&mut self, group_id: String, creator_identity: String) -> Result<()> {
        info!("Creating MLS group '{}' with creator '{}'", group_id, creator_identity);

        let keys = self.get_or_create_keys(&creator_identity)?;
        let creator_pk = keys.public_key();

        let config = NostrGroupConfigData {
            name: group_id.clone(),
            description: String::new(),
            image_hash: None,
            image_key: None,
            image_nonce: None,
            relays: self.default_relays.clone(),
            admins: vec![creator_pk],
            disappearing_message_secs: None,
        };

        let mdk = self.mdk.lock().map_err(|e| anyhow!("MDK lock poisoned: {}", e))?;
        let result = mdk.create_group(&creator_pk, vec![], config)
            .map_err(|e| anyhow!("Failed to create MLS group: {:?}", e))?;

        let mls_gid = result.group.mls_group_id.clone();
        self.name_to_mls_id.insert(group_id.clone(), mls_gid);

        info!("Created MLS group '{}' (mls_group_id={:?})", group_id, result.group.mls_group_id);
        Ok(())
    }

    pub fn generate_key_package(&mut self, identity: String) -> Result<Vec<u8>> {
        let keys = self.get_or_create_keys(&identity)?;
        let pk = keys.public_key();

        let mdk = self.mdk.lock().map_err(|e| anyhow!("MDK lock poisoned: {}", e))?;
        let kp_data = mdk.create_key_package_for_event_with_options(
            &pk,
            self.default_relays.clone(),
            KeyPackageOptions::default(),
        )
        .map_err(|e| anyhow!("Failed to generate key package: {:?}", e))?;

        info!("Generated key package for '{}'", identity);
        Ok(kp_data.content.into_bytes())
    }

    pub fn create_key_package_event(&mut self, identity: &str) -> Result<nostr::Event> {
        let keys = self.get_or_create_keys(identity)?;
        let pk = keys.public_key();

        let mdk = self.mdk.lock().map_err(|e| anyhow!("MDK lock poisoned: {}", e))?;
        let kp_data = mdk.create_key_package_for_event_with_options(
            &pk,
            self.default_relays.clone(),
            KeyPackageOptions::default(),
        )
        .map_err(|e| anyhow!("Failed to generate key package: {:?}", e))?;

        let tags = kp_data.tags_30443.clone();

        let event = EventBuilder::new(Kind::Custom(30443), &kp_data.content)
            .tags(tags)
            .sign_with_keys(&keys)
            .map_err(|e| anyhow!("Failed to sign key package event: {:?}", e))?;

        info!("Created kind:30443 key package event for '{}' (d_tag={})", identity, &kp_data.d_tag[..8]);
        Ok(event)
    }

    pub fn add_member(
        &mut self,
        group_id: &str,
        _key_package_bytes: &[u8],
        member_identity: String,
    ) -> Result<(Vec<u8>, Vec<Vec<u8>>)> {
        let mls_group_id = self.resolve_mls_group_id(group_id)?;

        let member_pk = PublicKey::from_hex(&member_identity)
            .map_err(|e| anyhow!("Invalid member pubkey '{}': {:?}", member_identity, e))?;

        let mdk = self.mdk.lock().map_err(|e| anyhow!("MDK lock poisoned: {}", e))?;

        let kp_data = mdk.create_key_package_for_event_with_options(
            &member_pk,
            self.default_relays.clone(),
            KeyPackageOptions::default(),
        )
            .map_err(|e| anyhow!("Failed to generate member key package: {:?}", e))?;

        let kp_event = build_key_package_event(&kp_data)?;

        let result = mdk.add_members(&mls_group_id, &[kp_event])
            .map_err(|e| anyhow!("Failed to add member: {:?}", e))?;

        mdk.merge_pending_commit(&mls_group_id)
            .map_err(|e| anyhow!("Failed to merge pending commit: {:?}", e))?;

        let commit_bytes = result.evolution_event.as_json().as_bytes().to_vec();

        let welcome_jsons: Vec<Vec<u8>> = result.welcome_rumors
            .unwrap_or_default()
            .iter()
            .map(|r| r.as_json().as_bytes().to_vec())
            .collect();

        drop(mdk);
        info!("Added member '{}' to group '{}'", member_identity, group_id);
        Ok((commit_bytes, welcome_jsons))
    }

    pub fn add_member_with_key_package_event(
        &mut self,
        group_id: &str,
        kp_event: &nostr::Event,
    ) -> Result<(Vec<u8>, Vec<Vec<u8>>)> {
        let mls_group_id = self.resolve_mls_group_id(group_id)?;

        let mdk = self.mdk.lock().map_err(|e| anyhow!("MDK lock poisoned: {}", e))?;

        let _key_package = mdk.parse_key_package(kp_event)
            .map_err(|e| anyhow!("Failed to parse key package event: {:?}", e))?;

        let result = mdk.add_members(&mls_group_id, &[kp_event.clone()])
            .map_err(|e| anyhow!("Failed to add member via key package: {:?}", e))?;

        mdk.merge_pending_commit(&mls_group_id)
            .map_err(|e| anyhow!("Failed to merge pending commit: {:?}", e))?;

        let commit_bytes = result.evolution_event.as_json().as_bytes().to_vec();

        let welcome_jsons: Vec<Vec<u8>> = result.welcome_rumors
            .unwrap_or_default()
            .iter()
            .map(|r| r.as_json().as_bytes().to_vec())
            .collect();

        drop(mdk);
        let member_pk = kp_event.pubkey.to_hex();
        info!("Added member '{}' to group '{}' via kind:30443 key package", member_pk, group_id);
        Ok((commit_bytes, welcome_jsons))
    }

    pub fn process_welcome(&mut self, welcome_bytes: &[u8], identity: String) -> Result<String> {
        let welcome_json = String::from_utf8(welcome_bytes.to_vec())
            .map_err(|e| anyhow!("Welcome bytes are not valid UTF-8: {:?}", e))?;
        let rumor: UnsignedEvent = UnsignedEvent::from_json(&welcome_json)
            .map_err(|e| anyhow!("Failed to parse welcome rumor: {:?}", e))?;
        self.process_welcome_rumor(&rumor, &identity)
    }

    pub fn process_welcome_rumor(&mut self, rumor: &UnsignedEvent, identity: &str) -> Result<String> {
        let rumor_id = rumor.id
            .ok_or_else(|| anyhow!("Welcome rumor has no event ID"))?;

        let mdk = self.mdk.lock().map_err(|e| anyhow!("MDK lock poisoned: {}", e))?;
        let welcome = mdk.process_welcome(&rumor_id, rumor)
            .map_err(|e| anyhow!("Failed to process welcome: {:?}", e))?;

        let group_name = welcome.group_name.clone();
        let mls_gid = welcome.mls_group_id.clone();

        mdk.accept_welcome(&welcome)
            .map_err(|e| anyhow!("Failed to accept welcome: {:?}", e))?;

        self.name_to_mls_id.insert(group_name.clone(), mls_gid);

        drop(mdk);
        info!("Joined MLS group '{}' via welcome (identity={})", group_name, identity);
        Ok(group_name)
    }

    pub fn encrypt(&mut self, group_id: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mls_group_id = self.resolve_mls_group_id(group_id)?;

        let creator_keys = self.keys.values().next().cloned()
            .ok_or_else(|| anyhow!("No keys available for encryption"))?;

        let content = String::from_utf8_lossy(plaintext).to_string();
        let rumor = EventBuilder::new(Kind::Custom(9), &content)
            .build(creator_keys.public_key());

        let mdk = self.mdk.lock().map_err(|e| anyhow!("MDK lock poisoned: {}", e))?;
        let event = mdk.create_message(&mls_group_id, rumor, None)
            .map_err(|e| anyhow!("MLS encrypt failed: {:?}", e))?;

        let event_json = event.as_json();
        Ok(event_json.as_bytes().to_vec())
    }

    pub fn decrypt(&mut self, _group_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let event_json = String::from_utf8(ciphertext.to_vec())
            .map_err(|e| anyhow!("Ciphertext is not valid UTF-8: {:?}", e))?;
        let event: nostr::Event = nostr::Event::from_json(&event_json)
            .map_err(|e| anyhow!("Failed to parse Nostr event from ciphertext: {:?}", e))?;

        let mdk = self.mdk.lock().map_err(|e| anyhow!("MDK lock poisoned: {}", e))?;

        let result = mdk.process_message(&event)
            .map_err(|e| anyhow!("MLS decrypt failed: {:?}", e))?;

        match result {
            MessageProcessingResult::ApplicationMessage(msg) => {
                Ok(msg.content.into_bytes())
            }
            MessageProcessingResult::Commit { .. } => {
                Ok(Vec::new())
            }
            MessageProcessingResult::Proposal(_) => {
                Ok(Vec::new())
            }
            MessageProcessingResult::PendingProposal { .. } => {
                Ok(Vec::new())
            }
            MessageProcessingResult::IgnoredProposal { .. } => {
                Ok(Vec::new())
            }
            MessageProcessingResult::ExternalJoinProposal { .. } => {
                Ok(Vec::new())
            }
            MessageProcessingResult::Unprocessable { .. } => {
                Err(anyhow!("Message unprocessable"))
            }
            MessageProcessingResult::PreviouslyFailed => {
                Err(anyhow!("Message previously failed processing"))
            }
        }
    }

    pub fn get_members(&self, group_id: &str) -> Result<Vec<String>> {
        let mls_group_id = self.resolve_mls_group_id(group_id)?;

        let mdk = self.mdk.lock().map_err(|e| anyhow!("MDK lock poisoned: {}", e))?;
        let members = mdk.get_members(&mls_group_id)
            .map_err(|e| anyhow!("Failed to get members: {:?}", e))?;

        Ok(members.iter().map(|pk| pk.to_hex()).collect())
    }

    pub fn has_group(&self, group_id: &str) -> bool {
        if self.name_to_mls_id.contains_key(group_id) {
            return true;
        }
        self.find_group_by_name(group_id)
            .ok()
            .flatten()
            .is_some()
    }

    pub fn group_ids(&self) -> Vec<String> {
        let mdk = match self.mdk.lock() {
            Ok(guard) => guard,
            Err(_) => return Vec::new(),
        };
        match mdk.get_groups() {
            Ok(groups) => groups.iter().map(|g| g.name.clone()).collect(),
            Err(_) => Vec::new(),
        }
    }
}

fn build_key_package_event(kp_data: &mdk_core::key_packages::KeyPackageEventData) -> Result<nostr::Event> {
    let d_tag_value = kp_data.d_tag.clone();
    let tags: Vec<nostr::Tag> = kp_data.tags_30443.iter()
        .cloned()
        .chain(std::iter::once(nostr::Tag::custom(nostr::TagKind::d(), [d_tag_value])))
        .collect();

    let ephemeral_keys = Keys::generate();

    let event = EventBuilder::new(Kind::Custom(30443), &kp_data.content)
        .tags(tags)
        .sign_with_keys(&ephemeral_keys)
        .map_err(|e| anyhow!("Failed to sign key package event: {:?}", e))?;

    Ok(event)
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
        assert!(manager.has_group("test"));
    }

    #[test]
    fn test_create_and_encrypt() {
        let mut manager = MlsGroupManager::new();
        manager.create_group("test".to_string(), "alice".to_string()).unwrap();
        assert!(manager.has_group("test"));

        let plaintext = b"Hello, MLS!";
        let _ciphertext = manager.encrypt("test", plaintext).unwrap();
    }

    #[test]
    fn test_group_ids() {
        let mut manager = MlsGroupManager::new();
        manager.create_group("alpha".to_string(), "user1".to_string()).unwrap();
        manager.create_group("beta".to_string(), "user1".to_string()).unwrap();
        let ids = manager.group_ids();
        assert!(ids.contains(&"alpha".to_string()));
        assert!(ids.contains(&"beta".to_string()));
    }

    #[test]
    fn test_e2e_two_identities() {
        let mut alice = MlsGroupManager::new();
        let mut bob = MlsGroupManager::new();

        let alice_keys = Keys::generate();
        let bob_keys = Keys::generate();
        let alice_pk = alice_keys.public_key();
        let bob_pk = bob_keys.public_key();

        alice.keys.insert("alice".to_string(), alice_keys.clone());
        bob.keys.insert("bob".to_string(), bob_keys.clone());

        alice.create_group("chat".to_string(), "alice".to_string()).unwrap();
        assert!(alice.has_group("chat"));

        let bob_kp_event = bob.create_key_package_event("bob").unwrap();

        let (_commit_bytes, welcome_jsons) = alice.add_member_with_key_package_event("chat", &bob_kp_event).unwrap();
        assert!(!welcome_jsons.is_empty(), "Should produce at least one welcome rumor");

        let welcome_bytes = &welcome_jsons[0];
        let group_name = bob.process_welcome(welcome_bytes, "bob".to_string()).unwrap();
        assert_eq!(group_name, "chat");
        assert!(bob.has_group("chat"));

        let alice_members = alice.get_members("chat").unwrap();
        assert_eq!(alice_members.len(), 2, "Alice should see 2 members in the group");

        let bob_members = bob.get_members("chat").unwrap();
        assert_eq!(bob_members.len(), 2, "Bob should see 2 members in the group");

        let plaintext = b"Hello from Alice!";
        let ciphertext = alice.encrypt("chat", plaintext).unwrap();

        let decrypted = bob.decrypt("chat", &ciphertext).unwrap();
        let decrypted_str = String::from_utf8(decrypted).unwrap();
        assert_eq!(decrypted_str, "Hello from Alice!", "Bob should decrypt Alice's message");

        let reply = b"Hi Alice, from Bob!";
        let reply_ct = bob.encrypt("chat", reply).unwrap();

        let reply_dec = alice.decrypt("chat", &reply_ct).unwrap();
        let reply_str = String::from_utf8(reply_dec).unwrap();
        assert_eq!(reply_str, "Hi Alice, from Bob!", "Alice should decrypt Bob's reply");
    }
}
