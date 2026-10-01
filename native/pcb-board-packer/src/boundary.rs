//! Binary64 is confined to this transport adapter, before creating F32 models.
//! Geometry, scores and legality never run here.
use serde_json::{Map, Value};
use std::collections::BTreeMap;

const ABSOLUTE_LIMIT: f64 = 1_000_000_000.0;
const LOCAL_LIMIT: f64 = 1024.0;

pub(crate) struct Frame {
    origin: [f64; 2],
    locked: BTreeMap<String, Value>,
    originals: BTreeMap<String, Value>,
    centered_output: bool,
}

fn local_field(key: &str) -> bool {
    matches!(key, "orientations" | "normal" | "offset" | "anchorOffset"
        | "boardOverflow" | "overflow")
}

/// All points and boxes in these native contracts are absolute except the
/// explicitly named local subtrees above. Distances and board overflow are
/// lengths, not box coordinates. IDs, integer angles and counts are untouched.
fn coordinates(value: &mut Value, absolute: bool, visit: &mut impl FnMut(&mut Value, usize) -> Result<(), String>) -> Result<(), String> {
    match value {
        Value::Array(items) => for item in items { coordinates(item, absolute, visit)?; },
        Value::Object(object) => {
            let point = absolute && (object.contains_key("x") || object.contains_key("y"));
            let box_ = absolute && ["left", "right", "top", "bottom"].iter().all(|key| object.contains_key(*key));
            for (key, item) in object.iter_mut() {
                if (point && matches!(key.as_str(), "x" | "y")) || (box_ && matches!(key.as_str(), "left" | "right" | "top" | "bottom")) {
                    if item.is_number() { visit(item, usize::from(matches!(key.as_str(), "y" | "top" | "bottom")))?; }
                } else { coordinates(item, absolute && !local_field(key), visit)?; }
            }
        },
        _ => (),
    }
    Ok(())
}

fn remember_placements(value: &Value, locked: &mut BTreeMap<String, Value>) {
    if let Some(items) = value.as_array() {
        for item in items {
            if let Some(designator) = item["designator"].as_str() {
                locked.insert(designator.to_owned(), item.clone());
            }
        }
    }
}

impl Frame {
    pub(crate) fn localize(value: &mut Value) -> Result<Self, String> {
        let mut locked = BTreeMap::new();
        let mut originals = BTreeMap::new();
        remember_placements(&value["placements"], &mut originals);
        if let Some(primitives) = value["primitives"].as_array() {
            for primitive in primitives {
                remember_placements(&primitive["placements"], &mut originals);
                if primitive["locked"] == true { remember_placements(&primitive["placements"], &mut locked); }
            }
        }
        if let (Some(components), Some(placements)) = (value["components"].as_array(), value["placements"].as_array()) {
            for (index, component) in components.iter().enumerate() {
                let explicitly_refinable = value["groups"].as_array().is_some_and(|groups| groups.iter().any(|g|
                    (g["rotate"] == true || g["swap"] == true) && g["members"].as_array().is_some_and(|members| members.iter().any(|m| m.as_u64() == Some(index as u64)))));
                if component["fixed"] == true && !explicitly_refinable {
                    if let Some(pose) = component["poseIndex"].as_u64().and_then(|i| placements.get(i as usize)) {
                        if let Some(designator) = pose["designator"].as_str() { locked.insert(designator.to_owned(), pose.clone()); }
                    }
                }
            }
        }
        let mut min = [f64::INFINITY; 2]; let mut max = [f64::NEG_INFINITY; 2];
        coordinates(value, true, &mut |item, axis| {
            let number = item.as_f64().ok_or("invalid coordinate")?;
            if !number.is_finite() || number.abs() > ABSOLUTE_LIMIT { return Err("PCB absolute coordinate exceeds the documented 1e9 mm frame".into()); }
            min[axis] = min[axis].min(number); max[axis] = max[axis].max(number); Ok(())
        })?;
        let mut origin = [0.0; 2];
        let bounds = [&value["fullBoardBounds"], &value["board"], &value["bounds"],
            &value["world"]["bounds"], &value["routeProblem"]["fullBoardBounds"]]
            .into_iter().find(|b| ["left","right","top","bottom"].iter().all(|key| b[*key].is_number()));
        for axis in 0..2 {
            if let Some(bounds) = bounds {
                let (lo,hi) = if axis == 0 { ("left","right") } else { ("top","bottom") };
                origin[axis] = ((bounds[lo].as_f64().unwrap()+bounds[hi].as_f64().unwrap())*0.5).round();
            } else if min[axis].is_finite() {
                // Keep a centered board's frame at zero, and preserve grid phase.
                origin[axis] = ((min[axis] + max[axis]) * 0.5).round();
            }
        }
        let frame = Self { origin, locked, originals, centered_output: false };
        frame.localize_related(value)?;
        Ok(frame)
    }

    pub(crate) fn localize_block(value: &mut Value) -> Result<Self, String> {
        let centered = value["bounds"].is_null() && value["world"].is_null()
            && value["obstacles"].as_array().is_none_or(|items| items.is_empty())
            && value["primitives"].as_array().is_none_or(|items| items.iter().all(|p| p["locked"] != true));
        // A resumed free block's seed is already in the centered destination
        // frame; its source templates can still have a large authored offset.
        let seed = if centered { value.as_object_mut().and_then(|v|v.remove("pairSeed")) } else { None };
        let result = Self::localize(value);
        if let Some(seed) = seed { value["pairSeed"] = seed; }
        let mut frame = result?; frame.centered_output = centered; Ok(frame)
    }

    pub(crate) fn localize_related(&self, value: &mut Value) -> Result<(), String> {
        coordinates(value, true, &mut |item, axis| {
            let absolute = item.as_f64().ok_or("invalid coordinate")?;
            let local = absolute - self.origin[axis]; // MUST precede narrowing.
            if !absolute.is_finite() || absolute.abs() > ABSOLUTE_LIMIT || local.abs() > LOCAL_LIMIT {
                return Err("PCB coordinate exceeds the documented absolute/local frame; a wider frame requires explicit validation".into());
            }
            // This is still JSON transport; deserialization creates the F32 DTO.
            *item = Value::from(local); Ok(())
        })
    }

    pub(crate) fn restore(&self, value: &mut Value) -> Result<(), String> {
        if self.centered_output {
            fn translations(value: &mut Value, origin: [f64;2]) {
                match value {
                    Value::Array(items) => for item in items { translations(item,origin); },
                    Value::Object(object) => for (key,item) in object.iter_mut() {
                        if matches!(key.as_str(),"translationX"|"translationY") && item.is_number() {
                            let axis = usize::from(key == "translationY");
                            *item = Value::from(item.as_f64().unwrap()-origin[axis]);
                        } else { translations(item,origin); }
                    },
                    _ => (),
                }
            }
            translations(value,self.origin);
            return Ok(());
        }
        coordinates(value, true, &mut |item, axis| {
            let local = item.as_f64().ok_or("invalid output coordinate")?;
            if !local.is_finite() { return Err("nonfinite PCB output coordinate".into()); }
            *item = Value::from(local + self.origin[axis]); Ok(())
        })?;
        self.restore_originals(value)?;
        Ok(())
    }

    fn restore_originals(&self, value: &mut Value) -> Result<(), String> {
        match value {
            Value::Array(items) => for item in items { self.restore_originals(item)?; },
            Value::Object(object) => {
                if object.contains_key("x") && object.contains_key("y") {
                    if let Some((id, original)) = object.get("designator").and_then(Value::as_str)
                        .and_then(|id| self.originals.get(id).map(|pose|(id,pose))) {
                        let unchanged = ["x","y"].iter().enumerate().all(|(axis,key)| {
                            let before = original[*key].as_f64().unwrap() - self.origin[axis];
                            let after = object[*key].as_f64().unwrap() - self.origin[axis];
                            before as f32 == after as f32
                        }) && ["rotate","layer"].iter().all(|key| object.get(*key)==original.get(*key));
                        if !unchanged && self.locked.contains_key(id) { return Err(format!("native solver moved locked placement {id}")); }
                        if unchanged {
                            for key in ["x", "y", "rotate", "layer"] {
                                if let Some(v) = original.get(key) { object.insert(key.into(), v.clone()); }
                            }
                        }
                    }
                }
                for item in object.values_mut() { self.restore_originals(item)?; }
            },
            _ => (),
        }
        Ok(())
    }

    pub(crate) fn metadata(&self) -> Value {
        let mut out = Map::new(); out.insert("origin".into(), serde_json::json!({"x":self.origin[0],"y":self.origin[1]}));
        out.insert("outputOrigin".into(), if self.centered_output { serde_json::json!({"x":0.0,"y":0.0}) }
            else { serde_json::json!({"x":self.origin[0],"y":self.origin[1]}) });
        out.insert("precision".into(), Value::from("f32-rte-ftz-v1")); Value::Object(out)
    }

    pub(crate) fn finish_block(&self, mut output: Value) -> Result<Value, String> {
        self.restore(&mut output)?;
        output["numericFrame"] = self.metadata();
        if let Some(checkpoints) = output["checkpoints"].as_array_mut() {
            for checkpoint in checkpoints { checkpoint["numericFrame"] = self.metadata(); }
        }
        Ok(output)
    }

    /// Opt-in evidence at the actual typed boundary, not the pre-narrowing JS
    /// object. Capture occurs before a timed solver starts and has no role in
    /// candidate order, scoring or cache keys. Never overwrite prior evidence.
    pub(crate) fn capture(&self, kind: &str, problem: &impl serde::Serialize) {
        let Some(directory)=std::env::var_os("PCB_F32_NATIVE_CAPTURE_DIR") else {return;};
        static SEQUENCE: std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
        let sequence=SEQUENCE.fetch_add(1,std::sync::atomic::Ordering::Relaxed);
        let result=(|| -> Result<(),Box<dyn std::error::Error>> {
            use std::io::Write;
            let directory=std::path::PathBuf::from(directory);std::fs::create_dir_all(&directory)?;
            let file=directory.join(format!("{kind}-{}-{sequence:06}.canonical.json",std::process::id()));
            let mut output=std::fs::OpenOptions::new().write(true).create_new(true).open(file)?;
            serde_json::to_writer_pretty(&mut output,&serde_json::json!({"schema":"pcb-f32-canonical-v1",
                "kind":kind,"problem":problem,"numericFrame":self.metadata(),
                "originalPlacements":self.originals,"lockedPlacements":self.locked}))?;
            output.write_all(b"\n")?;Ok(())
        })();
        if let Err(error)=result {eprintln!("[pcb-f32-capture] failed: {error}");}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn subtracts_before_narrowing_and_keeps_local_vectors() {
        for shift in [0.0, 1e6, -1e6, 1e9-1.0, -1e9+1.0] {
            let mut input = json!({"board":{"left":shift-0.25,"right":shift+0.25,"top":shift-0.25,"bottom":shift+0.25},
                "placements":[{"x":shift,"y":shift},{"x":shift+0.001,"y":shift+0.00025}],
                "normal":{"x":1.0,"y":0.0},"offset":{"x":0.001,"y":0.002},
                "overflow":{"left":0.2,"right":0.0,"top":0.0,"bottom":0.0},
                "orientations":[{"box":{"left":-1.0,"right":1.0,"top":-1.0,"bottom":1.0}}]});
            let original = input.clone(); let frame = Frame::localize(&mut input).unwrap();
            let a = input["placements"][0]["x"].as_f64().unwrap() as f32;
            let b = input["placements"][1]["x"].as_f64().unwrap() as f32;
            assert_ne!(a,b); assert!((b-0.001).abs() < 1e-6);
            assert_eq!(input["normal"],original["normal"]);
            assert_eq!(input["offset"],original["offset"]);
            assert_eq!(input["overflow"],original["overflow"]);
            assert_eq!(input["orientations"],original["orientations"]);
            frame.restore(&mut input).unwrap(); assert_eq!(input,original);
        }
    }
    #[test]
    fn preserves_authored_locked_pose_and_rejects_oversized_local_extent() {
        let pose = json!({"designator":"J1","x":1000000.001234567,"y":1000000.000001,"rotate":90,"layer":"bottom"});
        let mut input = json!({"primitives":[{"locked":true,"placements":[pose.clone()]}]});
        let frame = Frame::localize(&mut input).unwrap();
        input["primitives"][0]["placements"][0]["x"] = Value::from(input["primitives"][0]["placements"][0]["x"].as_f64().unwrap() as f32);
        frame.restore(&mut input).unwrap(); assert_eq!(input["primitives"][0]["placements"][0],pose);
        assert!(Frame::localize(&mut json!({"bounds":{"left":-1025.0,"right":1025.0,"top":0.0,"bottom":1.0}})).is_err());
    }
}
