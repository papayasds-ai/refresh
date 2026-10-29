fn main() {
    uniffi::generate_scaffolding("src/refresh.udl").expect("failed to generate UniFFI scaffolding");
}
