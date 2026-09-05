//! The dynamic filter built for a `FileObject`, whose shape depends on the requested `--fields`.
//!
//! Split out of the single 3,744-line `output.rs` in iteration 247
//! (deep-review hotspot). A file split only: every item keeps the visibility it
//! had in the one module, so `output::...` paths and behaviour are unchanged.

// ---------------------------------------------------------------------------
// FileObject dynamic filter builder
// ---------------------------------------------------------------------------

thread_local! {
    /// Whether the run asked for broken links specifically (`--broken-links`).
    ///
    /// UX-6 (iter-277): text mode used to print every link of every matched
    /// file, so an MDN page with ninety working links and one dead target
    /// rendered ninety-one lines and buried the reason it was listed. The
    /// JSON is deliberately untouched — it already carries `path: null` and
    /// `broken_anchor` per link, and a consumer filters on those — so the
    /// selection lives in the text renderer alone.
    ///
    /// A thread-local rather than a parameter: the renderer is a recursive
    /// `serde_json::Value` walk shared by every command, and the flag belongs
    /// to one command's output pass. One `hyalo` process renders one command's
    /// results, on one thread, so the value cannot outlive its run.
    static BROKEN_LINKS_ONLY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Restrict the rendered `links:` section to the genuinely broken links.
///
/// Called by `find` when `--broken-links` filtered the result set.
pub(crate) fn set_broken_links_only(on: bool) {
    BROKEN_LINKS_ONLY.with(|c| c.set(on));
}

/// Whether the `links:` section should be narrowed to broken links.
fn broken_links_only() -> bool {
    BROKEN_LINKS_ONLY.with(std::cell::Cell::get)
}

/// The jq expression selecting only the links `find --broken-links` calls
/// broken — the same rule the documented `--jq` recipe uses: an external URI
/// and a resolved attachment are never broken, an out-of-vault target is
/// reported as such rather than as broken, and a dead `#fragment` counts even
/// though its target resolved.
const BROKEN_LINK_SELECT: &str = r##"map(select((.kind != "external" and .kind != "attachment") and ((.path == null and ((.out_of_vault // false) | not)) or (.broken_anchor // false)))) | "##;

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
    // Header: the file path — the one unconditional key — followed by
    // whichever of `modified`, `size`, `lines` the payload carries, in that
    // order, as a parenthesised group. Since iteration 254 each is droppable
    // via an exact `--fields` projection, so the group is assembled from what
    // is actually present rather than from two fixed shapes; when none of the
    // three survive, the header is the quoted path alone.
    let mut header_bits: Vec<&str> = Vec::with_capacity(3);
    if map.contains_key("modified") {
        header_bits.push(r"\(.modified)");
    }
    if map.contains_key("size") {
        header_bits.push(r"\(.size) B");
    }
    if map.contains_key("lines") {
        header_bits.push(r"\(.lines) lines");
    }
    let header = if header_bits.is_empty() {
        r#""\"\(.file)\"""#.to_owned()
    } else {
        format!(r#""\"\(.file)\"  ({})""#, header_bits.join(", "))
    };
    let mut parts = vec![header];

    // Title: "  title: <value>" or "  title: (none)"
    if map.contains_key("title") {
        parts.push(r#""  title: \(if .title != null then .title else "(none)" end)""#.to_owned());
    }

    // Properties: header then each as "    key: value"
    if map.contains_key("properties") {
        parts.push(
            format!(
                r#"if (.properties | length) > 0 then "  properties:\n\(.properties | to_entries | map("    \(.key): \(.value | {expr})") | join("\n"))" else empty end"#,
                expr = super::filters::PROPERTY_VALUE_EXPR
            ),
        );
    }

    // Properties (typed): header then each as "    name (type): value"
    if map.contains_key("properties_typed") {
        parts.push(
            format!(
                r#"if (.properties_typed | length) > 0 then "  properties_typed:\n\(.properties_typed | map("    \(.name) (\(.type)): \(.value | {expr})") | join("\n"))" else empty end"#,
                expr = super::filters::PROPERTY_VALUE_EXPR
            ),
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
    // iter-261: the same shape the standalone `LINK_INFO_FILTER` renders —
    // the link `kind` after the arrow when it is not a plain `wikilink`, no
    // verdict at all for an `external` URI, and the DEC-268 heading
    // suggestion when a dead fragment has exactly one prefix match.
    // Anchored links (L-21) append "#fragment" to the target and, when the
    // heading is missing, " (broken anchor)" after the path — keep in sync
    // with LINK_INFO_ANCHORED_FILTER, which renders the standalone shape.
    // iter-215 (dogfood UX-6): the `line N:` prefix is what makes a
    // `find --broken-links` report actionable — it names the line the broken
    // link is written on, the same one `lint`/`backlinks` report. `// 0`
    // guards JSON from a pre-215 hyalo that has no `.line`.
    if map.contains_key("links") {
        let select = if broken_links_only() {
            BROKEN_LINK_SELECT
        } else {
            ""
        };
        parts.push(
            r##"if (.links | length) > 0 then "  links:\n\(.links | SELECT_PLACEHOLDERmap("    line \(.line // 0): \"\(.target)\(if .fragment then "#\(.fragment)" else "" end)\"\(if .path then " → \"\(.path)\"" else "" end)\(if .kind and .kind != "wikilink" then " (\(.kind))" else "" end)\(if .path then (if .broken_anchor then " (broken anchor)" else "" end) elif .out_of_vault then " (out of vault)" elif .kind == "external" then "" else " (unresolved)" end)\(if .via then " (via \(.via))" else "" end)\(if .suggested_fragment then " — did you mean \"#\(.suggested_fragment)\"?" else "" end)") | join("\n"))" else empty end"##.replace("SELECT_PLACEHOLDER", select),
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
