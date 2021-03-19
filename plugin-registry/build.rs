fn main() {
    tonic_build::compile_protos("proto/plugin/plugin.proto").unwrap();
}