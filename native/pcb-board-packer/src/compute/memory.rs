//! Backend-neutral memory budget. Driver estimates are not allocation guarantees.
use std::sync::Mutex;
use std::time::{Duration,Instant};

#[derive(Clone,Copy,Debug,serde::Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct Budget {
    pub budget_bytes:u64,
    pub usage_bytes:u64,
    pub source:&'static str,
}
impl Budget {
    pub fn headroom(self)->u64 {self.budget_bytes.saturating_sub(self.usage_bytes)}
    pub fn usable(self)->u64 {
        // Dynamic margin, not a fixed application quota. Protect against another
        // process allocating between the observation and our dispatch.
        self.headroom().saturating_sub(self.budget_bytes/20)
    }
}

pub(crate) trait BudgetSource: Send+Sync {
    fn read(&self)->Option<Budget>;
}

pub(crate) struct Monitor {
    source:Box<dyn BudgetSource>,
    cached:Mutex<Option<(Instant,Option<Budget>)>>,
}
impl Monitor {
    pub fn new(source:impl BudgetSource+'static)->Self {Self{source:Box::new(source),cached:Mutex::new(None)}}
    pub fn snapshot(&self)->Option<Budget> {
        let mut cached=self.cached.lock().unwrap_or_else(|e|e.into_inner());
        if let Some((at,value))=*cached {if at.elapsed()<Duration::from_millis(250){return value;}}
        let value=self.source.read();*cached=Some((Instant::now(),value));value
    }
    pub fn refresh(&self)->Option<Budget> {
        let mut cached=self.cached.lock().unwrap_or_else(|e|e.into_inner());
        let value=self.source.read();*cached=Some((Instant::now(),value));value
    }
    pub fn report(&self)->serde_json::Value {
        match self.snapshot() {
            Some(b)=>serde_json::json!({"known":true,"source":b.source,"budgetBytes":b.budget_bytes,
                "usageBytes":b.usage_bytes,"headroomBytes":b.headroom(),"usableHeadroomBytes":b.usable(),
                "estimated":true,"refreshIntervalMs":250}),
            None=>serde_json::json!({"known":false,"budgetBytes":null,"usageBytes":null,
                "headroomBytes":null,"usableHeadroomBytes":null,"estimated":true,"refreshIntervalMs":250}),
        }
    }
}

/// Free VRAM is an estimate, not a hard per-call capacity. Keep one candidate
/// admissible under pressure: cached allocator pages may satisfy it without a
/// new driver allocation. Actual allocation failures use whole-call recovery.
pub(super) fn batch_capacity(limit:usize,free:Option<u64>,reusable:usize,
    fixed:usize,per_candidate:usize,requested:usize)->Result<usize,super::Error> {
    let required=fixed.checked_add(per_candidate).ok_or_else(||
        super::Error::new(super::ErrorKind::InvalidInput,"GPU memory size overflow"))?;
    if required>limit {
        return Err(super::Error::new(super::ErrorKind::InvalidInput,
            "GPU minimum batch exceeds backend allocation limit"));
    }
    let budget=free.map_or(limit,|bytes|
        (bytes.min(usize::MAX as u64) as usize).saturating_add(reusable).min(limit));
    let capacity=budget.saturating_sub(fixed)/per_candidate.max(1);
    Ok(requested.max(1).min(capacity.max(1)))
}

/// Vulkan adapter implementation is private to the infrastructure, never used
/// by a solver. Other backends implement BudgetSource with their own APIs.
pub(super) struct VulkanBudget {adapter:wgpu::Adapter,supported:bool}
impl VulkanBudget {
    pub fn new(adapter:wgpu::Adapter)->Self {
        let supported=unsafe {adapter.as_hal::<wgpu::hal::api::Vulkan>().is_some_and(|hal| {
            hal.shared_instance().raw_instance().enumerate_device_extension_properties(hal.raw_physical_device())
                .ok().is_some_and(|extensions|extensions.iter().any(|e|
                    std::ffi::CStr::from_ptr(e.extension_name.as_ptr())==ash::ext::memory_budget::NAME))
        })};
        Self{adapter,supported}
    }
}
impl BudgetSource for VulkanBudget {
    fn read(&self)->Option<Budget> {
        if !self.supported{return None;}
        unsafe {
            let hal=self.adapter.as_hal::<wgpu::hal::api::Vulkan>()?;
            let mut budget=ash::vk::PhysicalDeviceMemoryBudgetPropertiesEXT::default();
            let mut properties=ash::vk::PhysicalDeviceMemoryProperties2::default().push_next(&mut budget);
            hal.shared_instance().raw_instance().get_physical_device_memory_properties2(hal.raw_physical_device(),&mut properties);
            // Do not add separate heaps together: one allocation cannot span
            // heaps. Use the largest device-local heap, not host/system memory.
            let memory=properties.memory_properties;
            let index=(0..memory.memory_heap_count as usize)
                .filter(|&i|memory.memory_heaps[i].flags.contains(ash::vk::MemoryHeapFlags::DEVICE_LOCAL))
                .max_by_key(|&i|memory.memory_heaps[i].size)?;
            if budget.heap_budget[index]==0{return None;}
            Some(Budget{budget_bytes:budget.heap_budget[index],usage_bytes:budget.heap_usage[index],source:"vulkan_memory_budget"})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_capacity_distinguishes_driver_pressure_from_hardware_limits() {
        assert_eq!(batch_capacity(1000,Some(800),0,200,100,10).unwrap(),6);
        assert_eq!(batch_capacity(1000,Some(0),0,200,100,10).unwrap(),1);
        assert_eq!(batch_capacity(1000,Some(0),600,200,100,10).unwrap(),4);
        assert_eq!(batch_capacity(1000,None,0,200,100,10).unwrap(),8);
        assert_eq!(batch_capacity(1000,Some(9000),0,200,100,10).unwrap(),8);
        let error=batch_capacity(1000,Some(9000),0,901,100,10).unwrap_err();
        assert_eq!(error.kind,super::super::ErrorKind::InvalidInput);
        assert!(!error.disables_runtime());
        assert!(batch_capacity(usize::MAX,None,0,usize::MAX,1,10).is_err());
    }
    #[test]
    fn budget_uses_driver_headroom_and_saturates_under_pressure() {
        let b=Budget{budget_bytes:1000,usage_bytes:200,source:"test"};
        assert_eq!(b.headroom(),800);assert_eq!(b.usable(),750);
        assert_eq!(Budget{usage_bytes:990,..b}.usable(),0);
        assert_eq!(Budget{usage_bytes:1200,..b}.headroom(),0);
    }
    struct Unknown;
    impl BudgetSource for Unknown {fn read(&self)->Option<Budget>{None}}
    #[test]
    fn unknown_budget_is_never_reported_as_free_vram() {
        let report=Monitor::new(Unknown).report();
        assert_eq!(report["known"],false);assert!(report["headroomBytes"].is_null());
    }
}

pub(super) struct CudaBudget(pub std::sync::Arc<cudarc::driver::CudaContext>);
impl BudgetSource for CudaBudget {
 fn read(&self)->Option<Budget> {
  let (free,total)=self.0.mem_get_info().ok()?;
  Some(Budget{budget_bytes:total as u64,usage_bytes:total.saturating_sub(free) as u64,source:"cuda_mem_get_info"})
 }
}
