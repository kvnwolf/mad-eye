# 0004. Sign every build with a stable identity so Keychain ACL grants survive rebuilds

Date: 2026-07-22
Status: accepted

macOS binds a Keychain item's "Always Allow" ACL entry to the requesting app's *designated requirement*. An ad-hoc-signed binary (what cargo/`tauri dev` produces) has a cdhash-based requirement — the hash of the exact binary — so every rebuild looked like a brand-new app and re-triggered the authorization prompt for Claude Code's credentials item within one 180s poll tick.

Fix: sign every build with a stable certificate identity, pinning the identifier to `com.kvnwolf.mad-eye` so the requirement is identity-based. Dev builds are re-signed at launch with the "Apple Development" certificate by the cargo runner in `src-tauri/.cargo/config.toml`; packaged builds are Developer ID-signed via `bundle.macOS.signingIdentity` (ADR 0005). The two identities differ, so dev builds and the installed app each need ONE "Always Allow" — but each grant survives every rebuild/update of its kind.

Ruled out: Claude Code does NOT recreate its item on token rotation (item `cdat` is months old while `mdat` is fresh — it updates in place, preserving the ACL), so no prompt cadence comes from that side. Ceilings: the Apple Development certificate expires yearly (renewal = one new prompt), and the identity exists only on this machine — the dev runner falls back to launching unsigned elsewhere (prompts recur there), while `tauri build` needs the certificate or the `signingIdentity` removed. The certificate comes from free Xcode provisioning (Personal Team) — no paid Developer Program needed; notarization for Gatekeeper-clean downloads would need the paid program and is out of scope. Check: `codesign -d -r- src-tauri/target/debug/mad-eye` must show a `certificate leaf`-based requirement, not `cdhash`.
