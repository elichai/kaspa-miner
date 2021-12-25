use std::ffi::CString;
use crate::Error;
use cust::context::CurrentContext;
use cust::device::DeviceAttribute;
use cust::function::Function;
use cust::prelude::*;
use log::{error, info};
use rand::Fill;
use std::rc::{Rc, Weak};

static PTX_61: &str = include_str!("../resources/kaspa-cuda-sm61.ptx");
static PTX_30: &str = include_str!("../resources/kaspa-cuda-sm30.ptx");
static PTX_20: &str = include_str!("../resources/kaspa-cuda-sm20.ptx");

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
    grid_size: u32,
}

impl<'kernel> Kernel<'kernel> {
    pub fn new(module: Weak<Module>, name: &'kernel str) -> Result<Kernel<'kernel>, Error> {
        let func: Function;
        unsafe {
            func = module.as_ptr().as_ref().unwrap().get_function(name).or_else(|e| {
                error!("Error loading function: {}", e);
                Result::Err(e)
            })?;
        }
        let (_, block_size) = func.suggested_launch_configuration(0, 0.into())?;
        let grid_size;

        let device = CurrentContext::get_device()?;
        let sm_count = device.get_attribute(DeviceAttribute::MultiprocessorCount)? as u32;
        grid_size = sm_count * func.max_active_blocks_per_multiprocessor(block_size.into(), 0)?;

        Ok(Self { func, block_size, grid_size })
    }

    pub fn get_workload(&self) -> u32 {
        self.block_size * self.grid_size
    }

    pub fn set_workload(&mut self, workload: u32) {
        self.grid_size = (workload + self.block_size - 1) / self.block_size
    }
}

pub struct GPUWork<'gpu> {
    _context: Context,
    _module: Rc<Module>,

    pub workload: usize,
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

impl<'gpu> GPUWork<'gpu> {
    pub fn new(device_id: u32, workload: f32, is_absolute: bool) -> Result<Self, Error> {
        let device = Device::get_device(device_id).unwrap();
        let _context = Context::create_and_push(ContextFlags::MAP_HOST | ContextFlags::SCHED_AUTO, device)?;

        let major = device.get_attribute(DeviceAttribute::ComputeCapabilityMajor)?;
        let minor = device.get_attribute(DeviceAttribute::ComputeCapabilityMinor)?;
        let _module: Rc<Module>;
        info!("Device #{} compute version is {}.{}", device_id, major, minor);
        if major > 6 || (major == 6 && minor >= 1) {
            _module = Rc::new(Module::from_str(PTX_61).or_else(|e| {
                error!("Error loading PTX: {}", e);
                Result::Err(e)
            })?);
        } else if major >= 3 {
            _module = Rc::new(Module::from_str(PTX_30).or_else(|e| {
                error!("Error loading PTX: {}", e);
                Result::Err(e)
            })?);
        } else if major >= 3 {
            _module = Rc::new(Module::from_str(PTX_20).or_else(|e| {
                error!("Error loading PTX: {}", e);
                Result::Err(e)
            })?);
        } else {
            return Err("Cuda compute version not supported".into());
        }

        let stream = Stream::new(StreamFlags::NON_BLOCKING, None)?;

        let mut rand_init = Kernel::new(Rc::downgrade(&_module), "init")?;
        let mut pow_hash_kernel = Kernel::new(Rc::downgrade(&_module), "pow_cshake")?;
        let mut matrix_mul_kernel = Kernel::new(Rc::downgrade(&_module), "matrix_mul")?;
        let mut heavy_hash_kernel = Kernel::new(Rc::downgrade(&_module), "heavy_hash_cshake")?;

        let mut chosen_workload = 0 as usize;
        if is_absolute {
            chosen_workload = 1;
        } else {
            for ker in [&pow_hash_kernel, &matrix_mul_kernel, &heavy_hash_kernel] {
                let cur_workload = ker.get_workload();
                if chosen_workload == 0 || chosen_workload < cur_workload as usize {
                    chosen_workload = cur_workload as usize;
                }
            }
        }
        chosen_workload = (chosen_workload as f32 * workload) as usize;
        info!("GPU #{} Chosen workload: {}", device_id, chosen_workload);
        for ker in [&mut rand_init, &mut pow_hash_kernel, &mut matrix_mul_kernel, &mut heavy_hash_kernel] {
            ker.set_workload(chosen_workload as u32);
        }

        let mut rand_state = unsafe { DeviceBuffer::<CurandStateSobol64>::zeroed(chosen_workload).unwrap() };

        let nonces_buff = vec![0u64; chosen_workload].as_slice().as_dbuf()?;
        let pow_hashes_buff = vec![[0u8; 32]; chosen_workload].as_slice().as_dbuf()?;
        let matrix_mul_out_buff = vec![[0u8; 32]; chosen_workload].as_slice().as_dbuf()?;

        let final_hashes_buff = vec![[0u8; 32]; heavy_hash_kernel.grid_size as usize].as_slice().as_dbuf()?;
        let final_nonces_buff = vec![0u64; heavy_hash_kernel.grid_size as usize].as_slice().as_dbuf()?;

        info!("GPU #{} is generating initial seed. This may take some time.", device_id);
        let func = rand_init.func;
        let mut seeds = vec![1u64; 64 * chosen_workload];
        seeds.try_fill(&mut rand::thread_rng())?;
        unsafe {
            launch!(
                func<<<rand_init.grid_size, rand_init.block_size, 0, stream>>>(
                    seeds.as_slice().as_dbuf()?.as_device_ptr(),
                    rand_state.as_device_ptr(),
                    chosen_workload,
                )
            )?;
        }
        stream.synchronize().or_else(|e| {
            error!("GPU #{} init failed: {}", device_id, rand_state.len());
            Err(e)
        })?;
        info!("GPU #{} initialized", device_id);
        Ok(Self {
            _context,
            _module: Rc::clone(&_module),
            workload: chosen_workload,
            stream,
            rand_state,
            nonces_buff,
            pow_hashes_buff,
            matrix_mul_out_buff,
            final_hashes_buff,
            final_nonces_buff,
            pow_hash_kernel,
            matrix_mul_kernel,
            heavy_hash_kernel,
        })
    }

    #[inline(always)]
    pub(crate) fn calculate_pow_hash(&mut self, hash_header: &[u8; 72], nonces: Option<&Vec<u64>>) {
        let func = &self.pow_hash_kernel.func;
        let stream = &self.stream;
        let mut generate = true;
        if let Some(inner) = nonces {
            self.nonces_buff.copy_from(inner).unwrap();
            generate = false;
        }
        let mut hash_header_gpu = self._module.get_global::<[u8; 72]>(&CString::new("hash_header").unwrap()).unwrap();
        hash_header_gpu.copy_from(hash_header);
        unsafe {
            launch!(
                func<<<
                    self.pow_hash_kernel.grid_size, self.pow_hash_kernel.block_size,
                    0, stream
                >>>(
                    self.nonces_buff.as_device_ptr(),
                    self.nonces_buff.len(),
                    self.pow_hashes_buff.as_device_ptr(),
                    generate,
                    self.rand_state.as_device_ptr(),
                )
            )
            .unwrap(); // We see errors in sync
        }
    }

    #[inline(always)]
    pub(crate) fn calculate_matrix_mul(&mut self, matrix: &[[u16; 64]; 64]) {
        let func = &self.matrix_mul_kernel.func;
        let stream = &self.stream;
        let mut matrix_gpu = self._module.get_global::<[[u16; 64]; 64]>(&CString::new("matrix").unwrap()).unwrap();
        matrix_gpu.copy_from(matrix);
        unsafe {
            launch!(
                func<<<
                    (32, self.matrix_mul_kernel.grid_size),
                    (1, self.matrix_mul_kernel.block_size),
                    0, stream
                >>>(
                        self.pow_hashes_buff.as_device_ptr(),
                        self.pow_hashes_buff.len(),
                        self.matrix_mul_out_buff.as_device_ptr()
                )
            )
            .unwrap(); // We see errors in sync
        }
        // TODO: synchronize?
    }

    #[inline(always)]
    pub(crate) fn calculate_heavy_hash(&mut self) {
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
            )
            .unwrap(); // We see errors in sync
        }
    }

    #[inline(always)]
    pub(crate) fn sync(&self) -> Result<(), Error> {
        self.stream.synchronize()?;
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn get_output_size(&self) -> usize {
        self.heavy_hash_kernel.grid_size as usize
    }

    #[inline(always)]
    pub(crate) fn copy_output_to(&self, hashes: &mut Vec<[u8; 32]>, nonces: &mut Vec<u64>) -> Result<(), Error> {
        self.final_hashes_buff.copy_to(hashes)?;
        self.final_nonces_buff.copy_to(nonces)?;
        Ok(())
    }

    pub fn set_current(&self) {
        CurrentContext::set_current(&self._context).unwrap();
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
