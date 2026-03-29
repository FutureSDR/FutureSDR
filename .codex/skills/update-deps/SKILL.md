---
name: update-deps
description: Update Rust dependencies across the FutureSDR repository by touching every Cargo.toml that matters, keeping version requirements as specific as they already are, ignoring the current MSRV when selecting newer releases, raising `rust-version` as needed, and fixing compile, clippy, wasm, and test breakages caused by upstream API changes. Use this when the user asks to bump deps, refresh Cargo manifests, or adapt the repo to newer crate APIs.
---

# Update Deps

## Overview

Use this skill when updating Rust dependencies in this repository. The job is not finished when versions change in `Cargo.toml`; you must also adapt the code to API changes, raise the repo MSRV when newer releases require it, and run the repo validation flow until it is clean.

## Repo Rules

- Work from the repo root: `/home/basti/src/futuresdr`.
- Inspect the root [`Cargo.toml`](../../../../Cargo.toml) first for `rust-version`, shared features, target-specific dependencies, and workspace members.
- Treat `rust-version` as movable. Do not let the current MSRV force dependency downgrades or block upgrades to the most recent stable release line.
- Enumerate manifests with `scripts/find-cargo-tomls.sh`. Do not assume only workspace members matter; this repo also has independent `examples/*` and `perf/*` crates.
- Keep version requirements as specific as they already are.
  - If a dependency is `0.17`, update to the new `major.minor` form, not `"0"` or `"^0.18"`.
  - If a dependency is pinned to `0.0.13`, keep that patch-level specificity.
  - Preserve existing table style, `features`, `default-features`, `optional`, `path`, and target-specific sections.
- Prefer the most recent stable release line for each dependency family.
  - Use `cargo upgrade --ignore-rust-version` when surveying or applying upgrades.
  - If the selected releases require a newer Rust toolchain, update the root `rust-version` to the minimum needed by the chosen dependency set.
  - Do not keep an older crate version just because it matches the previous MSRV.
- Do not introduce `[workspace.dependencies]`, broaden requirements, or normalize formatting beyond what `cargo fmt` or local style already does unless the user asks for that refactor.
- Check for local overrides such as `[patch.crates-io]` in leaf manifests before changing versions.

## Workflow

1. Inventory manifests.
   - Run `scripts/find-cargo-tomls.sh`.
   - Read the root manifest plus any leaf manifests that own the dependencies you intend to change.
2. Plan the update surface.
   - Group related crates together, for example `wgpu`, `burn`, audio, async, or GUI stacks.
   - Prefer coherent upgrades over random one-off bumps inside the same dependency family.
   - Identify the Rust version implied by the target upgrades and plan to raise `rust-version` early if needed.
3. Update manifest entries.
   - Edit only the version numbers or dependency tables that need to change.
   - Preserve the original specificity and structure of each entry.
   - Update the root `rust-version` when required by the selected dependency versions.
4. Compile and adapt.
   - Use targeted `cargo check` or `cargo clippy` runs first near the changed area.
   - Fix API changes in source code, examples, and perf crates, not just the library crate.
   - Use `rg` on compiler error symbols and renamed methods/types to find all affected call sites.
5. Run full validation.
   - Always finish with `./check.sh`.
   - Keep iterating until it exits successfully.

## Validation Targets

Use focused validation while iterating, then the full repo check:

```bash
cargo check --lib --workspace --features=burn,audio,seify_dummy,wgpu
cargo clippy --lib --workspace --features=burn,audio,seify_dummy,wgpu --target=wasm32-unknown-unknown -- -D warnings
./check.sh
```

Prefer narrower commands first when a dependency family only affects one crate, but `./check.sh` is the final gate.

When using `cargo upgrade`, prefer:

```bash
cargo upgrade --ignore-rust-version
```

## API Change Triage

- Read the actual compiler or clippy errors before patching.
- Update all downstream call sites for renamed methods, constructor signature changes, trait-bound changes, and feature-gated API moves.
- If an upgrade forces a newer Rust version, treat that as a normal part of the update and adapt the repo accordingly.
- Expect fallout in:
  - `src/`
  - `examples/*`
  - `perf/*`
  - wasm-specific builds
- If a dependency upgrade requires a semantic redesign instead of a local patch, stop and explain the tradeoff before forcing a risky rewrite.

## Output Expectations

When reporting back:

- Name the dependency groups that changed.
- Call out any `rust-version` bump and why it was necessary.
- Call out any intentionally skipped upgrades and why.
- State whether `./check.sh` passed.
