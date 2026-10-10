use crate::model::chart::{self, CHART_TEMP_RANGE_C, LATENT_HEAT_KJ_KG};
use crate::model::{AirHandlerUnit, AirState, DuctId, Simulation};
use egui::{Align, Align2, Color32, DragValue, Rect, RichText, Ui, UiBuilder, vec2};
use egui_plot::{
    HoverPosition, Line, LineStyle, Plot, PlotPoint, PlotPoints, PlotUi, Points, Text,
};

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct ChartSettings {
    pub kind: ChartKind,
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

/// Allowed width / height ratio of the plot area.
const PLOT_ASPECT_RANGE: (f32, f32) = (0.8, 2.0);

const SUPPLY_COLOR: Color32 = Color32::from_rgb(0x4b, 0x8f, 0xd9);
const EXTRACT_COLOR: Color32 = Color32::from_rgb(0xd9, 0x8a, 0x3c);

/// Psychrometric chart or Mollier h-x diagram with the state points of both airstreams.
///
/// Drag pans, Ctrl + scroll zooms, right-drag draws a zoom box, double-click resets the view.
///
/// Returns the screen rectangle of the plot when the user asked to save it as an image.
pub fn chart(ui: &mut Ui, unit: &mut AirHandlerUnit, settings: &mut ChartSettings) -> Option<Rect> {
    let mut save_requested = false;
    ui.horizontal(|ui| {
        for option in ChartKind::ALL {
            ui.selectable_value(&mut settings.kind, option, option.label());
        }
        ui.separator();
        ui.label("p");
        ui.add(
            DragValue::new(&mut unit.pressure_mbar)
                .range(300.0..=1100.0)
                .speed(1.0)
                .suffix(" mbar"),
        );
        save_requested = ui.button("Save PNG").clicked();
        ui.weak("drag: pan · Ctrl+scroll: zoom · right-drag: box zoom · double-click: reset");
    });
    let kind = settings.kind;
    let pressure_pa = unit.pressure_pa();
    let simulation = unit.simulate();
    let (x_label, y_label) = kind.axis_labels();

    let ducts = [
        (DuctId::Supply, SUPPLY_COLOR),
        (DuctId::Extract, EXTRACT_COLOR),
    ]
    .map(|(duct, color)| (duct, color, path(unit, &simulation, duct)));

    // Keep the plot's width / height within `PLOT_ASPECT_RANGE`: a very wide or very tall panel
    // leaves empty margins instead of stretching the chart.
    let available = ui.available_size();
    let (min_aspect, max_aspect) = PLOT_ASPECT_RANGE;
    let width = available.x.min(available.y * max_aspect);
    let height = available.y.min(available.x / min_aspect);
    let mut plot_rect = None;
    let top_left = ui.available_rect_before_wrap().min;
    let plot_area = Rect::from_min_size(
        top_left + vec2((available.x - width) / 2.0, (available.y - height) / 2.0),
        vec2(width, height),
    );
    ui.scope_builder(UiBuilder::new().max_rect(plot_area), |ui| {
        let response = Plot::new("air_chart")
            .width(width)
            .height(height)
            .data_aspect(1.0)
            .x_axis_label(x_label)
            .y_axis_label(y_label)
            .allow_drag(true)
            .allow_zoom(true)
            .allow_scroll(true)
            .allow_boxed_zoom(true)
            .allow_double_click_reset(true)
            .include_x(0.0)
            .include_y(f64::from(CHART_TEMP_RANGE_C.0))
            .label_formatter(|hover| match hover {
                HoverPosition::NearDataPoint {
                    plot_name, index, ..
                } => ducts
                    .iter()
                    .find(|(duct, ..)| duct.inlet_label() == *plot_name)
                    .and_then(|(_, _, states)| states.get(*index))
                    .map(|state| state_label(plot_name, *index, state)),
                HoverPosition::Elsewhere { .. } => None,
            })
            .show(ui, |plot| {
                let avoid: Vec<Vec<[f64; 2]>> = ducts
                    .iter()
                    .map(|(_, _, states)| {
                        states
                            .iter()
                            .map(|s| kind.project((s.temp_c, s.humidity_ratio)))
                            .collect()
                    })
                    .collect();
                background(plot, kind, pressure_pa, &avoid);
                for (duct, color, states) in &ducts {
                    let points: Vec<[f64; 2]> = states
                        .iter()
                        .map(|s| kind.project((s.temp_c, s.humidity_ratio)))
                        .collect();
                    let name = duct.inlet_label();
                    plot.line(
                        Line::new(name, PlotPoints::from(points.clone()))
                            .color(*color)
                            .width(2.5),
                    );
                    plot.points(
                        Points::new(name, PlotPoints::from(points))
                            .color(*color)
                            .radius(4.5),
                    );
                }
            });
        plot_rect = Some(response.response.rect);
    });
    plot_rect.filter(|_| save_requested)
}

/// Hover text of one state point of a duct.
fn state_label(duct_name: &str, index: usize, state: &AirState) -> String {
    let point = if index == 0 {
        "Inlet".to_owned()
    } else {
        format!("Point {index}")
    };
    format!(
        "{duct_name} · {point}
t = {:.1} °C
x = {:.2} g/kg
φ = {:.0} %
h = {:.1} kJ/kg
td = {:.1} °C",
        state.temp_c,
        state.humidity_ratio * 1000.0,
        state.rel_humidity(),
        state.enthalpy(),
        state.dew_point_c(),
    )
}

/// Draws the background lines and their labels for `kind`.
fn background(plot: &mut PlotUi<'_>, kind: ChartKind, pressure_pa: f64, avoid: &[Vec<[f64; 2]>]) {
    match kind {
        ChartKind::Psychrometric => background_psychrometric(plot, pressure_pa),
        ChartKind::Mollier => background_mollier(plot, pressure_pa, avoid),
    }
}

/// Font size of the labels on the background lines.
const LABEL_SIZE: f32 = 12.5;
/// How far the density lines are extended past the humidity ratio 0 axis, so that their labels
/// sit outside the grid.
const DENSITY_EXTENSION: f64 = 6.0;

/// Background of the psychrometric chart: saturation and relative-humidity curves, isotherms,
/// enthalpy and density lines, labelled at the line ends.
fn background_psychrometric(plot: &mut PlotUi<'_>, pressure_pa: f64) {
    let kind = ChartKind::Psychrometric;
    let faint = Color32::from_gray(128).gamma_multiply(0.45);
    let label_color = Color32::from_gray(128);
    let (t_min, t_max) = CHART_TEMP_RANGE_C;

    for t in (t_min.div_euclid(5) * 5..=t_max).step_by(5) {
        if t >= t_min {
            let line = chart::isotherm(f64::from(t), pressure_pa);
            let points = project_all(&line, kind);
            plot.line(plain_line(points, faint, 1.0));
        }
    }
    for h in (-20..=130).step_by(5) {
        let line = chart::enthalpy_line(f64::from(h), pressure_pa);
        let points = project_all(&line, kind);
        let Some(&end) = points.last() else { continue };
        plot.line(plain_line(points, faint, 1.0));
        if h % 10 == 0 {
            let text = format!("{h} kJ/kg");
            label(plot, end, &text, label_color, Align2::LEFT_TOP);
        }
    }
    for density in [1.05, 1.10, 1.15, 1.20, 1.25, 1.30] {
        let line = chart::density_line(density, pressure_pa);
        let points = extended(project_all(&line, kind), DENSITY_EXTENSION, 0.0);
        let start = points.first().copied();
        plot.line(plain_line(points, faint, 1.0).style(LineStyle::dashed_dense()));
        if let Some(start) = start {
            let text = format!("{density:.2} kg/m³");
            label(plot, start, &text, label_color, Align2::RIGHT_CENTER);
        }
    }
    for rh in (10..=90).step_by(10) {
        let line = chart::rel_humidity_curve(f64::from(rh), pressure_pa);
        let points = project_all(&line, kind);
        let end = points.last().copied();
        plot.line(plain_line(points, label_color.gamma_multiply(0.7), 1.0));
        if let Some(end) = end {
            let text = format!("{rh} %");
            label(plot, end, &text, label_color, Align2::LEFT_BOTTOM);
        }
    }
    let saturation = chart::rel_humidity_curve(100.0, pressure_pa);
    let color = Color32::from_gray(160);
    let points = project_all(&saturation, kind);
    plot.line(plain_line(points, color, 2.5).name("φ = 100 %"));
}

/// Text at a plot position.
fn label(plot: &mut PlotUi<'_>, at: [f64; 2], text: &str, color: Color32, anchor: Align2) {
    plot.text(
        Text::new("", at.into(), RichText::new(text).size(LABEL_SIZE))
            .color(color)
            .anchor(anchor),
    );
}

/// Humidity ratio (g/kg) the Mollier density lines are extended to, left of the axis, where
/// their labels sit.
const MOLLIER_DENSITY_LABEL_X: f64 = -3.0;
/// Largest humidity ratio (g/kg) at which a density line counts as starting on the axis.
const DENSITY_AXIS_TOLERANCE: f64 = 4.0;
/// Distance in points between a Mollier relative humidity or enthalpy label and its line end.
const LABEL_GAP: f32 = 5.0;
/// Margin in points kept free around the duct paths by the Mollier labels.
const PATH_MARGIN: f32 = 4.0;
/// Rough glyph width of a label relative to its font size, used to estimate its extent.
const LABEL_CHAR_WIDTH: f32 = 0.55;

/// Places the labels of the background lines, dropping any that would overlap a label placed
/// before it or leave the plot. Labels are placed in call order, so call it by priority.
struct Labels {
    placed: Vec<Rect>,
    color: Color32,
}

impl Labels {
    /// Keeps labels off the polyline through `path`, given in plot coordinates.
    fn avoid_path(&mut self, plot: &PlotUi<'_>, path: &[[f64; 2]]) {
        let transform = plot.transform();
        let screen = |p: &[f64; 2]| transform.position_from_point(&PlotPoint::new(p[0], p[1]));
        self.placed.extend(path.windows(2).map(|pair| {
            Rect::from_two_pos(screen(&pair[0]), screen(&pair[1])).expand(PATH_MARGIN)
        }));
    }

    /// Adds `text` at `at`, pushed `gap` points away from the point in the direction the
    /// anchor points away from.
    fn add(&mut self, plot: &mut PlotUi<'_>, at: [f64; 2], text: &str, anchor: Align2, gap: f32) {
        let transform = plot.transform();
        let away = |align: Align| match align {
            Align::Min => gap,
            Align::Center => 0.0,
            Align::Max => -gap,
        };
        let pos = transform.position_from_point(&PlotPoint::new(at[0], at[1]))
            + vec2(away(anchor.x()), away(anchor.y()));
        let at = transform.value_from_position(pos);
        let size = vec2(
            text.chars().count() as f32 * LABEL_SIZE * LABEL_CHAR_WIDTH,
            LABEL_SIZE * 1.2,
        );
        let rect = anchor.anchor_size(pos, size).expand(2.0);
        if !transform.frame().contains_rect(rect) || self.placed.iter().any(|r| r.intersects(rect))
        {
            return;
        }
        self.placed.push(rect);
        plot.text(
            Text::new("", at, RichText::new(text).size(LABEL_SIZE))
                .color(self.color)
                .anchor(anchor),
        );
    }
}

/// Background of the Mollier h-x diagram. Its lines run diagonally and crowd together, so the
/// labels are placed with collision avoidance instead of at fixed line ends.
fn background_mollier(plot: &mut PlotUi<'_>, pressure_pa: f64, avoid: &[Vec<[f64; 2]>]) {
    let kind = ChartKind::Mollier;
    let faint = Color32::from_gray(128).gamma_multiply(0.45);
    let label_color = Color32::from_gray(128);
    let (t_min, t_max) = CHART_TEMP_RANGE_C;
    let mut labels = Labels {
        placed: Vec::new(),
        color: label_color,
    };
    for path in avoid {
        labels.avoid_path(plot, path);
    }

    for t in (t_min.div_euclid(5) * 5..=t_max).step_by(5) {
        if t >= t_min {
            let line = chart::isotherm(f64::from(t), pressure_pa);
            let points = project_all(&line, kind);
            plot.line(plain_line(points, faint, 1.0));
        }
    }
    // Enthalpy lines start on the saturation curve; their labels sit just outside it, in the
    // otherwise empty fog region.
    let mut enthalpy_labels = Vec::new();
    for h in (-20..=130).step_by(5) {
        let line = chart::enthalpy_line(f64::from(h), pressure_pa);
        let points = project_all(&line, kind);
        if let Some(&start) = points.first()
            && h % 10 == 0
        {
            enthalpy_labels.push((start, format!("{h} kJ/kg")));
        }
        plot.line(plain_line(points, faint, 1.0));
    }
    let mut density_labels = Vec::new();
    for density in [1.05, 1.10, 1.15, 1.20, 1.25, 1.30] {
        let line = chart::density_line(density, pressure_pa);
        let mut points = project_all(&line, kind);
        // Start at the left end. Lines that reach the humidity ratio 0 axis there are extended
        // past it and labelled outside the grid; the others are labelled above their top end.
        if points
            .first()
            .zip(points.last())
            .is_some_and(|(a, b)| b[0] < a[0])
        {
            points.reverse();
        }
        let text = format!("{density:.2} kg/m³");
        if points
            .first()
            .is_some_and(|start| start[0] < DENSITY_AXIS_TOLERANCE)
        {
            // Continue the line straight past the axis, up to the label position.
            if let [first, second, ..] = points[..] {
                let (dx, dy) = (first[0] - second[0], first[1] - second[1]);
                if dx < 0.0 {
                    let k = (MOLLIER_DENSITY_LABEL_X - first[0]) / dx;
                    points.insert(0, [MOLLIER_DENSITY_LABEL_X, first[1] + k * dy]);
                }
            }
            density_labels.push((points[0], text, Align2::RIGHT_CENTER));
        } else if let Some(&top) = points.iter().max_by(|a, b| a[1].total_cmp(&b[1])) {
            density_labels.push((top, text, Align2::CENTER_BOTTOM));
        }
        plot.line(plain_line(points, faint, 1.0).style(LineStyle::dashed_dense()));
    }
    // Relative humidity labels have the highest priority: above the top edge when the curve
    // leaves through it, otherwise to the right of the humidity ratio limit.
    for rh in (10..=90).step_by(10) {
        let line = chart::rel_humidity_curve(f64::from(rh), pressure_pa);
        let points = project_all(&line, kind);
        let leaves_at_top = line
            .last()
            .is_some_and(|&(t, _)| t >= f64::from(t_max) - 1e-9);
        if let Some(&end) = points.last() {
            let anchor = if leaves_at_top {
                Align2::CENTER_BOTTOM
            } else {
                Align2::LEFT_CENTER
            };
            labels.add(plot, end, &format!("{rh} %"), anchor, LABEL_GAP);
        }
        plot.line(plain_line(points, label_color.gamma_multiply(0.7), 1.0));
    }
    for (at, text) in enthalpy_labels {
        labels.add(plot, at, &text, Align2::LEFT_TOP, LABEL_GAP);
    }
    for (at, text, anchor) in density_labels {
        labels.add(plot, at, &text, anchor, 0.0);
    }
    let saturation = chart::rel_humidity_curve(100.0, pressure_pa);
    let color = Color32::from_gray(160);
    let points = project_all(&saturation, kind);
    plot.line(plain_line(points, color, 2.5).name("φ = 100 %"));
}

fn project_all(points: &[(f64, f64)], kind: ChartKind) -> Vec<[f64; 2]> {
    points.iter().map(|&p| kind.project(p)).collect()
}

/// Prolongs a polyline straight along its first and last segment by the given plot lengths.
fn extended(mut points: Vec<[f64; 2]>, at_start: f64, at_end: f64) -> Vec<[f64; 2]> {
    let beyond = |from: [f64; 2], to: [f64; 2], length: f64| {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let norm = dx.hypot(dy);
        (norm > 0.0).then(|| [to[0] + dx / norm * length, to[1] + dy / norm * length])
    };
    if let [first, second, ..] = points[..]
        && let Some(point) = beyond(second, first, at_start)
    {
        points.insert(0, point);
    }
    if let [.., before, last] = points[..]
        && let Some(point) = beyond(before, last, at_end)
    {
        points.push(point);
    }
    points
}

fn plain_line(points: Vec<[f64; 2]>, color: Color32, width: f32) -> Line<'static> {
    Line::new("", PlotPoints::from(points))
        .color(color)
        .width(width)
}

/// Air states along one duct: inlet, then the outlet of every component, with the heat recovery
/// outlet inserted where the recovery sits.
fn path(unit: &AirHandlerUnit, simulation: &Simulation, duct: DuctId) -> Vec<AirState> {
    let result = simulation.duct(duct);
    let mut states = vec![unit.inlet(duct)];
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
