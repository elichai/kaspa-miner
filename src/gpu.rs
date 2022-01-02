use std::str::FromStr;
use opencl3::device::{CL_DEVICE_TYPE_ALL, CL_DEVICE_TYPE_GPU};
use opencl3::platform::get_platforms;
use crate::Error;
use crate::gpu::cuda::CudaGPUWork;
use crate::gpu::opencl::OpenCLGPUWork;

pub mod cuda;
pub mod opencl;
mod xoshiro256starstar;

#[derive(Copy, Clone, Debug)]
pub enum GPUWorkType{
    CUDA,
    OPENCL
}

impl FromStr for GPUWorkType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match (s) {
            "CUDA" => Ok(Self::CUDA),
            "OPENCL" => Ok(Self::OPENCL),
            _ => Err(String::from("Unknown string"))
        }
    }
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
    pub fn build(&self) -> Result<Box<dyn GPUWork + 'static>, Error> {
        match self.type_ {
            GPUWorkType::CUDA  => Ok(Box::new(CudaGPUWork::new(self.device_id, self.workload, self.is_absolute)?)),
            GPUWorkType::OPENCL => {
                let platforms = get_platforms().unwrap();
                let platform = &platforms[0];
                let device_ids = platform.get_devices(CL_DEVICE_TYPE_ALL).unwrap();
                Ok(Box::new(OpenCLGPUWork::new(device_ids[self.device_id as usize], self.workload, self.is_absolute)?))
            } // TODO: return error
        }
    }
}

pub trait GPUWork {
    //fn new(device_id: u32, workload: f32, is_absolute: bool) -> Result<Self, Error>;
    fn id(&self) -> String;
    fn load_block_constants(&mut self, hash_header: &[u8; 72], matrix: &[[u8; 64]; 64], target: &[u64; 4]);

    fn calculate_hash(&mut self, nonces: Option<&Vec<u64>>);
    fn sync(&self) -> Result<(), Error>;

    fn get_workload(&self) -> usize;
    fn copy_output_to(&mut self, nonces: &mut Vec<u64>) -> Result<(), Error>;
    //pub(crate) fn check_random(&self) -> Result<(),Error>;
    //pub(crate) fn copy_input_from(&mut self, nonces: &Vec<u64>);
}