use std::path::Path;
use traci::{analyze_source, Config, Rule};

#[test]
fn bad_fixture_exposes_dead_zones() {
    let source = include_str!("fixtures/bad.rs");
    let findings = analyze_source(Path::new("bad.rs"), source, &Config::default());
    assert!(findings
        .iter()
        .any(|finding| finding.rule == Rule::UntracedError));
    assert!(findings
        .iter()
        .any(|finding| finding.rule == Rule::SwallowedResult));
    assert!(findings
        .iter()
        .any(|finding| finding.rule == Rule::DetachedAsyncTask));
}

#[test]
fn good_fixture_is_reconstructable() {
    let source = include_str!("fixtures/good.rs");
    let config = Config {
        require_correlation_id: true,
        ..Config::default()
    };
    let findings = analyze_source(Path::new("good.rs"), source, &config);
    assert!(findings.is_empty(), "unexpected findings: {findings:#?}");
}
