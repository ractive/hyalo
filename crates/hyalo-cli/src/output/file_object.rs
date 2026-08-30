//! The dynamic filter built for a `FileObject`, whose shape depends on the requested `--fields`.
//!
//! Split out of the single 3,744-line `output.rs` in iteration 247
//! (deep-review hotspot). A file split only: every item keeps the visibility it
//! had in the one module, so `output::...` paths and behaviour are unchanged.

// ---------------------------------------------------------------------------
// FileObject dynamic filter builder
// ---------------------------------------------------------------------------

/// Build a jaq filter string for a `FileObject` by inspecting which optional
/// fields are present in the JSON object.
///
/// The file header is always emitted. Each optional section (properties, tags,
/// sections, tasks, matches, links) is included only when the key is present.
///
/// **How it works:** Each part is a jaq expression that either emits a string or
/// `empty` (when the field is absent/empty). Parts are joined with `, ` — jaq's
/// alternation operator — so the filter produces one output per present section.
/// `run_jq_filter` then joins those outputs with `"\n"`, producing the final
/// multi-line text block. This coupling is intentional: changing the separator
/// in `run_jq_filter` would affect `FileObject` rendering.
pub(super) fn build_file_object_filter(map: &serde_json::Map<String, serde_json::Value>) -> String {
    // Header: file path, modified timestamp — always present — plus the
    // iter-252 size/lines pair when the payload carries it (a find result
    // always does; other FileObject-shaped payloads and pre-252 JSON may not).
    let mut parts = if map.contains_key("size") && map.contains_key("lines") {
        vec![r#""\"\(.file)\"  (\(.modified), \(.size) B, \(.lines) lines)""#.to_owned()]
    } else {
        vec![r#""\"\(.file)\"  (\(.modified))""#.to_owned()]
    };

    // Title: "  title: <value>" or "  title: (none)"
    if map.contains_key("title") {
        parts.push(r#""  title: \(if .title != null then .title else "(none)" end)""#.to_owned());
    }

    // Properties: header then each as "    key: value"
    if map.contains_key("properties") {
        parts.push(
            r#"if (.properties | length) > 0 then "  properties:\n\(.properties | to_entries | map("    \(.key): \(if (.value | type) == "array" then "[" + (.value | map(tostring) | join(", ")) + "]" else .value end)") | join("\n"))" else empty end"#.to_owned(),
        );
    }

    // Properties (typed): header then each as "    name (type): value"
    if map.contains_key("properties_typed") {
        parts.push(
            r#"if (.properties_typed | length) > 0 then "  properties_typed:\n\(.properties_typed | map("    \(.name) (\(.type)): \(if (.value | type) == "array" then "[" + (.value | map(tostring) | join(", ")) + "]" else .value end)") | join("\n"))" else empty end"#.to_owned(),
        );
    }

    // Tags: "  tags: [tag1, tag2, ...]"
    if map.contains_key("tags") {
        parts.push(
            r#"if (.tags | length) > 0 then "  tags: [\(.tags | join(", "))]" else empty end"#
                .to_owned(),
        );
    }

    // Sections: header then each as "    ## Heading [done/total]" or "    ## Heading"
    // Note: uses r##"..."## because the jq filter contains the sequence "#" (hash-quoted).
    // NEW-16 (dogfood pre3): when a computed count is about to be appended,
    // strip a trailing hand-written `[n/m]` from `.heading` first — see
    // OUTLINE_SECTION_WITH_TASKS_FILTER's doc comment for why. Only stripped
    // when `.tasks` is present: a heading with no task section keeps its text
    // exactly as written, even if it happens to end in bracket text that
    // merely looks like a count.
    if map.contains_key("sections") {
        parts.push(
            r##"if (.sections | length) > 0 then "  sections:\n\(.sections | map("    \("#" * .level) \(if .tasks then ((.heading // "(pre-heading)") | sub("\\s*\\[[0-9]+/[0-9]+\\]\\s*$"; "")) else (.heading // "(pre-heading)") end)\(if .tasks then " [\(.tasks.done)/\(.tasks.total)]" else "" end)") | join("\n"))" else empty end"##.to_owned(),
        );
    }

    // Tasks: header then each as "    [x] text (line N)"
    if map.contains_key("tasks") {
        parts.push(
            r#"if (.tasks | length) > 0 then "  tasks:\n\(.tasks | map("    [\(if .done then "x" else " " end)] \(.text) (line \(.line))") | join("\n"))" else empty end"#.to_owned(),
        );
    }

    // Matches: header then each as "    line N (section): text"
    if map.contains_key("matches") {
        parts.push(
            r#"if (.matches | length) > 0 then "  matches:\n\(.matches | map("    line \(.line) (\(.section)): \(.text)") | join("\n"))" else empty end"#.to_owned(),
        );
    }

    // Score: "  score: <value>" — BM25 relevance score when pattern search was used
    if map.contains_key("score") {
        parts.push(r#""  score: \(.score)""#.to_owned());
    }

    // Links: header then each as "    line N: \"target\" → \"path\"" or
    // "    line N: \"target\" (unresolved)".
    // Anchored links (L-21) append "#fragment" to the target and, when the
    // heading is missing, " (broken anchor)" after the path — keep in sync
    // with LINK_INFO_ANCHORED_FILTER, which renders the standalone shape.
    // iter-215 (dogfood UX-6): the `line N:` prefix is what makes a
    // `find --broken-links` report actionable — it names the line the broken
    // link is written on, the same one `lint`/`backlinks` report. `// 0`
    // guards JSON from a pre-215 hyalo that has no `.line`.
    if map.contains_key("links") {
        parts.push(
            r##"if (.links | length) > 0 then "  links:\n\(.links | map("    line \(.line // 0): \"\(.target)\(if .fragment then "#\(.fragment)" else "" end)\"\(if .path then " → \"\(.path)\"\(if .broken_anchor then " (broken anchor)" else "" end)" else (if .out_of_vault then " (out of vault)" else " (unresolved)" end) end)") | join("\n"))" else empty end"##.to_owned(),
        );
    }

    // Backlinks: header then each as "    \"source\" line N" or "    \"source\" line N: label"
    if map.contains_key("backlinks") {
        parts.push(
            r#"if (.backlinks | length) > 0 then "  backlinks:\n\(.backlinks | map("    \"\(.source)\" line \(.line)\(if .label then ": \(.label)" else "" end)") | join("\n"))" else empty end"#.to_owned(),
        );
    }

    parts.join(", ")
}
