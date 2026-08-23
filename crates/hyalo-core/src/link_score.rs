//! Confidence scoring for broken-link fix candidates (iter-212).
//!
//! # Why a dedicated scorer
//!
//! Until iter-212 the confidence attached to a fuzzy fix was a raw
//! Jaro-Winkler score over the two filename *stems*. Jaro-Winkler rewards a
//! shared prefix heavily, which is exactly the wrong bias for documentation
//! slugs: on the GitHub Docs corpus
//! `/actions/reference/actions-limits` → `graphql/reference/actions.md`
//! scored **0.9** (a wrong document) while a genuine relocation whose
//! basename matched byte-for-byte was reported at the flat
//! `BasenameFallback` constant **0.6**. The ordering was inverted relative to
//! usefulness, so a bare `--apply-fuzzy` applied the garbage and the
//! confidence number could not be trusted for gating.
//!
//! # The model
//!
//! A candidate is scored on two independent features and the *basename*
//! dominates:
//!
//! ```text
//! confidence = 0.7 · basename_similarity + 0.3 · directory_similarity
//! ```
//!
//! * **basename similarity** — a soft token F1 over the slug tokens of the two
//!   filename stems (`actions-limits` → `["actions", "limits"]`). Each token
//!   is matched against its best partner in the other stem, but only counts
//!   when that pairing clears [`TOKEN_MATCH_FLOOR`]; below it the token is
//!   unmatched and scores 0. The floor is what stops Jaro's ~0.5–0.65 noise
//!   between unrelated English words from masquerading as partial credit,
//!   while still absorbing typos (`acions` ≈ `actions`).
//! * **directory similarity** — three quarters shared *leading* components,
//!   one quarter unordered token overlap. Generic levels (`how-tos`,
//!   `reference`) are shared by thousands of unrelated GitHub Docs pages, so
//!   membership alone made `actions/how-tos/x` look like a neighbour of
//!   `billing/how-tos/y`; the prefix term is what encodes "same section".
//!   Two empty directory lists are a perfect match; one empty and one not is a
//!   total mismatch, because that is precisely the "throw away the location the
//!   author wrote" case (`/actions` → `graphql/reference/actions.md`).
//!
//! The weights are deliberately lopsided: an identical basename in a
//! completely unrelated directory lands on exactly
//! [`BASENAME_WEIGHT`] (0.7), below the default apply floor
//! [`DEFAULT_FUZZY_MIN_CONFIDENCE`] (0.8), so a cross-tree same-name
//! substitution is reported but not written unless the user lowers the bar.
//! A relocation one level away inside the same section
//! (`a/b/c/page` → `a/b/d/page`) lands near 0.89 and is written.

use std::collections::HashSet;

/// Weight of the basename (final path segment) feature.
///
/// Also the confidence of a candidate whose basename matches perfectly but
/// whose directory shares nothing with the target — deliberately just below
/// [`DEFAULT_FUZZY_MIN_CONFIDENCE`].
pub const BASENAME_WEIGHT: f64 = 0.7;

/// Weight of the directory-path feature. `BASENAME_WEIGHT + DIR_WEIGHT == 1`.
pub const DIR_WEIGHT: f64 = 1.0 - BASENAME_WEIGHT;

/// Minimum Jaro-Winkler score for two slug tokens to count as the same token.
///
/// Jaro-Winkler almost never drops below ~0.5 for two real English words, so
/// without a floor every unrelated token pair contributes partial credit and
/// long slugs score high against short ones. 0.85 admits typos and small
/// morphological differences (`getting`/`get`) and rejects the rest.
pub const TOKEN_MATCH_FLOOR: f64 = 0.85;

/// Default minimum confidence a fuzzy/basename-fallback fix must reach before
/// `--apply-fuzzy` writes it (iter-212).
///
/// Before iter-212 a bare `--apply-fuzzy` accepted *every* proposal; on the
/// GitHub Docs corpus that was 1,047 rewrites, most of them to unrelated
/// documents. Override with `--min-confidence <0..1>` or
/// `[links] fuzzy_min_confidence` in `.hyalo.toml`; `--min-confidence 0`
/// restores the old accept-everything behaviour.
pub const DEFAULT_FUZZY_MIN_CONFIDENCE: f64 = 0.8;

/// Split a link target or vault-relative path into its directory components
/// and its filename stem (lowercased, `.md` stripped).
///
/// Accepts both `/` and `\` separators, tolerates a leading `/` and `./`
/// segments, and never panics on an empty input.
#[must_use]
pub fn split_path(path: &str) -> (Vec<&str>, &str) {
    let trimmed = path.trim_start_matches(['/', '\\']);
    let mut components: Vec<&str> = trimmed
        .split(['/', '\\'])
        .filter(|c| !c.is_empty() && *c != ".")
        .collect();
    let file = components.pop().unwrap_or("");
    let stem = file
        .strip_suffix(".md")
        .or_else(|| file.strip_suffix(".MD"))
        .or_else(|| {
            // Mixed case (`.Md`) — only strip when the extension really is md.
            let (head, ext) = file.rsplit_once('.')?;
            ext.eq_ignore_ascii_case("md").then_some(head)
        })
        .unwrap_or(file);
    (components, stem)
}

/// Split a slug into lowercase alphanumeric tokens.
///
/// `actions-minute-multipliers` → `["actions", "minute", "multipliers"]`.
/// Any non-alphanumeric run is a separator, so `-`, `_`, `.`, spaces and
/// `%20`-style leftovers all behave the same.
fn tokenize(slug: &str) -> Vec<String> {
    slug.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Best match for `token` among `others`, or `0.0` when nothing clears
/// [`TOKEN_MATCH_FLOOR`].
fn best_token_match(token: &str, others: &[String]) -> f64 {
    others
        .iter()
        .map(|o| strsim::jaro_winkler(token, o))
        .filter(|s| *s >= TOKEN_MATCH_FLOOR)
        .fold(0.0_f64, f64::max)
}

/// Soft token F1: the harmonic mean of how well each side is covered by the
/// other, where coverage is the mean best-match score per token.
///
/// Two empty token lists are identical (`1.0`); exactly one empty list is a
/// total mismatch (`0.0`). Using the harmonic mean rather than the arithmetic
/// one is what penalises a short slug matching a prefix of a long one:
/// `actions` is fully covered by `actions-limits` (recall 1.0) but only covers
/// half of it (precision 0.5), giving 0.67 instead of 0.75.
fn soft_token_f1(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let mean = |xs: &[String], ys: &[String]| -> f64 {
        xs.iter().map(|x| best_token_match(x, ys)).sum::<f64>() / xs.len() as f64
    };
    let precision = mean(a, b);
    let recall = mean(b, a);
    if precision + recall == 0.0 {
        return 0.0;
    }
    2.0 * precision * recall / (precision + recall)
}

/// Similarity of two filename stems in `[0.0, 1.0]`.
///
/// An exact (case-insensitive) match short-circuits to `1.0` so a genuine
/// relocation is never docked for tokenisation quirks.
#[must_use]
pub fn basename_similarity(a_stem: &str, b_stem: &str) -> f64 {
    if a_stem.eq_ignore_ascii_case(b_stem) {
        return 1.0;
    }
    soft_token_f1(&tokenize(a_stem), &tokenize(b_stem))
}

/// Weight of the shared-leading-components term inside
/// [`directory_similarity`]; the remainder goes to unordered token overlap.
///
/// Measured on the GitHub Docs corpus: without a strong prefix term, generic
/// path components (`how-tos`, `reference`, `guides`) are shared by thousands
/// of unrelated documents, so `actions/how-tos/x` scored 0.67 against
/// `billing/how-tos/y` and every cross-product same-name substitution cleared
/// the floor. Leading components are the section the author actually named.
const DIR_PREFIX_WEIGHT: f64 = 0.75;

/// Fraction of the deeper path that the two component lists share as a
/// *leading* run, compared case-insensitively.
fn common_prefix_ratio(a_dirs: &[&str], b_dirs: &[&str]) -> f64 {
    let shared = a_dirs
        .iter()
        .zip(b_dirs.iter())
        .take_while(|(x, y)| x.eq_ignore_ascii_case(y))
        .count();
    let deepest = a_dirs.len().max(b_dirs.len());
    if deepest == 0 {
        return 1.0;
    }
    #[allow(clippy::cast_precision_loss)]
    {
        shared as f64 / deepest as f64
    }
}

/// Similarity of two directory-component lists in `[0.0, 1.0]`.
///
/// Two terms, prefix-dominant:
///
/// * **shared leading components** ([`DIR_PREFIX_WEIGHT`]) — `a/b/c` and
///   `a/b/d` share two of three levels. This is the term that separates a
///   relocation *within* a section from a substitution *across* sections.
/// * **unordered token overlap** — the same soft token F1 used for basenames,
///   over the flattened components. It keeps a reorganisation that inserts or
///   reorders a level from collapsing to zero.
#[must_use]
pub fn directory_similarity(a_dirs: &[&str], b_dirs: &[&str]) -> f64 {
    if a_dirs.is_empty() && b_dirs.is_empty() {
        return 1.0;
    }
    if a_dirs.is_empty() || b_dirs.is_empty() {
        return 0.0;
    }
    let flatten = |dirs: &[&str]| -> Vec<String> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for d in dirs {
            for t in tokenize(d) {
                if seen.insert(t.clone()) {
                    out.push(t);
                }
            }
        }
        out
    };
    let overlap = soft_token_f1(&flatten(a_dirs), &flatten(b_dirs));
    let prefix = common_prefix_ratio(a_dirs, b_dirs);
    DIR_PREFIX_WEIGHT.mul_add(prefix, (1.0 - DIR_PREFIX_WEIGHT) * overlap)
}

/// Confidence that `candidate` (a vault-relative path) is the document the
/// broken `target` meant, in `[0.0, 1.0]`.
///
/// `target` should already have any site prefix stripped so both sides are
/// expressed in the same coordinate system.
#[must_use]
pub fn candidate_confidence(target: &str, candidate: &str) -> f64 {
    let (target_dirs, target_stem) = split_path(target);
    let (cand_dirs, cand_stem) = split_path(candidate);
    let base = basename_similarity(target_stem, cand_stem);
    let dirs = directory_similarity(&target_dirs, &cand_dirs);
    (BASENAME_WEIGHT * base + DIR_WEIGHT * dirs).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn split_path_handles_prefixes_and_extensions() {
        assert_eq!(split_path("a/b/c.md"), (vec!["a", "b"], "c"));
        assert_eq!(split_path("/a/b/c.MD"), (vec!["a", "b"], "c"));
        assert_eq!(split_path("./a/c.Md"), (vec!["a"], "c"));
        assert_eq!(split_path("c"), (Vec::new(), "c"));
        assert_eq!(split_path(""), (Vec::new(), ""));
        assert_eq!(split_path("a\\b\\c.md"), (vec!["a", "b"], "c"));
        // A non-md extension is part of the stem — link targets can be assets.
        assert_eq!(split_path("a/logo.png"), (vec!["a"], "logo.png"));
    }

    #[test]
    fn identical_basenames_score_one() {
        assert!(approx(basename_similarity("a-b-c", "a-b-c"), 1.0));
        assert!(approx(basename_similarity("A-B-C", "a-b-c"), 1.0));
    }

    #[test]
    fn typos_survive_the_token_floor() {
        // The classic fuzzy use case must keep working.
        assert!(basename_similarity("acions", "actions") > 0.9);
        assert!(basename_similarity("configuraton", "configuration") > 0.9);
    }

    #[test]
    fn shared_prefix_no_longer_inflates() {
        // `actions-limits` vs `actions`: Jaro-Winkler alone says 0.9.
        let s = basename_similarity("actions-limits", "actions");
        assert!(s < 0.7, "expected the extra token to cost, got {s}");
        assert!(s > 0.5, "one of two tokens still matches, got {s}");
        assert!(strsim::jaro_winkler("actions-limits", "actions") > 0.85);
    }

    #[test]
    fn unrelated_slugs_score_near_zero() {
        let s = basename_similarity("actions-minute-multipliers", "actions-built-in-queries");
        assert!(s < 0.35, "expected near-zero, got {s}");
    }

    #[test]
    fn directory_similarity_edges() {
        assert!(approx(directory_similarity(&[], &[]), 1.0));
        assert!(approx(directory_similarity(&[], &["a"]), 0.0));
        assert!(approx(directory_similarity(&["a"], &[]), 0.0));
        assert!(approx(directory_similarity(&["a", "b"], &["a", "b"]), 1.0));
        // Order matters for the prefix term but not for the overlap term, so a
        // pure reordering keeps partial credit without scoring as identical.
        let reordered = directory_similarity(&["a", "b"], &["b", "a"]);
        assert!(reordered > 0.0 && reordered < 1.0, "got {reordered}");
        // A shared *trailing* component is worth far less than a shared
        // leading one: `reference` is generic, the section name is not.
        let cross_section =
            directory_similarity(&["actions", "reference"], &["graphql", "reference"]);
        let same_section = directory_similarity(&["actions", "reference"], &["actions", "guides"]);
        assert!(
            same_section > cross_section,
            "same_section={same_section} cross_section={cross_section}"
        );
        assert!(cross_section < 0.2, "got {cross_section}");
    }

    #[test]
    fn cross_tree_same_name_sits_just_below_the_floor() {
        let c = candidate_confidence("actions", "graphql/reference/actions.md");
        assert!(approx(c, BASENAME_WEIGHT), "got {c}");
        assert!(
            c < DEFAULT_FUZZY_MIN_CONFIDENCE,
            "a cross-tree same-name guess must not clear the default floor"
        );
    }

    /// The three proposals from the v0.21.0-pre2 dogfood report must reorder:
    /// the only correct one has to score highest.
    #[test]
    fn dogfood_examples_reorder() {
        let wrong_a = candidate_confidence(
            "actions/reference/actions-limits",
            "graphql/reference/actions.md",
        );
        let wrong_b = candidate_confidence(
            "billing/reference/actions-minute-multipliers",
            "code-security/code-scanning/actions-built-in-queries.md",
        );
        let correct = candidate_confidence(
            "code-security/how-tos/scan-code-for-vulnerabilities/configuring-larger-runners-for-default-setup",
            "code-security/how-tos/find-and-fix-issues/configuring-larger-runners-for-default-setup.md",
        );
        assert!(
            correct > wrong_a && correct > wrong_b,
            "correct={correct} wrong_a={wrong_a} wrong_b={wrong_b}"
        );
        assert!(
            correct >= DEFAULT_FUZZY_MIN_CONFIDENCE,
            "the correct relocation must clear the default floor, got {correct}"
        );
        assert!(
            wrong_a < DEFAULT_FUZZY_MIN_CONFIDENCE && wrong_b < DEFAULT_FUZZY_MIN_CONFIDENCE,
            "wrong_a={wrong_a} wrong_b={wrong_b} must both fall below the floor"
        );
    }

    #[test]
    fn same_directory_typo_stays_applicable() {
        let c = candidate_confidence("guides/configuraton", "guides/configuration.md");
        assert!(
            c >= DEFAULT_FUZZY_MIN_CONFIDENCE,
            "a same-directory typo is the legitimate fuzzy case, got {c}"
        );
    }

    #[test]
    fn confidence_is_bounded() {
        for (a, b) in [
            ("", ""),
            ("", "a/b.md"),
            ("a/b.md", ""),
            ("-----", "a.md"),
            ("a/b/c/d/e/f", "f.md"),
        ] {
            let c = candidate_confidence(a, b);
            assert!((0.0..=1.0).contains(&c), "{a} vs {b} => {c}");
        }
    }
}
