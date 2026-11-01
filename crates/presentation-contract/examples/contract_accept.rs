use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let line = line.expect("read validation request");
        let request: serde_json::Value = serde_json::from_str(&line).expect("JSON request");
        println!("{}", fella_presentation_contract::evaluate(&request));
    }
}
