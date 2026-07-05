# Hierarchical Memory

Kota implements a two-tiered memory hierarchy to prevent context bloat:

1. **Working Memory**: In-memory message list limited to the current session.
2. **Episodic Memory**: SQL database (Turso/libSQL) that persists all conversational traces across sessions.

Fuzzy keyword matching retrieves previous relevant traces and injects them as episodic context, enabling "Mental Time Travel".
