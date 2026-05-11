# greentic-bundle-reader

`greentic-bundle-reader` is the read-only bundle reader for `greentic-bundle`.

It currently supports:

- opening built `.gtbundle` SquashFS artifacts
- opening normalized unpacked build directories
- validating the basic manifest/lock structure
- exposing a stable typed runtime surface for bundle metadata, app packs, extension providers, catalogs, resolved files, and setup state files

Artifact reads use the Rust-native `backhand` SquashFS reader and do not require `unsquashfs` or `squashfs-tools`.

The crate is kept inside the workspace for now so operator/runtime consumers can adopt a stable bundle-reading API before it is split or published independently.
