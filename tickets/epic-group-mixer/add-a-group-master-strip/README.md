---
title: Add a group master strip
id: add-a-group-master-strip
status: open
description: Give each group mixer a master strip so the group can be controlled as a bus instead of only per-member channels.
tags:
- mixer
- feature
labels:
  area: ui
  component: mixer
priority: high
assignee: trusch
created: 2026-03-20T22:33:25.703888993+01:00
updated: 2026-03-21T17:03:26.315731636+01:00
---

# Add a group master strip

- [2026-03-21 17:03] Reassessment: the current pseudo-master model is not intuitive because it derives a fake master from member gains and rewrites child levels destructively. Follow-up work is now tracked in remove-the-fake-group-master-fader and spec-a-dedicated-mixer-bus-node.
