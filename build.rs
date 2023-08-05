use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto");
    println!("cargo:rerun-if-changed=src/asm");
    tonic_build::configure()
        .build_server(false)
        // .type_attribute(".", "#[derive(Debug)]")
        .compile(
            &["proto/rpc.proto", "proto/p2p.proto", "proto/messages.proto"],
            &["proto"],
        )?;
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap();

    if target_arch == "x86_64" {
        let mut builder = cc::Build::new();
        builder.flag("-c");
        match target_os.as_str() {
            "macos" => drop(builder.file("src/asm/keccakf1600_x86-64-osx.s")),
            "linux" => drop(builder.file("src/asm/keccakf1600_x86-64-elf.s")),
            "windows" if target_env == "gnu" => drop(builder.file("src/asm/keccakf1600_x86-64-mingw64.s")),
            "windows" if target_env == "msvc" => drop(builder.file("src/asm/keccakf1600_x86-64-msvc.asm")),
            _ => unimplemented!("Unsupported OS"),
        }
        builder.compile("libkeccak.a");
    }
    Ok(())
}
