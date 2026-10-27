// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// What the address bar does with what the user typed.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Navigate(String),
    Search(String),
}

// Decides between "go there" and "search for that", biased towards
// searching: guessing wrong towards navigation visits a host the user did
// not mean and leaks the query in a DNS lookup.
pub fn parse(input: &str) -> Action {
    let text = input.trim();
    if text.is_empty() {
        return Action::Search(String::new());
    }

    // Our own pages, typed exactly.
    if crate::is_internal_address(text) {
        return Action::Navigate(text.to_string());
    }

    // An explicit scheme is unambiguous. Whether the target is safe is
    // weglet-security's decision.
    if has_explicit_scheme(text) {
        return Action::Navigate(text.to_string());
    }

    // Whitespace means a sentence, not a hostname.
    if text.chars().any(char::is_whitespace) {
        return Action::Search(text.to_string());
    }

    let authority = text.split(['/', '\\', '?', '#']).next().unwrap_or_default();

    // Typed userinfo is a search: "google.com@evil.example" reads as
    // Google and resolves to evil.example.
    if authority.contains('@') {
        return Action::Search(text.to_string());
    }

    let host = match authority.strip_prefix('[') {
        // IPv6 literal.
        Some(_) => match authority.find(']') {
            Some(end) => &authority[..=end],
            None => return Action::Search(text.to_string()),
        },
        None => authority.split(':').next().unwrap_or_default(),
    };

    if looks_like_a_host(host) {
        return Action::Navigate(format!("https://{text}"));
    }

    Action::Search(text.to_string())
}

// A bookmark or history entry, generic over what owns the string data:
// weglet-ffi hands over borrowed strings out of the profile's own
// structs, and this crate has no reason to know either shape.
#[derive(Debug, Clone, Copy)]
pub struct SuggestionSource<'a> {
    pub label: &'a str,
    pub url: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind {
    Bookmark,
    History,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub kind: SuggestionKind,
    pub label: String,
    pub url: String,
}

// Shown below what has room in the popup -- see WegletOmniboxSuggestionsPopup.
pub const MAX_SUGGESTIONS: usize = 6;

// Ranks bookmarks and history entries whose label or url contains `input`
// (case-insensitively). A prefix match beats a plain substring match, and
// at equal rank a bookmark beats a history entry -- a site you kept beats
// one you merely visited -- and an earlier (more recent, since both lists
// arrive newest-first) entry beats a later one.
pub fn suggest(
    input: &str,
    bookmarks: &[SuggestionSource],
    history: &[SuggestionSource],
) -> Vec<Suggestion> {
    let needle = input.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    let mut scored: Vec<(u8, usize, Suggestion)> = Vec::new();
    let mut collect = |base_rank: u8, kind: SuggestionKind, sources: &[SuggestionSource]| {
        for (order, source) in sources.iter().enumerate() {
            let lower_label = source.label.to_lowercase();
            let lower_url = source.url.to_lowercase();
            let is_prefix = lower_label.starts_with(&needle) || lower_url.starts_with(&needle);
            let is_match = is_prefix || lower_label.contains(&needle) || lower_url.contains(&needle);
            if !is_match {
                continue;
            }
            let rank = base_rank + u8::from(!is_prefix);
            scored.push((
                rank,
                order,
                Suggestion {
                    kind,
                    label: source.label.to_string(),
                    url: source.url.to_string(),
                },
            ));
        }
    };
    collect(0, SuggestionKind::Bookmark, bookmarks);
    collect(2, SuggestionKind::History, history);
    scored.sort_by_key(|(rank, order, _)| (*rank, *order));

    // A page that is both bookmarked and in history should appear once,
    // as the bookmark: collect() already puts bookmarks first.
    let mut seen_urls = std::collections::HashSet::new();
    scored
        .into_iter()
        .filter(|(_, _, suggestion)| seen_urls.insert(suggestion.url.clone()))
        .take(MAX_SUGGESTIONS)
        .map(|(_, _, suggestion)| suggestion)
        .collect()
}

fn has_explicit_scheme(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once(':') else {
        return false;
    };
    if scheme.is_empty() || !rest.starts_with("//") {
        // "localhost:3000" splits here too, and it is not a scheme.
        return false;
    }
    scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
}

fn looks_like_a_host(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    if host == "localhost" || host.starts_with('[') || is_ipv4(host) {
        return true;
    }
    // "hello." and ".com" are not hosts, so both sides of the last dot
    // have to be real labels.
    let Some((name, tld)) = host.rsplit_once('.') else {
        return false;
    };
    if name.is_empty() || tld.len() < 2 {
        return false;
    }
    // A numeric TLD is probably a version number or a decimal.
    if tld.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    host.split('.').all(is_label)
}

fn is_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || !c.is_ascii())
}

// Written out rather than u8::from_str, which accepts a leading "+" and
// would make "+1.+1.+1.+1" an address.
fn is_ipv4(host: &str) -> bool {
    let mut labels = 0;
    for label in host.split('.') {
        let valid = !label.is_empty()
            && label.len() <= 3
            && label.chars().all(|c| c.is_ascii_digit())
            && label.parse::<u32>().is_ok_and(|n| n <= 255);
        if !valid {
            return false;
        }
        labels += 1;
    }
    labels == 4
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nav(url: &str) -> Action {
        Action::Navigate(url.to_string())
    }

    fn search(text: &str) -> Action {
        Action::Search(text.to_string())
    }

    #[test]
    fn a_bare_domain_becomes_an_https_address() {
        assert_eq!(parse("example.com"), nav("https://example.com"));
        assert_eq!(parse("sub.example.co.uk/a?b=c"), nav("https://sub.example.co.uk/a?b=c"));
        assert_eq!(parse("  example.com  "), nav("https://example.com"));
    }

    #[test]
    fn an_explicit_scheme_is_taken_at_face_value() {
        assert_eq!(parse("http://example.com"), nav("http://example.com"));
        assert_eq!(parse("https://example.com/x"), nav("https://example.com/x"));
        // Whether this is safe is weglet-security's call, not ours.
        assert_eq!(parse("ftp://example.com"), nav("ftp://example.com"));
    }

    #[test]
    fn our_own_pages_are_navigated_to_exactly() {
        assert_eq!(parse(crate::SETTINGS_ADDRESS), nav(crate::SETTINGS_ADDRESS));
        assert_eq!(parse(crate::BLANK_TAB), nav(crate::BLANK_TAB));
        // Not one of ours, so not special.
        assert_eq!(
            parse("weglet://settings.evil.example"),
            nav("weglet://settings.evil.example")
        );
    }

    #[test]
    fn a_phrase_is_a_search() {
        assert_eq!(parse("how to build a browser"), search("how to build a browser"));
        assert_eq!(parse("rust vs c++"), search("rust vs c++"));
        // A dot does not save it once there is a space.
        assert_eq!(parse("go to example.com"), search("go to example.com"));
    }

    #[test]
    fn a_single_word_without_a_dot_is_a_search() {
        assert_eq!(parse("weather"), search("weather"));
        assert_eq!(parse("localhost3000"), search("localhost3000"));
    }

    #[test]
    fn localhost_and_ports_are_addresses() {
        assert_eq!(parse("localhost"), nav("https://localhost"));
        assert_eq!(parse("localhost:3000"), nav("https://localhost:3000"));
        assert_eq!(parse("localhost:3000/api"), nav("https://localhost:3000/api"));
    }

    #[test]
    fn ip_addresses_are_addresses() {
        assert_eq!(parse("127.0.0.1"), nav("https://127.0.0.1"));
        assert_eq!(parse("192.168.1.1:8080"), nav("https://192.168.1.1:8080"));
        assert_eq!(parse("[::1]"), nav("https://[::1]"));
        assert_eq!(parse("[::1]:9229/x"), nav("https://[::1]:9229/x"));
    }

    // "+1" parses as 1 through u8::from_str.
    #[test]
    fn a_signed_number_is_not_an_ip_address() {
        assert_eq!(parse("+1.+1.+1.+1"), search("+1.+1.+1.+1"));
    }

    // Typed userinfo is the classic way to make a hostile URL read as a
    // familiar one.
    #[test]
    fn typed_userinfo_is_a_search() {
        assert_eq!(parse("user@example.com"), search("user@example.com"));
        assert_eq!(
            parse("google.com@evil.example"),
            search("google.com@evil.example")
        );
        // A full URL the user pasted is still honoured; the credential
        // check lives in weglet-security.
        assert_eq!(
            parse("https://user@example.com/x"),
            nav("https://user@example.com/x")
        );
    }

    #[test]
    fn a_version_number_is_a_search_not_a_host() {
        assert_eq!(parse("1.2.3"), search("1.2.3"));
        assert_eq!(parse("3.14"), search("3.14"));
    }

    #[test]
    fn a_malformed_host_is_a_search() {
        assert_eq!(parse("hello."), search("hello."));
        assert_eq!(parse(".com"), search(".com"));
        assert_eq!(parse("a..b"), search("a..b"));
        assert_eq!(parse("-example.com"), search("-example.com"));
        assert_eq!(parse("[::1"), search("[::1"));
    }

    #[test]
    fn empty_input_searches_for_nothing() {
        assert_eq!(parse(""), search(""));
        assert_eq!(parse("   "), search(""));
    }

    #[test]
    fn an_internationalised_domain_is_an_address() {
        assert_eq!(parse("\u{43F}\u{440}\u{438}\u{43C}\u{435}\u{440}.\u{440}\u{444}"),
                   nav("https://\u{43F}\u{440}\u{438}\u{43C}\u{435}\u{440}.\u{440}\u{444}"));
    }

    #[test]
    fn a_backslash_ends_the_authority() {
        // Reads as host "example.com"; the backslash is weglet-security's
        // problem.
        assert_eq!(
            parse("example.com\\@evil.example"),
            nav("https://example.com\\@evil.example")
        );
    }

    fn src<'a>(label: &'a str, url: &'a str) -> SuggestionSource<'a> {
        SuggestionSource { label, url }
    }

    #[test]
    fn empty_input_suggests_nothing() {
        let bookmarks = [src("Example", "https://example.com")];
        assert!(suggest("", &bookmarks, &[]).is_empty());
        assert!(suggest("   ", &bookmarks, &[]).is_empty());
    }

    #[test]
    fn a_match_can_come_from_the_label_or_the_url() {
        let bookmarks = [src("Rust Language", "https://rust-lang.org")];
        assert_eq!(suggest("rust", &bookmarks, &[]).len(), 1);
        assert_eq!(suggest("lang.org", &bookmarks, &[]).len(), 1);
        assert!(suggest("nothing-like-this", &bookmarks, &[]).is_empty());
    }

    #[test]
    fn a_bookmark_outranks_a_history_entry_at_equal_match_quality() {
        let bookmarks = [src("Example Site", "https://example.com/a")];
        let history = [src("example query", "https://example.com/b")];
        let results = suggest("example", &bookmarks, &history);
        assert_eq!(results[0].kind, SuggestionKind::Bookmark);
        assert_eq!(results[1].kind, SuggestionKind::History);
    }

    #[test]
    fn a_prefix_match_outranks_a_substring_match() {
        let history = [
            src("has example in the middle", "https://a.example/x"),
            src("example first", "https://b.example/y"),
        ];
        let results = suggest("example", &[], &history);
        assert_eq!(results[0].label, "example first");
    }

    #[test]
    fn the_same_url_is_not_suggested_twice() {
        let bookmarks = [src("Example", "https://example.com")];
        let history = [src("example", "https://example.com")];
        assert_eq!(suggest("example", &bookmarks, &history).len(), 1);
    }

    #[test]
    fn results_are_capped_at_the_maximum() {
        let history: Vec<_> = (0..20)
            .map(|i| (format!("example {i}"), format!("https://example{i}.test")))
            .collect();
        let sources: Vec<_> = history
            .iter()
            .map(|(label, url)| src(label, url))
            .collect();
        assert_eq!(suggest("example", &[], &sources).len(), MAX_SUGGESTIONS);
    }
}
