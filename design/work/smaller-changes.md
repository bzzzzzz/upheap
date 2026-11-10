# Smaller changes

A feature pull request touches 30 to 50 files. Most of that is the layering doing its job: a feature crosses
`domain`, `store`, `service`, `api-types`, `api`, `cli` and `web`, with tests and docs at each. Part of it is not:
a few facts are written down in several places, so one new enum value or one new audited field means the same
edit in four or five files, some of them unchecked by the compiler. This track removes those repeats. It changes
no behaviour and serves no product.md section; it serves General §2 of the
[engineering principles](../engineering-principles.md#general), one source of truth.

## Progress

Seven pieces, a pull request each. Every one is a refactor with no change in behaviour: `openapi.json`, the
stored codes and every test's expectations stay as they are, unless the piece says otherwise. Pieces 1 to 4 are in
order, since each builds on the one before. Pieces 5, 6 and 7 depend on nothing and can go at any time.

- [ ] 1. **One declaration per coded enum in `domain`.** A `coded_enum!` macro beside `id_type!` takes each variant
   with its stable code once and generates the enum, `ALL`, `code` and `from_code`. Every enum with a hand-written
   `ALL` moves to it, and `AuditAction` and `EntityType` take `code` in place of `as_str`.
   Done when: `domain` has no hand-written `ALL` array or code `match`; one test pins every code string of every
   enum, so a stored code can't change unnoticed; the suites pass on SQLite, MySQL and Postgres.
- [ ] 2. **Labels live on the domain enum.** `label()` on `Size`, `Kind`, `Stage`, `StateKind`, `StepState` and
   `Role`, beside `Need::label()`, declared in the same `coded_enum!` entry as the code. The `describe_state`,
   `describe_kind`, `describe_stage`, `describe_size` and `describe_role` functions of `service::audit_log` go.
   The API's enum types forward `label()` for a Rust client, and the CLI's `kind_text`, `stage_text` and
   `need_text` go, if open question 1 says so.
   Done when: no value's name is written outside `domain` in the Rust crates, and the audit API tests pass
   unchanged.
- [ ] 3. **API enum types declared once.** A macro in `api-types` declares each enum that copies a domain one, with
   its serde and schema attributes, and generates the `From`. A lenient form adds the `Unknown` variant and the
   `TryFrom` that refuses it with `value_unknown`. The variants stay listed in `api-types`: the API's values are
   the contract, reviewed where they are declared (Boundaries §7).
   Done when: no hand-written `From` between a domain enum and its API copy remains, and `openapi.json` is
   byte for byte the same.
- [ ] 4. **Audit fields as a type.** An `AuditField` enum in `domain::audit`, declared with `coded_enum!`: the
   field's stored code, its label, and its place in the order (the declaration order). `service::audit` writes
   diffs keyed by it, and `service::audit_log` reads them back through `from_code`, with one exhaustive `match` for
   how a field's value reads. `FIELD_ORDER`, `UNSHOWN_FIELDS` and the two `match`es on strings go. A stored key no
   version declares is shown as it is today.
   Done when: no audit field name is a string literal outside the declaration; a field added without saying how
   its value reads fails the build; the audit API tests pass unchanged.
- [ ] 5. **One answer for what the caller may do with an item.** `allowed_item_actions`, `allowed_stage_choices` and
   `allowed_step_actions` are called together at four places in `service::items`. One function in
   `domain::permission` returns the three as one value, and `ItemView` holds that value.
   Done when: a new per-item allowance is added in `domain::permission` and the DTO conversion, and nowhere in
   `service::items`.
- [ ] 6. **Schema types without the alias list.** `gen:api` runs openapi-typescript with `--root-types
   --root-types-no-schema-prefix`, so `schema.d.ts` exports every schema under its own name. The 52 aliases in
   `web/src/api/client.ts` go, and the nine that rename a schema (`ItemPatch` for `ItemPatchRequest`) take the
   schema's name. `client.ts` keeps the client, `ApiProblem`, `HeapRef` and `ListViewQuery`.
   Done when: a new response type reaches a component with no edit to `client.ts`, and the type check and the
   lint pass in both build modes.
- [ ] 7. **Queries split by resource.** `web/src/api/queries.ts` (834 lines, 47 hooks) becomes a directory with a
   file per resource and one for the query keys, which the invalidations share.
   Done when: no file there is over about 250 lines, hooks are imported from their resource's file, and the
   Playwright suites pass in both build modes.

The pull request that builds the last piece dissolves this doc: the two macros are explained in their module docs,
What stays moves to architecture.md if it's still worth saying, the ideas go to the tracker, and the file goes.

## Design

### What was measured

The 43 feature commits that touch 15 or more files, up to #77, touch 1,430 files between them:

| Kind of file | Share |
| --- | --- |
| Edits to existing feature code | 31% |
| New files | 17% |
| Docs | 15% |
| Tests | 13% |
| Files nearly every feature edits: permissions, audit, errors, DTOs, `client.ts`, `queries.ts` | 11% |
| Wiring: a line or two in a `mod.rs`, `lib.rs` or `Cargo.toml` | 8% |
| Generated: `openapi.json`, `schema.d.ts`, `Cargo.lock` | 4% |
| Migrations | 1% |

So about four files in five are the feature, its tests, its docs and what the build generates. The track is about
the rest, and about the places inside the feature's own files where one fact is typed several times.

### The repeats

1. **An enum value is declared up to ten times.** A new `Stage` takes three places in `domain::item` (the variant,
   `ALL`, `code`), three in `api_types::common` (the variant, `From`, `TryFrom`), and its name as people read it
   in `service::audit_log`, in the CLI's `item show` and in the web app's `labels.ts`, with an icon in
   `kindIcons.tsx`. `api-types` holds 30 such enums.
2. **A value's name has four homes:** `domain` (`Need::label`), `service::audit_log` (the five `describe_`
   functions), the CLI (`need_text` and its siblings, in lowercase) and the web app (`labels.ts`). Nothing ties
   their wording together.
3. **Audit fields are strings on both sides.** `service::audit` writes `"kind"`, and `service::audit_log` matches
   on `"kind"` in `FIELD_ORDER`, `describe_field` and `describe_value`. A field missing from any of the three still
   compiles and still shows, in the wrong place or as a raw code. An audit action is exhaustively matched, so
   it's safe, but it is typed eight times in four files.
4. **What the caller may do with an item** is worked out by the same three calls at four places.
5. **Every schema type the web app uses is named by hand** in `client.ts`, so that file is in 20 of the first 77
   commits, and every hook is in `queries.ts`, which is in 23. Neither edit is hard. Both files are where two
   tracks working at once meet, as the tracker was before its lines stopped carrying progress.

### What a change costs, before and after

Hand-written files outside tests, docs and generated ones.

| Change | Today | After |
| --- | --- | --- |
| A new value of an item enum | 6 files, 10 places | 4 files, 4 places: `domain`, `api-types`, the web app's label and icon. 3 files if open question 2 moves the web's labels |
| A new audited field, its audit part | 2 files, 4 places, none checked | 3 files, 3 places, all checked: the declaration, the write, how its value reads |
| A new audit action | 4 files, 8 places | 4 files, 5 places |
| A new endpoint the web app calls | `client.ts` and `queries.ts`, shared by every feature | the resource's own queries file |

The gain in files is small: two or three a pull request that adds an enum value, and none for an audited field,
which trades its unchecked places for one more file. The larger gain is in places: fewer of them, and none the
compiler doesn't check.

### The macros

Two `macro_rules!` macros, each in the crate that uses it, as `id_type!` is in `domain`.

1. **Why not strum.** Its derives name a code after its variant, so renaming a variant would rename a stored code.
   The codes are stored in rows and audit diffs for good, so each is written out beside its variant, and the test
   of piece 1 pins them. It's also one dependency fewer (General §6).
2. **Why the API's enums aren't generated from the domain's.** They are different types on purpose (Boundaries
   §7): the API tolerates values it doesn't know in what it only reads, leaves some values out, and its list of
   values is the reviewed contract. The macro of piece 3 removes the conversions, not the declaration.
3. Both take doc comments and attributes on each variant, since the API's become the descriptions in
   `openapi.json`.

### What stays

Looked at and left, because the edit is the check or the rule.

1. **`domain::permission`, `api/src/error.rs` and `api/tests/access_control.rs`.** Every feature edits them because
   the build or the table test fails until it does. That's what they are for.
2. **Separate request, core, storage and response types** (Boundaries §7), and so a field's four declarations.
3. **Wiring.** A `mod` line, a migration in `migration/mod.rs`, a route. Rust and the migrator need them.
4. **Generated files and docs.** `openapi.json` and `schema.d.ts` change with the API by design, and a feature's
   docs change with it (Changes §2).
5. **`web/src/pages/AppLayout.tsx`.** It is in 26 commits, each of which changed the shell: the sidebar, the phone
   tabs, the heap switcher. Features that add no navigation, such as item kinds and invite links on the server,
   don't touch it.
6. **The CLI's command list in `crates/cli/src/lib.rs`.** A command is named three times in one file, each an
   exhaustive `match`.

### Testing

No piece adds behaviour, so the existing suites are the test, with two guards added: the code-pinning test of
piece 1, and for piece 3 a check in the pull request that `openapi.json` didn't change. Pieces 6 and 7 are checked
by the type check, the lint and Playwright in both build modes.

## Open questions

1. **Does the CLI show the server's wording?** Today `item show` prints `nice to have`, the web app `Nice to
   have`. Sharing the label makes the CLI print the capitalised form, which changes its output and its tests.
   Recommended: yes, one wording everywhere. If not, the CLI keeps its three functions and piece 2 stops at the
   server.
2. **Do the web app's labels come from the server?** `labels.ts` repeats the wording `domain` holds. API,
   Responses §1 says labels are the server's, and in the same sentence lets a client hold headings for a code, so
   the principles don't settle it. `labels.ts` is one file, typed as `Record<Need, string>`, so a missing value
   fails the type check. Moving the labels means a vocabulary endpoint in the manner of `GET /due-choices`: each
   enum's values in order, with label and hint, read once. That removes a file from every new value and adds a
   query every label waits for. Recommended: leave it, and revisit if the two wordings ever drift. It is not a
   piece above.

## Ideas

- **A lint for value names.** A test that fails when a label string of `domain` appears in another Rust crate.
- **Query keys from the path.** With queries split, each key could be derived from the API path it reads, so an
  invalidation can't name a key nothing uses.
