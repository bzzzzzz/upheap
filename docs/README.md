# Docs

Who each folder is for, what each doc holds, and when to update it. The rules the docs follow are in
[CLAUDE.md](../CLAUDE.md#docs).

| Folder | For | Start at |
| --- | --- | --- |
| [user/](user/README.md) | Someone using Upheap from the command line, alone or with agents | The guide, which links every page |
| [operator/](operator/README.md) | Someone running an Upheap server, us included | The guide, which links every page |
| `dev/` | Someone changing this repository | The table below |

| Doc | What it holds | Update it when |
| --- | --- | --- |
| [engineering-principles.md](dev/engineering-principles.md) | How we write code: the shared rules from the maintainer's engineering base, and this project's own. Must follow | A rule changes, code and a rule disagree and the rule was the wrong one, or the base changes |
| [product.md](dev/product.md) | What the product does and why: distribution, accounts, email, teams, group-managed teams, invite links, heaps, items and their kinds, questions, working with agents, review, ranking, the list, tags, what's never deleted, roles, the audit log. The core commitments, built or not | A promise to a user changes, not when the code catches up with it |
| [architecture.md](dev/architecture.md) | How it's built and why: principles, stack, data model conventions, handles, ranking, API conventions, authentication, access control, the audit log, deployment, the CLI, testing. Mechanisms and reasons, never lists of what the code holds | A mechanism or a reason changes |
| [decisions.md](dev/decisions.md) | The few choices that are costly to reverse: context, decision, what it replaced, consequences | A choice passes its bar, or one is superseded. Append; don't rewrite |
| [ux.md](dev/ux.md) | The web app's agreed design: principles, core flows, toolkit, layout at each width, each screen's behaviour, the rules that settle questions, and why. Not the copy, sizes, routes or keys the code holds | A principle, a layout or a behaviour changes |
| [../web/e2e/CLAUDE.md](../web/e2e/CLAUDE.md) | The rules the Playwright tests follow so they don't race the app: what to wait for after a save, what not to assert, how to reproduce a flake | A flake teaches a rule, or the tests' setup changes |

`dev/work/` holds the designs of unbuilt work, each deleted once it's built; there is none right now. Plans,
progress and ideas are in Upheap. Everything beside it is permanent.

The web app's mockups live outside the repository, in the "Upheap layouts" canvas. ux.md names a board where it
relies on one.
