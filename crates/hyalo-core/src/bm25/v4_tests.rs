//! Iteration 304 unit tests: BM25F field weights (DEC-337), phrase slop and
//! the proximity bonus (DEC-338), and incremental postings (DEC-339).

use std::collections::HashSet;

use super::*;

fn en() -> StemLanguage {
    StemLanguage::English
}

fn doc(path: &str, title: &str, body: &str) -> DocumentInput {
    DocumentInput {
        rel_path: path.to_owned(),
        title: title.to_owned(),
        body: body.to_owned(),
        language: en(),
        ..Default::default()
    }
}

fn ranked(index: &Bm25InvertedIndex, query: &CompiledQuery) -> Vec<(String, f64)> {
    index
        .score_compiled(query)
        .into_iter()
        .map(|m| (m.rel_path, m.score))
        .collect()
}

fn paths(index: &Bm25InvertedIndex, query: &str) -> Vec<String> {
    ranked(index, &CompiledQuery::new(query, en()))
        .into_iter()
        .map(|(p, _)| p)
        .collect()
}

#[test]
fn title_hit_outranks_the_same_term_in_the_body() {
    let index = Bm25InvertedIndex::build(vec![
        doc("body.md", "Other words", "kiwi appears once here"),
        doc("title.md", "Kiwi", "other words appear once"),
    ]);
    assert_eq!(paths(&index, "kiwi"), ["title.md", "body.md"]);
}

#[test]
fn heading_and_tag_fields_carry_weight() {
    let mut tagged = doc("tagged.md", "", "plain text mentions mango");
    tagged.tags = vec!["mango".to_owned()];
    let mut headed = doc("headed.md", "", "# Mango\nplain text words");
    headed.heading_lines = vec![0];
    let index = Bm25InvertedIndex::build(vec![
        doc("body.md", "", "plain text mentions mango"),
        tagged,
        headed,
    ]);
    let order = paths(&index, "mango");
    assert_eq!(
        order.last().map(String::as_str),
        Some("body.md"),
        "{order:?}"
    );
}

#[test]
fn tag_only_term_matches_without_positions() {
    let mut tagged = doc("tagged.md", "", "unrelated body");
    tagged.tags = vec!["papaya".to_owned()];
    let index = Bm25InvertedIndex::build(vec![tagged, doc("b.md", "", "other body")]);
    assert_eq!(paths(&index, "papaya"), ["tagged.md"]);
    // A tag has no position, so a phrase never spans into it.
    assert_eq!(paths(&index, "\"papaya unrelated\""), Vec::<String>::new());
    assert!(index.validate_doc_ids());
}

#[test]
fn weights_from_params_change_the_order() {
    let index = Bm25InvertedIndex::build(vec![
        doc("body.md", "x", "lime lime lime words"),
        doc("title.md", "Lime", "other words here too"),
    ]);
    let mut params = SearchSettings::default();
    params.weights.title = 0.1;
    let query = CompiledQuery::new("lime", en()).with_params(params);
    assert_eq!(ranked(&index, &query)[0].0, "body.md");
    params.weights.title = 10.0;
    let query = CompiledQuery::new("lime", en()).with_params(params);
    assert_eq!(ranked(&index, &query)[0].0, "title.md");
}

#[test]
fn phrase_slop_allows_gaps_in_order_only() {
    let index = Bm25InvertedIndex::build(vec![
        doc("gap.md", "", "error in the handling path"),
        doc("reverse.md", "", "handling of an error"),
        doc("far.md", "", "error a b c d e f handling"),
    ]);
    assert_eq!(paths(&index, "\"error handling\""), Vec::<String>::new());
    assert_eq!(paths(&index, "\"error handling\"~3"), ["gap.md"]);
    assert_eq!(paths(&index, "\"error handling\"~2"), ["gap.md"]);
    assert_eq!(paths(&index, "\"error handling\"~1"), Vec::<String>::new());
    let wide = paths(&index, "\"error handling\"~6");
    assert!(wide.contains(&"far.md".to_owned()) && !wide.contains(&"reverse.md".to_owned()));
}

#[test]
fn slop_is_parsed_clamped_and_inert_without_digits() {
    assert!(phrase_matches("a b c", "\"a c\"~1"));
    assert!(!phrase_matches("a b c", "\"a c\""));
    assert!(phrase_matches("a b c", "\"a c\"~999"));
    assert!(!phrase_matches("a b c", "\"a c\"~"));
}

fn phrase_matches(body: &str, query: &str) -> bool {
    let index = Bm25InvertedIndex::build(vec![doc("a.md", "", body)]);
    !paths(&index, query).is_empty()
}

#[test]
fn adjacent_terms_outrank_distant_ones() {
    let filler = "word ".repeat(30);
    let index = Bm25InvertedIndex::build(vec![
        doc("far.md", "", &format!("snapshot {filler} index")),
        doc("near.md", "", &format!("snapshot index {filler}")),
    ]);
    let order = ranked(&index, &CompiledQuery::new("snapshot index", en()));
    assert_eq!(order[0].0, "near.md");
    let params = SearchSettings {
        proximity_bonus: 0.0,
        ..Default::default()
    };
    let flat = ranked(
        &index,
        &CompiledQuery::new("snapshot index", en()).with_params(params),
    );
    assert!((flat[0].1 - flat[1].1).abs() < 1e-12, "{flat:?}");
    // The bonus multiplies by at most 1 + bonus.
    assert!(order[0].1 <= flat[0].1 * 1.5 + 1e-12);
}

#[test]
fn min_window_counts_extra_positions() {
    use super::query::min_window;
    assert_eq!(min_window(&[vec![3], vec![4]]), Some(0));
    assert_eq!(min_window(&[vec![1, 10], vec![12]]), Some(1));
    assert_eq!(min_window(&[vec![1], vec![]]), None);
}

fn sample_docs() -> Vec<PreTokenizedInput> {
    let stemmer = create_stemmer(en());
    let make =
        |path: &str, title: &str, body: &str, heading: &[usize], tags: &[&str]| PreTokenizedInput {
            rel_path: path.to_owned(),
            tokens: tokenize_document_text(
                title,
                body,
                heading,
                tags.iter().copied(),
                &stemmer,
                false,
            ),
        };
    vec![
        make("a.md", "Alpha", "# Intro\nalpha beta gamma", &[0], &["x"]),
        make("b.md", "Beta", "beta beta delta", &[], &["beta"]),
        make("c.md", "", "gamma delta epsilon alpha", &[], &[]),
        make(
            "d.md",
            "Delta",
            "## Delta\ndelta getUserName",
            &[0],
            &["y/z"],
        ),
    ]
}

#[test]
fn incremental_updates_score_exactly_like_a_rebuild() {
    let stemmer = create_stemmer(en());
    let mut incremental = Bm25InvertedIndex::build_from_tokens(sample_docs());
    let replacement = PreTokenizedInput {
        rel_path: "b.md".to_owned(),
        tokens: tokenize_document_text("Beta two", "beta epsilon", &[], ["new"], &stemmer, false),
    };
    let added = PreTokenizedInput {
        rel_path: "e.md".to_owned(),
        tokens: tokenize_document_text("", "alpha user name", &[], [], &stemmer, false),
    };
    let remove: HashSet<&str> = HashSet::from(["c.md"]);
    incremental.apply_updates(&remove, vec![replacement, added]);
    assert!(incremental.validate_doc_ids());

    let mut expected_docs: Vec<PreTokenizedInput> = sample_docs()
        .into_iter()
        .filter(|d| d.rel_path != "b.md" && d.rel_path != "c.md")
        .collect();
    expected_docs.push(PreTokenizedInput {
        rel_path: "b.md".to_owned(),
        tokens: tokenize_document_text("Beta two", "beta epsilon", &[], ["new"], &stemmer, false),
    });
    expected_docs.push(PreTokenizedInput {
        rel_path: "e.md".to_owned(),
        tokens: tokenize_document_text("", "alpha user name", &[], [], &stemmer, false),
    });
    let rebuilt = Bm25InvertedIndex::build_from_tokens(expected_docs);
    for query in [
        "alpha",
        "beta",
        "delta OR epsilon",
        "\"beta epsilon\"",
        "alpha -gamma",
        "user",
        "getUserName",
        "new",
        "del*",
        "alpha beta",
    ] {
        let q = CompiledQuery::new(query, en());
        let a = ranked(&incremental, &q);
        let b = ranked(&rebuilt, &q);
        assert_eq!(a.len(), b.len(), "{query}");
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.0, y.0, "{query}");
            assert_eq!(x.1.to_bits(), y.1.to_bits(), "{query}: {} vs {}", x.1, y.1);
        }
    }
    // The forward index reconstructs exactly, fields included.
    let back = incremental.reconstruct_all_tokens().unwrap();
    let original = sample_docs();
    assert_eq!(back["a.md"], original[0].tokens);
    assert_eq!(back["d.md"], original[3].tokens);
}

#[test]
fn snippet_prefers_the_min_window_line() {
    let matcher = query::SnippetMatcher::from_compiled(&CompiledQuery::new("alpha beta", en()));
    let stemmer = create_stemmer(en());
    let near = tokenize("alpha beta", &stemmer);
    let far = tokenize("alpha x y z beta", &stemmer);
    assert_eq!(matcher.coverage(&near), matcher.coverage(&far));
    assert!(matcher.window(&near) < matcher.window(&far));
}
