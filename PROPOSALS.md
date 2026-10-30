# Proposals: messaging

Written by loopers. Each proposal: what, why, links. Jody promotes accepted ones into ROADMAP.md (via an agent);
the looper then marks it `promoted` or `declined` here with the date.

## Open
- **One atomic `Apply` per SendMessage** (2026-10-29, #7): once the deliveries are written through Stratum's
  write surface, write the N deliveries of one submission as one `Apply` of `InsertIfAbsentMessage` ops in canonical
  order (sorted by `delivery_id`). The fan-out is then all or nothing, and the "partial fan-out, cannot be recalled"
  answer that `send_message` gives today goes away.
- **Message.spiffe_id has no stamper off the bus** (2026-10-29, #7, tacplus-dev/tacplus#207): eventbus stamped
  `(spec.identity_spiffe_subject)` / `(spec.identity_spiffe_actor)` on publish. Nothing stamps them on a write-surface
  write, so the record's `owner` would be stored empty. Messaging must not fill the field in itself (messaging.proto
  says why), so the store's generated wrapper should stamp it.
- **<title>** (<date>, <issue/run link>): <what and why>
- **ROADMAP.md: the floor is 2.5, not 2.4** (2026-10-29, #8): ROADMAP.md's "Now" still says "tacplus floor 2.4"; #8 moved `tacplus.yaml` to 2.5 because stratum, a `depends`, now needs 2.5. ROADMAP.md changes only through an agent at Jody's request, so this asks for that edit.

## Promoted / declined
