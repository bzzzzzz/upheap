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
| [work/README.md](work/README.md) | The tracker: the tracks in flight, what's next, ideas | A track starts or ends. One line |
| [work/work-with-agents.md](work/work-with-agents.md) | Work with agents: context, tasks with steps, reminders, questions and the agent loop. What it is and its progress | A piece is done, until it's built |
| [work/work-with-agents-design.md](work/work-with-agents-design.md) | Work with agents, the design: commitments, workflows, concepts and rules, interface, choices | The design changes, until it's built |
| [work/side-tasks.md](work/side-tasks.md) | Side tasks: small pieces that run beside the tracks in flight, each with when it's done | A piece is done, until all are |
| [work/smaller-changes.md](work/smaller-changes.md) | Smaller changes: why a feature touches so many files, the refactors that remove the repeats, and what stays as it is | A piece is done, or the plan changes, until it's built |

`work/` holds what's in flight: the tracker, and a doc per track of work, with its design in a second file when
that is long, deleted once it's built. Everything
beside it is permanent.

The web app's mockups live outside the repository, in the "Upheap layouts" canvas. ux.md names a board where it
relies on one.
