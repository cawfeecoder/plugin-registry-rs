fn main() {
    tonic_build::compile_protos("proto/plugin/health.proto").unwrap();
    tonic_build::compile_protos("proto/plugin/plugin.proto").unwrap();
}