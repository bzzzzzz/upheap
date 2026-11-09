# Design docs

What each doc is for, and when to update it. The rules the docs follow are in [CLAUDE.md](../CLAUDE.md#docs).

| Doc | What it holds | Update it when |
| --- | --- | --- |
| [engineering-principles.md](engineering-principles.md) | How we write code. Must follow | A rule changes, or code and a rule disagree and the rule was the wrong one |
| [requirements.md](requirements.md), [offering.md](offering.md) | What the product does and why: heaps, items, ranking, views; distribution, teams, roles, the audit log | The product changes, not when the code catches up with it |
| [architecture.md](architecture.md) | How it's built and why: principles, stack, data model conventions, ranking, API conventions, authentication, access control, the audit log, deployment, testing. Mechanisms and reasons, never lists of what the code holds | A mechanism or a reason changes |
| [decisions.md](decisions.md) | Choices that could have gone another way: context, decision, what it replaced, consequences | A choice is made or superseded. Append; don't rewrite |
| [progress.md](progress.md) | Status by area, what's next, open questions | Any change that moves an area's status. One line |
| [local-build.md](local-build.md) | The static, single-user build: the backend in WebAssembly on SQLite-wasm, single-user mode, keeping the data | The local build's mechanisms change |
| [readable-ids.md](readable-ids.md) | Handles: usernames, slugs, item keys and short links, and why they're shaped as they are | The handle rules change |
| [cloud-run-postgres.md](cloud-run-postgres.md) | Postgres support, and the production deployment on Cloud Run with Neon: setup, deploy, rollback, risks | The deployment changes |
| [ux-overview.md](ux-overview.md) | The web app's UX principles, and the rules that settle its open questions | A principle or a rule changes |
| [app-ux.md](app-ux.md) | The web app's layout and interactions at each width | The layout changes |
| [screens.md](screens.md) | An inventory of every screen and overlay, with how often each is used and how much each build needs it. The first design set was picked from it | Rarely. Its core flows and gaps are what's kept current |
| [cli.md](cli.md) | The `upheap` command-line client: commands, output, errors, sign-in, layout. Not built yet | The design changes, until it's built |

The web app's mockups live outside the repository, in the "Upheap layouts" canvas. The UX docs name its boards
(Main, PhoneItem, …) where they refer to one.
