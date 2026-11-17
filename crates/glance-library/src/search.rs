//! The FTS5 side of the library as Swift MaCrate writes and queries it: the `filename` token stored per row and the
//! search-box sanitiser (`MetadataStore.filenameToken(forPath:)`, `MetadataStore.sanitizeSearchQuery`).

use crate::unicode::{file_system_representation, is_alphanumeric, is_prepend, joins_previous};

/// The last path component without its extension, as `URL(fileURLWithPath:).deletingPathExtension()
/// .lastPathComponent` gives it: in the file-system representation (decomposed), trailing slashes ignored, and the
/// extension being whatever follows the last `.` when that dot is neither the first nor the last character
/// (`.hidden` and `name.` keep theirs; `sp ace.m ov` gives `sp ace`). `/` alone stays `/`.
pub fn filename_token(path: &str) -> String {
    let decomposed = file_system_representation(path);
    let trimmed = decomposed.trim_end_matches('/');
    if trimmed.is_empty() {
        return if decomposed.is_empty() { String::new() } else { "/".to_string() };
    }
    let last = trimmed.rsplit('/').next().unwrap_or(trimmed);
    match last.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < last.len() => last.get(..dot).unwrap_or(last).to_string(),
        _ => last.to_string(),
    }
}

/// `MetadataStore.sanitizeSearchQuery`: every scalar outside `CharacterSet.alphanumerics` becomes a space, the
/// result is split into `Character`s at whitespace, and each token becomes the quoted prefix term `"token"*`, joined
/// by single spaces (an implicit AND over every indexed column). Empty when nothing usable is left.
///
/// The split is by `Character`: a combining mark right after a separator belongs to the separator's `Character`
/// and is dropped with it, and a space right after a Prepend letter belongs to that letter's `Character` and stays
/// in the token — both as Swift does.
pub fn sanitize_search_query(raw: &str) -> String {
    let mapped: Vec<char> = raw.chars().map(|c| if is_alphanumeric(c) { c } else { ' ' }).collect();
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut cluster_is_space = false;
    let mut previous: Option<char> = None;
    for &c in &mapped {
        let continues = previous.is_some_and(|p| (c != ' ' && joins_previous(c)) || (p != ' ' && is_prepend(p)));
        if !continues {
            cluster_is_space = c == ' ';
            if cluster_is_space && !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        }
        if !cluster_is_space {
            current.push(c);
        }
        previous = Some(c);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens.iter().map(|t| format!("\"{t}\"*")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // MetadataStoreTests.testSanitizeSearchQueryProducesValidFTSExpression
    #[test]
    fn sanitiser() {
        assert_eq!(sanitize_search_query(""), "");
        assert_eq!(sanitize_search_query("   "), "");
        assert_eq!(sanitize_search_query("()*\"':"), "");
        assert_eq!(sanitize_search_query("blue"), "\"blue\"*");
        assert_eq!(sanitize_search_query("blue intro"), "\"blue\"* \"intro\"*");
        assert_eq!(sanitize_search_query("blue's"), "\"blue\"* \"s\"*");
        assert_eq!(sanitize_search_query("blue_intro"), "\"blue\"* \"intro\"*");
        assert_eq!(sanitize_search_query("blue-intro"), "\"blue\"* \"intro\"*");
    }

    #[test]
    fn tokens() {
        assert_eq!(filename_token("/a/blue_intro.mov"), "blue_intro");
        assert_eq!(filename_token("/a/.hidden"), ".hidden");
        assert_eq!(filename_token("/a/name."), "name.");
        assert_eq!(filename_token("/a/b.c/"), "b");
        assert_eq!(filename_token("/"), "/");
    }
}
