#!/bin/sh
# Build, notarize, and staple the distributable universal DMG (docs/adr/0005).
# Needs the Developer ID cert in the login Keychain and the "mad-eye-notary"
# notarytool credential profile (`xcrun notarytool store-credentials mad-eye-notary`).
set -e
bun tauri build --target universal-apple-darwin
DMG=$(ls -t src-tauri/target/universal-apple-darwin/release/bundle/dmg/mad-eye_*.dmg | head -1)
xcrun notarytool submit "$DMG" --keychain-profile mad-eye-notary --wait
xcrun stapler staple "$DMG"
echo "Notarized and stapled: $DMG"
