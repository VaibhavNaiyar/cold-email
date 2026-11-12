use std::collections::HashSet;

use crate::Email;

const MAX_WORDS: usize = 120;
const MAX_SUBJECT_CHARS: usize = 60;
const MAX_SENTENCE_WORDS: usize = 20;
const OPT_OUT: &str = "won't follow up";
const BANNED: &[&str] = &[
    "hope this finds you", "hope you're well", "dear ", "to whom it may concern", "pick your brain",
    "quick chat", "coffee chat", "let's connect", "perfect fit", "attaching my resume",
    // Claims only the sender can make. The writer has no way to know them.
    "i tried", "i've tried", "i used", "i've used", "i've been using", "i use ", "huge fan",
    "big fan", "i made you", "i built you", "i love your",
];
const LEFTOVERS: &[&str] = &["{{", "}}", "<MY "];
const ALWAYS_ALLOWED: &[&str] = &["I", "I'm", "I'd", "I'll", "I've", "AI", "ML", "CTO", "CEO"];

/// Every reason this email may not go to Drafts. Empty means it passes.
pub fn lint(email: &Email) -> Vec<String> {
    let mut problems = Vec::new();
    let text = format!("{}\n{}", email.subject, email.body);
    let lower = text.to_lowercase().replace('\u{2019}', "'");

    let words = email.body.split_whitespace().count();
    if words > MAX_WORDS {
        problems.push(format!("{words} words, limit is {MAX_WORDS}"));
    }
    if email.subject.is_empty() || email.subject.chars().count() > MAX_SUBJECT_CHARS {
        problems.push(format!("subject must be 1-{MAX_SUBJECT_CHARS} characters"));
    }
    if email.subject.contains(['\r', '\n']) {
        problems.push("subject has a line break".to_owned());
    }
    if text.chars().any(is_emoji) {
        problems.push("contains an emoji".to_owned());
    }
    if !lower.contains(OPT_OUT) {
        problems.push("opt-out line is missing".to_owned());
    }
    problems.extend(BANNED.iter().filter(|phrase| lower.contains(**phrase)).map(|phrase| {
        format!("uses the banned phrase \"{}\"", phrase.trim())
    }));
    problems.extend(LEFTOVERS.iter().filter(|mark| text.contains(**mark)).map(|mark| {
        format!("unfilled template text near \"{mark}\"")
    }));
    problems.extend(
        sentences(&email.body)
            .filter(|sentence| word_count(sentence) > MAX_SENTENCE_WORDS)
            .map(|sentence| format!("sentence over {MAX_SENTENCE_WORDS} words: {sentence}")),
    );
    problems
}

/// Numbers and proper nouns in `text` that `source` does not contain.
pub fn unsupported_facts(text: &str, source: &str) -> Vec<String> {
    let known: HashSet<String> = tokens(source).map(|token| token.to_lowercase()).collect();
    let mut unknown = Vec::new();
    for sentence in sentences(text) {
        for (position, token) in tokens(sentence).enumerate() {
            let number = token.chars().any(|c| c.is_ascii_digit());
            let name = position > 0 && token.chars().next().is_some_and(char::is_uppercase);
            let supported = known.contains(&token.to_lowercase()) || ALWAYS_ALLOWED.contains(&token);
            if (number || name) && !supported {
                unknown.push(token.to_owned());
            }
        }
    }
    unknown
}

fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || matches!(c, '\'' | '.' | '-' | '+')))
        .map(|token| token.trim_matches(['.', '\'', '-']))
        .filter(|token| !token.is_empty())
}

fn sentences(text: &str) -> impl Iterator<Item = &str> {
    text.split_inclusive(['\n', '!', '?'])
        .flat_map(|part| part.split(". "))
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn is_emoji(c: char) -> bool {
    matches!(u32::from(c), 0x2190..=0x2BFF | 0xFE0F | 0x1F000..=0x1FAFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn email(body: &str) -> Email {
        Email { subject: "a question".into(), body: format!("{body}\nReply no and I won't follow up.") }
    }

    #[test]
    fn a_short_honest_email_passes() {
        assert!(lint(&email("Hey Ada,\nSaw the seed round.")).is_empty());
    }

    #[test]
    fn catches_length_emoji_and_stock_phrases() {
        assert!(lint(&email(&"word ".repeat(121))).iter().any(|p| p.contains("limit is 120")));
        assert!(lint(&email("Nice round \u{1F680}")).iter().any(|p| p.contains("emoji")));
        assert!(!lint(&email("I hope this finds you well.")).is_empty());
    }

    #[test]
    fn requires_the_opt_out_line() {
        let email = Email { subject: "hi".into(), body: "Hello".into() };
        assert!(lint(&email).iter().any(|p| p.contains("opt-out")));
    }

    #[test]
    fn catches_claims_only_the_sender_could_make() {
        assert!(!lint(&email("I've been using your product.")).is_empty());
    }

    #[test]
    fn catches_unfilled_slots_and_long_sentences() {
        assert!(!lint(&email("Hey {{first_name}}")).is_empty());
        assert!(!lint(&email(&"word ".repeat(25))).is_empty());
    }

    #[test]
    fn subject_is_limited() {
        let long = Email { subject: "x".repeat(61), ..email("Hi") };
        assert!(!lint(&long).is_empty());
    }

    #[test]
    fn flags_numbers_and_names_missing_from_the_source() {
        let source = "Acme raised $3M from Sequoia. AI agents";
        assert!(unsupported_facts("Congrats on the $3M from Sequoia.", source).is_empty());
        assert_eq!(unsupported_facts("Congrats on the $5M.", source), ["5M"]);
        assert_eq!(unsupported_facts("Backed by Benchmark, right?", source), ["Benchmark"]);
    }

    #[test]
    fn sentence_openers_are_not_treated_as_names() {
        assert!(unsupported_facts("Agents are hard. Memory is harder.", "x").is_empty());
    }
}
