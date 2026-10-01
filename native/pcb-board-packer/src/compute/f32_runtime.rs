//! Enforce the PCB float contract on every compiled CubeCL SPIR-V kernel.
//! Delegates all execution/memory to the existing WgpuServer and one device.
use std::sync::Arc;
use cubecl::{client::ComputeClient, device::{DeviceId, DeviceService, ServerUtilitiesHandle},
    future::DynFut, ir::{StorageType, TargetProperties}, prelude::*, Runtime,
    stream_id::StreamId, wgpu::{AutoCompiler, AutoRepresentation, WgpuDevice, WgpuRuntime, WgpuServer},
    CompilationError, CubeTask, WgpuCompilationOptions};
use cubecl_runtime::{
    allocator::ContiguousMemoryLayoutPolicy,
    logging::ServerLogger, memory_management::{ManagedMemoryHandle, MemoryUsage, MemoryAllocationMode},
    server::{ComputeServer, ServerCommunication, ServerUtilities, Binding, CopyDescriptor,
        KernelArguments, ServerError, ProfilingToken, ProfileError},
    storage::{ComputeStorage, ManagedResource}};
use cubecl::{profile::ProfileDuration, bytes::Bytes};

#[derive(Clone, Debug)]
pub(crate) struct PcbRuntime;
#[derive(Debug)]
pub(crate) struct PcbServer {
    inner: WgpuServer,
    utilities: Arc<ServerUtilities<Self>>,
}
impl DeviceService for PcbServer {
    fn init(id: DeviceId) -> Self {
        let inner = <WgpuServer as DeviceService>::init(id);
        let source = <WgpuServer as ComputeServer>::utilities(&inner);
        let utilities = ServerUtilities::new(source.properties.clone(), source.logger.clone(), source.info,
            ContiguousMemoryLayoutPolicy::new(source.properties.memory.alignment as usize));
        Self { inner, utilities: Arc::new(utilities) }
    }
    fn utilities(&self) -> ServerUtilitiesHandle { self.utilities.clone() }
}
impl ServerCommunication for PcbServer { const SERVER_COMM_ENABLED: bool = false; }
impl ComputeServer for PcbServer {
    type Kernel = Box<dyn CubeTask<AutoCompiler>>;
    type Info = <WgpuServer as ComputeServer>::Info;
    type Storage = <WgpuServer as ComputeServer>::Storage;
    type MemoryLayoutPolicy = ContiguousMemoryLayoutPolicy;
    fn initialize_memory(&mut self,m:ManagedMemoryHandle,n:u64,s:StreamId) { self.inner.initialize_memory(m,n,s); }
    fn staging(&mut self,n:&[usize],s:StreamId)->Result<Vec<Bytes>,ServerError> {self.inner.staging(n,s)}
    fn logger(&self)->Arc<ServerLogger> { self.inner.logger() }
    fn utilities(&self)->Arc<ServerUtilities<Self>> { self.utilities.clone() }
    fn read(&mut self,d:Vec<CopyDescriptor>,s:StreamId)->DynFut<Result<Vec<Bytes>,ServerError>> {self.inner.read(d,s)}
    fn write(&mut self,d:Vec<(CopyDescriptor,Bytes)>,s:StreamId) {self.inner.write(d,s)}
    fn sync(&mut self,s:StreamId)->DynFut<Result<(),ServerError>> {self.inner.sync(s)}
    fn get_resource(&mut self,b:Binding,s:StreamId)->Result<ManagedResource<<Self::Storage as ComputeStorage>::Resource>,ServerError> {self.inner.get_resource(b,s)}
    unsafe fn launch(&mut self,k:Self::Kernel,n:CubeCount,b:KernelArguments,m:ExecutionMode,s:StreamId) {
        self.inner.launch(Box::new(StrictTask(k)),n,b,m,s);
    }
    fn flush(&mut self,s:StreamId)->Result<(),ServerError> {self.inner.flush(s)}
    fn memory_usage(&mut self,s:StreamId)->Result<MemoryUsage,ServerError> {self.inner.memory_usage(s)}
    fn memory_cleanup(&mut self,s:StreamId) {self.inner.memory_cleanup(s)}
    fn start_profile(&mut self,s:StreamId)->Result<ProfilingToken,ServerError> {self.inner.start_profile(s)}
    fn end_profile(&mut self,s:StreamId,t:ProfilingToken)->Result<ProfileDuration,ProfileError> {self.inner.end_profile(s,t)}
    fn allocation_mode(&mut self,m:MemoryAllocationMode,s:StreamId) {self.inner.allocation_mode(m,s)}
}
impl Runtime for PcbRuntime {
    type Compiler=AutoCompiler; type Server=PcbServer; type Device=WgpuDevice;
    fn client(d:&WgpuDevice)->ComputeClient<Self> {ComputeClient::load(d)}
    fn name(_: &ComputeClient<Self>)->&'static str {"wgpu<spirv,pcb-f32-strict>"}
    fn max_cube_count()->(u32,u32,u32) {WgpuRuntime::max_cube_count()}
    fn can_read_tensor(s:&cubecl::zspace::Shape,t:&cubecl::zspace::Strides)->bool {WgpuRuntime::can_read_tensor(s,t)}
    fn target_properties()->TargetProperties {WgpuRuntime::target_properties()}
    fn enumerate_devices(t:u16,i:&wgpu::Backend)->Vec<DeviceId> {WgpuRuntime::enumerate_devices(t,i)}
    fn enumerate_all_devices(i:&wgpu::Backend)->Vec<DeviceId> {WgpuRuntime::enumerate_all_devices(i)}
}
struct StrictTask(Box<dyn CubeTask<AutoCompiler>>);
impl KernelMetadata for StrictTask {
    fn name(&self)->&'static str {self.0.name()}
    fn id(&self)->KernelId {let original=self.0.id(); original.clone().info((original,"pcb-f32-rte-explicit-ftz-no-contraction-v4"))}
    fn address_type(&self)->StorageType {self.0.address_type()}
}
impl CubeTask<AutoCompiler> for StrictTask {
    fn compile(&self,c:&mut AutoCompiler,o:&WgpuCompilationOptions,m:ExecutionMode,a:StorageType)->Result<CompiledKernel<AutoCompiler>,CompilationError> {
        let mut result=self.0.compile(c,o,m,a)?;
        let Some(AutoRepresentation::SpirV(ref mut kernel))=result.repr else {
            panic!("PCB F32 requires SPIR-V execution modes");
        };
        kernel.assembled_module=enforce_modes(&kernel.assembled_module);
        // The compiler's unmodified disassembly must not be reported as emitted IR.
        kernel.module=None;
        result.source="PCB F32 SPIR-V: RTE, explicit FTZ, SignedZeroInfNanPreserve, NoContraction".into();
        if let Some(path)=std::env::var_os("PCB_F32_SHADER_AUDIT_DIR") {
            let path=std::path::PathBuf::from(path);std::fs::create_dir_all(&path).expect("shader audit directory");
            let bytes:Vec<u8>=kernel.assembled_module.iter().flat_map(|w|w.to_le_bytes()).collect();
            std::fs::write(path.join(format!("{}-{:x}.spv",result.entrypoint_name,self.id().stable_hash())),bytes).expect("shader audit output");
        }
        Ok(result)
    }
}

/// Amend assembled SPIR-V using its stable binary instruction layout, without
/// introducing a second compiler/parser dependency. Reject F64 and fast-math.
fn enforce_modes(words: &[u32]) -> Vec<u32> {
    use std::collections::BTreeMap;
    fn emit(out: &mut Vec<u32>, op: u32, args: &[u32]) {
        out.push((((args.len()+1) as u32)<<16)|op); out.extend_from_slice(args);
    }
    assert!(words.len() >= 5 && words[0] == 0x07230203, "invalid SPIR-V header");
    let mut instructions = Vec::new(); let mut offset = 5;
    let mut entries = Vec::new(); let mut float_shapes = BTreeMap::new();
    let mut glsl = None;
    while offset < words.len() {
        let length = (words[offset] >> 16) as usize; let opcode = words[offset] & 0xffff;
        assert!(length > 0 && offset+length <= words.len(), "invalid SPIR-V instruction");
        let instruction = words[offset..offset+length].to_vec();
        assert_ne!(opcode,4427,"explicit core Fma is not part of the PCB contract");
        if opcode == 15 { entries.push(instruction[2]); }
        if opcode == 22 {
            assert_eq!(instruction[2], 32, "non-F32 shader in PCB solver");
            float_shapes.insert(instruction[1], 1u32);
        }
        if opcode == 23 && float_shapes.contains_key(&instruction[2]) {
            float_shapes.insert(instruction[1], instruction[3]);
        }
        if opcode == 11 {
            let bytes: Vec<u8> = instruction[2..].iter().flat_map(|w|w.to_le_bytes()).collect();
            if bytes.starts_with(b"GLSL.std.450\0") { glsl=Some(instruction[1]); }
        }
        if opcode == 12 && length >= 6 && Some(instruction[3]) == glsl {
            assert_ne!(instruction[4],50,"explicit GLSL Fma is not part of the PCB contract");
        }
        if opcode == 71 && length >= 4 {
            assert_ne!(instruction[2], 40, "unexpected FPFastMathMode");
            assert_ne!(instruction[2], 0, "unexpected RelaxedPrecision");
        }
        if matches!(opcode,1|52) && length>=3 && float_shapes.contains_key(&instruction[1]) {
            panic!("floating undef/spec-constant operation needs explicit FTZ analysis");
        }
        instructions.push(instruction); offset += length;
    }
    let mut next = words[3];
    let mut alloc = || { let id = next; next += 1; id };
    let uint = alloc(); let boolean = alloc(); let zero = alloc(); let exponent = alloc(); let sign = alloc();
    let mut declarations = Vec::new();
    emit(&mut declarations, 21, &[uint,32,0]); emit(&mut declarations, 20, &[boolean]);
    emit(&mut declarations, 43, &[uint,zero,0]);
    emit(&mut declarations, 43, &[uint,exponent,0x7f800000]);
    emit(&mut declarations, 43, &[uint,sign,0x80000000]);
    let mut shapes = BTreeMap::new();
    for (&float, &width) in &float_shapes {
        if width == 1 { shapes.insert(float,(uint,boolean,zero,exponent,sign)); }
        else {
            let u = alloc(); let b = alloc();
            emit(&mut declarations,23,&[u,uint,width]); emit(&mut declarations,23,&[b,boolean,width]);
            let mut constants = Vec::new();
            for scalar in [zero,exponent,sign] {
                let id = alloc(); let mut args = vec![u,id]; args.extend(vec![scalar;width as usize]);
                emit(&mut declarations,44,&args); constants.push(id);
            }
            shapes.insert(float,(u,b,constants[0],constants[1],constants[2]));
        }
    }
    let mut body = Vec::new(); let mut arithmetic = Vec::new(); let mut in_function = false;
    let mut renamed = BTreeMap::new();
    for mut instruction in instructions {
        let opcode = instruction[0]&0xffff;
        if opcode == 54 { in_function = true; }
        // Constants are data, so canonicalize them with integer bits as well.
        if matches!(opcode,43|50) && instruction.len() == 4 && float_shapes.get(&instruction[1]) == Some(&1) {
            if instruction[3]&0x7f800000 == 0 { instruction[3] &= 0x80000000; }
        }
        // Function arguments/phi inputs already come from canonical constants
        // or canonical instruction results. Leave the mandatory phi prefix
        // contiguous; inserting bit operations between phis is invalid SPIR-V.
        if in_function && instruction.len() >= 3 && !matches!(opcode,54|55|245) {
            if let Some(&(u,b,z,e,s)) = shapes.get(&instruction[1]) {
                // Integers convert to zero or magnitude >= 1, never subnormal.
                // Negation and selection only copy canonical float operands
                // (constants, parameters/phis, or already rewritten results).
                // They cannot create a new tiny magnitude. Keep NoContraction
                // on negation, but avoid redundant FTZ checks on these copies.
                if matches!(opcode,111|112|127|169) {
                    if opcode == 127 { arithmetic.push(instruction[2]); }
                    body.extend_from_slice(&instruction);
                    continue;
                }
                let float = instruction[1]; let result = instruction[2]; let raw = alloc();
                instruction[2] = raw;
                renamed.insert(result,raw);
                if matches!(opcode,127|129|131|133|136|140|141|142..=148) { arithmetic.push(raw); }
                body.extend_from_slice(&instruction);
                let bits = alloc(); let exp = alloc(); let tiny = alloc(); let signed_zero = alloc(); let selected = alloc();
                emit(&mut body,124,&[u,bits,raw]);
                emit(&mut body,199,&[u,exp,bits,e]);
                emit(&mut body,170,&[b,tiny,exp,z]);
                emit(&mut body,199,&[u,signed_zero,bits,s]);
                emit(&mut body,169,&[u,selected,tiny,signed_zero,bits]);
                emit(&mut body,124,&[float,result,selected]);
                continue;
            }
        }
        body.extend_from_slice(&instruction);
        if opcode == 56 { in_function = false; }
    }
    let mut out = words[..5].to_vec(); out[3] = next;
    let mut capabilities = false; let mut modes = false; let mut decorations = false; let mut constants = false;
    let mut offset = 0;
    while offset < body.len() {
        let length = (body[offset]>>16) as usize; let instruction = &body[offset..offset+length]; let opcode = instruction[0]&0xffff;
        if !capabilities && opcode != 17 {
            for cap in [4466,4467] { emit(&mut out,17,&[cap]); }
            if words[1] < 0x00010400 {
                let mut bytes = b"SPV_KHR_float_controls\0".to_vec(); while bytes.len()%4 != 0 {bytes.push(0);}
                let extension:Vec<u32> = bytes.chunks_exact(4).map(|b|u32::from_le_bytes(b.try_into().unwrap())).collect();
                emit(&mut out,10,&extension);
            }
            capabilities = true;
        }
        if !modes && !matches!(opcode,17|10|11|14|15|16) {
            for &entry in &entries {for mode in [4461,4462] {emit(&mut out,16,&[entry,mode,32]);}}
            modes = true;
        }
        if !decorations && (19..=39).contains(&opcode) {
            for &id in &arithmetic {emit(&mut out,71,&[id,42]);}
            decorations = true;
        }
        if !constants && opcode == 54 { out.extend_from_slice(&declarations); constants = true; }
        if opcode==71 && instruction.len()>=3 && instruction[2]==42 {
            let mut decoration=instruction.to_vec();
            if let Some(&raw)=renamed.get(&decoration[1]) {decoration[1]=raw;}
            out.extend_from_slice(&decoration);
        } else {out.extend_from_slice(instruction);}
        offset += length;
    }
    assert!(capabilities && modes && decorations && constants, "missing SPIR-V declaration sections");
    out
}

#[cfg(test)]
mod tests {
    use super::enforce_modes;
    fn emit(words: &mut Vec<u32>,op:u32,args:&[u32]) {words.push(((args.len() as u32+1)<<16)|op);words.extend_from_slice(args);}
    fn module() -> Vec<u32> {
        let mut words=vec![0x07230203,0x00010400,0,64,0];
        emit(&mut words,17,&[1]);emit(&mut words,14,&[0,1]);emit(&mut words,15,&[5,10,0]);
        emit(&mut words,19,&[1]);emit(&mut words,22,&[2,32]);emit(&mut words,23,&[3,2,2]);
        emit(&mut words,43,&[2,4,1]);emit(&mut words,54,&[1,10,0,11]);
        emit(&mut words,248,&[12]);emit(&mut words,129,&[2,13,4,4]);
        emit(&mut words,148,&[2,14,20,21]);emit(&mut words,253,&[]);emit(&mut words,56,&[]);words
    }
    fn instructions(words:&[u32]) -> Vec<Vec<u32>> {
        let mut out=Vec::new();let mut offset=5;
        while offset<words.len() {let n=(words[offset]>>16) as usize;out.push(words[offset..offset+n].to_vec());offset+=n;}
        out
    }
    #[test]
    fn canonicalizes_constants_results_and_vector_arithmetic() {
        let output=instructions(&enforce_modes(&module()));
        assert!(output.iter().any(|i|i[0]&0xffff==43 && i[2]==4 && i[3]==0));
        for op in [129,148] {
            let result=output.iter().find(|i|i[0]&0xffff==op).unwrap()[2];
            assert!(output.iter().any(|i|i[0]&0xffff==71 && i[1]==result && i[2]==42));
        }
        assert_eq!(output.iter().filter(|i|i[0]&0xffff==169).count(),2);
        for mode in [4461,4462] {assert!(output.iter().any(|i|i[0]&0xffff==16 && i[2]==mode && i[3]==32));}
    }
    #[test]
    fn rejects_lower_precision_and_fast_math() {
        for decoration in [0,40] {
            let mut words=module();emit(&mut words,71,&[13,decoration,0]);
            assert!(std::panic::catch_unwind(||enforce_modes(&words)).is_err());
        }
        let mut words=module();let mut offset=5;
        while offset<words.len() {if words[offset]&0xffff==22 {words[offset+2]=64;break;}offset+=(words[offset]>>16) as usize;}
        assert!(std::panic::catch_unwind(||enforce_modes(&words)).is_err());
    }
    #[test]
    fn avoids_rechecking_operations_that_cannot_create_subnormal_magnitudes() {
        let mut words=module();
        // Insert canonical scalar copies before the function return. This is
        // a binary transformation test; actual execution is covered by probes.
        let end=words.len()-2;
        let mut copies=Vec::new();
        emit(&mut copies,127,&[2,30,13]);
        emit(&mut copies,169,&[2,31,40,13,30]);
        emit(&mut copies,111,&[2,32,41]);
        emit(&mut copies,112,&[2,33,42]);
        words.splice(end..end,copies);
        let output=instructions(&enforce_modes(&words));
        for (op,id) in [(127,30),(169,31),(111,32),(112,33)] {
            assert!(output.iter().any(|i|i[0]&0xffff==op && i[2]==id));
        }
        assert!(output.iter().any(|i|i[0]&0xffff==71 && i[1]==30 && i[2]==42));
        assert_eq!(output.iter().filter(|i|i[0]&0xffff==169).count(),3);
    }
}
