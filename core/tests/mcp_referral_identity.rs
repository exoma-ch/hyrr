//! #681 — the hand-off to `nucl-parquet-mcp` must not drop the data identity
//! HYRR just established.
//!
//! # What is actually being defended
//!
//! HYRR owns derived results and curated summaries; `nucl-parquet-mcp` owns
//! raw evaluated data. That split is deliberate (ADR 0001 renamed HYRR's tool
//! away from `get_cross_sections` precisely to avoid the collision), and the
//! referral in `list_reaction_channels` is where it is stated.
//!
//! The two servers resolve their data **independently** — upstream reads
//! `$NUCL_PARQUET_DATA`; HYRR tries `--data-dir`, then `HYRR_DATA`, then the
//! submodule, then `~/.hyrr/nucl-parquet`. Nothing compares them. So an agent
//! that follows an unqualified referral can read σ(E) from a different library
//! or a different data release than the simulation used, reconcile the two,
//! and report a discrepancy — or an agreement — that is an artefact of the
//! mismatch. No error appears at any step: the silent-wrong-answer class
//! (epic #649), arriving through a door HYRR opened itself.
//!
//! #593 / #601 / #671 established that a result carries the data that produced
//! it. These tests pin that the referral does not end that chain mid-sentence,
//! and — the load-bearing part — that the values are **read from the live
//! pin**, so a refactor cannot quietly drop them or freeze them at whatever
//! was current when someone typed a literal.
//!
//! Runs only with `--features mcp`. Data resolution mirrors
//! `mcp_dose_and_nuclide.rs`.

#![cfg(feature = "mcp")]

use hyrr_core::data_fetch::data_version;
use hyrr_core::db::{DatabaseProtocol, ParquetDataStore};
use hyrr_core::materials::MaterialRegistry;
use hyrr_core::mcp::tools::{call_tool, list_tools, server_instructions};
use serde_json::{json, Value};

/// A library id that is *not* the default, so a description echoing it proves
/// the value was threaded through rather than hardcoded.
const OTHER_LIBRARY: &str = "endfb-8.1";

fn store() -> ParquetDataStore {
    let data_dir = std::env::var("HYRR_DATA").unwrap_or_else(|_| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../nucl-parquet/data").to_string()
    });
    ParquetDataStore::new(&data_dir, "tendl-2023-iso").unwrap_or_else(|e| {
        panic!("ParquetDataStore::new({data_dir}) failed — is the data present? {e}")
    })
}

/// Every tool description that points at `nucl-parquet-mcp`.
fn outward_referrals(library: &str) -> Vec<(String, String)> {
    list_tools(library)
        .iter()
        .filter_map(|t| {
            let name = t["name"].as_str()?.to_string();
            let desc = t["description"].as_str()?.to_string();
            desc.contains("nucl-parquet-mcp").then_some((name, desc))
        })
        .collect()
}

/// A tool response with the global `*Library: … · data release: …*` footer
/// stripped.
///
/// Load-bearing for the response-side tests: the footer alone contains both
/// identifiers, so asserting on the raw text would pass even if the referral
/// itself had been stripped bare. These tests are about the referral, so they
/// must not be able to lean on the footer. (Caught by mutating the referral
/// back to its pre-#681 wording — both response tests still passed.)
fn body_without_footer(text: &str) -> &str {
    match text.rfind("\n\n---\n*Library:") {
        Some(i) => &text[..i],
        None => panic!("response lost the ADR-0001 library footer:\n{text}"),
    }
}

/// Just the referral paragraph out of a full tool description.
///
/// Needed because `SCOPE_SUFFIX` legitimately names `tendl-2023-iso` as *the
/// default* (it is the library that ships no neutron sublibrary), which is a
/// true statement whatever the live library is. Only the referral block is
/// claiming to describe *this* server's data.
fn referral_block(desc: &str) -> &str {
    let start = desc
        .find("DEEPER DATA:")
        .unwrap_or_else(|| panic!("no referral block in:\n{desc}"));
    let rest = &desc[start..];
    match rest.find("\n\nSCOPE:") {
        Some(end) => &rest[..end],
        None => rest,
    }
}

/// Longest CalVer-shaped token (`YYYY.M.P`) in `text`, if any. Hand-rolled so
/// the test suite gains no regex dependency for one scan.
fn find_calver(text: &str) -> Option<String> {
    let b: Vec<char> = text.chars().collect();
    let digits = |i: usize| b.get(i).is_some_and(|c| c.is_ascii_digit());
    for start in 0..b.len() {
        // Anchor on a 4-digit year not preceded by another digit or a dot.
        if !(digits(start) && digits(start + 1) && digits(start + 2) && digits(start + 3)) {
            continue;
        }
        if start > 0 && (b[start - 1].is_ascii_digit() || b[start - 1] == '.') {
            continue;
        }
        // Exactly four digits — `12345.6.7` is not a release id, and matching
        // it would make the "no other release named" test reject valid prose.
        if digits(start + 4) {
            continue;
        }
        let mut i = start + 4;
        let mut dots = 0;
        while i < b.len() && (b[i].is_ascii_digit() || (b[i] == '.' && digits(i + 1))) {
            if b[i] == '.' {
                dots += 1;
            }
            i += 1;
        }
        if dots == 2 {
            return Some(b[start..i].iter().collect());
        }
    }
    None
}

// ─── The acceptance criteria ────────────────────────────────────────────────

/// Criterion 1 + 2: every outward referral carries the data release, read from
/// the live pin rather than hardcoded.
///
/// This is the regression guard the issue asks for. It fails if a referral is
/// added without the identity, if one is stripped in a refactor, or if the
/// release is frozen into a literal — the last because the expected value is
/// read from the same constant `get_version_info` prints, so the two move
/// together or the test breaks.
#[test]
fn every_outward_referral_names_the_live_data_release() {
    let referrals = outward_referrals("tendl-2023-iso");
    assert!(
        !referrals.is_empty(),
        "no tool description mentions nucl-parquet-mcp — either the referral \
         was removed (then remove this test deliberately) or the marker text \
         changed and this guard has gone blind"
    );
    for (name, desc) in &referrals {
        assert!(
            desc.contains(data_version()),
            "tool `{name}` points at nucl-parquet-mcp without naming the data \
             release `{}` this server computed against — an agent following it \
             cannot tell whether that server serves the same data (#681). \
             Got:\n{desc}",
            data_version(),
        );
    }
}

/// Criterion 2, the other half: no referral may name a release that is not the
/// live one. A hardcoded CalVer would satisfy "contains a version" while
/// silently drifting at the next data bump.
#[test]
fn no_referral_names_a_release_other_than_the_live_one() {
    for (name, desc) in outward_referrals("tendl-2023-iso") {
        let stripped = desc.replace(data_version(), "");
        assert!(
            find_calver(&stripped).is_none(),
            "tool `{name}` names a data release other than the live pin `{}`: \
             found `{}`. Release ids must be read from \
             `data_fetch::data_version()`; write `2026.8.x` in prose if you \
             need an illustrative version.",
            data_version(),
            find_calver(&stripped).unwrap_or_default(),
        );
    }
}

/// Criterion 1: the library id is threaded from the live store, not baked in.
///
/// It is also not merely informative — upstream's `get_cross_sections` takes
/// `library` as a **required** argument, so a referral that omits it leaves
/// the agent guessing a required parameter.
#[test]
fn the_cross_section_referral_names_the_live_library() {
    let referrals = outward_referrals(OTHER_LIBRARY);
    let (_, desc) = referrals
        .iter()
        .find(|(name, _)| name == "list_reaction_channels")
        .expect("list_reaction_channels must carry the outward referral");
    let block = referral_block(desc);
    assert!(
        block.contains(OTHER_LIBRARY),
        "the referral must name the LIVE library (`{OTHER_LIBRARY}` here), not \
         a compiled-in default — a server started with `--library` would \
         otherwise misdescribe its own data. Got:\n{block}"
    );
    assert!(
        !block.contains("tendl-2023-iso"),
        "the referral names `tendl-2023-iso` while the live library is \
         `{OTHER_LIBRARY}` — the default is hardcoded somewhere. Got:\n{block}"
    );
}

/// Criterion 3: the wording must make the CONSEQUENCE explicit. "Versions
/// differ" is a fact an agent can note and move past; "the numbers do not
/// correspond" is one it has to act on.
#[test]
fn the_referral_states_the_consequence_not_just_the_versions() {
    let referrals = outward_referrals("tendl-2023-iso");
    let (_, desc) = referrals
        .iter()
        .find(|(name, _)| name == "list_reaction_channels")
        .expect("list_reaction_channels must carry the outward referral");
    let lower = desc.to_lowercase();
    assert!(
        lower.contains("artefact") || lower.contains("artifact"),
        "the referral must say a mismatch makes any comparison an ARTEFACT, \
         not merely that the versions differ. Got:\n{desc}"
    );
}

/// The description-side referral is held in context for the WHOLE session by
/// every client — including the many that never drill into a cross-section,
/// and the ones with no `nucl-parquet-mcp` connected at all.
///
/// So it buys only what qualifies the numbers regardless: the identity and the
/// consequence. The operational half (tool name, required `library` argument,
/// `element` keying) belongs in the response, where it is paid per call by a
/// caller who actually asked.
///
/// The budget is a real assertion, not decoration: the first version of this
/// referral ran to ~570 characters of always-resident text, and the natural
/// pressure on a string like this is to grow. If a future change genuinely
/// needs more room here, raise the number deliberately and say why.
#[test]
fn the_description_side_referral_stays_within_its_context_budget() {
    const BUDGET: usize = 400;
    for (name, desc) in outward_referrals("tendl-2023-iso") {
        let Some(start) = desc.find("DEEPER DATA:") else {
            continue;
        };
        let block = referral_block(&desc);
        assert!(
            block.chars().count() <= BUDGET,
            "tool `{name}`'s always-resident referral is {} chars, over the \
             {BUDGET}-char budget. Operational detail belongs in the response \
             (`cross_section_referral_full`), not here. Block starts at byte \
             {start}:\n{block}",
            block.chars().count(),
        );
    }
}

/// HYRR cannot see the client's tool list, so the description must not order an
/// agent to call a server it may not have. An agent told confidently to use a
/// tool it lacks is an agent invited to invent one.
#[test]
fn the_description_side_referral_hedges_on_availability() {
    let referrals = outward_referrals("tendl-2023-iso");
    let (_, desc) = referrals
        .iter()
        .find(|(name, _)| name == "list_reaction_channels")
        .expect("list_reaction_channels must carry the outward referral");
    let block = referral_block(desc).to_lowercase();
    assert!(
        block.contains("if your client has"),
        "the description must hedge that nucl-parquet-mcp is a SEPARATE server \
         the client may not have connected, rather than instructing a call \
         into the void. Got:\n{block}"
    );
}

// ─── The identity on the response side ──────────────────────────────────────

/// A client may have read `tools/list` many turns ago, or cached it. The turn
/// in which the agent decides whether to follow the referral is the one that
/// returns the summary, so the identity has to be in the response too.
#[test]
fn list_reaction_channels_response_carries_the_identity() {
    let db = store();
    let mut materials = MaterialRegistry::new();
    let out = call_tool(
        &db,
        &mut materials,
        "list_reaction_channels",
        &json!({ "projectile": "p", "target_z": 29, "target_a": 63 }),
    )
    .expect("list_reaction_channels on Cu-63 should succeed");

    let body = body_without_footer(&out.text);
    assert!(
        body.contains(data_version()),
        "the response referral must name the live data release `{}` in the \
         body — not merely in the footer, which is stripped here. Got:\n{body}",
        data_version(),
    );
    assert!(
        body.contains(db.library()),
        "the response referral must name the live library `{}` in the body, \
         got:\n{body}",
        db.library(),
    );

    // The operational half lives HERE rather than in the description, so that
    // a caller who never drills down never pays for it. Which means the
    // response is the only place these can be asserted.
    for needle in ["get_cross_sections", "library", "element"] {
        assert!(
            body.contains(needle),
            "the response referral must mention `{needle}` so the hand-off can \
             be acted on without guessing — upstream takes `library` as a \
             REQUIRED argument and keys on `element`, not (Z, A). Got:\n{body}"
        );
    }
    assert!(
        body.to_lowercase().contains("independent"),
        "the response referral must say the other server resolves its data \
         independently — that is WHY a mismatch is possible at all. Got:\n{body}"
    );
}

/// The empty branch needs the referral MORE than the populated one: "this
/// library has no such channel" is exactly when an agent goes looking
/// upstream, and exactly when it must not silently land on different data
/// (cf. #488, where a target that resolves to nothing here has data upstream).
#[test]
fn the_empty_channel_list_still_carries_the_identity() {
    let db = store();
    let mut materials = MaterialRegistry::new();
    // A nuclide no charged-particle sublibrary carries: C-99 does not exist.
    let out = call_tool(
        &db,
        &mut materials,
        "list_reaction_channels",
        &json!({ "projectile": "p", "target_z": 6, "target_a": 99 }),
    )
    .expect("an absent target should return an empty summary, not an error");

    let body = body_without_footer(&out.text);
    assert!(
        body.contains("No cross-section data found"),
        "expected the empty branch for C-99, got:\n{body}"
    );
    assert!(
        body.contains("get_cross_sections")
            && body.contains(data_version())
            && body.contains(db.library()),
        "the empty branch must carry the same referral and data identity as \
         the populated one — it is the branch that sends the agent upstream. \
         Got:\n{body}"
    );
}

/// ADR 0001 introduced the `*Library: <id>*` footer so agents never trust an
/// invisible default. A library id alone does not identify the data, so #681
/// extends it with the release. Checked on a tool that never touches the DB,
/// to prove the footer is applied by the dispatcher for every tool rather than
/// by each handler.
#[test]
fn every_response_footer_carries_library_and_release() {
    let db = store();
    let mut materials = MaterialRegistry::new();
    let out = call_tool(&db, &mut materials, "get_version_info", &json!({}))
        .expect("get_version_info should succeed");
    let expected = format!(
        "*Library: {} · data release: {}*",
        db.library(),
        data_version()
    );
    assert!(
        out.text.contains(&expected),
        "every response must be footed with `{expected}`, got:\n{}",
        out.text
    );
}

/// The server `instructions` are the first thing a fresh client reads. If the
/// identity is only in the referral, an agent that never calls
/// `list_reaction_channels` still carries numbers onward without it.
#[test]
fn server_instructions_state_the_data_release() {
    let text = server_instructions("tendl-2023-iso");
    assert!(
        text.contains(data_version()),
        "server instructions must name the live data release `{}`, got:\n{text}",
        data_version(),
    );
    assert!(
        text.contains("tendl-2023-iso"),
        "server instructions must name the active library, got:\n{text}"
    );
}

// ─── Guard on the guard ─────────────────────────────────────────────────────

/// `find_calver` is only trustworthy if it actually matches what it claims to.
/// A silently-broken scanner would make
/// `no_referral_names_a_release_other_than_the_live_one` vacuous.
#[test]
fn calver_scanner_matches_release_ids_and_nothing_else() {
    assert_eq!(
        find_calver("release 2026.8.2 here").as_deref(),
        Some("2026.8.2")
    );
    assert_eq!(find_calver("v2026.10.15").as_deref(), Some("2026.10.15"));
    for benign in [
        "2026.8.x",          // the sanctioned way to write one in prose
        "tendl-2023-iso",    // library id, not a release
        "ENDF/B-VIII.1",     // evaluation version
        "energy 12.5 MeV",   // a number
        "see #601 and #671", // issue refs
        "12345.6.7",         // five-digit run — not a CalVer year
        "2026.8",            // one component short
        "",
    ] {
        assert_eq!(
            find_calver(benign),
            None,
            "scanner false-positived on {benign:?}"
        );
    }
}

/// Sanity: `list_tools` still returns a parseable surface after the signature
/// change, and no description was accidentally emptied.
#[test]
fn all_tool_descriptions_survive_the_library_injection() {
    for t in list_tools("tendl-2023-iso") {
        let name = t["name"].as_str().unwrap_or("<unnamed>");
        let desc = t["description"].as_str().unwrap_or("");
        assert!(
            !desc.trim().is_empty(),
            "tool `{name}` has an empty description"
        );
        assert!(
            matches!(t.get("inputSchema"), Some(Value::Object(_))),
            "tool `{name}` lost its inputSchema"
        );
    }
}
