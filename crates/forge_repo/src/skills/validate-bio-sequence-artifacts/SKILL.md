---
name: validate-bio-sequence-artifacts
description: >-
  Validate biological sequence deliverables for DNA/RNA/protein engineering
  tasks. Invoke when outputs include FASTA/GenBank/plain-sequence artifacts,
  primers, coding sequences, translated proteins, fusion constructs, or
  sequence ordering/linker constraints. Focuses on artifact path compliance,
  sequence alphabet/format integrity, primer/annealing sanity, translation and
  frame correctness, motif/order constraints, and GC/content window checks.
---

# Validate Bio Sequence Artifacts

Run this workflow before finishing sequence-design tasks.

## 1) Lock output contract first

Extract exact required outputs from the prompt and verify:

- required file path and filename
- required file count and line structure
- required format (FASTA, plain sequence, GenBank, CSV)

Use direct existence and parse checks before deeper biology checks.

## 2) Validate sequence format and alphabet

For each output sequence:

- Normalize case and remove whitespace for validation only.
- Verify allowed alphabet for the molecule type:
  - DNA: `A/T/C/G` (plus explicitly allowed ambiguity codes only if task permits)
  - RNA: `A/U/C/G`
  - Protein: canonical amino-acid alphabet expected by the task
- Ensure FASTA headers and record grouping are exactly as required.
- Reject blank-line or extra-record formatting drift when strict output shape is required.

## 3) Validate primer and binding constraints

When primers are required:

- Check annealing segment length range from prompt.
- Compute primer thermodynamics with the exact required tool/settings from prompt.
- Enforce forward/reverse pair delta-Tm and absolute Tm bounds.
- Verify primer binding sites exist and orientation is correct (including reverse-complement logic).
- Confirm overhang/enzyme-site constraints and assembly compatibility when specified.

## 4) Validate translation and coding integrity

For coding constructs:

- Confirm codon frame is preserved across joins.
- Apply required start/stop-codon rules from prompt.
- Translate nucleotide sequence and compare resulting peptide segments to expected proteins/motifs.
- Ensure no forbidden residues/tokens introduced by frame shifts or junction errors.

## 5) Validate ordered architecture constraints

When tasks specify ordered subcomponents (domains, tags, linkers, binders):

- Verify every required component appears exactly as required.
- Verify strict left-to-right order in translated product when order is constrained.
- Validate linker presence/absence and linker length bounds where required.
- Ensure adjacency constraints hold (e.g., only specific components may separate others).

## 6) Validate composition and manufacturability constraints

Apply prompt-specified hard constraints such as:

- sequence length upper/lower bounds
- rolling GC-content window limits
- forbidden motifs/restriction sites
- uniqueness/non-repetition rules

Treat these as hard gates, not heuristics.

## 7) Completion gate

Before declaring done:

- All required artifacts exist at exact paths.
- Format/alphabet checks pass.
- Primer/thermodynamic checks pass when applicable.
- Translation/order/linker checks pass when applicable.
- Final verifier-aligned run passes on the cleaned workspace.
