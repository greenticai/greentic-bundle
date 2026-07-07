//! Integration tests for the agent-pack auto-wiring resolution pass (Task 5).
//!
//! These tests exercise `auto_wire_agent_packs` end-to-end:
//!
//! * `store://` path — a local `httpmock` server serves a fake `.gtpack` with a
//!   `dw-agents.json` sidecar; the pack must land in `packs_dir`.
//! * `file://` path — reads a pre-written temporary `.gtpack` from disk; the pack
//!   must land in `packs_dir`.
//! * Error path — a referenced agent with no `agent_packs` mapping must return
//!   `Err` whose message contains `"has no agent_packs mapping"`.
//! * Error path — a referenced agent whose fetched pack does NOT provide it must
//!   return `Err` naming the mismatch.
//!
//! The `GREENTIC_STORE_URL` env var is set inside tests that exercise the
//! `store://` path.  A `Mutex` serialises any concurrent tests that touch that
//! env var so they cannot race.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;
use std::sync::Mutex;

use greentic_bundle::project::BundleWorkspaceDefinition;
use greentic_bundle::project::agent_wiring::auto_wire_agent_packs;
use greentic_distributor_client::signing::TrustRoot;
use sha2::{Digest, Sha256};

/// Guard for tests that mutate `GREENTIC_STORE_URL`.
static ENV_LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

/// Build a minimal `manifest.cbor` CBOR bytes that contain a single flow with a
/// `dw.agent` node whose `operation` is `agent_id`.
fn flow_manifest_cbor(flow_id: &str, node_id: &str, agent_id: &str) -> Vec<u8> {
    use ciborium::Value;
    fn text(s: &str) -> Value {
        Value::Text(s.to_string())
    }
    fn int(n: i64) -> Value {
        Value::Integer(ciborium::value::Integer::from(n))
    }
    fn cbor_map(pairs: Vec<(&str, Value)>) -> Value {
        Value::Map(
            pairs
                .into_iter()
                .map(|(k, v)| (Value::Text(k.to_string()), v))
                .collect(),
        )
    }
    fn cbor_array(items: Vec<Value>) -> Value {
        Value::Array(items)
    }

    let symbols = cbor_map(vec![
        ("component_ids", cbor_array(vec![text("dw.agent")])),
        ("node_ids", cbor_array(vec![text(node_id)])),
    ]);
    let node = cbor_map(vec![
        ("id", int(0)),
        (
            "component",
            cbor_map(vec![("id", int(0)), ("operation", text(agent_id))]),
        ),
    ]);
    let flow = cbor_map(vec![
        ("id", text(flow_id)),
        ("flow", cbor_map(vec![("nodes", cbor_array(vec![node]))])),
    ]);
    let manifest = cbor_map(vec![
        ("symbols", symbols),
        ("flows", cbor_array(vec![flow])),
    ]);
    let mut bytes = Vec::new();
    ciborium::into_writer(&manifest, &mut bytes).expect("cbor serialization must succeed");
    bytes
}

/// Build a minimal `.gtpack` ZIP archive in memory.
///
/// The archive contains:
/// * `manifest.cbor` — an empty CBOR map (the agent-pack format; the runtime
///   reads agents from `dw-agents.json`, not the manifest).
/// * `dw-agents.json` — a JSON object `{ "<agent_id>": {} }` (minimal sidecar).
fn agent_pack_zip(agent_id: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip_writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        // minimal manifest.cbor (empty CBOR map)
        let mut cbor_bytes = Vec::new();
        ciborium::into_writer(&ciborium::Value::Map(vec![]), &mut cbor_bytes).expect("cbor write");
        zip_writer
            .start_file("manifest.cbor", options)
            .expect("zip start_file manifest.cbor");
        zip_writer
            .write_all(&cbor_bytes)
            .expect("zip write manifest.cbor");

        // dw-agents.json sidecar
        let sidecar: BTreeMap<String, serde_json::Value> =
            [(agent_id.to_string(), serde_json::json!({}))]
                .into_iter()
                .collect();
        let sidecar_bytes = serde_json::to_vec(&sidecar).expect("serde_json sidecar");
        zip_writer
            .start_file("dw-agents.json", options)
            .expect("zip start_file dw-agents.json");
        zip_writer
            .write_all(&sidecar_bytes)
            .expect("zip write dw-agents.json");

        zip_writer.finish().expect("zip finish");
    }
    buf
}

/// Build a minimal `BundleWorkspaceDefinition` with a single `agent_packs` entry.
fn workspace_def(agent_id: &str, coordinate: &str) -> BundleWorkspaceDefinition {
    let mut agent_packs = BTreeMap::new();
    agent_packs.insert(agent_id.to_string(), coordinate.to_string());
    BundleWorkspaceDefinition {
        schema_version: 1,
        bundle_id: "test-bundle".to_string(),
        bundle_name: "Test Bundle".to_string(),
        locale: "en".to_string(),
        mode: "create".to_string(),
        advanced_setup: false,
        app_packs: vec![],
        agent_packs,
        app_pack_mappings: vec![],
        extension_providers: vec![],
        remote_catalogs: vec![],
        hooks: vec![],
        subscriptions: vec![],
        capabilities: vec![],
        setup_execution_intent: false,
        export_intent: false,
    }
}

/// Check whether a given path in the `packs_dir` exists, ignoring version fragments.
fn pack_exists(packs_dir: &Path, name_fragment: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(packs_dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|e| e.file_name().to_string_lossy().contains(name_fragment))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Happy path: a `store://` coordinate is fetched from a mock server, verified
/// (sha256 only — empty trust root), and materialised into `packs_dir`.
/// The return value must include `"tavily_researcher"`.
#[test]
fn store_coordinate_fetches_and_materialises_pack() {
    let Some(server) = std::panic::catch_unwind(httpmock::MockServer::start).ok() else {
        eprintln!("skipping: unable to bind mock server");
        return;
    };

    let agent_id = "tavily_researcher";
    let pack_bytes = agent_pack_zip(agent_id);
    let sha = hex::encode(Sha256::digest(&pack_bytes));

    server.mock(|when, then| {
        when.method(httpmock::Method::GET)
            .path("/api/v1/agentic-workers/greentic.agentic-research-tavily-agent/0.1.0/artifact");
        then.status(200)
            .header("x-artifact-sha256", &sha)
            .body(&pack_bytes);
    });

    let temp = tempfile::tempdir().expect("tempdir");
    let packs_dir = temp.path().join("packs");
    let cache_dir = temp.path().join("cache");
    std::fs::create_dir_all(&packs_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cbor = flow_manifest_cbor("on_message", "research", agent_id);
    let def = workspace_def(
        agent_id,
        "store://greentic.agentic-research-tavily-agent@0.1.0",
    );
    let trust = TrustRoot::default(); // sha256-only

    // Point auto_wire_agent_packs at the mock server.
    let _guard = ENV_LOCK.lock().expect("env lock");
    // SAFETY: tests run in a process; we hold the mutex so no other test races.
    unsafe {
        std::env::set_var("GREENTIC_STORE_URL", server.base_url());
    }

    let result = auto_wire_agent_packs(
        &def,
        &[cbor.as_slice()],
        &[],
        &packs_dir,
        &cache_dir,
        false,
        &trust,
    );

    unsafe {
        std::env::remove_var("GREENTIC_STORE_URL");
    }

    let materialized = result.expect("auto_wire_agent_packs should succeed");
    assert!(
        materialized.contains(&agent_id.to_string()),
        "return value must include the materialised agent_id; got: {materialized:?}"
    );
    assert!(
        pack_exists(&packs_dir, "greentic"),
        "pack file must have been written to packs_dir; dir contents: {:?}",
        std::fs::read_dir(&packs_dir)
            .map(|rd| rd.flatten().map(|e| e.file_name()).collect::<Vec<_>>())
            .unwrap_or_default()
    );
}

/// Happy path: a `file://` coordinate is read from a pre-written temp file and
/// materialised into `packs_dir`.
#[test]
fn file_coordinate_materialises_pack() {
    let agent_id = "local_agent";
    let pack_bytes = agent_pack_zip(agent_id);

    let temp = tempfile::tempdir().expect("tempdir");
    let source_pack = temp.path().join("local-agent.gtpack");
    std::fs::write(&source_pack, &pack_bytes).expect("write source pack");

    let packs_dir = temp.path().join("packs");
    let cache_dir = temp.path().join("cache");
    std::fs::create_dir_all(&packs_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cbor = flow_manifest_cbor("flow1", "node1", agent_id);
    let coordinate = format!("file://{}", source_pack.display());
    let def = workspace_def(agent_id, &coordinate);
    let trust = TrustRoot::default();

    let materialized = auto_wire_agent_packs(
        &def,
        &[cbor.as_slice()],
        &[],
        &packs_dir,
        &cache_dir,
        false,
        &trust,
    )
    .expect("auto_wire_agent_packs should succeed for file:// coordinate");

    assert!(
        materialized.contains(&agent_id.to_string()),
        "return value must include the materialised agent_id; got: {materialized:?}"
    );
    assert!(
        pack_exists(&packs_dir, "local"),
        "pack file must have been written to packs_dir"
    );
}

/// Already-provided agents must not be re-fetched.
#[test]
fn already_provided_agent_is_skipped() {
    let agent_id = "already_there";
    let sidecar: BTreeMap<String, serde_json::Value> =
        [(agent_id.to_string(), serde_json::json!({}))]
            .into_iter()
            .collect();
    let sidecar_bytes = serde_json::to_vec(&sidecar).expect("sidecar json");

    let temp = tempfile::tempdir().expect("tempdir");
    let packs_dir = temp.path().join("packs");
    let cache_dir = temp.path().join("cache");
    std::fs::create_dir_all(&packs_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cbor = flow_manifest_cbor("flow1", "node1", agent_id);
    let def = workspace_def(agent_id, "store://should-not-be-fetched@0.0.0");
    let trust = TrustRoot::default();

    // Pass the sidecar as already-provided — the function must return an empty
    // materialized list without contacting the store.
    let materialized = auto_wire_agent_packs(
        &def,
        &[cbor.as_slice()],
        &[sidecar_bytes.as_slice()], // already provided
        &packs_dir,
        &cache_dir,
        false,
        &trust,
    )
    .expect("auto_wire_agent_packs should succeed when agent is already provided");

    assert!(
        materialized.is_empty(),
        "already-provided agent must not appear in the materialized list; got: {materialized:?}"
    );
}

/// Error case: a flow references an agent that has NO mapping in `agent_packs`.
/// The function must return `Err` whose message contains `"has no agent_packs mapping"`.
#[test]
fn missing_mapping_returns_actionable_error() {
    let temp = tempfile::tempdir().expect("tempdir");
    let packs_dir = temp.path().join("packs");
    let cache_dir = temp.path().join("cache");
    std::fs::create_dir_all(&packs_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cbor = flow_manifest_cbor("demo_flow", "node1", "unmapped_agent");

    // Workspace with NO agent_packs entries — unmapped_agent has no mapping.
    let def = BundleWorkspaceDefinition {
        schema_version: 1,
        bundle_id: "test".to_string(),
        bundle_name: "Test".to_string(),
        locale: "en".to_string(),
        mode: "create".to_string(),
        advanced_setup: false,
        app_packs: vec![],
        agent_packs: BTreeMap::new(), // no mapping
        app_pack_mappings: vec![],
        extension_providers: vec![],
        remote_catalogs: vec![],
        hooks: vec![],
        subscriptions: vec![],
        capabilities: vec![],
        setup_execution_intent: false,
        export_intent: false,
    };
    let trust = TrustRoot::default();

    let err = auto_wire_agent_packs(
        &def,
        &[cbor.as_slice()],
        &[],
        &packs_dir,
        &cache_dir,
        false,
        &trust,
    )
    .expect_err("a referenced agent with no mapping must produce an error");

    let message = err.to_string();
    assert!(
        message.contains("has no agent_packs mapping"),
        "error message must mention the missing mapping; got: {message}"
    );
    assert!(
        message.contains("unmapped_agent"),
        "error message must name the missing agent_id; got: {message}"
    );
}

/// Error case: fetched pack does NOT contain a `dw-agents.json` sidecar that
/// provides the expected agent_id.  `auto_wire_agent_packs` must bail with a
/// message naming the mismatch.
#[test]
fn mismatch_agent_id_in_pack_returns_error() {
    let agent_id_in_pack = "wrong_agent";
    let agent_id_in_mapping = "right_agent";

    let pack_bytes = agent_pack_zip(agent_id_in_pack); // pack provides wrong_agent

    let temp = tempfile::tempdir().expect("tempdir");
    let source_pack = temp.path().join("wrong.gtpack");
    std::fs::write(&source_pack, &pack_bytes).expect("write source pack");

    let packs_dir = temp.path().join("packs");
    let cache_dir = temp.path().join("cache");
    std::fs::create_dir_all(&packs_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cbor = flow_manifest_cbor("flow1", "node1", agent_id_in_mapping);
    let coordinate = format!("file://{}", source_pack.display());
    let def = workspace_def(agent_id_in_mapping, &coordinate);
    let trust = TrustRoot::default();

    let err = auto_wire_agent_packs(
        &def,
        &[cbor.as_slice()],
        &[],
        &packs_dir,
        &cache_dir,
        false,
        &trust,
    )
    .expect_err("pack that does not provide the expected agent_id must produce an error");

    let message = err.to_string();
    assert!(
        message.contains(agent_id_in_mapping),
        "error must name the expected agent_id; got: {message}"
    );
}

/// Idempotency: a second `auto_wire_agent_packs` call against the same `packs_dir`
/// that already contains the materialized pack must return an empty list and must
/// NOT attempt to re-fetch the coordinate.
///
/// The test removes the original `file://` source after the first run so that any
/// attempt to re-read it would produce an I/O error — proving that the second run
/// relies on `packs_dir` alone.
#[test]
fn idempotent_second_run_skips_already_materialized() {
    let agent_id = "idempotency_agent";
    let pack_bytes = agent_pack_zip(agent_id);

    let temp = tempfile::tempdir().expect("tempdir");
    let source_pack = temp.path().join("idempotency-agent.gtpack");
    std::fs::write(&source_pack, &pack_bytes).expect("write source pack");

    let packs_dir = temp.path().join("packs");
    let cache_dir = temp.path().join("cache");
    std::fs::create_dir_all(&packs_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    let cbor = flow_manifest_cbor("flow1", "node1", agent_id);
    let coordinate = format!("file://{}", source_pack.display());
    let def = workspace_def(agent_id, &coordinate);
    let trust = TrustRoot::default();

    // First run: should materialize the pack into packs_dir.
    let first_run = auto_wire_agent_packs(
        &def,
        &[cbor.as_slice()],
        &[],
        &packs_dir,
        &cache_dir,
        false,
        &trust,
    )
    .expect("first auto_wire run should succeed");

    assert!(
        first_run.contains(&agent_id.to_string()),
        "first run must materialize the agent pack; got: {first_run:?}"
    );

    // Remove the source so any re-fetch attempt would fail with an I/O error.
    std::fs::remove_file(&source_pack).expect("remove source pack to force packs_dir-only reuse");

    // Second run: packs_dir already contains the pack — must return empty materialized.
    let second_run = auto_wire_agent_packs(
        &def,
        &[cbor.as_slice()],
        &[],
        &packs_dir,
        &cache_dir,
        false,
        &trust,
    )
    .expect("second auto_wire run must succeed (pack already in packs_dir)");

    assert!(
        second_run.is_empty(),
        "second run must return empty — nothing to re-fetch; got: {second_run:?}"
    );
}

/// Offline reuse: a pack already present in `packs_dir` (from a previous online run)
/// must satisfy the agent reference when `offline=true`, so no network call is made
/// and the function succeeds.
#[test]
fn offline_reuse_of_already_materialized_pack_succeeds() {
    let agent_id = "offline_reuse_agent";
    let pack_bytes = agent_pack_zip(agent_id);

    let temp = tempfile::tempdir().expect("tempdir");
    let packs_dir = temp.path().join("packs");
    let cache_dir = temp.path().join("cache");
    std::fs::create_dir_all(&packs_dir).unwrap();
    std::fs::create_dir_all(&cache_dir).unwrap();

    // Pre-place the pack in packs_dir, simulating a previous successful online run.
    let pack_path = packs_dir.join("offline-reuse-agent.gtpack");
    std::fs::write(&pack_path, &pack_bytes).expect("pre-place pack in packs_dir");

    let cbor = flow_manifest_cbor("flow1", "node1", agent_id);
    // A store coordinate that would fail if fetched (no server + offline=true).
    let def = workspace_def(agent_id, "store://greentic.offline-reuse-agent@1.0.0");
    let trust = TrustRoot::default();

    let materialized = auto_wire_agent_packs(
        &def,
        &[cbor.as_slice()],
        &[],
        &packs_dir,
        &cache_dir,
        true, // offline — any network attempt would fail
        &trust,
    )
    .expect("offline auto_wire must succeed when pack is already in packs_dir");

    assert!(
        materialized.is_empty(),
        "already-materialized pack must not be re-fetched; got: {materialized:?}"
    );
}
