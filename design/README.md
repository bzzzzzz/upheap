# Design docs

What each doc is for, and when to update it. The rules the docs follow are in [CLAUDE.md](../CLAUDE.md#docs).

| Doc | What it holds | Update it when |
| --- | --- | --- |
| [engineering-principles.md](engineering-principles.md) | How we write code: the shared rules from the maintainer's engineering base, and this project's own. Must follow | A rule changes, code and a rule disagree and the rule was the wrong one, or the base changes |
| [product.md](product.md) | What the product does and why: distribution, accounts, teams, invite links, heaps, items and their kinds, questions, working with agents, review, ranking, views, what's never deleted, roles, the audit log. The core commitments, built or not | A promise to a user changes, not when the code catches up with it |
| [architecture.md](architecture.md) | How it's built and why: principles, stack, data model conventions, handles, ranking, API conventions, authentication, access control, the audit log, deployment, the local build, the CLI, testing. Mechanisms and reasons, never lists of what the code holds | A mechanism or a reason changes |
| [decisions.md](decisions.md) | The few choices that are costly to reverse: context, decision, what it replaced, consequences | A choice passes its bar, or one is superseded. Append; don't rewrite |
| [operations.md](operations.md) | Running production: the Cloud Run and Neon setup, deploying, the one-time cloud setup, rolling back, cautions | The deployment changes |
| [ux.md](ux.md) | The web app's UX: principles, core flows, toolkit, layout at each width, each screen's behaviour, the rules that settle questions, what comes later | A principle, a layout or a behaviour changes |
| [work/README.md](work/README.md) | The tracker: the tracks in flight, what's next, ideas | A track starts, ends or changes stage. One line |
| [work/getting-in.md](work/getting-in.md) | Getting in: invite links, password change and reset. What it is, its progress, its design | A piece is done, or the design changes, until it's built |
| [work/work-with-agents.md](work/work-with-agents.md) | Work with agents: context, tasks with steps, reminders, questions and the agent loop. What it is, its progress, its design | A piece is done, or the design changes, until it's built |

`work/` holds what's in flight: the tracker, and a doc per track of work, deleted once it's built. Everything
beside it is permanent.

The web app's mockups live outside the repository, in the "Upheap layouts" canvas. ux.md names a board where it
relies on one.
