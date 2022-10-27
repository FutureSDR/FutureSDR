use futuresdr::runtime::config;
use serde::Deserialize;

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct MyConfig {
    a: i32,
    b: String,
    #[serde(default = "c")]
    c: usize,
}

fn c() -> usize {
    42
}

fn main() {
    let c = config::config();
    println!("FutureSDR Config: {c:?}");

    if let Some(v) = config::get_value("my") {
        if let Ok(v) = v.try_deserialize::<MyConfig>() {
            println!("MyConfig: {:?}", &v);
        } else {
            println!("MyConfig could not be deserialized");
        }
    } else {
        println!("MyConfig not found");
    }
}
