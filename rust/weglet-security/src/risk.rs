// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// Punycode, confusable letters and brand impersonation. Advisory, and
// wrong in both directions; the measured rates are in tests/corpus.rs.

use std::collections::HashSet;

use crate::Lists;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Warning,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationRisk {
    pub level: RiskLevel,
    pub title: String,
    pub reason: String,
    pub normalized_host: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct BrandRule {
    #[serde(rename = "name")]
    display_name: String,
    // A label that is the brand wherever it sits in front of a public
    // suffix: google.com, google.co.nz and the other ~190 country domains
    // are all Google's. Hand-listing them blocked blog.google.
    #[serde(rename = "labels", default)]
    official_labels: Vec<String>,
    // Registrable domains the brand owns whose own label is not one of
    // the labels above.
    #[serde(rename = "domains", default)]
    official_domains: Vec<String>,
    // What an impostor imitates.
    #[serde(default)]
    tokens: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct BrandFile {
    #[serde(default)]
    brand: Vec<BrandRule>,
}

// The brands, as data. See data/brands.toml for the format. include_str!
// rather than a file read at startup: Weglet ships one executable and
// nothing beside it, and a profile can supply its own -- see Lists.
pub(crate) const BUILT_IN_BRANDS: &str = include_str!("../data/brands.toml");

pub(crate) fn parse_brands(text: &str) -> Result<Vec<BrandRule>, String> {
    toml::from_str::<BrandFile>(text)
        .map(|file| file.brand)
        .map_err(|error| error.to_string())
}

// The words, as data. One per line; see data/sensitive_words.txt.
// Matched as whole words: "auth" inside "authority" is not a signal.
pub(crate) const BUILT_IN_SENSITIVE_WORDS: &str = include_str!("../data/sensitive_words.txt");

pub(crate) fn parse_word_list(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|line| !line.is_empty())
        .map(str::to_lowercase)
        .collect()
}

// What the rest of the name says when a brand is embedded in it. On their
// own these mean nothing -- applebees is somebody's business -- but next
// to a brand name they are the shape of a phishing domain.
const LURE_WORDS: &[&str] = &[
    "account",
    "activate",
    "airdrop",
    "alert",
    "auth",
    "billing",
    "bonus",
    "claim",
    "confirm",
    "free",
    "gift",
    "giveaway",
    "help",
    "invoice",
    "limited",
    "login",
    "logon",
    "mfa",
    "nitro",
    "offer",
    "otp",
    "password",
    "payment",
    "premium",
    "prize",
    "promo",
    "recovery",
    "refund",
    "reset",
    "restore",
    "reward",
    "robux",
    "secure",
    "signin",
    "support",
    "suspended",
    "trade",
    "unlock",
    "update",
    "verification",
    "verify",
    "wallet",
    "warning",
    "winner",
];

const SHORTENERS: &[&str] = &[
    "bit.ly",
    "tinyurl.com",
    "t.co",
    "goo.su",
    "cutt.ly",
    "clck.ru",
    "is.gd",
    "rb.gy",
    "rebrand.ly",
    "shorturl.at",
    "tiny.cc",
    "u.to",
    "to.lk",
    "ow.ly",
    "buff.ly",
];

const HIGH_RISK_TLDS: &[&str] = &[
    "bond", "click", "cyou", "fit", "gq", "icu", "kim", "ml", "mom", "rest", "top", "work", "xyz",
    "zip",
];

// Suffixes that never leave the machine or the local network. Warning on
// every developer hitting their own server trains people to click through.
const LOCAL_SUFFIXES: &[&str] = &["localhost", "local", "test", "internal", "home.arpa", "lan"];

pub fn assess_navigation(lists: &Lists, raw_url: &str) -> Option<NavigationRisk> {
    if raw_url.len() > 4096 {
        return Some(block(
            "Address too long",
            "The address is unusually long, which can hide the real domain.",
            None,
        ));
    }
    if raw_url
        .chars()
        .any(|c| (c as u32) < 0x20 || (c as u32) == 0x7F || is_bidi_control(c))
    {
        return Some(block(
            "Hidden characters in address",
            "The link contains control characters that could disguise the real address.",
            None,
        ));
    }
    if raw_url.contains('\\') || has_encoded_control(raw_url) {
        return Some(block(
            "Unsafe link format",
            "The link contains a backslash or encoded control characters.",
            None,
        ));
    }

    let Ok(parsed) = url::Url::parse(raw_url) else {
        return Some(block(
            "Invalid address",
            "Could not safely parse the site address.",
            None,
        ));
    };
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Some(block(
            "Unsupported link",
            "The site is trying to open an unsupported link type.",
            None,
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Some(block(
            "Domain hidden in link",
            "A username or password appears before the domain. This is a common phishing trick.",
            None,
        ));
    }
    let Some(raw_host) = parsed.host_str() else {
        return Some(block(
            "Domain not determined",
            "Weglet couldn't determine the real domain of this site.",
            None,
        ));
    };
    let had_trailing_dot = raw_host.ends_with('.');
    let Some(host) = normalized_host(raw_host) else {
        return Some(block(
            "Invalid domain",
            "The domain name doesn't match a safe DNS format.",
            None,
        ));
    };
    if host.len() > 253
        || host
            .split('.')
            .any(|label| label.is_empty() || label.len() > 63)
    {
        return Some(block(
            "Invalid domain",
            "The domain name exceeds the safe length or has an empty part.",
            Some(host),
        ));
    }

    // Nothing below applies to a machine talking to itself or its own
    // network.
    if is_local(&host) {
        return None;
    }

    let (unicode_host, _) = idna::domain_to_unicode(&host);
    let uses_punycode = host.split('.').any(|label| label.starts_with("xn--"));
    let sensitive = has_sensitive_word(
        lists,
        &unicode_host,
        parsed.path(),
        parsed.query().unwrap_or(""),
    );

    let labels: Vec<&str> = unicode_host.split('.').collect();
    let disguised = labels.iter().any(|label| is_disguised_latin(label));
    let mixed_scripts = labels.iter().any(|label| mixes_scripts(label));

    if let Some(risk) = find_brand_impersonation(
        lists,
        &host,
        &unicode_host,
        sensitive,
        disguised,
        mixed_scripts,
        uses_punycode,
    ) {
        return Some(risk);
    }

    // A label written entirely in another alphabet whose every character
    // has a Latin lookalike is a Latin word in disguise. A genuine
    // Cyrillic or Greek domain has at least one character with no Latin
    // twin.
    if disguised {
        return Some(NavigationRisk {
            level: if sensitive {
                RiskLevel::Block
            } else {
                RiskLevel::Warning
            },
            title: "Lookalike letters in domain".to_string(),
            reason: "Every letter in this domain has a Latin lookalike, so the name is not \
                what it appears to be."
                .to_string(),
            normalized_host: Some(host),
        });
    }
    if mixed_scripts {
        return Some(warn(
            "Mixed alphabets in domain",
            "This domain mixes letters from different alphabets, which can be used to \
                disguise a fake site.",
            host,
        ));
    }
    if uses_punycode && sensitive {
        return Some(warn(
            "Encoded domain",
            "This login or payment page uses an internationalized encoded domain name. \
                Check it carefully.",
            host,
        ));
    }
    if had_trailing_dot {
        return Some(warn(
            "Unusual domain notation",
            "The address ends with a hidden trailing dot. Check the domain before continuing.",
            host,
        ));
    }

    if is_ipv4(&host) || host.contains(':') {
        return Some(warn(
            "Site has no domain name",
            "This site opens at a numeric IP address instead of a normal domain name. \
                Don't enter a password until you're sure of the address.",
            host,
        ));
    }
    if SHORTENERS
        .iter()
        .any(|s| host == *s || host.ends_with(&format!(".{s}")))
    {
        return Some(warn(
            "Shortened link",
            "This short link hides the destination site. Weglet will re-check the address \
                after the redirect.",
            host,
        ));
    }
    let tld = host.rsplit('.').next().unwrap_or("");
    if sensitive && HIGH_RISK_TLDS.contains(&tld) {
        return Some(warn(
            "Risky login-page address",
            "This page asks for sensitive data and uses a domain zone often seen in \
                throwaway phishing sites.",
            host,
        ));
    }
    if sensitive && host.matches('.').count() >= 5 {
        return Some(warn(
            "Too many subdomains",
            "This login page uses a long chain of subdomains, which can distract from the \
                real site owner.",
            host,
        ));
    }
    // Only worth mentioning on a page that wants credentials: plenty of
    // ordinary sites run on 8080 or 8443.
    if sensitive {
        if let Some(port) = parsed.port() {
            if port != 80 && port != 443 {
                return Some(warn(
                    "Unusual network port",
                    &format!("This login page uses port {port}. Make sure you recognize it."),
                    host,
                ));
            }
        }
    }
    None
}

// Splits the registrable domain off the public suffix, returning the index
// of the domain's own label so the caller can tell the name the owner
// chose from the subdomains in front of it.
//
// ICANN half only: the private half contains googleapis.com and
// githubusercontent.com, which would hide the brand's own label.
fn own_label_index(host: &str) -> Option<usize> {
    let suffix = psl::suffix(host.as_bytes())?;
    let mut suffix_labels = std::str::from_utf8(suffix.as_bytes())
        .ok()?
        .split('.')
        .count();
    if suffix.typ() == Some(psl::Type::Private) && suffix_labels > 1 {
        suffix_labels -= 1;
    }
    host.split('.').count().checked_sub(suffix_labels + 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Confidence {
    Exact,
    Embedded,
}

fn find_brand_impersonation(
    lists: &Lists,
    host: &str,
    unicode_host: &str,
    sensitive: bool,
    disguised: bool,
    mixed_scripts: bool,
    uses_punycode: bool,
) -> Option<NavigationRisk> {
    // A bare public suffix, or one the list does not know. Nothing to
    // compare a brand against.
    let own_index = own_label_index(host)?;
    let ascii_labels: Vec<&str> = host.split('.').collect();
    let unicode_labels: Vec<&str> = unicode_host.split('.').collect();
    // idna maps label for label, so the two line up.
    let own_label = unicode_labels
        .get(own_index)
        .copied()
        .or_else(|| ascii_labels.get(own_index).copied())
        .unwrap_or_default();
    let registrable = ascii_labels[own_index..].join(".");
    let own_skeleton = skeleton(own_label);
    let own_variants = homoglyph_variants(&own_skeleton);
    // Compared as registered, not folded: skeleton() maps 0 to o, so the
    // folded form would read "faceb00k" as the real "facebook".
    let own_ascii = ascii_labels.get(own_index).copied().unwrap_or_default();

    // It is the brand. Stop before any impersonation rule runs.
    for rule in &lists.brands {
        if rule.official_labels.iter().any(|label| label == own_ascii)
            || rule
                .official_domains
                .iter()
                .any(|domain| domain == &registrable)
        {
            return None;
        }
    }

    for rule in &lists.brands {
        let mut matched = None;
        for token in rule.tokens.iter().map(String::as_str) {
            // Typosquat or homoglyph of the name itself: gooogle,
            // faceb00k, or five Cyrillic letters that read as "apple".
            if own_variants
                .iter()
                .any(|variant| variant == token || edit_distance_at_most_one(variant, token))
            {
                matched = Some(Confidence::Exact);
                break;
            }
            // The brand name as a word inside a longer name, but only
            // when the rest of the name is doing a phishing page's job.
            // Otherwise applebees and microsoftware get flagged.
            if token_reads_as_a_word(&own_skeleton, own_label, token)
                && (sensitive || label_carries_a_lure(own_label, token))
            {
                matched = Some(Confidence::Embedded);
            }
            // The brand as a whole subdomain label in front of somebody
            // else's domain: accounts.google.com.evil-cdn.example.
            if unicode_labels[..own_index]
                .iter()
                .any(|label| skeleton(label) == *token)
            {
                matched = Some(Confidence::Embedded);
            }
        }
        let Some(confidence) = matched else {
            continue;
        };

        // An exact lookalike is unambiguous. A brand name embedded in a
        // longer domain is a guess -- fan sites and mirrors trip it -- so
        // it only hardens into a block when something else is wrong.
        let hard_block = confidence == Confidence::Exact
            || disguised
            || mixed_scripts
            || (sensitive && uses_punycode);
        let level = if hard_block {
            RiskLevel::Block
        } else {
            RiskLevel::Warning
        };
        let action = if hard_block {
            "Weglet blocked this navigation."
        } else {
            "Check the address carefully before continuing."
        };
        return Some(NavigationRisk {
            level,
            title: format!("Possible impersonation of {}", rule.display_name),
            reason: format!(
                "This domain resembles {} but isn't one of its official domains. {action}",
                rule.display_name
            ),
            normalized_host: Some(host.to_string()),
        });
    }
    None
}

// True when `token` is its own word in the label: at the start, or with a
// separator on at least one side. The boundary stops "livestream"
// matching "steam"; the start anchor still catches
// "googleaccountsecuritycheck".
fn token_reads_as_a_word(own_skeleton: &str, original: &str, token: &str) -> bool {
    if own_skeleton.starts_with(token) {
        return true;
    }
    original
        .to_lowercase()
        .split(['-', '_', '.'])
        .any(|part| skeleton(part) == token)
}

// True when the label carries a lure word somewhere other than the brand
// token itself.
fn label_carries_a_lure(own_label: &str, token: &str) -> bool {
    let folded = skeleton(own_label);
    let rest = folded.replacen(token, "", 1);
    LURE_WORDS.iter().any(|lure| rest.contains(lure))
}

// Letter pairs that render as one letter in most fonts: "rn" for "m",
// "vv" for "w". Compared alongside the plain skeleton, not instead of it.
fn homoglyph_variants(skeleton: &str) -> Vec<String> {
    let mut variants = vec![skeleton.to_string()];
    let folded = skeleton.replace("rn", "m").replace("vv", "w");
    if folded != skeleton {
        variants.push(folded);
    }
    variants
}

fn normalized_host(value: &str) -> Option<String> {
    let raw = value.trim().trim_end_matches('.').to_lowercase();
    if raw.is_empty() {
        return None;
    }
    if raw.contains(':') {
        return Some(
            raw.trim_start_matches('[')
                .trim_end_matches(']')
                .to_string(),
        );
    }
    idna::domain_to_ascii(&raw).ok().filter(|s| !s.is_empty())
}

fn is_local(host: &str) -> bool {
    if host == "localhost" || host == "::1" {
        return true;
    }
    if LOCAL_SUFFIXES
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
    {
        return true;
    }
    // Only for an IPv6 literal. Applied to any host string, these three
    // prefixes skip every check for fdic.gov, fcbarcelona.com and any
    // domain a typosquatter registers starting fc or fd. normalized_host
    // strips the brackets, so a colon is what is left to recognise one by.
    if host.contains(':')
        && (host.starts_with("fc")
            || host.starts_with("fd")
            || host.starts_with("fe80"))
    {
        return true;
    }
    if host.starts_with("::ffff:127.") {
        return true;
    }
    if !is_ipv4(host) {
        return false;
    }
    let octets: Vec<u32> = host.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    octets[0] == 10
        || octets[0] == 127
        || (octets[0] == 169 && octets[1] == 254)
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
}

// Words, not substrings: "authority" yields "authority" and never "auth".
fn has_sensitive_word(lists: &Lists, host: &str, path: &str, query: &str) -> bool {
    let mut words: HashSet<String> = HashSet::new();
    for part in [host, path, query] {
        for word in part
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
        {
            words.insert(word.to_string());
        }
    }
    lists
        .sensitive_words
        .iter()
        .any(|sensitive| {
            words.contains(sensitive.as_str()) || words.contains(&format!("{sensitive}s"))
        })
}

// Latin mixed with Cyrillic, Greek or Armenian inside one label.
fn mixes_scripts(label: &str) -> bool {
    let mut has_latin = false;
    let mut has_other = false;
    for c in label.chars().filter(|c| c.is_alphabetic()) {
        let code = c as u32;
        if code < 0x80 || (0x00C0..=0x024F).contains(&code) {
            has_latin = true;
        } else if (0x0400..=0x04FF).contains(&code)
            || (0x0370..=0x03FF).contains(&code)
            || (0x0530..=0x058F).contains(&code)
        {
            has_other = true;
        }
    }
    has_latin && has_other
}

// A label with no ASCII letters, every one of whose letters has a Latin
// lookalike: "\u{430}\u{440}\u{440}\u{4cf}\u{435}" spells "apple", while
// "\u{43f}\u{440}\u{438}\u{43c}\u{435}\u{440}" does not qualify because
// "\u{43f}" and "\u{438}" have no Latin twin.
fn is_disguised_latin(label: &str) -> bool {
    let letters: Vec<char> = label.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() || letters.iter().any(|c| c.is_ascii()) {
        return false;
    }
    letters.iter().all(|c| confusable_to_latin(*c).is_some())
}

// Folds a label to the Latin word it looks like. Anything outside [a-z0-9]
// after folding is dropped.
fn skeleton(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|c| confusable_to_latin(c).unwrap_or(c))
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect()
}

// Confusables from Unicode's own list, restricted to the ones that show
// up in domain abuse: Cyrillic, Greek, Armenian, digit/letter pairs.
fn confusable_to_latin(c: char) -> Option<char> {
    Some(match c {
        '\u{430}' | '\u{3b1}' | '\u{e4}' | '\u{e0}' | '\u{e1}' | '\u{e2}' => 'a',
        '\u{432}' | '\u{3b2}' | '\u{44c}' | '\u{44a}' => 'b',
        '\u{441}' | '\u{3f2}' | '\u{e7}' => 'c',
        '\u{501}' | '\u{10f}' => 'd',
        '\u{435}' | '\u{451}' | '\u{3b5}' | '\u{44d}' | '\u{eb}' | '\u{e8}' | '\u{e9}' | '\u{ea}' => 'e',
        '\u{493}' | '\u{3dd}' => 'f',
        '\u{581}' | '\u{123}' => 'g',
        '\u{43d}' | '\u{3b7}' | '\u{4bb}' => 'h',
        '\u{456}' | '\u{3b9}' | '\u{457}' | '\u{ed}' | '\u{ec}' | '\u{ee}' | '\u{ef}' => 'i',
        '\u{458}' | '\u{135}' => 'j',
        '\u{43a}' | '\u{3ba}' | '\u{137}' => 'k',
        // U+04CF palochka renders as a bare vertical stroke.
        '1' | 'l' | '\u{217c}' | '\u{4cf}' => 'l',
        '\u{43c}' | '\u{3bc}' => 'm',
        '\u{578}' | '\u{f1}' | '\u{144}' => 'n',
        '\u{43e}' | '\u{3bf}' | '\u{3c3}' | '\u{4e9}' | '\u{f6}' | '\u{f2}' | '\u{f3}' | '\u{f4}' | '0' => 'o',
        '\u{440}' | '\u{3c1}' | '\u{584}' => 'p',
        '\u{51b}' => 'q',
        '\u{433}' | '\u{159}' => 'r',
        '\u{455}' | '\u{15f}' | '\u{219}' => 's',
        '\u{442}' | '\u{3c4}' | '\u{163}' => 't',
        '\u{57d}' | '\u{446}' | '\u{fc}' | '\u{fa}' | '\u{f9}' | '\u{fb}' => 'u',
        '\u{3bd}' | '\u{475}' => 'v',
        '\u{461}' | '\u{561}' | '\u{3c9}' => 'w',
        '\u{445}' | '\u{3c7}' => 'x',
        '\u{443}' | '\u{3b3}' | '\u{fd}' | '\u{ff}' => 'y',
        '\u{437}' | '\u{3b6}' | '\u{17e}' => 'z',
        _ => return None,
    })
}

// A single substitution, insertion or deletion. Not full Levenshtein.
fn edit_distance_at_most_one(first: &str, second: &str) -> bool {
    // Too short to typosquat: one edit away from a four-letter name is
    // half the internet.
    if first.chars().count() < 5 || second.chars().count() < 5 {
        return first == second && !first.is_empty();
    }
    if first == second {
        return true;
    }
    let a: Vec<char> = first.chars().collect();
    let b: Vec<char> = second.chars().collect();
    if (a.len() as i64 - b.len() as i64).abs() > 1 {
        return false;
    }
    let mut left = 0;
    let mut right = 0;
    let mut changes = 0;
    while left < a.len() && right < b.len() {
        if a[left] == b[right] {
            left += 1;
            right += 1;
        } else {
            changes += 1;
            if changes > 1 {
                return false;
            }
            match a.len().cmp(&b.len()) {
                std::cmp::Ordering::Greater => left += 1,
                std::cmp::Ordering::Less => right += 1,
                std::cmp::Ordering::Equal => {
                    left += 1;
                    right += 1;
                }
            }
        }
    }
    if left < a.len() || right < b.len() {
        changes += 1;
    }
    changes <= 1
}

fn is_ipv4(host: &str) -> bool {
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 3
                && part.chars().all(|c| c.is_ascii_digit())
                && part.parse::<u32>().is_ok_and(|n| n <= 255)
        })
}

fn is_bidi_control(c: char) -> bool {
    matches!(
        c as u32,
        0x061C
            | 0x200E
            | 0x200F
            | 0x202A
            | 0x202B
            | 0x202C
            | 0x202D
            | 0x202E
            | 0x2066
            | 0x2067
            | 0x2068
            | 0x2069
    )
}

fn has_encoded_control(url: &str) -> bool {
    let lower = url.to_lowercase();
    ["%00", "%0a", "%0d", "%1b", "%7f"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn block(title: &str, reason: &str, host: Option<String>) -> NavigationRisk {
    NavigationRisk {
        level: RiskLevel::Block,
        title: title.to_string(),
        reason: reason.to_string(),
        normalized_host: host,
    }
}

fn warn(title: &str, reason: &str, host: String) -> NavigationRisk {
    NavigationRisk {
        level: RiskLevel::Warning,
        title: title.to_string(),
        reason: reason.to_string(),
        normalized_host: Some(host),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The data files are compiled in, so a malformed one would be a
    // browser that starts with no rules. These make it a build failure.
    #[test]
    fn the_built_in_brand_table_parses() {
        let rules = parse_brands(BUILT_IN_BRANDS).expect("data/brands.toml");
        assert!(!rules.is_empty());
        for rule in &rules {
            assert!(!rule.display_name.is_empty());
            // A brand with no tokens matches nothing; one with nothing
            // official cannot tell the real site from an impostor.
            assert!(
                !rule.official_labels.is_empty() || !rule.official_domains.is_empty(),
                "{} has nothing official",
                rule.display_name
            );
        }
    }

    #[test]
    fn the_built_in_word_list_parses() {
        let words = parse_word_list(BUILT_IN_SENSITIVE_WORDS);
        assert!(!words.is_empty());
        // Lowercased on load: so are the labels they are compared against.
        assert!(words.iter().all(|word| word == &word.to_lowercase()));
        assert!(words.iter().all(|word| !word.starts_with('#')));
    }

    #[test]
    fn a_malformed_brand_override_is_rejected_whole() {
        // Half a rule set catches some impostors and waves others
        // through.
        assert!(parse_brands("[[brand]]\nname = ").is_err());
        assert!(parse_brands("not toml at all = = =").is_err());
    }

    fn risk(url: &str) -> Option<NavigationRisk> {
        assess_navigation(&Lists::built_in(), url)
    }

    #[test]
    fn a_plain_safe_url_has_no_risk() {
        assert_eq!(risk("https://example.com/page?q=1"), None);
    }

    #[test]
    fn official_brand_domains_are_never_flagged() {
        for url in [
            "https://accounts.google.com/signin",
            "https://blog.google/",
            "https://about.google/",
            "https://google.co.nz/",
            "https://google.com.mx/",
            "https://github.com/anthropics/weglet",
            "https://github.dev/",
            "https://discord.media/",
            "https://steamstatic.com/",
            "https://youtube-nocookie.com/",
            "https://www.paypal.com/login",
        ] {
            assert_eq!(risk(url), None, "{url}");
        }
    }

    #[test]
    fn a_brand_word_inside_an_unrelated_name_is_not_a_brand() {
        for url in [
            "https://livestream.com/login",
            "https://esteam.io/account",
            "https://en.wikipedia.org/wiki/Authority",
            "https://myshop.top/support",
        ] {
            assert_eq!(risk(url), None, "{url}");
        }
    }

    #[test]
    fn local_addresses_are_never_flagged() {
        for url in [
            "http://localhost:3000/",
            "http://127.0.0.1:8080/login",
            "http://192.168.1.1/",
            "http://10.0.0.5:8443/admin",
            "http://[::1]/",
            "http://dev.localhost:5173/",
            "http://printer.local/",
            "http://api.test/login",
            "http://[fd00::1]/",
            "http://[fc00::42]:8080/",
            "http://[fe80::1]/",
        ] {
            assert_eq!(risk(url), None, "{url}");
        }
    }

    // The ULA prefixes are an IPv6 rule. Applied to any host string they
    // skipped every check for a domain beginning fc or fd, which is a
    // registration away.
    #[test]
    fn a_domain_starting_like_a_ula_prefix_is_still_assessed() {
        assert_eq!(
            risk("https://fd-google-account-verify.com/login")
                .map(|r| r.title.clone()),
            risk("https://xd-google-account-verify.com/login")
                .map(|r| r.title.clone()),
        );
        assert!(risk("https://fd-google-account-verify.com/login").is_some());
        assert!(risk("https://fc-facebook-login-verify.com/signin").is_some());
    }

    #[test]
    fn structural_problems_are_blocked() {
        for (url, title) in [
            (
                "https://example.com/\u{0001}",
                "Hidden characters in address",
            ),
            (
                "https://example.com/\u{202E}",
                "Hidden characters in address",
            ),
            ("https://example.com\\@evil.com", "Unsafe link format"),
            ("https://example.com/%00", "Unsafe link format"),
            ("ftp://example.com/file", "Unsupported link"),
            ("https://user:pass@evil.com", "Domain hidden in link"),
            ("https://a..com", "Invalid domain"),
        ] {
            let r = risk(url).unwrap_or_else(|| panic!("{url} should be blocked"));
            assert_eq!(r.level, RiskLevel::Block, "{url}");
            assert_eq!(r.title, title, "{url}");
        }
        let long = format!("https://example.com/{}", "a".repeat(5000));
        assert_eq!(risk(&long).unwrap().title, "Address too long");
        assert_eq!(risk("not a url at all").unwrap().level, RiskLevel::Block);
    }

    #[test]
    fn typosquats_are_blocked() {
        for url in [
            "https://gooogle.com/",
            "https://faceb00k.com/login",
            "https://g\u{043E}\u{043E}gle.com/",
        ] {
            let r = risk(url).unwrap_or_else(|| panic!("{url} not caught"));
            assert_eq!(r.level, RiskLevel::Block, "{url}");
            assert!(r.title.contains("impersonation"), "{url}: {}", r.title);
        }
    }

    #[test]
    fn a_whole_script_lookalike_is_caught() {
        // "\u{430}\u{440}\u{440}\u{4cf}\u{435}" -- every letter Cyrillic, spells "apple".
        let r = risk("https://\u{0430}\u{0440}\u{0440}\u{04CF}\u{0435}.com/").unwrap();
        assert_eq!(r.level, RiskLevel::Block);
        assert!(r.title.contains("Apple"), "{}", r.title);
    }

    #[test]
    fn a_genuine_cyrillic_domain_is_left_alone() {
        assert_eq!(
            risk("https://\u{043F}\u{0440}\u{0438}\u{043C}\u{0435}\u{0440}.\u{0440}\u{0444}/"),
            None
        );
    }

    #[test]
    fn a_brand_name_padded_out_is_still_caught() {
        // The old rule gave up past a fixed length.
        for url in [
            "https://google-account-verify.com/login",
            "https://google-account-security-check.com/",
            "https://secure-login-for-your-google-account-verification.com/signin",
            "https://accounts.google.com.evil-cdn.example/signin",
        ] {
            let r = risk(url).unwrap_or_else(|| panic!("{url} not caught"));
            assert!(r.title.contains("Google"), "{url}: {}", r.title);
        }
    }

    #[test]
    fn heuristic_matches_stay_warnings_unless_something_else_is_wrong() {
        let r = risk("https://google-account-help.example/").unwrap();
        assert_eq!(r.level, RiskLevel::Warning);
    }

    #[test]
    fn softer_signals_only_fire_on_a_credential_page() {
        assert_eq!(risk("https://myblog.top/posts/1"), None);
        assert_eq!(risk("https://example.com:8443/"), None);
        assert_eq!(risk("https://a.b.c.d.e.example.com/about"), None);

        assert_eq!(
            risk("https://free-gift.top/login").unwrap().title,
            "Risky login-page address"
        );
        assert_eq!(
            risk("https://example.com:8443/login").unwrap().title,
            "Unusual network port"
        );
        assert_eq!(
            risk("https://a.b.c.d.e.example.com/login").unwrap().title,
            "Too many subdomains"
        );
    }

    #[test]
    fn a_public_ip_warns() {
        let r = risk("https://8.8.8.8/").unwrap();
        assert_eq!(r.level, RiskLevel::Warning);
        assert_eq!(r.title, "Site has no domain name");
    }

    #[test]
    fn a_known_shortener_warns() {
        assert_eq!(
            risk("https://bit.ly/abc123").unwrap().title,
            "Shortened link"
        );
    }

    #[test]
    fn a_trailing_dot_warns() {
        assert_eq!(
            risk("https://example.com./").unwrap().title,
            "Unusual domain notation"
        );
    }

    #[test]
    fn skeleton_folds_lookalikes_to_latin() {
        assert_eq!(skeleton("g\u{043E}\u{043E}gle"), "google");
        assert_eq!(skeleton("faceb00k"), "facebook");
        assert_eq!(skeleton("pay-pal"), "paypal");
    }

    #[test]
    fn a_token_is_only_a_word_at_a_boundary() {
        assert!(token_reads_as_a_word(
            "googleaccounts",
            "googleaccounts",
            "google"
        ));
        assert!(token_reads_as_a_word("mygoogle", "my-google", "google"));
        assert!(!token_reads_as_a_word("livestream", "livestream", "steam"));
    }

    #[test]
    fn short_names_do_not_typosquat_each_other() {
        assert!(!edit_distance_at_most_one("tme", "tie"));
        assert!(edit_distance_at_most_one("gooogle", "google"));
    }

}
