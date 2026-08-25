#!/usr/bin/env bash
# Create — and then PROVE — the second release tag, hyrr-mcp-v* (#676).
#
#   scripts/tag-mcp-release.sh create         v0.21.0
#   scripts/tag-mcp-release.sh assert-trigger v0.21.0 [--wait 300]
#   scripts/tag-mcp-release.sh assert         v0.21.0
#   scripts/tag-mcp-release.sh assert --latest
#   scripts/tag-mcp-release.sh preflight
#
# A HYRR release cuts two tags off the same commit: `v<ver>` (GitHub Release +
# desktop artifacts) and `hyrr-mcp-v<ver>`. The PyPI wheels publish off the
# SECOND one, so a missing second tag ships nothing while every visible signal
# stays green — the Release publishes, `Release` reports success, and the next
# release-please run reports success too because it has nothing left to do.
# That is exactly what happened on 0.21.0.
#
# ── WHAT ACTUALLY BROKE (measured, 2026-08-25) ────────────────────────────
#
# The old step was `git tag && git push` from a credentialed checkout, and it
# was rejected:
#
#   ! [remote rejected] hyrr-mcp-v0.21.0 -> hyrr-mcp-v0.21.0
#     (refusing to allow a GitHub App to create or update workflow
#      `.github/workflows/tauri-build.yml` without `workflows` permission)
#
# #676 read that as "the release tree touched .github/workflows/**". It is not.
# The run checked out 9fd188a, the correct release commit, whose tree changes
# no workflow file at all. What happened is a RACE:
#
#   22:34:39  release-please publishes the Release; main's tip is 9fd188a
#   22:34:42  #672 merges — the one commit that edits tauri-build.yml — and
#             main's tip becomes aedc4d3
#   22:35:11  the tag step (after a 12s fetch-depth:0 checkout) asks for a ref
#             at 9fd188a, which is no longer the tip and whose workflow tree
#             now differs from the tip's. Refused.
#
# The rule, as measured against this installation: a GitHub App without the
# `workflows` permission may create or move a ref only where doing so does not
# change `.github/workflows/**` relative to the default branch's tip. On a
# fresh non-tip commit that edits a workflow file, with the hyrr-release-bot
# token, EVERY route is refused:
#
#   git push of the tag ................. remote rejected (message above)
#   POST /git/refs ...................... 403 Resource not accessible
#   POST /releases (tag_name+target) .... 403 Resource not accessible
#   create at the tip, force-PATCH over . 403 Resource not accessible
#
# At main's tip, all of them succeed. So #676's option 3 — "the API ref path
# may not be subject to the workflow-file check" — does NOT hold; the API is
# subject to exactly the same check and merely reports it as a bare 403.
# The installation holds contents:write + pull_requests:write + metadata:read.
#
# ── WHAT THIS SCRIPT DOES ABOUT IT ────────────────────────────────────────
#
# 1. Shrinks the window. Creating the ref over the REST API needs no working
#    tree and no credentialed checkout, so release-please.yml can call it
#    ~1s after the Release is published instead of ~32s. The race is against
#    "does another PR merge before we tag" — a third of a minute of exposure
#    becomes a second of it. It is a smaller target, not an eliminated one.
# 2. Anchors on the release. The commit comes from the `v*` tag, never from
#    main, so a tag can never be created on a commit that was not released.
# 3. Reads the tags BACK from the API instead of trusting a 201, because "the
#    call returned success and the world disagrees" is the whole failure class
#    (#461, #401, #676).
# 4. Asserts the tag actually TRIGGERED release-hyrr-mcp.yml. A tag that does
#    not trigger is the same bug wearing a different hat, which is why
#    GITHUB_TOKEN was rejected in #571 / ADR 0002. Verified once by probe (an
#    App-token ref created over the API does fire tag workflows) and re-checked
#    on every release rather than trusted forever.
# 5. `preflight` answers, BEFORE a release is cut, whether this credential can
#    tag at all — see cmd_preflight.
#
# The permanent fix for the race is to grant hyrr-release-bot the `workflows`
# permission, which is #676's option 1 and needs an App admin. That is a real
# privilege increase (the release bot could then rewrite CI) and is the
# maintainer's call, so it is not assumed here — but nothing else makes the
# refused case succeed, and `preflight` will keep saying so until it happens.
#
# `create` and `assert-trigger` are separate subcommands because they need
# different credentials: creating the ref needs the hyrr-release-bot App token
# (non-default, so the tag triggers workflows), while listing workflow runs
# needs `actions: read`, which that App installation does not have.
# GITHUB_TOKEN covers the read.
#
# Everything here is loud: every failure prints a ::error:: annotation and the
# command exits non-zero.
#
# Exit codes: 0 all good · 1 something is wrong · 2 usage.
#
# Requires: gh (authenticated — GH_TOKEN), jq.
# Tests: scripts/tests/test_tag_mcp_release.sh (offline, gh stubbed).

# No `set -e`: the assertions below deliberately keep going after a failure so
# the report is complete, which -e would defeat.
set -uo pipefail

# Seconds between polls while waiting for the tag to trigger the publish
# workflow. Overridable so the tests do not sleep.
POLL_INTERVAL="${TAG_MCP_POLL_INTERVAL:-15}"

MCP_WORKFLOW="release-hyrr-mcp.yml"

FAILURES=()
note_pass() { echo "  ✓ $1"; }
note_fail() {
  echo "::error::$1"
  echo "  ✗ $1" >&2
  FAILURES+=("$1")
}

usage() {
  cat >&2 <<'USAGE'
usage: tag-mcp-release.sh create         <v-tag>              (needs the App token)
       tag-mcp-release.sh assert-trigger <v-tag> [--wait <s>] (needs actions:read)
       tag-mcp-release.sh assert         <v-tag>
       tag-mcp-release.sh assert         --latest
       tag-mcp-release.sh preflight                           (needs the App token)
USAGE
  exit 2
}

for tool in gh jq; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "ERROR: '$tool' is required but not installed." >&2
    exit 2
  }
done

REPO="${GITHUB_REPOSITORY:-}"
if [ -z "$REPO" ]; then
  REPO="$(gh repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null)"
fi
[ -n "$REPO" ] || {
  echo "ERROR: cannot determine the repository (set GITHUB_REPOSITORY)." >&2
  exit 2
}

# ── Git ref plumbing ──────────────────────────────────────────────────────
# resolve_sha <tag> -> the COMMIT sha the tag names, or exit 1 if it does not
# exist. Annotated tags are dereferenced: release-please creates lightweight
# tags today, but a tag object pointing at the right commit must not read as
# "points somewhere else" if that ever changes.
resolve_sha() {
  local tag="$1" ref_json sha type tag_json
  ref_json="$(gh api "repos/${REPO}/git/ref/tags/${tag}" 2>/dev/null)" || return 1
  sha="$(jq -r '.object.sha // empty' <<<"$ref_json")"
  type="$(jq -r '.object.type // empty' <<<"$ref_json")"
  [ -n "$sha" ] || return 1
  if [ "$type" = "tag" ]; then
    tag_json="$(gh api "repos/${REPO}/git/tags/${sha}" 2>/dev/null)" || return 1
    sha="$(jq -r '.object.sha // empty' <<<"$tag_json")"
    [ -n "$sha" ] || return 1
  fi
  printf '%s\n' "$sha"
}

# ── The assertion the whole thing exists for ──────────────────────────────
# Both tags must exist AND name the same commit. "Exists" alone is not enough:
# a hyrr-mcp-v* tag on the wrong commit publishes wheels built from a tree
# that was never released, which is worse than publishing nothing.
assert_pair() {
  local vtag="$1" mcptag="hyrr-mcp-${1}" vsha msha

  if ! vsha="$(resolve_sha "$vtag")"; then
    note_fail "tag ${vtag} is missing — there is no release to check"
    return 1
  fi
  note_pass "tag ${vtag} -> ${vsha}"

  if ! msha="$(resolve_sha "$mcptag")"; then
    note_fail "tag ${mcptag} is missing — the wheels would build but never publish (${MCP_WORKFLOW} only triggers on hyrr-mcp-v*)"
    return 1
  fi

  if [ "$vsha" != "$msha" ]; then
    note_fail "tag ${mcptag} is at ${msha} but ${vtag} is at ${vsha} — the wheels would be built from a tree that was never released"
    return 1
  fi
  note_pass "tag ${mcptag} -> ${msha} (same commit)"
  return 0
}

# ── Did the tag actually TRIGGER the publish workflow? ────────────────────
# The point of the App token (ADR 0002, #571) is that its refs fire downstream
# workflows, unlike GITHUB_TOKEN's. Assert it instead of believing it. Any run
# for that tag counts, whenever it started — so a re-run of a release that was
# already tagged still passes.
wait_for_trigger() {
  local mcptag="$1" budget="$2" waited=0 runs step
  # Charge at least 1s per attempt even when the interval is 0 (the tests set
  # it to 0 so they do not sleep) — otherwise the budget never advances and
  # the loop spins forever on a tag that will never trigger, which is exactly
  # the case this function exists to report.
  step=$((POLL_INTERVAL > 0 ? POLL_INTERVAL : 1))
  while :; do
    runs="$(gh api "repos/${REPO}/actions/workflows/${MCP_WORKFLOW}/runs?per_page=100" 2>/dev/null)"
    if [ -n "$runs" ] && jq -e --arg t "$mcptag" \
      '[.workflow_runs[]? | select(.head_branch == $t)] | length > 0' >/dev/null 2>&1 <<<"$runs"; then
      note_pass "${mcptag} triggered ${MCP_WORKFLOW}"
      return 0
    fi
    [ "$waited" -ge "$budget" ] && break
    sleep "$POLL_INTERVAL"
    waited=$((waited + step))
  done
  note_fail "${mcptag} exists but did not trigger ${MCP_WORKFLOW} within ${budget}s — a tag that does not trigger is the same failure as no tag at all (#571, ADR 0002)"
  return 1
}

# ── create ────────────────────────────────────────────────────────────────
cmd_create() {
  local vtag="$1"
  local mcptag="hyrr-mcp-${vtag}" sha existing

  # The v* tag — NOT main, NOT the workflow's checkout — defines the commit.
  if ! sha="$(resolve_sha "$vtag")"; then
    note_fail "release tag ${vtag} does not exist — refusing to guess which commit ${mcptag} should name"
    return 1
  fi

  if existing="$(resolve_sha "$mcptag")"; then
    if [ "$existing" != "$sha" ]; then
      note_fail "${mcptag} already exists at ${existing}, but ${vtag} is at ${sha} — refusing to move a published release tag"
      return 1
    fi
    note_pass "${mcptag} already at ${sha} — nothing to create"
  else
    # No working tree, no credentialed checkout — that is what lets the caller
    # run this ~1s after the Release instead of ~32s. See the header.
    local out rc
    out="$(gh api -X POST "repos/${REPO}/git/refs" \
      -f ref="refs/tags/${mcptag}" -f sha="${sha}" 2>&1)"
    rc=$?
    if [ "$rc" -ne 0 ]; then
      echo "$out" >&2
      note_fail "could not create refs/tags/${mcptag} at ${sha} — the MCP wheels will NOT publish for ${vtag}"
      case "$out" in
        *"Resource not accessible by integration"*)
          note_fail "that 403 is the #676 race: main has moved past the release commit onto a commit that edits .github/workflows/**, and hyrr-release-bot has no 'workflows' permission, so it may no longer create a ref there. Re-running with retag=${vtag} will hit the same wall until either the App is granted 'workflows' (the permanent fix) or the tag is created once with a workflow-scoped PAT: gh api -X POST repos/${REPO}/git/refs -f ref=refs/tags/${mcptag} -f sha=${sha}"
          ;;
      esac
      return 1
    fi
    note_pass "created ${mcptag} -> ${sha}"
  fi

  # Read it back. A 201 is a claim; the ref listing is the fact.
  assert_pair "$vtag"
}

# ── preflight ─────────────────────────────────────────────────────────────
# "Can this credential tag a release at all?", answered before a release is
# cut rather than after.
#
# It cannot be answered by reading permissions — an installation token cannot
# introspect its own installation, and doing it with an App JWT would mean
# signing one in bash. So ask GitHub the same question behaviourally, by
# performing the exact operation that fails at release time: point a tag ref at
# a commit whose .github/workflows/** differs from the default branch's tip.
#
# Such a commit already exists and costs one API call to find: the PARENT of
# the most recent commit that touched .github/workflows/**. That commit is by
# construction not the tip (it is strictly older) and its workflow tree is by
# construction different from the tip's (its child changed exactly that). No
# blobs, trees or commits are fabricated — an earlier version of this built a
# synthetic commit and it failed against the real API while the stubbed tests
# passed, which is the same "green over a wrong world" this file is about.
#
# The probe tag matches no workflow trigger and is deleted either way.
#
# ONE-SIDED, deliberately. A refusal is real and worth acting on. A success is
# NOT a guarantee: the check is not a pure function of the commit. Measured on
# 2026-08-25, with the installation's permissions unchanged throughout, the
# identical POST /git/refs at the identical commit was refused at 21:00 and
# accepted at 21:12. So this reports a refusal loudly and reports a success
# without claiming anything — a check that can falsely reassure is the failure
# mode this whole file exists to delete, and it does not get to introduce a
# fresh one.
#
# A warning, not a gate, even when refused: a release only hits the wall if a
# workflow-touching PR merges in the second between the Release publishing and
# the tag being created. Rare — but the cost of it happening is that nothing
# reaches PyPI.
cmd_preflight() {
  local target ref out rc
  target="$(gh api "repos/${REPO}/commits?path=.github/workflows&per_page=1" 2>/dev/null |
    jq -r '.[0].parents[0].sha // empty')"
  if [ -z "$target" ]; then
    echo "  · preflight skipped: no commit touching .github/workflows/ has a parent to aim at"
    return 0
  fi

  ref="refs/tags/preflight-676-${target:0:12}"
  # A leftover from an interrupted run would make the create a no-op that
  # looks like a pass, so clear it first.
  gh api -X DELETE "repos/${REPO}/git/${ref}" > /dev/null 2>&1

  out="$(gh api -X POST "repos/${REPO}/git/refs" -f ref="$ref" -f sha="$target" 2>&1)"
  rc=$?
  if [ "$rc" -eq 0 ]; then
    gh api -X DELETE "repos/${REPO}/git/${ref}" > /dev/null 2>&1
    # Not note_pass: see the ONE-SIDED note above. No refusal was observed;
    # that is not the same as "the release will be taggable".
    echo "  · preflight: no refusal observed at ${target:0:12} (not a guarantee — #676)"
    return 0
  fi

  echo "$out" >&2
  case "$out" in
    *"Resource not accessible by integration"* | *"403"*)
      echo "::warning::The release credential cannot create a tag on a commit whose .github/workflows/** differs from the default branch tip. If any workflow-touching PR merges between a release publishing and its hyrr-mcp-v* tag being created, the tag is refused, the MCP wheels never publish, and every other signal stays green. That is #676, and it cost 0.21.0 its wheels. Grant the hyrr-release-bot App the 'workflows' permission to close it."
      echo "  ! release-bot cannot tag across a workflow change (#676) — see the warning above"
      ;;
    *)
      echo "::warning::preflight could not be completed (see the error above); this says nothing about whether the release can be tagged."
      ;;
  esac
  # Deliberately exit 0: this is the pre-publish heads-up, not a gate. The
  # release-time failure is the gate, and assert-tag-parity is the net.
  return 0
}

# ── main ──────────────────────────────────────────────────────────────────
CMD="${1:-}"
[ -n "$CMD" ] || usage
shift || true

case "$CMD" in
  create)
    TAG="${1:-}"
    [ -n "$TAG" ] || usage
    case "$TAG" in -*) usage ;; esac
    [ $# -eq 1 ] || usage
    echo "Tagging the MCP release for ${TAG} in ${REPO}"
    cmd_create "$TAG"
    ;;

  preflight)
    [ $# -eq 0 ] || usage
    echo "Preflighting the release credential against ${REPO}"
    cmd_preflight
    ;;

  assert-trigger)
    TAG="${1:-}"
    [ -n "$TAG" ] || usage
    case "$TAG" in -*) usage ;; esac
    shift
    WAIT=0
    while [ $# -gt 0 ]; do
      case "$1" in
        --wait)
          WAIT="${2:-}"
          case "$WAIT" in '' | *[!0-9]*) usage ;; esac
          shift 2
          ;;
        *) usage ;;
      esac
    done
    echo "Checking that hyrr-mcp-${TAG} triggered ${MCP_WORKFLOW} in ${REPO}"
    wait_for_trigger "hyrr-mcp-${TAG}" "$WAIT"
    ;;

  assert)
    TARGET="${1:-}"
    [ -n "$TARGET" ] || usage
    [ $# -eq 1 ] || usage
    if [ "$TARGET" = "--latest" ]; then
      LATEST="$(gh api "repos/${REPO}/releases/latest" 2>/dev/null | jq -r '.tag_name // empty')"
      if [ -z "$LATEST" ]; then
        echo "No published release yet — nothing to check."
        exit 0
      fi
      # Only the root package cuts a paired MCP tag. Anything else (a data
      # release, a hand-made tag) is not this check's business.
      case "$LATEST" in
        v*) ;;
        *)
          echo "Latest release ${LATEST} is not a v* release — nothing to check."
          exit 0
          ;;
      esac
      TARGET="$LATEST"
    fi
    case "$TARGET" in -*) usage ;; esac
    echo "Asserting both release tags for ${TARGET} in ${REPO}"
    assert_pair "$TARGET"
    ;;

  *) usage ;;
esac

if [ ${#FAILURES[@]} -gt 0 ]; then
  echo >&2
  echo "FAILED (${#FAILURES[@]}):" >&2
  for f in "${FAILURES[@]}"; do echo "  - $f" >&2; done
  exit 1
fi

echo "OK"
exit 0
