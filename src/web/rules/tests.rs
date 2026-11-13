//! `docs/tools/web-search.md` *Configuration* and *Measured* §1: the shipped
//! order, the overlay's two merge rules, and both halves of the validation.

use super::*;

/// The shipped file, as the crate embeds it.
const BUILTIN: &str = include_str!("../../../web.toml");

fn builtin() -> Rules {
    parse_rules(BUILTIN, std::path::Path::new("web.toml")).expect("the shipped file parses")
}

// ===== the shipped file =====

#[test]
fn the_shipped_order_is_the_measured_one() {
    // docs/tools/web-search.md *Measured* §1: parallel answered first,
    // anysearch cheapest, tavily needs a header, bing is the scrape tier and
    // exa needs a key today.
    assert_eq!(
        builtin().order,
        ["parallel", "anysearch", "tavily", "bing", "exa"]
    );
}

#[test]
fn the_shipped_file_carries_the_measured_scale_values() {
    let rules = builtin();
    assert_eq!(rules.results, 8);
    assert_eq!(rules.excerpt_chars, 1200);
    assert_eq!(rules.timeout.as_secs(), 20);
    assert_eq!(
        rules.cooldown,
        [30, 60, 120, 360].map(|m| std::time::Duration::from_secs(m * 60))
    );
    // Measured §3: the cap's cost, which is why it is not 2.
    assert_eq!(rules.limits.max_in_flight, 8);
    assert_eq!(rules.limits.max_per_host, 4);
    assert_eq!(rules.limits.requests_per_minute, 60);
}

#[test]
fn tavily_ships_its_keyless_header() {
    // Measured §1: without it the reply is a bare 401 with an empty body.
    let tavily = &builtin().sources["tavily"];
    assert!(
        tavily
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("x-tavily-access-mode") && v == "keyless"),
        "{:?}",
        tavily.headers
    );
}

#[test]
fn the_sources_measured_out_are_not_shipped() {
    // docs/tools/web-search.md §1: firecrawl refuses keyless access, ddg
    // answers a 202 bot check, brave wants a token, searxng wants an address.
    let rules = builtin();
    for absent in ["firecrawl", "duckduckgo", "brave", "searxng", "deepseek"] {
        assert!(
            !rules.sources.contains_key(absent),
            "{absent} should not be configured"
        );
    }
}

#[test]
fn every_configured_source_is_reachable_by_the_walk() {
    // A source in the file must be either in the order or a key/address tier
    // the walk may skip — never stranded with `needs = "nothing"` and no way in.
    let rules = builtin();
    for (id, source) in &rules.sources {
        if source.needs == Needs::Nothing {
            assert!(rules.order.contains(id), "{id} is keyless but not in order");
        }
    }
}

#[test]
fn fetch_order_names_only_sources_that_can_fetch() {
    let rules = builtin();
    assert_eq!(rules.fetch.order, ["parallel", "exa"]);
    for id in &rules.fetch.order {
        assert!(rules.sources[id].kind.fetches(), "{id} cannot fetch");
    }
}

// ===== the merge: scale keys merge, list keys replace =====

fn layered(text: &str) -> (Rules, Vec<RulesIssue>) {
    merge_rules(BUILTIN, text, std::path::Path::new("search.toml")).expect("merges")
}

#[test]
fn a_scale_key_changes_one_value_and_leaves_the_lists() {
    let (rules, issues) = layered("results = 3");
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(rules.results, 3);
    assert_eq!(rules.order, builtin().order, "the order must not move");
    assert_eq!(rules.excerpt_chars, 1200);
}

#[test]
fn a_list_key_replaces_the_whole_list() {
    let (rules, _) = layered(r#"order = ["bing"]"#);
    assert_eq!(rules.order, ["bing"], "a partial edit must not append");
}

#[test]
fn a_source_block_merges_field_by_field() {
    let (rules, issues) = layered(
        r#"
[sources.parallel]
name = "Parallel (mine)"
url = "https://search.parallel.ai/mcp"
"#,
    );
    assert!(issues.is_empty(), "{issues:?}");
    let parallel = &rules.sources["parallel"];
    assert_eq!(parallel.name, "Parallel (mine)");
    assert!(parallel.site_operator, "the unset fields must survive");
}

#[test]
fn adding_a_source_is_free_and_it_can_be_put_first() {
    let (rules, issues) = layered(
        r#"
order = ["mine", "parallel"]
[sources.mine]
name = "Mine"
kind = "anysearch"
url = "https://example.com/search"
needs = "nothing"
"#,
    );
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(rules.order, ["mine", "parallel"]);
    assert_eq!(rules.sources["mine"].kind, Kind::Anysearch);
}

#[test]
fn a_fetch_list_key_replaces_whole_too() {
    let (rules, _) = layered(
        r#"
[fetch]
order = ["exa"]
"#,
    );
    assert_eq!(rules.fetch.order, ["exa"]);
    assert_eq!(rules.fetch.page_chars, 16_000, "the rest must survive");
}

// ===== issues: reported, the session still runs =====

#[test]
fn an_unknown_top_level_key_is_reported_with_the_fix() {
    let (_, issues) = layered("num_results = 3");
    assert_eq!(issues.len(), 1);
    assert_eq!(
        issues[0].0,
        "num_results is not a known key — the key is `results`"
    );
}

#[test]
fn an_unknown_source_field_is_reported_and_the_block_still_loads() {
    let (rules, issues) = layered(
        r#"
[sources.parallel]
typo = 1
"#,
    );
    assert_eq!(
        issues,
        [RulesIssue(
            "sources.parallel.typo is not a known key".into()
        )]
    );
    assert!(
        rules.sources.contains_key("parallel"),
        "the block still loads"
    );
}

#[test]
fn a_source_written_without_its_prefix_is_named() {
    // The `[provider.groq]` hint transplanted: the block is plausible and
    // silently useless, so it must be named rather than ignored.
    let (_, issues) = layered("[sources_dir]\nx = 1");
    assert!(
        issues[0].0.contains("sources_dir is not a known key"),
        "{issues:?}"
    );
}

// ===== refusals: a document that cannot work =====

fn refuse(text: &str) -> String {
    match merge_rules(BUILTIN, text, std::path::Path::new("search.toml")) {
        Ok(_) => panic!("expected a refusal for {text:?}"),
        Err(err) => err.why,
    }
}

#[test]
fn an_order_naming_an_unknown_source_is_refused() {
    assert_eq!(
        refuse(r#"order = ["nope"]"#),
        "order names an unknown source nope"
    );
}

#[test]
fn an_order_naming_one_source_twice_is_refused() {
    // Otherwise a search asks the same provider twice and pays two timeouts
    // for the same answer.
    assert_eq!(
        refuse(r#"order = ["parallel", "parallel"]"#),
        "order names parallel twice"
    );
}

#[test]
fn the_browser_user_agent_falls_back_when_the_file_names_none() {
    let text = BUILTIN.replace(
        &format!(
            "browser_user_agent = \"{}\"\n",
            builtin().browser_user_agent
        ),
        "",
    );
    let rules = parse_rules(&text, std::path::Path::new("web.toml")).expect("parses without it");
    assert_eq!(rules.browser_user_agent, DEFAULT_BROWSER_USER_AGENT);
}

#[test]
fn a_fetch_order_naming_a_kind_that_cannot_fetch_is_refused() {
    // `bing` scrapes rather than fetching a body.
    let why = refuse(
        r#"
[fetch]
order = ["bing"]
"#,
    );
    assert_eq!(why, "fetch.order names bing, which cannot fetch pages");
}

#[test]
fn a_zero_result_count_is_refused() {
    assert!(refuse("results = 0").contains("must be above 0"));
}

#[test]
fn a_zero_page_chars_is_refused() {
    assert!(refuse("[fetch]\npage_chars = 0").contains("fetch needs"));
}

#[test]
fn an_empty_cooldown_ladder_is_refused() {
    assert!(refuse("cooldown_minutes = []").contains("cooldown_minutes"));
}

#[test]
fn a_cooldown_ladder_with_a_zero_step_is_refused() {
    assert!(refuse("cooldown_minutes = [30, 0]").contains("no 0"));
}

#[test]
fn a_source_without_a_url_is_refused() {
    let why = refuse(
        r#"
[sources.mine]
name = "Mine"
kind = "anysearch"
url = ""
needs = "nothing"
"#,
    );
    assert_eq!(why, "sources.mine has no url");
}

#[test]
fn an_address_source_with_a_url_is_refused() {
    let why = refuse(
        r#"
[sources.mine]
name = "Mine"
kind = "searxng"
url = "https://example.com"
needs = "address"
"#,
    );
    assert_eq!(
        why,
        "sources.mine needs an address, so its url must be empty"
    );
}

#[test]
fn an_unknown_kind_is_refused_and_names_the_source() {
    let why = refuse(
        r#"
[sources.mine]
name = "Mine"
kind = "google"
url = "https://example.com"
needs = "nothing"
"#,
    );
    assert!(why.starts_with("sources.mine:"), "{why}");
}

#[test]
fn a_zero_limit_is_refused() {
    assert!(refuse("[limits]\nmax_per_host = 0").contains("limits must all be above 0"));
}

#[test]
fn the_refusal_names_the_file() {
    let err = merge_rules(
        BUILTIN,
        "results = 0",
        std::path::Path::new("/tmp/x/search.toml"),
    )
    .expect_err("refused");
    assert_eq!(
        err.to_string().split(':').next(),
        Some("/tmp/x/search.toml")
    );
}

#[test]
fn broken_toml_is_refused_rather_than_ignored() {
    assert!(merge_rules(BUILTIN, "results = ", std::path::Path::new("s.toml")).is_err());
}

// ===== kinds =====

#[test]
fn kind_reports_what_it_can_do() {
    assert!(Kind::Exa.fetches() && Kind::Parallel.fetches() && Kind::Tavily.fetches());
    assert!(!Kind::Bing.fetches() && !Kind::Anysearch.fetches());
    assert!(Kind::Bing.is_scrape() && !Kind::Exa.is_scrape());
    assert!(Kind::Parallel.is_mcp() && !Kind::Anysearch.is_mcp());
}

#[test]
fn every_kind_round_trips_through_its_own_spelling() {
    let kinds = [
        Kind::Exa,
        Kind::Parallel,
        Kind::Firecrawl,
        Kind::Tavily,
        Kind::Anysearch,
        Kind::Brave,
        Kind::Deepseek,
        Kind::Searxng,
        Kind::Bing,
    ];
    for kind in kinds {
        let value = toml::Value::String(kind.as_str().to_string());
        let back: Kind = value.try_into().unwrap_or_else(|_| panic!("{kind:?}"));
        assert_eq!(back, kind);
    }
}
