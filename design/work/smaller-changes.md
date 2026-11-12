# Smaller changes

A feature pull request touches 30 to 50 files, and the larger ones lately 50 to 80. Most of that is the layering
doing its job: a feature crosses `domain`, `store`, `service`, `api-types`, `api`, `cli` and `web`, with tests and
docs at each. Part of it is not: a few facts are written down in several places, and some of those places the
compiler doesn't check, so a forgotten one still builds and shows up wrong. This track removes the unchecked
repeats, and the checked ones where removing them is cheap. It changes no behaviour and serves no product.md
section; it serves General §2 of the [engineering principles](../engineering-principles.md#general), one source of
truth.

## Pieces

Each refactor is a task in Upheap, tagged `refactor`, with when it's done and what it waits for. None changes
behaviour: `openapi.json`, the stored codes and every test's expectations stay as they are. Once they're built,
this doc goes: the macro is explained in its module doc, and What stays moves to architecture.md if it's still
worth saying.

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
once enums and audit fields are declared with the macro, no place where an enum value or an audit field is written can be forgotten and still build.

### The macro

One `macro_rules!` macro, `coded_enum!`, in `domain` beside `id_type!`.

1. **Why not strum.** Its derives name a code after its variant, so renaming a variant would rename a stored code.
   The codes are stored in rows and audit diffs for good, so each is written out beside its variant, and a test
   pins them. It's also one dependency fewer (General §6).
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
   the label would change the CLI's output, which is a product change and not a refactor.

### Testing

No piece adds behaviour, so the existing suites are the test, with one guard added: the test that pins every
code. Every Rust piece is also checked by `openapi.json` not changing. The web app's pieces are checked by the type
check, the lint and Playwright in both build modes.
