use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protowire_files = &["proto/messages.proto", "proto/rpc.proto", "proto/p2p.proto"];
    let dirs = &["proto"];

    println!("cargo:rerun-if-changed=proto");
    println!("cargo:rerun-if-changed=src/asm");
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        // .protoc_arg("--experimental_allow_proto3_optional") // no need for this if the proto files using optional are not passed directly to tonic_build
        // .type_attribute(".", "#[derive(Debug)]")
        .compile_protos(&protowire_files[0..1], dirs)?;

    for file in protowire_files {
        println!("cargo:rerun-if-changed={file}");
    }
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap();

    if target_arch == "x86_64" {
        let mut builder = cc::Build::new();
        builder.flag("-c");
        match target_os.as_str() {
            "macos" => builder.file("src/asm/keccakf1600_x86-64-osx.s"),
            "linux" => builder.file("src/asm/keccakf1600_x86-64-elf.s"),
            "windows" if target_env == "gnu" => builder.file("src/asm/keccakf1600_x86-64-mingw64.s"),
            "windows" if target_env == "msvc" => builder.file("src/asm/keccakf1600_x86-64-msvc.asm"),
            _ => unimplemented!("Unsupported OS"),
        };
        builder.compile("libkeccak.a");
    }
    Ok(())
}
