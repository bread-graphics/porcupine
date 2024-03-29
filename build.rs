// Boost/Apache2 License

//! The build flags emitted by this script are not public API.

fn main() {
    let cfg = match autocfg::AutoCfg::new() {
        Ok(cfg) => cfg,
        Err(e) => {
            println!(
                "cargo:warning=porcupine: failed to detect compiler features: {}",
                e
            );
            return;
        }
    };

    // We use "no_*" instead of "has_*" here. For non-Cargo
    // build tools that don't run build.rs, the negative
    // allows us to treat the current Rust version as the
    // latest stable version, for when version information
    // isn't available.
    if !cfg.probe_rustc_version(1, 64) {
        autocfg::emit("porcupine_no_cstr_in_core");
    }
}
