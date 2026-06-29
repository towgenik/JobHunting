# Agent Rules — Wiki Vault

This is the career knowledge graph for the JobHunting agent.
Follow these rules when generating CVs.

## Structure

- `System/index.md` — central catalog: keywords/topics → node links
- `wiki/` — interlinked concept nodes (skills, projects, roles)
- `raw/` — immutable source documents (original CV, reviews, notes)
- `portfolio/` — images referenced by wiki nodes

## Traversal Contract

1. **Read `System/index.md` first** — it maps keywords to wiki nodes.
2. **Fetch only relevant nodes** — don't load the whole wiki.
3. **Follow `[[wikilinks]]`** — each node links to related nodes.
4. **Budget**: you have `ctx_window` tokens of context. Traverse judiciously.
5. **Never fabricate** — only use material present in the wiki nodes you read.
6. **Stop when you have enough** — don't traverse for the sake of completeness.

## CV Output Contract

Return JSON matching the schema provided in the prompt. Fields:
- `summary`: 2-3 sentences, lead with most relevant strength for THIS role.
- `skills`: 5-15 skills from the wiki that match the JD.
- `experiences`: ≥1 entry, each with quantified bullet points.
- `constraints`: what you cannot provide from the wiki (omit if none).

## Self-Healing

When editing wiki nodes:
- Every mention of a skill/project/concept should be `[[linked]]`.
- Orphan nodes (no backlinks) should be linked from the index.
- Contradictions between nodes should be flagged, not silently merged.
