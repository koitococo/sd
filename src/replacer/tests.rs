use super::*;

use proptest::prelude::*;

proptest! {
    #[test]
    fn validate_doesnt_panic(s in r"(\PC*\$?){0,5}") {
        let _ = validate::validate_replace(&s);
    }

    // $ followed by a digit and a non-ident char or an ident char
    #[test]
    fn validate_ok(s in r"([^\$]*(\$([0-9][^a-zA-Z_0-9\$]|a-zA-Z_))?){0,5}") {
        validate::validate_replace(&s).unwrap();
    }

    // Force at least one $ followed by a digit and an ident char
    #[test]
    fn validate_err(s in r"[^\$]*?\$[0-9][a-zA-Z_]\PC*") {
        validate::validate_replace(&s).unwrap_err();
    }
}

fn replace(
    look_for: impl Into<String>,
    replace_with: impl Into<String>,
    literal: bool,
    flags: Option<&'static str>,
    src: &'static str,
    target: &'static str,
) {
    const UNLIMITED_REPLACEMENTS: usize = 0;
    let look_for = look_for.into();
    let replace_with = replace_with.into();
    
    let replacer = RegexReplacer::new(
        look_for.clone(),
        replace_with.clone(),
        literal,
        flags.map(ToOwned::to_owned),
        UNLIMITED_REPLACEMENTS,
    )
    .unwrap();
    assert_eq!(
        std::str::from_utf8(
            &replacer
                .replace(src.as_bytes(), false, false)
                .expect("No matched")
        ),
        Ok(target)
    );

    let replacer = FancyReplacer::new(
        look_for,
        replace_with,
        literal,
        flags.map(ToOwned::to_owned),
        UNLIMITED_REPLACEMENTS,
    )
    .unwrap();
    let r = replacer
        .replace(&src.to_string(), false, false)
        .expect("No matched").to_string();
    assert_eq!(r, target);
}

#[test]
fn default_global() {
    replace("a", "b", false, None, "aaa", "bbb");
}

#[test]
fn escaped_char_preservation() {
    replace("a", "b", false, None, "a\\n", "b\\n");
}

#[test]
fn case_sensitive_default() {
    replace("abc", "x", false, None, "abcABC", "xABC");
    replace("abc", "x", true, None, "abcABC", "xABC");
}

#[test]
fn sanity_check_literal_replacements() {
    replace("((special[]))", "x", true, None, "((special[]))y", "xy");
}

#[test]
fn unescape_regex_replacements() {
    replace("test", r"\n", false, None, "testtest", "\n\n");
}

#[test]
fn no_unescape_literal_replacements() {
    replace("test", r"\n", true, None, "testtest", r"\n\n");
}

#[test]
fn full_word_replace() {
    replace("abc", "def", false, Some("w"), "abcd abc", "abcd def");
}
