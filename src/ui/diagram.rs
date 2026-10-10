use super::{RECOVERY_COLOR, Selection, kind_color};
use crate::model::{
    AirHandlerUnit, AirState, ComponentKind, Duct, DuctId, DuctResult, HeatRecovery,
    HeatRecoveryKind, Placed, RecoveryStage, Stage,
};
use egui::{
    Align2, FontId, Painter, Pos2, Rect, Response, ScrollArea, Sense, Stroke, StrokeKind, Ui, Vec2,
    Visuals, vec2,
};

const ROW_HEIGHT: f32 = 150.0;
const ROW_SPACING: f32 = 8.0;
const LABEL_WIDTH: f32 = 130.0;
const BOX_SIZE: Vec2 = vec2(100.0, 56.0);
const GAP: f32 = 36.0;

enum Dropped {
    Component(ComponentKind),
    Recovery(HeatRecoveryKind),
}

/// Something dropped on a duct, to be inserted after `index` components.
struct Drop {
    duct: DuctId,
    index: usize,
    /// Which side of the heat recovery the pointer was on.
    before_recovery: bool,
    item: Dropped,
}

/// Where the heat recovery sits, as seen by the layout.
///
/// Both ducts share one column grid so the heat recovery lines up across the rows. Components
/// before the recovery keep their index as slot, the recovery takes the slot after the longer
/// "before" side, and later components follow it.
#[derive(Clone, Copy)]
struct Layout {
    /// Components before the recovery per duct, or `None` without a recovery.
    positions: Option<[usize; 2]>,
}

impl Layout {
    fn new(unit: &AirHandlerUnit) -> Self {
        let supply = unit.recovery_position(DuctId::Supply);
        let extract = unit.recovery_position(DuctId::Extract);
        Self {
            positions: supply.zip(extract).map(<[usize; 2]>::from),
        }
    }

    fn recovery_slot(self) -> Option<usize> {
        self.positions.map(|[s, e]| s.max(e))
    }

    /// Slot of the component at `index` in `duct`.
    fn slot(self, duct: DuctId, index: usize) -> usize {
        match (self.positions, self.recovery_slot()) {
            (Some(positions), Some(column)) => {
                let pos = positions[duct as usize];
                if index < pos {
                    index
                } else {
                    index + 1 + (column - pos)
                }
            }
            _ => index,
        }
    }

    /// Slots in use in `duct`, including the recovery.
    fn occupied(self, duct: DuctId, count: usize) -> Vec<usize> {
        (0..count)
            .map(|i| self.slot(duct, i))
            .chain(self.recovery_slot())
            .collect()
    }
}

/// Draws both ducts and the heat recovery. Accepts [`ComponentKind`] and [`HeatRecoveryKind`]
/// drops from the library and updates `selection` when something is clicked.
pub fn diagram(ui: &mut Ui, unit: &mut AirHandlerUnit, selection: &mut Option<Selection>) {
    let mut pending = None;
    let layout = Layout::new(unit);
    let sim = unit.simulate();
    ScrollArea::horizontal().show(ui, |ui| {
        let slots = DuctId::ALL
            .into_iter()
            .map(|id| layout.slot(id, unit.duct(id).components.len()))
            .max()
            .unwrap_or(0);
        let width = ui
            .available_width()
            .max(LABEL_WIDTH + GAP + (slots + 1) as f32 * (BOX_SIZE.x + GAP));

        let mut rows = [Rect::NOTHING; 2];
        for id in DuctId::ALL {
            let view = DuctView {
                id,
                duct: unit.duct(id),
                inlet: unit.inlet(id),
                result: sim.duct(id),
                layout,
            };
            let (rect, drop) = duct_row(ui, width, &view, selection);
            rows[id as usize] = rect;
            if pending.is_none() {
                pending = drop;
            }
            ui.add_space(ROW_SPACING);
        }
        if let (Some(recovery), Some(stage), Some(column)) =
            (unit.recovery(), sim.recovery, layout.recovery_slot())
        {
            recovery_box(ui, rows, column, recovery, &stage, selection);
        }
    });

    match pending {
        Some(Drop {
            duct,
            index,
            before_recovery,
            item: Dropped::Component(kind),
        }) => {
            let id = unit.add(duct, index, before_recovery, kind);
            *selection = Some(Selection::Component(id));
        }
        Some(Drop {
            duct,
            index,
            item: Dropped::Recovery(kind),
            ..
        }) => {
            unit.place_recovery(kind, duct, index);
            *selection = Some(Selection::Recovery);
        }
        None => {}
    }
}

struct DuctView<'a> {
    id: DuctId,
    duct: &'a Duct,
    inlet: AirState,
    result: &'a DuctResult,
    layout: Layout,
}

fn slot_rect(row: Rect, slot: usize) -> Rect {
    let x = row.left() + LABEL_WIDTH + GAP + slot as f32 * (BOX_SIZE.x + GAP);
    Rect::from_min_size(
        Pos2::new(x, row.center().y - BOX_SIZE.y / 2.0 - 12.0),
        BOX_SIZE,
    )
}

fn state_text(state: &AirState) -> String {
    format!("{:.1} °C · {:.0} %", state.temp_c, state.rel_humidity())
}

fn duct_row(
    ui: &mut Ui,
    width: f32,
    view: &DuctView<'_>,
    selection: &mut Option<Selection>,
) -> (Rect, Option<Drop>) {
    let DuctView {
        id,
        duct,
        result,
        layout,
        ..
    } = *view;
    let count = duct.components.len();
    let (rect, row) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals().clone();
    let line_y = slot_rect(rect, 0).center().y;

    let over_component = row.dnd_hover_payload::<ComponentKind>().is_some();
    let over_recovery = row.dnd_hover_payload::<HeatRecoveryKind>().is_some();
    let dragging_over = over_component || over_recovery;
    painter.rect_filled(rect, 6.0, visuals.faint_bg_color);
    if dragging_over {
        painter.rect_stroke(
            rect,
            6.0,
            Stroke::new(2.0, visuals.selection.bg_fill),
            StrokeKind::Inside,
        );
    }

    row_labels(ui, &row, rect, view, selection);

    for (index, (placed, stage)) in duct.components.iter().zip(&result.stages).enumerate() {
        let b = slot_rect(rect, layout.slot(id, index));
        let response = ui.interact(b, row.id.with(placed.id), Sense::click());
        if response.clicked() {
            *selection = Some(Selection::Component(placed.id));
        }
        let selected = *selection == Some(Selection::Component(placed.id));
        component_box(&painter, &visuals, b, placed, stage, selected);
    }

    // Insertion point under the pointer while dragging a library entry over the row.
    let occupied = layout.occupied(id, count);
    let pointer_x = ui
        .input(|i| i.pointer.latest_pos())
        .map_or(f32::MAX, |p| p.x);
    let left_of_pointer = |slot: usize| slot_rect(rect, slot).center().x < pointer_x;
    let index = (0..count)
        .filter(|&i| left_of_pointer(layout.slot(id, i)))
        .count();
    let before_recovery = layout
        .recovery_slot()
        .is_some_and(|slot| !left_of_pointer(slot));
    if dragging_over {
        let next_slot = occupied
            .iter()
            .copied()
            .filter(|&slot| !left_of_pointer(slot))
            .min()
            .unwrap_or_else(|| occupied.iter().max().map_or(0, |last| last + 1));
        let x = slot_rect(rect, next_slot).left() - GAP / 2.0;
        painter.line_segment(
            [Pos2::new(x, line_y - 40.0), Pos2::new(x, line_y + 40.0)],
            Stroke::new(3.0, visuals.selection.bg_fill),
        );
    }

    // Peek at the type first: releasing takes the payload out even when the type does not match.
    let item = if over_component {
        row.dnd_release_payload::<ComponentKind>()
            .map(|kind| Dropped::Component(*kind))
    } else if over_recovery {
        row.dnd_release_payload::<HeatRecoveryKind>()
            .map(|kind| Dropped::Recovery(*kind))
    } else {
        None
    };
    let drop = item.map(|item| Drop {
        duct: id,
        index,
        before_recovery,
        item,
    });
    (rect, drop)
}

/// Draws the name and conditions of the air entering and leaving the duct, and the duct line.
/// Clicking the inlet name selects the duct.
fn row_labels(
    ui: &Ui,
    row: &Response,
    rect: Rect,
    view: &DuctView<'_>,
    selection: &mut Option<Selection>,
) {
    let DuctView {
        id,
        inlet,
        result,
        layout,
        ..
    } = *view;
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    let line_y = slot_rect(rect, 0).center().y;
    // Name and conditions of the air entering the duct; clicking selects the duct.
    let label_rect = Rect::from_min_size(rect.min, vec2(LABEL_WIDTH, ROW_HEIGHT));
    let label = ui.interact(label_rect, row.id.with("label"), Sense::click());
    if label.clicked() {
        *selection = Some(Selection::Duct(id));
    }
    let selected = *selection == Some(Selection::Duct(id));
    painter.text(
        rect.min + vec2(12.0, 12.0),
        Align2::LEFT_TOP,
        id.inlet_label(),
        FontId::proportional(16.0),
        if selected {
            visuals.selection.stroke.color
        } else {
            visuals.text_color()
        },
    );
    painter.text(
        rect.min + vec2(12.0, 36.0),
        Align2::LEFT_TOP,
        state_text(&inlet),
        FontId::proportional(12.0),
        visuals.weak_text_color(),
    );

    painter.line_segment(
        [
            Pos2::new(rect.left() + LABEL_WIDTH, line_y),
            Pos2::new(rect.right() - 12.0, line_y),
        ],
        Stroke::new(2.0, visuals.widgets.noninteractive.fg_stroke.color),
    );

    // Name and conditions of the air leaving the duct. A heat recovery splits the row,
    // so the name then sits right behind it.
    let (outlet_x, align) = match layout.recovery_slot() {
        Some(column) => (slot_rect(rect, column).right() + 8.0, Align2::LEFT_BOTTOM),
        None => (rect.right() - 12.0, Align2::RIGHT_BOTTOM),
    };
    painter.text(
        Pos2::new(outlet_x, line_y - BOX_SIZE.y / 2.0 - 6.0),
        align,
        format!("{}: {}", id.outlet_label(), state_text(&result.outlet)),
        FontId::proportional(12.0),
        visuals.weak_text_color(),
    );
}

fn component_box(
    painter: &Painter,
    visuals: &Visuals,
    b: Rect,
    placed: &Placed,
    stage: &Stage,
    selected: bool,
) {
    let kind = placed.component.kind();
    painter.rect_filled(b, 6.0, visuals.window_fill);
    painter.rect_stroke(
        b,
        6.0,
        Stroke::new(if selected { 3.0 } else { 2.0 }, kind_color(kind)),
        StrokeKind::Inside,
    );
    painter.text(
        b.center() - vec2(0.0, 9.0),
        Align2::CENTER_CENTER,
        kind.label(),
        FontId::proportional(14.0),
        visuals.text_color(),
    );
    painter.text(
        b.center() + vec2(0.0, 11.0),
        Align2::CENTER_CENTER,
        placed.component.duty_summary(stage.duty),
        FontId::proportional(12.0),
        visuals.weak_text_color(),
    );
    // Air state leaving this component.
    painter.text(
        Pos2::new(b.center().x, b.bottom() + 8.0),
        Align2::CENTER_TOP,
        state_text(&stage.out),
        FontId::proportional(11.0),
        visuals.weak_text_color(),
    );
}

/// The heat recovery as one tall box reaching across both rows.
fn recovery_box(
    ui: &Ui,
    rows: [Rect; 2],
    column: usize,
    recovery: &HeatRecovery,
    stage: &RecoveryStage,
    selection: &mut Option<Selection>,
) {
    let top = slot_rect(rows[DuctId::Supply as usize], column);
    let bottom = slot_rect(rows[DuctId::Extract as usize], column);
    let b = Rect::from_min_max(
        Pos2::new(top.left(), top.top() - 6.0),
        Pos2::new(top.right(), bottom.bottom() + 6.0),
    );
    let response = ui.interact(b, ui.id().with("recovery"), Sense::click());
    if response.clicked() {
        *selection = Some(Selection::Recovery);
    }
    let visuals = ui.visuals().clone();
    let painter = ui.painter();
    let width = if *selection == Some(Selection::Recovery) {
        3.0
    } else {
        2.0
    };
    painter.rect_filled(b, 6.0, visuals.window_fill);
    painter.rect_stroke(
        b,
        6.0,
        Stroke::new(width, RECOVERY_COLOR),
        StrokeKind::Inside,
    );
    painter.text(
        b.center() - vec2(0.0, 9.0),
        Align2::CENTER_CENTER,
        recovery.kind().label(),
        FontId::proportional(14.0),
        visuals.text_color(),
    );
    painter.text(
        b.center() + vec2(0.0, 11.0),
        Align2::CENTER_CENTER,
        format!("{:.1} kW", stage.exchange.duty.heat_kw),
        FontId::proportional(12.0),
        visuals.weak_text_color(),
    );
}
