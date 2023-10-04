use m17::CallSign;


fn main() {

    let b = CallSign::new_broadcast();
    let me = CallSign::new_id("df1bBL");

    println!("b {:?}", &b);
    println!("me {:?}", &me);

    println!("b {}", b.to_string());
    println!("me {}", me.to_string());

    let p = me.encode();
    let n = CallSign::from_bytes(p);
    println!("parsed {}", n.to_string());
}
