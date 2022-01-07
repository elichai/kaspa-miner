
#[derive(clap::Args, Debug)]
pub struct OpenCLOpt {
    #[clap(long = "opencl-platform", help = "Which OpenCL platform to use (limited to one per executable)")]
    pub opencl_platform: Option<u16>,
    #[clap(long = "opencl-device", use_delimiter = true, help = "Which OpenCL GPUs to use on a specific platform")]
    pub opencl_device: Option<Vec<u16>>,
    #[clap(
    long = "opencl-workload",
    help = "Ratio of nonces to GPU possible parrallel run in OpenCL [defualt: 16]"
    )]
    pub opencl_workload: Option<Vec<f32>>,
    #[clap(
    long = "opencl-workload-absolute",
    help = "The values given by workload are not ratio, but absolute number of nonces in OpenCL [default: false]"
    )]
    pub opencl_workload_absolute: bool,
    #[clap(
    long = "opencl-enable"
    )]
    pub opencl_enable: bool,
}
