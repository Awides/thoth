# Persistence & Key Storage

## Architecture

```
~/.thoth/
├── keys/           # Encrypted credentials
├── thoth.mv2       # Event log (NO secrets!)
└── thoth.mv2.idx   # Indexes
```

## What's Saved Where

### In memvid (.mv2 file):
- ✅ All chat messages
- ✅ Onboarding events (audit trail)
- ✅ UI state snapshots
- ✅ System events

### In key_storage (~/.thoth/keys/):
- 🔐 Encrypted master seed (BIP39)
- 🔐 Encrypted device key (Nostr nsec)
- 📝 Key metadata (pubkey, timestamps)

## Next Steps

1. Wire up onboarding UI to use `complete_onboarding_full()`
2. Implement real encryption in key_storage
3. Add key backup/export UI
4. Multi-device sync via memvid event replay
