# Release Ledger

Asylum is **released manually**. We do not auto-cut releases on every commit, and there is no GitHub Actions release pipeline. Releases are deliberate, cut at delivery boundaries, and tracked here so "merged to main" never gets confused with "shipped to users."

## When to cut a release

- **After a delivery cycle.** A planned multi-PR initiative (an audit/plan in `docs/reviews/` or `docs/handoff/`) is delivered → cut a release as the last step. Tracking docs include a release-status section that's red until the ledger below is updated.
- **For a user-facing fix worth waiting one cycle on.** If a bug fix or capability addition has accumulated on `main` and is worth getting onto user machines, cut a release.
- **Not for trivial doc-only changes.** README typo, plan-progress checkbox, internal note → no release needed.

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

## Build host limitations

| Platform | Buildable from |
|---|---|
| `linux-x86_64` | Native on x86_64 Linux (no Docker needed) **OR** cross-compiled from arm64 Linux container on Apple Silicon |
| `linux-arm64` | Native on arm64 Linux **OR** Docker+QEMU on x86_64 Linux (requires `qemu-user-static` + `binfmt-support`; `sudo apt install` once per host) |
| `darwin-arm64` | macOS with Apple Silicon (or any macOS via `rustup target add aarch64-apple-darwin`) |
| `darwin-x86_64` | macOS with `rustup target add x86_64-apple-darwin` |

## Ledger

| Version | Tag | Date | main commit | Status | Platforms shipped | Notes |
|---|---|---|---|---|---|---|
| 0.1.2 | v0.1.2 | 2026-04-29 | 922e951 | **Published** | linux-x86_64 | macOS + linux-arm64 archives outstanding — need a Mac and (optionally) an arm64 Linux build host. Re-run `publish-release.sh --version 0.1.2 --targets darwin-arm64,darwin-x86_64,linux-arm64 --allow-clobber` after building those. Cuts the cockpit deliverability + prototype-residue cleanup (40+ commits, 7 PRs). |
| 0.1.1 | v0.1.1 | 2026-04-28 | (pre-cleanup) | Published | linux-x86_64, linux-arm64, darwin-arm64, darwin-x86_64 | Initial cleanup of 9 ultrareview High findings. |

## Open release-side action items

- [ ] **v0.1.2 platform parity** — build and re-publish darwin-arm64, darwin-x86_64, linux-arm64 archives once a Mac/arm64 build host is available.
- [ ] **minisign trust path** — paste a real signing key into `ASYLUM_RELEASE_PUBKEY_DEFAULT` in `scripts/install.sh` and set `ASYLUM_RELEASE_SIGNING_KEY` for `publish-release.sh`. Until then, signature verification is skipped.

## Where this ledger is referenced

- [AGENTS.md](AGENTS.md) — release tracking convention for agents
- Delivery handoffs in `docs/handoff/*.md` — each delivery's "Release status" section links here
