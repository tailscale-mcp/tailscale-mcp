//! Every tool, checked the same way.
//!
//! The point of a table-driven contract is that adding a tool is not enough:
//! the tool has to say what a successful call and a failed call look like, or
//! the suite fails. What is asserted here is what a client can see — the tier,
//! the toolset, the annotations, and one call of each kind through a real
//! session — so a tool cannot pass by being right on the inside.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use std::collections::BTreeSet;

use serde_json::{Value, json};
use tailscale_cli::stub::Reply;
use tailscale_mcp::gating::Preset;
use tailscale_mcp::meta::{Tier, ToolMeta};
use tailscale_rest::fake::Response;

use harness::Setup;

/// What one tool needs to be answered with for one call.
#[derive(Default)]
struct Arrangement {
    /// Answers the fake `tailscale` should give, matched on an argument prefix.
    cli: Vec<(Vec<&'static str>, Reply)>,
    /// Answers the fake control plane should give.
    api: Vec<(&'static str, &'static str, Response)>,
}

impl Arrangement {
    fn cli(mut self, argv: &[&'static str], reply: Reply) -> Self {
        self.cli.push((argv.to_vec(), reply));
        self
    }

    #[expect(
        dead_code,
        reason = "used by the tailnet tools, which land in ticket 17"
    )]
    fn api(mut self, method: &'static str, path: &'static str, response: Response) -> Self {
        self.api.push((method, path, response));
        self
    }
}

/// One tool's contract: what it does when it works, and when it does not.
struct Contract {
    tool: &'static str,
    /// A call that should succeed, and what the world looks like when it does.
    success: (Value, Arrangement),
    /// A call that should fail, and the code it should fail with.
    failure: (Value, Arrangement, &'static str),
}

/// The contract for every tool in the table.
///
/// A tool with no row here fails [`every_tool_has_a_contract`], which is the
/// mechanism that keeps this list complete as the table grows.
fn contracts() -> Vec<Contract> {
    vec![Contract {
        tool: "tailscale_version",
        success: (
            json!({}),
            Arrangement::default().cli(
                &["version"],
                Reply::ok(harness::fixture("tailscale-version.txt")),
            ),
        ),
        failure: (
            json!({}),
            Arrangement::default().cli(
                &["version"],
                Reply::failed(1, "failed to connect to local tailscaled"),
            ),
            "cli_failed",
        ),
    }]
}

/// Build a session in which `meta`'s tool is on offer, arranged as the case says.
async fn session(meta: &ToolMeta, arrangement: &Arrangement) -> harness::Harness {
    let mut setup = Setup::new().toolsets(meta.toolset.as_str()).tier(meta.tier);
    for (argv, reply) in &arrangement.cli {
        setup = setup.cli_answers(argv, reply.clone());
    }
    for (method, path, response) in &arrangement.api {
        setup = setup.api_answers(method, path, response.clone()).await;
    }
    setup.start().await
}

/// The arguments a call is made with, plus the confirmation the tool requires.
fn arguments(meta: &ToolMeta, args: &Value) -> Value {
    let mut args = args.clone();
    if meta.requires_confirmation
        && let Some(object) = args.as_object_mut()
    {
        object.insert("confirm".to_owned(), json!(true));
    }
    args
}

fn table() -> Vec<ToolMeta> {
    tailscale_mcp::tools::entries()
        .into_iter()
        .map(|e| e.meta)
        .collect()
}

fn contract_for(name: &str) -> Contract {
    contracts()
        .into_iter()
        .find(|c| c.tool == name)
        .unwrap_or_else(|| panic!("no contract for `{name}`"))
}

#[test]
fn every_tool_has_a_contract() {
    let declared: BTreeSet<&str> = table().iter().map(|m| m.name).collect();
    let covered: BTreeSet<&str> = contracts().iter().map(|c| c.tool).collect();

    let uncovered: Vec<&&str> = declared.difference(&covered).collect();
    assert!(
        uncovered.is_empty(),
        "these tools have no contract row, so nothing checks what they do: {uncovered:?}"
    );

    let invented: Vec<&&str> = covered.difference(&declared).collect();
    assert!(
        invented.is_empty(),
        "these contract rows name tools that do not exist: {invented:?}"
    );
}

#[tokio::test]
async fn every_tool_is_named_for_the_surface_it_acts_on() {
    for meta in table() {
        assert!(
            meta.name.starts_with(meta.surface().prefix()),
            "`{}` belongs to the {} surface but is not named for it",
            meta.name,
            meta.surface().as_str()
        );
        assert_eq!(
            meta.toolset.surface(),
            meta.surface(),
            "`{}` is in a toolset from another surface",
            meta.name
        );
    }
}

#[tokio::test]
async fn every_tool_describes_itself_the_way_its_tier_says() {
    for meta in table() {
        let harness = session(&meta, &Arrangement::default()).await;
        let tool = harness
            .tool(meta.name)
            .await
            .unwrap_or_else(|| panic!("`{}` is not offered by its own toolset", meta.name));

        assert!(
            tool.description.as_deref().is_some_and(|d| !d.is_empty()),
            "`{}` has no description, so a model cannot choose it",
            meta.name
        );

        let annotations = tool
            .annotations
            .unwrap_or_else(|| panic!("`{}` is not annotated", meta.name));
        assert_eq!(
            annotations.read_only_hint,
            Some(meta.tier == Tier::Read),
            "`{}` is at the {} tier",
            meta.name,
            meta.tier
        );
        assert_eq!(
            annotations.destructive_hint,
            Some(meta.tier == Tier::Destructive),
            "`{}` is at the {} tier",
            meta.name,
            meta.tier
        );
        assert_eq!(
            annotations.idempotent_hint,
            Some(meta.idempotent),
            "`{}` declares idempotent: {}",
            meta.name,
            meta.idempotent
        );
        assert_eq!(annotations.open_world_hint, Some(true), "{}", meta.name);

        harness.shutdown().await;
    }
}

#[tokio::test]
async fn every_tool_answers_its_success_case() {
    for meta in table() {
        let contract = contract_for(meta.name);
        let (args, arrangement) = contract.success;
        let harness = session(&meta, &arrangement).await;

        let answer = harness.call_ok(meta.name, arguments(&meta, &args)).await;
        assert!(
            answer.is_object(),
            "`{}` answered with something a client cannot destructure: {answer}",
            meta.name
        );

        harness.shutdown().await;
    }
}

#[tokio::test]
async fn every_tool_answers_its_failure_case_with_the_code_it_promised() {
    for meta in table() {
        let contract = contract_for(meta.name);
        let (args, arrangement, code) = contract.failure;
        let harness = session(&meta, &arrangement).await;

        let error = harness.call_err(meta.name, arguments(&meta, &args)).await;
        assert_eq!(
            error["code"], code,
            "`{}` failed with the wrong code: {error:#}",
            meta.name
        );
        assert!(
            error["message"].as_str().is_some_and(|m| !m.is_empty()),
            "`{}` failed without a message: {error:#}",
            meta.name
        );

        harness.shutdown().await;
    }
}

#[tokio::test]
async fn every_tool_that_needs_confirming_refuses_without_it() {
    for meta in table().into_iter().filter(|m| m.requires_confirmation) {
        let contract = contract_for(meta.name);
        let (args, arrangement) = contract.success;
        let harness = session(&meta, &arrangement).await;

        // The same call that succeeds above, minus the confirmation.
        let error = harness.call_err(meta.name, args).await;
        assert_eq!(
            error["code"], "confirmation_required",
            "`{}` ran without being confirmed",
            meta.name
        );

        harness.shutdown().await;
    }
}

#[tokio::test]
async fn each_preset_offers_more_than_the_one_below_it() {
    // The presets are meant to nest, so an operator moving up never loses a
    // tool they had. Checked against the real table, at every tier.
    for tier in [Tier::Read, Tier::Write, Tier::Destructive] {
        let mut previous: Option<(&str, BTreeSet<String>)> = None;
        for preset in Preset::ALL {
            let harness = Setup::new()
                .preset(preset.as_str())
                .tier(tier)
                .start()
                .await;
            let offered: BTreeSet<String> = harness.tool_names().await.into_iter().collect();

            if let Some((smaller, below)) = &previous {
                let lost: Vec<&String> = below.difference(&offered).collect();
                assert!(
                    lost.is_empty(),
                    "moving from {smaller} to {} at the {tier} tier loses {lost:?}",
                    preset.as_str()
                );
            }
            previous = Some((preset.as_str(), offered));
            harness.shutdown().await;
        }
    }
}

#[tokio::test]
async fn no_tool_is_reachable_from_a_session_that_did_not_ask_for_its_toolset() {
    // A session that selected some other toolset cannot see or call this tool,
    // even at the destructive tier. A zero-tool session cannot start, so the
    // session is built from every *other* toolset that has tools in it.
    let occupied: BTreeSet<&str> = table().iter().map(|m| m.toolset.as_str()).collect();

    for meta in table() {
        let elsewhere: Vec<&str> = occupied
            .iter()
            .copied()
            .filter(|t| *t != meta.toolset.as_str())
            .collect();
        if elsewhere.is_empty() {
            // Nothing else has tools yet, so there is no session to build.
            continue;
        }

        let harness = Setup::new()
            .toolsets(&elsewhere.join(","))
            .tier(Tier::Destructive)
            .start()
            .await;

        assert!(
            harness.tool(meta.name).await.is_none(),
            "`{}` is offered by a session that did not select {}",
            meta.name,
            meta.toolset.as_str()
        );
        let error = harness.call_err(meta.name, json!({})).await;
        assert_eq!(error["code"], "not_permitted", "{}", meta.name);

        harness.shutdown().await;
    }
}
