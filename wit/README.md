# WIT vendor

These `.wit` files describe the `greentic:extension-bundle@0.1.0` world that
bundle extensions implement. Vendored from `greentic-bundle-extensions/wit/`
which itself vendors from `greentic-biz/greentic-designer-extensions`.

Refresh procedure:

```bash
cp ../greentic-bundle-extensions/wit/*.wit wit/
git diff wit/  # review for breaking changes
```

DO NOT edit these files directly — push schema changes upstream first.
