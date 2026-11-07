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
# Uso, dalla radice del repository, su un Mac:  bash tools/release/build-macos.sh [cartella-di-uscita]
set -euo pipefail

target=aarch64-apple-darwin
out="${1:-}"

export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=${HOME}=~"
rustup target add "$target" >/dev/null
npx tauri build --target "$target" --bundles dmg

target_dir=$(cargo metadata --format-version 1 --no-deps | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).target_directory))')
bin="$target_dir/$target/release/micfx"
user=$(basename "$HOME")
if grep -q "/$user/" "$bin"; then
  echo "micfx contiene ancora '$user': non pubblicarlo." >&2
  exit 1
fi

for f in "$target_dir/$target/release/bundle/dmg/"*.dmg; do
  shasum -a 256 "$f" | sed "s|  .*/|  |"
  if [ -n "$out" ]; then
    mkdir -p "$out"
    cp "$f" "$out/"
  fi
done
