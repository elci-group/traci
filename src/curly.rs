//! Curly-brace expansion for path-like strings and patterns.
//!
//! Wraps `brace_expand` so that Traci can accept shell-style brace
//! expressions even when the invoking shell or environment does not
//! expand them (for example, when arguments are passed as a single
//! string, or on Windows).

use brace_expand::brace_expand;

/// Expand a single argument that may contain brace expressions.
///
/// Returns one item when the input contains no braces, or the full
/// cross-product of alternatives otherwise.
#[must_use]
pub fn expand_arg(input: &str) -> Vec<String> {
    brace_expand(input)
}

/// Expand a collection of arguments or patterns in-place.
#[must_use]
pub fn expand_args(inputs: &[String]) -> Vec<String> {
    inputs.iter().flat_map(|input| expand_arg(input)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_through_unbraced_input() {
        assert_eq!(expand_arg("src/lib.rs"), vec!["src/lib.rs"]);
    }

    #[test]
    fn expands_simple_alternatives() {
        assert_eq!(
            expand_arg("src/{foo,bar}.rs"),
            vec!["src/foo.rs", "src/bar.rs"]
        );
    }

    #[test]
    fn expands_multiple_brace_sets() {
        assert_eq!(
            expand_arg("{src,tests}/{foo,bar}.rs"),
            vec![
                "src/foo.rs",
                "src/bar.rs",
                "tests/foo.rs",
                "tests/bar.rs",
            ]
        );
    }

    #[test]
    fn expands_nested_braces() {
        assert_eq!(
            expand_arg("{a,{b,c}}.rs"),
            vec!["a.rs", "b.rs", "c.rs"]
        );
    }

    #[test]
    fn expands_pattern_collection() {
        assert_eq!(
            expand_args(&["{target,.git}/**".into()]),
            vec!["target/**", ".git/**"]
        );
    }
}
