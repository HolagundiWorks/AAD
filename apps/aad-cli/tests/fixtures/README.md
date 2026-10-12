# Test fixtures

Every file here is made for the tests, contains no third-party drawing content, and says how it was
made and what it must do. Fixtures are read-only inputs: tests copy them before changing anything.

| File | Made by | Expected behaviour |
|---|---|---|
| `dwg/alloc-bomb-ac18-section-size.dwg` | `examples/apartment.dxf` written to DWG by AAD, then corrupted by `crates/io/tests/mutation_robustness.rs` (seed in that test's history; 15 019 bytes). One damaged field in the AC18 section map makes the `acadrust` DWG reader ask for about 3.4 petabytes (`dwg_reader.rs:2610`, `vec![0u8; total_size]`) | Reading it in-process **aborts the process** (exit status 0xc0000409). Reading it through the isolated worker (`aad_dwg::isolated`) is an ordinary error and the program continues. See BUG-001 in `docs/BUG_BACKLOG.md` |
