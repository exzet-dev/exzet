# exzet
A stupid simple way to distribute tasks of limitless complexity to any exzet enabled server, all from your IDE.

exzed (daemon) + exzec (client) + exfile (task file) = exzet

## Install

```sh
cargo install --git https://github.com/nickheyer/exzet
```

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
```

 - Workspace is hashed and `.gitignore` works
 - Only files the server is missing get uploaded
 - Output streams back
 - Task exit code is exzec's exit code
 - ctrl-c kills job, here or remote 
 - `outputs` go back into your workspace if remote, useless if local

### exfile syntax

- `[key: value, ...]` = attributes for task
- `key := value` = for all tasks in exfile, see below

| key | example | |
|---|---|---|
| `servers` | `token@host:7433` | space-separated, first reachable |
| `image` | `rust:1` | docker image workspace, overrides entrypoint |
| `cpus` | `8` | cpu limit |
| `mem` | `64g` | k/m/g/t |
| `time` | `10m` | s/m/h/d, exit 124 on timeout |
| `replicas` | `4` | parallel copies |
| `workspace` | `live` | `sync` default |
| `outputs` | `target/release/exzec` | copied back after run |
| `gpus`, `disk` | `1`, `100g` | TODO |

### replicas

`replicas: N` runs N copies of the script concurrently all on the one server that takes the job (or all locally) in a shared workspace.
You can use these env variables (set per process) in order to split up work among replicas:
- `EXZET_RANK` (0..N-1)
- `EXZET_WORLD` (N)
- `EXZET_JOB` (id shared by the whole run)
- `EXZET_MAIN` (address of rank 0's host, for rendezvous)

Output is line-buffered and each line prefixed `[rank]`. 
`cpus`/`mem` apply per rank, not per job. 
Exit code is 0 only if every rank exits 0, else the first nonzero.
Ranks share the working directory.
It's your scripts job to split work, exzet just gives you the ability to do that.

### live

When a server is configured, `exzec --live <task>` or `[workspace: live]` will serve your working tree over NFS and `exzed` mounts it as the task's working directory.
NFS rides a second client-dialed connection to the same server port, so no new ports and your machine never needs to be reachable.
Unlike sync, `.gitignore` is ignored and `exzed` sees the whole tree.
Needs root owned `exzed` and `replicas: 1`.
