//! Interface text. AAD's interface is English only: every string in the code is its own display
//! text, and these helpers exist so call sites keep one way to mark and format user-visible text
//! (`tl!`, `tf!`, [`count`]). Command IDs, drawing data and automation are unaffected.

/// The text to display for `source` (English: the text itself).
pub fn t(source: &str) -> &str {
    source
}

/// Named placeholders: `{name}` is replaced by its value. Inserted values are never looked up or
/// substituted again, so a user's name containing `{n}` is preserved verbatim.
pub fn format(source: &str, values: &[(&str, String)]) -> String {
    let mut result = String::with_capacity(source.len());
    let mut rest = source;
    while let Some((before, tail)) = rest.split_once('{') {
        result.push_str(before);
        let Some((key, after)) = tail.split_once('}') else {
            result.push('{');
            rest = tail;
            break;
        };
        if let Some((_, value)) = values.iter().find(|(name, _)| *name == key) {
            result.push_str(value);
        } else {
            result.push('{');
            result.push_str(key);
            result.push('}');
        }
        rest = after;
    }
    result.push_str(rest);
    result
}

/// `one` for 1, else `other`, with `{n}` replaced by the number.
pub fn count(n: u64, one: &str, other: &str) -> String {
    format(if n == 1 { one } else { other }, &[("n", n.to_string())])
}

#[macro_export]
macro_rules! tl {
    ($source:literal) => {
        $crate::i18n::t($source)
    };
}

#[macro_export]
macro_rules! tf {
    ($source:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::i18n::format($source, &[$((stringify!($name), ($value).to_string())),*])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_replaced_once_and_user_text_is_kept_verbatim() {
        assert_eq!(format("{n} items in {where}", &[("n", "3".into()), ("where", "{n}".into())]), "3 items in {n}");
        assert_eq!(format("no {such} key", &[]), "no {such} key");
        assert_eq!(format("open { brace", &[]), "open { brace");
    }

    #[test]
    fn counts_use_the_singular_only_for_one() {
        assert_eq!(count(1, "{n} commit", "{n} commits"), "1 commit");
        assert_eq!(count(0, "{n} commit", "{n} commits"), "0 commits");
        assert_eq!(count(21, "{n} commit", "{n} commits"), "21 commits");
    }
}
