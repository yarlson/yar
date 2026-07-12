# Language Tooling Projections

## Authority

YAR owns the language contract. The Rust token, lexer, and parser implementation
in `crates/yar-compiler` is authoritative for lexical and syntactic acceptance.
`docs/YAR.md` is the public reference for the implemented language. When prose
and implementation disagree, code and executable tests settle behavior and the
public reference must be repaired.

`testdata/syntax_surface` is the portable accepted-syntax fixture. For a given
YAR revision, the contract is the recursive set of tracked `*.yar` files under
that directory, including support packages. It contains representative source
accepted by the YAR frontend without depending on editor- or parser-specific
tree shapes. Syntax changes update this fixture when they add, remove, or
materially reshape accepted source.

## Projection ownership

Tree-sitter and JetBrains integrations are external projections of YAR syntax.
Their repositories own:

- grammar and lexer definitions;
- generated parser, lexer, and PSI artifacts;
- syntax trees, highlighting, navigation, formatting, and editor queries;
- invalid-input and recovery behavior;
- projection-specific fixtures, CI, compatibility metadata, and releases.

The YAR repository does not vendor those projections or treat their generated
artifacts as language authority. External tooling must not introduce syntax or
claim parity based only on a hand-maintained feature list.

## Compatibility contract

Each projection declares the YAR revision used for comparison. Its validation
must:

- parse every file in `testdata/syntax_surface` without `ERROR`, `MISSING`, or
  the projection's equivalent parse-failure nodes;
- compare against that declared revision rather than an incidental sibling
  checkout;
- test its own rejected syntax, incomplete-input recovery, tree shape, and query
  or semantic-highlighting contracts;
- verify generated artifacts match their owned grammar sources;
- update its declared revision only after the comparison and projection-specific
  tests pass.

Passing the accepted-syntax fixture proves coverage of that fixture, not full
semantic equivalence. The YAR compiler remains responsible for semantic
validation, and projection repositories remain responsible for editor behavior.

## Change boundary

A YAR syntax change is ready for external projection comparison when the Rust
frontend, public reference, executable tests, and portable accepted-syntax
fixture agree. The repository-wide completion rules in `AGENTS.md` and
`docs/language/process.md` still govern every other affected owned surface.
Projection updates and releases are separate changes in their owning
repositories. A consumer may temporarily lag, but it must identify its compared
YAR revision and must not describe itself as current until its compatibility
contract passes against the newer revision.

## Consumer compatibility observations

The inspected Tree-sitter consumer relies on a particular local sibling layout
for fixture parsing and does not automate revision declaration, generated-file
freshness, negative/recovery coverage, and query assertions together. The
inspected JetBrains consumer maintains an independent grammar and parser fixture
set without declaring or comparing a YAR revision through the portable fixture.
These are consumer compliance gaps; they do not transfer projection ownership
into this repository or weaken the contract above.
