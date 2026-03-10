# Pipeflow – Product Requirements Document (PRD)

## 1. Overview

### 1.1 Purpose

This document defines the complete product requirements for **Pipeflow**, a next‑generation PipeWire graph and control application. It is intended to be sufficiently precise and verifiable so it can be directly decomposed into **epics, user stories, and tickets**.

### 1.2 Problem Statement

Existing PipeWire tools either:

* expose raw power with poor UX (pw-cli), or
* provide limited, fragmented control (pavucontrol, qpwgraph), or
* visualize the graph without meaningful control (Helvum)

Better Helvum aims to become a **full‑fledged PipeWire control surface**, combining visual routing, live control, and reproducibility.

### 1.3 Goals

* Full read/write control over PipeWire graphs
* Safe, predictable behavior under live audio conditions
* Scales from laptops to complex studio setups
* Suitable for both daily use and live performance

### 1.4 Non‑Goals

* Audio DSP processing
* DAW replacement
* PipeWire configuration management (system‑level)

---

## 2. Target Users

### 2.1 Audio Professionals / Musicians

* Live routing changes
* Low‑latency awareness
* Stage‑safe UX

### 2.2 Power Users / Streamers

* Rapid reconfiguration
* Application‑centric workflows
* Preset‑based routing

### 2.3 Developers

* Debugging PipeWire graphs
* Introspection of nodes and ports
* Deterministic graph snapshots

---

## 3. Functional Requirements

### FR‑1: Graph Visualization & Interaction

**Description**
The application shall present a real‑time visual representation of the PipeWire graph and allow direct manipulation.

**Requirements**

* Display nodes and ports in a directed graph
* Support pan and zoom
* Drag nodes freely
* Multi‑select nodes

**Definition of Done (DoD)**

* [ ] Graph updates within ≤100ms of PipeWire change
* [ ] Nodes can be repositioned via drag
* [ ] Node positions persist across app restarts
* [ ] Zoom and pan do not affect graph correctness
* [ ] Multi‑selection works with mouse and keyboard

---

### FR‑2: Link Management (Create / Remove / Toggle)

**Description**
Users must be able to fully manage links between ports.

**Requirements**

* Create links between compatible ports
* Remove existing links
* Enable/disable links without deleting them

**Definition of Done (DoD)**

* [ ] User can create a link via drag or context menu
* [ ] User can remove a link via UI interaction
* [ ] Disabled links stop audio flow but remain visible
* [ ] UI state always reflects actual PipeWire state
* [ ] Removing a link never crashes PipeWire or the app

---

### FR‑3: Node Inspection Panel

**Description**
Selecting a node opens a detailed inspection panel.

**Requirements**

* Display metadata (name, client, media class, ID)
* List all ports
* Display format and channel count

**Definition of Done (DoD)**

* [ ] Selecting a node opens inspection UI within 50ms
* [ ] Metadata matches PipeWire reported values
* [ ] Port list updates dynamically on change
* [ ] No stale data after reconnects or restarts

---

### FR‑4: Volume, Mute & Channel Control

**Description**
Users can control audio parameters per node.

**Requirements**

* Master volume per node
* Per‑channel volume (where applicable)
* Mute / unmute

**Definition of Done (DoD)**

* [ ] Volume changes apply immediately (<20ms)
* [ ] Per‑channel controls reflect actual channel count
* [ ] Mute state persists across restarts
* [ ] External volume changes are reflected in UI

---

### FR‑5: Live Signal Metering

**Description**
Visual feedback for audio activity.

**Requirements**

* Per‑node level meters
* Optional per‑port meters
* Configurable refresh rate

**Definition of Done (DoD)**

* [ ] Meter values update in real time
* [ ] Meter refresh rate is user‑configurable
* [ ] Meters can be globally disabled
* [ ] CPU usage remains bounded (<5% on typical system)

---

### FR‑6: Graph Filtering & Organization

**Description**
Users can reduce complexity via filtering and grouping.

**Requirements**

* Filter by application, media class, direction
* Manual node groups
* Collapsible groups

**Definition of Done (DoD)**

* [ ] Filters can be toggled independently
* [ ] Grouped nodes move as a unit
* [ ] Collapsing a group hides internal nodes
* [ ] Group membership persists across restarts

---

### FR‑7: Snapshots & Presets

**Description**
Users can save and restore routing and control states.

**Requirements**

* Save full graph snapshot
* Restore snapshot on demand
* Partial restore (routing only, volumes only)

**Definition of Done (DoD)**

* [ ] Snapshot contains nodes, links, volumes, mutes
* [ ] Restoring snapshot reproduces identical routing
* [ ] Partial restore affects only selected dimensions
* [ ] Snapshot restore is idempotent

---

### FR‑8: Search & Command Palette

**Description**
Fast access to actions via keyboard.

**Requirements**

* Global command palette
* Fuzzy search
* Action execution from text

**Definition of Done (DoD)**

* [ ] Palette opens via keyboard shortcut
* [ ] Actions execute correctly from text command
* [ ] Invalid commands fail gracefully
* [ ] Palette is extensible for future commands

---

### FR‑9: Safety & Stage Mode

**Description**
Prevent accidental destructive actions.

**Requirements**

* Read‑only mode
* Routing lock
* Panic actions

**Definition of Done (DoD)**

* [ ] Read‑only mode prevents all state changes
* [ ] Locked routing cannot be modified accidentally
* [ ] Panic mute disconnects all outputs reliably
* [ ] Visual indicators show safety state clearly

---

## 4. Non‑Functional Requirements

### NFR‑1: Performance

**Definition of Done (DoD)**

* [ ] Handles ≥500 nodes without UI lag
* [ ] Graph updates are incremental
* [ ] UI frame rate ≥60 FPS with meters disabled

---

### NFR‑2: Reliability & Consistency

**Definition of Done (DoD)**

* [ ] App recovers from PipeWire restart
* [ ] External changes are reflected within 200ms
* [ ] No desync between UI model and PipeWire state

---

### NFR‑3: Usability

**Definition of Done (DoD)**

* [ ] All core actions are keyboard accessible
* [ ] No destructive action without confirmation (unless panic)
* [ ] Clear visual hierarchy at all zoom levels

---

## 5. MVP Scope

### Included

* FR‑1 through FR‑4
* Basic snapshot save/load
* Persistent graph layout

### Excluded

* Plugin system
* OSC/MIDI control
* Advanced analyzers

---

## 6. Acceptance Criteria Summary

The product is considered **MVP‑complete** when:

* All MVP functional requirements meet their DoD
* All NFRs pass basic stress tests
* The application can replace Helvum + pavucontrol for daily use

---

## 7. Next Steps

* Decompose FRs into epics
* Define technical architecture
* UX wireframes
* Prototype implementation
