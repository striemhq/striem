# StrIEM Helm chart

Deploys StrIEM on Kubernetes: the API/UI, the rsigma detection engine, and
Vector, with an optional ClickHouse destination.

## Architecture

The chart mirrors the `docker-compose.yaml` topology as four Deployments:

```
vector --(OCSF, grpc)--> detection --(findings, grpc)--> vector --> storage
vector <--(config, http)-- api
```

- **api** — HTTP/UI and Vector config generator. Serves the config that Vector
  loads over HTTP. One image is shared with detection (`image.repository`).
- **detection** — `rsigma-detection`. Receives OCSF events on gRPC, runs Sigma
  rules, emits findings back to Vector. Rules live on a persistent volume.
- **vector** — the fork with parquet codecs and env interpolation. An init
  container clones the OCSF parquet schemas and remap VRL into the pod; only
  Vector reads those files, so they are not a shared volume. The api emits the
  `${STRIEM_SCHEMA_DIR}` / `${STRIEM_REMAPS}` placeholders and Vector
  interpolates them.
- **clickhouse** *(optional, `clickhouse.enabled`)* — a query destination.

The only volume shared between pods is `storage` (parquet): Vector writes it and
the api reads it. With the default `ReadWriteOnce` access mode the api and
vector pods schedule onto the same node; set `storage.persistence.accessMode`
to `ReadWriteMany` (with a suitable storage class) to spread them across nodes.

## Install

```bash
# Build and load/push the striem image first (repository: striem, tag: latest),
# then:
helm install striem deploy/helm/striem
```

## Common values

| Key | Default | Description |
| --- | --- | --- |
| `image.repository` / `image.tag` | `striem` / `latest` | Shared api+detection image |
| `api.service.type` | `ClusterIP` | Set `LoadBalancer` to expose the UI |
| `storage.persistence.size` | `20Gi` | Shared parquet volume |
| `storage.persistence.accessMode` | `ReadWriteOnce` | Use `ReadWriteMany` for multi-node |
| `vector.ingest.type` | `ClusterIP` | Set `LoadBalancer`/`NodePort` to accept external logs |
| `vector.setup.schemaRepo` / `remapRepo` | OCSF repos | Cloned by the init container |
| `ingress.enabled` | `false` | Expose the UI through an Ingress |
| `clickhouse.enabled` | `false` | Deploy the optional ClickHouse |
| `otel.endpoint` | `""` | OTLP endpoint for traces (metrics are always on) |

See [values.yaml](values.yaml) for the full list.
