use std::borrow::Borrow;
use std::ffi::CString;
use std::fs;
use std::ops::Deref;
use log::info;
use opencl3::command_queue::{CL_QUEUE_PROFILING_ENABLE, CommandQueue};
use opencl3::context::Context;
use opencl3::device::{CL_DEVICE_TYPE_GPU, Device};
use opencl3::kernel::{ExecuteKernel, Kernel};
use opencl3::memory::{CL_MAP_READ, CL_MAP_WRITE};
use opencl3::platform::{get_platforms, Platform};
use opencl3::program::{CL_STD_2_0, CL_STD_3_0, Program};
use opencl3::svm::SvmVec;
use opencl3::types::{CL_BLOCKING, cl_device_id, cl_int, cl_long, CL_NON_BLOCKING, cl_uchar, cl_ulong};
use crate::Error;
use crate::pow::State;

//static KERNEL: &[u8] = include_bytes!("../resources/kaspa-opencl-kernels.spirv");
static PROGRAM_SOURCE: &str = include_str!("../../resources/kaspa-opencl.cl");
//static PROGRAM_SOURCE_FILE: &str = "../resources/kaspa-opencl.cl";

pub fn run_kenel(state: &mut State) -> Result<(), Error>{
    let platforms = get_platforms().unwrap();
    let platform = &platforms[0];
    let device_ids = platform.get_devices(CL_DEVICE_TYPE_GPU).unwrap();
    let device = Device::new(device_ids[0]);
    info!("Device: {}", device.name().map_err(|e| e.to_string())?);
    let context = Context::from_device(&device).expect("Context::from_device failed");

    // If we use SPIR-V:
    // Program::create_and_build_from_il(&context, KERNEL, CL_STD_3_0)
    //    .expect("Program::create_and_build_from_source failed");
    //let PROGRAM_SOURCE_STRING = fs::read_to_string(PROGRAM_SOURCE_FILE)?;
    //let PROGRAM_SOURCE: &str = PROGRAM_SOURCE_STRING.borrow();
    let program = Program::create_and_build_from_source(&context, PROGRAM_SOURCE, CL_STD_2_0)
        .expect("Program::create_and_build_from_source failed");

    let pow_cshake = Kernel::create(&program, "pow_cshake").expect("Kernel::create failed");
    let matrix_mul = Kernel::create(&program, "matrix_mul").expect("Kernel::create failed");
    let heavy_hash_cshake = Kernel::create(&program, "heavy_hash_cshake").expect("Kernel::create failed");


    let queue = CommandQueue::create_with_properties(
        &context,
        context.default_device(),
        CL_QUEUE_PROFILING_ENABLE,
        0,
    )
        .expect("CommandQueue::create_with_properties failed");

    let mut nonces = SvmVec::<cl_ulong>::allocate(&context, 10).expect("SVM allocation failed");
    let mut pow_cshake_out = SvmVec::<[cl_uchar; 32]>::allocate(&context, 10).expect("SVM allocation failed");
    let mut matrix_mul_out = SvmVec::<[cl_uchar; 32]>::allocate(&context, 10).expect("SVM allocation failed");
    let mut final_nonce = SvmVec::<cl_ulong>::allocate(&context, 1).expect("SVM allocation failed");
    let mut final_hash = SvmVec::<[cl_uchar; 32]>::allocate(&context, 1).expect("SVM allocation failed");


    let mut hash_header = SvmVec::<[cl_uchar; 72]>::allocate(&context, 1).expect("SVM allocation failed");
    let mut matrix = SvmVec::<[[cl_uchar; 64]; 64]>::allocate(&context, 1).expect("SVM allocation failed");
    let mut target = SvmVec::<[cl_ulong; 4]>::allocate(&context, 1).expect("SVM allocation failed");

    queue.enqueue_svm_map(CL_BLOCKING, CL_MAP_WRITE, &mut hash_header, &[]).map_err(|e| e.to_string())?;
    hash_header[0].copy_from_slice(&state.pow_hash_header.map(|i| i as cl_uchar));
    let unmap_event = queue.enqueue_svm_unmap(&hash_header, &[]).map_err(|e| e.to_string())?;

    queue.enqueue_svm_map(CL_BLOCKING, CL_MAP_WRITE, &mut matrix, &[]).map_err(|e| e.to_string())?;
    matrix[0].copy_from_slice(state.cl_uchar_matrix.as_ref());
    let unmap_event = queue.enqueue_svm_unmap(&matrix, &[]).map_err(|e| e.to_string())?;

    queue.enqueue_svm_map(CL_BLOCKING, CL_MAP_WRITE, &mut target, &[]).map_err(|e| e.to_string())?;
    target[0].copy_from_slice(&state.target.0);
    let unmap_event = queue.enqueue_svm_unmap(&target, &[]).map_err(|e| e.to_string())?;

    // Run the kernel on the input data
    let size: cl_long = 10;
    let kernel_event = ExecuteKernel::new(&pow_cshake)
        .set_arg_svm(hash_header.as_ptr())
        .set_arg_svm(nonces.as_mut_ptr())
        .set_arg(&size)
        .set_arg_svm(pow_cshake_out.as_mut_ptr())
        .set_global_work_size(10)
        .set_event_wait_list(&[unmap_event.get()])
        .enqueue_nd_range(&queue).map_err(|e| e.to_string())?;
    kernel_event.wait().map_err(|e| e.to_string())?;

    let kernel_event = ExecuteKernel::new(&matrix_mul)
        .set_arg_svm(matrix.as_ptr())
        .set_arg_svm(pow_cshake_out.as_ptr())
        .set_arg(&size)
        .set_arg_svm(matrix_mul_out.as_mut_ptr())
        .set_global_work_sizes(&[32usize,10usize])
        .enqueue_nd_range(&queue).map_err(|e| e.to_string())?;
    kernel_event.wait().map_err(|e| e.to_string())?;

    let kernel_event = ExecuteKernel::new(&heavy_hash_cshake)
        .set_arg_svm(target.as_ptr())
        .set_arg_svm(nonces.as_ptr())
        .set_arg_svm(matrix_mul_out.as_ptr())
        .set_arg(&size)
        .set_arg_svm(final_nonce.as_mut_ptr())
        .set_arg_svm(final_hash.as_mut_ptr())
        .set_global_work_size(10)
        .enqueue_nd_range(&queue).map_err(|e| e.to_string())?;
    kernel_event.wait().map_err(|e| e.to_string())?;

    let _map_results_event =
        queue.enqueue_svm_map(CL_BLOCKING, CL_MAP_READ, &mut final_hash, &[]).map_err(|e| e.to_string())?;
    let _map_results_event2 =
        queue.enqueue_svm_map(CL_BLOCKING, CL_MAP_READ, &mut final_nonce, &[]).map_err(|e| e.to_string())?;

    info!("Results: {}, {:02X?}", final_nonce[0], final_hash[0]);
    info!("Expected: {:02X?}", state.calculate_pow(final_nonce[0]).to_le_bytes());
    assert!(final_hash[0] == state.calculate_pow(final_nonce[0]).to_le_bytes(), "Wrong comparisson");
    Ok(())
}