# cards2pack-core

`cards2pack-core` is a pure-Rust library that converts Adaptive Card JSON sequences into
[YGTc 2.0](https://docs.greentic.ai) flow YAML for use in Greentic bundles.

It is deliberately free of async runtimes, filesystem I/O, and subprocess calls so that
it can be compiled to `wasm32-wasip2` without modifications.

## Features

- Parses card sequences (AdaptiveCard + HTTP node entries) from JSON
- Builds a routing graph that respects `data.nextCardId` (preferred) or
  `data.routeToCardId` (legacy fallback) and preserves back-edges
- Emits deterministic YGTc 2.0 YAML with `when:` conditional routes
- Fixes four bugs present in the legacy `greentic-cards2pack` v0.4 converter:
  - Alphabetical-ordering instead of route-key-driven routing
  - `demo_wrapup` incorrectly chosen as flow start
  - Route-key field silently ignored in some card shapes
  - Schema bloat duplicating flat fields outside `card.call.payload`

## Usage

```rust
use cards2pack_core::{parse_cards, convert, ConvertOptions};

let cards = parse_cards(json_str)?;
let result = convert(&cards, &ConvertOptions {
    flow_name: "my-flow".into(),
    strict: false,
})?;
println!("{}", result.flow_yaml);
```
