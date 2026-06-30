# Terminal In-Tab Split Plan

## Goal

Add a new terminal feature, "Split within", that creates splits inside the
current terminal tab instead of creating a new top-level Limux pane. Keep the
existing workspace-level pane split behavior unchanged.

The intended long-term model is:

- Workspace
- Workspace split tree
- Pane
- Tab
- Terminal split tree
- Leaf
- Ghostty surface

Each terminal split leaf owns exactly one Ghostty surface. The split tree is a
layout/container model, not a single shared Ghostty surface.

## Naming

- User-facing:
  - "Split within"
  - "terminal split"
  - "surface"
- Internal:
  - `TerminalSplitTree`
  - `TerminalSplitNode`
  - `TerminalLeafState`
  - `leaf_id`

Avoid using "surface" to mean "layout node". A leaf contains a surface but is
not the same abstraction.

## Baseline Constraints

- Existing workspace-level split behavior must remain intact.
- Existing tabs continue to exist as they do today.
- Existing sessions must still load.
- Existing CLI/control behavior must continue to work for non-split terminal
  tabs.
- Vendored Ghostty remains read-only. Limux must integrate through the C API.

## Compatibility Scope

Compatibility is required at behavior boundaries, not in internal structure.

Required backward compatibility:

- existing sessions load
- existing CLI/control flows continue to work
- existing env-based auto-targeting continues to work
- existing workspace-level split behavior stays unchanged

Non-requirements:

- preserving the current internal `pane_id:tab_id` model in code
- preserving current runtime type shapes if external behavior stays the same
- keeping old tab-level surface identity as an internal implementation detail

## Current State Summary

The current host model is:

- One terminal tab owns one `TerminalWidget`
- One `TerminalWidget` owns one Ghostty surface
- Surface identity is effectively `pane_id:tab_id`
- Persistence stores workspace-level split trees and pane tabs
- Hover focus is per terminal `GLArea`
- Control-bridge terminal targeting resolves at pane/tab surface granularity

This is too narrow for in-tab terminal splits.

## Required High-Level Changes

### 1. Re-model terminal tabs around a split tree

Change terminal-tab content from "single terminal surface" to "terminal split
tree whose initial shape is one leaf". This removes the mode switch between
plain terminal tabs and split terminal tabs.

Target shape:

- Fresh terminal tab -> one-leaf terminal tree
- "Split within" -> mutate that tree
- Close leaf -> collapse siblings as needed

### 2. Introduce leaf identity and surface identity v2

Change terminal surface identity from:

- `pane_id:tab_id`

to:

- `pane_id:tab_id:leaf_id`

Requirements:

- `leaf_id` must be stable for persistence and agent restore
- each terminal leaf must own its own Ghostty surface handle
- each tab must track the active leaf

Compatibility:

- old persisted `pane_id:tab_id` identifiers must still load
- non-split tabs should still behave as a one-leaf case

### 3. Extend the persistence model

Current session persistence only understands:

- workspace layout split tree
- panes
- tabs inside panes

Add terminal-tab subtree persistence:

- a serialized terminal split tree per terminal tab
- orientation
- ratio
- leaf order
- active leaf
- per-leaf terminal metadata
  - `leaf_id`
  - cwd
  - restorable agent metadata

Migration:

- old terminal tabs load as a one-leaf tree
- old sessions require no user migration step

### 4. Rework terminal tab runtime composition

Replace the terminal tab content root from:

- one `TerminalWidget`

to:

- a GTK container that renders a terminal split tree

Needed runtime operations:

- split active leaf
- close active leaf
- focus adjacent leaf
- resize splits
- equalize splits
- optional zoom active split later

The existing workspace split tree code should be evaluated for reuse at tab
scope, but the terminal-tab case should not be forced into pane terminology.

### 5. Rework focus semantics

Focus must become leaf-aware.

Needed behavior:

- active terminal tab has an active leaf
- keyboard routing goes to the active leaf surface
- hover focus updates active leaf when enabled
- focused surface reporting in control payloads resolves to leaf surface id

The current hover-focus path is already per terminal `GLArea`, which is a good
fit if each leaf continues to own its own `GLArea`.

### 6. Rework control-bridge targeting and discovery

All terminal-surface APIs need to resolve at leaf level:

- `surface.list`
- `pane.surfaces`
- `surface.send_text`
- `surface.send_key`
- `surface.read_text`
- `surface.health`
- focused surface discovery
- `pane.create` follow-up targeting behavior where relevant

Behavioral requirements:

- leaf-level surfaces are discoverable
- the active leaf is explicit
- non-split terminal tabs still behave naturally

### 7. Rework env wiring and agent restore

Every launched shell in a leaf must inherit:

- `LIMUX_WORKSPACE_ID`
- `LIMUX_PANE_ID`
- `LIMUX_TAB_ID`
- `LIMUX_SURFACE_ID`
- `LIMUX_SOCKET`

with `LIMUX_SURFACE_ID = pane:tab:leaf`.

Agent restore must also bind to leaf-level surface ids, not tab-only ids.

### 8. Route terminal events to leaves and bubble upward

These events currently assume one terminal surface per terminal tab:

- title changes
- cwd changes
- bell
- desktop notifications
- child exit
- unread state

New model:

- event attaches to leaf
- leaf bubbles to tab/workspace UI
- tab presentation can summarize leaf activity without losing leaf identity

### 9. Extend Ghostty FFI and runtime integration

The local FFI bindings are behind current upstream split-oriented surface API.

Investigate and integrate:

- `ghostty_surface_inherited_config`
- `ghostty_surface_split`
- `ghostty_surface_split_focus`
- `ghostty_surface_split_resize`
- `ghostty_surface_split_equalize`

Notes:

- Ghostty exposes split-related API, but the embedder still owns host UI state
  and persistence
- we should not assume Ghostty will create/manage nested GTK widgets for Limux
- this feature may still be implemented mostly on the Limux side even if some
  Ghostty split actions become useful

### 10. Add UI entry points without regressing current behavior

Add a new visible action called "Split within".

Likely surfaces:

- terminal right-click context menu
- pane/tab header actions where appropriate
- shortcut system

Keep existing actions unchanged:

- split right/down at workspace pane level
- open browser in split

The new action must be clearly distinct from top-level pane splitting.

## Proposed Data Model

### Runtime

- `TerminalTabRuntime`
  - `tree: TerminalSplitTree`
  - `active_leaf_id: String`
- `TerminalSplitTree`
  - `root: TerminalSplitNode`
- `TerminalSplitNode`
  - `Leaf(TerminalLeafRuntime)`
  - `Split { orientation, ratio, start, end }`
- `TerminalLeafRuntime`
  - `leaf_id: String`
  - `surface_id: String`
  - `handle: TerminalHandle`
  - `cwd`
  - optional agent restore metadata

### Persisted

- terminal tab content:
  - `tab_kind = terminal`
  - `terminal_layout = ...`
- terminal layout node:
  - `leaf`
  - `split`
- terminal leaf state:
  - `leaf_id`
  - `cwd`
  - `agent`

## Compatibility Strategy

### Old session load

- old `tab_kind = terminal` without terminal split-tree payload
  - load as one-leaf tree
- old surface ids
  - treat as legacy tab-level identifiers during restore only

### Old CLI targeting behavior

- if a tab has one leaf, current targeting remains effectively unchanged
- if a tab has multiple leaves, explicit leaf-aware surface ids become canonical
- old tab-level selectors should be accepted at the boundary as aliases where
  needed, but do not need to remain first-class internal identities

## Delivery Phases

### Phase 0. Baseline and compile/test validation

- initialize and verify Ghostty submodule state
- build Ghostty embedded library if needed
- run current compile/test/quality gate
- record baseline failures separately from feature work

### Phase 1. Runtime tree for terminal tabs

- create terminal split-tree runtime container
- render a one-leaf terminal tab through that container
- keep current behavior identical for the one-leaf case

### Phase 2. Identity and model groundwork

- introduce `leaf_id`
- introduce `surface_id = pane:tab:leaf`
- add session schema extensions and migration path
- keep backward compatibility at the external boundaries only

### Phase 3. "Split within" mutation path

- add split-within action
- split active leaf
- close/collapse leaf paths
- active leaf tracking

### Phase 4. Control bridge and identity plumbing

- update surface discovery payloads
- update send/read/health routing
- update env vars and agent restore

### Phase 5. Persistence and restore

- persist split-within trees
- restore split-within trees on startup
- verify old sessions still load

### Phase 6. UX polish and parity

- hover focus within split trees
- leaf focus navigation
- notifications/unread handling
- shortcut integration
- tab/pane UI affordances

### Phase 7. Validation

- unit tests for tree transforms and migrations
- integration coverage for control discovery and routing
- smoke coverage for persistence, hover focus, and agent restore

## Risks

### Identity churn

Changing surface ids is a repo-wide concern. Control APIs, env wiring, restore,
and discovery will all break if this change is partial.

### Model duplication

There is a risk of maintaining two similar split-tree implementations:

- workspace split tree
- terminal-tab split tree

We should reuse the generic parts where clean, but avoid over-generalizing too
early if it complicates rollout.

### Restore ambiguity

Legacy restore paths and agent metadata currently assume tab-level surface
identity. Migration must be explicit and deterministic.

### UI confusion

Users must be able to distinguish:

- split workspace pane
- split within tab

without ambiguous labels or shortcuts.

## Initial Validation Checklist

- project compiles from a clean baseline
- current `./scripts/check.sh` result is known
- Ghostty submodule/build prerequisites are known
- existing session load behavior is understood
- current control payload format is documented before changing ids

## Suggested First Implementation Slice

After baseline validation, the safest first code slice is:

- add terminal-tab runtime tree with one leaf only
- keep all current UX behavior unchanged
- preserve compatibility at the external behavior boundaries only

That creates the foundation for `Split within` without immediately touching
every user-facing path at once.
