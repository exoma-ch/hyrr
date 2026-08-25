#!/usr/bin/env bash
# Bash reader for scripts/frontend-data-libraries.txt — the SSoT list of
# nuclear-data libraries shipped in the frontend bundle (#651, epic #649).
#
# Sourced, not executed. Counterpart to
# frontend/scripts/frontend-data-libraries.ts; the two must stay in lockstep,
# and `scripts/tests/test_frontend_data_libraries.sh` asserts they agree on the
# real list under both LF and CRLF.
#
# Why a shared file rather than an inline `while read` at each site: there were
# three near-copies of this loop (the copy script, the guard test, the vite
# plugin) and the odd one out shipped a broken release. The plugin stripped
# comments with a JS regex whose `.` does not match `\r`, so on the CRLF
# checkout Windows CI produces it demanded library subdirectories named after
# this file's own comment prose — and v0.21.0 went out with no Windows
# installer (#677).
#
# Usage:
#   . scripts/frontend-data-libraries.sh
#   read_frontend_data_libraries [path]   # → LIBRARY_SPECS array

# Same grammar as ENTRY in frontend-data-libraries.ts: `name` or `name:subdir`.
# Narrow on purpose — these become path segments under the bundle root, so
# anything outside this alphabet is far likelier to be a parse gone wrong than
# a library someone meant. `..` and `/` are rejected, which is also what keeps
# a malformed entry from writing outside the destination.
FRONTEND_DATA_LIBRARY_RE='^[A-Za-z0-9][A-Za-z0-9._-]*(:[A-Za-z0-9][A-Za-z0-9._-]*)?$'

# Populates the global array LIBRARY_SPECS with the verbatim entries.
# Returns non-zero (and explains itself) on a malformed or empty list.
read_frontend_data_libraries() {
  local list="${1:-}"
  if [ -z "$list" ]; then
    local here
    here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    list="$here/frontend-data-libraries.txt"
  fi

  [ -f "$list" ] || { echo "frontend-data-libraries: missing $list" >&2; return 1; }

  LIBRARY_SPECS=()
  local lineno=0 line entry
  while IFS= read -r line || [ -n "$line" ]; do
    lineno=$((lineno + 1))
    entry="${line%%#*}"        # comments run to end of line
    entry="${entry//$'\r'/}"   # CRLF checkouts (Windows CI)

    # Trim, do NOT squash. This function used to run `tr -d '[:space:]'`, which
    # deletes *interior* whitespace too — and that is quietly dangerous now that
    # entries are validated: squashing turns the prose line
    # `Format: one entry per line` into `Format:oneentryperline`, which is a
    # perfectly well-formed `name:subdir` spec and sails through the check.
    # Trimming leaves the interior spaces in place, so prose fails the pattern,
    # which is the entire point of validating.
    #
    # Loop form rather than the nested `${e#"${e%%[![:space:]]*}"}` idiom:
    # tauri-build.yml runs this on macos-latest, where `bash` is still 3.2, and
    # this form needs no assumptions about how old shells parse nested quoting.
    while [ -n "$entry" ] && [[ "$entry" == [[:space:]]* ]]; do entry="${entry#[[:space:]]}"; done
    while [ -n "$entry" ] && [[ "$entry" == *[[:space:]] ]]; do entry="${entry%[[:space:]]}"; done
    [ -n "$entry" ] || continue

    if ! [[ "$entry" =~ $FRONTEND_DATA_LIBRARY_RE ]]; then
      echo "frontend-data-libraries: $list:$lineno: malformed entry '$entry'" >&2
      echo "  Expected 'name' or 'name:subdir' (letters, digits, dot, dash, underscore)." >&2
      echo "  If that looks like prose, the comment stripping is broken — fix the" >&2
      echo "  parser, not the list." >&2
      return 1
    fi
    LIBRARY_SPECS+=("$entry")
  done < "$list"

  if [ "${#LIBRARY_SPECS[@]}" -eq 0 ]; then
    echo "frontend-data-libraries: $list lists no libraries — the frontend would" >&2
    echo "  ship no cross-sections at all." >&2
    return 1
  fi
}

# Destination directory for a spec: `name` → xs, `name:subdir` → subdir.
# Mirrors the `subdir` field in frontend-data-libraries.ts.
frontend_data_subdir() {
  case "$1" in
    *:*) printf '%s' "${1#*:}" ;;
    *)   printf '%s' "xs" ;;
  esac
}

# Catalog name for a spec.
frontend_data_library() { printf '%s' "${1%%:*}"; }
