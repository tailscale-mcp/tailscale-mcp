//! What a session says it offers is what it offers.
//!
//! Two places describe the selection to somebody: the startup note, which
//! reaches an operator's terminal, and the instructions, which reach the
//! model. Both listed `gate.toolsets()` — what was *asked for* — while the
//! count and the tool listing beside them reported what came of it. In a
//! session without a credential the instructions said "The tailnet surface is
//! not available in this session, so no `tailnet_*` tool is offered" and then,
//! eleven lines later, "Toolsets offered: ..., tailnet-devices, ...". A model
//! reading the second calls a tool the first said was not there.
//!
//! Both now go through `Gate::offered_toolsets`. This holds the property at
//! the seam a caller actually sees, so a third renderer that reaches for the
//! unfiltered list fails here rather than in somebody's session.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use harness::Setup;

/// Every toolset named in either rendering is one with a tool behind it.
#[tokio::test]
async fn a_session_without_a_tailnet_names_no_tailnet_toolset() {
    let harness = Setup::new()
        .preset("full")
        .without_credentials()
        .start()
        .await;

    let offered: Vec<String> = harness
        .tools()
        .await
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect();
    assert!(
        offered.iter().all(|name| !name.starts_with("tailnet_")),
        "the premise of this test is a session with no tailnet tools: {offered:?}"
    );

    let instructions = harness.instructions();
    assert!(
        !instructions.contains("tailnet-"),
        "the instructions name a tailnet toolset in a session that offers none, \
         which is what a model reads before calling one: {instructions}"
    );
}

/// And the converse, so the filter is not simply hiding everything.
#[tokio::test]
async fn a_session_with_a_tailnet_still_names_its_toolsets() {
    let harness = Setup::new().preset("full").start().await;

    let instructions = harness.instructions();
    assert!(
        instructions.contains("tailnet-devices"),
        "a session that offers the tailnet should say so: {instructions}"
    );
    assert!(
        instructions.contains("local-status"),
        "and should not have dropped the local half either: {instructions}"
    );
}
