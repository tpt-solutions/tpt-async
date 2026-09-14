# {{project-name}}

{{description}}

Built on the [tpt-async](https://github.com/tpt-solutions/tpt-async) runtime-agnostic async stack.

## Run

```sh
cargo run
```

## Profiles

The `variant` prompt picks the dependency profile:

- **desktop** — `LocalExecutor` + std timer driver + `#[tpt_async::main]`
- **embedded** — no_std timer wheel + `Sleep` (drive from your tick interrupt; see `examples/embedded`)
- **wasm** — wheel driven by `performance.now()` (see `examples/wasm`)

## Generate your own copy

```sh
cargo install cargo-generate
cargo generate tpt-solutions/tpt-async --branch main
```
