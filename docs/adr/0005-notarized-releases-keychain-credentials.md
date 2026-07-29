# 0005. Releases are Developer ID-signed and notarized; credentials never leave the Keychain

Date: 2026-07-29
Status: accepted

`tauri build` signs bundles with "Developer ID Application: Kevin Villalobos" (`bundle.macOS.signingIdentity`), and `scripts/release-mac.sh` then notarizes and staples the universal DMG via `xcrun notarytool submit --keychain-profile mad-eye-notary --wait`. Notarized downloads pass Gatekeeper cleanly, which retired the README's `xattr` quarantine workaround.

Tauri can notarize during the build itself, but only by receiving the Apple ID app-specific password through environment variables. The post-build `notarytool` step instead uses the credential profile stored once in the login Keychain (`xcrun notarytool store-credentials mad-eye-notary`), so no secret ever appears in the repo, the environment, or CI. Releases are built only on this machine — CI never runs `tauri build`, so the certificate and profile never need exporting.

Keychain-ACL consequence (amends ADR 0004): dev builds (Apple Development identity) and the installed app (Developer ID identity) carry different designated requirements, so each needs its own one-time "Always Allow" on Claude Code's credentials item. Ceiling: the Developer ID certificate expires in 5 years; renewal means one new prompt for installed builds.
