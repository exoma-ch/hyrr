#!/usr/bin/env bash
# Smoke test for scripts/check-mcp-registry.sh (#706 MCP Registry listing).
#
# Each mutation below is a way the tag-time registry publish would fail — or
# worse, succeed with a wrong listing — after the wheels are already on PyPI.
# The gate must catch every one of them at PR time.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
GATE="$SCRIPT_DIR/check-mcp-registry.sh"

failed=0
report() {
  local name="$1" status="$2"
  if [ "$status" = "pass" ]; then
    echo "PASS: $name"
  else
    echo "FAIL: $name" >&2
    failed=1
  fi
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

setup_repo() {
  local dst="$work/$1"
  rm -rf "$dst"
  mkdir -p "$dst/scripts" "$dst/py-mcp"
  cp "$GATE" "$dst/scripts/"
  cp "$ROOT/py-mcp/server.json" "$ROOT/py-mcp/pyproject.toml" "$ROOT/py-mcp/README.md" "$dst/py-mcp/"
  echo "$dst"
}

# Apply a Python mutation to server.json: `s` is the parsed document.
mutate_server() {
  python3 - "$1/py-mcp/server.json" "$2" <<'PY'
import json, sys
path, code = sys.argv[1], sys.argv[2]
s = json.load(open(path))
exec(code)
json.dump(s, open(path, "w"), indent=2)
PY
}

expect_pass() {
  local name="$1" dir="$2"
  if (cd "$dir" && bash scripts/check-mcp-registry.sh >/dev/null 2>&1); then
    report "$name" pass
  else
    report "$name" fail
  fi
}

expect_fail() {
  local name="$1" dir="$2"
  if ! (cd "$dir" && bash scripts/check-mcp-registry.sh >/dev/null 2>&1); then
    report "$name" pass
  else
    report "$name" fail
  fi
}

expect_pass "real server.json passes" "$(setup_repo happy)"

d="$(setup_repo missing)"
rm "$d/py-mcp/server.json"
expect_fail "missing server.json fails" "$d"

d="$(setup_repo malformed)"
echo "{not json" > "$d/py-mcp/server.json"
expect_fail "malformed JSON fails" "$d"

d="$(setup_repo foreign_ns)"
mutate_server "$d" 's["name"] = "io.github.gerchowl/hyrr"'
expect_fail "namespace outside the OIDC grant fails" "$d"

d="$(setup_repo long_desc)"
mutate_server "$d" 's["description"] = "x" * 101'
expect_fail "description over 100 chars fails" "$d"

d="$(setup_repo stale_version)"
mutate_server "$d" 's["version"] = "0.0.1"'
expect_fail "top-level version drift fails" "$d"

d="$(setup_repo stale_pkg_version)"
mutate_server "$d" 's["packages"][0]["version"] = "0.0.1"'
expect_fail "package version drift fails" "$d"

d="$(setup_repo wrong_ident)"
mutate_server "$d" 's["packages"][0]["identifier"] = "hyrr"'
expect_fail "identifier not the PyPI project fails" "$d"

d="$(setup_repo wrong_transport)"
mutate_server "$d" 's["packages"][0]["transport"] = {"type": "streamable-http", "url": "https://x"}'
expect_fail "non-stdio transport fails" "$d"

d="$(setup_repo no_marker)"
sed -i '/mcp-name:/d' "$d/py-mcp/README.md"
expect_fail "README without mcp-name token fails" "$d"

d="$(setup_repo glued_marker)"
sed -i 's|mcp-name: io.github.exoma-ch/hyrr -->|mcp-name: io.github.exoma-ch/hyrr.|' "$d/py-mcp/README.md"
expect_fail "mcp-name glued to trailing punctuation fails" "$d"

d="$(setup_repo no_space_marker)"
sed -i 's|mcp-name: io.github.exoma-ch/hyrr|mcp-name:io.github.exoma-ch/hyrr|' "$d/py-mcp/README.md"
expect_fail "mcp-name without its single space fails (registry matches it literally)" "$d"

d="$(setup_repo double_space_marker)"
sed -i 's|mcp-name: io.github.exoma-ch/hyrr|mcp-name:  io.github.exoma-ch/hyrr|' "$d/py-mcp/README.md"
expect_fail "mcp-name with a double space fails" "$d"

d="$(setup_repo renamed)"
mutate_server "$d" 's["name"] = "io.github.exoma-ch/hyrr-mcp"'
expect_fail "server renamed without updating README token fails" "$d"

if [ "$failed" -ne 0 ]; then
  echo "check-mcp-registry.sh smoke test FAILED" >&2
  exit 1
fi
echo "check-mcp-registry.sh smoke test passed."
