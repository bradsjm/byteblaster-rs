# Relay Mode (`byteblaster-relay`)

`byteblaster-relay` is a dedicated ByteBlaster TCP retransmission process.
It receives the upstream feed and forwards the raw wire bytes to downstream
ByteBlaster clients with low added latency.

## Behavior Contract

- Passthrough only: no payload filtering and no frame transformation.
- Downstream clients must authenticate on connect and re-authenticate every 12 minutes.
- Downstream authentication is local to relay tracking and is not forwarded upstream.
- Upstream server-list updates are forwarded as-is to downstream clients.
- If downstream active connections exceed limit, relay sends a server-list frame and disconnects the new client.
- Each downstream client has a 64 KiB queue budget. If exceeded, that client is disconnected.
- Relay maintains a quality window (`forwarded_bytes / attempted_bytes`).
  - Pause forwarding when quality drops below `0.95`.
  - Resume forwarding when quality recovers to `>= 0.97`.

## Default Runtime Values

- Relay bind: `0.0.0.0:2211`
- Max downstream clients: `100`
- Downstream auth timeout: `720` seconds
- Per-client queue budget: `65536` bytes
- Metrics bind: `127.0.0.1:9090`
- Quality window: `60` seconds

## Metrics Endpoint

`GET /metrics` returns a JSON snapshot with:

- Upstream connectivity totals (attempts/success/fail/disconnect)
- Downstream connection totals (accepted/rejected)
- Disconnect reason counters (auth timeout/slow client/lag)
- Byte counters (`bytes_in`, `bytes_attempted`, `bytes_forwarded`, `bytes_dropped`)
- Quality state (`rolling_quality`, `forwarding_paused`, pause event count)
- Active authenticated users (`email`, peer, connected/auth timestamps)

## Health Endpoint

`GET /health` returns a compact JSON health snapshot with:

- `status` (`"ok"`)
- `forwarding_paused` (bool)
- `downstream_active_clients` (current count)
