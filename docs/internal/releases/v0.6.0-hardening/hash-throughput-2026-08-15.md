---
title: "AC-16 evidence: hash throughput on real data"
type: release-evidence
release: v0.6.0-hardening
criterion: AC-16
date: 2026-08-15
status: measured
---

# AC-16 (hash throughput on real data): the measurement, and what it decides

`AC-16` is the descope trigger for `F-702` (hash verification): if hashing is
unacceptably slow against the real library, the release still ships, but
duplicates run flag-only and set-aside-by-hash becomes post-release work. It had
never been measured. This document is the measurement.

**Recommendation: ship `F-702` as designed. Do not take the flag-only descope.**
The reasoning is below; the short version is that the hashing code is roughly 35
times faster than the drive can feed it, so a descope would remove no waiting at
all. It would only mean never reading the bytes.

## How to reproduce

```text
cargo test --release -p abo-core --test real_library_hash_throughput -- --ignored --nocapture
```

`--release` is not optional. In a debug build BLAKE3 runs at a small fraction of
its real speed, and a debug figure would understate the shipping product badly
enough to argue for the wrong decision.

The tests are `#[ignore]` because they read `E:\Books - Audio`, which does not
exist on the CI runners. They are strictly READ-ONLY against the library (`D-09`):
a walk, a stat, and opening files for reading. The only writes go to a throwaway
temp-dir SQLite database.

## The machine and the medium

| | |
|---|---|
| CPU | Intel Core i9-13900K |
| RAM | 64 GB |
| Library volume | `E:` - WDC WD181KFGX, 18 TB, 7200 RPM SATA HDD |
| Scratch volume | system temp on `C:` - Crucial CT2000P5PSSD8, NVMe SSD |
| Build | `--release` |
| Read buffer | 1 MiB (`READ_BUFFER_BYTES`) |

The library lives on a mechanical disk. That single fact turns out to be the
whole answer.

## What `F-702` would actually hash

`AC-10` forbids a hash-everything path, so the population is the detected
CANDIDATES, not the library. The size-and-name detector has already done the
narrowing for free:

| Population | Groups | Members | Bytes | Share of library |
|---|---|---|---|---|
| Whole library | - | 14,080 files | 298.72 GB | 100% |
| **Exact file candidates (what `F-702` hashes)** | **293** | **630** | **14.96 GB** | **5.0%** |
| Book folder candidates (`AC-54` content tier, on request only) | 7 | 14 | 7.00 GB | 2.3% |
| Subsumed exact groups (true, not counted per `FD-08`) | 113 | - | - | - |

Detection itself, over the whole library: **1.5 seconds**.

The 5% figure is the design working. Hashing everything would be 299 GB; hashing
what the detector narrowed to is 15 GB.

## The measurement

Two full runs over all 293 exact candidate groups, through the shipping code
path (`verify_groups` plus `FsContentSource`), on a cold cache:

| Run | Bytes | Wall time | Throughput | Hashed | Failed |
|---|---|---|---|---|---|
| 1 | 14.96 GB | 196.8s | 78 MB/s | 630 | 0 |
| 2 | 14.96 GB | 368.9s | 42 MB/s | 630 | 0 |

Zero read failures across 1,260 file reads, which is also the first evidence
that `FsContentSource` handles this library's real paths.

The 1.8x spread between two identical runs is itself a finding. The bytes were
identical and the CPU was identical, so neither can explain it; the variable is
the drive. Treat the honest range as **40 to 80 MB/s on this hardware**, not a
single number.

## What is actually slow: the ceiling measurement

A rate alone does not say whether the code is the reason, and that distinction
decides whether `AC-16` is a fact to design around or a defect to fix. So the
same `FsContentSource` was pointed at a 1 GB scratch file on the NVMe SSD, read
twice, the second time warm from the OS cache:

| Pass | Wall time | Throughput |
|---|---|---|
| Cold, from NVMe | 0.40s | 2,588 MB/s |
| **Warm, from cache** | **0.37s** | **2,765 MB/s** |

**The read loop plus BLAKE3 sustain about 2,765 MB/s.** The library run delivers
42 to 78 MB/s.

So of the time a user spends waiting, the hashing code accounts for roughly 2 to
3 percent and the mechanical disk accounts for the rest. `F-702` is not slow.
The drive is, by a factor of about 35 to 66.

Two consequences follow, and they are the reason for the recommendation:

1. **Descoping saves nothing.** Flag-only does not make reading 15 GB off a
   7200 RPM disk faster. It replaces the wait with never knowing whether two
   files are the same book, which is the one guess this product refuses to make
   (`D-09`).
2. **There is no optimisation worth doing.** A faster hash, a bigger buffer, or a
   cheaper loop would compete for the 2 to 3 percent the code owns. Parallel
   reads across files would make a mechanical disk seek more, not less.

## What a user actually waits for

The 3 to 6 minute figure is for verifying every duplicate in the library at
once. That is not the common interaction:

| Action | Bytes | Wait at 42 MB/s | Wait at 78 MB/s |
|---|---|---|---|
| Verify one duplicate group (52 MB average) | 52 MB | ~1.2s | ~0.7s |
| Verify all 293 groups | 14.96 GB | 369s | 197s |
| Hash the whole library (the path `AC-10` forbids) | 298.72 GB | ~2h | ~65min |

Verifying a single group, which is what the duplicates surface will do when
someone asks about one book, lands at about **one second**. The full-library case
is a cancellable background job with progress (`AC-11`), and its results persist
on `duplicate_members` (`AC-15`), so re-opening the surface does not re-hash
anything already verified. A one-time few-minute background job that never
repeats is a different proposition from a few-minute wait on every visit.

## Recommendation

**Ship `F-702` (hash verification) as designed. The `AC-16` descope trigger is
not met.**

The supporting facts, in the order that matters:

1. The hashing code runs at 2,765 MB/s and the drive delivers 42 to 80 MB/s, so
   the code is not the cost and descoping it removes no waiting.
2. `AC-10`'s candidates-only design already reduced the work by 95%: 15 GB
   instead of 299 GB.
3. The interactive case, verifying one group, is about one second.
4. The exhaustive case is a few minutes, once, in a cancellable background job
   whose results persist.

## Limits of this measurement, stated plainly

- **One library, one machine.** Everything here describes a 299 GB library on a
  7200 RPM SATA drive. A library on an SSD would be far faster; a library on a
  USB 2.0 external or a network share would be slower, and nothing here measures
  those.
- **Two runs, not a distribution.** The 42 to 80 MB/s range comes from two
  samples. It is enough to establish that the drive dominates, which is the
  question `AC-16` asks. It is not a benchmark.
- **The `AC-54` content tier was reported, not hashed.** Book folder groups reach
  content comparison through `verify_book_group`, a separate on-request path.
  Their 7.00 GB projects to roughly 90 to 170 seconds at the measured rate.
- **Nothing here measures the surface.** `F-702`'s user-visible half (progress,
  cancel, the verified state) is `AC-11` and `AC-15` and is tested separately.

## Where the code lives

- `crates/abo-core/src/dupes/hash.rs` - `FsContentSource`, the production read
  path, added for this measurement. It is the only `ContentSource` that touches a
  disk; every other implementation is an in-memory test double. Routes through
  `to_extended_length_prefixed` so paths past the legacy 260-character limit open
  rather than reporting as unreadable (`F-101`, `FD-19`).
- `crates/abo-core/tests/real_library_hash_throughput.rs` - the three
  `#[ignore]` measurements: the candidate population, the throughput run, and the
  read-path ceiling.
