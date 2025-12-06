use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    let proto_dir = Path::new(&out_dir).join("proto");
    fs::create_dir_all(&proto_dir).unwrap();

    if !(Path::exists(&proto_dir.join("event.proto"))
        && Path::exists(&proto_dir.join("vector.proto")))
    {
        let event = reqwest::blocking::get(
            "https://github.com/vectordotdev/vector/raw/refs/heads/master/lib/vector-core/proto/event.proto")
            .unwrap();
        let vector = reqwest::blocking::get(
            "https://github.com/vectordotdev/vector/raw/refs/heads/master/proto/vector/vector.proto")
            .unwrap();
        std::fs::write(proto_dir.join("event.proto"), event.text().unwrap()).unwrap();
        std::fs::write(proto_dir.join("vector.proto"), vector.text().unwrap()).unwrap();
    }
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&proto_dir)
        .compile_protos(
            &[
                &proto_dir.join("vector.proto"),
                &proto_dir.join("event.proto"),
            ],
            &[&proto_dir, &Path::new("/usr/include").to_path_buf()],
        )
        .unwrap();
}
