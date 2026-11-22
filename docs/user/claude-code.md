# From Claude Code

The `upheap` plugin for Claude Code: three skills that add items, report status and do the next item from a
conversation.

This repository is also a Claude Code plugin marketplace with one plugin, `upheap`. Its skills work through the
CLI, so they need `upheap` installed and signed in. Each runs when you type its command or name Upheap in a
request.

- `/upheap:add` adds notes, tasks and reminders to a heap from a conversation, and a new heap when you ask for
  one.
- `/upheap:status` says what's in progress, what waits for you, and what's next and why.
- `/upheap:do-next` takes the next item: a note gets its context and becomes a task or stays a note; a task gets
  done, or part of it, or you're told how to do it when an agent can't.

`status` and `do-next` take a heap (`/upheap:status bzzzz/personal`) or `all`.

```
/plugin marketplace add bzzzzzz/upheap
/plugin install upheap@upheap
```

The plugin is in [plugins/upheap](../../plugins/upheap).
