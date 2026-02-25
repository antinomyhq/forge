{{#if env.background}}
IMPORTANT: Apply these success patterns only when the current task matches the corresponding template/task family. Do not copy unrelated steps across different tasks.

Use this only for benchmark/eval runs. Keep execution terse, verifier-first, and stop after first valid pass.

## Universal todo list
- [ ] Parse task and extract required artifact path(s)
- [ ] Run a quick env gate (OS/arch, disk, memory, required tools)
- [ ] Execute minimum deterministic path
- [ ] For long commands (>20s), use background pattern: `nohup <cmd> > /tmp/forge_job.log 2>&1 & echo $! >/tmp/forge_job.pid`; poll with `kill -0 $(cat /tmp/forge_job.pid) && tail -n 40 /tmp/forge_job.log`; if no progress after ~6 polls, switch strategy
- [ ] Run official verifier/harness early (within first 3-5 shell commands), then iterate against it
- [ ] Run task preflight: required artifact path exists, output schema/format matches, threshold/metric constraints pass
- [ ] Finalize immediately after first pass

## Per-task todo templates

### adaptive-rejection-sampler
- [ ] run `Rscript --version`; if missing, run `apt-get update && apt-get install -y r-base-core` [ ] implement deterministic `/app/ars.R` with modular helpers (`ars`, `test`) and explicit input/log-concavity checks; if a temporary `/app/test_ars.R` is used, remove it before finalize [ ] run `Rscript /app/ars.R`, verify required sample outputs, and confirm `/app/ars.R` is the final canonical artifact

### bn-fit-modify
- [ ] inspect sample CSV shape/columns and target artifacts first; if dependency issues occur, use an isolated venv for `pandas/pgmpy` [ ] run deterministic DAG-fit + intervention sampling pipeline and generate all required artifacts (`learned_dag.csv`, `intervened_dag.csv`, `final_bn_sample.csv`) [ ] verify output files/schemas and final sample row count/consistency, then remove temporary verifier scripts/venv

### break-filter-js-from-html
- [ ] read `/app/filter.py` + `/app/test_outputs.py`, then run the official test command first (use `python -c "import test_outputs; test_outputs.test_out_html_bypasses_filter()"` if `pytest` is unavailable) [ ] update only `/app/out.html` with one focused bypass candidate per iteration (do not patch tests or add many exploratory scripts) [ ] after each candidate, rerun the same official test command; stop immediately on first pass and avoid prolonged BeautifulSoup fuzz loops

### build-cython-ext
- [ ] clone pinned source and run `python setup.py build_ext --inplace` immediately [ ] fix compatibility/build deps in-place (Cython/numpy alias issues, setuptools, optional extras) and rebuild once [ ] verify via import smoke + targeted pytest (use `PYTHONPATH=.` when needed), then keep only minimal required patches

### build-pmars
- [ ] enable source repos (`deb-src`) and run `apt-get source pmars` after installing build deps [ ] patch Unix/X11 build flags deterministically in `Makefile`/config, then compile once [ ] install resulting `pmars` binary and smoke-test both normal and debugger flows (`echo "quit" | pmars -e ...` expecting debugger behavior)

### build-pov-ray
- [ ] fetch/extract the required POV-Ray 2.2 source tarballs and copy Unix machine config files into `source/` [ ] patch minimal Unix build compatibility, compile, and place binary at `/usr/local/bin/povray` [ ] render the target scene with explicit include path (`+L.../povdoc/include`) and verify output artifact at required location

### caffe-cifar-10
- [ ] run env gate first (disk/RAM/toolchain) and install minimal CPU deps only [ ] follow conservative build path with capped parallelism (`-j2`, avoid `-j$(nproc)`) [ ] run caffe/CIFAR smoke check and stop at first verifier-valid result

### cancel-async-tasks
- [ ] implement canonical `Semaphore + TaskGroup` cancellation flow [ ] run deterministic async suite covering concurrency, cancellation, and KeyboardInterrupt propagation paths [ ] verify graceful shutdown/no leaked tasks and remove temporary test scripts

### chess-best-move
- [ ] derive board state once [ ] validate with engine once [ ] write move artifact

### circuit-fibsqrt
- [ ] generate `gates.txt` deterministically via one canonical generator script [ ] compile simulator and validate sentinel inputs early (`0`, `1`, `208`, `20000`, `65535`) [ ] ensure gate count/format constraints pass, then remove temporary test binaries/scripts

### cobol-modernization
- [ ] inspect binary/input data layout first (`hexdump`/byte checks) before changing logic [ ] apply deterministic modernization transform and compile/run with `cobc` [ ] verify behavior parity against required data outputs, then remove temporary test COBOL files/scripts

### code-from-image
- [ ] extract logic from image once [ ] resolve ambiguity with one hint check [ ] write final output and verify once

### compile-compcert
- [ ] install exact deps [ ] configure target and build with safe jobs [ ] verify /tmp/CompCert/ccomp with smoke compile/run

### configure-git-webserver
- [ ] create bare repo and deterministic `post-receive` hook that exports checked-out content to web root [ ] run static HTTP server in background (e.g., `nohup python3 -m http.server ...`) serving deploy dir [ ] verify end-to-end by clone/commit/push and `curl` exact served file content/status

### constraints-scheduling
- [ ] run `/app/check_slots.py` first to enumerate valid 1-hour slots and choose the earliest constraint-satisfying slot [ ] write `/app/meeting_scheduled.ics` with strict ICS fields (`BEGIN/END:VCALENDAR`, `VERSION`, `PRODID`, UTC `DTSTART/DTEND`, attendees) and verify raw formatting/line endings (e.g., `cat`, `hexdump`) [ ] run `/app/test_meeting.py` plus one independent verifier script, then remove temp scripts (`/app/check_slots.py`, `/app/test_meeting.py`, verifier temp file)

### count-dataset-tokens
- [ ] install/validate dataset tooling (`datasets`, `transformers`, `huggingface_hub`) and inspect target dataset path first [ ] run one deterministic token-count script against the required split/content [ ] write `/app/answer.txt` and verify it is a single integer in required format

### crack-7z-hash
- [ ] install cracking tools (`p7zip`, `7z2john`, `john`) and generate archive hash deterministically [ ] recover password with bounded john workflow and extract candidate secret [ ] verify recovered output by direct archive extraction + byte/`cmp` check and write final solution artifact

### custom-memory-heap-crash
- [ ] build both debug and release binaries first and reproduce crash behavior [ ] patch allocator/heap logic minimally and re-run debug/release binaries [ ] verify with debugger/memory tools (`gdb`/`valgrind`) to confirm no crash/regression

### db-wal-recovery
- [ ] inspect `main.db` + WAL metadata first (e.g., `xxd` WAL header + SQLite schema checks) [ ] recover records via deterministic SQLite/WAL parse flow and write `recovered.json` [ ] run recovery verifier script plus SQLite integrity/count checks, then remove temporary verifier scripts

### distribution-search
- [ ] implement deterministic search method in one canonical solution script [ ] run target query set to produce required distribution artifact(s) [ ] verify with dedicated checker and clean temporary helper scripts

### dna-assembly
- [ ] inspect FASTA fragments/read ordering and junction constraints first; verify `oligotm`/primer tooling availability (`primer3`) [ ] run deterministic assembly + primer/junction generation workflow (single canonical scripts for assembly and primer generation) [ ] verify assembled sequence and junction consistency in required output artifact

### dna-insert
- [ ] inspect FASTA targets and localize insertion/mutation site first; verify primer tooling availability (`primer3`, `oligotm`) [ ] generate candidate edited sequence/primer design deterministically with explicit Tm/constraint checks [ ] verify final artifact format/content (e.g., `primers.fasta`) and clean temporary helper scripts

### extract-elf
- [ ] inspect ELF structure first (`file`/`readelf`/header bytes) and confirm target offsets/layout [ ] run one deterministic extraction script path (e.g., canonical `extract.js`) to produce expected JSON/artifacts [ ] verify output keys/content against expected sections, then clean temporary verifier scripts/files

### extract-moves-from-video
- [ ] inspect video sample and fetch source deterministically first [ ] run frame/OCR extraction pipeline with one canonical script path [ ] verify final move transcript format/content and clean temp media artifacts

### feal-differential-cryptanalysis
- [ ] inspect FEAL implementation and chosen-plaintext pair format first [ ] implement one canonical deterministic attack script (e.g., `/app/attack.py`) to recover key material [ ] verify both correctness and performance against required check vectors, then remove temporary verifier scripts

### feal-linear-cryptanalysis
- [ ] inspect `feal.c`, `decrypt.c`, `pairs.txt`, `ciphertexts.txt` first, then create one deterministic `solve.py` (install `z3-solver` only if missing) [ ] recover 20-bit seed keys and generate `/app/plaintexts.txt` via `decrypt.c` in one deterministic flow [ ] verify both key correctness against known pairs and plaintext/ciphertext consistency, then clean temporary verifier scripts/binaries

### filter-js-from-html
- [ ] parse HTML and isolate script payloads only [ ] normalize/clean JS (entities/attrs/self-closing edge cases) in one canonical `/app/filter.py` path [ ] run exact-output verifier against tricky fixtures

### financial-document-processor
- [ ] inspect input PDFs first and ensure OCR/text tooling availability (`pdftotext`, `tesseract`, python libs) [ ] run deterministic extraction + classification/aggregation pipeline in one canonical path [ ] verify final report/output against expected invoice/non-invoice splits and totals

### fix-code-vulnerability
- [ ] run official tests first to reproduce failing security behavior and locate vulnerable helper paths [ ] apply minimal safe patch only in target code path [ ] re-run full/targeted tests and verify generated security report/output indicates fix

### fix-git
- [ ] inspect git state/history first (`status`, `log`, `reflog`, target commit diffs) [ ] apply minimal corrective git operations (merge/cherry-pick/reset as required) [ ] verify final branch state and build/site generation if required by task

### fix-ocaml-gc
- [ ] build once (`./configure && make`) and locate failing GC/runtime path in `runtime/shared_heap.c` via targeted sweeper/compression symbols [ ] patch minimal runtime GC logic in-place [ ] rerun build/regression checks and verify crash/leak behavior is resolved

### gcode-to-text
- [ ] inspect `text.gcode` traces/comments first (optionally quick plot for path sanity) [ ] decode embedded text and write exact `/app/out.txt` content once [ ] run provided check script and assert exact format/newline

### git-leak-recovery
- [ ] locate leaked history/content across commits/blobs/reflog (e.g., `git fsck --unreachable`, object scans) [ ] rewrite/sanitize history deterministically and restore required secret artifact/output if task demands it [ ] verify leak removal with history-aware scans and clean repo state

### git-multibranch
- [ ] set up git SSH access (`git` user, password auth) and nginx HTTPS serving (`/var/www/main`, `/var/www/dev`) deterministically [ ] create bare repo + robust `post-receive` hook that deploys by branch (`main`→`/var/www/main`, `dev`→`/var/www/dev`) and clears stale files before extract [ ] verify with scripted clone/push of both branches plus `curl -k` checks for `/index.html` and `/dev/index.html` content

### gpt2-codegolf
- [ ] verifier-first: read exact scoring/check harness [ ] iterate shortest candidate with strict correctness first [ ] only then optimize byte count with re-check each change

### headless-terminal
- [ ] run headless workflow script and validate required terminal interactions/dependencies (e.g., `tmux`, `nano`) [ ] verify command execution path including interrupt/editor behaviors [ ] run final verifier script and clean temporary test artifacts

### hf-model-inference
- [ ] install/load model runtime deps and prefetch model/cache assets [ ] start service/inference path and validate endpoint behavior on positive/negative/error inputs [ ] verify required output schema/fields and confidence keys via automated checks

### install-windows-3.11
- [ ] prepare installer/runtime artifacts [ ] perform install path [ ] verify boot/command success

### kv-store-grpc
- [ ] install gRPC deps and generate protobuf stubs from `kv-store.proto` [ ] start KV gRPC server (background if needed) and run client operations against target port [ ] verify CRUD behavior with an explicit verification client, then clean temporary test scripts

### large-scale-text-editing
- [ ] inspect input/expected files and field structure first [ ] apply deterministic batch edit via one canonical macro/script path [ ] verify exact output parity (e.g., `cmp`) and remove temporary macro/test files

### largest-eigenval
- [ ] inspect evaluator + matrix source first [ ] implement fast dominant-eigenvalue path (power/compiled fallback) [ ] run official eval and verify tolerance/format

### llm-inference-batching-scheduler
- [ ] inspect cost model/baseline packer and input bucket constraints first [ ] implement deterministic scheduler in one canonical script path and generate required plans for each bucket [ ] verify request coverage/uniqueness and run final verifier script, then clean temporary tests

### log-summary-date-ranges
- [ ] inspect raw log format/levels and date windows first [ ] generate deterministic summary CSV in required schema/order [ ] verify counts/headers/date-range outputs with checker script and remove temporary verifier files

### mailman
- [ ] inspect mail stack config/state first (`mailman3`/`postfix`/policy) [ ] apply required list/policy/script updates deterministically [ ] run end-to-end post/join/delivery checks and verify expected mailbox artifacts

### make-doom-for-mips
- [ ] install MIPS cross-toolchain and verify target endianness/VM expectations first (`mips-linux-gnu` vs `mipsel-linux-gnu`) [ ] build with `Makefile.mips` and patch missing headers/toolchain issues deterministically (e.g., `my_stdlib.h`) [ ] run binary in `/app/vm.js` and verify frame/output artifacts

### make-mips-interpreter
- [ ] inspect target MIPS binary first (ELF headers/symbols/syscall surface) [ ] install minimal cross-tooling and implement interpreter/patch path deterministically [ ] run VM/sample execution checks and verify required output artifacts

### mcmc-sampling-stan
- [ ] verify R/Stan toolchain first (R, compiler, BLAS/LAPACK; install `libcurl/libssl/libxml2` dev deps if missing) [ ] prepare Stan model/data and run deterministic sampling script (`analysis.R`) [ ] verify required posterior summary artifact(s) (e.g., `posterior_alpha_mean.txt`, `posterior_beta_mean.txt`) plus script-config requirements (chains/iter/seed/model clauses) and diagnostics output

### merge-diff-arc-agi-task
- [ ] inspect/merge source bundles/branches deterministically and resolve conflicts with minimal edits [ ] validate merged implementation against provided examples/tests [ ] verify clean repo state and required outputs, then remove temporary diagnostic scripts

### model-extraction-relu-logits
- [ ] inspect `forward.py` interface/shape behavior first [ ] implement deterministic extraction script in one canonical path (`/app/steal.py`) [ ] run official check/verify script and ensure required output contract

### modernize-scientific-stack
- [ ] run provided modernization script/tests first and inspect legacy dependency/config usage [ ] apply deterministic stack updates in target module(s) only [ ] verify runtime behavior plus compile/import checks (e.g., `python3 -m py_compile`) and dependency constraints

### mteb-leaderboard
- [ ] pin dataset/code snapshot inputs first (results cache + leaderboard code revision) [ ] run deterministic leaderboard table generation path once [ ] verify final output format/content (e.g., `org/model`) and clean temporary cache/repos

### mteb-retrieve
- [ ] inspect corpus file first (`wc/head`) and load exact model+revision (confirm embedding shape/dtype) without patching site-packages [ ] run retrieval with deterministic sort/tie-break (score desc, index asc) in one canonical script [ ] write exact required output line to result file (trimmed, newline-safe) and verify with harness

### multi-source-data-merger
- [ ] inspect all source datasets and dtypes first (JSON/CSV/Parquet) [ ] run deterministic merge by required keys/rules and emit merged artifact + conflicts report when required [ ] verify merged output integrity (row content + schema/dtypes) with checker

### nginx-request-logging
- [ ] install nginx, configure `nginx.conf` (`log_format` + `limit_req_zone`) and `/etc/nginx/conf.d/benchmark-site.conf` once [ ] remove default site, run `nginx -t`, restart, and generate traffic (`curl` + `ab`) [ ] verify `/var/log/nginx/benchmark-access.log` and `/var/log/nginx/benchmark-error.log` plus required config entries (`limit_req_zone`, `limit_req`)

### openssl-selfsigned-cert
- [ ] generate cert/key deterministically with required subject/validity and build combined PEM if required [ ] validate cert details (subject, dates, fingerprint) and key strength/permissions [ ] verify usage in target flow with automated checks

### overfull-hbox
- [ ] inspect `main.tex` + inputs/synonyms first, then run `pdflatex` once to reproduce [ ] apply minimal text/layout replacement only in target source (avoid multi-script rewrite loops) [ ] rerun `pdflatex` and verify `main.log` contains no `Overfull \\hbox`

### password-recovery
- [ ] inspect filesystem/process-recovery vectors first (deleted files, nested blobs/archives) [ ] recover candidate secret deterministically and write required output artifact [ ] verify recovered credential matches strict required format/length before finalize

### path-tracing
- [ ] inspect baseline renderer/output first (quick stat/pixel diff; convert preview with `ffmpeg` if useful) [ ] apply minimal deterministic renderer/math patch and rebuild (avoid broad disassembly/test-file brute-force loops) [ ] run official render/compare harness and keep the smallest passing diff

### path-tracing-reverse
- [ ] inspect binary and baseline output once (`file`, `strings`, initial run) [ ] derive minimal reverse-engineering patch path from targeted disassembly (avoid broad brute-force test loops and many temporary `test*.c` files) [ ] run parity/diff verifier early and stop after first passing reconstruction

### polyglot-c-py
- [ ] create `/app/polyglot` scaffold and write one source satisfying polyglot constraints [ ] compile/run in Python mode (`python3`) and C mode (`gcc`) across required sample inputs [ ] verify both outputs match required text via one test script, then remove temporary probes

### polyglot-rust-c
- [ ] create `/app/polyglot` scaffold and write one source satisfying polyglot constraints [ ] compile/run in Rust mode (`rustc`) and C/C++ mode (`g++ -x c++`) across required sample inputs [ ] verify both outputs match required text via one test script, then remove temporary probes/binaries

### portfolio-optimization
- [ ] inspect baseline + extension skeleton first (`portfolio_baseline.py`/`portfolio_optimized.c`/`benchmark.py`) [ ] implement C extension + thin wrapper with contiguous float64 paths and minimal overhead, then build via `python3 setup.py build_ext --inplace` [ ] run benchmark twice; require correctness parity and stable speedup margin (target >1.2x, prefer >=1.25x before finalize)

### protein-assembly
- [ ] inspect core biological inputs first (`antibody.fasta`, `pdb_ids.txt`, and any plasmid/template files) [ ] run deterministic assembly/search/reverse-translation workflow with minimal deps [ ] verify required output artifact constraints and clean temporary helper scripts

### prove-plus-comm
- [ ] run Coq checker (`coqc`) first to reproduce failing proof state [ ] complete proof steps deterministically in target file only [ ] verify checker acceptance, ensure no `admit`, and clean temporary proof files

### pypi-server
- [ ] build test package artifact (`python -m build`) and install/start pypi-server deterministically [ ] run server (background if needed) and publish/query package via index URL [ ] verify install/import behavior (including reinstall path) and clean temporary test scripts

### pytorch-model-cli
- [ ] implement/patch CLI model commands and ensure build toolchain/runtime deps are present (install minimal extras only if missing) [ ] build CLI binary and run required inference workflow on target inputs [ ] verify output contract/format exactly (e.g., single class index formatting) and clean temporary helper scripts

### pytorch-model-recovery
- [ ] inspect corrupted/incomplete checkpoint structure first (state_dict keys/shapes/metadata) and run load/train sanity probes [ ] recover/repair model artifact deterministically with minimal architectural drift [ ] verify `torch.load` + inference/training sanity, then remove temporary verification scripts

### qemu-alpine-ssh
- [ ] boot Alpine in QEMU with hostfwd (e.g., `2222->22`) [ ] configure/enable sshd + root password inside guest [ ] verify login non-interactively (`sshpass`/scripted ssh)

### qemu-startup
- [ ] verify `/app/alpine.iso` and `qemu-system-x86_64` availability first [ ] boot with required serial/telnet params in background (`qemu-system-x86_64 ... -serial telnet:127.0.0.1:6665,server,nowait ... &`) [ ] verify readiness with one minimal check (`ps` for qemu process + `ss -lnt`/listener on `127.0.0.1:6665`) and stop

### query-optimize
- [ ] profile baseline query [ ] apply optimization [ ] verify latency/plan improvement

### raman-fitting
- [ ] inspect `graphene.dat` shape/range/baseline first (`head`/`tail` + one inspect script) [ ] run deterministic constrained fit via single `fit_peaks.py` path [ ] verify `results.json` with a dedicated results-check script

### regex-chess
- [ ] inspect checker constraints first (`check.py`/task rules + perf/timeout expectations) [ ] generate regex candidate deterministically in one canonical script path (e.g., `/app/generate.py`) and keep iterative edits in that file only [ ] run official checker after each change until exact pass, then run one final perf/edge check and cleanup temp scripts

### regex-log
- [ ] inspect expected log-field constraints and craft one deterministic extraction regex [ ] run against the full log corpus plus targeted edge/invalid cases [ ] verify extracted records match required schema and clean temporary regex test scripts

### reshard-c4-data
- [ ] locate input shard set and validate compressor/decompressor environment first [ ] run deterministic reshaping/reshard pipeline and verify round-trip reconstruction (`diff -rq`) [ ] verify shard constraints/content, then clean temporary test directories/scripts

### rstan-to-pystan
- [ ] inspect original RStan workflow/data contracts first and validate PyStan/httpstan runtime/toolchain availability [ ] translate to PyStan API with deterministic sampling configuration and explicit parameter settings [ ] verify required posterior CSV artifacts/parity constraints and clean temporary Stan probe scripts

### sam-cell-seg
- [ ] validate input image/metadata shapes first and ensure MobileSAM + CPU torch runtime are importable [ ] fetch/confirm weights path (`/app/mobile_sam.pt`) and run one canonical conversion script (`/app/convert_masks.py`) with exact required CLI args [ ] verify output CSV exists, row-count/column schema match metadata, and mask dimensions are valid

### sanitize-git-repo
- [ ] identify sensitive content in working tree and git history first [ ] rewrite/sanitize history and head consistently (not just working tree replacement) [ ] verify no secret patterns remain via repo scan + history-aware checks

### schemelike-metacircular-eval
- [ ] implement/fix evaluator semantics [ ] run language tests [ ] verify expected eval outputs

### sparql-university
- [ ] inspect RDF/SPARQL targets and confirm query runtime availability (e.g., Apache Jena `sparql`) [ ] run required queries deterministically against target TTL/graph files [ ] verify result set format/content and clean temporary runtime downloads

### sqlite-db-truncate
- [ ] inspect DB layout/file state first (`.tables`, schema checks) and identify required truncation/recovery targets [ ] run deterministic truncate/recovery flow on target SQLite DB [ ] verify output/state with provided checker and clean temporary helper scripts

### sqlite-with-gcov
- [ ] extract/build sqlite with gcov/coverage flags enabled and ensure sqlite3 binary path is set correctly [ ] run target workload to generate coverage artifacts (`*.gcda`) [ ] verify coverage output presence/behavior and clean stale coverage files between checks

### torch-pipeline-parallelism
- [ ] verify Python/torch runtime first and inspect model `forward`/block signatures before edits [ ] implement fixed partitions + deterministic microbatch schedule in one canonical script path (`/app/pipeline_parallel.py`) [ ] run single-process then pipeline smoke tests and verify output shape/numerics

### torch-tensor-parallelism
- [ ] verify Python + torch distributed runtime first (`python`/`python3`/`/uv/forge/bin/python`, `torch.distributed.run`); install CPU torch if missing [ ] implement tensor-parallel split/merge with deterministic test harness [ ] run single-proc then multi-proc checks and verify numerics

### train-fasttext
- [ ] prepare training corpus [ ] train fastText model [ ] verify model and sample predictions

### tune-mjcf
- [ ] run `python /app/test_contacts.py` and `python /app/eval.py` first, then inspect contact/solver options from `model_ref.xml` (keep physical body properties unchanged) [ ] apply one minimal XML option change per iteration in `/app/model.xml` (avoid repeated full rewrites unless needed) [ ] run `python /app/eval.py` after each change, keep best config, reset from baseline when regressions appear; once threshold passes, run one confirmation eval and stop

### video-processing
- [ ] inspect media with `ffprobe` and run a quick baseline analyzer check first [ ] implement only canonical `/app/jump_analyzer.py` path (avoid many temporary `test_*.py` loops) [ ] run official verifier on target videos and re-check after each candidate change (do not continue once a passing candidate is found), then clean temporary frame/test artifacts

### vulnerable-secret
- [ ] inspect target binary first (`file`, `strings`, `readelf`, `objdump`) and identify exploit/recovery path [ ] perform deterministic extraction/recovery (e.g., crafted payload if required by binary behavior) [ ] verify recovered secret format/content in required artifact and clean temporary reverse-engineering files

### winning-avg-corewars
- [ ] inspect corewars setup and baseline warriors first (`warriors/*.red`, `pmars -V`) [ ] optimize warrior strategy deterministically from baseline variants and sanity-check against key opponents before full eval [ ] run official evaluation and verify winning average metric

### write-compressor
- [ ] inspect decompressor/format constraints first (compile `decomp.c` if needed) [ ] generate compressed artifact deterministically with one script and bounded parameter search [ ] verify roundtrip (`decomp` + `cmp`) plus strict size/ratio target, then clean temporary compressor artifacts
## Guardrails
- Keep todos compact (2-3) for linear runs
- Keep reasoning to one actionable line per iteration
- Use bounded retries; switch strategy instead of long thrash loops
- On strict schemas, validate required fields before completion
{{/if}}
