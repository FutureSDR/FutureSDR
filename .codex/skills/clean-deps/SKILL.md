---
name: clean-deps
description: Audit Rust dependencies across the FutureSDR repository by checking every Cargo.toml for unused dependencies, unnecessary enabled features, and stale optional flags, then remove only what is proven unnecessary and validate the repo afterwards. Use this when the user asks to clean up Cargo manifests, trim features, or remove dependency bloat.
---

# Clean Deps

## Overview

Use this skill when pruning Rust dependencies in this repository. The goal is to remove only dependencies and feature flags that are demonstrably unnecessary, without changing behavior or weakening version specificity.

## Repo Rules

- Work from the repo root: `/home/basti/src/futuresdr`.
- Enumerate manifests with `scripts/find-cargo-tomls.sh`. This repo has one main workspace plus many independent `examples/*` and `perf/*` crates.
- Inspect all dependency sections that can matter:
  - `[dependencies]`
  - `[dev-dependencies]`
  - `[build-dependencies]`
  - target-specific dependency tables
  - `[features]`
  - `[patch.crates-io]`
- Do not broaden version requirements or normalize manifest style. Keep remaining entries as specific as they already are.
- Do not remove a dependency or feature just because it looks unused in one crate. Prove it from code, feature wiring, and validation.
- Be careful with proc-macro, build-script, example-only, bench-only, test-only, wasm-only, and platform-specific dependencies.

## Workflow

1. Inventory manifests.
   - Run `scripts/find-cargo-tomls.sh`.
   - Read the root [`Cargo.toml`](../../../../Cargo.toml) first, then the manifests you plan to edit.
2. Find candidates.
   - Check whether each dependency name is referenced in the owning crate’s source, tests, benches, examples, build script, and feature wiring.
   - Look for feature flags enabled on dependencies that are no longer needed.
   - Watch for optional dependencies that are no longer referenced by crate features.
3. Prove removal safety.
   - Use `rg` to inspect symbol use, crate-path imports, feature names, and cfg gates.
   - When a dependency might be implicit, check:
     - `build.rs`
     - `tests/`
     - `benches/`
     - binaries under `src/bin/`
     - example crates and perf crates
4. Remove the smallest safe set.
   - Prefer targeted deletions over broad manifest rewrites.
   - If several crates repeat the same dead dependency pattern, clean them in a coherent batch.
5. Validate.
   - Run focused checks first for edited crates.
   - Finish with `./check.sh`.

## What Counts As Evidence

Acceptable reasons to remove a dependency or feature:

- No code path, feature, build script, test, bench, or binary in that crate uses it.
- The feature list contains an entry that is redundant because it is already defaulted or implied by another enabled feature and dropping it does not change behavior.
- An optional dependency is no longer reachable from any crate feature or code path.

Reasons that are not enough on their own:

- "It looks unused."
- "cargo tree is smaller without it."
- "Another crate already depends on it."

## Good Search Patterns

Use `rg` in the owning crate before editing. Typical patterns:

```bash
rg -n "crate_name|feature_name|TypeName|fn_name" src tests benches examples build.rs
rg -n "\\[features\\]|dep:crate_name|crate_name/" Cargo.toml
```

For repo-wide verification after pruning a shared pattern:

```bash
rg -n "crate_name|feature_name" . -g '!target'
```

## Validation Targets

Use narrower checks while iterating, then the full repo gate:

```bash
cargo check --lib --workspace --features=burn,audio,seify_dummy,wgpu
cargo clippy --lib --workspace --features=burn,audio,seify_dummy,wgpu --target=wasm32-unknown-unknown -- -D warnings
./check.sh
```

If you edit a leaf example or perf crate, run a local `cargo check` or `cargo clippy` there first.

## Output Expectations

When reporting back:

- List which dependencies or enabled features were removed.
- Call out anything suspicious that you deliberately kept because the evidence was inconclusive.
- State whether `./check.sh` passed.
