//! Markdown link check for `make ci`.
//!
//! Walks the repository for `.md` files and verifies that every inline relative link
//! (`[text](path)` and image `![alt](path)`) resolves to a file or directory on disk. This catches
//! the recurring class of doc defect where a plugin spec links `adr/…` instead of `../adr/…`, or a
//! moved/retired plan leaves dangling references behind. It is path-existence only — `#fragment`
//! anchors are not validated, and external links (`http(s)://`, `mailto:`, any `scheme://`) and pure
//! `#anchor` links are skipped, as are links inside fenced or inline code.

use std::path::Path;
use std::{fs, io};

/// A relative Markdown link whose target does not exist on disk.
struct BrokenLink {
    file: String,
    line: usize,
    target: String,
}

/// Check every Markdown file under `root`. Returns an error listing all broken relative links.
pub fn check_doc_links(root: &Path) -> Result<(), String> {
    let mut broken = Vec::new();
    collect(root, root, &mut broken)
        .map_err(|error| format!("failed to check doc links: {error}"))?;
    if broken.is_empty() {
        return Ok(());
    }
    broken.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    let mut message = String::from("Doc link check failed — broken relative links:\n");
    for link in &broken {
        message.push_str(&format!(
            "  {}:{}  ->  {}\n",
            link.file, link.line, link.target
        ));
    }
    Err(message)
}

fn collect(dir: &Path, root: &Path, broken: &mut Vec<BrokenLink>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // Skip build output and hidden dirs (`target*`, `.git`, `.claude`, …).
            if name.starts_with('.')
                || matches!(name.as_ref(), "target" | "target-build" | "target-release")
            {
                continue;
            }
            collect(&path, root, broken)?;
        } else if path.extension().is_some_and(|extension| extension == "md") {
            check_file(&path, root, broken)?;
        }
    }
    Ok(())
}

fn check_file(path: &Path, root: &Path, broken: &mut Vec<BrokenLink>) -> io::Result<()> {
    let content = fs::read_to_string(path)?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    for (line, target) in extract_link_targets(&content) {
        if !dir.join(&target).exists() {
            broken.push(BrokenLink {
                file: relative_slash_path(path, root),
                line,
                target,
            });
        }
    }
    Ok(())
}

/// The resolvable relative-link targets in `content`, paired with 1-based line numbers. Fenced code
/// blocks and inline code spans are stripped; external and pure-anchor links are dropped; any
/// `#fragment` is removed from the returned path.
fn extract_link_targets(content: &str) -> Vec<(usize, String)> {
    let mut targets = Vec::new();
    let mut in_fence = false;
    for (index, raw) in content.lines().enumerate() {
        let trimmed = raw.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let line = strip_inline_code(raw);
        for inner in inline_link_targets(&line) {
            if let Some(path) = resolvable_target(&inner) {
                targets.push((index + 1, path));
            }
        }
    }
    targets
}

/// Drop text inside single-backtick inline code spans so link-like example text in code is ignored.
fn strip_inline_code(line: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for ch in line.chars() {
        if ch == '`' {
            in_code = !in_code;
        } else if !in_code {
            out.push(ch);
        }
    }
    out
}

/// The raw contents of every `](…)` on a line (covers both `[text](…)` links and `![alt](…)` images).
fn inline_link_targets(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b'(' {
            let rest = &line[i + 2..];
            if let Some(close) = rest.find(')') {
                out.push(rest[..close].to_string());
                i += 2 + close + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Reduce a `](…)` inner string to a checkable relative path, or `None` if it should be skipped
/// (external scheme, pure `#anchor`, or empty). Strips an optional `"title"`, `<>` wrapping, and a
/// trailing `#fragment`.
fn resolvable_target(inner: &str) -> Option<String> {
    let path = inner.split_whitespace().next().unwrap_or("");
    let path = path.trim_start_matches('<').trim_end_matches('>');
    if path.is_empty() || path.starts_with('#') {
        return None;
    }
    if path.starts_with("http://")
        || path.starts_with("https://")
        || path.contains("://")
        || path.starts_with("mailto:")
    {
        return None;
    }
    let path = path.split('#').next().unwrap_or("");
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

fn relative_slash_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_relative_links_and_skips_external_anchors_and_code() {
        let md = "\
intro [spec](../adr/0001.md) and ![img](plots/x.svg#frag) here.
[site](https://example.com) and [anchor](#section) and [mail](mailto:a@b.c).
`[incode](should-skip.md)` stays out.
```
[fenced](also-skip.md)
```
trailing [titled](path.md \"a title\").";
        let got = extract_link_targets(md);
        let paths: Vec<&str> = got.iter().map(|(_, p)| p.as_str()).collect();
        assert_eq!(paths, vec!["../adr/0001.md", "plots/x.svg", "path.md"]);
        // line numbers are 1-based and point at the source line.
        assert_eq!(got[0].0, 1);
    }

    #[test]
    fn this_repo_has_no_broken_doc_links() {
        // The check must pass on the committed tree (it runs in `make ci`). Resolve the repo root
        // from the manifest dir so the test is independent of the cwd.
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask has a parent (the repo root)")
            .to_path_buf();
        if let Err(report) = check_doc_links(&repo_root) {
            panic!("{report}");
        }
    }
}
