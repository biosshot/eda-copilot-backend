use crate::compute::f32_runtime::PcbRuntime as WgpuRuntime;
use cubecl::{client::ComputeClient, server::Handle};
use std::collections::BTreeMap;

/// A consumer-owned layout name and a slot local to that layout. The runtime
/// knows neither domain buffer formats nor meanings of slot numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ScratchKey {
    layout: &'static str,
    slot: usize,
}

impl ScratchKey {
    pub const fn new(layout: &'static str, slot: usize) -> Self {
        Self { layout, slot }
    }
}

#[derive(Default)]
pub(super) struct Workspace {
    buffers: BTreeMap<ScratchKey, (usize, Handle)>,
}

impl Workspace {
    pub fn buffer(
        &mut self,
        client: &ComputeClient<WgpuRuntime>,
        key: ScratchKey,
        size: usize,
    ) -> Handle {
        let entry = self.buffers.entry(key).or_insert_with(|| {
            let capacity = size.max(8).next_power_of_two();
            (capacity, client.empty(capacity))
        });
        if entry.0 < size.max(8) {
            let capacity = size.max(8).next_power_of_two();
            *entry = (capacity, client.empty(capacity));
        }
        entry.1.clone()
    }

    pub fn bytes(&self) -> usize {
        self.buffers.values().map(|(capacity, _)| capacity).sum()
    }
}
