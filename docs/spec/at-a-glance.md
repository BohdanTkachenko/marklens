# The DSL at a glance

A schema is a skeletal markdown file: an optional `--- … ---` frontmatter block
of typed keys, then a body skeleton of headings and typed (optionally nested)
lists. The `@name` alias sits **right after the marker**; a `<? description ?>`
trails the line.

```markdown
---
title: string                        <? human title, also the H1 ?>
status: enum(planned, partial, implemented, wontfix)  <? lifecycle state ?>
owner?: string
---

## @test_plan Test plan
  - [ ] @cases                       <? one checkbox per behavior ?>

## @manual Manual verification
  ### @setup Setup                   <? preconditions for the run ?>
    - +@items
  ### @procedure Procedure
    1. +@steps                       <? ordered, reproducible steps ?>
      - ?@note                       <? optional note under a step ?>
  ### @expect Expectations
    - [ ] +@checks
```

The same text — cardinality stripped, placeholders filled — is the **scaffold**
for a new document ([Scaffolding](./operations.md#scaffolding)). The `@names`
define the shape of the **extracted data object**
([the data model](./data-model.md)) and the addressing for **edits**
([Edit in-place](./operations.md#edit-in-place)). Indenting the `###` lines
under their `##` reads well but is optional — headings nest by level regardless.
