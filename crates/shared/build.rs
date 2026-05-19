use std::{env, path::PathBuf};

fn main() {
    if env::var_os("PROTOC").is_none() {
        let protoc =
            protoc_bin_vendored::protoc_bin_path().expect("failed to find vendored protoc binary");
        env::set_var("PROTOC", protoc);
    }

    let project_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();

    tonic_build::configure()
        .build_server(true)
        .file_descriptor_set_path(PathBuf::from(out_dir).join("azookey_service_descriptor.bin"))
        .compile_protos(
            &[
                format!("{}/service.proto", project_dir),
                format!("{}/window.proto", project_dir),
            ],
            &[format!("{}", project_dir)],
        )
        .unwrap();
}
