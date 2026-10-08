use crate::model::chart::{self, CHART_PRESSURE_PA, CHART_TEMP_RANGE_C, LATENT_HEAT_KJ_KG};
use crate::model::{AirHandlerUnit, AirState, DuctId, Simulation};
use egui::{Align2, Color32, DragValue, Ui};
use egui_plot::{Line, LineStyle, Plot, PlotPoints, PlotUi, Points, Text};

/// Which diagram the chart widget draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum ChartKind {
    /// Dry-bulb temperature on x, humidity ratio on y.
    Psychrometric,
    /// Mollier h-x diagram: humidity ratio on x, with skewed axes so isenthalps are straight
    /// diagonals and the temperature scale sits on the y axis at x = 0.
    #[default]
    Mollier,
}

/// User settings of the chart widget.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct ChartSettings {
    pub kind: ChartKind,
    /// Barometric pressure the chart is drawn for, in mbar.
    pub pressure_mbar: f64,
}

impl Default for ChartSettings {
    fn default() -> Self {
        Self {
            kind: ChartKind::default(),
            pressure_mbar: CHART_PRESSURE_PA / 100.0,
        }
    }
}

impl ChartKind {
    const ALL: [Self; 2] = [Self::Mollier, Self::Psychrometric];

    fn label(self) -> &'static str {
        match self {
            Self::Psychrometric => "Psychrometric chart",
            Self::Mollier => "Mollier h-x",
        }
    }

    /// Projects `(temperature °C, humidity ratio kg/kg)` onto plot coordinates.
    fn project(self, (temp_c, hum_ratio): (f64, f64)) -> [f64; 2] {
        match self {
            Self::Psychrometric => [temp_c, hum_ratio * 1000.0],
            Self::Mollier => [
                hum_ratio * 1000.0,
                (chart::enthalpy_at(temp_c, hum_ratio) - LATENT_HEAT_KJ_KG * hum_ratio)
                    / chart::enthalpy_at(1.0, 0.0),
            ],
        }
    }

    fn axis_labels(self) -> (&'static str, &'static str) {
        match self {
            Self::Psychrometric => ("Dry-bulb temperature t [°C]", "Humidity ratio x [g/kg]"),
            Self::Mollier => ("Humidity ratio x [g/kg]", "Temperature t [°C] (at x = 0)"),
        }
    }
}

const SUPPLY_COLOR: Color32 = Color32::from_rgb(0x4b, 0x8f, 0xd9);
const EXTRACT_COLOR: Color32 = Color32::from_rgb(0xd9, 0x8a, 0x3c);

/// Psychrometric chart or Mollier h-x diagram with the state points of both airstreams.
///
/// Drag pans, Ctrl + scroll zooms, right-drag draws a zoom box, double-click resets the view.
pub fn chart(ui: &mut Ui, unit: &AirHandlerUnit, settings: &mut ChartSettings) {
    ui.horizontal(|ui| {
        for option in ChartKind::ALL {
            ui.selectable_value(&mut settings.kind, option, option.label());
        }
        ui.separator();
        ui.label("p");
        ui.add(
            DragValue::new(&mut settings.pressure_mbar)
                .range(300.0..=1100.0)
                .speed(1.0)
                .suffix(" mbar"),
        );
        ui.weak("drag: pan · Ctrl+scroll: zoom · right-drag: box zoom · double-click: reset");
    });
    let kind = settings.kind;
    let pressure_pa = settings.pressure_mbar * 100.0;
    let simulation = unit.simulate();
    let (x_label, y_label) = kind.axis_labels();

    Plot::new("air_chart")
        .x_axis_label(x_label)
        .y_axis_label(y_label)
        .allow_drag(true)
        .allow_zoom(true)
        .allow_scroll(true)
        .allow_boxed_zoom(true)
        .allow_double_click_reset(true)
        .include_x(0.0)
        .include_y(f64::from(CHART_TEMP_RANGE_C.0))
        .show(ui, |plot| {
            background(plot, kind, pressure_pa);
            for (duct, color) in [
                (DuctId::Supply, SUPPLY_COLOR),
                (DuctId::Extract, EXTRACT_COLOR),
            ] {
                let points: Vec<[f64; 2]> = path(unit, &simulation, duct)
                    .iter()
                    .map(|s| kind.project((s.temp_c, s.humidity_ratio)))
                    .collect();
                let name = duct.inlet_label();
                plot.line(
                    Line::new(name, PlotPoints::from(points.clone()))
                        .color(color)
                        .width(2.5),
                );
                plot.points(
                    Points::new(name, PlotPoints::from(points))
                        .color(color)
                        .radius(4.5),
                );
            }
        });
}

/// Saturation and relative-humidity curves, isotherms, enthalpy and density lines, labelled
/// like a printed h-x sheet.
fn background(plot: &mut PlotUi<'_>, kind: ChartKind, pressure_pa: f64) {
    let faint = Color32::from_gray(128).gamma_multiply(0.45);
    let label_color = Color32::from_gray(128);
    let (t_min, t_max) = CHART_TEMP_RANGE_C;

    for t in (t_min.div_euclid(5) * 5..=t_max).step_by(5) {
        if t >= t_min {
            let line = chart::isotherm(f64::from(t), pressure_pa);
            plot.line(curve("", &line, kind, faint, 1.0));
        }
    }
    for h in (-20..=130).step_by(5) {
        let line = chart::enthalpy_line(f64::from(h), pressure_pa);
        let Some(&end) = line.last() else { continue };
        plot.line(curve("", &line, kind, faint, 1.0));
        if h % 10 == 0 {
            label(
                plot,
                kind,
                end,
                &h.to_string(),
                label_color,
                Align2::LEFT_TOP,
            );
        }
    }
    for density in [1.05, 1.10, 1.15, 1.20, 1.25, 1.30] {
        let line = chart::density_line(density, pressure_pa);
        plot.line(
            Line::new("", PlotPoints::from(project_all(&line, kind)))
                .color(faint)
                .style(LineStyle::dashed_dense()),
        );
        if let Some(&start) = line.first() {
            let text = format!("{density:.2}");
            label(plot, kind, start, &text, label_color, Align2::RIGHT_CENTER);
        }
    }
    for rh in (10..=90).step_by(10) {
        let line = chart::rel_humidity_curve(f64::from(rh), pressure_pa);
        plot.line(curve("", &line, kind, label_color.gamma_multiply(0.7), 1.0));
        if let Some(&end) = line.last() {
            let text = format!("{rh} %");
            label(plot, kind, end, &text, label_color, Align2::LEFT_BOTTOM);
        }
    }
    let saturation = chart::rel_humidity_curve(100.0, pressure_pa);
    let color = Color32::from_gray(160);
    plot.line(curve("φ = 100 %", &saturation, kind, color, 2.5));
}

fn label(
    plot: &mut PlotUi<'_>,
    kind: ChartKind,
    at: (f64, f64),
    text: &str,
    color: Color32,
    anchor: Align2,
) {
    plot.text(
        Text::new("", kind.project(at).into(), text)
            .color(color)
            .anchor(anchor),
    );
}

fn project_all(points: &[(f64, f64)], kind: ChartKind) -> Vec<[f64; 2]> {
    points.iter().map(|&p| kind.project(p)).collect()
}

fn curve(
    name: &str,
    points: &[(f64, f64)],
    kind: ChartKind,
    color: Color32,
    width: f32,
) -> Line<'static> {
    Line::new(name, PlotPoints::from(project_all(points, kind)))
        .color(color)
        .width(width)
}

/// Air states along one duct: inlet, then the outlet of every component, with the heat recovery
/// outlet inserted where the recovery sits.
fn path(unit: &AirHandlerUnit, simulation: &Simulation, duct: DuctId) -> Vec<AirState> {
    let result = simulation.duct(duct);
    let mut states = vec![unit.duct(duct).inlet()];
    let recovery_state = simulation.recovery.map(|stage| match duct {
        DuctId::Supply => stage.exchange.supply_out,
        DuctId::Extract => stage.exchange.extract_out,
    });
    let recovery_pos = unit.recovery_position(duct);
    for (index, stage) in result.stages.iter().enumerate() {
        if recovery_pos == Some(index) {
            states.extend(recovery_state);
        }
        states.push(stage.out);
    }
    if recovery_pos == Some(result.stages.len()) {
        states.extend(recovery_state);
    }
    states
}
