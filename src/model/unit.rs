//! The air handler unit: two ducts, each an ordered chain of components, optionally coupled
//! by one heat recovery.

use super::air::AirState;
use super::component::{Component, ComponentKind, Duty};
use super::recovery::{Exchange, HeatRecovery, HeatRecoveryKind};

/// Stable identity of a placed component, independent of its position in the duct.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
pub struct ComponentId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
pub enum DuctId {
    /// Outdoor air to supply air.
    Supply,
    /// Extract air to exhaust air.
    Extract,
}

impl DuctId {
    pub const ALL: [Self; 2] = [Self::Supply, Self::Extract];

    /// Name of the air entering the duct.
    pub fn inlet_label(self) -> &'static str {
        match self {
            Self::Supply => "Outdoor air",
            Self::Extract => "Extract air",
        }
    }

    /// Name of the air leaving the duct.
    pub fn outlet_label(self) -> &'static str {
        match self {
            Self::Supply => "Supply air",
            Self::Extract => "Exhaust air",
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Placed {
    pub id: ComponentId,
    pub component: Component,
}

/// Result of one component in a duct, as computed by [`AirHandlerUnit::simulate`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    pub id: ComponentId,
    pub out: AirState,
    pub duty: Duty,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Duct {
    pub inlet_temp_c: f64,
    pub inlet_rel_humidity: f64,
    pub flow_m3h: f64,
    pub components: Vec<Placed>,
}

impl Duct {
    fn new(inlet_temp_c: f64, inlet_rel_humidity: f64, flow_m3h: f64) -> Self {
        Self {
            inlet_temp_c,
            inlet_rel_humidity,
            flow_m3h,
            components: Vec::new(),
        }
    }

    pub fn inlet(&self) -> AirState {
        AirState::from_rel_humidity(self.inlet_temp_c, self.inlet_rel_humidity, self.flow_m3h)
    }
}

/// The heat recovery and where it sits: the number of components before it in each duct.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PlacedRecovery {
    pub recovery: HeatRecovery,
    supply_pos: usize,
    extract_pos: usize,
}

impl PlacedRecovery {
    fn pos_mut(&mut self, duct: DuctId) -> &mut usize {
        match duct {
            DuctId::Supply => &mut self.supply_pos,
            DuctId::Extract => &mut self.extract_pos,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default)] // new fields fall back to defaults when loading older persisted state
pub struct AirHandlerUnit {
    supply: Duct,
    extract: Duct,
    recovery: Option<PlacedRecovery>,
    next_id: u64,
}

impl Default for AirHandlerUnit {
    fn default() -> Self {
        Self {
            supply: Duct::new(-5.0, 80.0, 3000.0),
            extract: Duct::new(22.0, 40.0, 3000.0),
            recovery: None,
            next_id: 0,
        }
    }
}

/// Result of simulating one duct.
#[derive(Clone, Debug, PartialEq)]
pub struct DuctResult {
    /// One entry per component, in duct order.
    pub stages: Vec<Stage>,
    /// Air leaving the duct (supply air or exhaust air).
    pub outlet: AirState,
}

/// What entered and left the heat recovery.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecoveryStage {
    pub supply_in: AirState,
    pub extract_in: AirState,
    pub exchange: Exchange,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Simulation {
    pub supply: DuctResult,
    pub extract: DuctResult,
    pub recovery: Option<RecoveryStage>,
}

impl Simulation {
    pub fn duct(&self, id: DuctId) -> &DuctResult {
        match id {
            DuctId::Supply => &self.supply,
            DuctId::Extract => &self.extract,
        }
    }

    pub fn stage(&self, id: ComponentId) -> Option<Stage> {
        self.supply
            .stages
            .iter()
            .chain(&self.extract.stages)
            .find(|stage| stage.id == id)
            .copied()
    }
}

/// Runs `state` through `components`, appending one stage each. Returns the final state.
fn run(components: &[Placed], mut state: AirState, stages: &mut Vec<Stage>) -> AirState {
    for placed in components {
        let (out, duty) = placed.component.apply(state);
        stages.push(Stage {
            id: placed.id,
            out,
            duty,
        });
        state = out;
    }
    state
}

impl AirHandlerUnit {
    pub fn duct(&self, id: DuctId) -> &Duct {
        match id {
            DuctId::Supply => &self.supply,
            DuctId::Extract => &self.extract,
        }
    }

    pub fn duct_mut(&mut self, id: DuctId) -> &mut Duct {
        match id {
            DuctId::Supply => &mut self.supply,
            DuctId::Extract => &mut self.extract,
        }
    }

    pub fn recovery(&self) -> Option<&HeatRecovery> {
        self.recovery.as_ref().map(|placed| &placed.recovery)
    }

    pub fn recovery_mut(&mut self) -> Option<&mut HeatRecovery> {
        self.recovery.as_mut().map(|placed| &mut placed.recovery)
    }

    /// Number of components in `duct` that come before the heat recovery.
    pub fn recovery_position(&self, duct: DuctId) -> Option<usize> {
        let placed = self.recovery.as_ref()?;
        let pos = match duct {
            DuctId::Supply => placed.supply_pos,
            DuctId::Extract => placed.extract_pos,
        };
        Some(pos.min(self.duct(duct).components.len()))
    }

    /// Inserts a new component of `kind` at `index` (clamped to the duct length).
    ///
    /// `before_recovery` says on which side of the heat recovery it lands. It only matters
    /// when `index` equals the recovery position, where both sides are the same list index.
    pub fn add(
        &mut self,
        duct: DuctId,
        index: usize,
        before_recovery: bool,
        kind: ComponentKind,
    ) -> ComponentId {
        let id = ComponentId(self.next_id);
        self.next_id += 1;
        let components = &mut self.duct_mut(duct).components;
        let index = index.min(components.len());
        components.insert(
            index,
            Placed {
                id,
                component: kind.instantiate(),
            },
        );
        if before_recovery && let Some(placed) = &mut self.recovery {
            *placed.pos_mut(duct) += 1;
        }
        id
    }

    pub fn remove(&mut self, id: ComponentId) {
        for duct in DuctId::ALL {
            let position = self.duct(duct).components.iter().position(|p| p.id == id);
            let Some(index) = position else { continue };
            self.duct_mut(duct).components.remove(index);
            if let Some(placed) = &mut self.recovery {
                let pos = placed.pos_mut(duct);
                if index < *pos {
                    *pos -= 1;
                }
            }
        }
    }

    /// Places a heat recovery of `kind`, replacing any existing one. `index` is the number of
    /// components before it in `duct`; the other duct gets the same count, clamped to its length.
    pub fn place_recovery(&mut self, kind: HeatRecoveryKind, duct: DuctId, index: usize) {
        let other = match duct {
            DuctId::Supply => DuctId::Extract,
            DuctId::Extract => DuctId::Supply,
        };
        let mut placed = PlacedRecovery {
            recovery: kind.instantiate(),
            supply_pos: 0,
            extract_pos: 0,
        };
        *placed.pos_mut(duct) = index.min(self.duct(duct).components.len());
        *placed.pos_mut(other) = index.min(self.duct(other).components.len());
        self.recovery = Some(placed);
    }

    pub fn remove_recovery(&mut self) {
        self.recovery = None;
    }

    pub fn component_mut(&mut self, id: ComponentId) -> Option<&mut Component> {
        self.supply
            .components
            .iter_mut()
            .chain(self.extract.components.iter_mut())
            .find(|placed| placed.id == id)
            .map(|placed| &mut placed.component)
    }

    /// Runs both airstreams through their components and the heat recovery.
    ///
    /// Each duct is simulated up to the heat recovery, the two streams exchange heat there,
    /// and each duct then continues with the treated air.
    pub fn simulate(&self) -> Simulation {
        let split = |duct: DuctId| {
            self.recovery_position(duct)
                .unwrap_or_else(|| self.duct(duct).components.len())
        };
        let (supply_split, extract_split) = (split(DuctId::Supply), split(DuctId::Extract));

        let mut supply_stages = Vec::new();
        let mut extract_stages = Vec::new();
        let supply_mid = run(
            &self.supply.components[..supply_split],
            self.supply.inlet(),
            &mut supply_stages,
        );
        let extract_mid = run(
            &self.extract.components[..extract_split],
            self.extract.inlet(),
            &mut extract_stages,
        );

        let (recovery, supply_state, extract_state) = match &self.recovery {
            Some(placed) => {
                let exchange = placed.recovery.exchange(supply_mid, extract_mid);
                let stage = RecoveryStage {
                    supply_in: supply_mid,
                    extract_in: extract_mid,
                    exchange,
                };
                (Some(stage), exchange.supply_out, exchange.extract_out)
            }
            None => (None, supply_mid, extract_mid),
        };

        let supply_out = run(
            &self.supply.components[supply_split..],
            supply_state,
            &mut supply_stages,
        );
        let extract_out = run(
            &self.extract.components[extract_split..],
            extract_state,
            &mut extract_stages,
        );
        Simulation {
            supply: DuctResult {
                stages: supply_stages,
                outlet: supply_out,
            },
            extract: DuctResult {
                stages: extract_stages,
                outlet: extract_out,
            },
            recovery,
        }
    }

    /// The simulation result for one component, wherever it sits.
    pub fn stage(&self, id: ComponentId) -> Option<Stage> {
        self.simulate().stage(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_inserts_in_order_and_clamps_index() {
        let mut unit = AirHandlerUnit::default();
        let a = unit.add(DuctId::Supply, 0, false, ComponentKind::Heater);
        let b = unit.add(DuctId::Supply, 0, false, ComponentKind::Cooler);
        let c = unit.add(DuctId::Supply, 99, false, ComponentKind::Fan);
        let ids: Vec<_> = unit
            .duct(DuctId::Supply)
            .components
            .iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(ids, [b, a, c]);
    }

    #[test]
    fn remove_and_lookup() {
        let mut unit = AirHandlerUnit::default();
        let id = unit.add(DuctId::Extract, 0, false, ComponentKind::Humidifier);
        assert!(unit.component_mut(id).is_some(), "component should exist");
        unit.remove(id);
        assert!(unit.component_mut(id).is_none(), "component should be gone");
        assert!(unit.stage(id).is_none(), "stage should be gone");
    }

    #[test]
    fn stages_chain_outlet_to_inlet() {
        let mut unit = AirHandlerUnit::default();
        unit.add(DuctId::Supply, 0, false, ComponentKind::Heater);
        unit.add(DuctId::Supply, 1, false, ComponentKind::Humidifier);
        let sim = unit.simulate();
        let stages = &sim.supply.stages;
        assert!(
            (stages[0].out.temp_c - 21.0).abs() < 1e-9,
            "heater setpoint"
        );
        assert!(
            (stages[1].out.rel_humidity() - 45.0).abs() < 1e-6,
            "humidified at heated temp"
        );
    }

    #[test]
    fn recovery_position_follows_inserts_and_removals() {
        let mut unit = AirHandlerUnit::default();
        let first = unit.add(DuctId::Supply, 0, false, ComponentKind::Heater);
        unit.place_recovery(HeatRecoveryKind::PlateExchanger, DuctId::Supply, 1);
        assert_eq!(unit.recovery_position(DuctId::Supply), Some(1));
        assert_eq!(
            unit.recovery_position(DuctId::Extract),
            Some(0),
            "clamped to the empty duct"
        );

        unit.add(DuctId::Supply, 1, true, ComponentKind::Cooler); // before the recovery
        assert_eq!(unit.recovery_position(DuctId::Supply), Some(2));
        unit.add(DuctId::Supply, 2, false, ComponentKind::Fan); // after the recovery
        assert_eq!(unit.recovery_position(DuctId::Supply), Some(2));

        unit.remove(first);
        assert_eq!(unit.recovery_position(DuctId::Supply), Some(1));
    }

    #[test]
    fn components_before_recovery_precondition_the_exchange() {
        let mut unit = AirHandlerUnit::default();
        unit.place_recovery(HeatRecoveryKind::PlateExchanger, DuctId::Supply, 0);
        let cold = unit.simulate();

        unit.place_recovery(HeatRecoveryKind::PlateExchanger, DuctId::Supply, 1);
        unit.add(DuctId::Supply, 0, true, ComponentKind::Heater);
        let preheated = unit.simulate();

        let cold_in = cold.recovery.map(|r| r.supply_in.temp_c);
        let warm_in = preheated.recovery.map(|r| r.supply_in.temp_c);
        assert_eq!(cold_in, Some(-5.0));
        assert_eq!(warm_in, Some(21.0), "heater runs before the recovery");
    }
}
