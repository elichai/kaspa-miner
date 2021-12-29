use crate::Error;
use crate::gpu::cuda::CudaGPUWork;

pub mod cuda;

pub enum GPUWorkType {
    CUDA,
    OPENCL
}

pub struct GPUWorkFactory {
    type_: GPUWorkType,
    device_id: u32,
    workload: f32,
    is_absolute: bool
}

impl GPUWorkFactory {
    pub fn new(type_: GPUWorkType, device_id: u32, workload: f32, is_absolute: bool) -> Self {
        Self{ type_, device_id, workload, is_absolute  }
    }
    pub fn build(&self) -> Result<impl GPUWork, Error> {
        match self.type_ {
            GPUWorkType::CUDA  => Ok(CudaGPUWork::new(self.device_id, self.workload, self.is_absolute)?),
            _ => Ok(CudaGPUWork::new(self.device_id, self.workload, self.is_absolute)?) // TODO: return error
        }
    }
}

pub trait GPUWork {
    //fn new(device_id: u32, workload: f32, is_absolute: bool) -> Result<Self, Error>;
    fn id(&self) -> String;
    fn load_block_constants(&mut self, hash_header: &[u8; 72], matrix: &[[u8; 64]; 64], target: &[u64; 4]);

    fn calculate_pow_hash(&mut self, nonces: Option<&Vec<u64>>);
    fn calculate_matrix_mul(&mut self);
    fn calculate_heavy_hash(&mut self);
    fn sync(&self) -> Result<(), Error>;

    fn get_workload(&self) -> usize;
    fn get_output_size(&self) -> usize;
    fn copy_output_to(&self, nonces: &mut Vec<u64>) -> Result<(), Error>;
    //pub(crate) fn check_random(&self) -> Result<(),Error>;
    //pub(crate) fn copy_input_from(&mut self, nonces: &Vec<u64>);
}