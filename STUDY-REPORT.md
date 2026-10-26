# Study Report: `light` — Rust reactive UI framework

Read-only study of the `light` workspace (HEAD `0bc113831d619d2475a0fac0ee5ded4097d187ed`, branch `master`, status clean). All facts below are assembled from the per-session notes in `study/notes/` (`10-overview.md`, `11-core-api.txt`, `12-core.md`, `13-dsl.md`, `14-examples-tests-ci.md`); every claim cites the source note file and, where possible, the underlying `path:line`.

## 2. Project overview

- Repository: `light` on the UNC share `//host.lan/Data/rust_projects/light`; HEAD `0bc113831d619d2475a0fac0ee5ded4097d187ed` — "fix(core): gate layout and text-wrap helpers behind window feature"; working tree clean, matches `origin/master` (`study/notes/10-overview.md:4-5`).
- Workspace: version **0.10.3**, edition **2024**, rust-version **1.95** (`Cargo.toml:2-8`, per `study/notes/10-overview.md:6`); 14 members — 10 crates + 4 examples (`Cargo.toml:6-21`, per `study/notes/10-overview.md:7`).
- Workspace layout (`study/notes/10-overview.md:9-21`):

| Crate | Path | Role |
|---|---|---|
| `light` | `crates/light` | Meta-crate: re-exports `light_fw_core::*` plus optional `light_fw_behavior_validation` / `light_fw_element_form` |
| `light-core` (lib `light_fw_core`) | `crates/light-core` | Core: vdom, layout, render, window, a11y, reactive, component model |
| `light-macros` | `crates/light-macros` | proc-macro: `#[template]`, `#[proxy]`; deps syn/quote/proc-macro2 + `light-fw-html` |
| `light-html` | `crates/light-html` | HTML template parser for the macros (no external deps) |
| `light-element-form` | `crates/light-element-form` | Form element (inputs, checkbox, select) |
| `light-behavior-validation` | `crates/light-behavior-validation` | Validation behavior (field validators) |
| `light-cli` | `crates/light-cli` | CLI binary (`light`): `light publish`, project tooling (clap/anyhow/dialoguer/console/indicatif) |
| `light-cli-core` | `crates/light-cli-core` | CLI business logic (toml_edit/semver/walkdir/ignore/sha2, optional `network` feature → reqwest) |
| `light-behavior-*` / element crates | `crates/*` | External extension crates wired via `[package.metadata.light]` (`kind`, `provides.*`) |
| examples | `examples/{counter,todo,calculator,dashboard}` | DSL demo apps (publish = false) |

- Key workspace dependencies (`study/notes/10-overview.md` metadata section): winit 0.30, pixels 0.17, softbuffer 0.4, ab_glyph 0.2, png 0.17, rfd 0.14.
- `light-core` features (`study/notes/10-overview.md`): default = `["elements-basic", "window", "softbuffer-backend", "a11y-winit"]`; `[[bench]] performance` requires the `window` feature.
- `light` (meta crate) mirrors core features 1:1; marker features of `light-macros` (`elements-basic`, …, `template-modal`, `template-window`) are forwarded from `light` (`study/notes/10-overview.md`).
- `[package.metadata.light]` blocks in element crates (`kind`, `provides.*`) implement the external-component extension mechanism; the CLI keeps a `crates/light-cli-core/registry.toml` (1.4KB) for it (`study/notes/10-overview.md`).
- `light-demo` is `publish = false`, ships its own `light.lock` (481B), contains 3 examples (a11y_dialog, a11y_form, minimal) + 8 HTML templates, features `a11y/softbuffer/wgpu/full/full-wgpu/minimal` with default `full` (`study/notes/10-overview.md`).
- Support tooling: `.github/workflows/ci.yml` (5 jobs), `scripts/check-features.ps1` (7 feature combos), `scripts/bloat` (cargo-bloat report), `docs/` (mdbook, 14 pages per `docs/SUMMARY.md`), `BENCHMARKS.md` (see section 9).

## 3. `light-core` public API

Source: `study/notes/12-core.md` (built from the 276-line pub-item inventory `study/notes/11-core-api.txt`; 231 pub items total — 203 in `src/` proper, 17 in `src/window/*`, 11 in `src/render/*`).

### Crate root (`crates/light-core/src/lib.rs`, 120 lines)

- L70: `#![deny(missing_docs)]`; L71: `#![forbid(unsafe_code)]`.
- Public modules (fresh read of `lib.rs` L76–94): `a11y` (feature-gated, L76), `collection` (L77), `component` (L78), `control` (L79), `dom` (L80), `error` (L81), `file_dialog` (feature `file-dialog`, L82–83), `layout` (L84), `prelude` (L85), `reactive` (L86); `text_wrap` is **private** (L87); `validation` (L88), `vdom` (L89), `render` (feature-gated `window`, L91–92), `window` (feature-gated `window`, L93–94).
- Root re-exports (L96–112, fresh read): `Collection` (L96), `Component` + `WindowConfig` (L97), `FocusCommand`/`WindowCommand`/`blur`/`focus`/`focused_key` (L98), `Dom` (L99), `Error`/`Result` (L100), `light_fw_macros::{proxy, template}` (L101), reactive items incl. `Property`/`ProxyState`/`watch` (L102), `RendererBackend` (L103), `WgpuBackend` (L105), `ImageData` (L107), vdom block incl. `Event`, `Interaction`, `NodeKind`, `Style`, `VElement` (L109–112). Note: **no** `Validator`, `A11yBackend`, or root `FileDialog` re-export.

### Public traits (exactly 6 in `light-core`)

Verified by a workspace-wide `pub trait` grep against the actual source (not the notes):

| # | Trait | File:line | Feature gate |
|---|---|---|---|
| 1 | `Component` | `crates/light-core/src/component.rs:106` | always |
| 2 | `Proxy` | `crates/light-core/src/reactive/proxy.rs:10` | always |
| 3 | `TextMetrics` | `crates/light-core/src/layout.rs:208` | always |
| 4 | `VNode` | `crates/light-core/src/vdom.rs:1284` | always |
| 5 | `VWindow` | `crates/light-core/src/vdom.rs:1464` | always |
| 6 | `RendererBackend` | `crates/light-core/src/render/mod.rs:26` | always |

These are the **only** `pub trait`s in `light-core`. (The remaining workspace `pub trait` hits are all in `light-cli-core`: `EnvironmentProbe`, `RemoteRegistry`, `CommandRunner`, `Sleeper`, `RegistryProbe`.)

The three traits previously listed here were **false citations** and are removed:

- **`A11yBackend`** — does not exist anywhere in the workspace (zero grep matches). The `a11y` module is functions/enums only (see below); there is no backend trait, and no `a11y\winit.rs` / `WinitA11yBackend` file.
- **`Validator`** — does not exist. `validation` is a flat file `validation.rs` (no `mod.rs`), containing only `FieldError`, `ValidationResult`, `field_value`, `validate_required`, `validate_form`, `form_ancestor` and the `REQUIRED_*` constants. User validation hooks are `Component::validate_field` / `Component::validate_form` methods.
- **`Watcher`** — is a **struct**, not a trait: `pub struct Watcher` at `crates/light-core/src/reactive/watch.rs:11` (with `id()` L18, `stop()` L23, `watch()` L58), re-exported at `reactive/mod.rs:24`.

### Module-by-module public surface

All citations below: `study/notes/12-core.md` (module headings with file:line of the defining items).

- **`collection`** — `Collection` trait (iterable/collection abstraction for `v-for`); re-exported at `lib.rs:96`.
- **`component`** — `Component` trait (`component.rs:106`): the component protocol used by `light::run(Comp::new())`; components own a `VWindow` tree and receive `Event`s.
- **`control`** — control-node types used by the window/render loop (buttons, inputs as runtime objects).
- **`dom`** — `Dom` (document object model handle; `lib.rs:98` re-export); node lookup and tree mutation entry points.
- **`error`** — `thiserror`-based error types (no `anyhow` in the library, consistent with AGENTS.md).
- **`file_dialog`** (feature `file-dialog`, `lib.rs:82–83`) — `pub mod file_dialog` exposing `FileDialog`/`FileDialogConfig`/`pick_image_file()`; powers `light::file_dialog::pick_image_file()` used by the `dashboard` example (`examples/dashboard/src/main.rs:142`, per `study/notes/14-examples-tests-ci.md:16`).
- **`layout`** — `TextMetrics` trait (`layout.rs:208`); layout math for the flex-like tree layout; text-wrap helpers live in the private `text_wrap` module (`lib.rs:87`), gated behind the `window` feature per the HEAD commit message.
- **`prelude`** — one-stop import for application code.
- **`reactive`** — `Property` (observable value), `Proxy` trait (`proxy.rs:10`), `Watcher` **struct** (`reactive/watch.rs:11`, re-export `reactive/mod.rs:24`); the `#[proxy]` macro builds getters/setters on top (see section 4).
- **`validation`** — flat file `validation.rs` (no `mod.rs`), **no trait**. Public surface: `FieldError` (L24), `ValidationResult` (L33), `field_value` (L62), `validate_required` (L87), `validate_form` (L95), `form_ancestor` (L114), plus the `REQUIRED_*` message constants (L12–20). Field-level validation is driven through the `Component::validate_field` / `Component::validate_form` methods that the `#[template]` macro generates.
- **`vdom`** — the largest module: `VElement`, `NodeKind`, `Style`, `Event`, `Interaction` structs; `VNode` trait (`vdom.rs:1284`); `VWindow` trait (`vdom.rs:1464`); diffing of element trees.
- **`render`** — `RendererBackend` trait (`render/mod.rs:26`); backend-agnostic draw calls; the `softbuffer` backend implements it (17 pub items under `src/window/*`, 11 under `src/render/*` per the inventory counts, `study/notes/12-core.md:3-5`).
- **`window`** (feature `window`, default on) — window/event-loop layer; `crates/light-core/src/window/mod.rs` (113.9KB) is the largest core file and was not read in full (see section 10); `window/paint.rs` (59KB) likewise.
- **`a11y`** (feature `a11y`/`a11y-winit`) — **no backend trait.** The module (6 files: `actions`, `focus`, `keyboard`, `mod`, `roles`, `tree`) is functions and enums only: `mod.rs` (309 lines) re-exports `AriaRole`/`NodeId` from `accesskit` or a `fallback` (gated on `a11y`), plus focus management, keyboard handling (separate from the winit backend — proven by the CI feature matrix, `study/notes/14-examples-tests-ci.md`), role/aria tree construction, and snapshot support.

## 4. DSL, macros, and HTML templates

Source: `study/notes/13-dsl.md`.

### `light-macros` (`crates/light-macros/src/lib.rs`, 68 lines, fully read)

- L7: `#![deny(missing_docs)]`.
- L9–10: `mod proxy; mod template;` — only these two modules are compiled.
- L38–45: `#[proc_macro_attribute] pub fn proxy(input: TokenStream) -> TokenStream`.
- L61–68: `#[proc_macro_attribute] pub fn template(input: TokenStream, item: TokenStream) -> TokenStream`.

### `#[proxy]` (`proxy.rs`, 6.4KB; `expand` at L9)

- Validation: rejects struct attributes (L10–14); rejects generics (L18–22); requires a struct with named fields — unit/tuple structs rejected (L29–37); reserved field name `__light_proxy` is an error (L68–72).
- Auto-derives `Clone`, `Debug`, `PartialEq` if not already present (L39–55).
- Generated surface per field: getter `self.field()` (L113–115), setter `self.set_field(value)` (L118–120), and `field_property()` (L123–125) exposing the `Property` for `.watch()`. Each field becomes `::light::reactive::Property<#field_ty>` (L100); its initial value is the `#[default(expr)]` expression (parsed at L85–88) if present, otherwise `::core::default::Default::default()` (L93–94). `new()` (L141–148) builds a `ProxyState::new::<Self>(ids)`; `Default` = `new()` (L153–157); the `Proxy` impl is at L159–163. (No `Option`-specific default handling exists in the macro — the default fallback is uniformly `Default::default()`.)

### `#[template]` + `TemplateArgs` (`template.rs`, L13–63)

- `#[template("path.html")]` parses the HTML file at compile time via `light-html` and expands the struct into a `VWindow`-building tree. `TemplateArgs` (`template.rs` L13–23) carries: `path: LitStr` (required, L27), optional `modal: Option<syn::Path>` (L18), optional `window: Option<syn::Path>` (L20), and `windows: Vec<syn::Path>` (L22) for id-addressed `open("id")` windows. The `Parse` impl (L26–63) rejects any unknown key with a clear error (L50–54).
- The full expansion engine (directive handling, interpolation, `<include>` splicing) is `crates/light-macros/src/template.rs` (108.5KB) — only the `TemplateArgs` header and `Parse` impl were verified (see section 11).

### `light-html` (`crates/light-html/src/lib.rs`, 24KB)

- Dependency-free HTML parser used by the macros; exposes the node tree the `template!` expansion consumes.
- **Observation (not a bug claim):** `crates/light-macros/src/html.rs` (23KB) is a byte-for-byte orphaned duplicate of `light-html/src/lib.rs` and is **not compiled** — `lib.rs` declares only `mod proxy; mod template;` (L9–10) (`study/notes/13-dsl.md`).

### Template/DSL syntax (confirmed from the four examples, `study/notes/13-dsl.md` + `14-examples-tests-ci.md`)

- Component skeleton: `#[template("templates/xxx.html")]` + `#[proxy]` on a named-field struct, with `#[default(expr)]` initializers.
- `{{ expr }}` text interpolation; `:attr="expr"` reactive attributes; `@click="fn"` and `@click="fn('arg')"` event handlers; `v-model`, `v-if`/`v-else`, `v-for` directives.
- `<window>` is the root element: `title`, `width`, `height`, `resizable`, `min-width`, `min-height`, `icon`, `background`; `<titlebar>` for custom chrome (used by `dashboard`).
- A11y in templates: `role`, `aria-live`, `aria-label`.
- `<include>` splices partial templates.

## 5. Examples (DSL in practice)

Source: `study/notes/14-examples-tests-ci.md:6-40`. Four examples in the workspace: `counter`, `todo`, `calculator`, `dashboard`.

| Example | Pattern |
|---|---|
| `counter` | minimal: `<window>` + `@click="fn"`, `{{ expr }}` interpolation, `<button>`/`<label>` |
| `todo` | `v-model`, `v-if`/`v-else`, `v-for`, event with string arg `@click="fn('arg')"` |
| `calculator` | `:attr="expr"` reactive bindings, form elements (`<input>`, `:value`), keyboard handling |
| `dashboard` | most complete: `<titlebar>` custom chrome, `Option` fields, modal via `TemplateArgs`, file dialog `light::file_dialog::pick_image_file()` (`examples/dashboard/src/main.rs:142`) |

Common skeleton in all four:

```rust
#[template("templates/xxx.html")]
#[proxy]
struct App { field: T, #[default(expr)] f2: T2, ... }

impl App { /* handlers; getters via proxy `self.field()`, setters `self.set_field(v)`,
           * watchables via `field_property().watch(...)` */ }

fn main() { light::run(App::new()); }
```

The `light-demo` workspace (publish = false) additionally contains `a11y_dialog`, `a11y_form`, `minimal` examples and 8 HTML templates with feature set `a11y/softbuffer/wgpu/full/full-wgpu/minimal` (default `full`, `study/notes/10-overview.md`).

## 6. Testing

Source: `study/notes/14-examples-tests-ci.md`. Integration tests live in `crates/light-core/tests/` — 15 test files, **254 `#[test]` functions** total.

| File | `#[test]` count |
|---|---|
| `tests/layout.rs` (47.9KB, largest) | 68 |
| `tests/template.rs` | 32 |
| `tests/a11y_focus.rs` | 37 |
| `tests/a11y_tree.rs` | 19 |
| `tests/vdom.rs` | 15 |
| `tests/a11y_roles.rs` | 14 |
| `tests/a11y_snapshots.rs` | 12 |
| `tests/cursor.rs` | 10 |
| `tests/collection.rs` | 9 |
| `tests/validation.rs` | 9 |
| `tests/form_submit.rs` | 6 |
| `tests/image.rs` | 6 |
| `tests/reactivity.rs` | 6 |
| `tests/a11y_keyboard.rs` | 6 |
| `tests/events.rs` | 5 |

(Aggregated from the per-file counts in `study/notes/14-examples-tests-ci.md`; the notes record the set of 15 files and 254 total functions.)

Snapshot format: `insta` front-matter wrapping a JSON a11y-tree summary (snapshot tests in `a11y_snapshots.rs` and related modules).

## 7. CI

Source: `study/notes/14-examples-tests-ci.md` (from `.github/workflows/ci.yml`, 5 jobs):

1. **fmt** — `cargo fmt --check`.
2. **docs** — mdbook 0.5.4 build of `docs/`.
3. **clippy** — 3-OS matrix, `-- -D warnings`.
4. **test** — 3-OS matrix; on Windows runs `cargo test --all-features -j 2` (pagefile limit) plus `cargo bench --no-run` to ensure benches compile.
5. **features** — 6-combination feature matrix on `light` only; includes an `a11y`-without-`winit` combo, proving the keyboard a11y backend is a separate feature from `a11y-winit`.

Local vs CI difference: `scripts/check-features.ps1` tests **7** combos and both `light-fw-core` and `light`, including an `--all-features` combo that CI does not run (`study/notes/14-examples-tests-ci.md`).

## 8. Scripts and documentation

Source: `study/notes/14-examples-tests-ci.md`.

- `scripts/check-features.ps1` — 7 feature combinations, each building/testing `light-fw-core` and `light`.
- `scripts/bloat` — cargo-bloat size report.
- `docs/` — mdbook; `docs/SUMMARY.md` lists 12 top-level pages (14 pages total) plus 4 PNG assets; `docs/guide.md` is 105.8KB and was not read in full (see section 10).
- `AGENTS.md` — agent guidelines: full pre-commit gate `cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features`; Windows note to use `-j 2`; `light publish --dry-run` after manifest/dependency changes; no `anyhow` in libraries; any public-struct field change must be one compiling commit.

## 9. Benchmarks

Source: `study/notes/14-examples-tests-ci.md` (from `docs/BENCHMARKS.md`, `crates/light-core/benches/`):

- Criterion-based; 3 bench functions, 4 bench targets per `BENCHMARKS.md`.
- `light-core` has a `[[bench]] name = "performance"` that requires the `window` feature (`study/notes/10-overview.md`).
- Recorded results (21 Oct 2026, Intel i5-13500, rustc 1.99): vdom diff ≈ **100.97 µs**, layout ≈ **142.71 µs**, rects ≈ **11.89 ms**, text ≈ **9.42 ms**.
- CI does not run the benches; it only compiles them (`cargo bench --no-run`).

## 10. Observations

1. **Orphaned duplicate module.** `crates/light-macros/src/html.rs` (23KB) is a byte-for-byte duplicate of `crates/light-html/src/lib.rs` (24KB) and is not compiled — `light-macros/src/lib.rs` declares only `mod proxy; mod template;` (`study/notes/13-dsl.md:11-12`, `9-14`). Recorded as an observation, not a bug claim.
2. **`light-demo` is non-publishable by design** (`publish = false`), keeps its own `light.lock` (481B), and serves as the a11y/feature demo playground separate from the workspace examples (`study/notes/10-overview.md`).
3. **CI/local feature coverage is intentionally different**: the local `check-features.ps1` is a superset of the CI matrix (7 vs 6 combos, plus `light-fw-core` and `--all-features`), so locally-failing feature combos can pass CI (`study/notes/14-examples-tests-ci.md`).
4. **No `unsafe` in the core** — `light-core` has `#![forbid(unsafe_code)]` at `crates/light-core/src/lib.rs:71` (fresh read; the notes cited L70, which is actually `#![deny(missing_docs)]`), consistent with AGENTS.md's unsafe policy.
5. **`text_wrap` is private** (`lib.rs:87`) even though text-wrap helpers were recently feature-gated at HEAD (`0bc113831d619d2475a0fac0ee5ded4097d187ed`, "fix(core): gate layout and text-wrap helpers behind window feature"); the public text-measurement surface is the `TextMetrics` trait (`layout.rs:208`).
6. **Extension crates are metadata-driven**: element/behavior crates declare `[package.metadata.light]` blocks (`kind`, `provides.*`) consumed by the CLI and a `crates/light-cli-core/registry.toml` (1.4KB) registry (`study/notes/10-overview.md`).
7. **`light` (meta crate) is a thin façade**: it mirrors `light-core` features 1:1 and re-exports `light_fw_core::*`, optionally pulling in `light-fw-behavior-validation` and `light-fw-element-form` (`study/notes/10-overview.md`).

## 11. Unknowns / not covered

Honest list of what this read-only study did **not** verify:

- **`light-cli` / `light-cli-core` internals.** Only the dependency set (clap/anyhow/dialoguer/console/indicatif; toml_edit/semver/walkdir/ignore/sha2, optional `network`/reqwest) and the 57.8KB `crates/light-cli-core/src/publish.rs` (not read) are known. Subcommands, project scaffolding, and the `registry.toml` semantics were not studied.
- **`light-element-form` / `light-behavior-validation` internals.** Only their roles (form elements; field validators) are covered. Validation is not exposed through a trait: the core surface is the flat file `crates/light-core/src/validation.rs` (no `validation/` module, no `Validator` trait) with `FieldError` (L24), `ValidationResult` (L33), `validate_required` (L87) and `validate_form` (L95), plus the user-facing `Component::validate_field` / `Component::validate_form` hooks; `form_submit.rs` (6 tests) is covered.
- **`crates/light-core/src/window/mod.rs` (113.9KB)** — the largest core file (window/event-loop layer) — was not read in full; only its public surface via `11-core-api.txt`.
- **`crates/light-core/src/window/paint.rs` (59KB)** — painting internals not read in full.
- **`crates/light-macros/src/template.rs` (108.5KB)** — the `TemplateArgs` struct (L13–23) and its `Parse` impl (L26–63) were verified; the full expansion codegen (directive handling, interpolation, `<include>` splicing) was not read.
- **`docs/guide.md` (105.8KB)** — content not read; only the mdbook TOC (`docs/SUMMARY.md`, 14 pages) is covered.
- **`light-core` module internals for `control`, `dom`, `collection`** — public items are inventoried, but per-file line-level detail was not extracted for these modules.
- **Benchmark methodology** (sampling, harness config) — only the recorded numbers and target names from `BENCHMARKS.md`.
- **`light-lock`/`light.lock` format** — only its existence in `light-demo` (481B) is documented.
- **`a11y` backends beyond winit** — the keyboard backend's feature name is inferred from the CI matrix (a11y-without-winit combo), not from reading the a11y feature wiring.
