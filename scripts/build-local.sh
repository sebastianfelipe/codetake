#!/usr/bin/env bash
# Builds CodeTake.app and the .dmg for this Mac, signed with a certificate
# from your keychain when one is available.
#
# Why: macOS remembers Screen Recording, Camera and Microphone permissions
# per code signature. An ad-hoc signature changes with every build, so each
# rebuilt app looks new and asks for permission again. Signing with a
# certificate (a free "Apple Development" one from Xcode works) keeps the
# identity stable across builds.
set -euo pipefail

cd "$(dirname "$0")/.."

identity="${APPLE_SIGNING_IDENTITY:-}"
if [[ -z "$identity" ]]; then
  identities="$(security find-identity -v -p codesigning 2>/dev/null || true)"
  if grep -q '"Developer ID Application' <<<"$identities"; then
    identity="Developer ID Application"
  elif grep -q '"Apple Development' <<<"$identities"; then
    identity="Apple Development"
  fi
fi

if [[ -z "$identity" ]]; then
  echo "No code signing certificate found; building ad-hoc signed." >&2
  echo "macOS will ask for permissions again after every rebuild." >&2
  echo "Tip: sign in to Xcode (Settings → Accounts) to get a free Apple Development certificate." >&2
  identity="-"
fi

echo "Signing with: $identity"
APPLE_SIGNING_IDENTITY="$identity" pnpm --filter @codetake/desktop tauri build --target universal-apple-darwin "$@"
