use std::any::Any;
use std::str::FromStr;
use std::error::Error as StdError;
pub mod xoshiro256starstar;
use libloading::{Library, Symbol};


const DEFAULT_WORKLOAD_SCALE: f32 = 16.;

pub type Error = Box<dyn StdError + Send + Sync + 'static>;

#[derive(Copy, Clone, Debug)]
pub enum GPUWorkType{
    CUDA,
    OPENCL
}

impl FromStr for GPUWorkType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CUDA" => Ok(Self::CUDA),
            "OPENCL" => Ok(Self::OPENCL),
            _ => Err(String::from("Unknown string"))
        }
    }
}

pub struct GPUWorkFactory {
    pub plugins: Vec<Box<dyn Plugin>>,
    loaded_libraries: Vec<Library>,
    type_: GPUWorkType,
    opencl_platform: u16,
    device_id: u32,
    workload: f32,
    is_absolute: bool
}

impl GPUWorkFactory {
    pub fn new() -> Self {
        Self{
            plugins: Vec::new(),
            loaded_libraries: Vec::new(),
            type_: GPUWorkType::CUDA, opencl_platform: 0, device_id: 0, workload: 16., is_absolute: false
        }
    }

    pub(crate) unsafe fn load_single_plugin<'help>(&mut self, app: clap::App<'help>, path: &str) -> Result<clap::App<'help>,Error> {
        type PluginCreate<'help> = unsafe fn(*const clap::App<'help>) -> (*mut clap::App<'help>, *mut dyn Plugin);

        let lib = Library::new(path).expect("Unable to load the plugin");
        self.loaded_libraries.push(lib);
        let lib = self.loaded_libraries.last().unwrap();

        let constructor: Symbol<PluginCreate> = lib.get(b"_plugin_create")
            .expect("The `_plugin_create` symbol wasn't found.");
        let app = Box::into_raw(Box::new(app));
        let (app, boxed_raw) = constructor(app);

        let plugin = Box::from_raw(boxed_raw);
        self.plugins.push(plugin);

        //Ok(Box::from_raw(app))
        Ok(*Box::from_raw(app))
    }

    pub fn build(&self) -> Result<Box<dyn WorkerSpec + 'static>, Error> {
        /*match self.type_ {
            GPUWorkType::CUDA  => Ok(Box::new(CudaGPUWork::new(self.device_id, self.workload, self.is_absolute)?)),
            GPUWorkType::OPENCL => {
                let platforms = get_platforms().unwrap();
                let platform = &platforms[self.opencl_platform as usize];
                let device_ids = platform.get_devices(CL_DEVICE_TYPE_ALL).unwrap();
                Ok(Box::new(OpenCLGPUWork::new(device_ids[self.device_id as usize], self.workload, self.is_absolute)?))
            } // TODO: return error
        }*/
        Ok(self.plugins.last().unwrap().get_worker_spec())
    }
}

pub trait Plugin: Any + Send + Sync {
    fn get_worker_spec(&self) -> Box<dyn WorkerSpec>;
}

pub trait WorkerSpec: Any + Send + Sync {
    fn build (&self) -> Box<dyn Worker>;
}

pub trait Worker {
    //fn new(device_id: u32, workload: f32, is_absolute: bool) -> Result<Self, Error>;
    fn id(&self) -> String;
    fn load_block_constants(&mut self, hash_header: &[u8; 72], matrix: &[[u16; 64]; 64], target: &[u64; 4]);

    fn calculate_hash(&mut self, nonces: Option<&Vec<u64>>);
    fn sync(&self) -> Result<(), Error>;

    fn get_workload(&self) -> usize;
    fn copy_output_to(&mut self, nonces: &mut Vec<u64>) -> Result<(), Error>;
    //pub(crate) fn check_random(&self) -> Result<(),Error>;
    //pub(crate) fn copy_input_from(&mut self, nonces: &Vec<u64>);
}

pub fn load_plugins<'help>(app: clap::App<'help>, paths: &[&str]) -> Result<(clap::App<'help>, GPUWorkFactory),Error> {
    let mut factory = GPUWorkFactory::new();
    let mut app = app;
    for path in paths {
        app = unsafe { factory.load_single_plugin(app, *path)? };
    }
    Ok((app, factory))
}

#[macro_export]
macro_rules! declare_plugin {
    ($plugin_type:ty, $constructor:path, $args:ty) => {
        use clap::Args;
        #[no_mangle]
        pub extern "C" fn _plugin_create(app: *mut clap::App) -> (*mut clap::App, *mut dyn $crate::Plugin) {
            // make sure the constructor is the correct type.
            let constructor: fn() -> $plugin_type = $constructor;

            let object = constructor();
            let boxed: Box<dyn $crate::Plugin> = Box::new(object);

            let boxed_app = Box::new(<$args>::augment_args(unsafe{*Box::from_raw(app)}));
            (Box::into_raw(boxed_app), Box::into_raw(boxed))
        }
    };
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
