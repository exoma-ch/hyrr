//! #572 — end-to-end MCP integration for the impact-classified changelog.
//!
//! Verifies the surface an agent actually sees:
//!  - `tools/list` advertises `get_changelog` with a `since_version` schema
//!  - `initialize` instructions mention the tool + its purpose
//!  - `call_tool("get_changelog", {since_version: "0.18.0"})` returns the
//!    #533/#488/#529 entries in a shape a client can consume without
//!    re-parsing free-form prose
//!  - omitting `since_version` returns every release
//!  - `physics_affecting_summary` is populated whenever a physics_affecting
//!    entry is in the filtered set (this is the "worth interrupting for"
//!    signal an agent uses at handshake time, and it is what pairs with
//!    #571's "a newer release exists" check)
//!
//! Deliberately does NOT need `HYRR_DATA` or a `ParquetDataStore`. The
//! changelog tool takes no DB dependency — the classification is a
//! release-time artifact compiled into the crate, so this test is fully
//! hermetic and can run in the same CI job as the transport tests without
//! nucl-parquet on disk.

#![cfg(feature = "mcp")]

use hyrr_core::db::{DatabaseProtocol, InMemoryDataStore};
use hyrr_core::materials::MaterialRegistry;
use hyrr_core::mcp::tools::{call_tool, list_tools, server_instructions};
use serde_json::{json, Value};

/// Empty in-memory DB — `call_tool` suffixes every response with
/// `*Library: <id>*`, but `get_changelog` never actually queries the DB
/// (the classification is a compiled-in release-time artifact). The empty
/// store lets us assert that fact without needing `HYRR_DATA` on disk.
fn empty_db() -> InMemoryDataStore {
    InMemoryDataStore::new("tendl-2023-iso")
}

fn call(name: &str, args: Value) -> String {
    let db = empty_db();
    let mut mats: MaterialRegistry = MaterialRegistry::new();
    let db_ref: &dyn DatabaseProtocol = &db;
    call_tool(db_ref, &mut mats, name, &args)
        .unwrap_or_else(|e| panic!("{name} failed: {e}"))
        .text
}

// ─── tools/list surface ─────────────────────────────────────────────────────

#[test]
fn tools_list_advertises_get_changelog_with_since_version() {
    let tools = list_tools("tendl-2023-iso");
    let entry = tools
        .iter()
        .find(|t| t.get("name").and_then(|v| v.as_str()) == Some("get_changelog"))
        .expect("get_changelog must appear in tools/list");

    // Description must telegraph the load-bearing distinction: impact +
    // silent + guidance, not just "here is a changelog".
    let desc = entry.get("description").and_then(|v| v.as_str()).unwrap();
    for needle in ["impact", "silent", "guidance", "since_version"] {
        assert!(
            desc.contains(needle),
            "get_changelog description should mention `{needle}` — got:\n{desc}",
        );
    }

    // Schema must accept an optional since_version string. Missing/undefined
    // is the "give me everything" case.
    let props = entry
        .get("inputSchema")
        .and_then(|s| s.get("properties"))
        .expect("inputSchema.properties");
    let since = props
        .get("since_version")
        .expect("since_version property must exist");
    assert_eq!(since.get("type").and_then(|v| v.as_str()), Some("string"));

    // Not required — omitting it must be legal (returns every release).
    let required = entry
        .get("inputSchema")
        .and_then(|s| s.get("required"))
        .cloned()
        .unwrap_or(json!([]));
    assert!(
        required.as_array().map(|a| a.is_empty()).unwrap_or(true),
        "since_version must be optional — got required: {required}",
    );
}

// ─── initialize `instructions` mentions the tool (#572) ─────────────────────

#[test]
fn server_instructions_mention_get_changelog_and_impact() {
    let text = server_instructions("tendl-2023-iso");
    // The line the agent needs to see: (a) the tool exists, (b) its
    // purpose is deciding "should I re-run" — not "what changed".
    assert!(
        text.contains("get_changelog"),
        "server_instructions must name `get_changelog` so a client can find it: {text}",
    );
    assert!(
        text.to_lowercase().contains("impact") && text.to_lowercase().contains("re-run"),
        "server_instructions must explain why the tool matters (impact / re-run), got: {text}",
    );
}

// ─── since_version filter — the acceptance criterion ───────────────────────

#[test]
fn call_get_changelog_since_0_18_0_surfaces_533_488_529() {
    let out = call("get_changelog", json!({ "since_version": "0.18.0" }));
    // The response is a JSON envelope wrapped in the Library-suffixed text.
    // Split on the first `---` to peel off the suffix, then parse the JSON.
    let json_part = out
        .split("\n\n---")
        .next()
        .expect("envelope should precede the Library separator");
    let v: Value = serde_json::from_str(json_part.trim())
        .unwrap_or_else(|e| panic!("get_changelog output must be valid JSON: {e}\n---\n{out}"));

    // `since_version: 0.18.0` must NOT include 0.18.0 itself (strict >),
    // and must include 0.19.0 where the three motivating fixes landed.
    let versions: Vec<&str> = v["releases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["version"].as_str().unwrap())
        .collect();
    assert!(
        versions.contains(&"0.19.0"),
        "since 0.18.0 must include 0.19.0 — got {versions:?}",
    );
    assert!(
        !versions.contains(&"0.18.0"),
        "since 0.18.0 must NOT include 0.18.0 (strict greater-than) — got {versions:?}",
    );

    // Every one of #533, #488, #529 must be represented AND classified
    // physics_affecting + silent. This is the acceptance test #572 calls out.
    let mut ref_seen: std::collections::HashMap<u32, (bool, bool)> = [533, 488, 529]
        .iter()
        .map(|i| (*i, (false, false)))
        .collect();
    for release in v["releases"].as_array().unwrap() {
        for entry in release["entries"].as_array().unwrap() {
            let refs: Vec<u32> = entry["refs"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|x| x.as_u64().map(|n| n as u32))
                .collect();
            for r in &refs {
                if let Some(state) = ref_seen.get_mut(r) {
                    let impact = entry["impact"].as_str().unwrap();
                    let silent = entry["silent"].as_bool().unwrap();
                    state.0 = state.0 || impact == "physics_affecting";
                    state.1 = state.1 || silent;
                    assert_eq!(
                        impact, "physics_affecting",
                        "#{r} must be classified physics_affecting — got {impact}: {}",
                        entry["summary"],
                    );
                    assert!(
                        silent,
                        "#{r} must be flagged silent (users had no warning) — got: {}",
                        entry["summary"],
                    );
                }
            }
        }
    }
    for (issue, (pa, silent)) in ref_seen {
        assert!(pa, "#{issue} must be present as a physics_affecting entry");
        assert!(silent, "#{issue} must be flagged silent");
    }

    // physics_affecting_summary is the handshake signal — every
    // physics_affecting entry must appear there too.
    let summary = v["physics_affecting_summary"].as_array().unwrap();
    let summary_refs: std::collections::HashSet<u32> = summary
        .iter()
        .flat_map(|e| {
            e["refs"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|x| x.as_u64().map(|n| n as u32))
                .collect::<Vec<_>>()
        })
        .collect();
    for issue in [533u32, 488, 529] {
        assert!(
            summary_refs.contains(&issue),
            "physics_affecting_summary must lift #{issue}",
        );
    }
}

#[test]
fn call_get_changelog_without_since_returns_every_release() {
    let out = call("get_changelog", json!({}));
    let json_part = out.split("\n\n---").next().unwrap();
    let v: Value = serde_json::from_str(json_part.trim()).unwrap();
    let count = v["releases"].as_array().unwrap().len();
    assert!(
        count >= 2,
        "with no since_version, all releases return (currently {count})",
    );
    assert!(v["header"]["since_version"].is_null());
}

#[test]
fn call_get_changelog_since_newest_returns_no_entries() {
    // Feed the newest version we know about back in — should get an empty
    // list, not the whole artifact.
    let out = call("get_changelog", json!({}));
    let json_part = out.split("\n\n---").next().unwrap();
    let v: Value = serde_json::from_str(json_part.trim()).unwrap();
    let newest = v["releases"][0]["version"].as_str().unwrap().to_string();

    let out2 = call("get_changelog", json!({ "since_version": newest }));
    let json_part2 = out2.split("\n\n---").next().unwrap();
    let v2: Value = serde_json::from_str(json_part2.trim()).unwrap();
    assert!(
        v2["releases"].as_array().unwrap().is_empty(),
        "since(newest) must be empty",
    );
    assert!(
        v2["physics_affecting_summary"]
            .as_array()
            .unwrap()
            .is_empty(),
        "no releases → no physics-affecting summary",
    );
}

#[test]
fn call_get_changelog_rejects_non_string_since_version() {
    let db = empty_db();
    let mut mats: MaterialRegistry = MaterialRegistry::new();
    let db_ref: &dyn DatabaseProtocol = &db;
    let err = call_tool(
        db_ref,
        &mut mats,
        "get_changelog",
        &json!({ "since_version": 42 }),
    )
    .expect_err("numeric since_version must error");
    assert!(
        err.to_lowercase().contains("since_version"),
        "error should name the offending field: {err}",
    );
}

// ─── data_version is represented per release (P1) ───────────────────────────

#[test]
fn every_release_in_response_carries_data_version() {
    // A nuclear-data bump can change results with zero code change — that is
    // exactly what a commit-derived changelog cannot show, and why the field
    // is compulsory.
    let out = call("get_changelog", json!({}));
    let json_part = out.split("\n\n---").next().unwrap();
    let v: Value = serde_json::from_str(json_part.trim()).unwrap();
    for release in v["releases"].as_array().unwrap() {
        let dv = release["data_version"].as_str();
        assert!(
            dv.is_some_and(|s| !s.is_empty()),
            "release {} missing data_version (P1 of #572)",
            release["version"],
        );
    }
}
