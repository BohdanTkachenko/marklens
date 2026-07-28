# A conforming document

```markdown
---
title: Dark mode for the dashboard
status: in_review
sprint: 42
owners: [alice, bob]
tracking: WEB-1234
---

# Dark mode for the dashboard

## Summary
Adds a dark theme toggle that persists across sessions.

## Rollout

### Feature flags
- dashboard_dark_mode
- theme_persistence

### Deploy steps
1. Ship the flag disabled
2. Enable for staff
   - flip the flag back off

## Verification
- [x] Toggle switches the theme
- [ ] Preference survives a reload

## Decisions

### Store the preference server-side
Accept. It follows the user across devices.

## Sign-off
- Approved by: design review
```

`schema.validate(doc)` → `[]` (conforms).

Frontmatter is checked now: a `status` outside the enum, a non-integer `sprint`,
a `tracking` value not matching `/^[A-Z]+-[0-9]+$/`, a missing required key, or
an undeclared key each produce a validation problem. `%strict` (the default) is
enforced too: an extra `## Rogue section` is reported as `unexpected section`.
