use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use yar_compiler::parser::parse_file;

const DESIGN_STATUSES: &[&str] = &[
    "exploring",
    "proposed",
    "accepted",
    "rejected",
    "deferred",
    "withdrawn",
];
const IMPLEMENTATION_STATUSES: &[&str] = &["not started", "partial", "implemented", "removed"];

#[derive(Clone, Debug, Eq, PartialEq)]
struct Proposal {
    id: String,
    basename: String,
    status: String,
    implementation: String,
}

#[derive(Debug, Eq, PartialEq)]
struct RegistryRow {
    id: String,
    basename: String,
    status: String,
    implementation: String,
    line: usize,
}

#[derive(Debug)]
struct DecisionEntry<'a> {
    line: usize,
    title: &'a str,
    statuses: Vec<(usize, &'a str)>,
}

#[derive(Clone, Copy, Debug)]
struct MarkdownFence {
    marker: u8,
    length: usize,
    line: usize,
}

impl MarkdownFence {
    fn from_line(line: &str, line_number: usize) -> Option<Self> {
        let marker = *line.as_bytes().first()?;
        if marker != b'`' && marker != b'~' {
            return None;
        }
        let length = line.bytes().take_while(|byte| *byte == marker).count();
        (length >= 3).then_some(Self {
            marker,
            length,
            line: line_number,
        })
    }

    fn closes(self, candidate: Self) -> bool {
        self.marker == candidate.marker && candidate.length >= self.length
    }
}

fn update_fence(fence: &mut Option<MarkdownFence>, line: &str, line_number: usize) -> bool {
    let Some(candidate) = MarkdownFence::from_line(line.trim_start(), line_number) else {
        return false;
    };
    match *fence {
        Some(open) if open.closes(candidate) => *fence = None,
        None => *fence = Some(candidate),
        Some(_) => {}
    }
    true
}

fn line_error(file: &str, line: usize, message: impl AsRef<str>) -> String {
    format!("{file}:{line}: {}", message.as_ref())
}

fn proposal_id(basename: &str) -> Result<&str, String> {
    let Some(stem) = basename.strip_suffix(".md") else {
        return Err(format!("{basename}: proposal filename must end in .md"));
    };
    let bytes = stem.as_bytes();
    if bytes.len() < 6 || bytes[4] != b'-' || !bytes[..4].iter().all(u8::is_ascii_digit) {
        return Err(format!(
            "{basename}: proposal filename must match [0-9]{{4}}-*.md"
        ));
    }
    Ok(&stem[..4])
}

fn parse_proposal(basename: &str, source: &str) -> Result<Proposal, String> {
    let id = proposal_id(basename)?.to_owned();
    let file = format!("docs/language/proposals/{basename}");
    let mut headings = Vec::new();
    let mut statuses = Vec::new();
    let mut implementations = Vec::new();
    let mut in_header = true;
    let mut fence = None;
    let mut first_content = None;

    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        if first_content.is_none() && !line.trim().is_empty() {
            first_content = Some((line_number, line));
        }
        if update_fence(&mut fence, line, line_number) {
            continue;
        }
        if fence.is_some() {
            continue;
        }
        if line.starts_with("## ") {
            in_header = false;
        }
        if let Some(title) = line.strip_prefix("# ") {
            headings.push((line_number, title));
        }
        if let Some(status) = line.strip_prefix("Status: ") {
            if !in_header {
                return Err(line_error(
                    &file,
                    line_number,
                    "Status metadata must appear before the first H2",
                ));
            }
            statuses.push((line_number, status));
        }
        if let Some(implementation) = line.strip_prefix("Implementation: ") {
            if !in_header {
                return Err(line_error(
                    &file,
                    line_number,
                    "Implementation metadata must appear before the first H2",
                ));
            }
            implementations.push((line_number, implementation));
        }
    }

    if let Some(open) = fence {
        return Err(line_error(&file, open.line, "unterminated Markdown fence"));
    }

    if headings.len() != 1 {
        return Err(format!(
            "{file}: expected exactly one H1, found {}",
            headings.len()
        ));
    }
    let Some((first_line, first)) = first_content else {
        return Err(format!("{file}: proposal is empty"));
    };
    let Some(first_title) = first.strip_prefix("# Proposal: ") else {
        return Err(line_error(
            &file,
            first_line,
            "first nonblank content must be '# Proposal: <title>'",
        ));
    };
    if first_title.trim().is_empty() {
        return Err(line_error(
            &file,
            first_line,
            "proposal H1 title cannot be empty",
        ));
    }
    let (heading_line, heading) = headings[0];
    let Some(_title) = heading.strip_prefix("Proposal: ") else {
        return Err(line_error(
            &file,
            heading_line,
            "proposal H1 must start with '# Proposal: '",
        ));
    };
    let (status_line, status) = exactly_one_metadata(&file, "Status", &statuses)?;
    if !DESIGN_STATUSES.contains(&status) {
        return Err(line_error(
            &file,
            status_line,
            format!("unknown design status {status:?}"),
        ));
    }
    let (implementation_line, implementation) =
        exactly_one_metadata(&file, "Implementation", &implementations)?;
    if !IMPLEMENTATION_STATUSES.contains(&implementation) {
        return Err(line_error(
            &file,
            implementation_line,
            format!("unknown implementation status {implementation:?}"),
        ));
    }

    Ok(Proposal {
        id,
        basename: basename.to_owned(),
        status: status.to_owned(),
        implementation: implementation.to_owned(),
    })
}

fn exactly_one_metadata<'a>(
    file: &str,
    name: &str,
    values: &[(usize, &'a str)],
) -> Result<(usize, &'a str), String> {
    match values {
        [(line, value)] => Ok((*line, *value)),
        [] => Err(format!(
            "{file}: expected exactly one {name} before the first H2, found none"
        )),
        _ => Err(format!(
            "{file}: expected exactly one {name} before the first H2, found {} at lines {}",
            values.len(),
            values
                .iter()
                .map(|(line, _)| line.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn parse_registry(source: &str) -> Result<Vec<RegistryRow>, String> {
    let file = "docs/language/README.md";
    let mut in_registry = false;
    let mut saw_registry = false;
    let mut table_stage = 0;
    let mut rows = Vec::new();
    let mut fence = None;

    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        if update_fence(&mut fence, line, line_number) {
            continue;
        }
        if fence.is_some() {
            continue;
        }
        if line == "## Proposal registry" {
            if saw_registry {
                return Err(line_error(
                    file,
                    line_number,
                    "duplicate proposal registry section",
                ));
            }
            saw_registry = true;
            in_registry = true;
            continue;
        }
        if in_registry && line.starts_with("## ") {
            if table_stage != 2 {
                return Err(line_error(
                    file,
                    line_number,
                    "incomplete proposal registry table",
                ));
            }
            in_registry = false;
            continue;
        }
        if !in_registry || line.trim().is_empty() {
            continue;
        }
        if table_stage == 0 {
            if line != "| ID | Proposal | Status | Implementation |" {
                return Err(line_error(
                    file,
                    line_number,
                    "invalid proposal registry header",
                ));
            }
            table_stage = 1;
            continue;
        }
        if table_stage == 1 {
            if line != "| --- | --- | --- | --- |" {
                return Err(line_error(
                    file,
                    line_number,
                    "invalid proposal registry separator",
                ));
            }
            table_stage = 2;
            continue;
        }
        if !line.starts_with('|') {
            return Err(line_error(
                file,
                line_number,
                "malformed proposal registry row: expected a pipe-delimited row",
            ));
        }
        let columns = line.split('|').map(str::trim).collect::<Vec<_>>();
        if columns.len() != 6 || !columns[0].is_empty() || !columns[5].is_empty() {
            return Err(line_error(
                file,
                line_number,
                "malformed proposal registry row: expected four columns",
            ));
        }
        let id = columns[1];
        if id.len() != 4 || !id.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(line_error(
                file,
                line_number,
                format!("invalid proposal ID {id:?}"),
            ));
        }
        let (_, link) = parse_markdown_link(file, line_number, columns[2])?;
        let Some(basename) = link.strip_prefix("proposals/") else {
            return Err(line_error(
                file,
                line_number,
                format!("proposal link must be local under proposals/, found {link:?}"),
            ));
        };
        if basename.contains('/') || proposal_id(basename).is_err() {
            return Err(line_error(
                file,
                line_number,
                format!("invalid proposal link {link:?}"),
            ));
        }
        rows.push(RegistryRow {
            id: id.to_owned(),
            basename: basename.to_owned(),
            status: columns[3].to_owned(),
            implementation: columns[4].to_owned(),
            line: line_number,
        });
    }

    if let Some(open) = fence {
        return Err(line_error(file, open.line, "unterminated Markdown fence"));
    }
    if !saw_registry {
        return Err(format!("{file}: missing '## Proposal registry' section"));
    }
    if table_stage != 2 {
        return Err(format!("{file}: incomplete proposal registry table"));
    }
    Ok(rows)
}

fn parse_markdown_link<'a>(
    file: &str,
    line: usize,
    value: &'a str,
) -> Result<(&'a str, &'a str), String> {
    let Some(rest) = value.strip_prefix('[') else {
        return Err(line_error(
            file,
            line,
            "proposal title must be a Markdown link",
        ));
    };
    let Some((title, rest)) = rest.split_once("](") else {
        return Err(line_error(file, line, "malformed proposal Markdown link"));
    };
    let Some(link) = rest.strip_suffix(')') else {
        return Err(line_error(file, line, "malformed proposal Markdown link"));
    };
    if title.is_empty() || link.is_empty() {
        return Err(line_error(
            file,
            line,
            "proposal Markdown link cannot be empty",
        ));
    }
    Ok((title, link))
}

fn validate_registry(proposals: &[Proposal], source: &str) -> Result<(), String> {
    let rows = parse_registry(source)?;
    let mut proposal_ids = BTreeSet::new();
    for proposal in proposals {
        if !proposal_ids.insert(proposal.id.as_str()) {
            return Err(format!(
                "docs/language/proposals: duplicate proposal ID {} ({})",
                proposal.id, proposal.basename
            ));
        }
    }
    let mut ids = BTreeSet::new();
    let mut basenames = BTreeSet::new();
    let mut previous_id: Option<&str> = None;
    for row in &rows {
        if previous_id.is_some_and(|previous| previous >= row.id.as_str()) {
            return Err(line_error(
                "docs/language/README.md",
                row.line,
                format!(
                    "proposal IDs must be strictly ascending; found {} after {previous_id:?}",
                    row.id
                ),
            ));
        }
        previous_id = Some(&row.id);
        if !ids.insert(row.id.as_str()) {
            return Err(line_error(
                "docs/language/README.md",
                row.line,
                format!("duplicate proposal ID {}", row.id),
            ));
        }
        if !basenames.insert(row.basename.as_str()) {
            return Err(line_error(
                "docs/language/README.md",
                row.line,
                format!("duplicate proposal link proposals/{}", row.basename),
            ));
        }
        let linked_id = proposal_id(&row.basename).expect("registry parser validates links");
        if linked_id != row.id {
            return Err(line_error(
                "docs/language/README.md",
                row.line,
                format!(
                    "registry ID {} does not match linked filename prefix {linked_id}",
                    row.id
                ),
            ));
        }
    }

    let proposals_by_id = proposals
        .iter()
        .map(|proposal| (proposal.id.as_str(), proposal))
        .collect::<BTreeMap<_, _>>();
    for row in &rows {
        let Some(proposal) = proposals_by_id.get(row.id.as_str()) else {
            return Err(line_error(
                "docs/language/README.md",
                row.line,
                format!("orphan registry row for proposal {}", row.id),
            ));
        };
        compare_registry_field(row, "link", &proposal.basename, &row.basename)?;
        compare_registry_field(row, "Status", &proposal.status, &row.status)?;
        compare_registry_field(
            row,
            "Implementation",
            &proposal.implementation,
            &row.implementation,
        )?;
    }
    for proposal in proposals {
        if !ids.contains(proposal.id.as_str()) {
            return Err(format!(
                "docs/language/README.md: missing registry row for proposal {} ({})",
                proposal.id, proposal.basename
            ));
        }
    }
    Ok(())
}

fn compare_registry_field(
    row: &RegistryRow,
    field: &str,
    expected: &str,
    actual: &str,
) -> Result<(), String> {
    if expected == actual {
        return Ok(());
    }
    Err(line_error(
        "docs/language/README.md",
        row.line,
        format!(
            "proposal {} {field} mismatch: expected {expected:?}, found {actual:?}",
            row.id
        ),
    ))
}

fn validate_decisions(source: &str) -> Result<(), String> {
    let file = "docs/language/decisions.md";
    let mut state: Option<&str> = None;
    let mut entry: Option<DecisionEntry<'_>> = None;
    let mut titles = BTreeSet::new();
    let mut fence = None;
    let mut categories = BTreeSet::new();

    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        if update_fence(&mut fence, line, line_number) {
            continue;
        }
        if fence.is_some() {
            continue;
        }
        if line.starts_with('#') {
            finish_decision_entry(file, state, entry.take())?;
        }
        if let Some(heading) = line.strip_prefix("## ") {
            state = match heading {
                "Accepted" => Some("accepted"),
                "Rejected" => Some("rejected"),
                "Deferred" => Some("deferred"),
                "Withdrawn" => Some("withdrawn"),
                _ => None,
            };
            if state.is_some() && !categories.insert(heading) {
                return Err(line_error(
                    file,
                    line_number,
                    format!("duplicate decision category {heading:?}"),
                ));
            }
            continue;
        }
        if let Some(title) = line.strip_prefix("### ") {
            if state.is_some() {
                if !titles.insert(title) {
                    return Err(line_error(
                        file,
                        line_number,
                        format!("duplicate decision title {title:?}"),
                    ));
                }
                entry = Some(DecisionEntry {
                    line: line_number,
                    title,
                    statuses: Vec::new(),
                });
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("Status: ") {
            let Some(entry) = entry.as_mut() else {
                return Err(line_error(
                    file,
                    line_number,
                    "Status appears outside a decision entry",
                ));
            };
            if !DESIGN_STATUSES.contains(&value) {
                return Err(line_error(
                    file,
                    line_number,
                    format!("unknown decision status {value:?}"),
                ));
            }
            entry.statuses.push((line_number, value));
        }
    }
    if let Some(open) = fence {
        return Err(line_error(file, open.line, "unterminated Markdown fence"));
    }
    finish_decision_entry(file, state, entry)
}

fn finish_decision_entry(
    file: &str,
    state: Option<&str>,
    entry: Option<DecisionEntry<'_>>,
) -> Result<(), String> {
    let Some(entry) = entry else {
        return Ok(());
    };
    let expected = state.expect("entries are created only inside state sections");
    match entry.statuses.as_slice() {
        [(line, actual)] if *actual == expected => Ok(()),
        [(_, actual)] => Err(line_error(
            file,
            entry.line,
            format!(
                "decision {:?} is under {expected:?} but declares {actual:?}",
                entry.title
            ),
        )),
        [] => Err(line_error(
            file,
            entry.line,
            format!("decision {:?} is missing Status: {expected}", entry.title),
        )),
        _ => Err(line_error(
            file,
            entry.line,
            format!(
                "decision {:?} has duplicate Status metadata at lines {}",
                entry.title,
                entry
                    .statuses
                    .iter()
                    .map(|(line, _)| line.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

fn reject_dead_reference(file: &str, source: &str) -> Result<(), String> {
    for (index, line) in source.lines().enumerate() {
        if line.to_ascii_lowercase().contains("current-state.md") {
            return Err(line_error(
                file,
                index + 1,
                "dead reference to current-state.md",
            ));
        }
    }
    Ok(())
}

fn validate_syntax_surface(
    directory_exists: bool,
    files: &[(String, String)],
    codegen_source: &str,
) -> Result<(), String> {
    const MAIN_FIXTURE: &str = "testdata/syntax_surface/main.yar";

    if !directory_exists {
        return Err("testdata/syntax_surface: directory is missing".to_owned());
    }
    let yar_files = files
        .iter()
        .filter(|(path, _)| path.ends_with(".yar"))
        .collect::<Vec<_>>();
    if yar_files.is_empty() {
        return Err("testdata/syntax_surface: expected at least one .yar file".to_owned());
    }
    if !yar_files.iter().any(|(path, _)| path == MAIN_FIXTURE) {
        return Err(format!(
            "testdata/syntax_surface: missing required {MAIN_FIXTURE}"
        ));
    }
    for (path, source) in yar_files {
        let (_, diagnostics) = parse_file(path, source);
        if !diagnostics.is_empty() {
            return Err(format!(
                "{path}: syntax-surface source does not parse independently: {diagnostics:?}"
            ));
        }
    }

    let marker = "const CODEGEN_FIXTURES: &[&str] = &[";
    let Some((_, after_marker)) = codegen_source.split_once(marker) else {
        return Err("crates/yar-compiler/src/codegen.rs: missing CODEGEN_FIXTURES".to_owned());
    };
    let Some((fixture_body, _)) = after_marker.split_once("\n    ];") else {
        return Err("crates/yar-compiler/src/codegen.rs: malformed CODEGEN_FIXTURES".to_owned());
    };
    let registrations = fixture_body
        .lines()
        .filter_map(|line| line.trim().strip_prefix('"')?.strip_suffix("\","))
        .filter(|fixture| *fixture == MAIN_FIXTURE)
        .count();
    if registrations != 1 {
        return Err(format!(
            "crates/yar-compiler/src/codegen.rs: expected exactly one {MAIN_FIXTURE} entry in CODEGEN_FIXTURES, found {registrations}"
        ));
    }
    Ok(())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("compiler crate is nested under the workspace root")
        .to_path_buf()
}

fn markdown_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|error| format!("read {}: {error}", dir.display()))? {
        let path = entry
            .map_err(|error| format!("read {} entry: {error}", dir.display()))?
            .path();
        if path.is_dir() {
            markdown_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "md") {
            files.push(path);
        }
    }
    Ok(())
}

fn collect_relative_files(root: &Path, dir: &Path, files: &mut Vec<String>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|error| format!("read {}: {error}", dir.display()))? {
        let path = entry
            .map_err(|error| format!("read {} entry: {error}", dir.display()))?
            .path();
        if path.is_dir() {
            collect_relative_files(root, &path, files)?;
        } else {
            files.push(
                path.strip_prefix(root)
                    .map_err(|error| format!("relativize {}: {error}", path.display()))?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

#[test]
fn live_design_records_are_synchronized() {
    let root = repo_root();
    let proposals_dir = root.join("docs/language/proposals");
    let mut proposal_paths = fs::read_dir(&proposals_dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", proposals_dir.display()))
        .map(|entry| entry.expect("read proposal directory entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect::<Vec<_>>();
    proposal_paths.sort();

    let proposals = proposal_paths
        .iter()
        .map(|path| {
            let basename = path.file_name().unwrap().to_string_lossy();
            let source = fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            parse_proposal(&basename, &source).unwrap_or_else(|error| panic!("{error}"))
        })
        .collect::<Vec<_>>();

    let registry = fs::read_to_string(root.join("docs/language/README.md"))
        .expect("read docs/language/README.md");
    validate_registry(&proposals, &registry).unwrap_or_else(|error| panic!("{error}"));

    let decisions = fs::read_to_string(root.join("docs/language/decisions.md"))
        .expect("read docs/language/decisions.md");
    validate_decisions(&decisions).unwrap_or_else(|error| panic!("{error}"));

    let docs_dir = root.join("docs");
    let mut docs = Vec::new();
    markdown_files(&docs_dir, &mut docs).unwrap_or_else(|error| panic!("{error}"));
    docs.extend(
        ["README.md", "AGENTS.md", "LLM.txt"]
            .into_iter()
            .map(|file| root.join(file))
            .filter(|path| path.is_file()),
    );
    docs.sort();
    for path in docs {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let relative = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy();
        reject_dead_reference(&relative, &source).unwrap_or_else(|error| panic!("{error}"));
    }

    let syntax_surface = root.join("testdata/syntax_surface");
    let mut syntax_files = Vec::new();
    if syntax_surface.is_dir() {
        collect_relative_files(&root, &syntax_surface, &mut syntax_files)
            .unwrap_or_else(|error| panic!("{error}"));
    }
    let syntax_sources = syntax_files
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(root.join(&path))
                .unwrap_or_else(|error| panic!("read {path}: {error}"));
            (path, source)
        })
        .collect::<Vec<_>>();
    let codegen = fs::read_to_string(root.join("crates/yar-compiler/src/codegen.rs"))
        .expect("read crates/yar-compiler/src/codegen.rs");
    validate_syntax_surface(syntax_surface.is_dir(), &syntax_sources, &codegen)
        .unwrap_or_else(|error| panic!("{error}"));
}

#[cfg(test)]
mod mutation_tests {
    use super::*;

    const VALID_PROPOSAL: &str =
        "# Proposal: Example\n\nStatus: accepted\nImplementation: implemented\n\n## Summary\n";
    const VALID_REGISTRY: &str = "# Design\n\n## Proposal registry\n\n| ID | Proposal | Status | Implementation |\n| --- | --- | --- | --- |\n| 0001 | [Example](proposals/0001-example.md) | accepted | implemented |\n";

    fn proposal() -> Proposal {
        parse_proposal("0001-example.md", VALID_PROPOSAL).unwrap()
    }

    #[test]
    fn accepts_valid_records() {
        validate_registry(&[proposal()], VALID_REGISTRY).unwrap();
        validate_decisions("# Decisions\n\n## Accepted\n\n### Example\n\nStatus: accepted\n")
            .unwrap();
    }

    #[test]
    fn accepts_registry_after_fenced_fake_sections_and_tables() {
        let registry = format!(
            "# Design\n\n```md\n## Proposal registry\n| ID | Proposal | Status | Implementation |\n| --- | --- | --- | --- |\n| 9999 | [Fake](proposals/9999-fake.md) | rejected | removed |\n```\n\n~~~md\n## Proposal registry\n~~~\n\n{}",
            VALID_REGISTRY
                .strip_prefix("# Design\n\n")
                .expect("valid registry prefix")
        );
        validate_registry(&[proposal()], &registry).unwrap();
    }

    #[test]
    fn ignores_decision_metadata_inside_fences() {
        validate_decisions(
            "# Decisions\n\n## Accepted\n\n### Real\n\nStatus: accepted\n\n````md\n~~~\n```\n## Rejected\n### Fake backtick\nStatus: rejected\n````\n\n~~~md\n## Withdrawn\n### Fake tilde\nStatus: withdrawn\n~~~\n",
        )
        .unwrap();
    }

    #[test]
    fn rejects_missing_duplicate_invalid_and_combined_proposal_metadata() {
        for source in [
            "# Proposal: Example\nImplementation: implemented\n## Summary\n",
            "# Proposal: Example\nStatus: accepted\nStatus: accepted\nImplementation: implemented\n## Summary\n",
            "# Proposal: Example\nStatus: shipped\nImplementation: implemented\n## Summary\n",
            "# Proposal: Example\nStatus: accepted and implemented\nImplementation: implemented\n## Summary\n",
            "# Proposal: Example\nStatus: accepted\nImplementation: complete\n## Summary\n",
        ] {
            assert!(
                parse_proposal("0001-example.md", source).is_err(),
                "{source}"
            );
        }
        assert!(
            parse_proposal(
                "0001-example.md",
                "# Proposal: Example\nStatus: accepted\nImplementation: implemented\n## Summary\nStatus: accepted\n",
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_invalid_proposal_headings_and_filenames() {
        assert!(parse_proposal("1-example.md", VALID_PROPOSAL).is_err());
        assert!(
            parse_proposal(
                "0001-example.md",
                &VALID_PROPOSAL.replace("# Proposal: Example", "# Example")
            )
            .is_err()
        );
        assert!(
            parse_proposal("0001-example.md", &format!("{VALID_PROPOSAL}\n# Extra\n")).is_err()
        );
        assert!(
            parse_proposal(
                "0001-example.md",
                "Status: accepted\nImplementation: implemented\n# Proposal: Example\n## Summary\n",
            )
            .is_err()
        );
        assert!(
            parse_proposal(
                "0001-example.md",
                "# Proposal: \nStatus: accepted\nImplementation: implemented\n## Summary\n",
            )
            .is_err()
        );
        assert!(
            parse_proposal(
                "0001-example.md",
                "```md\nignored\n```\n# Proposal: Example\nStatus: accepted\nImplementation: implemented\n## Summary\n",
            )
            .is_err()
        );
        for fence in ["```", "~~~"] {
            assert!(
                parse_proposal(
                    "0001-example.md",
                    &format!("{VALID_PROPOSAL}\n{fence}\nunterminated\n"),
                )
                .is_err()
            );
        }
    }

    #[test]
    fn rejects_registry_field_mismatches() {
        for row in [
            "| 0001 | [Example](proposals/0001-example.md) | proposed | implemented |",
            "| 0001 | [Example](proposals/0001-example.md) | accepted | partial |",
            "| 0001 | [Example](proposals/0002-example.md) | accepted | implemented |",
            "| 0002 | [Example](proposals/0001-example.md) | accepted | implemented |",
            "| 0001 | [Example](other/0001-example.md) | accepted | implemented |",
        ] {
            let registry = VALID_REGISTRY
                .lines()
                .map(|line| {
                    if line.starts_with("| 0001 ") {
                        row
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                validate_registry(&[proposal()], &registry).is_err(),
                "{row}"
            );
        }
    }

    #[test]
    fn rejects_duplicate_missing_or_orphan_registry_rows() {
        let duplicate = format!(
            "{VALID_REGISTRY}| 0001 | [Example](proposals/0001-example.md) | accepted | implemented |\n"
        );
        assert!(validate_registry(&[proposal()], &duplicate).is_err());

        let missing = VALID_REGISTRY
            .lines()
            .filter(|line| !line.starts_with("| 0001 "))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(validate_registry(&[proposal()], &missing).is_err());

        let orphan = VALID_REGISTRY
            .replace("0001-example", "0002-example")
            .replace("| 0001 |", "| 0002 |");
        assert!(validate_registry(&[proposal()], &orphan).is_err());

        let mut duplicate_id = proposal();
        duplicate_id.basename = "0001-other.md".to_owned();
        assert!(validate_registry(&[proposal(), duplicate_id], VALID_REGISTRY).is_err());
    }

    #[test]
    fn rejects_malformed_registry_rows() {
        let malformed = VALID_REGISTRY.replace(
            "| 0001 | [Example](proposals/0001-example.md) | accepted | implemented |",
            "0001 Example accepted implemented",
        );
        assert!(validate_registry(&[proposal()], &malformed).is_err());
        assert!(
            validate_registry(
                &[proposal()],
                &VALID_REGISTRY.replace(
                    "| ID | Proposal | Status | Implementation |",
                    "| Proposal | ID | Status | Implementation |",
                )
            )
            .is_err()
        );
        assert!(
            validate_registry(
                &[proposal()],
                &VALID_REGISTRY.replace("| --- | --- | --- | --- |", "| -- | -- | -- | -- |"),
            )
            .is_err()
        );
        for fence in ["```", "~~~"] {
            assert!(parse_registry(&format!("{VALID_REGISTRY}\n{fence}\n")).is_err());
        }
    }

    #[test]
    fn rejects_later_duplicate_registry_section() {
        let duplicate = format!(
            "{VALID_REGISTRY}\n## Other\n\nUnrelated prose.\n\n## Proposal registry\n\n| ID | Proposal | Status | Implementation |\n| --- | --- | --- | --- |\n"
        );
        assert!(parse_registry(&duplicate).is_err());
    }

    #[test]
    fn rejects_registry_rows_out_of_order() {
        let second = parse_proposal(
            "0002-second.md",
            "# Proposal: Second\nStatus: proposed\nImplementation: not started\n## Summary\n",
        )
        .unwrap();
        let registry = "# Design\n\n## Proposal registry\n\n| ID | Proposal | Status | Implementation |\n| --- | --- | --- | --- |\n| 0002 | [Second](proposals/0002-second.md) | proposed | not started |\n| 0001 | [Example](proposals/0001-example.md) | accepted | implemented |\n";
        assert!(validate_registry(&[proposal(), second], registry).is_err());
    }

    #[test]
    fn rejects_invalid_decision_metadata() {
        for source in [
            "# Decisions\nStatus: accepted\n",
            "# Decisions\n## Accepted\n### Entry\nStatus: rejected\n",
            "# Decisions\n## Accepted\n### Entry\nStatus: shipped\n",
            "# Decisions\n## Accepted\n### Entry\n",
            "# Decisions\n## Accepted\n### Entry\nStatus: accepted\nStatus: accepted\n",
            "# Decisions\n## Accepted\n### Entry\nStatus: accepted\n### Entry\nStatus: accepted\n",
            "# Decisions\n## Accepted\n### First\nStatus: accepted\n## Accepted\n### Second\nStatus: accepted\n",
        ] {
            assert!(validate_decisions(source).is_err(), "{source}");
        }
        for fence in ["```", "~~~"] {
            assert!(
                validate_decisions(&format!(
                    "# Decisions\n## Accepted\n### Entry\nStatus: accepted\n{fence}\n"
                ))
                .is_err()
            );
        }
    }

    #[test]
    fn rejects_dead_current_state_reference() {
        assert!(reject_dead_reference("doc.md", "see `current-state.md`").is_err());
        assert!(reject_dead_reference("doc.md", "see CURRENT-STATE.MD").is_err());
        reject_dead_reference("doc.md", "current state documentation").unwrap();
    }

    #[test]
    fn validates_syntax_surface_contract() {
        let files = vec![
            (
                "testdata/syntax_surface/main.yar".to_owned(),
                "package main\nfn main() i32 { return 0 }\n".to_owned(),
            ),
            (
                "testdata/syntax_surface/support/support.yar".to_owned(),
                "package support\nfn value() i32 { return 1 }\n".to_owned(),
            ),
        ];
        let codegen = "const CODEGEN_FIXTURES: &[&str] = &[\n        \"testdata/syntax_surface/main.yar\",\n    ];\n";
        validate_syntax_surface(true, &files, codegen).unwrap();
        assert!(validate_syntax_surface(false, &files, codegen).is_err());
        assert!(validate_syntax_surface(true, &[], codegen).is_err());
        assert!(validate_syntax_surface(true, &[files[1].clone()], codegen,).is_err());
        assert!(
            validate_syntax_surface(
                true,
                &files,
                "const CODEGEN_FIXTURES: &[&str] = &[\n        \"testdata/hello/main.yar\",\n    ];\n",
            )
            .is_err()
        );
        let mut invalid = files.clone();
        invalid[1].1 = "package support\nfn broken(\n".to_owned();
        let error = validate_syntax_surface(true, &invalid, codegen).unwrap_err();
        assert!(error.contains("support/support.yar"), "{error}");
        assert!(error.contains("does not parse independently"), "{error}");
    }
}
