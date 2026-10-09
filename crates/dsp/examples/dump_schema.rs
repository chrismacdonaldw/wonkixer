fn main() {
    let schema = wonkixer_dsp::params::schema();
    println!(
        "{}",
        serde_json::to_string_pretty(&schema).expect("schema serializes")
    );
}
