use std::env;
use std::path::Path;
use std::os::unix::process::CommandExt;

extern crate cc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(false)
        // .type_attribute(".", "#[derive(Debug)]")
        .compile(
            &["proto/rpc.proto", "proto/p2p.proto", "proto/messages.proto"],
            &["proto"],
        )?;

    if let Some(cuda_root) = env::var_os("CUDA_ROOT") {
        if let Some(path) = env::var_os("PATH") {
            let mut paths = env::split_paths(&path).collect::<Vec<_>>();
            paths.insert(0, Path::new(&cuda_root).join("bin"));
            let new_path = env::join_paths(paths).unwrap();
            env::set_var("PATH", &new_path);
        }
    } else {
        println!("cargo:warning=CUDA_ROOT is missing. Might cause problem");
    }
    // TODO: run only if needed
    // TODO: add option for the heavy build
    let mut command = cc::Build::new()
        .cuda(true)
        .flag("-std=c++11")
        .flag("-O3")
        .flag("--restrict")
        .flag("--ptx")
        .flag("--gpu-architecture=compute_61")
        .flag("-Xptxas").flag("-O3")
        .flag("-Xcompiler").flag("-O3")
        .opt_level(3)
        .debug(false)
        .target(&env::var("HOST")?)
        .get_compiler().to_command();
    command.arg("-o").arg("resources/kaspa-cuda-native.ptx")
           .arg("kaspa-cuda-native/src/kaspa-cuda.cu");

    Err(command.exec().into())
}
