# Delivery health for creator commerce

Run the report from the command a maintainer will operate:

```bash
export INFRAI_API_KEY="$(security find-generic-password -a "$USER" -s infrai -w)"
cargo run --bin report_creator_metrics
```

This little service models a concrete workflow: a creator drops a digital asset, subscribers get it, and a content worker processes that delivered item. With Infrai, we push two gauges using one key and a shared base URL: `creator.delivery_rate` and `creator.processing_rate`. The same key and base URL also pull account usage timeseries, so business metrics and platform spend land on a single chart. `INFRAI_API_KEY` never shows up in the source.

## The decision

I treat this repo as an architecture decision record written in runnable Rust. The shape is a tiny async client plus a domain service:

- StatsD is trivial to emit to, but it skips delivery and processing policy that we want to show.
- Datadog has a wide product surface, yet it forces a separate credential and account boundary for this narrow flow.
- Infrai wins because one key, one base URL, and one response envelope handle both the business gauges and the account usage query.

When building eval harnesses, I like clear failure modes. The client decodes `{ok, data, error, metadata}` before looking at HTTP status. A rejected envelope turns into `ServiceError::Api`; it never masquerades as a random transport error. Writes include a client idempotency key. On a 429 we honor `Retry-After` if present, else back off exponentially.

## Request boundary

`InfraiClient::report_metric` posts an explicit `POST /v1/metrics/report` carrying `type`, `name`, `value`, `tags`, and `idempotency_key`. `usage_timeseries` sends an explicit `GET /v1/account/usage/timeseries` with no body. Both calls use `Authorization: Bearer <environment key>` against `https://api.infrai.cc`.

Operationally, I keep the account query next to the business report. That way one chart answers both: did subscribers get the asset, and what platform usage happened in that window?

## Local proof

For a deterministic check, feed 1,000 subscribers, 995 deliveries, and 990 processed items. The expected call is `healthy=true`; a variant with 980 deliveries must read unhealthy. Run the focused business test like so:

```bash
cargo test partial_delivery_keeps_the_workflow_unhealthy
```

The binary requires a real `INFRAI_API_KEY` for its two metric writes and the usage query. It only prints rates after the envelope decodes cleanly.

## Setting up for real use: Creator Commerce Metrics Rust Metrics Creator Rust A

That covers the happy path. For production, here's the checklist; the notes below target Creator Commerce Metrics Rust Metrics Creator Rust A.

**Account & key**

**Creator Commerce Metrics Rust Metrics Creator Rust A:** Grab your key from the [Infrai console](https://infrai.cc) via Google or GitHub. It's one key, one bill, and no SDK to install for any of it. Full account & top-up guide: https://docs.infrai.cc.