# detection

A variant of StrIEM's `detection` service. The [rsigma](https://github.com/timescale/rsigma)
engine drives it. It is a replacement for StrIEM's built-in detection service.
It uses StrIEM's Vector input and output and its event model with no change. But
it replaces the `sigmars` engine with rsigma-runtime. Thus it takes rsigma's
conflict-based `logsource_compatible` pruning path. It does not take the strict
logsource subset filter.

```text
  Vector ──gRPC PushEvents──▶ striem_vector server ──▶ LogProcessor / RuntimeEngine
                                                              │  logsource_compatible
                                                              │  conflict pruning
                                                              ▼
  Vector ◀─gRPC PushEvents── striem_vector client ◀── OCSF Detection Findings (2004)
```

## What it reuses and what it replaces

| Concern | Source |
|---|---|
| Vector gRPC input and output | `striem_vector` (`Server`, `Client`) — **reused as-is** |
| In-process event model (`Event`, `SysMessage`) | `striem_common` — **reused as-is** |
| OCSF Detection Finding output (class_uid 2004) | this crate (`ocsf.rs`) — the same shape as StrIEM's `rule_to_ocsf` |
| Detection engine | `rsigma-runtime` `RuntimeEngine` / `LogProcessor` — **replaces `sigmars`** |
| Rule selection | rsigma `LogSourceExtractor` and conflict pruning — **replaces the subset filter** |

## The pruning path — how it is not the same as StrIEM's detection service

StrIEM's `detection` service selects rules with a logsource **subset** filter. A
rule runs only if its logsource is a subset of the event's logsource. Thus an
event that is *less* specific than a rule **excludes** that rule.

This service installs a `LogSourceExtractor` on the engine in place of the subset
filter. This selects rsigma's **conflict-based** evaluation:

- The engine skips a rule **only** when a logsource dimension of the rule
  *conflicts* with the event's extracted logsource (for example, rule
  `product: linux` against event `product: windows`).
- A rule with no conflict still runs. A rule with no logsource also runs.
- An event with **no** logsource evaluates against all the rules (fail-open).

This means "do not run rules that contradict the event." It is not a whitelist.
[`tests/pruning.rs`](tests/pruning.rs) holds the three cases. A full
event-to-finding round trip is in [`tests/handler.rs`](tests/handler.rs).

## Running

```sh
cargo run -p detection -- \
  --rules ./rules \
  --input 0.0.0.0:6000 \
  --output http://vector:6001 \
  --event-logsource product=windows
```

| Flag | Env variable | Default | Meaning |
|---|---|---|---|
| `--rules <PATH>` | `RSIGMA_DETECTION_RULES` | *(required)* | The Sigma rules directory or file |
| `--input <ADDR>` | `RSIGMA_DETECTION_INPUT` | `0.0.0.0:6000` | The Vector input bind address |
| `--output <URL>` | `RSIGMA_DETECTION_OUTPUT` | *(none)* | The downstream Vector endpoint for findings |
| `--batch-size <N>` | `RSIGMA_DETECTION_BATCH_SIZE` | `64` | Events for each engine evaluation |
| `--logsource-field-map <KV>` | `RSIGMA_DETECTION_LOGSOURCE_FIELD_MAP` | *(none)* | Sets which `metadata.logsource` sub-keys feed each dimension |
| `--event-logsource <KV>` | `RSIGMA_DETECTION_EVENT_LOGSOURCE` | *(none)* | The fixed logsource when the event's metadata has none |
| `--no-logsource-pruning` | — | *(off)* | Turns off conflict pruning (evaluate every rule) |

`<KV>` is `product=…,service=…,category=…,custom.<dim>=…`.
`RUST_LOG` sets the log level (the default is `info`), the same as the rest of
StrIEM.

### Where the logsource comes from

Each event's logsource comes from **`Event.metadata["logsource"]`** (StrIEM's
convention). It does *not* come from the log body. Vector, or an upstream
normalizer, must set it, for example:

```json
{ "product": "windows", "service": "sysmon" }
```

The rules match against the event body (`Event.data`). The two are apart. The
service shows the logsource only to the pruning extractor. It never shows the
logsource to keyword or `|contains` matching. Thus a logsource value such as
`"windows"` can never make a keyword rule fire (see `src/logsource_event.rs`).

- **Field map** (`--logsource-field-map product=os_type`): read a dimension from
  a sub-key of `metadata.logsource` that has a different name (here, product
  reads from `metadata.logsource.os_type`). A dimension that you do not set uses
  the standard `product`, `service`, or `category` key. Most deployments do not
  set this.
- **Fixed default** (`--event-logsource product=windows`): the service uses it
  when the event's metadata has no such dimension. This is useful when one input
  stream is always one platform.

When the metadata has no logsource and there is no fixed default, the event is
**fail-open**: every rule can run (the service prunes nothing).

## Vector configuration

Point a Vector `vector` sink at the input address. Point a `vector` source at
the place where you route the findings:

```toml
[sinks.to_detector]
type = "vector"
inputs = ["parse_logs"]
address = "http://detection:6000"

[sources.from_detector]
type = "vector"
address = "0.0.0.0:6001"
```

## Relationship to the workspace

`detection` is a member of the StrIEM Cargo workspace. It path-depends on
StrIEM's `lib/common` and `lib/vector`. It depends on the rsigma engine crates
(`rsigma-parser`, `rsigma-eval`, `rsigma-runtime`) as **git dependencies** pinned
to a tag. Thus the Docker image builds on its own.

For local development against a live `../rsigma` checkout, the workspace-root
`.cargo/config.toml` has a `paths` override. This override sends those crates to
`../rsigma/crates/*`. Thus you can use rsigma changes at once. The
`.dockerignore` excludes this override from the Docker build. Thus the image
always builds from the pinned git tag. (Cargo prints a warning about the override
because the rsigma crates depend on each other. The warning is harmless. It does
not affect correctness.)

## Docker

The service ships in the shared `striem:latest` image. It runs as the
`detection` service in `docker-compose.yaml`:

```sh
docker compose build      # builds striem_api and detection
docker compose up
```

The rsigma crates are a **private** git dependency. Thus the image build needs
Git credentials for `github.com/timescale/rsigma`. Give them with a BuildKit SSH
mount (`docker build --ssh default …`) or a token-based credential helper. The
Dockerfile sets `CARGO_NET_GIT_FETCH_WITH_CLI=true`. Thus the system git does the
authentication.

The compose runtime configuration uses the `RSIGMA_DETECTION_*` variables (see
the table above). `detection` hosts the detection-admin gRPC API on its
input listener. Thus the api and UI rule proxy works against it. The rules also
live on disk under `RSIGMA_DETECTION_RULES`.
