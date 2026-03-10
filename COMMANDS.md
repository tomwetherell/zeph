# Zeph Commands Design

## Background: How Zarr Stores Actually Work

Raw zarr is simpler than the xarray mental model — it only has two primitives:

- **Groups** — containers (like directories)
- **Arrays** — N-dimensional data (like files)

That's it. Zarr itself has **no native concept** of "dimensions", "coordinates", or "data variables". That layering comes from **conventions**, primarily xarray's.

### How xarray encodes its data model into zarr

When xarray writes a Dataset to zarr, it:

1. Writes every variable (coords and data vars alike) as a zarr array at the root group level — they are structurally identical
2. Stamps each array with a `_ARRAY_DIMENSIONS` attribute (e.g. `["time", "lat", "lon"]`) — this is xarray's own ad-hoc convention, not part of the zarr spec itself
3. Writes a `coordinates` attribute on the root group (following CF conventions) listing which arrays are coordinate variables vs data variables (e.g. `"time lat lon"`)
4. Zarr v3 adds a native `dimension_names` field on arrays, which formalises part of this (replacing the need for `_ARRAY_DIMENSIONS`)

So the familiar xarray `print(ds)` output (Dimensions, Coordinates, Data variables, Attributes) is **reconstructable from zarr metadata**, but requires understanding these conventions.

### When are groups actually used?

For the typical xarray-written zarr store, groups are rarely used — everything sits in one flat root group. But groups matter in specific domains:

- **OME-Zarr (bioimaging)** — stores multi-resolution image pyramids as groups (e.g. `/0`, `/1`, `/2` for different zoom levels)
- **xarray DataTree** — xarray's newer hierarchical data structure maps each tree node to a zarr group, so multi-model or multi-experiment datasets use nested groups
- **CMIP6 / climate ensembles** — sometimes grouped by model/experiment/variable

Groups are uncommon in the "single xarray Dataset" case (which is the most common use case), but important for multi-dataset or multi-resolution stores.

## What Information Scientists and Engineers Want to See

### Store-level

- Zarr format version (v2 vs v3)
- Total size on disk
- Root group attributes/metadata
- Number of groups and arrays
- Store type (local filesystem, S3, GCS, etc.)

### Group-level

- Group path within hierarchy
- Attributes (user-defined metadata, e.g. `Conventions: "CF-1.8"`)
- Child groups and arrays

### Array (variable) level

- Shape, dtype, dimension names
- Chunk shape and chunk layout (regular vs sharded)
- Compression codec (e.g. blosc, zstd, gzip) and settings
- Fill value
- Total size vs compressed size on disk (compression ratio)
- Attributes (units, long_name, calendar, etc.)
- Number of chunks, how many are initialized vs missing

### Data-level (deeper inspection)

- Actual data values (head/tail/slice)
- Statistics (min, max, mean, NaN count)
- Coordinate values (for dimension arrays like `time`, `lat`, `lon`)

## Command Design

### Philosophy

Follow the "one obvious way" principle — a small, memorable set of commands where each piece of information has one clear place to find it.

Commands take **no arguments**. When a command needs additional input (e.g. which variable to inspect), it prompts the user interactively. This follows the Claude Code interaction model.

### Commands

#### `/summary` — the "at a glance" overview

The landing page for a store. Should answer: "what's in this store?"

For xarray-convention stores (the common case), present in the familiar xarray `print(ds)` style:

```
Store: /data/era5_temperature.zarr  (zarr v2, 4.2 GB)

Dimensions:    time: 8760, lat: 721, lon: 1440
Coordinates:
    time       (time)             datetime64  8760
    lat        (lat)              float64     721
    lon        (lon)              float64     1440
Data variables:
    t2m        (time, lat, lon)   float32     8760 × 721 × 1440
    sst        (time, lat, lon)   float32     8760 × 721 × 1440
Attributes:
    Conventions: CF-1.6
    history:     2024-01-15 ...
```

This is reconstructed by:

1. Reading `_ARRAY_DIMENSIONS` (v2) or `dimension_names` (v3) from each array
2. Reading the root `coordinates` attribute to distinguish coords from data vars
3. Inferring dimensions from the union of all dimension names + their sizes

For grouped stores (OME-Zarr, DataTree, etc.), show the hierarchy with a per-group summary:

```
Store: /data/multiscale.zarr  (zarr v3, 12.1 GB)

Groups:
  /               2 attrs
  /scale0         1 array   (t, z, y, x) uint16
  /scale1         1 array   (t, z, y, x) uint16
  /scale2         1 array   (t, z, y, x) uint16
```

For stores without xarray conventions, fall back to a simpler "here are the arrays and their shapes" view.

Think of it like `ncdump -h` for NetCDF users — the single most-used inspection command.

#### `/tree` — group hierarchy

Show the group/array hierarchy like unix `tree`. Only interesting for grouped stores, but always available. Clean separation from `/summary` — shows structure, not content.

#### `/info` — detailed view of one variable or group

The deep dive on a single array or group. This is where you see codec, compression ratio, fill value, dimension names, all the technical details.

After running `/info`, the user is prompted with an interactive selection list of all variables and groups in the store. They arrow-key to one and hit enter.

Distinct from `/summary` because it's _deep on one thing_ vs _wide on everything_.

#### `/attrs` — attributes/metadata

Separated from `/info` because attributes can be verbose (CF conventions can store dozens of attributes). Scientists frequently want _just_ the attrs.

Prompts with a selection list: root group, plus all variables/groups. Selecting root shows store-level attributes.

#### `/data` — inspect actual values

Actual data values. Defaults to showing first/last few values.

Interactive flow: first prompts to select a variable, then could show the shape and offer an input field pre-filled with the full range (e.g. `[0:8760, 0:721, 0:1440]`) that the user can edit down to a slice. Or default to a sensible preview (first few values) with an option to refine.

Inherently separate from metadata commands because it involves reading (potentially large amounts of) data, not just metadata.

#### `/chunks` — chunk layout and storage details

Chunk layout visualisation, initialized vs missing chunks, storage sizes per chunk. Useful for debugging performance and understanding storage efficiency. Niche enough to warrant its own command rather than cluttering `/info`.

Prompts to select a variable (only arrays, not groups — groups don't have chunks).

### Commands we decided against

- **`/variables` or `/arrays`** — `/summary` already lists them, and `/tree` shows the hierarchy. No need for a third way to see the same information.
- **`/metadata`** — too ambiguous. `/info` and `/attrs` cover this with clearer semantics.
- **Per-field commands (`/shape`, `/dtype`, `/codec`)** — too granular. `/info` handles all of these in one place.

## Interaction Design: No Arguments, Interactive Prompts

### The model

Inspired by Claude Code's UX: commands take no arguments. When a command needs additional input, it prompts the user interactively. This gives us:

- **Discoverability** — you don't need to know syntax, the tool guides you
- **Lower cognitive load** — just type `/info` and get prompted, rather than remembering `/info temperature` or wondering "was it the variable name or the path?"
- **Consistency** — every command works the same way: type the name, answer prompts if needed
- **Natural extension of existing UX** — zeph already has `/` autocomplete for commands, so extending that pattern to sub-selections feels natural

Arguments would be better for power users who want speed (typing `/info temperature` is faster than selecting from a list) and for scripting/repeatability — but zeph is interactive-only, so that's not a concern.

### Command categories

**Immediate commands** (no follow-up needed):

- `/summary` — runs immediately, shows the full store overview
- `/tree` — runs immediately, shows the hierarchy
- `/help` — runs immediately
- `/exit` — runs immediately

**Target-selection commands** (prompt for a variable/group):

- `/info` — select from all variables and groups
- `/attrs` — select from root group + all variables/groups
- `/data` — select from variables only (then optionally refine slice)
- `/chunks` — select from variables only (arrays have chunks, groups don't)

### Selection list UX

When a command needs a target, present an interactive selection list (similar to the existing command autocomplete):

- Full list of variables/groups displayed
- Arrow keys to navigate, Enter to select
- **Type-to-filter** — user can start typing to narrow the list (important for stores with 50+ variables)
- This reuses the same interaction pattern as the `/` command autocomplete, so it feels familiar

### Multi-step flows

Some commands may need more than one piece of input. For example, `/data` could work as:

1. Select a variable from the list
2. See the shape displayed (e.g. `(8760, 721, 1440)`)
3. Get an input field pre-filled with the full range that the user can edit to a slice
4. Or just show a sensible default preview (first/last few values) with an option to dig deeper

Each step is a small, focused interaction rather than one complex command with syntax to remember.

### Impact on command handler design

Currently commands have the signature `fn() -> CommandResult`. This will need to change:

- Commands declare whether they need a target (variable/group selection)
- The REPL handles the interactive selection _before_ calling the handler
- The handler receives the selected target as context
- This keeps interactive logic in the REPL layer rather than scattered across individual command handlers

## Startup Validation and Metadata Caching

### The problem

Currently, no validation happens until the user runs a command. For remote stores, `StoreLocation::parse` just constructs the `object_store` client without hitting the network. A typo'd bucket, missing credentials, or non-zarr directory all surface as cryptic errors only when the user runs `/summary`.

### Decision: eagerly fetch `.zmetadata` on startup

On startup, immediately after parsing the store location, attempt to read and parse `.zmetadata`. This serves three purposes at once:

1. **Validates the store** — confirms it exists, is accessible, and is a valid zarr store
2. **Validates credentials** — surfaces auth errors before the user starts interacting
3. **Populates the metadata cache** — every subsequent command (`/summary`, `/info`, `/attrs`, the variable picker) is instant

### Consolidated metadata requirement

Zeph requires consolidated metadata (`.zmetadata` file) for now. This covers the vast majority of zarr stores in practice — xarray always writes `.zmetadata` when saving to zarr. Stores without consolidated metadata (raw zarr, some zarr v3 stores) are not supported. A clear error message should explain this limitation and suggest running `zarr.consolidate_metadata()` in Python.

This can be extended later to fall back to reading individual `.zgroup` + `.zarray` files, but that's a separate concern.

### Local stores

Local validation is straightforward:
- Path existence is already checked in `StoreLocation::parse`
- Read `.zmetadata` from the filesystem (fast, no spinner needed)
- If `.zmetadata` is missing: error out with a message explaining consolidated metadata is required
- If `.zmetadata` is malformed: error out with a parse error

### Remote stores

Remote stores show a brief status message while connecting:

```
  Connecting to s3://mur-sst/zarr/ ...
```

On success, the welcome box renders with store info already available from the cached metadata.

On failure, display provider-specific error messages. The `object_store` crate gives us distinct error variants for each failure mode:

| `object_store::Error` variant | Meaning | Example message |
|---|---|---|
| `NotFound` | Bucket or path doesn't exist | `Store not found at s3://mur-stt/zarr/. Check the URL is correct.` |
| `Unauthenticated` | Missing or invalid credentials (401) | `Authentication required for s3://bucket/path. Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY, or configure AWS_PROFILE.` |
| `PermissionDenied` | Insufficient permissions (403) | `Permission denied for s3://bucket/path. Your credentials may not have access to this store.` |
| `Generic` | Network error, DNS failure, etc. | `Could not connect to s3://bucket/path: <underlying error>` |

Auth error messages should include the relevant environment variables for each provider:
- **S3**: `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY`, or `AWS_PROFILE`
- **GCS**: `GOOGLE_APPLICATION_CREDENTIALS`, or `gcloud auth application-default login`
- **Azure**: `AZURE_STORAGE_ACCOUNT_NAME` + `AZURE_STORAGE_ACCOUNT_KEY`, or `AZURE_CLIENT_ID`

### Metadata cache on `Ctx`

The parsed `StoreMeta` is stored on `Ctx` and shared across all commands. Since `.zmetadata` is a snapshot of the store's structure, it won't change during a zeph session, so caching is safe.

Commands that need data beyond metadata (actual array values for `/data`, chunk listings for `/chunks`) still make their own requests — only the metadata parse is cached.

## Open Questions

1. **Auto-summary on startup** — should `/summary` run automatically when zeph opens on a store? Could be a nice UX — you open zeph and immediately see what's in it. With metadata already cached from startup validation, this would be essentially free.
2. **Output format for `/data`** — tabular? Should we handle 1D, 2D, and nD differently? Scientists often want a pandas-DataFrame-like view for 2D slices.
3. **xarray convention detection** — how confidently can we detect whether a store follows xarray conventions? What's the fallback experience?
4. **Grouped store navigation** — for stores with groups, should the selection list show flat paths (`/scale0/image`) or navigate hierarchically (select group, then select array within it)?
