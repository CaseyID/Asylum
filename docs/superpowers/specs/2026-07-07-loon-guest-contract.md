# Loon guest contract for the Asylum substrate rewrite (verified 2026-07-07)

Facts established by the Phase C prep work (LoonV2 main merged at 2662310; live host reinstalled on that build and healthy at https://127.0.0.1:7777). The Phase C Asylum loon-substrate rewrite builds against this. Everything below was verified live on this machine, including real claude/codex calls from inside a microVM.

## LoonV2 upstream changes shipped (merged to LoonV2 main, host reinstalled)

- Installer fix: `loon-host install` no longer silently skips `manifest://` artifacts it cannot resolve on PATH (this was why vmlinux never got staged). It honors `LOON_KERNEL`/`LOON_FIRECRACKER`/`LOON_JAILER`/`LOON_GUEST_BINARY` and generic `LOON_ARTIFACT_DIR/<name>` overrides, bails loudly when an artifact is unresolvable and absent at the destination, and is idempotent when already staged.
- Tombstones: `loon vm ls` hides `destroyed` instances by default (`--all` to show; `ListInstancesQuery.include_destroyed`). New prune path: `loon vm prune` / `POST /instances/prune` deletes destroyed rows plus dependent events/execs/attachments/allocations transactionally.

## The guest image

- Reference is a LOCAL OCI-TAR PATH, not a tag: `/var/lib/loon/agent-images/claude-dev.oci.tar` (digest sha256:30e7c0bc...). `loon run <path> -- <cmd>` and `loon vm create <path>` both accept it. Stage tars where user `loon` can read them.
- Contents: node 22 (bookworm-slim base), npm, git, ripgrep, curl, ca-certificates, npm-global `@anthropic-ai/claude-code` and `@openai/codex`, both on PATH. Built off-repo via docker buildx (`--output type=oci`; `docker save` is NOT OCI-layout), imported offline with `loon image pull`.
- loon-guest is baked as /sbin/init at image-import time and the ext4 is cached by digest: after any loon-guest rebuild, `loon image rm <digest>` + re-pull, or VMs boot a stale guest.
- Network egress works out of the box: guest gets a /30 from 10.42.0.0/16, NAT via loon-netd, DNS/IP/route on the kernel cmdline. api.anthropic.com verified reachable over HTTPS.

## Driving harnesses inside a VM (all verified live)

- `loon exec` has NO stdin on the non-PTY path. Deliver file content with `loon cp <local> <vm>:/abs/path --mode <decimal>` (384 = 0600).
- Exec runs as root with HOME unset: always `export HOME=/root` (or pass env) before launching claude/codex.
- Credential injection (never bake into the image; per-VM, after create):
  - `loon cp ~/.claude/.credentials.json <vm>:/root/.claude/.credentials.json --mode 384`
  - `loon cp ~/.codex/auth.json <vm>:/root/.codex/auth.json --mode 384`
  - Write `{"hasCompletedOnboarding": true}` to `<vm>:/root/.claude.json` or `claude -p` may enter the setup wizard.
- Non-interactive invocations that worked: `sh -lc 'export HOME=/root; cd /root; claude -p "..." --output-format text'` and `sh -lc 'export HOME=/root; cd /tmp; codex exec --skip-git-repo-check "..."'` (flag needed outside a git repo; codex falls back to a bundled bubblewrap — add `bubblewrap` to the image if the native sandbox is wanted).
- Live evidence: claude returned GUEST-OK, codex returned CODEX-OK, one call each on Casey's real subscriptions.
- `loon run` and `loon vm rm` leave a `destroyed` tombstone row; enumerate with the default (hidden) listing and prune periodically or after teardown.
- Timing: `vm create` returns fast, but the FIRST harness call is cold (node startup + auth-token refresh) — allow ~120s timeout on the first exec; a retry succeeded where the first call produced no output.

## Still open for the substrate rewrite (Asylum-side)

- Rewrite `substrate/loon.rs` against `loon run` / `loon vm create|stop|rm` / `loon exec` / `loon exec attach|signal` + `loon cp`; profile comes from `~/.config/loon/config.toml` (no LOON_* env contract).
- Loon node workspaces live INSIDE the guest (no host bind mounts): clone/provision repos in-guest.
- MCP + harness-event bridge from inside the guest must reach the daemon over HTTP with a token (`ASYLUM_BASE_URL`/`ASYLUM_TOKEN` — the W2 bridge already supports this); the unix socket does not cross the VM boundary. The daemon must listen on an address reachable from 10.42.0.0/16 guests and mint a per-node token.
- Interactive attach/observe for Loon nodes (`loon exec attach` PTY path) still needs Asylum-side wiring (observe/attach are Local-only today).
