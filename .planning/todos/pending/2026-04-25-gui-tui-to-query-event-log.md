---
created: 2026-04-25T08:52:55.966Z
title: GUI / TUI to query the event log
area: tooling
files: []
---

## Problem

There is no way to inspect or query the event log without writing a custom application. Users exploring the library or debugging production systems have no lightweight tool to browse events, filter by stream or tag, or replay projections interactively.

## Solution

Build a GUI or TUI that connects to a logstore and lets users query the event log interactively — filter by stream ID, tag, event type, or sequence range, and optionally apply a projection definition to see the current read model. Could ship as one or more example applications in an `examples/` directory rather than as a first-class crate, keeping it out of the core library surface area.
