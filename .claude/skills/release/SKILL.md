---
name: release
description: Cut a GitHub release of mad-eye — inferred bump, universal macOS DMG, tag, GitHub release with changelog, and Homebrew cask bump.
disable-model-invocation: true
argument-hint: "[patch|minor|major]"
---

Cut ONE release of **mad-eye** (the macOS menubar app) to **GitHub Releases + a Homebrew cask**. Every step ends on a gate — a checkable condition; stop and report at the first gate that fails.

Distribution is **unsigned/ad-hoc on purpose**: mad-eye uses `macOSPrivateApi` (the transparent + vibrancy Popover), which bars the Mac App Store, and its audience is Claude Code users (small, technical). So we ship an unsigned **Homebrew cask**: users clear the Gatekeeper quarantine themselves (`xattr -dr com.apple.quarantine`, or System Settings → Privacy & Security → Open Anyway) since Homebrew 6 quarantines casks with no opt-out — cheaper than an Apple Developer ID + notarization, at the cost of that one-time friction. Graduate to signing only if it gets real traction. The app version lives in `src-tauri/tauri.conf.json` (`CFBundleVersion` derives from it); `package.json` + `src-tauri/Cargo.toml` mirror it in lockstep.

**One-time setup** (before the FIRST release, else Step 1/3/5 gates fail): a GitHub remote exists (`git remote add origin …` + an initial push); both universal Rust targets installed (`rustup target add aarch64-apple-darwin x86_64-apple-darwin`); `gh` authenticated. The Homebrew tap (`kvnwolf/homebrew-tap`) and its cask are scaffolded by Step 5 on the first release if missing.

## Step 1: Preflight gates

From the **main checkout** (NOT a worktree), ALL must pass:

1. **Main checkout**: `[ "$(git rev-parse --git-dir)" = "$(git rev-parse --git-common-dir)" ]` → true. A worktree can't release.
2. **On main, clean, current**: `git rev-parse --abbrev-ref HEAD` → `main`; `git status --porcelain` empty; if main has an upstream (`git rev-parse --abbrev-ref main@{upstream}` resolves), `git pull --ff-only` succeeds.
3. **GitHub reachable**: `gh auth status` ok AND `git remote get-url origin` resolves. No remote → STOP and point to the one-time setup (create + push the repo first).
4. **Universal toolchain**: `bun tauri --version` works; `rustup target list --installed` includes BOTH `aarch64-apple-darwin` and `x86_64-apple-darwin` (missing x86_64 → `rustup target add x86_64-apple-darwin`, then re-check).
5. **CI** (only if `.github/workflows/` exists): `gh run list --branch main --limit 1` → completed/success (watch it to completion if still running). No workflows yet → skip.

## Step 2: Infer the bump and apply it

- Argument `patch`/`minor`/`major` → use it, skip inference.
- Otherwise infer from the commits since the last release:
  - Last tag: `git describe --tags --abbrev=0 --match 'v*'`. **No tag** (first release) → nothing to infer from; AskUserQuestion for the bump (or release the current `tauri.conf.json` version as-is if it was never released).
  - Range: `git log <last-tag>..HEAD --pretty=format:'%h %s'` (subjects) and `git log <last-tag>..HEAD --pretty=%B | grep -c 'BREAKING CHANGE'` (bodies).
  - Rules, first match wins: any subject with `!` before the `:` OR any `BREAKING CHANGE` body → **major**; else any `feat` subject → **minor**; else → **patch**.
  - **0.x exception**: inferred major while the current version is `<1.0.0` → AskUserQuestion (true `1.0.0` vs the 0.x convention of shipping breaking as minor) — don't cross to 1.0.0 silently.
  - Report the inferred bump AND the commit subjects that justify it before proceeding — the user reads the reasoning, no confirmation gate.
- Apply the new version `<V>` in **lockstep** to all three:
  - `src-tauri/tauri.conf.json` → `"version"` (the source of truth — `CFBundleVersion`/`CFBundleShortVersionString` derive from it).
  - `package.json` → `"version"`.
  - `src-tauri/Cargo.toml` → `version` under `[package]`.
- Sync the Rust lockfile **and** sanity-compile before committing: `(cd src-tauri && cargo check)` — any cargo command reconciles `Cargo.lock`'s local-crate version to `<V>`, and a green check is a cheap pre-build gate. Gate: it compiles.
- `git add -A && git commit -m "release: v<V>"`. Do NOT push — the push gates on a successful build (Step 4). A failed build is undone with `git reset --hard HEAD~1` while the commit is still local.

## Step 3: Build the universal DMG

- `bun tauri build --bundles dmg --target universal-apple-darwin` — builds a Universal binary (Apple Silicon + Intel) and runs `beforeBuildCommand` (`vite build`) first.
- Locate the artifact: `DMG=$(ls src-tauri/target/universal-apple-darwin/release/bundle/dmg/*.dmg)`.
- Gate: exactly ONE `.dmg` at that path (`$DMG` is non-empty and a single match), AND the built app's version matches —
  `/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "src-tauri/target/universal-apple-darwin/release/bundle/macos/mad-eye.app/Contents/Info.plist"` → `<V>`.
  Any mismatch/failure → `git reset --hard HEAD~1` and stop (the bump is still local).

## Step 4: Tag, push, GitHub release with changelog + DMG

- `git tag v<V> && git push origin main v<V>` (pushes the bump commit + the tag; on a first release this also publishes `main` to origin).
- Build the notes from the SAME commit range used for the bump inference (single source for bump + changelog). Sections in this order, empty sections omitted, each line `- <subject> (<short-sha>)`:
  - `### Breaking changes` (commits that matched the major rule), `### Features` (`feat`), `### Fixes` (`fix`), `### Other` (the rest).
  - First release (no prior tag): notes are just `Initial release.`
  - Prepend an install line at the top: `` Install: `brew install --cask kvnwolf/tap/mad-eye` ``.
  - Write the notes to a temp file (multi-line survives intact).
- `gh release create v<V> --title "v<V>" --notes-file <notes-file> "$DMG"` — the positional arg uploads the DMG as the release asset.
- Gate: `gh release view v<V>` exits 0 and lists the `.dmg` asset.

## Step 5: Bump the Homebrew cask

The cask changes only two fields per release — `version` and `sha256` (the download URL templates `version`).

- `SHA=$(shasum -a 256 "$DMG" | cut -d' ' -f1)`.
- Clone the tap into a scratch dir (create it on the first release):
  ```bash
  TAP=$(mktemp -d)
  if gh repo view kvnwolf/homebrew-tap >/dev/null 2>&1; then
    gh repo clone kvnwolf/homebrew-tap "$TAP"
  else
    gh repo create kvnwolf/homebrew-tap --public
    gh repo clone kvnwolf/homebrew-tap "$TAP"
  fi
  mkdir -p "$TAP/Casks"
  ```
- If `$TAP/Casks/mad-eye.rb` is **missing**, scaffold it from the template below (fill `<V>` + `$SHA`). Otherwise update in place:
  - `sed -i '' "s/^  version .*/  version \"<V>\"/" "$TAP/Casks/mad-eye.rb"`
  - `sed -i '' "s/^  sha256 .*/  sha256 \"$SHA\"/" "$TAP/Casks/mad-eye.rb"`
- Commit + push: `git -C "$TAP" add Casks/mad-eye.rb && git -C "$TAP" commit -m "mad-eye <V>" && git -C "$TAP" push`.
- Gate: the pushed cask's `version` line reads `<V>`; if `brew` is installed, `brew audit --cask "$TAP/Casks/mad-eye.rb"` passes (at minimum `ruby -c "$TAP/Casks/mad-eye.rb"` parses).

**Cask template** (first-release scaffold — only `version` + `sha256` change on later releases):

```ruby
cask "mad-eye" do
  version "<V>"
  sha256 "<SHA>"

  url "https://github.com/kvnwolf/mad-eye/releases/download/v#{version}/mad-eye_#{version}_universal.dmg"
  name "mad-eye"
  desc "Menubar Eye tracking Claude subscription usage"
  homepage "https://github.com/kvnwolf/mad-eye"

  depends_on macos: :monterey

  app "mad-eye.app"

  zap trash: [
    "~/Library/Application Support/com.kvnwolf.mad-eye",
    "~/Library/LaunchAgents/com.kvnwolf.mad-eye.plist",
    "~/Library/Saved Application State/com.kvnwolf.mad-eye.savedState",
  ]

  caveats <<~EOS
    mad-eye reads Claude Code's OAuth credentials from your Keychain, so it needs
    Claude Code installed and logged in. On first launch macOS asks to read the
    "Claude Code-credentials" item — click "Always Allow".
  EOS
end
```

## Step 6: Smoke + report

- Optional local smoke (skip if the user doesn't want it live-installed on this machine): `brew install --cask --force kvnwolf/tap/mad-eye`, then `xattr -dr com.apple.quarantine /Applications/mad-eye.app` (Homebrew 6 quarantines casks with no opt-out — `--no-quarantine` is gone; the `xattr` clears it so the unsigned app opens). Launch and confirm the Eye appears in the menubar.
- Report to the user: version published, inferred bump + reasoning, DMG path + `sha256`, tag + GitHub release URL, cask commit URL, and the `brew install --cask kvnwolf/tap/mad-eye` command.

## Acceptance checklist

- [ ] All Step-1 preflight gates passed (main checkout + branch, clean + current, gh auth + remote, both universal targets, CI if present)
- [ ] Bump inferred from the commit range (or taken from the argument); reasoning reported; 0.x-major and first-release cases asked, never assumed
- [ ] Version applied in lockstep to `tauri.conf.json` + `package.json` + `Cargo.toml`; `Cargo.lock` synced via `cargo check`; committed `release: v<V>`; nothing pushed before the build succeeded
- [ ] Universal DMG built; exactly one artifact; the built `.app`'s `CFBundleShortVersionString` equals `<V>`
- [ ] `v<V>` tagged and pushed with main; GitHub release created with the DMG asset and the type-grouped changelog from the same commit range as the bump
- [ ] Homebrew cask bumped (scaffolded on the first release): `version` + `sha256` updated, committed + pushed to `kvnwolf/homebrew-tap`
- [ ] Reported: version, bump reasoning, DMG + sha, release URL, cask URL, `brew install --cask` command
