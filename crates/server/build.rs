// Rebuild when the web app is rebuilt, so the embedded assets are never stale.
fn main() {
    println!("cargo:rerun-if-changed=../../web/dist");
}
