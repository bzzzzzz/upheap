# Design docs

What each doc is for, and when to update it. The rules the docs follow are in [CLAUDE.md](../CLAUDE.md#docs).

| Doc | What it holds | Update it when |
| --- | --- | --- |
| [engineering-principles.md](engineering-principles.md) | How we write code: the shared rules from the maintainer's engineering base, and this project's own. Must follow | A rule changes, code and a rule disagree and the rule was the wrong one, or the base changes |
| [product.md](product.md) | What the product does and why: distribution, accounts, teams, heaps, items, ranking, views, what's never deleted, roles, the audit log. Commitments, built or not | The product changes, not when the code catches up with it |
| [architecture.md](architecture.md) | How it's built and why: principles, stack, data model conventions, handles, ranking, API conventions, authentication, access control, the audit log, deployment, the local build, testing. Mechanisms and reasons, never lists of what the code holds | A mechanism or a reason changes |
| [decisions.md](decisions.md) | Choices that could have gone another way: context, decision, what it replaced, consequences | A choice is made or superseded. Append; don't rewrite |
| [operations.md](operations.md) | Running production: the Cloud Run and Neon setup, deploying, the one-time cloud setup, rolling back, cautions | The deployment changes |
| [ux.md](ux.md) | The web app's UX: principles, core flows, toolkit, layout at each width, each screen's behaviour, the rules that settle questions, what comes later | A principle, a layout or a behaviour changes |
| [work/progress.md](work/progress.md) | Status by area, what's next, open questions | Any change that moves an area's status. One line |
| [work/cli.md](work/cli.md) | The `upheap` command-line client: commands, output, errors, sign-in, layout. Built; to be folded into the permanent docs | The design changes, until it's built |
| [work/getting-in.md](work/getting-in.md) | The design of getting in: invite links, password change and reset. Not built yet; being designed step by step | The design changes, until it's built |

`work/` holds what's in flight: the status, and a doc per piece of unbuilt work, deleted once it's built. Everything
beside it is permanent.

The web app's mockups live outside the repository, in the "Upheap layouts" canvas. ux.md names a board where it
relies on one.
