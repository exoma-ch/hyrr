#!/usr/bin/env bash
# check-mcp-registry.sh — keep py-mcp/server.json publishable to the official
# MCP Registry (#706).
#
# The registry listing is what lets an agent *find* hyrr-mcp. It is published
# once per release, after PyPI, by `release-hyrr-mcp.yml` — so any drift below
# surfaces only at tag time, as a failed or wrong listing, after the wheels
# are already public. This gate moves those failures to the PR.
#
# Fails if:
#   1. py-mcp/server.json is missing or malformed JSON.
#   2. `name` is not `io.github.<owner>/<server>`, where <owner> is the owner
#      in `repository.url`. CI authenticates with GitHub OIDC, which grants
#      exactly `io.github.<repository_owner>/*` — any other namespace is a 403.
#   3. `description` / `title` are empty or longer than the schema's 100 chars.
#   4. `version` or `packages[0].version` differs from py-mcp/pyproject.toml.
#      release-please bumps all three (release-please-config.json).
#   5. `packages[0]` is not the PyPI `hyrr-mcp` project over stdio.
#   6. The PyPI README (pyproject `readme`) does not carry
#      `mcp-name: <name>` followed by a boundary. The registry proves package
#      ownership by finding that token in the README of the exact version being
#      listed; without it the publish is rejected.
#
# Offline and stdlib-only (python3 >= 3.11 for tomllib), so it runs in prek and
# in CI's script-tests. Schema validation proper happens against the live
# registry (`mcp-publisher validate`) in the publish job.
#
# Usage:
#   scripts/check-mcp-registry.sh
#
# Run automatically by:
#   - prek (local git hook)
#   - ci.yml `script-tests` (via scripts/tests/test_check_mcp_registry.sh)
#   - release-hyrr-mcp.yml `publish-mcp-registry` job, before publishing

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

case "${1:-}" in
  -h|--help)
    sed -n '2,33p' "$0"
    exit 0
    ;;
esac

python3 - "$ROOT/py-mcp" <<'PY'
import json
import re
import sys
import tomllib
from pathlib import Path

pkg_dir = Path(sys.argv[1])
server_path = pkg_dir / "server.json"
pyproject_path = pkg_dir / "pyproject.toml"

errors = []

try:
    server = json.loads(server_path.read_text())
except FileNotFoundError:
    print(f"::error::{server_path} missing — the MCP Registry listing (#706) is published from it.", file=sys.stderr)
    sys.exit(1)
except json.JSONDecodeError as e:
    print(f"::error::{server_path} is malformed JSON: {e}", file=sys.stderr)
    sys.exit(1)

project = tomllib.loads(pyproject_path.read_text())["project"]

# --- 2. Namespace must be the one GitHub OIDC grants ---
name = server.get("name", "")
repo_url = server.get("repository", {}).get("url", "")
owner_match = re.fullmatch(r"https://github\.com/([^/]+)/[^/]+", repo_url)
if not owner_match:
    errors.append(f"repository.url `{repo_url}` must be https://github.com/<owner>/<repo>")
else:
    namespace = f"io.github.{owner_match.group(1)}/"
    if not re.fullmatch(re.escape(namespace) + r"[a-zA-Z0-9._-]+", name):
        errors.append(
            f"name `{name}` must be `{namespace}<server>` — CI publishes with GitHub OIDC, "
            f"which only grants `{namespace}*`"
        )

# --- 3. Schema length limits ---
for field, required in (("description", True), ("title", False)):
    value = server.get(field)
    if value is None and not required:
        continue
    if not isinstance(value, str) or not 1 <= len(value) <= 100:
        length = len(value) if isinstance(value, str) else "missing"
        errors.append(f"`{field}` must be 1-100 characters (is {length})")

# --- 4. Versions track pyproject ---
want_version = project["version"]
packages = server.get("packages") or [{}]
pkg = packages[0]
for where, got in (("version", server.get("version")), ("packages[0].version", pkg.get("version"))):
    if got != want_version:
        errors.append(
            f"server.json {where} is `{got}` but py-mcp/pyproject.toml is `{want_version}` — "
            "release-please-config.json must bump both"
        )

# --- 5. The package is the PyPI project, over stdio ---
if len(packages) != 1:
    errors.append(f"expected exactly one package entry, found {len(packages)}")
if pkg.get("registryType") != "pypi":
    errors.append(f"packages[0].registryType is `{pkg.get('registryType')}`, expected `pypi`")
if pkg.get("identifier") != project["name"]:
    errors.append(f"packages[0].identifier is `{pkg.get('identifier')}`, expected `{project['name']}`")
if pkg.get("transport", {}).get("type") != "stdio":
    errors.append("packages[0].transport.type must be `stdio` — hyrr-mcp speaks stdio JSON-RPC only")

# --- 6. Ownership token in the README PyPI will render ---
readme_path = pkg_dir / project.get("readme", "")
try:
    readme = readme_path.read_text()
except (FileNotFoundError, IsADirectoryError):
    errors.append(f"pyproject `readme` ({readme_path}) not found — it is the PyPI description the registry reads")
    readme = ""
# Exactly the registry's rule: the literal `mcp-name: <name>` (one space — the
# registry does not accept `mcp-name:<name>` or a double space), followed by
# whitespace, `<`, `-->`, or end of text.
if readme and not re.search(r"mcp-name: " + re.escape(name) + r"(?=\s|<|-->|$)", readme):
    errors.append(
        f"{readme_path.name} lacks `mcp-name: {name}` on its own line (e.g. `<!-- mcp-name: {name} -->`) — "
        "the registry rejects the publish without it"
    )

if errors:
    for e in errors:
        print(f"::error::{e}", file=sys.stderr)
    sys.exit(1)

print(f"py-mcp/server.json OK ({name} {want_version}).")
PY
