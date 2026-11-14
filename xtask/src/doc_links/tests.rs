//! The link check on constructed pages: which links are found, which anchors a heading gets, and
//! which links are reported. Every expected value is written out by hand.

use super::*;

fn pages(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(path, text)| ((*path).to_owned(), (*text).to_owned())).collect()
}

/// Oracle: the links of the page, read by eye. Code blocks and code spans hide theirs; a title
/// and angle brackets are not part of the destination; parentheses inside one are kept.
#[test]
fn links_are_found_outside_code_with_their_lines() {
    let text = "\
See [a](a.md) and ![b](img/b.png \"title\").
`[not](code.md)` and [c](<c d.md>)
```text
[not](fenced.md)
```
[e](e_(1).md#x) then [f](#f)
[ref]: docs/ref.md
";
    let want = [
        (1, "a.md"),
        (1, "img/b.png"),
        (2, "c d.md"),
        (6, "e_(1).md#x"),
        (6, "#f"),
        (7, "docs/ref.md"),
    ];
    let want: Vec<(usize, String)> = want.iter().map(|(n, d)| (*n, (*d).to_owned())).collect();
    assert_eq!(links(text), want);
}

/// Oracle: GitHub's rule, applied by hand. Punctuation and backticks go, spaces become `-`,
/// hyphens and underscores stay, a repeated heading is numbered, a fenced `#` is no heading.
#[test]
fn headings_get_githubs_anchors() {
    let text = "\
# The `lumen` command
## Smooth shading: `lumen-smooth-diffuse-path/1`
### Clarification, 2026-11-14: what it says
## Examples
## Examples ##
```sh
# not a heading
```
#not-a-heading
";
    let want = [
        "the-lumen-command",
        "smooth-shading-lumen-smooth-diffuse-path1",
        "clarification-2026-11-14-what-it-says",
        "examples",
        "examples-1",
    ];
    assert_eq!(anchors(text), want);
}

/// Oracle: each link's target, checked by hand against the two pages and the one other file.
/// Witness count: four good links pass and four broken ones are each reported once.
#[test]
fn each_broken_link_is_reported_with_its_reason() {
    let docs = pages(&[
        (
            "README.md",
            "[ok](docs/guide.md#run-it)\n[ok](LICENSE)\n[ok](#top)\n[web](https://example.com/x)\n\
             [gone](docs/missing.md)\n[bad](docs/guide.md#nowhere)\n[up](../outside.md)\n\
             # Top\n",
        ),
        ("docs/guide.md", "# Run it\n[ok](../README.md#top)\n[bad](#run)\n"),
    ]);
    let files = ["README.md", "LICENSE", "docs/guide.md"];
    let broken = check(&docs, |path| files.contains(&path));
    let found: Vec<(&str, usize, &str, &str)> =
        broken.iter().map(|b| (b.page.as_str(), b.line, b.link.as_str(), b.why)).collect();
    assert_eq!(
        found,
        [
            ("README.md", 5, "docs/missing.md", "no such file or directory"),
            ("README.md", 6, "docs/guide.md#nowhere", "no such heading"),
            ("README.md", 7, "../outside.md", "leaves the repository"),
            ("docs/guide.md", 3, "#run", "no such heading"),
        ]
    );
}

/// Oracle: path arithmetic by hand. A link is taken from its page's directory; `..` climbs, a
/// trailing `/` names the directory itself.
#[test]
fn a_path_is_resolved_from_its_pages_directory() {
    assert_eq!(resolve("docs/adr/0001.md", "0003.md").as_deref(), Some("docs/adr/0003.md"));
    assert_eq!(resolve("docs/reference/cli.md", "../../README.md").as_deref(), Some("README.md"));
    assert_eq!(resolve("README.md", "docs/measurements/").as_deref(), Some("docs/measurements"));
    assert_eq!(resolve("README.md", "./docs/./x.md").as_deref(), Some("docs/x.md"));
    assert_eq!(resolve("docs/x.md", "../../y.md"), None);
}
