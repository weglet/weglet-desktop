// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Ad and tracker hosts, built-in and the user's own. Asked at navigation
// time; CreateURLLoaderThrottles is the hook for per-resource blocking
// and nothing uses it yet. See docs/security.md.

use std::collections::HashSet;

use crate::Lists;

// The built-in list, as data. One host per line -- see data/blocklist.txt.
// include_str! rather than a file read at startup: Weglet ships one
// executable and nothing beside it, and a profile can replace the table --
// see Lists.
pub(crate) const BUILT_IN_BLOCKLIST: &str = include_str!("../data/blocklist.txt");

// One host per line, '#' starts a comment.
pub fn parse_host_list(text: &str) -> HashSet<String> {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|line| !line.is_empty())
        .map(canonical_host)
        .collect()
}


// One spelling of a host. Without it "EXAMPLE.com", "example.com.", the
// punycode form and the unicode form are four different strings.
pub fn canonical_host(host: &str) -> String {
    let trimmed = host.trim().trim_end_matches('.').to_lowercase();
    // An IPv6 literal has no idna form; leave it as written.
    if trimmed.contains(':') {
        return trimmed;
    }
    idna::domain_to_ascii(&trimmed).unwrap_or(trimmed)
}

// Walks up the domain (a.b.example.com -> b.example.com -> ...), so a
// subdomain of a blocked host is caught too.
pub fn is_blocked_host(lists: &Lists, host: &str) -> bool {
    walk_up(&canonical_host(host), |candidate| {
        lists.blocked_hosts.contains(candidate)
    })
}

// What the notice says when the user's own list stopped a navigation.
// Here rather than in the browser process because the page renders
// whatever title and reason it is handed and has no wording of its own.
pub const USER_BLOCK_TITLE: &str = "Site blocked";
pub const USER_BLOCK_REASON: &str =
    "You blocked this site. Remove it from the block list in settings to \
     visit it again.";

pub fn matches_user_blocklist(host: &str, blocked: &[String]) -> bool {
    if blocked.is_empty() {
        return false;
    }
    // The user's list is small and changes at runtime, so it is
    // canonicalised per call rather than cached.
    let blocked: HashSet<String> = blocked.iter().map(|host| canonical_host(host)).collect();
    walk_up(&canonical_host(host), |candidate| {
        blocked.contains(candidate)
    })
}

fn walk_up(host: &str, matches: impl Fn(&str) -> bool) -> bool {
    let mut candidate = host;
    loop {
        if matches(candidate) {
            return true;
        }
        match candidate.find('.') {
            Some(dot) => candidate = &candidate[dot + 1..],
            None => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_list_parses_and_is_canonical() {
        let hosts = parse_host_list(BUILT_IN_BLOCKLIST);
        assert!(!hosts.is_empty());
        for host in &hosts {
            assert_eq!(host, &canonical_host(host));
            assert!(!host.starts_with('#'));
            assert!(!host.contains(' '));
        }
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let hosts = parse_host_list(
            "# a comment\n\n  EXAMPLE.com.  \nads.example # trailing\n",
        );
        assert_eq!(hosts.len(), 2);
        assert!(hosts.contains("example.com"));
        assert!(hosts.contains("ads.example"));
    }

    #[test]
    fn an_exact_blocked_host_matches() {
        assert!(is_blocked_host(&Lists::built_in(), "doubleclick.net"));
    }

    #[test]
    fn a_subdomain_of_a_blocked_host_matches() {
        assert!(is_blocked_host(&Lists::built_in(), "stats.doubleclick.net"));
    }

    #[test]
    fn an_unrelated_host_does_not_match() {
        assert!(!is_blocked_host(&Lists::built_in(), "example.com"));
    }

    #[test]
    fn a_host_that_merely_contains_a_blocked_name_does_not_match() {
        // Not a suffix match: "notdoubleclick.net" != "doubleclick.net".
        assert!(!is_blocked_host(&Lists::built_in(), "notdoubleclick.net"));
    }

    #[test]
    fn user_blocklist_matches_exact_and_subdomains() {
        let blocked = vec!["evil.example".to_string()];
        assert!(matches_user_blocklist("evil.example", &blocked));
        assert!(matches_user_blocklist("sub.evil.example", &blocked));
        assert!(!matches_user_blocklist("example.com", &blocked));
    }

    #[test]
    fn an_empty_user_blocklist_matches_nothing() {
        assert!(!matches_user_blocklist("evil.example", &[]));
    }

    // One domain, four spellings. Blocking it once has to block all of
    // them.
    #[test]
    fn a_host_matches_however_it_is_spelled() {
        assert!(is_blocked_host(&Lists::built_in(), "DoubleClick.net"));
        assert!(is_blocked_host(&Lists::built_in(), "doubleclick.net."));
        assert!(is_blocked_host(&Lists::built_in(), "STATS.DoubleClick.NET."));

        let blocked = vec!["\u{41f}\u{440}\u{438}\u{43c}\u{435}\u{440}.\u{440}\u{444}".to_string()];
        assert!(matches_user_blocklist("xn--e1afmkfd.xn--p1ai", &blocked));
        assert!(matches_user_blocklist(
            "\u{43F}\u{440}\u{438}\u{43C}\u{435}\u{440}.\u{440}\u{444}",
            &blocked
        ));
        assert!(matches_user_blocklist(
            "sub.xn--e1afmkfd.xn--p1ai",
            &blocked
        ));
    }

    #[test]
    fn canonical_host_leaves_an_ipv6_literal_alone() {
        assert_eq!(canonical_host("[::1]"), "[::1]");
    }
}
