# Template pack composition — the agnostic spec

`template.extends` + `template.merge` define how a `type: template`
pack composes with parent packs. This document is the **contract**:
any evaluator — the tamias driver today, a handl-lang eval or another
engine later — implements the same semantics from the same manifest
data. The manifest vocabulary is data; the source grammar belongs to
the resolver.

## Manifest vocabulary

```yaml
template:
  extends: git:org/base          # form 1: one parent, bare source
  # extends:                     # form 2: one structured parent
  #   source: git:org/base
  #   via: tamias-ref            #   resolution-scheme hint (optional)
  # extends:                     # form 3: several parents
  #   - git:org/platform         #   lowest precedence first
  #   - source: corp/overlay
  #     via: tamias-ref
  merge:                         # optional; defaults shown
    vars: child-wins             #   child-wins | parent-wins
    into: child-wins             #   child-wins | parent-wins | error
    max-depth: 8                 #   ancestor nesting cap (>= 1)
```

- `extends` absent/`null` → no declared parents. Materialized
  parents found anyway are honored (vendored composition).
- `extends: []` or a parent with an empty `source` → manifest error.
- `source` is an **opaque locator**. Its grammar is the resolver's
  business — tamias refs (`git:`, `cf:`, `http:`, paths, store
  names), local paths, kit refs, whatever a `via` scheme defines.
  Evaluators MUST NOT parse or interpret it.
- `via` (optional) names the resolution scheme. Absent = the ambient
  resolver's default. A resolver that doesn't implement a declared
  `via` MUST error — never silently fall back to another scheme.
  Known schemes today: `tamias-ref` (and its alias `tamias`); the
  bare-string/absent form means the same under a tamias install.
- `merge` on the **installing (root) pack** governs the whole
  composition. Ancestor packs' own `merge` tables describe how they
  compose when installed directly — they do not re-govern a
  descendant's composition.

## Evaluation contract

1. **Locate.** For each pack, find its parent *payload roots*
   (each a self-contained pack dir: manifest + `files[].src` + its
   own materialized ancestors, recursively). How parents are located
   or fetched is implementation-defined — the tamias driver reads
   the `.scarab-parent/` (single parent) / `.scarab-parents/<i>/`
   (multiple parents, index = declaration order) layout that
   `tamias install` writes; another evaluator may resolve `source`
   live. A locator returns dirs in declaration order (lowest
   precedence first).
2. **Validate counts.** When `extends` is declared, the number of
   located parents MUST equal the number of declarations — fewer or
   more is an error (the payload didn't come through its composition
   source's install, or the layout is corrupt).
3. **Order.** Compose a chain **ancestor-first** (post-order):
   a pack's parents render in declaration order, each parent's own
   ancestors before it, the pack itself last. A shared ancestor
   reachable through several paths renders **once**, at its first
   occurrence (diamonds collapse to tree order). Cycles through
   materialized dirs terminate — each dir is visited once.
4. **Vars.** Context layers, later winning: ancestor `template.vars`
   → … → descendant `template.vars` → auto vars → `--config` file →
   env → CLI args (the driver's existing layering; `merge.vars =
   parent-wins` inverts only the pack-`vars` fold). All var layers
   are flat `name → JSON value` maps merged key-wise.
5. **Render + `into` policy.** Render every chain entry's
   `files[]` in chain order against the one context, then apply
   `merge.into`:
   - `child-wins` (default): each `into` keeps its **last** render.
   - `parent-wins`: each `into` keeps its **first** render.
   - `error`: any duplicate `into` fails the composition.
6. **Depth.** `merge.max-depth` (default 8, must be ≥ 1) caps
   ancestor nesting; exceeding it is an error.

`uninstall`/state semantics are evaluator-side: record the digest of
the *final* written bytes per `into` — exactly what a
child-wins/default composition produces.

## What tamias owns vs. what scarab owns

- **scarab-template** owns this vocabulary (`Extends`, `ParentDecl`,
  `MergeRules`) and the evaluation contract (`compose_chain`,
  `apply_into_policy`, `PackLoader`/`ParentLocator` ports). It never
  fetches, never parses a `source` grammar.
- **tamias-install** owns `tamias-ref` *resolution*: store name →
  stored payload; anything else the ref grammar claims → recursive
  install. It materializes resolved parents into the child payload
  (`.scarab-parent/` for one parent, `.scarab-parents/<i>/` for
  several) so the *committed store payload* carries the whole chain —
  `deploy`/`uninstall` later see it with no fetch capability.
- **tamias-driver-template** owns the *materialized-parent* locator
  and the render/write/uninstall verbs, evaluating the merge rules
  above.

A second evaluator (e.g. handl-lang) implements `PackLoader` +
`ParentLocator` (or its own equivalent) and follows this spec — no
manifest migration required.
