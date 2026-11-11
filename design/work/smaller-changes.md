# Smaller changes

A feature pull request touches 30 to 50 files, and the larger ones lately 50 to 80. Most of that is the layering
doing its job: a feature crosses `domain`, `store`, `service`, `api-types`, `api`, `cli` and `web`, with tests and
docs at each. Part of it is not: a few facts are written down in several places, and some of those places the
compiler doesn't check, so a forgotten one still builds and shows up wrong. This track removes the unchecked
repeats, and the checked ones where removing them is cheap. It changes no behaviour and serves no product.md
section; it serves General §2 of the [engineering principles](../engineering-principles.md#general), one source of
truth.

## Progress

Six pieces, a pull request each. Every one is a refactor with no change in behaviour: `openapi.json`, the stored
codes and every test's expectations stay as they are. Piece 3 builds on piece 1. The others depend on nothing and
can go at any time. Pieces 1 and 3 come first, since they close the places the compiler doesn't check. Piece 6 is
optional and goes last.

- [ ] 1. **One declaration per coded enum in `domain`.** A `coded_enum!` macro beside `id_type!` takes each variant
   with its stable code once and generates the enum, `ALL`, `code` and `from_code`, and nothing else. Every enum
   with a hand-written `ALL` moves to it (17 today), and `AuditAction` and `EntityType` take `code` in place of
   `as_str`.
   Done when: `domain` has no hand-written `ALL` array or code `match`; one test pins every code string of every
   enum, so a stored code can't change unnoticed; the suites pass on SQLite, MySQL and Postgres.
- [ ] 2. **Labels live on the domain enum.** `label()` on `Size`, `Kind`, `Stage`, `StateKind`, `StepState`, `Role`,
   `QuestionKind`, `Reach`, `Goal`, `TurnSource` and `TakeBackWhy`, each a plain `match` as `Need::label()` is. The
   eleven functions of `service::audit_log` that name a value (`describe_state` to `describe_role`) go. The CLI
   and the API's enum types are not touched.
   Done when: no value's name is written in `service`, and the audit API tests pass unchanged.
- [ ] 3. **Audit fields as a type.** An `AuditField` enum in `domain::audit`, declared with `coded_enum!`: the
   field's stored code and its place in the order (the declaration order). `service::audit` writes diffs keyed by
   it, and `service::audit_log` reads them back through `from_code`, with two exhaustive `match`es: a field's
   label, by field and entity type, and how its value reads. `FIELD_ORDER`, `UNSHOWN_FIELDS` and the two `match`es
   on strings go. A stored key no version declares is shown as it is today, and so are the per-parameter changes
   under `parameters`, whose keys are parameter IDs.
   Done when: no audit field name is a string literal outside the declaration; a field added without a label or
   without saying how its value reads fails the build; the audit API tests pass unchanged.
- [ ] 4. **One answer for what the caller may do with an item.** `allowed_item_actions`, `allowed_stage_choices` and
   `allowed_step_actions` are called together at four places in `service::items`. One function in
   `domain::permission` returns the three as one value, and `ItemView` holds that value.
   Done when: a new per-item allowance is added in `domain::permission` and the DTO conversion, and nowhere in
   `service::items`.
- [ ] 5. **Schema types without the alias list.** `gen:api` runs openapi-typescript with `--root-types
   --root-types-no-schema-prefix`, so `schema.d.ts` exports every schema under its own name. The aliases in
   `web/src/api/client.ts` go (64 today), and those that rename a schema (`ItemPatch` for `ItemPatchRequest`) take
   the schema's name. `client.ts` keeps the client, `ApiProblem`, `HeapRef` and `ListViewQuery`.
   Done when: a new response type reaches a component with no edit to `client.ts`, and the type check and the
   lint pass in both build modes.
- [ ] 6. **Queries split by resource. Optional.** `web/src/api/queries.ts` (1,062 lines, 53 hooks) becomes a
   directory with a file per resource and one for the query keys, which the invalidations share. It removes no
   repeated fact: the reason is the file's size, so it's worth doing only while the file keeps growing.
   Done when: no file there is over about 250 lines, hooks are imported from their resource's file, and the
   Playwright suites pass in both build modes.

The pull request that builds the last piece dissolves this doc: the macro is explained in its module doc, What
stays moves to architecture.md if it's still worth saying, the ideas go to the tracker, and the file goes.

## Design

### What was measured

The 51 feature commits that touch 15 or more files, up to #97, touch 1,787 files between them. A file counts once:
as generated, a migration, a doc or a test first, then as new, then as one of the rest.

| Kind of file | All, to #97 | Since #77 only |
| --- | --- | --- |
| Edits to existing feature code | 35% | 33% |
| New files | 16% | 11% |
| Tests | 14% | 18% |
| Docs | 13% | 7% |
| Files nearly every feature edits: permissions, audit, errors, DTOs, `client.ts`, `queries.ts` | 9% | 18% |
| Wiring: a few lines in a `mod.rs`, `lib.rs` or `Cargo.toml` | 6% | 9% |
| Generated: `openapi.json`, `schema.d.ts`, `Cargo.lock` | 4% | 4% |
| Migrations | 1% | 1% |

So about four files in five are the feature, its tests, its docs and what the build generates. The track is about
the rest, and about the places inside the feature's own files where one fact is typed several times.

The rest is growing. The last column holds only seven commits, five of them pieces of Work with agents, so its
shares are rough, but the files every feature edits went from 7% of a commit's files to 18%, and the median
commit from 28 files to 54. Between #83 and #97 the hand-written `ALL` arrays went from 10 to 17, the audit fields
listed as strings from 28 to 45, and the enums of `api-types` from 30 to 38.

### The repeats

1. **An enum value is declared up to ten times.** A new `Stage` takes three places in `domain::item` (the variant,
   `ALL`, `code`), three in `api_types::common` (the variant, `From`, `TryFrom`), and its name as people read it
   in `service::audit_log`, in the CLI's `item show` and in the web app's `labels.ts`, with an icon in
   `kindIcons.tsx`. Every one of them fails the build when it's missed, except `ALL`: a variant left out of it
   compiles, and `from_code` then answers `None` for a code that is stored.
2. **A value's name has four homes:** `domain` (`Need::label`), `service::audit_log` (the `describe_` functions),
   the CLI (`kind_text` and its siblings, in lowercase) and the web app (`labels.ts`). Nothing ties their wording
   together.
3. **Audit fields are strings on both sides.** `service::audit` writes `"kind"`, and `service::audit_log` matches
   on `"kind"` in `FIELD_ORDER`, `describe_field` and `describe_value`. A field missing from any of the three still
   compiles and still shows, in the wrong place or as a raw code. An audit action is exhaustively matched, so
   it's safe, but it is typed eight times in four files.
4. **What the caller may do with an item** is worked out by the same three calls at four places.
5. **Every schema type the web app uses is named by hand** in `client.ts`, so that file is in 26 of the 96 commits,
   and every hook is in `queries.ts`, which is in 31, and in 8 of the last 19.

### What a change costs, before and after

Hand-written files outside tests, docs and generated ones.

| Change | Today | After |
| --- | --- | --- |
| A new value of an item enum | 6 files, 10 places, one unchecked (`ALL`) | 5 files, 8 places, all checked: `domain` (the entry, the label), `api-types` (the variant, `From`, `TryFrom`), the CLI, the web app's label and icon |
| A new audited field, its audit part | 2 files, 4 places, none checked | 3 files, 4 places, all checked: the declaration, the write, the label, how its value reads |
| A new audit action | 4 files, 8 places, one unchecked (`ALL`) | 4 files, 6 places, all checked |
| A new response type the web app uses | an alias in `client.ts`, shared by every feature | nothing |

The gain in files is small, and for an audited field it's one file more. The gain is in what the compiler checks:
after pieces 1 and 3, no place where an enum value or an audit field is written can be forgotten and still build.

### The macro

One `macro_rules!` macro, `coded_enum!`, in `domain` beside `id_type!`.

1. **Why not strum.** Its derives name a code after its variant, so renaming a variant would rename a stored code.
   The codes are stored in rows and audit diffs for good, so each is written out beside its variant, and the test
   of piece 1 pins them. It's also one dependency fewer (General §6).
2. **Why it generates codes only.** It exists for what a `match` can't check: that `ALL` holds every variant. A
   label written as a `match` on the enum already fails the build when a variant is missing, so labels stay plain
   functions, and the macro stays small enough to read in one look.
3. It takes doc comments and attributes on each variant.

### What stays

Looked at and left, because the edit is the check or the rule.

1. **The API's enum types and their conversions.** `api-types` copies a domain enum as its own type on purpose
   (Boundaries §7): the API tolerates values it doesn't know in what it only reads (13 of its 38 enums have an
   `Unknown` variant), leaves some values out, and its list of values is the reviewed contract. A macro that
   declared each copy with its `From` and `TryFrom` was planned and dropped: both conversions are exhaustive
   `match`es, so the build already fails until a new value is added to them, and the macro would have to carry
   the serde and schema attributes, the doc comments that become `openapi.json`'s descriptions, and the `Unknown`
   form. That hides the contract to save lines nobody can get wrong.
2. **`domain::permission`, `api/src/error.rs` and `api/tests/access_control.rs`.** Every feature edits them because
   the build or the table test fails until it does. That's what they are for.
3. **Separate request, core, storage and response types** (Boundaries §7), and so a field's four declarations.
4. **Wiring.** A `mod` line, a migration in `migration/mod.rs`, a route. Rust and the migrator need them.
5. **Generated files and docs.** `openapi.json` and `schema.d.ts` change with the API by design, and a feature's
   docs change with it (Changes §2).
6. **`web/src/pages/AppLayout.tsx`.** It is in 28 commits, each of which changed the shell: the sidebar, the phone
   tabs, the heap switcher. Only 2 of the last 19 touch it.
7. **The CLI's command list in `crates/cli/src/lib.rs`.** A command is named three times in one file, each an
   exhaustive `match`.
8. **The CLI's own wording.** `item show` prints `nice to have` where the web app prints `Nice to have`. Sharing
   the label would change the CLI's output, which is a product change and not a refactor. See Ideas.

### Testing

No piece adds behaviour, so the existing suites are the test, with one guard added: the code-pinning test of
piece 1. Every Rust piece is also checked by `openapi.json` not changing. Pieces 5 and 6 are checked by the type
check, the lint and Playwright in both build modes.

## Open questions

1. **Do the web app's labels come from the server?** `labels.ts` repeats the wording `domain` holds. API,
   Responses §1 says labels are the server's, and in the same sentence lets a client hold headings for a code, so
   the principles don't settle it. `labels.ts` is one file, typed as `Record<Need, string>`, so a missing value
   fails the type check. Moving the labels means a vocabulary endpoint in the manner of `GET /due-choices`: each
   enum's values in order, with label and hint, read once. That removes a file from every new value and adds a
   query every label waits for. Recommended: leave it, and revisit if the two wordings ever drift. It is not a
   piece above.

## Ideas

- **One wording in the CLI.** The API's enum types forward `label()`, and the CLI prints the server's capitalised
  wording in place of its own `kind_text` and siblings. It changes the CLI's output and its tests.
- **A lint for value names.** A test that fails when a label string of `domain` appears in another Rust crate.
- **Query keys from the path.** With queries split, each key could be derived from the API path it reads, so an
  invalidation can't name a key nothing uses.
