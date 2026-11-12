# Boundaries

Scarab owns two things: **config loading** (discovery + formats,
generic over the model) and **templated content packs** (the
`type: template` driver). Everything adjacent belongs to a neighbor.

## Neighbors

| Repo                        | Owns                                                                         | Scarab's relation                                                                                                                                                      |
| --------------------------- | ---------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **tamias**                  | Content distribution — refs, fetch, store, install pipeline, driver dispatch | Scarab ships _inside_ tamias's world: `tamias-driver-template` is a content driver; template packs are tamias packages. Scarab never reimplements fetch/install/store. |
| **cuora**                   | Secret values — sealed, not spilled                                          | Config carries secret _names_, never values. A template var named `api_key` holds a reference/name; cuora resolves it.                                                 |
| **tarsi**                   | Release plane — changelog → bump → tag → publish                             | A consumer: `tarsi.toml`/profile loading rides `scarab-config`.                                                                                                        |
| **myco / kuda / mimus**     | Work ledger, code-map, verification                                          | Consumers of the config port for their own `<name>.yaml`/`.toml` discovery — the exact problem P-017 generalized.                                                      |
| **hypha**                   | Pure distributed substrate                                                   | The P-015 config crates were ported _here_, not kept there — hypha stays substrate-only.                                                                               |
| **peregrin / sepia / pavo** | Orchestration, sessions, agent UI                                            | May consume either plane; neither defines scarab's contracts.                                                                                                          |

## What scarab deliberately is not

- **Not a config schema registry.** It loads and renders; it doesn't
  validate semantics. Your serde model _is_ the schema.
- **Not a secrets store.** See cuora above — the boundary is
  enforced by convention (`secret names, never values`) and by
  cuora owning resolution.
- **Not a template marketplace.** Packs are ordinary tamias
  packages; distribution, provenance, and digests are tamias's
  job.
- **Not consumer-aware.** No crate in scarab may name another
  repo's `Config` type — that coupling is what the port exists to
  prevent.

## The naming story

_Scarabaeus sacer_ — the sacred scarab rolls the ball into place;
scarab amulets were seals and signets. Config as the seal that
validates what runs: discovery finds the right file, the format
parses it, the model validates it. Design history lives in the org
ledger — proposal P-017 (supersedes P-015).
