use cust::error::CudaResult;
use cust::function::Function;
use cust::prelude::*;
use crate::Error;
use log::{error, info};
use rand::Fill;

static PTX: &str = include_str!("../resources/kaspa-cuda-native.ptx");

// Get this from the device!

type CurandDirectionVectors64 = [u64; 64];

pub struct CurandStateSobol64 {
    i: CurandDirectionVectors64, // u64, but in c, sturcts are aligned by the longest field
    x: CurandDirectionVectors64, // u64, see above
    direction_vectors: CurandDirectionVectors64,
}

pub struct Kernel<'kernel> {
    func: Function<'kernel>,
    block_size: u32,
    grid_size: u32
}

impl Kernel<'kernel> {
    pub fn new(module: &'kernel Module, name: &str, workload: usize) -> Result<Kernel<'kernel>, Error> {
        let func = module.get_function(name).or_else(|e| { error!("Error loading function: {}", e); Result::Err(e)})?;
        let (_, block_size) = func.suggested_launch_configuration(0, 0.into())?;
        let grid_size = (workload as u32 + block_size - 1) / block_size;
        Ok(
            Self { func, block_size, grid_size }
        )
    }
}


pub struct GPUWork<'gpu> {
    workload: usize,
    stream: Stream,
    rand_state: DeviceBuffer<CurandStateSobol64>,

    nonces_buff: DeviceBuffer<u64>,
    pow_hashes_buff: DeviceBuffer<[u8; 32]>,
    matrix_mul_out_buff: DeviceBuffer<[u8; 32]>,
    final_hashes_buff: DeviceBuffer<[u8; 32]>,
    final_nonces_buff: DeviceBuffer<u64>,

    pow_hash_kernel: Kernel<'gpu>,
    matrix_mul_kernel: Kernel<'gpu>,
    heavy_hash_kernel: Kernel<'gpu>,
}

pub struct GPUContext {
    context: CudaResult<Context>,
    module: Module,
}

impl GPUContext{
    pub fn new(context :CudaResult<Context>) -> Result<Self, Error> {
        let module = Module::from_str(PTX).or_else(|e| { error!("Error loading PTX: {}", e); Result::Err(e)})?;
        Ok(Self{ context, module})
    }

    pub fn get_worker(&self, workload: usize) -> Result<GPUWork, Error>{
        GPUWork::new(self, workload)
    }
}

impl GPUWork<'gpu> {
    pub fn new(context: &'gpu GPUContext, workload: usize) -> Result<Self, Error> {
        let stream = Stream::new(StreamFlags::NON_BLOCKING, None)?;

        let rand_init = Kernel::new(&context.module, "init", workload)?;
        let pow_hash_kernel = Kernel::new(&context.module, "pow_cshake", workload)?;
        let matrix_mul_kernel = Kernel::new(&context.module, "matrix_mul", workload)?;
        let heavy_hash_kernel = Kernel::new(&context.module, "heavy_hash_cshake", workload)?;

        let mut rand_state = unsafe {
            DeviceBuffer::<CurandStateSobol64>::zeroed(workload).unwrap()
        };

        let nonces_buff = vec![0u64; workload].as_slice().as_dbuf()?;
        let pow_hashes_buff = vec![[0u8; 32]; workload].as_slice().as_dbuf()?;
        let matrix_mul_out_buff = vec![[0u8; 32]; workload].as_slice().as_dbuf()?;

        let final_hashes_buff = vec![[0u8; 32]; heavy_hash_kernel.grid_size as usize].as_slice().as_dbuf()?;
        let final_nonces_buff = vec![0u64; heavy_hash_kernel.grid_size as usize].as_slice().as_dbuf()?;

        info!("Generating initial seed. This may take some time.");
        let func = rand_init.func;
        let mut seeds = vec![1u64; 64*workload];
        seeds.try_fill(&mut rand::thread_rng())?;
        unsafe {
            launch!(
                func<<<rand_init.grid_size, rand_init.block_size, 0, stream>>>(
                    seeds.as_slice().as_dbuf()?.as_device_ptr(),
                    rand_state.as_device_ptr(),
                    workload,
                )
            )?;
        }
        stream.synchronize().or_else(|e| {error!("GPU Init failed: {}", rand_state.len()); Err(e)})?;
        info!("GPU Initialized");
        Ok(
            Self {
                workload, stream, rand_state, nonces_buff,
                pow_hashes_buff, matrix_mul_out_buff, final_hashes_buff, final_nonces_buff,
                pow_hash_kernel, matrix_mul_kernel, heavy_hash_kernel
            }
        )
    }

    #[inline(always)]
    pub(crate) fn calculate_pow_hash(&mut self, hash_header: &Vec<u8>, nonces: Option<&Vec<u64>> ) {
        let func = &self.pow_hash_kernel.func;
        let stream = &self.stream;
        let mut generate = true;
        if let Some(inner) = nonces {
            self.nonces_buff.copy_from(inner).unwrap();
            generate = false;
        }
        unsafe {
            launch!(
                func<<<
                    self.pow_hash_kernel.grid_size, self.pow_hash_kernel.block_size,
                    0, stream
                >>>(
                    hash_header.as_slice().as_dbuf().unwrap().as_device_ptr(),
                    self.nonces_buff.as_device_ptr(),
                    self.nonces_buff.len(),
                    self.pow_hashes_buff.as_device_ptr(),
                    generate,
                    self.rand_state.as_device_ptr(),
                )
            ).unwrap(); // We see errors in sync
        }
    }

    #[inline(always)]
    pub(crate) fn calculate_matrix_mul(&mut self, matrix_gpu: &mut DeviceBuffer<[u16; 64]>){
        let func = &self.matrix_mul_kernel.func;
        let stream = &self.stream;
        unsafe {
            launch!(
                func<<<
                    (32, self.matrix_mul_kernel.grid_size),
                    (1, self.matrix_mul_kernel.block_size),
                    0, stream
                >>>(
                        matrix_gpu.as_device_ptr(),
                        matrix_gpu.len(),
                        self.pow_hashes_buff.as_device_ptr(),
                        self.pow_hashes_buff.len(),
                        self.matrix_mul_out_buff.as_device_ptr()
                )
            ).unwrap(); // We see errors in sync
        }
        // TODO: synchronize?
    }

    #[inline(always)]
    pub(crate) fn calculate_heavy_hash(&mut self){
        let func = &self.heavy_hash_kernel.func;
        let stream = &self.stream;
        unsafe {
            launch!(
                func<<<
                    self.heavy_hash_kernel.grid_size,
                    self.heavy_hash_kernel.block_size,
                    0, stream
                >>>(
                    self.nonces_buff.as_device_ptr(),
                    self.matrix_mul_out_buff.as_device_ptr(),
                    self.matrix_mul_out_buff.len(),
                    self.final_nonces_buff.as_device_ptr(),
                    self.final_hashes_buff.as_device_ptr(),
                )
            ).unwrap(); // We see errors in sync
        }
    }

    #[inline(always)]
    pub(crate) fn sync(&self) -> Result<(), Error>{
        self.stream.synchronize()?;
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn get_output_size(&self) -> usize{
        self.heavy_hash_kernel.grid_size as usize
    }

    #[inline(always)]
    pub(crate) fn copy_output_to(& self, hashes: &mut Vec<[u8; 32]>, nonces: &mut Vec<u64>) -> Result<(),Error> {
        self.final_hashes_buff.copy_to(hashes)?;
        self.final_nonces_buff.copy_to(nonces)?;
        Ok(())
    }

    /*pub(crate) fn check_random(&self) -> Result<(),Error> {
        let mut nonces = vec![0u64; GPU_THREADS];
        self.nonces_buff.copy_to(&mut nonces)?;
        println!("Nonce: {}", nonces[0]);
        Ok(())
    }*/

    /*#[inline(always)]
    pub(crate) fn copy_input_from(&mut self, nonces: &Vec<u64>){
        self.nonces_buff.copy_from(nonces);
    }*/

}