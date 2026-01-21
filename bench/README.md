### Compilation
Compile forge for `x86_64-unknown-linux-musl`, the MUSL binary will be
used for benchmarks.

```shell
cross build --release --target x86_64-unknown-linux-musl
```

Note: Only x86_64 linux MUSL build will work for all benchmarks,
some benches uses linux image which does not have required GLIBC version. 

### Set binary path in env

And set the binary path in `$FORGE_BIN` env var.

Example: `export FORGE_BIN=/home/ssdd/RustroverProjects/forge/target/x86_64-unknown-linux-musl/release/forge`

### Install harbor

Refer: https://harborframework.com/docs/getting-started

### Run tbench

```shell
harbor run -d terminal-bench@2.0 --agent-import-path bench.forge_agent:ForgeAgent --export-traces --export-verifier-metadata --force-build --debug -n 32
```

Note: `-n 32` will run 32 tests in parallel (upto which, M* Pro chips works fine)

### Some info about forge_agent.py

In [forge_agent.py](forge_agent.py), since we have no way to provide API for Provider,
it copies current files: `~/forge/.credentials.json` and `~/forge/.config.json`
