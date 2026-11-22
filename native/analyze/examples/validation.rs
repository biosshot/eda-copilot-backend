use std::io::{self, Read};
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let request = serde_json::from_str(&input).expect("JSON validation request");
    println!("{}", pcb_analyze::validation::validate_json(request));
}
