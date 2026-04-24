# Extension test fixtures

`fixture-bundle/` is the minimal bundle extension used by integration tests in
`tests/ext_*.rs`. It declares `execution.kind="wasm"`, so the runtime never
instantiates the WASM binary — the 8-byte stub here is a placeholder magic
header only.

Regenerate with:

    printf '\0asm\x01\0\0\0' > fixture-bundle/extension.wasm

Do not delete or rename — tests reference these paths by string.
