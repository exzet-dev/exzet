# exzet
A stupid simple way to distribute tasks of limitless complexity to any exzet enabled server, all from your IDE.

exzed (daemon) + exzec (client) + exfile (task file) = exzet

## Install

```sh
curl -fsSL https://exzet.net/install.sh | sh
```

Static binaries (`exzec` + `exzed`) for linux/macos (x86_64 + aarch64) and windows (x86_64) on [releases](https://github.com/exzet-dev/exzet/releases).

From source: `cargo install --git https://github.com/exzet-dev/exzet`

Start exzed and copy server entry from log:

```
exzed listening on 0.0.0.0:7433
server entry: 9f2ca61b...@<this-host>:7433
```

The token is generated on first run and kept in the state dir

Flags: `--listen host:port` (default `0.0.0.0:7433`), `--state path`, `--token value`, `--containers true|false`, `--service`/`--disable` (systemd unit, idempotent).

### Client

Server entry goes in any of:

1. exfile: `servers := token@host:7433`
2. per run: `exzec --server token@host:7433 build`
3. `~/.config/exzet/servers`, one per line

First reachable wins. No servers: runs locally.

## Usage

[`exfile`](exfile) at project root

```sh
exzec # list tasks
exzec release
exzec -f ./exfile release # same same, working dir is rel to exfile
exzec -d release # print job id then run in background
exzec ps # jobs on your servers
exzec attach <job> # replay + follow output, ctrl-c kills it
```

 - Workspace is hashed and `.gitignore` works
 - Only files the server is missing get uploaded
 - Output streams back
 - Task exit code is exzec's exit code
 - ctrl-c kills job, here or remote 
 - `outputs` go back into your workspace if remote, useless if local

### exfile syntax

- `[key: value, ...]` = attributes for task
- `key := value` = for all tasks in exfile
- `[ENV_EXAMPLE := value, ...]` = env for task
- `ENV_EXAMPLE := value` = for all tasks in exfile
- `${ENV_EXAMPLE}` expands when setting values
- `.env` beside the exfile auto-loads into exzec env
- `build: check lint` = serialized dependencies just like every other task runner
- `exzec task a b` -> `$0` = "task", `$1` = "a", `$2` = "b"
- `#!` shebangs override shell

| key | example | |
|---|---|---|
| `servers` | `token@host:7433` | space-separated, first reachable |
| `shell` | `bash -euc` | default `sh -euc` on mac/linux, powershell on windows |
| `image` | `rust:1` | docker image workspace, overrides entrypoint |
| `cpus` | `8` | cpu limit |
| `mem` | `64g` | k/m/g/t |
| `time` | `10m` | s/m/h/d, exit 124 on timeout |
| `replicas` | `4` | parallel copies |
| `workspace` | `live` | `sync` default |
| `outputs` | `target/release/exzec` | copied back after run |
| `gpus` | `1` | docker only, passed as `--gpus` |
| `disk` | `100g` | TODO |

### replicas

`replicas: N` runs N copies of the script concurrently, spread across every reachable server in `servers` (or all locally).
You can use these env variables (set per process) in order to split up work among replicas:
- `EXZET_RANK` (0..N-1)
- `EXZET_WORLD` (N)
- `EXZET_JOB` (id shared by the whole run)
- `EXZET_MAIN` (address of the server hosting rank 0, for rendezvous)

Output is line-buffered and each line prefixed `[rank]`. 
`cpus`/`mem` apply per rank, not per job. 
Exit code is 0 only if every rank exits 0, else the first nonzero.
Ranks share a working directory per server, synced separately.
`outputs` come back from the rank 0 server only.
It's your scripts job to split work, exzet just gives you the ability to do that.

### detach

`exzec -d <task>` hands the job to the server and exits with its id; the job outlives your connection, laptop, everything.
Output spools on the server. `exzec attach <id>` replays it, follows live, delivers `outputs`, and returns the exit code; ctrl-c while attached kills the job.
With multiple servers you get one job id per server.
Needs a server and the sync workspace. Job list resets when exzed restarts.

### live

When a server is configured, `exzec --live <task>` or `[workspace: live]` will serve your working tree over NFS and `exzed` mounts it as the task's working directory.
NFS uses the client-dialed connection on the same server port, so no new ports and your machine ever need to be exposed.
Unlike sync, `.gitignore` is ignored and `exzed` sees the whole tree.
Needs root owned `exzed` and `replicas: 1`.

### platforms

Doing everything I can to keep everything running exactly the same on every platform:

| | linux | macos | windows |
|---|---|---|---|
| run/kill | process groups | process groups | job objects |
| `cpus`/`mem` | cgroup v2 (root) | off, warns | job objects |
| live workspace | kernel nfs mount (root) | `mount_nfs` (root) | needs 'Client for NFS' Windows feature on in OS |
| `--service` | systemd | launchd | scheduled task |
| docker/`gpus` | wherever docker runs | same | same |
