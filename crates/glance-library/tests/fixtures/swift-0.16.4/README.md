# Swift MaCrate 0.16.4 golden fixtures

Written by Swift MaCrate's own persistence code (CrateKit 0.16.4, GRDB 7.10.0, the system SQLite and Foundation of
macOS 27.0.1), through its public and `@testable` API, by the oracle in `tools/swift-library-oracle`:

```sh
tools/swift-library-oracle/run.sh goldens <MaCrate 0.16.4 source> <work dir> <out dir>
```

All data is synthetic (paths under `/Volumes/Fixture` and `/private/var/fixture`, ids made of repeated hex digits).
Licence: CC0, as the rest of this repository's own fixtures.

| Path | What |
|---|---|
| `empty/` | `MetadataStore(location: .file(…))` opened once: `library.sqlite`, `measurements.sqlite`, and their dumps |
| `populated/` | `Fx.build` (every nullable column NULL and set, NFC and NFD twin paths, `#%&?'"` in a path, two paths sharing a clip id, an orphan and a dismissed orphan, 0 / 1 / many tags, presets, stashes, Alpha Type and probe, a native and two lab measurements, a `scan_skips` row, a rename), the dumps, `checks.json`, and Swift's answers: `reads.json` (every read), `queries.json` (77 search / filter / sort cases: plan SQL and arguments, paths, ids), `search.json` |
| `legacy-v7/` | a v7-era library made with `sqlite3` from `tools/swift-library-oracle/legacy-v7.sql` (the first seven migrations' SQL, duplicate-path rows) |
| `legacy-v7-upgraded/` | the same file opened by Swift (v8–v12 run, FTS backfilled), its dump, and `upgrade.json` (the re-index flag, the two backups equal to the original) |
| `json/` | sidecar and stash files as Swift encodes them, and what Swift's decoders make of a list of inputs (`*-decode.json`); filter JSON as a plain `JSONEncoder` writes it (key order varies run to run) |
| `rules.json` | sidecar names, FTS filename tokens, the search sanitiser, trimming, `Double.description`, the Alpha probe codec, the file-system representation of names, and the seeded shuffle and name order of `ClipSorting` |

A dump (`dump.json`) is `sqlite_master`, the file pragmas, `table_xinfo` / `index_list` / `foreign_key_list` per table,
`index_xinfo` per index and every row by primary key, each value as `[storage class, value]` (a REAL as its bit
pattern), the shape `glance_library::dump` writes. Values stamped with "now" (a rename's `updated_at`, dismissals,
preset and Alpha Type stamps) are masked by the tests before comparing.

`src/unicode_tables.rs` comes from the same run (`testWriteUnicodeTables`).
