#!/usr/bin/env bash
# Generates Lean definitions for the `automata_builder` rust crate with
# Charon (rust -> LLBC) and Aeneas (LLBC -> Lean).
#
# Usage: ./scripts/generate_lean.sh
#
# Environment variables:
#   AENEAS_RELEASE  Aeneas release tag to use (the release bundles a matching
#                   Charon). Keep in sync with the `rev` in lean/lakefile.toml
#   AENEAS_DIR      Directory containing the `aeneas` and `charon` binaries.
#                   Defaults to .aeneas/$AENEAS_RELEASE (downloaded if missing)
set -euo pipefail

AENEAS_RELEASE="${AENEAS_RELEASE:-nightly-2026.10.30-de16f3c}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
AENEAS_DIR="${AENEAS_DIR:-$REPO_ROOT/.aeneas/$AENEAS_RELEASE}"
LEAN_DIR="$REPO_ROOT/lean"
LEAN_OUT_DIR="$LEAN_DIR/AutomataBuilder"
BUILD_DIR="$REPO_ROOT/target/aeneas"
LLBC_FILE="$BUILD_DIR/automata_builder.llbc"

cd "$REPO_ROOT"

# The lean dependency on the Aeneas lean library must match the release used
# to generate the definitions
release_commit="${AENEAS_RELEASE##*-}"
if ! grep -q "rev = \"$release_commit" "$LEAN_DIR/lakefile.toml"; then
    echo "error: lean/lakefile.toml does not pin aeneas to commit" \
        "$release_commit (from AENEAS_RELEASE=$AENEAS_RELEASE)" >&2
    exit 1
fi

# 1. Download the prebuilt Aeneas + Charon binaries
if [[ ! -x "$AENEAS_DIR/aeneas" || ! -x "$AENEAS_DIR/charon" ]]; then
    case "$(uname -s)-$(uname -m)" in
        Linux-x86_64) platform="linux-x86_64" ;;
        Linux-aarch64 | Linux-arm64) platform="linux-aarch64" ;;
        Darwin-arm64) platform="macos-aarch64" ;;
        *) echo "error: unsupported platform $(uname -s)-$(uname -m)" >&2; exit 1 ;;
    esac
    url="https://github.com/AeneasVerif/aeneas/releases/download/$AENEAS_RELEASE/aeneas-$platform.tar.gz"
    echo "Downloading $url"
    mkdir -p "$AENEAS_DIR"
    curl -fsSL "$url" | tar xz -C "$AENEAS_DIR"
fi

# 2. Install the nightly rust toolchain that Charon was built against
toolchain="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$AENEAS_DIR/rust-toolchain")"
if ! rustup component list --toolchain "$toolchain" --installed 2> /dev/null \
    | grep -q '^rustc-dev'; then
    rustup toolchain install "$toolchain" --profile minimal \
        --component rustc-dev,llvm-tools,rust-src,miri
fi

# 3. Rust -> LLBC with Charon. Only the pure rust modules are translated: the
#    pyo3 bindings (`py_*` modules) and serde derives are out of scope
start_from=()
for module_file in src/automata/*.rs; do
    module="$(basename "$module_file" .rs)"
    if [[ "$module" == "mod" || "$module" == py_* ]]; then
        continue
    fi
    start_from+=(--start-from "crate::automata::$module")
done

mkdir -p "$BUILD_DIR"
# - `-Wl,--undefined-version`: Charon does not generate code, so the pyo3
#   `PyInit__rust` symbol exported by the cdylib does not exist at link time
# - `Iterator::{copied,cloned}` signatures crash Aeneas, so they are excluded
# - rayon's parallel iterator traits are mutually recursive, which Aeneas does
#   not support, so rayon is excluded (functions using it become axioms)
"$AENEAS_DIR/charon" cargo --preset=aeneas \
    --rustc-arg=-Clink-arg=-Wl,--undefined-version \
    "${start_from[@]}" \
    --exclude '{impl serde_core::ser::Serialize for _}' \
    --exclude '{impl serde_core::de::Deserialize for _}' \
    --exclude 'core::iter::traits::iterator::Iterator::copied' \
    --exclude 'core::iter::traits::iterator::Iterator::cloned' \
    --exclude 'rayon' \
    --dest-file "$LLBC_FILE" \
    -- --lib

# 4. LLBC -> Lean with Aeneas
mkdir -p "$LEAN_OUT_DIR"
aeneas_log="$BUILD_DIR/aeneas.log"
set +e
"$AENEAS_DIR/aeneas" -backend lean -split-files -no-progress-bar -filter-trait-methods \
    -dest "$LEAN_OUT_DIR" "$LLBC_FILE" > "$aeneas_log" 2>&1
aeneas_status=$?
set -e
# Aeneas exits with 1 when some definitions could not be translated (they are
# then emitted as axioms) - anything else is a fatal error
if [[ $aeneas_status -ne 0 && $aeneas_status -ne 1 ]]; then
    cat "$aeneas_log" >&2
    echo "error: aeneas failed with exit code $aeneas_status" >&2
    exit "$aeneas_status"
fi
num_errors="$(sed 's/\x1b\[[0-9;]*m//g' "$aeneas_log" | grep -c '^\[Error\]' || true)"
echo "Aeneas reported $num_errors errors (see $aeneas_log)"

# Hand-written models of external definitions are kept in *External.lean.
# Seed them from the generated templates the first time only
for kind in Types Funs; do
    if [[ ! -f "$LEAN_OUT_DIR/${kind}External.lean" ]]; then
        cp "$LEAN_OUT_DIR/${kind}External_Template.lean" \
            "$LEAN_OUT_DIR/${kind}External.lean"
    fi
done

echo "Lean definitions written to $LEAN_OUT_DIR"
