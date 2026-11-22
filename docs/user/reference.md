# CLI reference

What `upheap --help` doesn't list in one place: where the CLI keeps its settings, the environment variables that
override them, and the exit codes. The commands and flags are in `upheap --help` and `upheap guide show`.

## Settings

The server address and the default heap are kept in `config.toml` under the platform's config directory.
`upheap auth login --server` and `upheap heap use` write it.

| Setting | File | Environment | Flag |
| --- | --- | --- | --- |
| The server's address | `config.toml` | `UPHEAP_SERVER` | `--server` |
| The heap to work in | `config.toml` | `UPHEAP_HEAP` | `--heap` |
| A personal token | | `UPHEAP_TOKEN` | |
| The run an agent writes in | | `UPHEAP_RUN` | |

A flag beats the environment, and the environment beats the file.

`UPHEAP_TOKEN`, a personal token made on the web app's Account page ([tokens.md](tokens.md)), wins over the
keychain sign-in: when it is set, every command uses it and never opens a browser, except `auth login` and
`auth logout`, which manage the keychain sign-in.

With `UPHEAP_RUN` set to a run's ID, every request is made inside that run
([agents.md](agents.md#what-a-run-may-do)).

## Naming things

An item is named by its key (`k3x9a`) within the heap in use, by `team/heap/key`, or by a link to it. A step is
its item and its number, `k3x9a.1`, and a question is `k3x9a.q1`. A heap is `team/heap`, or its slug alone when
only one of your teams has it.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Done |
| 1 | Anything else |
| 2 | Bad command line |
| 3 | Refused |
| 4 | Not signed in, or not found |
| 5 | The server can't be reached |
| 6 | The local setup is broken: no keychain, no agent tool or one too old, or a `UPHEAP_TOKEN` the server refuses |

When the server refuses a token, the message says whether it was revoked, has expired or isn't one the server
knows.
