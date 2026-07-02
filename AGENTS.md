# Agent Rules

- Treat `upstream` as read-only. Fetch and merge from it when needed, but never push our rewritten or locally modified code to `upstream`.
- Push repository changes only to our fork remote, normally `origin`, unless the user gives an explicit one-off instruction to do otherwise.
