# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`air_handler` is an egui/eframe app (Rust 2024 edition, toolchain pinned to 1.99.0 via `rust-toolchain`) that builds natively and to WASM. Goal: design and calculate different types of air handler units (AHUs). The user drags air-treatment components (heater, cooler, humidifier, fan, ...) from a library onto a duct, and a properties widget edits the selected component. Calculations are live. It was bootstrapped from emilk's `eframe_template`; only the `main.rs` entry points and build setup remain from it. UI labels are English.

## Commands

```sh
cargo run --release                     # native app
trunk serve                             # web build at http://127.0.0.1:8080/index.html#dev
trunk build --release                   # static site in dist/ (deployed to GitHub Pages by .github/workflows/pages.yml)
./check.sh                              # full local CI: check (native + wasm32), fmt, clippy -D warnings, tests, trunk build
cargo clippy --all-targets --all-features -- -D warnings
cargo test <test_name>                  # run a single test, e.g. cargo test heater_reaches_setpoint (unit tests live in src/model)
```

During web development, use the `#dev` URL suffix. Without it, `assets/sw.js` caches the app and serves stale builds.

## Architecture

Two layers. `model` has no egui dependency, `ui` only reads and edits the model.

- `src/model/` is the domain and calculation layer, covered by unit tests.
  - `air.rs`: `AirState` (temperature, humidity ratio, volume flow, static gauge pressure) built on the vendored `psychrolib/` crate (ASHRAE 2017, path dependency, a copy of the Rust port of PsychroLib). Humidity ratio is the source of truth, and relative humidity is derived. Dry-air mass flow is conserved along a duct: `Component::apply` and `HeatRecovery::exchange` rescale the volume flow with `AirState::with_mass_flow`. `PressureDrop` is the general pressure loss component (filter, damper, ...), and fans raise the pressure.
  - `chart.rs`: background curves of the psychrometric chart (relative humidity, isotherms, enthalpy lines) as `(temp, humidity ratio)` points, all computed with psychrolib. No egui here.
  - `component.rs`: `ComponentKind` (the fieldless identity used by the library and as the drag payload) and `Component` (the kind plus its parameters). `Component::apply(inlet) -> (outlet, Duty)` is where each component's physics lives. **To add a component type, add a variant to both enums, then handle it in `apply`, `duty_summary`, `ui::kind_color` and `ui::properties::component_params`.**
  - `recovery.rs`: `HeatRecoveryKind` / `HeatRecovery` (run-around coil, rotary wheel, plate exchanger). `HeatRecovery::exchange(supply_in, extract_in)` uses one shared model: temperature efficiency defined on the supply side, heat limited by the weaker stream, extract condenses below its dew point. Only the wheel transfers moisture. Types differ in parameters, so add new ones as variants.
  - `unit.rs`: `AirHandlerUnit` has two `Duct`s (`DuctId::Supply` = outdoor air to supply air, `Extract` = extract air to exhaust air), each with an ordered `Vec<Placed>`, plus an optional `PlacedRecovery`. Components have a stable `ComponentId`, and selection refers to that id, not to a position. The heat recovery is **not** in either duct's list. It stores how many components sit before it in each duct, and `add`/`remove` keep those counts in sync. `AirHandlerUnit::simulate()` runs each duct up to the recovery, exchanges there, then continues each duct, and returns a `Simulation` with a `Stage` per component. The UI recomputes it every frame, with no cached results.
- `src/ui/` is the widgets, as plain functions taking the model and `&mut Option<Selection>`.
  - `library.rs`: a palette of drag sources using egui's built-in DnD (`Ui::dnd_drag_source`). The payload is a `ComponentKind` or a `HeatRecoveryKind`.
  - `diagram.rs`: draws each duct and acts as the drop target (`Response::dnd_release_payload`). Both rows share one column grid (`Layout`) so the heat recovery lines up as one tall box across them. It works out the insertion index and the side of the recovery from the pointer x position and returns it, and the unit is mutated after drawing.
  - `chart.rs`: `ui::chart` draws the psychrometric chart or the Mollier h-x diagram (`ChartKind`, persisted in `AirHandlerApp`) with `egui_plot`, in a bottom panel. It projects the model's `(temp, humidity ratio)` curves and the duct state paths (inlet, each component outlet, heat recovery outlet) onto either axis set.
  - `properties.rs`: edits the selected `Selection::Component`, `Selection::Duct` (inlet conditions) or `Selection::Recovery` and shows the computed outlet state and duty.
- `src/app.rs`: `AirHandlerApp` owns the `AirHandlerUnit` and the (non-persisted) selection, and lays out the panels: library on the left, properties on the right, diagram in the centre. The unit is serialized with serde through eframe's `persistence`, so **changing `Default` values has no visible effect while old state is stored.** Use `#[serde(default)]` for new fields, or clear the stored app state.
- `src/main.rs` has two `main` functions behind `cfg(target_arch = "wasm32")`. The native one uses `eframe::run_native`. The web one mounts onto `<canvas id="the_canvas_id">` in `index.html`.
- New modules must be declared in `src/lib.rs`.

Not built yet: moving or reordering placed components by drag, more component types (filter, damper, ...), frost protection for the heat recovery, and showing the extract duct in its physical flow direction (both rows are drawn left to right).

## Lints

`Cargo.toml` turns on `clippy::pedantic` plus a long list of extra lints (for example `unwrap_used`, `str_to_string`, `print_stdout`, `todo`, `use_self`), and `unsafe_code` is denied. CI and `check.sh` run with `-D warnings`, so every warning fails the build. Code must also compile for `wasm32-unknown-unknown`, so put native-only APIs behind `cfg`. `.github/workflows/typos.yml` runs a spell check, with `.typos.toml` holding the exceptions.
