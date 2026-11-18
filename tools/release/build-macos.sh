#!/usr/bin/env bash
# App per macOS da pubblicare (release su GitHub): .dmg per Apple Silicon (arm64).
#
# Come `npm run build`, ma i percorsi della cartella utente (es. /Users/<nome>/.cargo/registry/...),
# che rustc scrive nei messaggi di panic delle dipendenze, diventano "~": il binario pubblico non
# contiene il nome utente di chi l'ha compilato.
#
# Firma ad-hoc (`signingIdentity: "-"` in tauri.conf.json): senza un account Apple Developer l'app non
# è notarizzata e al primo avvio macOS chiede conferma (vedi le note di release).
#
# Con TAURI_SIGNING_PRIVATE_KEY produce anche l'aggiornamento automatico: MicFX_<versione>_aarch64.app.tar.gz
# e la sua firma .sig (vedi tauri.release.conf.json).
#
# Uso, dalla radice del repository, su un Mac:  bash tools/release/build-macos.sh [cartella-di-uscita]
set -euo pipefail

target=aarch64-apple-darwin
out="${1:-}"

export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=${HOME}=~"
rustup target add "$target" >/dev/null
if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  npx tauri build --target "$target" --bundles app,dmg --config src-tauri/tauri.release.conf.json
else
  echo "TAURI_SIGNING_PRIVATE_KEY assente: niente aggiornamento automatico." >&2
  npx tauri build --target "$target" --bundles dmg
fi

target_dir=$(cargo metadata --format-version 1 --no-deps | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).target_directory))')
bin="$target_dir/$target/release/micfx"
user=$(basename "$HOME")
if grep -q "/$user/" "$bin"; then
  echo "micfx contiene ancora '$user': non pubblicarlo." >&2
  exit 1
fi

version=$(node -p "require('./src-tauri/tauri.conf.json').version")
bundle="$target_dir/$target/release/bundle"
for f in "$bundle/dmg/"*.dmg; do
  shasum -a 256 "$f" | sed "s|  .*/|  |"
  if [ -n "$out" ]; then
    mkdir -p "$out"
    cp "$f" "$out/"
  fi
done
# Aggiornamento automatico: nome con versione e architettura, come il .dmg.
if [ -n "$out" ] && [ -f "$bundle/macos/MicFX.app.tar.gz" ]; then
  cp "$bundle/macos/MicFX.app.tar.gz" "$out/MicFX_${version}_aarch64.app.tar.gz"
  cp "$bundle/macos/MicFX.app.tar.gz.sig" "$out/MicFX_${version}_aarch64.app.tar.gz.sig"
fi
