---
name: check
description: Checks if the code is ready to be committed
---

- Run the `lint` and `test` commands and verify if everything is fine.
  <lint>cargo +nightly fmt --all; cargo +nightly clippy --fix --allow-staged --allow-dirty --workspace</lint>
  <test>cargo insta test --accept --unreferenced=delete</test>
- Fix every issue found in the process
