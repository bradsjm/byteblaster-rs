# Weather Wire Protocol and Runtime Specification

Version: 1.0.0
Last Updated: 2026-03-05
Status: Authoritative for `byteblaster-core` Weather Wire runtime

## 1. Purpose

This document defines the normative runtime behavior and event contract for the Weather Wire implementation in `byteblaster-core`.

## 2. Fixed Upstream Contract

Weather Wire uses fixed upstream constants, not dynamic server lists:

- Host: `nwws-oi.weather.gov`
- Port: `5222`
- Room: `nwws@conference.nwws-oi.weather.gov`

These values are compile-time constants exposed by `byteblaster-core`.

## 3. Connection and Failover Policy

Policy: single host with DNS-level IP selection.

Rules:
1. Runtime connects to `nwws-oi.weather.gov:5222` only.
2. DNS provides available IP addresses for that hostname.
3. Reconnect uses bounded exponential backoff:
4. Minimum delay: 5 seconds.
5. Maximum delay: 5 minutes.
6. Runtime emits `Connected` and `Disconnected` events around each transport session.

## 4. Message Scope and Filtering

The runtime accepts only XMPP `groupchat` messages whose `from` bare JID equals the fixed room bare JID.

All other stanza types are ignored.

## 5. Payload Decoding Rules

A message is projected to a file event only when it contains a payload element:

- element name: `x`
- namespace: `nwws-oi`

Required/used payload fields:
- `id`
- `issue`
- `ttaaii`
- `cccc`
- `awipsid`
- text body inside `x`

Semantics:
1. Missing `x` namespace payload emits `Warning::MissingNwwsNamespace`.
2. Empty payload body emits `Warning::EmptyBody`.
3. Invalid `issue` timestamp emits `Warning::TimestampParseFallback` and uses current UTC fallback.
4. Optional `delay` payload (`urn:xmpp:delay`) is parsed when present.
5. File body is converted to NOAAPort framing (`SOH` prefix, normalized line endings, `ETX` suffix).

## 6. Event Model

Weather Wire emits full-file events only. It does not emit chunk/segment data events.

- `WxWireClientEvent::Frame(WeatherWireFrameEvent::File(_))`
- `WxWireClientEvent::Frame(WeatherWireFrameEvent::Warning(_))`
- `WxWireClientEvent::Connected(String)`
- `WxWireClientEvent::Disconnected`
- `WxWireClientEvent::Telemetry(WxWireTelemetrySnapshot)`

## 7. Backpressure and Handler Isolation

1. Event queue is bounded.
2. If queue is full, events are dropped and accounted.
3. Runtime emits `Warning::BackpressureDrop` with window and total counters.
4. Handler failures are isolated and converted to `Warning::HandlerError`.

## 8. Idle Timeout

If no accepted room message is received for `idle_timeout_secs`, runtime:
1. emits `Warning::IdleTimeoutReconnect`
2. disconnects current transport
3. enters reconnect cycle with bounded backoff

## 9. Unstable Ingress Surface

Raw stanza injection is unstable-only API:

- `byteblaster_core::unstable::UnstableWxWireIngress`

No stability guarantees apply to this surface.

## 10. Test Mapping

- Decoder behavior and warning projection: `weather_wire::codec::tests::*`
- Config validation: `weather_wire::config::tests::*`
- Runtime event/handler/unstable ingress behavior: `weather_wire::client::tests::*`
