use std::borrow::Borrow;
use std::ffi::c_void;
use std::ptr;
use std::sync::Arc;
use log::info;
use opencl3::command_queue::{CL_QUEUE_PROFILING_ENABLE, CommandQueue};
use opencl3::context::Context;
use opencl3::device::{CL_DEVICE_MAX_WORK_ITEM_SIZES, CL_DEVICE_TYPE_GPU, Device, get_device_info, CL_DEVICE_VERSION};
use opencl3::event::{Event, release_event, retain_event, wait_for_events};
use opencl3::kernel::{ExecuteKernel, Kernel};
use opencl3::memory::{CL_MAP_READ, CL_MAP_WRITE, CL_MEM_READ_WRITE, Buffer, CL_MEM_WRITE_ONLY, ClMem, CL_MEM_READ_ONLY};
use opencl3::program::{CL_FAST_RELAXED_MATH, CL_FINITE_MATH_ONLY, CL_MAD_ENABLE, CL_STD_2_0, CL_STD_3_0, DEBUG_OPTION, Program};
use opencl3::types::{CL_BLOCKING, cl_event, CL_NON_BLOCKING, cl_uchar, cl_ulong};
use rand::Fill;
use crate::Error;
use kaspa_miner::Worker;
use kaspa_miner::xoshiro256starstar::Xoshiro256StarStar;

static PROGRAM_SOURCE: &str = include_str!("../resources/kaspa-opencl.cl");
//let cl_uchar_matrix: Arc<[[u8;64];64]> = Arc::new(matrix.0.map(|row| row.map(|v| v as cl_uchar)));

pub struct OpenCLGPUWorker {
    context: Arc<Context>,
    workload: usize,

    heavy_hash: Kernel,

    queue: CommandQueue,

    random_state: Buffer<[cl_ulong; 4]>,
    final_nonce: Buffer<cl_ulong>,
    final_hash: Buffer<[cl_ulong; 4]>,

    hash_header: Buffer<cl_uchar>,
    matrix: Buffer<cl_uchar>,
    target: Buffer<cl_ulong>,

    events: Vec<cl_event>,
}

impl Worker for OpenCLGPUWorker {
    fn id(&self) -> String {
        let device =  Device::new(self.context.default_device());
        format!("{}", device.name().unwrap())
    }

    fn load_block_constants(&mut self, hash_header: &[u8; 72], matrix: &[[u16; 64]; 64], target: &[u64; 4]) {
        let cl_uchar_matrix = matrix.iter().flat_map(|row| row.map(|v| v as cl_uchar)).collect::<Vec<cl_uchar>>();

        let reset_final_nonce = self.queue.enqueue_write_buffer(&mut self.final_nonce, CL_BLOCKING, 0, &[0], &[]).map_err(|e| e.to_string()).unwrap().wait();
        let copy_header = self.queue.enqueue_write_buffer(&mut self.hash_header, CL_BLOCKING, 0, hash_header, &[]).map_err(|e| e.to_string()).unwrap().wait();
        let copy_matrix = self.queue.enqueue_write_buffer(&mut self.matrix, CL_BLOCKING, 0, cl_uchar_matrix.as_slice(), &[]).map_err(|e| e.to_string()).unwrap().wait();
        let copy_target = self.queue.enqueue_write_buffer(&mut self.target, CL_BLOCKING, 0, target, &[]).map_err(|e| e.to_string()).unwrap();

        self.events = vec!(copy_target.get());
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

impl OpenCLGPUWorker {
    pub fn new(device: Device, workload: f32, is_absolute: bool) -> Result<Self,Error> {
        info!("Using OpenCL");
        let version = device.version().expect("Device::could not query device version");
        info!("Device  {} supports {} with extensions: {}", device.name().unwrap(), version, device.extensions().expect("Device::failed extension query"));
        let chosen_workload:usize;
        if is_absolute {
            chosen_workload = workload as usize
        } else {
            let max_work_group_size = (
                device.max_work_group_size().map_err(|e| e.to_string())? * (device.max_compute_units().map_err(|e| e.to_string())? as usize)
            ) as f32;
            chosen_workload = (workload * max_work_group_size) as usize;
        }
        info!("Device {} chosen workload is {}", device.name().unwrap(), chosen_workload);
        let context = Arc::new(Context::from_device(&device).expect("Context::from_device failed"));
        let context_ref = unsafe{Arc::as_ptr(&context).as_ref().unwrap()};

        let v = version.split(" ").nth(1).unwrap();
        let mut compile_options = "".to_string();
        compile_options += CL_MAD_ENABLE;
        compile_options += CL_FINITE_MATH_ONLY;
        if v == "2.0" || v == "2.1" || v=="3.0" {
            info!("Compiling with OpenCl 2");
            compile_options += CL_STD_2_0;
        }
        //let source = fs::read_to_string("kaspa-opencl.cl")?;
        //let PROGRAM_SOURCE1 = source.as_str();
        let program = Program::create_and_build_from_source(&context, PROGRAM_SOURCE, compile_options.as_str())
            .expect("Program::create_and_build_from_source failed");

        let heavy_hash = Kernel::create(&program, "heavy_hash").expect("Kernel::create failed");

        let queue = CommandQueue::create_with_properties(
            &context,
            context.default_device(),
            CL_QUEUE_PROFILING_ENABLE,
            0,
        ).expect("CommandQueue::create_with_properties failed");

        let mut random_state = Buffer::<[cl_ulong;4]>::create(context_ref, CL_MEM_READ_WRITE, chosen_workload, ptr::null_mut()).expect("Buffer allocation failed");
        let final_nonce = Buffer::<cl_ulong>::create(context_ref, CL_MEM_READ_WRITE, 1, ptr::null_mut()).expect("Buffer allocation failed");
        let final_hash = Buffer::<[cl_ulong; 4]>::create(context_ref, CL_MEM_WRITE_ONLY, 1, ptr::null_mut()).expect("Buffer allocation failed");

        let hash_header = Buffer::<cl_uchar>::create(context_ref, CL_MEM_READ_ONLY, 72, ptr::null_mut()).expect("Buffer allocation failed");
        let matrix = Buffer::<cl_uchar>::create(context_ref, CL_MEM_READ_ONLY, 64*64, ptr::null_mut()).expect("Buffer allocation failed");
        let target = Buffer::<cl_ulong>::create(context_ref, CL_MEM_READ_ONLY, 4, ptr::null_mut()).expect("Buffer allocation failed");

        info!("GPU ({}) is generating initial seed. This may take some time.", device.name().unwrap());
        let mut seed = [1u64; 4];
        seed.try_fill(&mut rand::thread_rng())?;
        let rand_state = Xoshiro256StarStar::new(&seed).iter_jump_state().take(chosen_workload).collect::<Vec<[u64;4]>>();
        let mut random_state_local: *mut c_void = 0 as *mut c_void;

        queue.enqueue_map_buffer(&mut random_state, CL_BLOCKING, CL_MAP_WRITE, 0, 32*chosen_workload, &mut random_state_local, &[]).map_err(|e| e.to_string())?.wait();
        if random_state_local.is_null() {
            return Err("could not load random state vector to memory. Consider changing random or lowering workload".into());
        }
        unsafe{ random_state_local.copy_from(rand_state.as_ptr() as *mut c_void, 32*chosen_workload ); }
        // queue.enqueue_svm_unmap(&random_state,&[]).map_err(|e| e.to_string())?;
        queue.enqueue_unmap_mem_object(random_state.get(), random_state_local, &[]).map_err(|e| e.to_string()).unwrap().wait();
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
