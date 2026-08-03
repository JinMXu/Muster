---
name: muster
description: >-
  Drive the Muster terminal workspace from the shell with the `muster` binary — list workspaces/tabs/panes, split a
  pane, send keystrokes into one, capture what is on a pane's screen, run a command in a real PTY and read its output
  plus exit code, and see which coding agents are running in which session. Use this whenever Muster, panes, sessions,
  or "the other terminal" come up; whenever you need to start something long-running or interactive (dev server, REPL,
  `tail -f`, a TUI) that should not sit blocking your Bash tool; whenever a program needs a real terminal to behave
  the way the user sees it; and whenever you need to look at or report on what is running in some other terminal on
  this machine. Cheap to check: `muster doctor` tells you in one line whether the bridge is up.
---

# Driving Muster from the command line

`muster` is the same binary as the Muster GUI; invoked with a verb it becomes a
thin, non-interactive client of the running app over local IPC. Every verb
returns and exits. The GUI does not have to be visible — closing the window
keeps Muster alive in the tray and its sessions running.

## First: where are you?

```bash
muster doctor
```

One table, and it answers everything you need before doing anything else:
whether the bridge is reachable, how many windows/sessions are open, and how
many sessions currently run a coding agent.

If `muster doctor` says the bridge is unreachable, Muster is not running (or
is an old build). The CLI will start it automatically and wait for the bridge,
so usually you don't need to do anything. Starting Muster opens its window —
say so if that surprised the user.

## When to use this instead of the Bash tool

The Bash tool is right for anything that starts, does its job, and exits.
Reach for `muster` when one of these is true:

- **It shouldn't block you.** A dev server, a watcher, `tail -f`, a long test
  run you want to check on later. Put it in a pane, come back and read it.
- **It's interactive or stateful.** A REPL, a database shell, anything where
  you send one thing, read the answer, then send the next. A pane keeps the
  session alive between your turns; a Bash call cannot.
- **It needs a real TTY.** Programs that detect a pipe and change behaviour —
  colour, progress bars, TUIs, `top`, anything using raw mode. `muster run`
  gives a genuine PTY.
- **The user should be able to watch it.** Anything in a pane shows up in
  their Muster window, live. That is often the whole point.
- **You're being asked about something you didn't start.** "What's running in
  that pane?", "why is port 3000 taken?", "what are my agents doing?" — you
  can answer those from here without touching anything.

## Addresses

Sessions are identified by their UUID (printed by `new`/`split`/`run`, listed
by `muster ls`). A session id is stable for the whole life of the session —
safe to remember across steps. Re-resolve `ls` before referencing a session
that might have been closed.

## Running a command: two shapes

### Blocking: `run` returns the output and exit code

```bash
muster run -- cargo test           # streams nothing; waits, then prints output + exit code
muster run --dir C:\work -- cargo build
```

`run` opens a **new terminal tab** in the project, types the command, waits
until the command's whole process tree has exited, then prints the captured
output (plain text, escapes stripped). The last line of the output carries the
command's exit code as `__MUSTER_RC=<n>` — actually it's removed and returned
to you in the JSON (`--json`) or used for the CLI's own exit status.

Notes:
- Use `--` before the command so its own flags aren't eaten (`muster run --
  npm --version`).
- Timeout: 600s by default; `--timeout 30` to change. On timeout the partial
  output is returned with `timed_out: true` (the pane stays open — you can
  `capture` it later).
- Exit code is read via `$LASTEXITCODE` (PowerShell) / `%errorlevel%` (cmd).
  It is stale for pure-cmdlet commands — don't rely on it for `Copy-Item`
  etc.

### Non-blocking: a pane you talk to over time

This is the one that makes Muster worth reaching for. Get a session, send it
work, come back later.

```bash
SID=$(muster split)                # splits the focused pane, prints the new session id
muster send "$SID" 'npm run dev' --enter
muster send "$SID" 'ls -la'        # no --enter: just types, nothing runs yet
```

`split` splits whatever pane the user is focused on and prints the new
session's id — the whole point is the user can watch it. Say that you did it.

If there is no project yet, `muster new <dir>` opens one (and prints the id of
its first terminal).

## Reading a pane

```bash
muster capture $SID                # last ~200 lines of output, ANSI stripped
muster capture $SID --lines 1000   # more scrollback (ring keeps the last 400 lines)
```

`capture` returns what the backend ring buffer has: ANSI escapes are gone, a
line wrapped at the terminal edge is one long line, a progress bar reads as
its final value. It's a snapshot — call it again for a newer one. The full
terminal buffer lives in the GUI (xterm.js); the ring holds the last 400
lines.

```bash
muster procs $SID
```

lists the process tree inside the session — shell first, then its children —
plus any listening TCP ports. When the only entry left is the shell, a command
you sent is done. That is a far more reliable "finished?" signal than
grepping the screen.

## Looking around

```bash
muster ls                  # every window: projects, tabs, panes (kind + cwd)
muster agents              # every coding agent detected in a session: running / waiting
muster doctor              # bridge health: version, windows, sessions, agents
```

`muster agents` is worth knowing about: it reports each session running a
recognised coding agent (opencode, Claude Code, Codex, Kimi Code, aider,
Gemini CLI, Goose) with a semantic state:

- `working` — alive, produced output recently;
- `waiting` — alive but silent (likely needs input/approval);
- `done` — the agent process exited but the session is still alive and the
  user hasn't looked at it yet.

If you are one of the recognised agents, you are in that list too.

### Coordinating with `wait` and `watch`

```bash
muster wait $SID                    # blocks until the session's agent reaches `done` (default)
muster wait $SID --until working    # or --until waiting
muster wait $SID --timeout 30       # default timeout 600s; exits 1 on timeout
muster watch                        # streams agent-status-changed events until you stop it
```

`wait` polls the shared agent cache every ~0.5s (the cache itself is
refreshed every ~3s by the background poller) and returns the resolved
state. A session whose agent disappears (closed, or the user already saw it
finish) counts as `done` for a `--until done` wait, so multi-agent
orchestration ("kick off the other agent, then wait for it to finish")
works even when nobody is watching the pane.

`watch` opens a long-lived connection and prints one `agent-status-changed`
JSON object per line as statuses change (plus a `ping` heartbeat). It's the
push equivalent of repeatedly polling `muster agents`. Stop it with Ctrl+C
or by closing the pipe.

### Sending key combos, not just text

```bash
muster send-keys $SID ctrl+c        # interrupt the running command
muster send-keys $SID enter         # press Enter
muster send-keys $SID "ctrl+l up"   # multiple combos, space-separated
muster send-keys $SID '["ctrl+c","enter"]' --json   # or a JSON array
```

`send-keys` sends semantic key combos (not literal text): named keys
(`enter`, `esc`, `tab`, `backspace`, `space`, `up`/`down`/`left`/`right`,
`home`, `end`, `pageup`/`pagedown`, `insert`, `delete`, `f1`–`f12`), an
ASCII char, or `ctrl+`/`alt+`/`shift+` combos of those (`ctrl+c`,
`shift+tab`, `alt+f1`, `ctrl+right`, …). `send` is for literal text; reach
for `send-keys` when you mean a key (interrupting, scrolling history with
arrows, accepting a prompt with Enter).

Add `--json` to any verb for machine-readable output.

## Don't break the user's session

The panes on this machine are the user's real work, and some of them are
other coding agents mid-task. Treat anything you did not create as read-only:

- **Never `send` into a session you didn't open.** Keystrokes into another
  agent's pane, or into a shell the user is typing in, land in the middle of
  whatever is happening there. Check `muster agents` before you touch a pane.
- **Never close or kill anything you didn't create.** There is no close verb
  on purpose. If the user wants a pane gone, they close it.
- **Clean up what you did create** only when it's no longer useful — actually,
  `run`/`split` panes are small; leaving a dev server pane open for the user
  to inspect is usually the point. Mention what you left running.

## Full command reference

| verb | args | prints |
|---|---|---|
| `doctor` | — | version, windows, sessions, agents |
| `ls` | — | window/project/tab/pane tree with cwds |
| `agents` | — | session id, agent, state per detected agent |
| `new` | `[dir]` / `--dir <path>` | new project's first session id |
| `split` | `--v` / `--h` (default `--h`), `--dir <path>` | new session id |
| `send` | `<id> <text> [--enter]` | `ok` |
| `send-keys` | `<id> <combo...>` (e.g. `ctrl+c enter`) | number of combos sent |
| `capture` | `<id> [--lines N]` | plain text |
| `procs` | `<id>` | process tree + listening ports |
| `run` | `-- <command>`, `--dir <path>`, `--timeout <secs>` | session id, then output |
| `wait` | `<id>`, `--until done\|working\|waiting`, `--timeout <secs>` | resolved state; exit 1 on timeout |
| `watch` | — | streamed `agent-status-changed` events (newline JSON) |

Common flags: `--json` (structured output). `--` ends flag parsing.
