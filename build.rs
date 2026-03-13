//! Build script for compiling protobuf definitions.

fn main() {
    #[cfg(feature = "network")]
    {
        let proto_file = "src/network/proto/pipeflow.proto";

        // Recompile if proto file changes
        println!("cargo:rerun-if-changed={}", proto_file);

        tonic_build::configure()
            .build_server(true)
            .build_client(true)
            // Suppress clippy warnings for generated code
            .type_attribute(
                ".",
                "#[allow(clippy::large_enum_variant, clippy::enum_variant_names)]",
            )
            .compile_protos(&[proto_file], &["src/network/proto"])
            .expect("Failed to compile protobuf definitions");
    }
}
