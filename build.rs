fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_server(false)
        .extern_path(".google.protobuf.Struct", "::prost_wkt_types::Struct")
        .compile_protos(
            &["user-service-proto/service.proto"],
            &["user-service-proto"],
        )?;

    Ok(())
}
