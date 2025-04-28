#[cfg(all(feature = "lttng", target_os = "linux"))]
fn gen_lttng_tracepoints() {
    use lttng_ust_generate::CIntegerType;
    use lttng_ust_generate::CTFType;
    use lttng_ust_generate::Generator;
    use lttng_ust_generate::Provider;
    use std::env;
    use std::path::PathBuf;

    let mut provider = Provider::new("futuresdr");
    let c = provider
        .create_class("samples")
        .add_field("block", CTFType::Integer(CIntegerType::U64))
        .add_field("samples", CTFType::Integer(CIntegerType::U64));
    c.instantiate("rx");
    c.instantiate("tx");

    let output_file_name = PathBuf::from(env::var("OUT_DIR").unwrap()).join("tracepoints.rs");

    Generator::default()
        .generated_lib_name("futuresdr")
        .register_provider(provider)
        .output_file_name(&output_file_name)
        .generate()
        .expect("Unable to generate tracepoint bindings");

    let bindings = std::fs::read_to_string(&output_file_name).expect("output file name not found");
    let bindings = bindings.replace("pub(", "#[allow(dead_code)]\npub(");
    std::fs::write(output_file_name, bindings).expect("writing back bindings failed");
}

fn main() {
    #[cfg(all(feature = "lttng", target_os = "linux"))]
    gen_lttng_tracepoints();
}
