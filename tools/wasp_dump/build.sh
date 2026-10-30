#!/usr/bin/env bash
# Build the canonical WASP oracle used by the ruhit conformance suite.
#
# WASP is not vendored into ruhit.  Point WASP_SOURCE_DIR at a checkout of the
# pinned commit (see tests/wasp/manifest.toml), or let this script clone it into
# target/wasp.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WASP_COMMIT="ce25dcde73ba81c70c330204ea5bf856762b95ac"
WASP_SOURCE_DIR="${WASP_SOURCE_DIR:-$ROOT/target/wasp}"
BUILD_DIR="${WASP_DUMP_BUILD_DIR:-$ROOT/target/wasp_dump}"
CXX="${CXX:-g++}"

if [[ ! -f "$WASP_SOURCE_DIR/wasphit/HITInterpreter.h" ]]; then
    mkdir -p "$(dirname "$WASP_SOURCE_DIR")"
    if [[ ! -d "$WASP_SOURCE_DIR/.git" ]]; then
        git init -q "$WASP_SOURCE_DIR"
        git -C "$WASP_SOURCE_DIR" remote add origin \
            https://github.com/ornl-neams-workbench/wasp.git
    fi
    git -C "$WASP_SOURCE_DIR" fetch --depth 1 origin "$WASP_COMMIT" -q
    git -C "$WASP_SOURCE_DIR" checkout -q FETCH_HEAD
fi

mkdir -p "$BUILD_DIR/include/waspcore" "$BUILD_DIR/include/waspson" "$BUILD_DIR/bin"

cat > "$BUILD_DIR/include/waspcore/version.h" <<'EOF'
#ifndef WASP_VERSION_H
#define WASP_VERSION_H
#include <string>
#define WASP_VERSION "conformance"
namespace wasp_version_info {
static std::string name = "wasp";
static std::string version = "conformance";
static std::string full_version = "conformance";
}
#endif
EOF

cat > "$BUILD_DIR/include/waspson/son_config.h" <<'EOF'
#ifndef son_config_h
#define son_config_h
#endif
EOF

SOURCES=(
    "$WASP_SOURCE_DIR/waspcore/Definition.cpp"
    "$WASP_SOURCE_DIR/waspcore/Interpreter.cpp"
    "$WASP_SOURCE_DIR/waspcore/utils.cpp"
    "$WASP_SOURCE_DIR/waspcore/Object.cpp"
    "$WASP_SOURCE_DIR/wasphit/HITLexer.cpp"
    "$WASP_SOURCE_DIR/wasphit/HITParser.cpp"
    "$WASP_SOURCE_DIR/wasphit/HITNodeView.cpp"
    "$WASP_SOURCE_DIR/waspson/SONLexer.cpp"
    "$WASP_SOURCE_DIR/waspson/SONParser.cpp"
    "$WASP_SOURCE_DIR/waspson/SONNodeView.cpp"
    "$WASP_SOURCE_DIR/waspsiren/SIRENLexer.cpp"
    "$WASP_SOURCE_DIR/waspsiren/SIRENParser.cpp"
    "$WASP_SOURCE_DIR/wasphive/HIVE.cpp"
)

"$CXX" -std=c++17 -O2 -DDISABLE_HIT_TYPE_PROMOTION=1 \
    -I"$WASP_SOURCE_DIR" \
    -I"$BUILD_DIR/include" \
    -I"$WASP_SOURCE_DIR/waspcore" \
    -I"$WASP_SOURCE_DIR/wasphit" \
    -I"$WASP_SOURCE_DIR/waspson" \
    -I"$WASP_SOURCE_DIR/waspsiren" \
    -I"$WASP_SOURCE_DIR/wasphive" \
    "$ROOT/tools/wasp_dump/main.cpp" \
    "${SOURCES[@]}" \
    -o "$BUILD_DIR/bin/wasp_dump"

echo "$BUILD_DIR/bin/wasp_dump"
