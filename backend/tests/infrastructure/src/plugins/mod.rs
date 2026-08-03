#[cfg(test)] mod circuit_breaker;
// Unconditional: the sink has no boa dependency, so it is compiled in the lean
// build too. Gating this on `script_plugins` would disable it permanently —
// this test crate declares no such feature.
#[cfg(test)] mod log_sink;
#[cfg(test)] mod manifest_loader;
#[cfg(test)] mod package_extractor;
