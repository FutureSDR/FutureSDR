use std::env;
use std::path::PathBuf;

use lttng_ust_generate::{Provider, Generator, CTFType, CIntegerType};

fn main() {
    let mut provider = Provider::new("null_rand_latency");
    let c = provider.create_class("samples")
        .add_field("block", CTFType::Integer(CIntegerType::U64))
        .add_field("samples", CTFType::Integer(CIntegerType::U64));
    c.instantiate("rx");
    c.instantiate("tx");

    Generator::default()
        .generated_lib_name("null_rand_tracepoints")
        .register_provider(provider)
        .output_file_name(PathBuf::from(env::var("OUT_DIR").unwrap()).join("tracepoints.rs"))
        .generate()
        .expect("Unable to generate tracepoint bindings");

}
