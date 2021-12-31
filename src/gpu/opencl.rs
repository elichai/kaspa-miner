use std::borrow::Borrow;
use std::ffi::CString;
use std::fs;
use std::ops::Deref;
use std::sync::Arc;
use log::info;
use opencl3::command_queue::{CL_QUEUE_PROFILING_ENABLE, CommandQueue};
use opencl3::context::Context;
use opencl3::device::{CL_DEVICE_MAX_WORK_ITEM_SIZES, CL_DEVICE_TYPE_GPU, Device};
use opencl3::event::{Event, release_event, retain_event, wait_for_events};
use opencl3::kernel::{ExecuteKernel, Kernel};
use opencl3::memory::{CL_MAP_READ, CL_MAP_WRITE};
use opencl3::platform::{get_platforms, Platform};
use opencl3::program::{CL_FAST_RELAXED_MATH, CL_FINITE_MATH_ONLY, CL_MAD_ENABLE, CL_STD_2_0, CL_STD_3_0, DEBUG_OPTION, Program};
use opencl3::svm::SvmVec;
use opencl3::types::{CL_BLOCKING, cl_device_id, cl_event, cl_int, cl_long, CL_NON_BLOCKING, cl_uchar, cl_uint, cl_ulong};
use rand::Fill;
use crate::Error;
use crate::gpu::GPUWork;
use crate::pow::State;

static PROGRAM_SOURCE: &str = include_str!("../../resources/kaspa-opencl.cl");

pub struct OpenCLGPUWork<'gpu> {
    context: Arc<Context>,
    workload: usize,

    heavy_hash: Kernel,

    queue: CommandQueue,

    random_state: SvmVec<'gpu, [cl_ulong; 2]>,
    final_nonce: SvmVec<'gpu, cl_ulong>,
    final_hash: SvmVec<'gpu, [cl_ulong; 4]>,

    hash_header: SvmVec<'gpu, [cl_uchar; 72]>,
    matrix: SvmVec<'gpu, [[cl_uchar; 64]; 64]>,
    target: SvmVec<'gpu, [cl_ulong; 4]>,

    events: Vec<cl_event>,
}

impl<'gpu> GPUWork for OpenCLGPUWork<'gpu> {
    fn id(&self) -> String {
        let device =  Device::new(self.context.default_device());
        format!("{}", device.name().unwrap())
    }

    fn load_block_constants(&mut self, hash_header: &[u8; 72], matrix: &[[cl_uchar; 64]; 64], target: &[u64; 4]) {
        self.queue.enqueue_svm_map(CL_BLOCKING, CL_MAP_WRITE, &mut self.final_nonce, &[]).map_err(|e| e.to_string()).unwrap();
        self.final_nonce[0] = 0;
        let reset_final_nonce = self.queue.enqueue_svm_unmap(&self.final_nonce, &[]).map_err(|e| e.to_string()).unwrap();

        self.queue.enqueue_svm_map(CL_BLOCKING, CL_MAP_WRITE, &mut self.hash_header, &[]).map_err(|e| e.to_string()).unwrap();
        self.hash_header[0].copy_from_slice(&hash_header.map(|i| i as cl_uchar));
        let copy_header = self.queue.enqueue_svm_unmap(&self.hash_header, &[]).map_err(|e| e.to_string()).unwrap();

        self.queue.enqueue_svm_map(CL_BLOCKING, CL_MAP_WRITE, &mut self.matrix, &[]).map_err(|e| e.to_string()).unwrap();
        self.matrix[0].copy_from_slice(matrix);
        let copy_matrix = self.queue.enqueue_svm_unmap(&self.matrix, &[]).map_err(|e| e.to_string()).unwrap();

        self.queue.enqueue_svm_map(CL_BLOCKING, CL_MAP_WRITE, &mut self.target, &[]).map_err(|e| e.to_string()).unwrap();
        self.target[0].copy_from_slice(target.as_slice());
        let copy_target = self.queue.enqueue_svm_unmap(&self.target, &[]).map_err(|e| e.to_string()).unwrap();

        self.events = vec!(reset_final_nonce.get(), copy_header.get(), copy_matrix.get(), copy_target.get());
        for event in &self.events{
            retain_event(*event).unwrap();
        }
        //wait_for_events(&self.events);
        //self.events = Vec::<cl_event>::new();
    }

    fn calculate_hash(&mut self, _nonces: Option<&Vec<u64>>) {
        let kernel_event = ExecuteKernel::new(&self.heavy_hash)
            .set_arg_svm(self.hash_header.as_ptr())
            .set_arg_svm(self.matrix.as_ptr())
            .set_arg_svm(self.target.as_ptr())
            .set_arg_svm(self.random_state.as_mut_ptr())
            .set_arg_svm(self.final_nonce.as_mut_ptr())
            .set_arg_svm(self.final_hash.as_mut_ptr())
            .set_global_work_size(self.workload)
            .set_event_wait_list(self.events.borrow())
            .enqueue_nd_range(&self.queue).map_err(|e| e.to_string()).unwrap();

        kernel_event.wait().unwrap();
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
        //self.queue.enqueue_svm_map(CL_BLOCKING, CL_MAP_READ, &mut self.final_hash, &[]).map_err(|e| e.to_string())?;
        self.queue.enqueue_svm_map(CL_BLOCKING, CL_MAP_READ, &mut self.final_nonce, &[]).map_err(|e| e.to_string())?;
        nonces[0] = self.final_nonce[0];
        self.queue.enqueue_svm_unmap(&self.final_nonce,&[]).map_err(|e| e.to_string())?;
        Ok(())
    }
}

impl<'gpu> OpenCLGPUWork<'gpu> {
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

        let program = Program::create_and_build_from_source(&context, PROGRAM_SOURCE, format!("{}", CL_STD_2_0).as_str())
            .expect("Program::create_and_build_from_source failed");

        let heavy_hash = Kernel::create(&program, "heavy_hash").expect("Kernel::create failed");

        let queue = CommandQueue::create_with_properties(
            &context,
            context.default_device(),
            CL_QUEUE_PROFILING_ENABLE,
            0,
        ).expect("CommandQueue::create_with_properties failed");

        let mut random_state = SvmVec::<[cl_ulong;2]>::allocate(context_ref, chosen_workload).expect("SVM allocation failed");
        let final_nonce = SvmVec::<cl_ulong>::allocate(context_ref, 1).expect("SVM allocation failed");
        let final_hash = SvmVec::<[cl_ulong; 4]>::allocate(context_ref, 1).expect("SVM allocation failed");

        let hash_header = SvmVec::<[cl_uchar; 72]>::allocate(context_ref, 1).expect("SVM allocation failed");
        let matrix = SvmVec::<[[cl_uchar; 64]; 64]>::allocate(context_ref, 1).expect("SVM allocation failed");
        let target = SvmVec::<[cl_ulong; 4]>::allocate(context_ref, 1).expect("SVM allocation failed");

        info!("GPU ({}) is generating initial seed. This may take some time.", device.name().unwrap());
        let mut seeds = vec![1u64; 2 * chosen_workload];
        seeds.try_fill(&mut rand::thread_rng())?;
        queue.enqueue_svm_map(CL_BLOCKING, CL_MAP_WRITE, &mut random_state, &[]).map_err(|e| e.to_string())?;
        for i in 0..chosen_workload {
            random_state[i][0] = seeds[2*i];
            random_state[i][1] = seeds[2*i+1];
        }
        queue.enqueue_svm_unmap(&random_state,&[]).map_err(|e| e.to_string())?;

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