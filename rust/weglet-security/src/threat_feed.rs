// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// OpenPhish's public feed, reduced to URL indicators and checked locally:
// a browsing URL is never sent anywhere. Kept as plain text: the feed is
// public, so hashing it hid nothing, and the check is local either way.
//
// The fetch is the browser process's job -- it already has a network
// stack. This module only ever sees text the browser already downloaded.

use std::collections::BTreeSet;

const MAX_FEED_LINES: usize = 50_000;
const MIN_INDICATORS: usize = 50;

// Wording lives here, not in the browser process: the page renders
// whatever it is handed and has no words of its own, the same way
// blocklist.rs's USER_BLOCK_TITLE/USER_BLOCK_REASON work.
pub const KNOWN_PHISHING_TITLE: &str = "Known phishing site";
pub const KNOWN_PHISHING_REASON: &str =
    "This address matches a known phishing site from a community threat feed.";

// Ordered, so the cache that holds them is written the same way every time.
pub struct ParsedFeed {
    pub indicators: BTreeSet<String>,
    pub source_count: usize,
}

// `body` is the feed's raw text, already fetched by the browser process.
// Rejected rather than accepted-but-empty when it looks broken: a feed
// that fails to parse should leave the last good cache in place, not
// silently replace it with nothing.
pub fn parse_feed(body: &str) -> Result<ParsedFeed, String> {
    let mut indicators = BTreeSet::new();
    let mut source_count = 0usize;
    for line in body.lines().take(MAX_FEED_LINES) {
        let line = line.trim();
        if !line.starts_with("http://") && !line.starts_with("https://") {
            continue;
        }
        let line_indicators = indicators_for_url(line);
        if !line_indicators.is_empty() {
            indicators.extend(line_indicators);
            source_count += 1;
        }
    }

    if indicators.len() < MIN_INDICATORS {
        return Err(format!(
            "threat feed produced too few indicators ({}, expected at least {MIN_INDICATORS})",
            indicators.len()
        ));
    }
    Ok(ParsedFeed {
        indicators,
        source_count,
    })
}

// `indicators` is the profile's own set -- see ThreatFeedCache.
pub fn is_known_phishing(url: &str, indicators: &BTreeSet<String>) -> bool {
    if indicators.is_empty() {
        return false;
    }
    indicators_for_url(url)
        .iter()
        .any(|indicator| indicators.contains(indicator))
}

// Authority + path, and authority + path + query when there's a query --
// a non-root phishing route usually stays malicious when tracking
// params change, so both the with- and without-query forms are indexed.
fn indicators_for_url(raw_url: &str) -> Vec<String> {
    let Ok(parsed) = url::Url::parse(raw_url) else {
        return Vec::new();
    };
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Vec::new();
    }
    let Some(host) = parsed.host_str() else {
        return Vec::new();
    };
    let authority = match parsed.port() {
        Some(port) if port != 80 && port != 443 => format!("{host}:{port}"),
        _ => host.to_string(),
    };
    let mut path = parsed.path().to_string();
    while path.contains("//") {
        path = path.replace("//", "/");
    }
    if path.len() > 1 {
        path = path.trim_end_matches('/').to_string();
    }
    path.truncate(2048.min(path.len()));
    let query: String = parsed.query().unwrap_or("").chars().take(2048).collect();

    let base = format!("{authority}|{path}");
    let mut indicators = Vec::new();
    if query.is_empty() {
        indicators.push(base);
    } else {
        indicators.push(format!("{base}?{query}"));
        if path != "/" {
            indicators.push(base);
        }
    }
    indicators
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_line(url: &str) -> String {
        format!("{url}\n")
    }

    #[test]
    fn the_same_url_always_gives_the_same_indicators() {
        let a = indicators_for_url("https://evil.example/login");
        let b = indicators_for_url("https://evil.example/login");
        assert_eq!(a, b);
        assert!(!a.is_empty());
    }

    #[test]
    fn a_query_string_produces_both_with_and_without_query_indicators() {
        let indicators = indicators_for_url("https://evil.example/login?id=123");
        assert_eq!(indicators.len(), 2);
    }

    #[test]
    fn the_root_path_with_a_query_only_indexes_the_with_query_form() {
        let indicators = indicators_for_url("https://evil.example/?id=123");
        assert_eq!(indicators.len(), 1);
    }

    #[test]
    fn a_non_http_url_produces_no_indicators() {
        assert!(indicators_for_url("ftp://evil.example/file").is_empty());
    }

    #[test]
    fn an_indicator_is_the_host_and_the_normalised_path() {
        assert_eq!(
            indicators_for_url("https://evil.example//login/"),
            ["evil.example|/login"]
        );
    }

    #[test]
    fn is_known_phishing_matches_an_indicator() {
        let indicators: BTreeSet<String> =
            indicators_for_url("https://evil.example/login").into_iter().collect();
        assert!(is_known_phishing("https://evil.example/login", &indicators));
        assert!(!is_known_phishing("https://safe.example/", &indicators));
    }

    #[test]
    fn an_empty_indicator_set_matches_nothing() {
        assert!(!is_known_phishing("https://evil.example/login", &BTreeSet::new()));
    }

    #[test]
    fn parsing_a_feed_below_the_indicator_floor_is_rejected() {
        let body = feed_line("https://evil.example/one");
        assert!(parse_feed(&body).is_err());
    }

    #[test]
    fn parsing_a_real_looking_feed_collects_every_indicator() {
        let mut body = String::new();
        for i in 0..(MIN_INDICATORS + 10) {
            body.push_str(&feed_line(&format!("https://evil{i}.example/login")));
        }
        let parsed = parse_feed(&body).unwrap();
        assert!(parsed.indicators.len() >= MIN_INDICATORS);
        assert_eq!(parsed.source_count, MIN_INDICATORS + 10);
    }

    #[test]
    fn non_url_lines_are_skipped_without_counting_as_a_source() {
        let mut body = "# a comment\n\n".to_string();
        for i in 0..MIN_INDICATORS {
            body.push_str(&feed_line(&format!("https://evil{i}.example/login")));
        }
        let parsed = parse_feed(&body).unwrap();
        assert_eq!(parsed.source_count, MIN_INDICATORS);
    }
}
