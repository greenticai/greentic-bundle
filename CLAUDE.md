# CLAUDE.md

Agent-facing repository guidance has moved to [docs/coding-agents.md](docs/coding-agents.md).

Use that document for:

- wizard replay expectations
- `gtc` and downstream toolchain coordination
- local relative app-pack reference handling
- documentation rules for agents

For the current command surface, prefer:

```bash
greentic-bundle --help
greentic-bundle wizard --help
```

For the main local verification flow, use:

```bash
bash ci/local_check.sh
```
