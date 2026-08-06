# Signing Policy

LingBi maintains three independent trust roots:

1. Windows code-signing certificate
2. Tauri updater private/public key
3. Offline entitlement Ed25519 private/public key

These credentials must never be reused across roots. Private keys must never
be committed to source control.

Formal Windows GA requires:

- signed installer
- signed binary where appropriate
- valid updater signature
- published download hash
- assessed SmartScreen/reputation behavior

If no valid commercial signing solution is available, the release must be
classified as Public Beta, not Commercial GA.
