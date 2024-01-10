use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (proto_path, proto_dir) = compile_args("user-service-proto/service.proto");
    tonic_build::configure()
        .build_server(false)
        .type_attribute(".", "#[cfg_attr(feature = \"serde\", derive(serde::Serialize, serde::Deserialize))]")
        .extern_path(".google.protobuf.Struct", "::prost_wkt_types::Struct")
        .compile(&proto_path, &proto_dir)?;

    Ok(())
}

/// tonic-build/src/prost.rs — compile_protos()
fn compile_args(proto: &str) -> ([impl AsRef<Path> + '_; 1], [impl AsRef<Path> + '_; 1]) {
    let proto_path: &Path = proto.as_ref();

    // directory the main .proto file resides in
    let proto_dir = proto_path
        .parent()
        .expect("proto file should reside in a directory");

    ([proto_path], [proto_dir])
}
