# Air Handler

A desktop and web app for designing and calculating air handling units (AHUs). It is written in Rust with [egui](https://github.com/emilk/egui)/[eframe](https://github.com/emilk/egui/tree/master/crates/eframe) and compiles natively and to WebAssembly.

You build a unit by dragging components from a library onto the ducts. A properties panel edits whatever is selected, and every change recalculates the air state through the whole unit.

## Using the app

The window has three areas:

- **Library (left):** the available components. Drag one onto a duct.
- **Diagram (centre):** the unit. The top row is the supply duct (outdoor air → supply air). The bottom row is the extract duct (extract air → exhaust air). Each component shows its duty, and the air state leaving it is printed below it.
- **Properties (right):** click a component, a duct name or the heat recovery to edit its parameters and see the calculated results.

A highlighted line shows where a dropped component will be inserted. A duct's inlet conditions (temperature, humidity, flow) are edited by clicking its name on the left of the row.

### Components

| Component  | Behaviour                                                                              |
|------------|----------------------------------------------------------------------------------------|
| Heater     | Heats the air to a setpoint if it is colder. Reports thermal power.                    |
| Cooler     | Cools the air to a setpoint. Below the dew point it dehumidifies and reports condensate. |
| Humidifier | Adds steam up to a target relative humidity. Reports water use.                        |
| Fan        | Raises the pressure. Reports electrical power, which also heats the air.               |

### Heat recovery

A heat recovery spans both ducts and exchanges heat between the supply and the extract stream. Dropping one splits the rows into *outdoor air | supply air* and *extract air | exhaust air*. Components placed to its left treat the incoming air, for example a preheater. Components to its right treat the recovered air. A unit has at most one heat recovery.

| Type            | Notes                                                                                   |
|-----------------|-----------------------------------------------------------------------------------------|
| Plate exchanger | Sensible heat only. Counterflow caps the efficiency at 0.95 and crossflow at 0.75.       |
| Rotary wheel    | Sensible heat plus optional moisture transfer (hygroscopic wheel). Has a drive power.   |
| Run-around coil | Sensible heat only, with a lower typical efficiency. Has a pump power.                  |

All types share one steady-state model. The temperature efficiency is defined on the supply side, the transferred heat is limited by the stream with the lower heat capacity, and the extract condenses when it is cooled below its dew point. The default efficiencies are typical placeholder values, not manufacturer data.

### Calculation assumptions

- Moist air at sea-level pressure (1013.25 hPa) with a constant density of 1.2 kg/m³.
- Saturation pressure from the Magnus formula.
- Steady state, with no pressure-drop or fan-curve model. A fan's pressure rise is an input.
- Latent heat released by condensation in the heat recovery is not credited to the supply air, which makes results slightly conservative.
- There is no frost protection yet.

The state of the unit is saved when you close the app and restored on the next start.

## Building

Requires the Rust toolchain pinned in `rust-toolchain` (installed automatically by `rustup`).

### Native

```sh
cargo run --release
```

On Linux you first need:

```sh
sudo apt-get install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
```

A `flake.nix` with a development shell is also provided.

### Web

The web build uses [Trunk](https://trunk-rs.github.io/trunk).

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
trunk serve
```

Then open <http://127.0.0.1:8080/index.html#dev>. The `#dev` suffix matters. Without it, the service worker in `assets/sw.js` caches the app and serves stale builds.

`trunk build --release` writes a static site to `dist/`. The workflow in `.github/workflows/pages.yml` deploys it to GitHub Pages on every push to `main`. To enable it, set the repository's Pages source to the `gh-pages` branch.

## Development

```sh
./check.sh                                    # everything CI runs: check, wasm check, fmt, clippy, tests, trunk build
cargo test                                    # unit tests (they cover the model)
cargo clippy --all-targets -- -D warnings     # the lint set in Cargo.toml is strict; warnings fail CI
```

The code has two layers:

- `src/model/` is the domain and calculation code (psychrometrics, components, heat recovery, the unit). It does not depend on egui and is unit tested.
- `src/ui/` holds the widgets (library, diagram, properties). They read and edit the model.
- `src/app.rs` owns the unit and arranges the panels.

## License

Licensed under [MIT license](LICENSE-MIT).

---

The project started from [eframe_template](https://github.com/emilk/eframe_template).
