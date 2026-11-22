//! Authored physical contacts, conductor recovery and post-weld initial state.
use super::*;
#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Terminal {
    Node(Node),
    Ground(Ground),
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub node: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ground {
    pub reference: GroundReference,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GroundReference {
    Ground,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortTerminals {
    pub positive: Terminal,
    pub negative: Terminal,
}
#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Contacts {
    #[default]
    Both,
    Positive,
    Negative,
    /// Positive of the first port and negative of the second.
    PositiveNegative,
    /// Negative of the first port and positive of the second.
    NegativePositive,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Potential {
    pub instance_id: String,
    pub node: String,
    pub index: Option<usize>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Current {
    pub instance_id: String,
    pub dof: String,
    pub index: usize,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recovery {
    pub mapped: bool,
    pub potentials: Vec<Potential>,
    pub currents: Vec<Current>,
}
impl Recovery {
    pub fn single(model: &Model, id: &str) -> Self {
        Self {
            mapped: model.port_terminals.is_some(),
            potentials: model
                .potential_nodes
                .iter()
                .enumerate()
                .map(|(i, n)| Potential {
                    instance_id: id.into(),
                    node: n.clone(),
                    index: Some(i),
                })
                .collect(),
            currents: model
                .current_dofs
                .iter()
                .enumerate()
                .map(|(i, n)| Current {
                    instance_id: id.into(),
                    dof: n.clone(),
                    index: model.potential_nodes.len() + i,
                })
                .collect(),
        }
    }
    pub fn initial(&self, value: &Value, n: usize) -> Result<(Vec<f64>, bool)> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Pv {
            instance_id: String,
            node: String,
            value: f64,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Ji {
            instance_id: String,
            dof: String,
            value: f64,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct State {
            kind: String,
            steady: bool,
            potentials_v: Vec<Pv>,
            currents_a: Vec<Ji>,
        }
        if value["potentialsV"]
            .as_array()
            .is_none_or(|v| v.len() != self.potentials.len())
            || value["currentsA"]
                .as_array()
                .is_none_or(|v| v.len() != self.currents.len())
        {
            return Err(fail(
                "COUPLED_INITIAL_CHARGE",
                "Complete bounded physical state required before cloning",
            ));
        }
        let state: State =
            serde_json::from_value(value.clone()).map_err(|e| fail("COUPLED_INITIAL", e))?;
        if state.kind != "physical-state"
            || state.potentials_v.len() != self.potentials.len()
            || state.currents_a.len() != self.currents.len()
        {
            return Err(fail(
                "COUPLED_INITIAL_CHARGE",
                "Complete physical node/current state required",
            ));
        }
        let mut x = vec![0.; n];
        let mut seen = vec![false; n];
        let mut names = BTreeSet::new();
        for v in state.potentials_v {
            if !v.value.is_finite() || !names.insert((v.instance_id.clone(), v.node.clone())) {
                return Err(fail("COUPLED_INITIAL", "Duplicate/nonfinite potential"));
            }
            let p = self
                .potentials
                .iter()
                .find(|p| p.instance_id == v.instance_id && p.node == v.node)
                .ok_or_else(|| fail("COUPLED_INITIAL", "Unknown physical node"))?;
            if let Some(i) = p.index {
                if seen[i] && (x[i] - v.value).abs() > 1e-12 {
                    return Err(fail(
                        "COUPLED_INITIAL",
                        "Initial potentials disagree across a physical weld; no averaging",
                    ));
                }
                if !seen[i] {
                    x[i] = v.value;
                    seen[i] = true;
                }
            } else if v.value != 0. {
                return Err(fail(
                    "COUPLED_INITIAL",
                    "Explicit ground weld requires zero potential",
                ));
            }
        }
        names.clear();
        for v in state.currents_a {
            if !v.value.is_finite() || !names.insert((v.instance_id.clone(), v.dof.clone())) {
                return Err(fail(
                    "COUPLED_INITIAL",
                    "Duplicate/nonfinite branch current",
                ));
            }
            let p = self
                .currents
                .iter()
                .find(|p| p.instance_id == v.instance_id && p.dof == v.dof)
                .ok_or_else(|| fail("COUPLED_INITIAL", "Unknown branch dof"))?;
            x[p.index] = v.value;
            seen[p.index] = true;
        }
        if seen.iter().any(|v| !*v) {
            return Err(fail("COUPLED_INITIAL_CHARGE", "Missing assembled state"));
        }
        Ok((x, state.steady))
    }
}
pub fn terminals(model: &Model, ports: usize) -> Result<Option<Vec<[Option<usize>; 2]>>> {
    let Some(map) = &model.port_terminals else {
        return Ok(None);
    };
    if map.len() != ports {
        return Err(fail(
            "COUPLED_TERMINAL_MAP",
            "One physical terminal pair per ordered port required",
        ));
    }
    let node = |t: &Terminal| -> Result<Option<usize>> {
        match t {
            Terminal::Node(t) => model
                .potential_nodes
                .iter()
                .position(|n| n == &t.node)
                .map(Some)
                .ok_or_else(|| fail("COUPLED_TERMINAL_MAP", "Unknown physical potential node")),
            Terminal::Ground(_) => {
                if model.reference_topology == "grounded" {
                    Ok(None)
                } else {
                    Err(fail(
                        "COUPLED_TERMINAL_MAP",
                        "Infinity has no implicit ground terminal",
                    ))
                }
            }
        }
    };
    let b = entries(&model.port_incidence, model.potential_nodes.len(), ports)?;
    let mut result = vec![];
    for (j, p) in map.iter().enumerate() {
        let pair = [node(&p.positive)?, node(&p.negative)?];
        if pair[0] == pair[1] {
            return Err(fail(
                "COUPLED_TERMINAL_MAP",
                "Distinct physical contacts required",
            ));
        }
        for i in 0..model.potential_nodes.len() {
            let expected = if pair[0] == Some(i) {
                1.
            } else if pair[1] == Some(i) {
                -1.
            } else {
                0.
            };
            if b.get(&(i, j)).copied().unwrap_or(0.) != expected {
                return Err(fail("COUPLED_TERMINAL_MAP","B differs from positive-minus-negative physical trace; reduced/general trace unavailable"));
            }
        }
        result.push(pair);
    }
    Ok(Some(result))
}
