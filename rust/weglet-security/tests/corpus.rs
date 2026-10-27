// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Measures assess_navigation against a fixed set of real addresses, so a
// change to the heuristics has a number attached. See corpus.txt.

use weglet_security::{assess_navigation, Lists, RiskLevel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    Ok,
    Warn,
    Block,
}

impl Expect {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "ok" => Some(Self::Ok),
            "warn" => Some(Self::Warn),
            "block" => Some(Self::Block),
            _ => None,
        }
    }
}

fn actual(url: &str) -> Expect {
    match assess_navigation(&Lists::built_in(), url) {
        None => Expect::Ok,
        Some(risk) => match risk.level {
            RiskLevel::Warning => Expect::Warn,
            RiskLevel::Block => Expect::Block,
        },
    }
}

fn entries() -> Vec<(Expect, String)> {
    include_str!("corpus.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (expect, url) = line
                .split_once('\t')
                .unwrap_or_else(|| panic!("corpus line is not <expect>TAB<url>: {line}"));
            let expect = Expect::parse(expect.trim())
                .unwrap_or_else(|| panic!("unknown expectation {expect:?} in: {line}"));
            (expect, url.trim().to_string())
        })
        .collect()
}

// The budget is zero. A legitimate address that gets flagged teaches
// people to click through warnings, which disarms the rest.
#[test]
fn no_false_positives_on_legitimate_addresses() {
    let mut failures = Vec::new();
    let mut total = 0;
    for (expect, url) in entries() {
        if expect != Expect::Ok {
            continue;
        }
        total += 1;
        let got = actual(&url);
        if got != Expect::Ok {
            let risk = assess_navigation(&Lists::built_in(), &url).unwrap();
            failures.push(format!("  {url}\n      {:?} -- {}", got, risk.title));
        }
    }
    if !failures.is_empty() {
        panic!(
            "{} of {total} legitimate addresses were flagged:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

// Missing a phishing address is a quality bug, not a hole: nothing else
// trusts this verdict. Reported as a rate so it can be watched.
#[test]
fn false_negative_rate_stays_within_budget() {
    // Zero today. Raising it is a decision, not a workaround. Signed
    // because `usize <= 0` is a comparison clippy refuses.
    const MAX_MISSED: i64 = 0;

    let mut missed = Vec::new();
    let mut total = 0;
    for (expect, url) in entries() {
        if expect == Expect::Ok {
            continue;
        }
        total += 1;
        if actual(&url) == Expect::Ok {
            missed.push(url);
        }
    }
    eprintln!(
        "false negatives: {} of {total} ({:.1}%)",
        missed.len(),
        missed.len() as f64 / total as f64 * 100.0
    );
    assert!(
        missed.len() as i64 <= MAX_MISSED,
        "{} suspicious addresses passed unflagged (budget {MAX_MISSED}):\n  {}",
        missed.len(),
        missed.join("\n  ")
    );
}

// Severity is its own decision: downgrading a block to a warning should
// be deliberate.
#[test]
fn severity_matches_the_corpus() {
    let mut wrong = Vec::new();
    for (expect, url) in entries() {
        let got = actual(&url);
        if got != expect && got != Expect::Ok {
            wrong.push(format!("  {url}\n      expected {expect:?}, got {got:?}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} addresses came back at the wrong severity:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn the_corpus_is_well_formed() {
    let all = entries();
    assert!(all.len() > 150, "corpus is too small to mean anything");

    let legit = all.iter().filter(|(e, _)| *e == Expect::Ok).count();
    assert!(
        legit > all.len() / 2,
        "most of the corpus should be legitimate addresses, or the \
         false-positive budget measures nothing"
    );

    let mut seen = std::collections::HashSet::new();
    for (_, url) in &all {
        assert!(seen.insert(url.clone()), "duplicate corpus entry: {url}");
    }
}
