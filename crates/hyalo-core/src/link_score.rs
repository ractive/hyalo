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
//!   filename stems (`actions-limits` → `["actions", "limits"]`, `CatMuse` →
//!   `["cat", "muse"]`). Each token is matched against its best partner in the
//!   other stem, but only counts when that pairing clears
//!   [`TOKEN_MATCH_FLOOR`]; below it the token is unmatched and scores 0. The
//!   floor is what stops Jaro's ~0.5–0.65 noise between unrelated English
//!   words from masquerading as partial credit, while still absorbing typos
//!   (`acions` ≈ `actions`). The F1 is then scaled by the pairing's
//!   **explained mass**, so that a whole *word* neither side accounts for costs
//!   in proportion to how much of the name it is.
//!
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
//!
//! # Near-neighbour stems (iter-279, DEC-324)
//!
//! DEC-319's runner-up margin damps a fuzzy winner that only just outran a
//! rival. It cannot touch a wrong candidate whose rivals are absent or far
//! away — there the *absolute* score is simply too generous. Three signals in
//! the basename feature keep such a candidate below the apply floor:
//!
//! * [`tokenize`] splits camelCase, so an Obsidian note name missing a whole
//!   word (`Cat` vs `CatMuse`) is two tokens against one rather than one
//!   opaque token that looks like a typo of the other.
//! * [`token_similarity`] admits a pair on plain Jaro, not Jaro-Winkler,
//!   unless their common prefix consumes the shorter token
//!   ([`shares_dominant_prefix`]) — `creat` is all but the `e` of `create`, but
//!   `paul` is four of eleven in `paulbricman`, and a shared given name must
//!   not buy token identity.
//! * [`scored_token_f1`] charges for unmatched tokens by their character share,
//!   so a dropped word (`obsidian-floating-toc-plugin` /
//!   `obsidian-plugin-toc`) is not absorbed by a forgiving harmonic mean.
//!
//! # The candidacy gate agrees (iter-280, DEC-325)
//!
//! None of the above matters for a pair the *candidacy gate* never shortlists.
//! That gate — `LinkMatcher::fuzzy_shortlist` — is a raw Jaro-Winkler prefilter
//! over filename stems, and until iter-280 it ran on the stems as written: the
//! scorer rated `my-long-note` against `MyLongNote` a perfect 1.0 while the gate
//! put the same pair at 0.53 and dropped it. [`gate_key`] is the normal form
//! that settles the disagreement — the same tokens, joined with `-` — and it is
//! the identity function on a plain lowercase-hyphen slug, so nothing about the
//! documentation corpora the gate was tuned on changes.

use std::collections::HashSet;

/// Weight of the basename (final path segment) feature.
///
/// Also the confidence of a candidate whose basename matches perfectly but
/// whose directory shares nothing with the target — deliberately just below
/// [`DEFAULT_FUZZY_MIN_CONFIDENCE`].
pub const BASENAME_WEIGHT: f64 = 0.7;

/// Weight of the directory-path feature. `BASENAME_WEIGHT + DIR_WEIGHT == 1`.
pub const DIR_WEIGHT: f64 = 1.0 - BASENAME_WEIGHT;

/// Minimum score for two slug tokens to count as the same token.
///
/// Jaro-Winkler almost never drops below ~0.5 for two real English words, so
/// without a floor every unrelated token pair contributes partial credit and
/// long slugs score high against short ones. 0.85 admits typos and small
/// morphological differences (`getting`/`get`) and rejects the rest.
///
/// Which similarity is measured against the floor is decided by
/// [`token_similarity`] (iter-279): Winkler's prefix bonus may *sharpen* a
/// match but may not *create* one.
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
///
/// iter-279: an alphanumeric run is additionally split at camelCase
/// boundaries, so `CatMuse` → `["cat", "muse"]`. Obsidian vaults name notes in
/// prose case with no separator at all (`CatMuse.md`, `HTMLParser.md`), and
/// without this the whole note name is one opaque token: `Cat` then looks like
/// a *typo* of `CatMuse` (Jaro-Winkler 0.867, above [`TOKEN_MATCH_FLOOR`])
/// rather than what it is — a name missing a whole word. The split also makes
/// the two naming conventions comparable, so `MyNote` matches `my-note`.
fn tokenize(slug: &str) -> Vec<String> {
    let mut out = Vec::new();
    for run in slug
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
    {
        push_camel_tokens(run, &mut out);
    }
    out
}

/// Split one alphanumeric run at its camelCase boundaries, lowercasing each
/// piece into `out`.
///
/// A boundary sits before an uppercase letter that follows a lowercase one
/// (`catMuse`), and before the final uppercase of an uppercase run that is
/// followed by a lowercase one (`HTMLParser` → `html` + `parser`). A run with
/// no case transition — `catmuse`, `README`, `279` — stays whole.
fn push_camel_tokens(run: &str, out: &mut Vec<String>) {
    let chars: Vec<char> = run.chars().collect();
    let mut start = 0usize;
    for i in 1..chars.len() {
        let prev = chars[i - 1];
        let cur = chars[i];
        let lower_to_upper = prev.is_lowercase() && cur.is_uppercase();
        let acronym_tail = prev.is_uppercase()
            && cur.is_uppercase()
            && chars.get(i + 1).is_some_and(|n| n.is_lowercase());
        if lower_to_upper || acronym_tail {
            out.push(chars[start..i].iter().collect::<String>().to_lowercase());
            start = i;
        }
    }
    out.push(chars[start..].iter().collect::<String>().to_lowercase());
}

/// Normal form of a filename stem for `LinkMatcher`'s fuzzy **candidacy gate**
/// (iter-280, DEC-325): [`tokenize`]'s words joined by a single `-`.
///
/// The gate is a cheap Jaro-Winkler prefilter that decides which files are even
/// worth scoring; until iter-280 it ran on the raw, case-sensitive stems, so a
/// pair the scorer rates 1.0 — `my-long-note` / `MyLongNote`, `html-parser` /
/// `HTMLParser` — never reached it (raw Jaro-Winkler 0.53 and 0.45, far under
/// the 0.8 floor). Normalising both sides first makes candidacy agree with
/// [`basename_similarity`] about what a stem *is*, without changing what the
/// gate costs: it is still one `strsim::jaro_winkler` call per file, over a
/// string precomputed once at matcher build.
///
/// The `-` join (rather than concatenating the tokens) is deliberate: a plain
/// lowercase-hyphen slug — the whole of the GitHub Docs and MDN corpora — is its
/// own gate key byte for byte, so on those vaults the gate admits exactly the
/// candidates it admitted before, at exactly the same cost. Only names carrying
/// case, spaces or `_` — Obsidian-style vaults — see a different key.
///
/// A stem with no alphanumeric character at all (`-----`) would tokenise to
/// nothing; it keys to itself instead, so two unrelated punctuation-only names
/// cannot meet at an empty-vs-empty score of 1.0.
#[must_use]
pub fn gate_key(stem: &str) -> String {
    let tokens = tokenize(stem);
    if tokens.is_empty() {
        return stem.to_string();
    }
    tokens.join("-")
}

/// Longest trailing remainder the shorter token may keep past the shared
/// prefix and still count as a prefix relationship (iter-281, DEC-326).
///
/// One character, because that is exactly what English gerund-to-imperative
/// morphology leaves behind: `creat` + `ing` against `creat` + **`e`**,
/// `manag`/`manage`, `enabl`/`enable`, `writ`/`write`, `us`/`use`,
/// `configur`/`configure`. Zero would reject every one of those; two admits
/// `excalidraw`/`excalibur`.
const MAX_SHORTER_REMAINDER: usize = 1;

/// `true` when the two tokens' common prefix consumes the shorter one, give or
/// take a single trailing character.
///
/// This is the question Jaro-Winkler's prefix bonus *should* ask and does not:
/// Winkler credits a shared prefix up to four characters regardless of how much
/// of the words that is. Four characters are the whole of `get` in `getting`
/// and five are all but the `e` of `create` in `creating` — real morphology —
/// but they are barely a third of `paulbricman` and `paultreanor`, two
/// different people.
///
/// # Why the remainder and not the share (iter-281, DEC-326)
///
/// Until iter-281 this asked only for the prefix to cover *at least half* of
/// the shorter token, and half is not a prefix relationship. `mathjax` and
/// `mathpad` are seven characters each and share `math`: four of seven on both
/// sides clears "half", so the Obsidian Hub's `[[Mathjax]]` — no such note
/// exists — was admitted as a single-token match against `Plugins/mathpad.md`
/// and reported at 0.886, over the [`DEFAULT_FUZZY_MIN_CONFIDENCE`] apply
/// floor, even though plain Jaro rates the pair 0.810, *below*
/// [`TOKEN_MATCH_FLOOR`]. Widening the candidacy gate in iter-280 (DEC-325) is
/// what exposed it; the defect is the exemption's.
///
/// A prefix relationship is definitionally *one word extending into another*:
/// the shorter token is spent, and only the longer one carries on. So the test
/// is the **shorter token's own remainder**, which is empty for `get`/`getting`
/// and `run`/`running` and a lone `e` for `create`/`creating` — but `pad`
/// against `jax`, two distinct words' worth, for `mathjax`/`mathpad`.
///
/// Two alternatives were weighed and rejected:
///
/// * *Raise the share* (demand three quarters rather than half). It happens to
///   separate the fixtures — 4/7 fails, 5/6 passes — but only by moving a
///   magic number until the known counter-example falls the right side of it.
///   It still calls two words that merely begin alike a prefix relationship,
///   and a longer such pair (`mathematics`/`mathematica`, 10 of 11) sails
///   through.
/// * *Require the lengths to differ.* True of a real prefix relationship, and
///   it does reject `mathjax`/`mathpad` — but only that exact shape. One letter
///   of slack (`mathjax`/`mathpads`) restores the false positive, and
///   `excalidraw`/`excalibur` never had equal lengths to begin with. Under the
///   remainder rule the requirement is redundant anyway: equal lengths plus a
///   remainder of at most one means the tokens differ in their last character
///   alone, which clears [`TOKEN_MATCH_FLOOR`] on plain Jaro and needs no
///   exemption.
///
/// Both arguments come from [`tokenize`] and are already lowercase.
fn shares_dominant_prefix(a: &str, b: &str) -> bool {
    let common = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
    let shorter = a.chars().count().min(b.chars().count());
    shorter > 0 && shorter - common <= MAX_SHORTER_REMAINDER
}

/// Similarity of two slug tokens, `0.0` when they are not the same token.
///
/// iter-279: Jaro-**Winkler** adds up to `0.4 · (1 − jaro)` for a shared
/// four-character prefix. That bonus exists to rank search results by a shared
/// beginning, and it is exactly the wrong instrument for deciding whether two
/// words *are* the same word: on the Obsidian Hub `paulbricman` scored 0.855
/// against `paultreanor` — two different people whose only common ground is a
/// given name — where the unprefixed Jaro is 0.758, far under
/// [`TOKEN_MATCH_FLOOR`]. So the bonus may *sharpen* a match but may not
/// *create* one: the pair must clear the floor on plain Jaro.
///
/// The exception is a pair whose common prefix *consumes* the shorter token
/// ([`shares_dominant_prefix`]) — `get` in `getting`, `creat` in
/// `create`/`creating`. There the shared beginning is not a coincidence between
/// two diverging words, it is nearly all of one of them, and the bonus is
/// measuring the real relationship: this is the morphology
/// [`TOKEN_MATCH_FLOOR`] was chosen to admit, and documentation renames slugs
/// that way constantly (GitHub Docs moved a whole tree from `creating-…` and
/// `managing-…` to `create-…` and `manage-…`).
///
/// The reported score is Jaro-Winkler either way; only admission changes.
fn token_similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let winkler = strsim::jaro_winkler(a, b);
    if winkler < TOKEN_MATCH_FLOOR {
        return 0.0;
    }
    let admitted = strsim::jaro(a, b) >= TOKEN_MATCH_FLOOR || shares_dominant_prefix(a, b);
    if admitted { winkler } else { 0.0 }
}

/// Best match for `token` among `others`, or `0.0` when nothing clears
/// [`TOKEN_MATCH_FLOOR`].
fn best_token_match(token: &str, others: &[String]) -> f64 {
    others
        .iter()
        .map(|o| token_similarity(token, o))
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
///
/// The result is then scaled by its explained mass (iter-279), which charges
/// for whole tokens the pairing leaves unmatched in proportion to how much of
/// the two names they are — 0.47 for that same `actions` / `actions-limits`
/// pair, where `limits` is six of the twenty characters in play.
fn soft_token_f1(a: &[String], b: &[String]) -> f64 {
    scored_token_f1(a, b).0
}

/// [`soft_token_f1`] together with the pairing's **explained mass**: `1 −` the
/// share of characters living in tokens left *entirely* unmatched, counted
/// across both sides.
///
/// iter-279: the F1 weights every token alike and its harmonic mean is
/// forgiving when one side is fully covered, so `obsidian-floating-toc-plugin`
/// against `obsidian-plugin-toc` scored 0.857 — three of four tokens matched,
/// recall a perfect 1.0 — even though the one unmatched token, `floating`, is
/// the word that names the plugin and a third of the target's characters. A
/// dropped or added *whole word* is not a near miss; weighting the residue by
/// how much of the name it is lands that pair at 0.694, while leaving every
/// pairing with no unmatched token — a typo, a punctuation change, a
/// relocation — untouched at its full F1.
///
/// The two numbers are returned separately because only the *basename* charges
/// for unmatched mass. A directory reorganisation renames whole levels by
/// design — `dependabot/dependabot-alerts` → `concepts/supply-chain-security`
/// is what a relocation *is* — and charging directory tokens by character
/// share pushed 828 GitHub Docs fixes whose basename matched byte-for-byte
/// below the apply floor. A dropped word in the *name* changes which document
/// is meant; a dropped word in the *path* is the move being described.
#[allow(clippy::cast_precision_loss)]
fn scored_token_f1(a: &[String], b: &[String]) -> (f64, f64) {
    if a.is_empty() && b.is_empty() {
        return (1.0, 1.0);
    }
    if a.is_empty() || b.is_empty() {
        return (0.0, 1.0);
    }
    // One pass per side accumulates both the coverage mean and the character
    // mass; this runs once per fuzzy candidate, so it holds no temporaries.
    let mut unmatched_chars = 0usize;
    let mut total_chars = 0usize;
    let mut side = |xs: &[String], ys: &[String]| -> f64 {
        let mut sum = 0.0;
        for x in xs {
            let score = best_token_match(x, ys);
            sum += score;
            let len = x.chars().count();
            total_chars += len;
            if score == 0.0 {
                unmatched_chars += len;
            }
        }
        sum / xs.len() as f64
    };
    let precision = side(a, b);
    let recall = side(b, a);
    if precision + recall == 0.0 {
        return (0.0, 1.0);
    }
    let f1 = 2.0 * precision * recall / (precision + recall);
    let mass = if total_chars == 0 {
        1.0
    } else {
        1.0 - unmatched_chars as f64 / total_chars as f64
    };
    (f1, mass)
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
    let (f1, mass) = scored_token_f1(&tokenize(a_stem), &tokenize(b_stem));
    f1 * mass
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
/// `target` must already be expressed in the same coordinate system as
/// `candidate`: site prefix stripped, `../` resolved against the source
/// directory. Whether the author asserted a location at all is inferred from
/// `target` containing a separator — call
/// [`candidate_confidence_with_claim`] directly when the written form and the
/// comparison form disagree (a site-absolute `/actions` normalises to a bare
/// `actions` but still claims the site root).
#[must_use]
pub fn candidate_confidence(target: &str, candidate: &str) -> f64 {
    let asserts_location = target.contains('/') || target.contains('\\');
    candidate_confidence_with_claim(target, candidate, asserts_location)
}

/// [`candidate_confidence`] with an explicit answer to "did the author assert
/// a location?".
///
/// When `asserts_location` is `false` the directory feature is *neutral*
/// rather than zero and the basename carries the whole score. A bare
/// `[[targt]]` claims no location (DEC-076 short-form semantics), so there is
/// nothing for a candidate's directory to contradict — docking it for living
/// in a subdirectory would be scoring it against a claim that was never made.
#[must_use]
pub fn candidate_confidence_with_claim(
    target: &str,
    candidate: &str,
    asserts_location: bool,
) -> f64 {
    let (target_dirs, target_stem) = split_path(target);
    let (cand_dirs, cand_stem) = split_path(candidate);
    let base = basename_similarity(target_stem, cand_stem).clamp(0.0, 1.0);
    if !asserts_location {
        return base;
    }
    let dirs = directory_similarity(&target_dirs, &cand_dirs);
    BASENAME_WEIGHT
        .mul_add(base, DIR_WEIGHT * dirs)
        .clamp(0.0, 1.0)
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
        // iter-279: the bound was 0.5 before the explained-mass charge counted the
        // unmatched `limits` by its six-of-twenty character share. Partial
        // credit for the token that *does* match still has to survive — this
        // pair must stay well clear of the near-zero band unrelated slugs land
        // in (0.089 for `actions-minute-multipliers` / `actions-built-in-queries`).
        assert!(s > 0.4, "one of two tokens still matches, got {s}");
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
    fn cross_tree_same_name_sits_below_the_floor() {
        // `/actions` normalises to a bare `actions` but still claims the site
        // root, so the claim is passed in explicitly (iter-200 / M-1).
        let c = candidate_confidence_with_claim("actions", "graphql/reference/actions.md", true);
        assert!(approx(c, BASENAME_WEIGHT), "got {c}");
        assert!(
            c < DEFAULT_FUZZY_MIN_CONFIDENCE,
            "a cross-tree same-name guess must not clear the default floor"
        );
    }

    #[test]
    fn a_bare_stem_is_not_docked_for_the_candidates_directory() {
        // `[[targt]]` asserts no location: the only evidence is the basename,
        // so a typo fix must keep its full score even though the candidate
        // lives in a subdirectory.
        let c = candidate_confidence("targt", "notes/target.md");
        assert!(
            c > 0.96,
            "expected the ~0.966 typo offer to survive, got {c}"
        );
        // The same text written as a location claim is a different question.
        let claimed = candidate_confidence_with_claim("targt", "notes/target.md", true);
        assert!(claimed < c, "claimed={claimed} bare={c}");
    }

    /// The three proposals from the v0.21.0-pre2 dogfood report (BUG-11) must
    /// reorder: the only correct one has to score highest. Paths are verbatim
    /// from the GitHub Docs corpus the report was measured on; the old
    /// Jaro-Winkler-on-stems confidences were 0.9 / 0.889 / 0.6 respectively —
    /// exactly backwards.
    #[test]
    fn dogfood_examples_reorder() {
        let wrong_a = candidate_confidence(
            "actions/reference/actions-limits",
            "graphql/reference/actions.md",
        );
        let wrong_b = candidate_confidence(
            "billing/reference/actions-minute-multipliers",
            "code-security/reference/code-scanning/codeql/codeql-queries/actions-built-in-queries.md",
        );
        let correct = candidate_confidence(
            "code-security/how-tos/scan-code-for-vulnerabilities/manage-your-configuration/configuring-larger-runners-for-default-setup",
            "code-security/how-tos/find-and-fix-code-vulnerabilities/manage-your-configuration/configuring-larger-runners-for-default-setup.md",
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

    // -----------------------------------------------------------------
    // iter-279 / DEC-324 — near-neighbour stems
    // -----------------------------------------------------------------

    /// The three wrong above-floor proposals DEC-319's runner-up margin could
    /// not reach, verbatim from the post-274 dogfood of the Obsidian Hub with
    /// their measured pre-iter-279 confidences. All three are bare wikilinks,
    /// so the basename carries the whole score.
    #[test]
    fn near_neighbour_stems_fall_below_the_apply_floor() {
        for (target, candidate, was) in [
            ("Cat", "CatMuse.md", 0.867),
            ("paulbricman", "paultreanor.md", 0.855),
            (
                "obsidian-floating-toc-plugin",
                "obsidian-plugin-toc.md",
                0.857,
            ),
        ] {
            let c = candidate_confidence(target, candidate);
            assert!(
                c < DEFAULT_FUZZY_MIN_CONFIDENCE,
                "[[{target}]] -> {candidate} was {was}, still {c}"
            );
        }
    }

    /// The counterpart the same dogfood named as correct: it must keep its
    /// perfect score, not merely stay above the floor.
    #[test]
    fn the_real_match_keeps_its_confidence() {
        let c = candidate_confidence("Obsidian Publish.", "Obsidian Publish.md");
        assert!(approx(c, 1.0), "got {c}");
    }

    #[test]
    fn camel_case_runs_split_into_words() {
        assert_eq!(tokenize("CatMuse"), vec!["cat", "muse"]);
        assert_eq!(tokenize("HTMLParser"), vec!["html", "parser"]);
        // No case transition: the run stays whole, so an all-lowercase or an
        // all-uppercase name is tokenised exactly as before.
        assert_eq!(tokenize("catmuse"), vec!["catmuse"]);
        assert_eq!(tokenize("README"), vec!["readme"]);
        assert_eq!(
            tokenize("obsidian-plugin-toc"),
            vec!["obsidian", "plugin", "toc"]
        );
        // The split makes the two naming conventions comparable to the scorer,
        // and since iter-280 (DEC-325) to `LinkMatcher`'s candidacy gate too:
        // both stems reach it as their `gate_key`, so `[[my-long-note]]` now
        // shortlists `MyLongNote.md` instead of being filtered out before the
        // scorer ever ran.
        assert!(approx(basename_similarity("MyNote", "my-note"), 1.0));
        assert_eq!(gate_key("MyNote"), gate_key("my-note"));
    }

    #[test]
    fn winkler_may_sharpen_a_token_match_but_not_create_one() {
        // Two different people sharing a given name: Jaro-Winkler clears the
        // floor only because of the four-character `paul` prefix.
        assert!(strsim::jaro_winkler("paulbricman", "paultreanor") >= TOKEN_MATCH_FLOOR);
        assert!(strsim::jaro("paulbricman", "paultreanor") < TOKEN_MATCH_FLOOR);
        assert!(approx(token_similarity("paulbricman", "paultreanor"), 0.0));
        // `paul` is four of eleven characters; Winkler credits it the same as
        // it credits five of six in `creat`.
        assert!(!shares_dominant_prefix("paulbricman", "paultreanor"));
        // A dominant shared prefix is the exception — the morphology
        // TOKEN_MATCH_FLOOR was chosen to admit, and the gerund-to-imperative
        // slug rename documentation sites do wholesale.
        assert!(token_similarity("get", "getting") >= TOKEN_MATCH_FLOOR);
        assert!(token_similarity("creating", "create") >= TOKEN_MATCH_FLOOR);
        assert!(token_similarity("managing", "manage") >= TOKEN_MATCH_FLOOR);
        assert!(token_similarity("running", "run") >= TOKEN_MATCH_FLOOR);
        assert!(
            basename_similarity("creating-a-composite-action", "create-a-composite-action")
                >= DEFAULT_FUZZY_MIN_CONFIDENCE,
            "GitHub Docs renamed a whole tree this way"
        );
        assert!(
            basename_similarity("get-started", "getting-started") >= DEFAULT_FUZZY_MIN_CONFIDENCE,
            "a morphological variant of one token must survive"
        );
        // A typo is admitted on plain Jaro, with no help from the prefix.
        assert!(token_similarity("acions", "actions") > 0.9);
        assert!(token_similarity("xctions", "actions") > 0.85);
    }

    #[test]
    fn an_unmatched_token_costs_its_character_share() {
        // Nothing unmatched: the F1 is reported untouched.
        let same = tokenize("alpha-beta");
        let (f1, mass) = scored_token_f1(&same, &same);
        assert!(approx(mass, 1.0), "got {mass}");
        assert!(approx(f1, 1.0), "got {f1}");
        // `floating` is 8 of the 42 characters in play.
        let a = tokenize("obsidian-floating-toc-plugin");
        let b = tokenize("obsidian-plugin-toc");
        let (f1, mass) = scored_token_f1(&a, &b);
        assert!((mass - (1.0 - 8.0 / 42.0)).abs() < 1e-9, "got {mass}");
        // Which is what takes the pair from 0.857 to below the apply floor.
        assert!((f1 - 6.0 / 7.0).abs() < 1e-9, "got {f1}");
        assert!(f1 * mass < DEFAULT_FUZZY_MIN_CONFIDENCE);
    }

    // -----------------------------------------------------------------
    // iter-281 / DEC-326 — the dominant-prefix exemption is a prefix
    // *relationship*, not a shared beginning
    // -----------------------------------------------------------------

    #[test]
    fn two_words_that_merely_begin_alike_are_not_one_token() {
        // The premise, exactly as iter-280 reported it: plain Jaro puts
        // `mathjax` and `mathpad` below the floor, and only the exemption
        // admitted them.
        assert!(strsim::jaro("mathjax", "mathpad") < TOKEN_MATCH_FLOOR);
        assert!(strsim::jaro_winkler("mathjax", "mathpad") >= TOKEN_MATCH_FLOOR);
        // Half of the shorter token — the pre-iter-281 bar — was cleared on
        // both sides: `math` is four of seven either way.
        let common = "mathjax"
            .chars()
            .zip("mathpad".chars())
            .take_while(|(x, y)| x == y)
            .count();
        assert_eq!(common, 4);
        assert!(common * 2 >= "mathjax".len(), "test premise: half was met");
        // And it is no longer enough.
        assert!(!shares_dominant_prefix("mathjax", "mathpad"));
        assert!(approx(token_similarity("mathjax", "mathpad"), 0.0));
        // Two plugin names sharing six characters go the same way.
        assert!(!shares_dominant_prefix("excalidraw", "excalibur"));
        assert!(approx(token_similarity("excalidraw", "excalibur"), 0.0));
        // One token each, so the whole basename feature collapses with it and
        // `[[Mathjax]]` lands far under the apply floor rather than at 0.886.
        assert!(approx(basename_similarity("mathjax", "mathpad"), 0.0));
        assert!(
            candidate_confidence("Mathjax", "Plugins/mathpad.md") < DEFAULT_FUZZY_MIN_CONFIDENCE,
            "got {}",
            candidate_confidence("Mathjax", "Plugins/mathpad.md")
        );
    }

    #[test]
    fn a_prefix_relationship_spends_the_shorter_token() {
        // Empty remainder: the shorter token is literally a prefix.
        for (a, b) in [("get", "getting"), ("run", "running"), ("plugin", "plugins")] {
            assert!(shares_dominant_prefix(a, b), "{a} / {b}");
        }
        // One character of remainder: the gerund-to-imperative `e`, which is
        // the whole reason MAX_SHORTER_REMAINDER is 1 and not 0.
        for (a, b) in [
            ("creating", "create"),
            ("managing", "manage"),
            ("enabling", "enable"),
            ("writing", "write"),
            ("configuring", "configure"),
        ] {
            assert!(shares_dominant_prefix(a, b), "{a} / {b}");
            assert!(token_similarity(a, b) >= TOKEN_MATCH_FLOOR, "{a} / {b}");
        }
        // Both sides carrying their own word-sized remainder is not that.
        for (a, b) in [
            ("mathjax", "mathpad"),
            ("mathjax", "mathpads"),
            ("paulbricman", "paultreanor"),
            ("mathematics", "mathematica"),
        ] {
            assert!(!shares_dominant_prefix(a, b), "{a} / {b}");
        }
        // The rejected alternative, recorded as a test: requiring the *lengths*
        // to differ would have admitted this pair, because they do.
        assert_ne!("mathjax".len(), "mathpads".len());
        assert!(!shares_dominant_prefix("mathjax", "mathpads"));
        // And an equal-length pair that survives the remainder rule differs in
        // its last character alone — which clears the floor on plain Jaro, so
        // the exemption is not what carries it.
        assert!(shares_dominant_prefix("notea", "noteb"));
        assert!(strsim::jaro("notea", "noteb") >= TOKEN_MATCH_FLOOR);
    }

    #[test]
    fn tightening_the_exemption_leaves_the_iteration_279_fixtures_alone() {
        // Every basename-level assertion iter-279 made about the exemption,
        // re-run against the narrower rule.
        assert!(
            basename_similarity("creating-a-composite-action", "create-a-composite-action")
                >= DEFAULT_FUZZY_MIN_CONFIDENCE
        );
        assert!(basename_similarity("get-started", "getting-started") >= DEFAULT_FUZZY_MIN_CONFIDENCE);
        // A typo still rides in on plain Jaro, with no prefix help at all.
        assert!(token_similarity("acions", "actions") > 0.9);
        assert!(!shares_dominant_prefix("acions", "actions"));
    }

    // -----------------------------------------------------------------
    // iter-280 / DEC-325 — the candidacy gate's normal form
    // -----------------------------------------------------------------

    #[test]
    fn gate_key_is_the_identity_on_a_plain_slug() {
        // The property the whole change rests on: on a lowercase-hyphen corpus
        // (GitHub Docs, MDN) the gate compares exactly the strings it compared
        // before, so neither its shortlist nor its cost moves.
        for stem in [
            "actions-limits",
            "getting-started",
            "code-scanning",
            "actions-minute-multipliers",
            "readme",
            "279",
            "",
        ] {
            assert_eq!(gate_key(stem), stem, "gate_key must not touch {stem}");
        }
    }

    #[test]
    fn gate_key_folds_case_and_separators_together() {
        assert_eq!(gate_key("MyLongNote"), "my-long-note");
        assert_eq!(gate_key("HTMLParser"), "html-parser");
        assert_eq!(gate_key("Obsidian Publish."), "obsidian-publish");
        assert_eq!(gate_key("my_long_note"), "my-long-note");
        // A stem with nothing to tokenise keys to itself, so two unrelated
        // punctuation-only names cannot meet at an empty-vs-empty 1.0.
        assert_eq!(gate_key("-----"), "-----");
        assert_eq!(gate_key("***"), "***");
        assert!(strsim::jaro_winkler(&gate_key("-----"), &gate_key("***")) < 0.5);
    }

    #[test]
    fn gate_key_lets_the_scorers_verdict_through() {
        // The pairs iteration 279 could score but never see. Both are 1.0 to
        // `basename_similarity`; the gate must now agree they are candidates.
        for (target, candidate) in [
            ("my-long-note", "MyLongNote"),
            ("html-parser", "HTMLParser"),
            ("my-note", "MyNote"),
        ] {
            assert!(
                strsim::jaro_winkler(target, candidate) < DEFAULT_FUZZY_MIN_CONFIDENCE,
                "test premise: the raw gate rejected {target} vs {candidate}"
            );
            assert!(
                approx(
                    strsim::jaro_winkler(&gate_key(target), &gate_key(candidate)),
                    1.0
                ),
                "{target} vs {candidate}"
            );
            assert!(approx(basename_similarity(target, candidate), 1.0));
        }
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
