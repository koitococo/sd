use std::borrow::Cow;

use crate::Result;

#[cfg(test)]
mod tests;
mod validate;

pub use validate::{validate_replace, InvalidReplaceCapture};

pub(crate) trait Replacer<T: ?Sized + ToOwned> {
    fn new(
        look_for: String,
        replace_with: String,
        is_literal: bool,
        flags: Option<String>,
        replacements: usize,
    ) -> Result<Self>
    where
        Self: Sized;

    fn replace<'a>(
        &'a self,
        content: &'a T,
        only_matched: bool,
        use_color: bool,
    ) -> Option<Cow<'a, T>>;
}

pub(crate) struct RegexReplacer {
    regex: regex::bytes::Regex,
    replace_with: Vec<u8>,
    is_literal: bool,
    replacements: usize,
}

impl Replacer<[u8]> for RegexReplacer {
    fn new(
        look_for: String,
        replace_with: String,
        is_literal: bool,
        flags: Option<String>,
        replacements: usize,
    ) -> Result<Self> {
        let (look_for, replace_with) = if is_literal {
            (regex::escape(&look_for), replace_with.into_bytes())
        } else {
            validate_replace(&replace_with)?;

            (
                look_for,
                unescape::unescape(&replace_with)
                    .unwrap_or(replace_with)
                    .into_bytes(),
            )
        };

        let mut regex = regex::bytes::RegexBuilder::new(&look_for);
        regex.multi_line(true);

        if let Some(flags) = flags {
            flags.chars().for_each(|c| {
                #[rustfmt::skip]
                match c {
                    'c' => { regex.case_insensitive(false); },
                    'i' => { regex.case_insensitive(true); },
                    'e' => { regex.multi_line(false); },
                    's' => {
                        if !flags.contains('m') {
                            regex.multi_line(false);
                        }
                        regex.dot_matches_new_line(true);
                    },
                    'w' => {
                        regex = regex::bytes::RegexBuilder::new(&format!(
                            "\\b{}\\b",
                            look_for
                        ));
                    },
                    _ => {},
                };
            });
        };

        Ok(Self {
            regex: regex.build()?,
            replace_with,
            is_literal,
            replacements,
        })
    }

    fn replace<'a>(
        &'a self,
        content: &'a [u8],
        only_matched: bool,
        use_color: bool,
    ) -> Option<Cow<'a, [u8]>> {
        let regex = &self.regex;
        let limit = self.replacements;
        if self.is_literal {
            RegexReplacer::replacen(
                regex,
                limit,
                content,
                use_color,
                regex::bytes::NoExpand(&self.replace_with),
                only_matched,
            )
        } else {
            RegexReplacer::replacen(
                regex,
                limit,
                content,
                use_color,
                &*self.replace_with,
                only_matched,
            )
        }
    }
}

impl RegexReplacer {
    /// A modified form of [`regex::bytes::Regex::replacen`] that supports
    /// coloring replacements
    fn replacen<'haystack, R: regex::bytes::Replacer>(
        regex: &regex::bytes::Regex,
        limit: usize,
        haystack: &'haystack [u8],
        use_color: bool,
        mut rep: R,
        only_matched: bool,
    ) -> Option<Cow<'haystack, [u8]>> {
        let mut it = regex.captures_iter(haystack).enumerate().peekable();
        _ = it.peek()?;
        let mut new = Vec::with_capacity(haystack.len());
        let mut last_match = 0;
        for (i, cap) in it {
            // unwrap on 0 is OK because captures only reports matches
            let m = cap.get(0).unwrap();
            if !only_matched {
                new.extend_from_slice(&haystack[last_match..m.start()]);
                if use_color {
                    new.extend_from_slice(
                        ansi_term::Color::Blue.prefix().to_string().as_bytes(),
                    );
                }
            }
            rep.replace_append(&cap, &mut new);
            if !only_matched && use_color {
                new.extend_from_slice(
                    ansi_term::Color::Blue.suffix().to_string().as_bytes(),
                );
            }
            last_match = m.end();
            if limit > 0 && i >= limit - 1 {
                break;
            }
        }
        if !only_matched {
            new.extend_from_slice(&haystack[last_match..]);
        }
        Some(Cow::Owned(new))
    }
}

pub(crate) struct FancyReplacer {
    regex: fancy_regex::Regex,
    replace_with: String,
    is_literal: bool,
    replacements: usize,
}

impl Replacer<str> for FancyReplacer {
    fn new(
        look_for: String,
        replace_with: String,
        is_literal: bool,
        flags: Option<String>,
        replacements: usize,
    ) -> Result<Self>
    where
        Self: Sized,
    {
        let (look_for, replace_with) = if is_literal {
            (fancy_regex::escape(&look_for).to_string(), replace_with)
        } else {
            validate_replace(&replace_with)?;

            (
                look_for,
                unescape::unescape(&replace_with).unwrap_or(replace_with),
            )
        };
        let mut regex = fancy_regex::RegexBuilder::new(&look_for);
        // regex.multi_line(true);
        if let Some(flags) = flags {
            flags.chars().for_each(|c| {
                #[rustfmt::skip]
                match c {
                    'c' => { regex.case_insensitive(false); },
                    'i' => { regex.case_insensitive(true); },
                    'w' => {
                        regex = fancy_regex::RegexBuilder::new(&format!(
                            "\\b{}\\b",
                            look_for
                        ));
                    },
                    _ => {},
                };
            });
        };
        Ok(Self {
            regex: regex.build()?,
            replace_with,
            is_literal,
            replacements,
        })
    }

    fn replace<'a>(
        &'a self,
        content: &'a str,
        only_matched: bool,
        use_color: bool,
    ) -> Option<Cow<'a, str>> {
        let regex = &self.regex;
        let limit = self.replacements;
        if self.is_literal {
            FancyReplacer::replacen(
                regex,
                limit,
                content,
                use_color,
                fancy_regex::NoExpand(&self.replace_with),
                only_matched,
            )
        } else {
            FancyReplacer::replacen(
                regex,
                limit,
                content,
                use_color,
                &*self.replace_with,
                only_matched,
            )
        }
    }
}

impl FancyReplacer {
    fn replacen<'haystack, R: fancy_regex::Replacer>(
        regex: &fancy_regex::Regex,
        limit: usize,
        haystack: &'haystack str,
        use_color: bool,
        mut rep: R,
        only_matched: bool,
    ) -> Option<Cow<'haystack, str>> {
        let mut it = regex.captures_iter(haystack).enumerate().peekable();
        _ = it.peek()?;
        let mut new = String::new();
        let mut last_match = 0;
        for (i, cap) in it {
            // unwrap on 0 is OK because captures only reports matches
            let cap = cap.ok()?;
            let m = cap.get(0).unwrap();
            if !only_matched {
                new.push_str(&haystack[last_match..m.start()]);
                if use_color {
                    new.push_str(
                        ansi_term::Color::Blue.prefix().to_string().as_str(),
                    );
                }
            }
            rep.replace_append(&cap, &mut new);
            if !only_matched && use_color {
                new.push_str(
                    ansi_term::Color::Blue.suffix().to_string().as_str(),
                );
            }
            last_match = m.end();
            if limit > 0 && i >= limit - 1 {
                break;
            }
        }
        if !only_matched {
            new.push_str(&haystack[last_match..]);
        }
        Some(Cow::Owned(new))
    }
}
