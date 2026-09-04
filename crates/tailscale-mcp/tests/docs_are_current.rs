//! The documentation says what the code does, and goes on saying it.
//!
//! Three files here describe things the code decides: which tools exist, which
//! settings there are, and which error codes a caller can meet. All three go
//! stale silently — a tool added and a table not regenerated is documentation
//! that is wrong rather than missing, which is worse. So the tool table is
//! generated from the metadata table and compared, and the other two are held
//! to the lists the code publishes.
//!
//! When `docs/tools.md` no longer matches, regenerate it:
//!
//! ```text
//! UPDATE_DOCS=1 cargo test -p tailscale-mcp --test docs_are_current
//! ```
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod repo;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use tailscale_mcp::config::Cli;
use tailscale_mcp::error::ErrorCode;
use tailscale_mcp::gating::{Gate, Preset};
use tailscale_mcp::meta::{Surface, Tier, ToolMeta, Toolset};
use tailscale_mcp::registry::Registry;

/// The whole metadata table, in the order the tools are declared.
fn all_tools() -> Vec<ToolMeta> {
    Registry::new(tailscale_mcp::tools::entries())
        .expect("the tool table is well formed")
        .metas()
}

/// One documentation file.
fn doc(name: &str) -> String {
    let path = repo::root().join("docs").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|why| panic!("docs/{name}: {why}"))
}

/// The README, which three of these tests read.
fn readme() -> String {
    std::fs::read_to_string(repo::root().join("README.md")).expect("the README")
}

/// A cell of a markdown table, with what would break the table escaped.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

/// The presets a toolset is in, as an operator would write them.
fn presets_with(toolset: Toolset) -> Vec<&'static str> {
    Preset::ALL
        .iter()
        .filter(|preset| preset.toolsets().contains(&toolset))
        .map(|preset| preset.as_str())
        .collect()
}

/// Where a toolset is offered, for the overview table's cell and for the
/// sentence that opens its section. A toolset in no preset is the interesting
/// case and it is spelled out in one place, so the two cannot drift apart.
fn offered_in(toolset: Toolset) -> (String, String) {
    let presets = presets_with(toolset);
    if presets.is_empty() {
        return (
            "none — ask for it by name".to_owned(),
            format!("no preset — `--toolsets +{toolset}` offers it"),
        );
    }
    let plural = if presets.len() == 1 { "" } else { "s" };
    let list = presets.join(", ");
    (list.clone(), format!("the {list} preset{plural}"))
}

/// What is worth saying about a tool beyond its tier.
///
/// Each of these changes what a caller has to do or can expect, and none of
/// them is visible from the name: a tool that asks for `confirm`, one whose
/// tier depends on its arguments, one that only exists on some systems, and
/// one that can cut this server off from what it is driving.
fn notes(tool: &ToolMeta) -> String {
    let mut notes = Vec::new();
    if tool.requires_confirmation {
        notes.push("needs `confirm`".to_owned());
    }
    if tool.varying_tier {
        notes.push("tier depends on arguments".to_owned());
    }
    if let Some(platforms) = tool.platforms {
        notes.push(format!("{} only", platforms.join(", ")));
    }
    if let Some(version) = tool.min_version {
        notes.push(format!("needs tailscale {version}"));
    }
    if tool.self_severing {
        notes.push("can sever this server".to_owned());
    }
    notes.join("; ")
}

/// `docs/tools.md`, rendered from the metadata table.
fn tool_table() -> String {
    let tools = all_tools();
    let mut by_toolset: BTreeMap<Toolset, Vec<&ToolMeta>> = BTreeMap::new();
    for tool in &tools {
        by_toolset.entry(tool.toolset).or_default().push(tool);
    }

    let mut out = String::new();
    out.push_str(
        "<!-- Generated from the tool metadata table. Do not edit by hand; run\n     \
         UPDATE_DOCS=1 cargo test -p tailscale-mcp --test docs_are_current -->\n\n",
    );
    let _ = writeln!(out, "# Tools\n");
    let _ = writeln!(
        out,
        "{} tools in {} toolsets. Which of them a session offers depends on the\n\
         preset, the tier and which surfaces are reachable; see\n\
         [configuration.md](configuration.md).\n",
        tools.len(),
        by_toolset.len()
    );
    let _ = writeln!(out, "| Toolset | Surface | Tools | In presets |");
    let _ = writeln!(out, "|---|---|---|---|");
    for (toolset, members) in &by_toolset {
        let _ = writeln!(
            out,
            "| [`{toolset}`](#{toolset}) | {} | {} | {} |",
            toolset.surface().as_str(),
            members.len(),
            offered_in(*toolset).0
        );
    }

    for (toolset, members) in &by_toolset {
        let _ = writeln!(out, "\n## {toolset}\n");
        let _ = writeln!(
            out,
            "{} tool{} on the {} surface, in {}.\n",
            members.len(),
            if members.len() == 1 { "" } else { "s" },
            toolset.surface().as_str(),
            offered_in(*toolset).1
        );
        let _ = writeln!(out, "| Tool | Tier | Notes | What it does |");
        let _ = writeln!(out, "|---|---|---|---|");
        for tool in members {
            let _ = writeln!(
                out,
                "| `{}` | {} | {} | {} |",
                tool.name,
                tool.tier.as_str(),
                cell(&notes(tool)),
                cell(tool.summary)
            );
        }
    }
    out
}

#[test]
fn the_tool_table_is_the_one_the_code_would_write() {
    let rendered = tool_table();
    let path = repo::root().join("docs").join("tools.md");
    if std::env::var_os("UPDATE_DOCS").is_some() {
        std::fs::write(&path, &rendered).expect("docs/tools.md is writable");
        return;
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        current, rendered,
        "docs/tools.md is stale; regenerate it with \
         `UPDATE_DOCS=1 cargo test -p tailscale-mcp --test docs_are_current`"
    );
}

/// The cells of every table row in a document, in order.
fn rows(document: &str) -> Vec<Vec<String>> {
    document
        .lines()
        .filter(|line| line.trim_start().starts_with('|'))
        .map(|line| {
            line.trim()
                .trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().to_owned())
                .collect()
        })
        .collect()
}

/// The row documenting `name`, found by its first two cells.
fn documented(rows: &[Vec<String>], name: &str) -> Option<Vec<String>> {
    rows.iter()
        .find(|row| {
            row.iter()
                .take(2)
                .any(|cell| cell.contains(&format!("`{name}`")))
        })
        .cloned()
}

#[test]
fn every_setting_is_documented_with_its_default() {
    // The two lists the code publishes for exactly this, and every long flag
    // clap knows about. A setting that reaches the server and not the
    // documentation is one nobody can find; one with no default written down
    // is one nobody can predict.
    let configuration = doc("configuration.md");
    let rows = rows(&configuration);
    assert!(rows.len() > 10, "docs/configuration.md has no table");

    let variables = tailscale_mcp::config::ENV_VARS
        .iter()
        .chain(tailscale_rest::credentials::ENV_VARS)
        .copied();
    let flags: Vec<String> = {
        use clap::CommandFactory;
        Cli::command()
            .get_arguments()
            // `--help` and `--version` are clap's, not settings of this
            // server; they are in the table all the same, with no default to
            // write down.
            .filter_map(|argument| argument.get_long())
            .filter(|long| !matches!(*long, "help" | "version"))
            .map(|long| format!("--{long}"))
            .collect()
    };
    assert!(flags.len() > 5, "clap reported {} flags", flags.len());

    for name in variables.chain(flags.iter().map(String::as_str)) {
        let row = documented(&rows, name)
            .unwrap_or_else(|| panic!("{name} is not in docs/configuration.md"));
        let default = row
            .get(2)
            .unwrap_or_else(|| panic!("{name}'s row has no default column: {row:?}"));
        assert!(!default.is_empty(), "{name} has no default written down");
    }

    // A subcommand's own flags are not settings and have no default to quote —
    // they belong to one question the binary answers rather than to how it
    // serves. But they are still flags somebody has to be told about, so they
    // have to be named somewhere on the page.
    let under_subcommands = subcommand_flags();
    assert!(
        under_subcommands.len() > 1,
        "clap reported {} flags under subcommands",
        under_subcommands.len()
    );
    for (subcommand, flag) in under_subcommands {
        assert!(
            configuration.contains(&flag),
            "`{subcommand} {flag}` is not in docs/configuration.md"
        );
    }
}

/// Every long flag under a subcommand, at any depth, with the subcommand it is
/// under. `--help` is clap's own and is on all of them.
fn subcommand_flags() -> Vec<(String, String)> {
    use clap::CommandFactory;

    fn walk(command: &clap::Command, path: &str, found: &mut Vec<(String, String)>) {
        for sub in command.get_subcommands() {
            let here = if path.is_empty() {
                sub.get_name().to_owned()
            } else {
                format!("{path} {}", sub.get_name())
            };
            for long in sub
                .get_arguments()
                .filter_map(|argument| argument.get_long())
            {
                if !matches!(long, "help" | "version") {
                    found.push((here.clone(), format!("--{long}")));
                }
            }
            walk(sub, &here, found);
        }
    }

    let mut found = Vec::new();
    walk(&Cli::command(), "", &mut found);
    found
}

#[test]
fn every_error_code_is_documented_and_no_others() {
    // A code is what a caller branches on, so a code it cannot look up is a
    // string it has to guess the meaning of — and a code documented but never
    // sent is one somebody writes a branch for that will never be taken. The
    // table is the whole set, so it is compared as a set.
    let errors = doc("errors.md");
    let table = errors
        .split_once("## The codes")
        .expect("docs/errors.md has a table of codes")
        .1;
    let documented: BTreeSet<&str> = table
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|row| row.split('`').next())
        .collect();
    let sent: BTreeSet<&str> = ErrorCode::ALL.iter().map(|code| code.as_str()).collect();
    assert_eq!(
        documented, sent,
        "docs/errors.md and ErrorCode disagree about which codes exist"
    );
}

/// How many of these tools a preset offers at a tier, the way the server
/// decides it.
fn offered(tools: &[ToolMeta], preset: Preset, tier: Tier) -> usize {
    let gate = Gate::unchecked(preset.toolsets(), tier, BTreeSet::new());
    tools.iter().filter(|meta| gate.permits(meta)).count()
}

#[test]
fn the_counts_the_readme_quotes_are_the_counts() {
    // The README's table of what each preset offers is the first thing anybody
    // reads about the tier model, and it is nine numbers nothing else checks.
    let readme = readme();
    let rows = rows(&readme);
    let tools = all_tools();
    for preset in Preset::ALL {
        let row = rows
            .iter()
            .find(|row| {
                row.first()
                    .is_some_and(|cell| cell.contains(preset.as_str()))
            })
            .unwrap_or_else(|| panic!("the README has no row for the {preset} preset"));
        for (column, tier) in [Tier::Read, Tier::Write, Tier::Destructive]
            .iter()
            .enumerate()
        {
            let counted = offered(&tools, *preset, *tier).to_string();
            let written = row
                .get(column + 1)
                .unwrap_or_else(|| panic!("the {preset} row has no column for {tier:?}"));
            assert!(
                written.contains(&counted),
                "the README says {written} tools for {preset} at the {} tier; there are {counted}",
                tier.as_str()
            );
        }
        // The last column is how many toolsets the preset selects, which is
        // the other half of the same claim.
        let counted = preset.toolsets().len().to_string();
        let written = row
            .get(4)
            .unwrap_or_else(|| panic!("the {preset} row has no toolset column"));
        assert!(
            written.contains(&counted),
            "the README says {written} toolsets for {preset}; there are {counted}"
        );
    }
}

#[test]
fn every_count_the_readme_links_is_the_count() {
    // The comparison table's own column is a list of numbers, and a number
    // beside a link is one somebody will read as current. Each is written as
    // `[N tools](docs/tools.md#<toolset>)`, so this reads them back: a toolset
    // that grows and a README that does not is a failure here rather than a
    // quiet exaggeration in the one table that makes a claim about the others.
    let readme = readme();
    let tools = all_tools();
    const LINK: &str = "](docs/tools.md";
    let mut checked = 0;
    let mut unread = readme.as_str();
    while let Some(at) = unread.find(LINK) {
        let (before, after) = unread.split_at(at);
        // Everything past the file name: the fragment where there is one, and
        // then the rest of the README, which is where the search goes on.
        let fragment_onward = &after[LINK.len()..];
        unread = fragment_onward;
        // The link's own text, and the number it starts with. A link whose
        // text is not a count — the plain pointers to the table — is prose.
        let text = before.rsplit_once('[').map_or("", |(_, text)| text);
        let Some(Ok(written)) = text.split_whitespace().next().map(str::parse::<usize>) else {
            continue;
        };
        let (subject, counted) = match fragment_onward.strip_prefix('#') {
            // A link to the whole table is the total.
            None => ("the server in all".to_owned(), tools.len()),
            Some(fragment) => {
                let name = fragment
                    .split(')')
                    .next()
                    .expect("a fragment ends before the bracket");
                let toolset =
                    Toolset::parse(name).unwrap_or_else(|| panic!("no toolset named {name}"));
                (
                    name.to_owned(),
                    tools.iter().filter(|t| t.toolset == toolset).count(),
                )
            }
        };
        assert_eq!(
            written, counted,
            "the README says {written} tools for {subject}; there are {counted}"
        );
        checked += 1;
    }
    assert!(
        checked > 15,
        "only {checked} counts were checked; the README's links changed shape"
    );
}

#[test]
fn the_readme_names_the_four_features_this_server_does_not_have() {
    // The superset claim is only honest if the exceptions are written down
    // where the claim is made (spec, "Superset").
    let readme = readme();
    for exception in [
        "configuration file",
        "tool-schema resource",
        "OAuth resource-server",
        "environment knob",
    ] {
        assert!(
            readme.contains(exception),
            "the README does not say that there is no {exception}"
        );
    }
}

#[test]
fn the_surfaces_are_named_the_way_the_documentation_names_them() {
    // `tool_table` prints a surface into a heading; this is the check that it
    // stays a word rather than a debug spelling.
    assert_eq!(Surface::Local.as_str(), "local");
    assert_eq!(Surface::Tailnet.as_str(), "tailnet");
}
