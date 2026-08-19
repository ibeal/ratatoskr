use serde::Serialize;

/// Keys the frontmatter schema recognizes. Everything else is reported by `rata doctor`.
pub const KNOWN_KEYS: &[&str] = &["description", "tags"];

/// Keys that would let a file decide whether or when it gets packed. `rata.toml` owns eagerness;
/// frontmatter owns self-description only, so finding one of these is a hard error.
pub const EAGERNESS_KEYS: &[&str] = &[
    "always", "context", "eager", "include", "pack", "path", "profile", "profiles", "root",
    "scope", "store",
];

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
    /// Parsed and carried, but not part of the schema.
    UnknownKey { key: String },
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
            "tags" => frontmatter.tags.extend(parse_inline_tags(value)),
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
    } else if !KNOWN_KEYS.contains(&key) {
        issues.push(FrontmatterIssue::UnknownKey {
            key: key.to_string(),
        });
    }
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

fn parse_inline_tags(value: &str) -> Vec<String> {
    let Some(inner) = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
    else {
        return Vec::new();
    };
    inner
        .split(',')
        .map(|tag| unquote(tag.trim()))
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
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
    fn eagerness_keys_are_reported_separately_from_unknown_keys() {
        let frontmatter = parse("---\nprofile: build\nauthor: ian\n---\nbody\n").0;
        assert!(super::has_eagerness_key(&frontmatter.issues));
        assert!(matches!(
            frontmatter.issues.as_slice(),
            [
                FrontmatterIssue::EagernessKey { key: eager },
                FrontmatterIssue::UnknownKey { key: unknown },
            ] if eager == "profile" && unknown == "author"
        ));
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
