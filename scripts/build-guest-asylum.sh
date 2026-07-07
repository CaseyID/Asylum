#!/usr/bin/env bash
# Build the static musl `asylum` binary that runs INSIDE a Loon microVM guest as
# the injected MCP server (`asylum mcp`) and harness-event bridge
# (`asylum harness-event ...`). It is staged into the VM at /usr/local/bin/asylum
# by the Loon substrate at provision (`loon cp`).
#
# The guest is a different C library environment than the host, so the binary
# MUST be a fully static x86_64-unknown-linux-musl build. reqwest uses rustls (no
# OpenSSL) and rusqlite is bundled, so the workspace links cleanly against musl.
#
# The unified `asylum` binary also contains the daemon, whose release build
# embeds cockpit/dist via rust-embed. The guest never serves cockpit, so if
# cockpit/dist is absent (e.g. cockpit assets not built in this checkout) we
# stage a tiny inert placeholder purely so rust-embed has a folder to embed. The
# placeholder is never served in the guest and lives under the gitignored dist/.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

target="x86_64-unknown-linux-musl"

if ! rustup target list --installed 2>/dev/null | grep -q "$target"; then
  echo "error: rust target $target is not installed (rustup target add $target)" >&2
  exit 1
fi

# Inert cockpit placeholder so the release embed compiles when real assets are
# absent (guest never serves cockpit).
if [ ! -f cockpit/dist/index.html ]; then
  mkdir -p cockpit/dist/assets
  printf '<!doctype html><title>asylum guest</title>\n' > cockpit/dist/index.html
fi

export CC_x86_64_unknown_linux_musl="${CC_x86_64_unknown_linux_musl:-musl-gcc}"

cargo build -p asylum --release --target "$target" "$@"

out="target/$target/release/asylum"
# Strip symbols to shrink the staged binary (fewer cp chunks into the guest).
strip -s "$out" 2>/dev/null || true
echo "built guest asylum binary: $repo_root/$out ($(du -h "$out" | cut -f1))"
file "$out" 2>/dev/null || true
