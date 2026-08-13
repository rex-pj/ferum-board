// Mail adapters. None of these touch a real SMTP relay or api.resend.com — the
// pure decision functions are factored out of the adapters precisely so they can
// be asserted without one.
//
// There is deliberately no live-HTTP test of the Resend adapter. That would need
// `wiremock` or `httpmock`, and adding a crate for a single test would violate the
// dependency discipline this workspace holds elsewhere. What is left uncovered is
// the `reqwest` call itself; what is covered is every decision around it.
#[cfg(test)] mod lettre_service;
#[cfg(test)] mod reloadable_service;
#[cfg(test)] mod resend_service;
