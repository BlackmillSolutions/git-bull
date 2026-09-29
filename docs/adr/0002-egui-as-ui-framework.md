---
status: accepted
date: 2026-09-29
---

# egui as the UI framework

git-bull builds its interface with egui and eframe. We chose the framework
with a clean MIT/Apache licence and a stable source on crates.io, and accepted
that we build more widgets ourselves and that the result looks less native.

## Considered Options

- **GPUI with gpui-component** fits best technically. It has every widget the
  SourceTree layout needs and prior art for this exact problem: Zed's Git
  graph, GitComet and rgitui. It was rejected for two reasons:
  - Zed issue #55470, open since May 2026, reports that `gpui` links
    GPL-3.0-or-later code. That conflicts with ADR 0001.
  - The official `gpui` crate has been frozen at 0.2.2 since October 2025.
    `gpui-component` depends on a weekly snapshot published by a third party.
- **Tauri 2 with a web front end** offers the largest UI ecosystem. It was
  rejected because WebKitGTK performance problems on Linux persist, and the
  CEF runtime was still alpha. That conflicts with staying fluid on
  repositories with more than one million commits.
- **iced** and **Slint** were not pursued. iced has no virtualised list and an
  accessibility issue open since 2020. Slint has no tree view and no canvas
  API.

## Consequences

- We build the diff view, the tree view and the virtual list ourselves.
- egui positions content with 32-bit floats, so very long lists need a custom
  widget (see ADR 0007).
- egui's bundled fonts lack CJK coverage, so system fonts are loaded as
  fallback.
- egui has breaking changes in each release. The version is pinned and
  upgrades are separate, deliberate changes.
- The UI lives in its own crate (see ADR 0004), so revisiting this decision
  affects that crate only. GPUI is worth re-evaluating once the licence
  question is resolved and an official release exists.

## Sources

Research of 2026-09-29.

- https://github.com/zed-industries/zed/issues/55470
- https://crates.io/crates/gpui-component
- https://v2.tauri.app/develop/debug/linux-graphics/
