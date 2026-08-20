use serde::Serialize;

/// Keys the frontmatter schema recognizes.
pub const KNOWN_KEYS: &[&str] = &["description", "tags"];

/// Keys that would let a file decide whether or when it gets packed. `rata.toml` owns eagerness;
/// frontmatter owns self-description only, so finding one of these is a hard error.
///
/// Deliberately narrow. Store files carry frontmatter written for other tools — ticket templates,
/// agent skill manifests — and rata does not police conventions it does not own. Common words that
/// merely *sound* like topology (`path`, `context`, `scope`, `store`, `root`) are excluded, because
/// failing `doctor` on a foreign key would make the guardrail the problem.
pub const EAGERNESS_KEYS: &[&str] = &[
    "always",
    "eager",
    "eagerness",
    "include",
    "pack",
    "profile",
    "profiles",
    "rata",
];

/// How far a key can be from a known key and still be treated as a typo of it rather than a
/// foreign convention.
const TYPO_DISTANCE: usize = 2;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Frontmatter {
    pub present: bool,
    pub description: Option<String>,
    pub tags: Vec<String>,
    /// Every top-level key, in the order it appeared.
    pub keys: Vec<String>,
    pub issues: Vec<FrontmatterIssue>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FrontmatterIssue {
    /// A key that would change what gets packed. Frontmatter may never do that.
    EagernessKey { key: String },
    /// A near-miss of a schema key — almost certainly a typo, so the value is being silently lost.
    /// Keys that are nothing like a schema key are ignored: they belong to another tool.
    MisspelledKey { key: String, expected: String },
    /// An opening `---` with no closing delimiter; the whole file was treated as body.
    Unterminated,
    /// A line inside the block that is neither `key: value` nor a list item.
    Malformed { line: String },
}

/// True when any issue is a file trying to decide its own eagerness — the one hard error.
pub fn has_eagerness_key(issues: &[FrontmatterIssue]) -> bool {
    issues
        .iter()
        .any(|issue| matches!(issue, FrontmatterIssue::EagernessKey { .. }))
}

/// Split `contents` into its optional frontmatter block and the body that follows.
///
/// Never fails: a malformed or unterminated block yields issues plus the whole file as body, so
/// reading a file is always possible and `doctor` is the place problems surface.
pub fn parse(contents: &str) -> (Frontmatter, &str) {
    let Some(rest) = strip_open_delimiter(contents) else {
        return (Frontmatter::default(), contents);
    };

    let Some((block, body)) = split_at_close_delimiter(rest) else {
        return (
            Frontmatter {
                present: true,
                issues: vec![FrontmatterIssue::Unterminated],
                ..Frontmatter::default()
            },
            contents,
        );
    };

    (parse_block(block), body)
}

fn strip_open_delimiter(contents: &str) -> Option<&str> {
    let rest = contents.strip_prefix("---")?;
    rest.strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
}

fn split_at_close_delimiter(rest: &str) -> Option<(&str, &str)> {
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if is_close_delimiter(line) {
            return Some((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }
    None
}

fn is_close_delimiter(line: &str) -> bool {
    let trimmed = line.trim_end_matches(['\n', '\r']);
    trimmed == "---" || trimmed == "..."
}

fn parse_block(block: &str) -> Frontmatter {
    let mut frontmatter = Frontmatter {
        present: true,
        ..Frontmatter::default()
    };
    let mut current_key: Option<String> = None;

    for line in block.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }

        if let Some(item) = list_item(line) {
            match current_key.as_deref() {
                Some("tags") => frontmatter.tags.push(item.to_string()),
                Some(_) => {}
                None => frontmatter.issues.push(FrontmatterIssue::Malformed {
                    line: line.trim().to_string(),
                }),
            }
            continue;
        }

        let Some((key, value)) = split_key_value(line) else {
            frontmatter.issues.push(FrontmatterIssue::Malformed {
                line: line.trim().to_string(),
            });
            continue;
        };

        // Nested keys belong to their parent; only top-level keys are part of the schema.
        if line.starts_with(char::is_whitespace) {
            continue;
        }

        frontmatter.keys.push(key.to_string());
        classify_key(key, &mut frontmatter.issues);

        match key {
            "description" if !value.is_empty() => {
                frontmatter.description = Some(unquote(value).to_string());
            }
            // `tags` is reserved, not yet queryable — which is exactly why a shape rata cannot
            // read must be reported. Silently dropping a value nothing else consumes is the
            // worst outcome for a key written today for use later.
            "tags" if !value.is_empty() => match parse_inline_tags(value) {
                Some(tags) => frontmatter.tags.extend(tags),
                None => frontmatter.issues.push(FrontmatterIssue::Malformed {
                    line: line.trim().to_string(),
                }),
            },
            _ => {}
        }

        current_key = Some(key.to_string());
    }

    frontmatter
}

fn classify_key(key: &str, issues: &mut Vec<FrontmatterIssue>) {
    if EAGERNESS_KEYS.contains(&key) {
        issues.push(FrontmatterIssue::EagernessKey {
            key: key.to_string(),
        });
        return;
    }
    if KNOWN_KEYS.contains(&key) {
        return;
    }
    if let Some(expected) = KNOWN_KEYS
        .iter()
        .find(|known| edit_distance(key, known) <= TYPO_DISTANCE)
    {
        issues.push(FrontmatterIssue::MisspelledKey {
            key: key.to_string(),
            expected: (*expected).to_string(),
        });
    }
}

/// Levenshtein distance, capped implicitly by the short keys it runs on.
fn edit_distance(left: &str, right: &str) -> usize {
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];

    for (row, left_char) in left.chars().enumerate() {
        current[0] = row + 1;
        for (column, right_char) in right.iter().enumerate() {
            let substitute = previous[column] + usize::from(left_char != *right_char);
            current[column + 1] = substitute
                .min(previous[column + 1] + 1)
                .min(current[column] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right.len()]
}

fn list_item(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let item = trimmed.strip_prefix("- ").or_else(|| {
        // A lone `-` is an empty item, not a key/value line.
        (trimmed == "-").then_some("")
    })?;
    Some(unquote(item.trim()))
}

fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || key.contains(char::is_whitespace) {
        return None;
    }
    Some((key, value.trim()))
}

/// `[a, b]` only. `None` means the value is not a list rata can read.
fn parse_inline_tags(value: &str) -> Option<Vec<String>> {
    let inner = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))?;
    Some(
        inner
            .split(',')
            .map(|tag| unquote(tag.trim()))
            .filter(|tag| !tag.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|value| value.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::{FrontmatterIssue, parse};

    #[test]
    fn absent_frontmatter_is_never_an_error() {
        let (frontmatter, body) = parse("# Heading\n\nProse.\n");
        assert!(!frontmatter.present);
        assert!(frontmatter.description.is_none());
        assert!(frontmatter.issues.is_empty());
        assert_eq!(body, "# Heading\n\nProse.\n");
    }

    #[test]
    fn description_and_tags_parse_in_both_list_forms() {
        let inline = parse("---\ndescription: A one-liner\ntags: [nix, agents]\n---\n# H\n").0;
        assert_eq!(inline.description.as_deref(), Some("A one-liner"));
        assert_eq!(inline.tags, vec!["nix", "agents"]);
        assert!(inline.issues.is_empty());

        let block = parse("---\ntags:\n  - nix\n  - \"agents\"\n---\nbody\n").0;
        assert_eq!(block.tags, vec!["nix", "agents"]);
        assert!(block.issues.is_empty());
    }

    #[test]
    fn body_starts_after_the_closing_delimiter() {
        let (_, body) = parse("---\ndescription: x\n---\n# Heading\n\nProse.\n");
        assert_eq!(body, "# Heading\n\nProse.\n");
    }

    #[test]
    fn only_eagerness_keys_are_flagged_and_foreign_conventions_are_left_alone() {
        // `author`, `phase`, `argument-hint` belong to other tools; rata does not police them.
        let frontmatter =
            parse("---\nprofile: build\nauthor: ian\nphase: build\nargument-hint: x\n---\nbody\n")
                .0;
        assert!(super::has_eagerness_key(&frontmatter.issues));
        assert!(matches!(
            frontmatter.issues.as_slice(),
            [FrontmatterIssue::EagernessKey { key }] if key == "profile"
        ));
    }

    #[test]
    fn a_near_miss_of_a_schema_key_is_flagged_as_a_typo() {
        let frontmatter = parse("---\ndescriptio: oops\n---\nbody\n").0;
        assert!(matches!(
            frontmatter.issues.as_slice(),
            [FrontmatterIssue::MisspelledKey { key, expected }]
                if key == "descriptio" && expected == "description"
        ));
        // The value really is lost, so flagging it is the point.
        assert!(frontmatter.description.is_none());
    }

    #[test]
    fn a_tags_value_rata_cannot_read_is_reported_rather_than_dropped() {
        let scalar = parse("---\ntags: nix\n---\nbody\n").0;
        assert!(scalar.tags.is_empty());
        assert!(matches!(
            scalar.issues.as_slice(),
            [FrontmatterIssue::Malformed { .. }]
        ));

        // An empty value is an author who has not filled it in yet, not a mistake.
        let empty = parse("---\ntags:\n---\nbody\n").0;
        assert!(empty.issues.is_empty());
    }

    #[test]
    fn unterminated_block_keeps_the_whole_file_as_body() {
        let contents = "---\ndescription: x\n# Heading\n";
        let (frontmatter, body) = parse(contents);
        assert!(matches!(
            frontmatter.issues.as_slice(),
            [FrontmatterIssue::Unterminated]
        ));
        assert_eq!(body, contents);
    }

    #[test]
    fn nested_keys_do_not_join_the_top_level_schema() {
        let frontmatter = parse("---\nmetadata:\n  type: user\n---\nbody\n").0;
        assert_eq!(frontmatter.keys, vec!["metadata"]);
    }
}
