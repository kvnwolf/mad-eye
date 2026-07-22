# Capturing the launch visuals

The README references two images — drop them in this folder with these exact names.

## 1. `demo.gif` — the hero (and your tweet's video)

A short (5–10s) screen recording of the Eye in the menubar: let it climb through the moods and
shatter, then click it to open the popover.

**Record it**
- Tool: [Kap](https://getkap.co) (free, open source, exports GIF **and** MP4) — or QuickTime → New Screen Recording.
- Drive the moods on demand instead of waiting for real usage (the Eye reads `MAD_EYE_FAKE_PCT`):
  ```sh
  MAD_EYE_FAKE_PCT=40  bun tauri dev   # calm
  MAD_EYE_FAKE_PCT=90  bun tauri dev   # paranoid
  MAD_EYE_FAKE_PCT=97  bun tauri dev   # frantic
  MAD_EYE_FAKE_PCT=100 bun tauri dev   # shattered
  ```
  Record a zoomed-in region around the menubar Eye so it reads at tweet size.
- Click the Eye to reveal the frosted popover with the gauges.

**Export**
- **`demo.gif`** for the README — ~640px wide, keep it **under 10 MB** (GitHub's cap; smaller loops load faster).
- **`demo.mp4`** to attach to the tweet — X prefers native video (autoplays, loops, far more reach than a link). Don't commit the MP4 to the README; GitHub won't autoplay a committed MP4. Attach it straight to the tweet.

## 2. `popover.png` — the detail shot

Open the popover, then **⌘⇧4 → Space → click the popover** to grab just that window (with its shadow). Save as `popover.png`.

## Repo social preview (the X link-card thumbnail)

If you paste the repo link in the tweet, X shows a card from the image at
**repo → Settings → General → Social preview** (1280×640). A still of the demo — Eye + a sliver of
the popover — works well. (If you attach the MP4 directly instead, the video is the star and this
matters less.)
