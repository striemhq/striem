# rsigma-detection

A StrIEM `detection`-service variant powered by the [rsigma](https://github.com/timescale/rsigma)
engine. It is a drop-in alternative to StrIEM's built-in detection service that
reuses StrIEM's Vector ingest/egress and event model unchanged, but swaps the
`sigmars` engine for rsigma-runtime — taking rsigma's conflict-based
`logsource_compatible` pruning path instead of a strict logsource subset filter.

```text
  Vector ──gRPC PushEvents──▶ striem_vector server ──▶ LogProcessor / RuntimeEngine
                                                              │  logsource_compatible
                                                              │  conflict pruning
                                                              ▼
  Vector ◀─gRPC PushEvents── striem_vector client ◀── OCSF Detection Findings (2004)
```

## What it reuses vs. replaces

| Concern | Source |
|---|---|
| Vector gRPC ingest / egress | `striem_vector` (`Server`, `Client`) — **reused as-is** |
| In-process event model (`Event`, `SysMessage`) | `striem_common` — **reused as-is** |
| OCSF Detection Finding output (class_uid 2004) | this crate (`ocsf.rs`) — same shape as StrIEM's `rule_to_ocsf` |
| Detection engine | `rsigma-runtime` `RuntimeEngine` / `LogProcessor` — **replaces `sigmars`** |
| Rule selection | rsigma `LogSourceExtractor` + conflict pruning — **replaces the subset filter** |

## The pruning path — how it differs from StrIEM's detection service

StrIEM's `detection` service selects rules with a logsource **subset** filter: a
rule runs only if its logsource ⊆ the event's, so an event *less* specific than
a rule **excludes** that rule.

This service instead installs a `LogSourceExtractor` on the engine, selecting
rsigma's **conflict-based** evaluation:

- a rule is skipped **only** when a logsource dimension it declares *conflicts*
  with the event's extracted logsource (e.g. rule `product: linux` vs event
  `product: windows`);
- rules with no conflict — and all logsource-less rules — still run;
- an event with **no** extractable logsource evaluates against everything
  (fail-open).

This is "don't run contradictory rules," not a whitelist. The three cases are
pinned in [`tests/pruning.rs`](tests/pruning.rs); a full event→finding round
trip is in [`tests/handler.rs`](tests/handler.rs).

## Running

```sh
cargo run -p rsigma-detection -- \
  --rules ./rules \
  --input 0.0.0.0:6000 \
  --output http://vector:6001 \
  --event-logsource product=windows
```

| Flag | Env fallback | Default | Meaning |
|---|---|---|---|
| `--rules <PATH>` | `RSIGMA_DETECTION_RULES` | *(required)* | Sigma rules directory or file |
| `--input <ADDR>` | `RSIGMA_DETECTION_INPUT` | `0.0.0.0:6000` | Vector ingest bind address |
| `--output <URL>` | `RSIGMA_DETECTION_OUTPUT` | *(none)* | Downstream Vector endpoint for findings |
| `--batch-size <N>` | `RSIGMA_DETECTION_BATCH_SIZE` | `64` | Events per engine evaluation |
| `--logsource-field-map <KV>` | `RSIGMA_DETECTION_LOGSOURCE_FIELD_MAP` | *(none)* | Override which `metadata.logsource` sub-keys feed each dimension |
| `--event-logsource <KV>` | `RSIGMA_DETECTION_EVENT_LOGSOURCE` | *(none)* | Static logsource when the event's metadata carries none |
| `--no-logsource-pruning` | — | *(off)* | Disable conflict pruning (evaluate every rule) |

`<KV>` is `product=…,service=…,category=…,custom.<dim>=…`.
`RUST_LOG` controls log verbosity (default `info`), same as the rest of StrIEM.

### Where the logsource comes from

Each event's logsource is taken from **`Event.metadata["logsource"]`** — StrIEM's
convention — *not* from the log body. Vector (or an upstream normalizer) is
expected to set it, e.g.:

```json
{ "product": "windows", "service": "sysmon" }
```

The event body (`Event.data`) is what the rules match against. The two are kept
strictly separate: the logsource is shown only to the pruning extractor, never
to keyword/`|contains` matching, so a logsource value like `"windows"` can never
cause a keyword rule to fire (see `src/logsource_event.rs`).

- **Field map** (`--logsource-field-map product=os_type`): read a dimension from
  a differently-named sub-key of `metadata.logsource` (here, product ←
  `metadata.logsource.os_type`). Unset dimensions default to the standard
  `product` / `service` / `category` keys. Most deployments leave this unset.
- **Static default** (`--event-logsource product=windows`): applied when the
  event's metadata does not carry that dimension — useful when an ingest stream
  is known to be one platform.

With no logsource in metadata and no static default, the event is **fail-open**:
every rule is eligible (nothing is pruned).

## Vector configuration

Point a Vector `vector` sink at the ingest address, and a `vector` source at
wherever you route findings:

```toml
[sinks.to_detector]
type = "vector"
inputs = ["parse_logs"]
address = "http://rsigma-detection:6000"

[sources.from_detector]
type = "vector"
address = "0.0.0.0:6001"
```

## Relationship to the workspace

`rsigma-detection` is a member of the StrIEM Cargo workspace. It path-depends on
StrIEM's `lib/common` and `lib/vector`, and depends on the rsigma engine crates
(`rsigma-parser`, `rsigma-eval`, `rsigma-runtime`) as **git dependencies** pinned
to a tag, so the Docker image builds self-contained.

For local development against a live `../rsigma` checkout, the workspace-root
`.cargo/config.toml` carries a `paths` override that redirects those crates to
`../rsigma/crates/*`, so uncommitted rsigma changes are picked up immediately.
That override is excluded from the Docker build via `.dockerignore`; the image
always builds from the pinned git tag. (Cargo prints a harmless warning about
the override because the rsigma crates depend on each other — it does not affect
correctness.)

## Docker

The service ships in the shared `striem:latest` image and runs as the
`detection` service in `docker-compose.yaml`:

```sh
docker compose build      # builds striem_api, usdetect, and rsigma-detection
docker compose up
```

Because the rsigma crates are a **private** git dependency, the image build
needs Git credentials for `github.com/timescale/rsigma`. Provide them with a
BuildKit SSH mount (`docker build --ssh default …`) or a token-based credential
helper; the Dockerfile sets `CARGO_NET_GIT_FETCH_WITH_CLI=true` so the system
git handles auth.

Runtime configuration in compose uses the `RSIGMA_DETECTION_*` variables (see
the table above). Note that, unlike the legacy `usdetect` service,
`rsigma-detection` does not host the detection-admin gRPC API — rules are
managed on disk under `RSIGMA_DETECTION_RULES`, not through the api/UI proxy.
