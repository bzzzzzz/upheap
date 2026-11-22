# Using Upheap

For someone using Upheap from the command line, alone or with agents: from nothing to a ranked list, and then to
an agent working it. It needs a running server; starting one is in the [operator's guide](../operator/README.md).
The web app explains itself, so nothing here walks through its screens.

## 1. Install the CLI

`upheap` talks to a running server over its API. On macOS with Apple silicon:

```sh
brew install bzzzzzz/tap/upheap
```

On Linux (x86_64 or aarch64), a static binary with no system dependencies:

```sh
arch=$(uname -m)   # x86_64 or aarch64
base=https://github.com/bzzzzzz/upheap/releases/latest/download
curl -fsSLO "$base/upheap-$arch-unknown-linux-musl.tar.gz"
curl -fsSLO "$base/SHA256SUMS"
grep " upheap-$arch-unknown-linux-musl.tar.gz\$" SHA256SUMS | sha256sum -c
mkdir -p ~/.local/bin
tar -xzf "upheap-$arch-unknown-linux-musl.tar.gz" -C ~/.local/bin upheap
```

`~/.local/bin` must be on your `PATH`; some distributions add it only if it existed when you logged in.

## 2. Sign in

```sh
upheap auth login --server https://upheap.example.com
```

It opens the browser, you sign in there, and the sign-in goes in the keychain. The CLI never sees your password.
On Linux the keychain is the Secret Service, so a desktop keyring (GNOME Keyring or KWallet) must be running. On a
machine with no browser or no keyring, use a personal token: [tokens.md](tokens.md).

## 3. Pick a heap

A heap is one ranked list. You have a personal one from the start.

```sh
upheap heap list
upheap heap add "Website"        # in your personal team; --team infra for a shared one
upheap heap use website          # the default for the commands that follow
```

## 4. Put everything in

Add items as they come up, each with how much it's needed and whatever else you know. Only the title is required.

```sh
upheap item add "Rotate the staging certs" --need expected --effort s --due next-week
upheap item add "Order the new certificate" --task --step "Order it" --step "Install it"
upheap item add "Renew the domain" --reminder 2027-03-12   # out of the list until that date
```

A note is a thought or a piece of work, a task is work with a checklist, and a reminder stays out of the list
until its date.

## 5. Read the ranking

```sh
upheap item list                 # the heap, most important first, with why each item is where it is
upheap item list --all-heaps     # every heap you can use, in one list
upheap item show k3x9a
```

The top is what matters now. Each line says what put the item there. What the ranking weighs, and how to move an
item when you disagree with it, is in [ranking.md](ranking.md).

## 6. Work the list

```sh
upheap item start k3x9a
upheap step tick k3x9a.1
upheap item block k3x9a --reason "Waits for the vendor's quote"
upheap item done k3x9a
```

Nothing is deleted: work that won't happen is closed as won't do, and a closed item can be reopened for a week.

## 7. Hand work to an agent

An agent tool you already have, Claude Code or GitHub Copilot CLI, can prepare your notes and do your tasks on
your own machine. It asks you when it needs you and waits for your review before anything is closed.

```sh
upheap work run --tool claude --root ~/agent-runs
upheap item send k3x9a           # this item, one step further
upheap question reply k3x9a.q1 --pick Internal
```

The setup, what a run may do and the flags are in [agents.md](agents.md).

## More

| To | Read |
| --- | --- |
| See every command and flag | `upheap --help`, `upheap <command> --help`, or `upheap guide show` for all of it in one read |
| Understand why an item ranks where it does | [ranking.md](ranking.md) |
| Run an agent loop | [agents.md](agents.md) |
| Sign in on a machine with no browser | [tokens.md](tokens.md) |
| Use Upheap from a Claude Code conversation | [claude-code.md](claude-code.md) |
| Look up the config file, the environment variables or the exit codes | [reference.md](reference.md) |
