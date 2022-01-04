use std::borrow::Borrow;
use std::ffi::{c_void, CString};
use std::{fs, ptr};
use std::ops::Deref;
use std::sync::Arc;
use log::info;
use opencl3::command_queue::{CL_QUEUE_PROFILING_ENABLE, CommandQueue};
use opencl3::context::Context;
use opencl3::device::{CL_DEVICE_MAX_WORK_ITEM_SIZES, CL_DEVICE_TYPE_GPU, Device};
use opencl3::event::{Event, release_event, retain_event, wait_for_events};
use opencl3::kernel::{ExecuteKernel, Kernel};
use opencl3::memory::{CL_MAP_READ, CL_MAP_WRITE, CL_MEM_READ_WRITE, Buffer, CL_MEM_WRITE_ONLY, ClMem};
use opencl3::platform::{get_platforms, Platform};
use opencl3::program::{CL_FAST_RELAXED_MATH, CL_FINITE_MATH_ONLY, CL_MAD_ENABLE, CL_STD_2_0, CL_STD_3_0, DEBUG_OPTION, Program};
use opencl3::svm::SvmVec;
use opencl3::types::{CL_BLOCKING, cl_device_id, cl_event, cl_int, cl_long, cl_mem, CL_NON_BLOCKING, cl_uchar, cl_uint, cl_ulong};
use rand::Fill;
use crate::Error;
use crate::gpu::GPUWork;
use crate::pow::State;
use std::ptr::null;

static PROGRAM_SOURCE: &str = include_str!("../../resources/kaspa-opencl.cl");

pub struct OpenCLGPUWork {
    context: Arc<Context>,
    workload: usize,

    heavy_hash: Kernel,

    queue: CommandQueue,

    random_state: Buffer<[cl_ulong; 2]>,
    final_nonce: Buffer<cl_ulong>,
    final_hash: Buffer<[cl_ulong; 4]>,

    hash_header: Buffer<[cl_uchar; 72]>,
    matrix: Buffer<[[cl_uchar; 64]; 64]>,
    target: Buffer<[cl_ulong; 4]>,

    events: Vec<cl_event>,
}

impl GPUWork for OpenCLGPUWork {
    fn id(&self) -> String {
        let device =  Device::new(self.context.default_device());
        format!("{}", device.name().unwrap())
    }

    fn load_block_constants(&mut self, hash_header: &[u8; 72], matrix: &[[cl_uchar; 64]; 64], target: &[u64; 4]) {
        let reset_final_nonce = self.queue.enqueue_write_buffer(&mut self.final_nonce, CL_NON_BLOCKING, 0, &[0], &[]).map_err(|e| e.to_string()).unwrap();
        let copy_header = self.queue.enqueue_write_buffer(&mut self.hash_header, CL_NON_BLOCKING, 0, &[*hash_header], &[]).map_err(|e| e.to_string()).unwrap();
        let copy_matrix = self.queue.enqueue_write_buffer(&mut self.matrix, CL_NON_BLOCKING, 0, &[*matrix], &[]).map_err(|e| e.to_string()).unwrap();
        let copy_target = self.queue.enqueue_write_buffer(&mut self.target, CL_NON_BLOCKING, 0, &[*target], &[]).map_err(|e| e.to_string()).unwrap();

        self.events = vec!(reset_final_nonce.get(), copy_header.get(), copy_matrix.get(), copy_target.get());
        for event in &self.events{
            retain_event(*event).unwrap();
        }
    }

    fn calculate_hash(&mut self, _nonces: Option<&Vec<u64>>) {
        let kernel_event = ExecuteKernel::new(&self.heavy_hash)
            .set_arg(&self.hash_header)
            .set_arg(&self.matrix)
            .set_arg(&self.target)
            .set_arg(&self.random_state)
            .set_arg(&self.final_nonce)
            .set_arg(&self.final_hash)
            .set_global_work_size(self.workload)
            .set_event_wait_list(self.events.borrow())
            .enqueue_nd_range(&self.queue).map_err(|e| e.to_string()).unwrap();

        kernel_event.wait().unwrap();

        /*let mut nonces = [0u64; 1];
        let mut hash = [[0u64; 4]];
        self.queue.enqueue_read_buffer(&self.final_nonce, CL_BLOCKING, 0, &mut nonces, &[]).map_err(|e| e.to_string()).unwrap();
        self.queue.enqueue_read_buffer(&self.final_hash, CL_BLOCKING, 0, &mut hash, &[]).map_err(|e| e.to_string()).unwrap();
        log::info!("Hash from kernel: {:?}", hash);*/
        /*for event in &self.events{
            release_event(*event).unwrap();
        }
        let event = kernel_event.get();
        self.events = vec!(event);
        retain_event(event);*/
    }

    fn sync(&self) -> Result<(), Error> {
        wait_for_events(&self.events).map_err(|e| format!("waiting error code {}", e))?;
        for event in &self.events{
            release_event(*event).unwrap();
        }
        Ok(())
    }

    fn get_workload(&self) -> usize {
        self.workload as usize
    }

    fn copy_output_to(&mut self, nonces: &mut Vec<u64>) -> Result<(), Error> {
        self.queue.enqueue_read_buffer(&self.final_nonce, CL_BLOCKING, 0, nonces, &[]).map_err(|e| e.to_string()).unwrap();
        Ok(())
    }
}

impl OpenCLGPUWork {
    pub fn new(device_id: cl_device_id, workload: f32, is_absolute: bool) -> Result<Self,Error> {
        info!("Using OpenCL");
        let device =  Device::new(device_id);
        let chosen_workload:usize;
        if is_absolute {
            chosen_workload = workload as usize
        } else {
            let max_work_group_size = (
                device.max_work_group_size().map_err(|e| e.to_string())? * (device.max_compute_units().map_err(|e| e.to_string())? as usize)
            ) as f32;
            chosen_workload = (workload * max_work_group_size) as usize;
        }
        let context = Arc::new(Context::from_device(&device).expect("Context::from_device failed"));
        let context_ref = unsafe{Arc::as_ptr(&context).as_ref().unwrap()};

        let program = Program::create_and_build_from_source(&context, PROGRAM_SOURCE, "")
            .expect("Program::create_and_build_from_source failed");

        let heavy_hash = Kernel::create(&program, "heavy_hash").expect("Kernel::create failed");

        let queue = CommandQueue::create_with_properties(
            &context,
            context.default_device(),
            CL_QUEUE_PROFILING_ENABLE,
            0,
        ).expect("CommandQueue::create_with_properties failed");

        let mut random_state = Buffer::<[cl_ulong;2]>::create(context_ref, CL_MEM_READ_WRITE, chosen_workload, ptr::null_mut()).expect("Buffer allocation failed");
        let final_nonce = Buffer::<cl_ulong>::create(context_ref, CL_MEM_READ_WRITE, 1, ptr::null_mut()).expect("Buffer allocation failed");
        let final_hash = Buffer::<[cl_ulong; 4]>::create(context_ref, CL_MEM_READ_WRITE, 1, ptr::null_mut()).expect("Buffer allocation failed");

        let hash_header = Buffer::<[cl_uchar; 72]>::create(context_ref, CL_MEM_READ_WRITE, 1, ptr::null_mut()).expect("Buffer allocation failed");
        let matrix = Buffer::<[[cl_uchar; 64]; 64]>::create(context_ref, CL_MEM_READ_WRITE, 1, ptr::null_mut()).expect("Buffer allocation failed");
        let target = Buffer::<[cl_ulong; 4]>::create(context_ref, CL_MEM_READ_WRITE, 1, ptr::null_mut()).expect("Buffer allocation failed");

        info!("GPU ({}) is generating initial seed. This may take some time.", device.name().unwrap());
        let mut seeds = vec![1u64; 2 * chosen_workload];
        seeds.try_fill(&mut rand::thread_rng())?;
        let mut random_state_local: *mut c_void = 0 as *mut c_void;

        queue.enqueue_map_buffer(&mut random_state, CL_BLOCKING, CL_MAP_WRITE, 0, chosen_workload, &mut random_state_local, &[]).map_err(|e| e.to_string())?.wait();
        unsafe{ random_state_local.copy_from(seeds.as_ptr() as *mut c_void, 2*chosen_workload ); }
        // queue.enqueue_svm_unmap(&random_state,&[]).map_err(|e| e.to_string())?;
        queue.enqueue_unmap_mem_object(random_state.get(), random_state_local, &[]).map_err(|e| e.to_string()).unwrap();
        Ok(
            Self{
                context: context.clone(), workload: chosen_workload,
                heavy_hash, random_state,
                queue, final_nonce, final_hash,
                hash_header, matrix, target, events: Vec::<cl_event>::new()
            }
        )
    }
}