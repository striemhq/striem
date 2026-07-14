use std::path::Path;

fn main() {
    let proto = Path::new("proto/detection.proto");
    println!("cargo:rerun-if-changed=proto/detection.proto");

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&[proto], &[Path::new("proto")])
        .unwrap();
}
