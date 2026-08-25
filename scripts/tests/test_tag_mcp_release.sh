#!/usr/bin/env bash
# Tests for scripts/tag-mcp-release.sh (#676), fully offline.
#
# `gh` is stubbed onto PATH and answers out of a per-case fixture directory, so
# every branch can be forced — including the ones that are, by construction,
# invisible in production: a POST that returns 201 while the ref never appears,
# and a tag that exists but triggers nothing.
#
# `git` is stubbed too, as a tripwire. The bug in #676 was a `git push` of the
# tag being rejected by GitHub's App workflow-file check; the fix is to stop
# pushing a tree at all. If the script ever shells out to git again, the stub
# records it and the last test fails.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TAGGER="$SCRIPT_DIR/tag-mcp-release.sh"

failed=0
report() {
  if [ "$2" = "pass" ]; then echo "PASS: $1"; else echo "FAIL: $1" >&2; failed=1; fi
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

bin="$work/bin"
mkdir -p "$bin"

# Stub `gh`. State lives in $FIXTURES:
#   refs/<tag>            -> the sha (prefixed "tag:" for an annotated object)
#   tagobj/<sha>          -> the commit sha an annotated tag dereferences to
#   runs.json             -> the release-hyrr-mcp.yml run list
#   latest.json           -> the /releases/latest payload
#   deny_create           -> POST /git/refs fails
#   phantom_create        -> POST /git/refs returns 201 but records nothing
#   post.log              -> every POST, appended
cat > "$bin/gh" <<'GH'
#!/usr/bin/env bash
[ "$1" = "api" ] || exit 1
shift

method=GET
path=""
declare -A fields=()
while [ $# -gt 0 ]; do
  case "$1" in
    -X) method="$2"; shift 2 ;;
    -f) fields["${2%%=*}"]="${2#*=}"; shift 2 ;;
    -*) shift ;;
    *) [ -z "$path" ] && path="$1"; shift ;;
  esac
done

if [ "$method" = "POST" ]; then
  case "$path" in
    */git/refs)
      ref="${fields[ref]:-}"
      sha="${fields[sha]:-}"
      echo "POST $path $ref $sha" >> "$FIXTURES/post.log"
      case "$ref" in
        refs/tags/preflight-676-*)
          [ -f "$FIXTURES/deny_workflow_ref" ] && {
            echo 'gh: Resource not accessible by integration (HTTP 403)'
            exit 1
          }
          [ -f "$FIXTURES/break_workflow_ref" ] && {
            echo 'gh: connection reset by peer'
            exit 1
          }
          ;;
        *) [ -f "$FIXTURES/deny_create" ] && {
          echo 'gh: Resource not accessible by integration (HTTP 403)'
          exit 1
        } ;;
      esac
      if [ ! -f "$FIXTURES/phantom_create" ]; then
        mkdir -p "$FIXTURES/refs"
        printf '%s\n' "$sha" > "$FIXTURES/refs/${ref##*/}"
      fi
      printf '{"ref":"%s","object":{"sha":"%s","type":"commit"}}\n' "$ref" "$sha"
      exit 0
      ;;
    *) exit 1 ;;
  esac
  exit 0
fi

if [ "$method" = "DELETE" ]; then
  echo "DELETE $path" >> "$FIXTURES/delete.log"
  case "$path" in
    */refs/tags/*) rm -f "$FIXTURES/refs/${path##*/}" ;;
  esac
  exit 0
fi

case "$path" in
  # preflight: the most recent commit touching .github/workflows/, whose
  # parent is the commit it aims at.
  *"commits?path=.github/workflows"*)
    if [ -f "$FIXTURES/no_workflow_history" ]; then
      echo '[]'
    else
      printf '[{"sha":"%s","parents":[{"sha":"%s"}]}]\n' "$TIP_SHA" "$REL_SHA_ENV"
    fi
    ;;
  */git/ref/tags/*)
    tag="${path##*/git/ref/tags/}"
    [ -f "$FIXTURES/refs/$tag" ] || exit 1
    val="$(cat "$FIXTURES/refs/$tag")"
    if [ "${val#tag:}" != "$val" ]; then
      printf '{"object":{"sha":"%s","type":"tag"}}\n' "${val#tag:}"
    else
      printf '{"object":{"sha":"%s","type":"commit"}}\n' "$val"
    fi
    ;;
  */git/tags/*)
    sha="${path##*/git/tags/}"
    [ -f "$FIXTURES/tagobj/$sha" ] || exit 1
    printf '{"object":{"sha":"%s","type":"commit"}}\n' "$(cat "$FIXTURES/tagobj/$sha")"
    ;;
  */actions/workflows/*/runs*)
    cat "$FIXTURES/runs.json" 2>/dev/null || echo '{"workflow_runs":[]}'
    ;;
  */releases/latest)
    cat "$FIXTURES/latest.json" 2>/dev/null || exit 1
    ;;
  *) exit 1 ;;
esac
GH

# Tripwire: the fix is that the tag is never pushed with git.
cat > "$bin/git" <<'GIT'
#!/usr/bin/env bash
echo "git $*" >> "$FIXTURES/git.log"
exit 0
GIT

chmod +x "$bin/gh" "$bin/git"
export PATH="$bin:$PATH"
export GITHUB_REPOSITORY="exoma-ch/hyrr"
export TAG_MCP_POLL_INTERVAL=0

REL_SHA="9fd188a929073d0cd76bf2b3f202ea5aaac0f5aa"
OTHER_SHA="aedc4d3bf058dded721fffc0825c02d75a09a71a"
TIP_SHA="$OTHER_SHA"
REL_SHA_ENV="$REL_SHA"
export TIP_SHA REL_SHA_ENV

# new_case <name> — fresh fixture dir, exported as $FIXTURES
new_case() {
  FIXTURES="$work/case-$1"
  export FIXTURES
  mkdir -p "$FIXTURES/refs" "$FIXTURES/tagobj"
}

put_ref() { printf '%s\n' "$2" > "$FIXTURES/refs/$1"; }
triggered() { printf '{"workflow_runs":[{"head_branch":"%s"}]}\n' "$1" > "$FIXTURES/runs.json"; }
run_tagger() { bash "$TAGGER" "$@" > "$FIXTURES/out.txt" 2>&1; }

# ── create: the happy path ────────────────────────────────────────────────
new_case create-happy
put_ref v0.21.0 "$REL_SHA"
triggered "hyrr-mcp-v0.21.0"
if run_tagger create v0.21.0 \
  && [ "$(cat "$FIXTURES/refs/hyrr-mcp-v0.21.0")" = "$REL_SHA" ] \
  && grep -q 'refs/tags/hyrr-mcp-v0.21.0' "$FIXTURES/post.log"; then
  report "create makes hyrr-mcp-v* at the v* tag's commit, via POST /git/refs" pass
else
  report "create makes hyrr-mcp-v* at the v* tag's commit, via POST /git/refs" fail
  cat "$FIXTURES/out.txt"
fi

# ── create pins to the RELEASE commit, never to whatever main is ──────────
# The 0.21.0 incident tagged main's tip (an unrelated PR) because the checkout
# had no `ref:`. The script only ever reads the v* tag, so there is no path by
# which $OTHER_SHA can end up on the MCP tag.
new_case create-pins-release-sha
put_ref v0.21.0 "$REL_SHA"
put_ref main "$OTHER_SHA" # present, and must be ignored
triggered "hyrr-mcp-v0.21.0"
run_tagger create v0.21.0
if [ "$(cat "$FIXTURES/refs/hyrr-mcp-v0.21.0" 2>/dev/null)" = "$REL_SHA" ]; then
  report "create pins the MCP tag to the release commit, not to main's tip" pass
else
  report "create pins the MCP tag to the release commit, not to main's tip" fail
fi

# ── create: no v* tag to anchor to ────────────────────────────────────────
new_case create-no-vtag
if ! run_tagger create v9.9.9 && grep -q 'refusing to guess' "$FIXTURES/out.txt"; then
  report "create refuses (loudly) when the v* tag does not exist" pass
else
  report "create refuses (loudly) when the v* tag does not exist" fail
fi

# ── create: idempotent re-run ─────────────────────────────────────────────
new_case create-idempotent
put_ref v0.21.0 "$REL_SHA"
put_ref hyrr-mcp-v0.21.0 "$REL_SHA"
triggered "hyrr-mcp-v0.21.0"
if run_tagger create v0.21.0 && [ ! -f "$FIXTURES/post.log" ]; then
  report "create is idempotent — an existing correct tag is not re-POSTed" pass
else
  report "create is idempotent — an existing correct tag is not re-POSTed" fail
  cat "$FIXTURES/out.txt"
fi

# ── create: the MCP tag already exists, on the wrong commit ───────────────
new_case create-wrong-commit
put_ref v0.21.0 "$REL_SHA"
put_ref hyrr-mcp-v0.21.0 "$OTHER_SHA"
if ! run_tagger create v0.21.0 && grep -q 'refusing to move' "$FIXTURES/out.txt"; then
  report "create fails when the MCP tag already names a different commit" pass
else
  report "create fails when the MCP tag already names a different commit" fail
fi

# ── create: the API refuses ───────────────────────────────────────────────
new_case create-denied
put_ref v0.21.0 "$REL_SHA"
: > "$FIXTURES/deny_create"
if ! run_tagger create v0.21.0 \
  && grep -q '::error::' "$FIXTURES/out.txt" \
  && grep -q 'will NOT publish' "$FIXTURES/out.txt"; then
  report "create fails loudly when the ref cannot be created" pass
else
  report "create fails loudly when the ref cannot be created" fail
fi

# ── create: the 201 that lied ─────────────────────────────────────────────
# The whole point of the read-back. This is #676's failure class: the call
# reports success and the tag is not there.
new_case create-phantom
put_ref v0.21.0 "$REL_SHA"
: > "$FIXTURES/phantom_create"
if ! run_tagger create v0.21.0 && grep -q 'is missing' "$FIXTURES/out.txt"; then
  report "create fails when the API reports success but the tag is absent" pass
else
  report "create fails when the API reports success but the tag is absent" fail
fi

# ── assert-trigger: the tag exists but fires nothing ──────────────────────
# A tag that does not fire release-hyrr-mcp.yml is the same bug wearing a
# different hat (#571, ADR 0002) — so it must be red, not green.
new_case trigger-absent
put_ref v0.21.0 "$REL_SHA"
put_ref hyrr-mcp-v0.21.0 "$REL_SHA"
if ! run_tagger assert-trigger v0.21.0 --wait 1 \
  && grep -q 'did not trigger' "$FIXTURES/out.txt"; then
  report "assert-trigger fails when the tag triggered no publish run" pass
else
  report "assert-trigger fails when the tag triggered no publish run" fail
fi

# A run for a DIFFERENT tag must not be mistaken for this one's.
new_case trigger-other-tag
triggered "hyrr-mcp-v0.20.1"
if ! run_tagger assert-trigger v0.21.0 --wait 1 \
  && grep -q 'did not trigger' "$FIXTURES/out.txt"; then
  report "assert-trigger does not accept a run belonging to another tag" pass
else
  report "assert-trigger does not accept a run belonging to another tag" fail
fi

new_case trigger-present
triggered "hyrr-mcp-v0.21.0"
if run_tagger assert-trigger v0.21.0 --wait 1; then
  report "assert-trigger passes once a run exists for the tag" pass
else
  report "assert-trigger passes once a run exists for the tag" fail
  cat "$FIXTURES/out.txt"
fi

# ── annotated v* tags are dereferenced ────────────────────────────────────
new_case create-annotated
put_ref v0.21.0 "tag:deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
printf '%s\n' "$REL_SHA" > "$FIXTURES/tagobj/deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
triggered "hyrr-mcp-v0.21.0"
run_tagger create v0.21.0
if [ "$(cat "$FIXTURES/refs/hyrr-mcp-v0.21.0" 2>/dev/null)" = "$REL_SHA" ]; then
  report "an annotated v* tag is dereferenced to its commit" pass
else
  report "an annotated v* tag is dereferenced to its commit" fail
  cat "$FIXTURES/out.txt"
fi

# ── assert --latest ───────────────────────────────────────────────────────
new_case assert-latest-ok
echo '{"tag_name":"v0.21.0"}' > "$FIXTURES/latest.json"
put_ref v0.21.0 "$REL_SHA"
put_ref hyrr-mcp-v0.21.0 "$REL_SHA"
if run_tagger assert --latest; then
  report "assert --latest passes when both tags name the release commit" pass
else
  report "assert --latest passes when both tags name the release commit" fail
  cat "$FIXTURES/out.txt"
fi

# This is the state 0.21.0 was actually in for ~14 hours, while two separate
# release-please runs reported success.
new_case assert-latest-missing
echo '{"tag_name":"v0.21.0"}' > "$FIXTURES/latest.json"
put_ref v0.21.0 "$REL_SHA"
if ! run_tagger assert --latest \
  && grep -q 'build but never publish' "$FIXTURES/out.txt"; then
  report "assert --latest catches a released version with no MCP tag" pass
else
  report "assert --latest catches a released version with no MCP tag" fail
fi

new_case assert-latest-skewed
echo '{"tag_name":"v0.21.0"}' > "$FIXTURES/latest.json"
put_ref v0.21.0 "$REL_SHA"
put_ref hyrr-mcp-v0.21.0 "$OTHER_SHA"
if ! run_tagger assert --latest && grep -q 'never released' "$FIXTURES/out.txt"; then
  report "assert --latest catches an MCP tag on the wrong commit" pass
else
  report "assert --latest catches an MCP tag on the wrong commit" fail
fi

new_case assert-latest-none
if run_tagger assert --latest && grep -q 'No published release yet' "$FIXTURES/out.txt"; then
  report "assert --latest is a no-op on a repo with no releases" pass
else
  report "assert --latest is a no-op on a repo with no releases" fail
fi

new_case assert-latest-not-root
echo '{"tag_name":"data-2026.8.3"}' > "$FIXTURES/latest.json"
if run_tagger assert --latest && grep -q 'not a v\* release' "$FIXTURES/out.txt"; then
  report "assert --latest ignores releases that are not the root package" pass
else
  report "assert --latest ignores releases that are not the root package" fail
fi

# ── preflight ─────────────────────────────────────────────────────────────
# The pre-publish heads-up: can the release credential point a ref at a commit
# that touches .github/workflows/**? Measured behaviourally, because an
# installation token cannot read its own permissions.
new_case preflight-allowed
if run_tagger preflight \
  && grep -q 'can tag a commit across a workflow change' "$FIXTURES/out.txt" \
  && grep -q 'DELETE.*preflight-676-' "$FIXTURES/delete.log"; then
  report "preflight passes, and cleans up its probe tag, when tagging is allowed" pass
else
  report "preflight passes, and cleans up its probe tag, when tagging is allowed" fail
  cat "$FIXTURES/out.txt"
fi

# The state this repo is actually in: hyrr-release-bot has no `workflows`
# permission, so this ref creation is refused with 403.
new_case preflight-refused
: > "$FIXTURES/deny_workflow_ref"
if run_tagger preflight \
  && grep -q '::warning::' "$FIXTURES/out.txt" \
  && grep -q '#676' "$FIXTURES/out.txt"; then
  report "preflight warns (but does not gate) when the credential is refused" pass
else
  report "preflight warns (but does not gate) when the credential is refused" fail
  cat "$FIXTURES/out.txt"
fi

# A broken probe must NOT read as "refused" — the first version of this
# preflight silently reported "skipped" against the real API while its stubbed
# tests were green, which is the exact failure mode this file exists to catch.
new_case preflight-inconclusive
: > "$FIXTURES/break_workflow_ref"
if run_tagger preflight \
  && grep -q 'could not be completed' "$FIXTURES/out.txt" \
  && ! grep -q 'cannot create a tag' "$FIXTURES/out.txt"; then
  report "preflight distinguishes a broken probe from a refused one" pass
else
  report "preflight distinguishes a broken probe from a refused one" fail
  cat "$FIXTURES/out.txt"
fi

# The probe must aim at the parent of a workflow-touching commit — i.e. a
# commit that is not the tip and whose workflow tree differs from it.
new_case preflight-aims-correctly
run_tagger preflight
if grep -q "refs/tags/preflight-676-${REL_SHA:0:12} ${REL_SHA}" "$FIXTURES/post.log"; then
  report "preflight aims at the parent of the last workflow-touching commit" pass
else
  report "preflight aims at the parent of the last workflow-touching commit" fail
  cat "$FIXTURES/post.log" 2>/dev/null
fi

new_case preflight-no-history
: > "$FIXTURES/no_workflow_history"
if run_tagger preflight && grep -q 'preflight skipped' "$FIXTURES/out.txt"; then
  report "preflight is a no-op on a repo with no workflow history" pass
else
  report "preflight is a no-op on a repo with no workflow history" fail
fi

# ── usage ─────────────────────────────────────────────────────────────────
new_case usage
exits_with_2() {
  bash "$TAGGER" "$@" > /dev/null 2>&1
  [ $? -eq 2 ]
}
if exits_with_2; then
  report "no arguments is a usage error (exit 2)" pass
else
  report "no arguments is a usage error (exit 2)" fail
fi
if exits_with_2 frobnicate v1; then
  report "an unknown subcommand is a usage error (exit 2)" pass
else
  report "an unknown subcommand is a usage error (exit 2)" fail
fi

# ── the tripwire ──────────────────────────────────────────────────────────
# Every case above ran with a `git` stub on PATH that logs any invocation.
if ! find "$work" -name git.log -print -quit | grep -q .; then
  report "the tag is never created by shelling out to git (#676)" pass
else
  report "the tag is never created by shelling out to git (#676)" fail
  cat "$work"/case-*/git.log 2>/dev/null
fi

exit "$failed"
