# Documentation

Setup, configuration and day-to-day commands live in the
[top-level README](../README.md). These are the deeper references.

| Document | What it covers |
| --- | --- |
| [plugin-system/plugin-developer-guide.md](plugin-system/plugin-developer-guide.md) | Writing, packaging and installing a plugin. Manifest reference, the `Ferum.*` JavaScript API, hooks, RPC actions, UI slots. |
| [plugin-system/technical-design.md](plugin-system/technical-design.md) | How the plugin runtime works internally: tiers, lifecycle, hook dispatch, circuit breaker, SQL isolation, security model. |
| [db-entities-and-testing.md](db-entities-and-testing.md) | Regenerating Sea-ORM entities from migrations, and the per-test database harness. |
| [i18n.md](i18n.md) | Locale catalogs, key namespaces, locale negotiation, `/admin/languages`. |
| [security-audit-checklist.md](security-audit-checklist.md) | Pre-release audit procedure, with PASS/FAIL criteria per vulnerability class. |

Removed in 2026-08: `i18n-plan.md`, `unit-testing-plan.md` and `marketplace-spec.md`.
The first two were design plans that have since been implemented — `i18n.md` above
carries what remained useful from the first, and the test layout from the second is
described in `db-entities-and-testing.md`. The third specified a plugin marketplace and
a SaaS multi-tenancy layer, neither of which was started, and its Phase 1 sections
described a WASM plugin runtime that was never built. All three are in git history.
