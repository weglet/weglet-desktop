// Copyright 2026 Weglet - Licensed under Apache 2.0
//
// The lists the block list and the heuristics consult, one set per
// profile: built in, or replaced whole by a file in the profile directory.
// Owned by the profile rather than kept in a static, since one process
// runs several profiles and each one's files are its own.

use std::collections::HashSet;

use crate::blocklist::{parse_host_list, BUILT_IN_BLOCKLIST};
use crate::risk::{parse_brands, parse_word_list, BrandRule, BUILT_IN_BRANDS, BUILT_IN_SENSITIVE_WORDS};

#[derive(Debug)]
pub struct Lists {
    pub(crate) blocked_hosts: HashSet<String>,
    pub(crate) brands: Vec<BrandRule>,
    pub(crate) sensitive_words: Vec<String>,
}

impl Lists {
    // The compiled-in lists. Each is checked by a test in this crate, so a
    // malformed one fails the build rather than the browser.
    pub fn built_in() -> Self {
        Self {
            blocked_hosts: parse_host_list(BUILT_IN_BLOCKLIST),
            brands: parse_brands(BUILT_IN_BRANDS).unwrap_or_default(),
            sensitive_words: parse_word_list(BUILT_IN_SENSITIVE_WORDS),
        }
    }

    // Each replaces its built-in list entirely: someone who supplies a list
    // has a reason, and keeping entries they removed would defeat it.
    pub fn replace_blocklist(&mut self, text: &str) {
        self.blocked_hosts = parse_host_list(text);
    }

    // Rejected whole when malformed: half a rule set catches some
    // impostors and waves others through.
    pub fn replace_brands(&mut self, text: &str) -> Result<(), String> {
        self.brands = parse_brands(text)?;
        Ok(())
    }

    pub fn replace_sensitive_words(&mut self, text: &str) {
        self.sensitive_words = parse_word_list(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replaced_list_belongs_to_its_own_lists_only() {
        let mut own = Lists::built_in();
        own.replace_blocklist("only.example\n");
        let other = Lists::built_in();
        assert!(crate::is_blocked_host(&own, "only.example"));
        assert!(!crate::is_blocked_host(&other, "only.example"));
        assert!(crate::is_blocked_host(&other, "doubleclick.net"));
    }

    #[test]
    fn a_malformed_brand_file_leaves_the_rules_as_they_were() {
        let mut lists = Lists::built_in();
        let before = lists.brands.len();
        assert!(lists.replace_brands("[[brand]]\nname = ").is_err());
        assert_eq!(lists.brands.len(), before);
    }
}
