#!/usr/bin/env bash
# Fetch the extracted avatar sprite-sheet library from Google Drive and
# unpack it into assets/sprites/avatar/.
#
# The zip is produced by tools/avatar-pipeline/extract.mjs against the
# upstream glitch-avatars SWF library. Running that pipeline locally takes
# ~30 minutes of active CPU (caffeinate recommended to prevent idle sleep);
# this fetch script is the fast path for contributors who just need the
# assets to get a rendering avatar in-game.
#
# Content: ~32 MB compressed, ~79 MB unpacked, 661 items / 1,547 sheets.

set -euo pipefail

FILE_ID="1iL1crPLmU5gsfVxo1zd8aPFfrRaEaIkC"
EXPECTED_SHA256="b455ceaca010a42454faa2b4a0c579e413e5708147b546604d4a775e5fd78b89"
OUT_ROOT="assets/sprites"
OUT_DIR="${OUT_ROOT}/avatar"

# Script lives in scripts/; cd to repo root so relative paths work regardless
# of where the script is invoked from.
cd "$(dirname "$0")/.."

if [ -f "$OUT_DIR/manifest.json" ]; then
  echo "Avatar assets already present at $OUT_DIR/."
  echo "Remove the directory first if you want to re-fetch: rm -rf $OUT_DIR"
  exit 0
fi

# Unique temp path + guaranteed cleanup on any exit path (success, failure,
# interrupt). Defends against concurrent invocations and stale files on crash.
ZIP_PATH="$(mktemp "${TMPDIR:-/tmp}/harmony-glitch-avatar-assets.XXXXXX")"
trap 'rm -f "$ZIP_PATH"' EXIT

echo "Fetching avatar asset zip from Google Drive..."
# For files under ~100 MB, this direct URL works without a confirm-token.
# If the file ever outgrows that or Google changes policy, the script errors
# out below rather than silently unpacking an HTML interstitial page.
curl -fsSL --max-time 300 -o "$ZIP_PATH" \
  "https://drive.google.com/uc?export=download&id=$FILE_ID"

# A valid zip starts with the local-file-header magic "PK\x03\x04". If Google
# returned an HTML "confirm download" page instead, bail with guidance.
if [ "$(head -c 4 "$ZIP_PATH" | xxd -p)" != "504b0304" ]; then
  cat >&2 <<EOF
Error: downloaded file is not a zip.

Google Drive may be gating this file behind a virus-scan interstitial
(more common for files >100 MB, or if many users have downloaded it
recently). Workarounds:

  1. Download manually from
       https://drive.google.com/file/d/$FILE_ID/view
     and unzip into $OUT_DIR/

  2. Or regenerate locally:
       caffeinate -i node tools/avatar-pipeline/extract.mjs
EOF
  exit 1
fi

# Verify the download matches the pinned hash — protects against a corrupted
# transfer or a swapped-out Drive file. Content integrity comes from this
# single check; the path/mtime/ownership of the zip are not trusted.
if command -v sha256sum >/dev/null 2>&1; then
  actual_sha256="$(sha256sum "$ZIP_PATH" | awk '{print $1}')"
else
  actual_sha256="$(shasum -a 256 "$ZIP_PATH" | awk '{print $1}')"
fi
if [ "$actual_sha256" != "$EXPECTED_SHA256" ]; then
  cat >&2 <<EOF
Error: SHA256 mismatch for avatar asset zip.
  expected: $EXPECTED_SHA256
  actual:   $actual_sha256

The zip at the pinned Google Drive URL has changed since this script was
written, or the download was corrupted. If intentional (new asset batch),
regenerate the hash with:
  shasum -a 256 <path-to-new-zip>
and update EXPECTED_SHA256 at the top of this script.
EOF
  exit 1
fi

echo "Unpacking into $OUT_DIR/..."
mkdir -p "$OUT_ROOT"
unzip -q -o "$ZIP_PATH" -d "$OUT_ROOT"

echo "Done. Loaded categories:"
jq -r '.categories | to_entries[] | "  \(.key): \(.value.items | length) items"' \
  "$OUT_DIR/manifest.json"
