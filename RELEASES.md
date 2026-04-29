# Release Ledger

Asylum is **released manually**. We do not auto-cut releases on every commit, and there is no GitHub Actions release pipeline. Releases are deliberate, cut at delivery boundaries, and tracked here so "merged to main" never gets confused with "shipped to users."

## Who cuts a release

The user. Not the agent — unless the user explicitly authorized autonomous mode for the delivery in question.

An agent finishing a delivery should:
- update the ledger row + the plan/handoff "Release status" section so the state is **legible**;
- if the agent thinks a cut is warranted, **recommend it** to the user (one line: "recommend cutting vX.Y.Z");
- otherwise leave it. The user's manual gate is the design, not friction.

When the user (or an authorized autonomous agent) decides to cut:

- **After a delivery cycle.** A planned multi-PR initiative (audit/plan in `docs/reviews/` or `docs/handoff/`) is delivered → typically a release follows.
- **For a user-facing fix worth waiting one cycle on.** Bug fix or small capability addition that's worth getting onto machines.
- **Not for trivial doc-only changes.** README typo, plan-progress checkbox, internal note → skip; folds into the next real release.

## Workflow (manual, by an agent or human)

1. Bump version in `Cargo.toml` (workspace) and `cockpit/package.json`.
2. Update `CHANGELOG.md` with a new top section for the version + date.
3. `cargo update -w` (sync `Cargo.lock`).
4. Commit: `release: bump to vX.Y.Z` (and any release-tooling fixes).
5. Push `main`.
6. `bash scripts/build-release-artifacts.sh --version X.Y.Z --targets <available>` — see "Build host limitations" below.
7. `git tag -a vX.Y.Z -m "asylum vX.Y.Z — <one-line>"`.
8. `git push origin vX.Y.Z`.
9. `bash scripts/publish-release.sh --version X.Y.Z --targets <same as build>`.
10. Update this ledger (move row to "Published" with the platforms + asset URL).
11. If platforms are partial: arrange a follow-up build on the missing host (Mac for darwin-*; arm64 Linux for linux-arm64) and re-run publish with `--allow-clobber`.

## Build host capabilities

The two supported build hosts are **macOS Apple Silicon** and **Linux x86_64**. Both can produce a complete release with no host-side toolchain installs beyond the standard ones (Rust, Node, Docker on Linux):

| Platform | Apple Silicon Mac | Linux x86_64 |
|---|---|---|
| `darwin-arm64` | ✅ native cargo | ✅ Docker (`ghcr.io/rust-cross/cargo-zigbuild`; SDK baked in) |
| `darwin-x86_64` | ✅ rustup cross | ✅ Docker (same image) |
| `linux-arm64`  | ✅ native Docker (`--platform linux/arm64`) | ✅ Docker (same image; zig cross) |
| `linux-x86_64` | ✅ cross-compile inside an arm64 Linux container | ✅ native cargo (no Docker) |

**`build-release-artifacts.sh`** auto-detects host arch and routes to the appropriate path. On Linux x86_64, all three cross targets share a single Docker image (`ghcr.io/rust-cross/cargo-zigbuild`) — no QEMU/binfmt setup, no osxcross, no Apple SDK download. The image bundles zig (used as the linker), `cargo-zigbuild`, the Rust toolchain, and the macOS SDK.

**For a partial-platform cut today, full parity tomorrow:** build what the current host can; publish with `--targets <subset>`; later, on the missing-platform host, build the rest and publish again with `--targets <new subset> --allow-clobber` to merge them into the same release.

## Ledger

| Version | Tag | Date | main commit | Status | Platforms shipped | Notes |
|---|---|---|---|---|---|---|
| 0.1.2 | v0.1.2 | 2026-04-29 | 922e951 | **Published** | linux-x86_64, linux-arm64, darwin-arm64, darwin-x86_64 | Cuts the cockpit deliverability + prototype-residue cleanup (40+ commits, 7 PRs). All four platforms cross-built from Linux x86_64 via `ghcr.io/rust-cross/cargo-zigbuild`. |
| 0.1.1 | v0.1.1 | 2026-04-28 | (pre-cleanup) | Published | linux-x86_64, linux-arm64, darwin-arm64, darwin-x86_64 | Initial cleanup of 9 ultrareview High findings. |

## Open release-side action items

- [ ] **minisign trust path** — paste a real signing key into `ASYLUM_RELEASE_PUBKEY_DEFAULT` in `scripts/install.sh` and set `ASYLUM_RELEASE_SIGNING_KEY` for `publish-release.sh`. Until then, signature verification is skipped.

## Where this ledger is referenced

- [AGENTS.md](AGENTS.md) — release tracking convention for agents
- Delivery handoffs in `docs/handoff/*.md` — each delivery's "Release status" section links here
