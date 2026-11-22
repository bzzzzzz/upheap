# Working with agents

Running an agent loop on your own machine: the setup, what a run may do, where it works, and the flags that change
it. The loop is `upheap work run`. It claims the next item an agent may take, starts Claude Code or GitHub Copilot
CLI on it, records the run, and repeats. Upheap runs no model itself.

## Start an agent loop

1. Install `upheap` ([the guide](README.md#1-install-the-cli)) and Claude Code (`claude`, 2.1.326 or newer) or
   GitHub Copilot CLI (`copilot`, 1.0.112 or newer), signed in.
2. Sign in to your server: `upheap auth login --server https://<your server>`.
3. Make or pick a heap: `upheap heap add "Website"`, then `upheap heap use website`.
4. Write its context: where the repository is, and how work is done and reviewed there. `upheap heap edit
   --context-file context.md`.
5. In the web app, open the heap's settings and turn on "Agents prepare new notes", so each note you add goes to an
   agent. The CLI has no flag for it.
6. Run `upheap work run --tool claude --root ~/agent-runs` (or `--tool copilot`) and leave it running. Ctrl-C stops
   it.
7. Add a note (`upheap item add "..." --need expected`), answer what the agent asks (in the web app, or with
   `upheap question reply`), and send items on with `upheap item send` or from the web app with "Send to an agent".

When it starts, the loop runs `<tool> --version` and refuses to start, before it claims anything, if the tool is
older than the oldest version it supports (step 1) or if it can't read a version in what the tool printed.
`claude update` or `copilot update` brings the tool up to date.

## What goes back and forth

```sh
upheap work next                          # what an agent would work on next, without taking it
upheap work queue                         # the agents' whole queue, in order, with why each is there
upheap work log                           # what the agents reached in the last 30 days, newest first
upheap item send k3x9a --line "Use the new CA."   # to the next stable state; --all-the-way goes on until done
upheap item take-back k3x9a               # the agent's turn ends; what it changed stays
upheap question reply k3x9a.q1 --pick Internal    # answer what the agent asked
```

An agent takes an item one stable state further and stops: a note gets its context, then becomes a defined task,
then is done. It never closes its own work. It asks for a review with a link to where the work is, and the task
is done when you agree.

## What a run may do

1. **A run writes only its own item.** The loop starts the tool with `UPHEAP_RUN` set to the run's ID. Every
   `upheap` request is then made inside that run: it may write the item's context, steps, questions and comments,
   make a note or a reminder a task and start it, on the run's own item only, and is refused anything else with
   exit 3.
2. **Without `--root`, a run works in the directory the loop was started in.** To prepare an item (give it its
   context, or make it a defined task), `claude` may run the `upheap` commands a run uses (not `auth`), read files
   under that directory and use the web, and nothing else. To do a task, it may also edit files, and run what your
   own and the project's Claude settings allow.
3. **With `--root`, each item gets its own directory,** `<root>/<team>/<heap>/<key>`, with a `CLAUDE.md` that says
   how to work there. `claude` may run any command, edit and read, for preparing as well: a run there has to clone
   and build what it works on.
4. **The directory is kept** and used again for the item's next run, with what the last run left in it. The loop
   removes it once the item is closed or can't be found, or after 7 days with no run on the item, checking when it
   starts and about every hour. Old `<key>-<run id>` directories from earlier versions are left for you to clear.
5. **Every run is denied `upheap auth` and `upheap heap use`,** though an allow rule as wide as `Bash` lets a run
   get around that through a shell.
6. **A silent run is stopped.** Claude's streamed output for each run is saved in `<config dir>/runs/<run id>.jsonl`
   (kept 7 days), and a `claude` that shows no output for 30 minutes is stopped and its run failed. `claude` prints
   nothing while a single tool call runs, so a tool call longer than 30 minutes (a raised Bash timeout, a slow MCP
   tool) counts as silence.

## Flags

```sh
upheap work run --tool claude --heap work --once              # one run in a heap, or nothing when there is nothing to do
upheap work run --tool claude --root ~/agent-runs --workers 3  # up to 3 runs at once, each on its own item
upheap work run --tool claude --instructions ~/how-i-work.md   # tell every run how you want work done on this machine
upheap work run --tool claude --allow mcp__claude_ai_Claude_Docs --allow-do 'Bash(npm:*)'
upheap work run --tool claude --model opus                     # start every run's claude with --model opus
```

1. **`--instructions <file>`** tells every run what the file says: your own way of working on this machine (which
   skills to use, how to check and review a change). It comes before the heap's context on how the work is done.
   Repeat it to pass several files, such as a shared one and one for this machine: they are joined in the order
   given. The files are read once, when the loop starts.
2. **`--allow <rule>`** lets every run's `claude` also use what the rule allows (Claude's rule syntax, such as
   `mcp__claude_ai_Claude_Docs`), without allowing it in your own sessions. `--allow-prepare` and `--allow-do` add
   a rule to runs that prepare an item or do a task only. Each flag takes one rule and can be repeated; a rule
   with a comma or a space, or one starting with `-`, is refused.
3. **`--model <model>`** is passed to the tool as it is. Each run records the tool and the model (the one the
   tool's own output names, else the one given), and the item's Runs list shows them.

## GitHub Copilot CLI

With `--tool copilot`, the loop starts `copilot` the same way, and these differ:

1. A run may run any `upheap` command but `auth` and `heap use`, since Copilot's rules can't allow single
   subcommands.
2. To do a task, edits are accepted and other commands follow your saved Copilot approvals.
3. Under `--root` the directory gets `AGENTS.md`, and any tool is allowed.
4. `--allow` rules are in Copilot's syntax (`shell(npm:*)`) and may hold spaces.
5. The cost is counted from Copilot's AI credits, at 100 credits to the dollar.
6. A usage limit is not recognised, so a run that stops at one fails.
7. Copilot saves each run as a session in `~/.copilot`; account sync is turned off for the run.

## Playing the agent by hand

The loop is built from commands you can run yourself, which is how another tool can be wired in:

```sh
upheap work claim --quiet                                        # take the next item: starts a run and prints its ID
UPHEAP_RUN=<id> upheap item edit k3x9a --context-file context.md   # with a run, writes are the run's
UPHEAP_RUN=<id> upheap work reached --result "Wrote the context"   # the turn's goal is reached
UPHEAP_RUN=<id> upheap work finish --cost-usd 0.12                 # the run is over; what it cost
```

`upheap guide show` prints every command with instructions for an agent to read first.
