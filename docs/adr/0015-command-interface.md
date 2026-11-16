# ADR 0015: The command's interface: job files in, JSON Lines out, published schemas

Status: accepted (flyingrobots/lumen#218); built by flyingrobots/lumen#219 to #222. Four details
the maintainer's decision left open were settled on 2026-11-16 by the coordinator, to whom the
maintainer delegated decisions: the rule for a new message `type`, the rule for a render's `name` and its file,
the output directory and when messages are written. Nothing in this record is built yet: the
command still prints the key-value report described in the [command reference](../reference/cli.md).

## Context

The maintainer asked for a command that a program drives as easily as a person does: a JSON
configuration file in and an output path; images at the resolutions the configuration gives; a
published, versioned JSON Schema for the configuration; every line on standard output as JSON
Lines under a versioned schema; and errors on standard error, also JSON Lines under a versioned
schema.

Today's command is the opposite on each point. `lumen render` takes one scene, one size and one
output file as options, prints a `key value` report whose only contract is that keys are added
and never renamed, writes seconds as decimals, prints `--help` as prose and reports failures as
one line of text, followed by the usage text for a refused argument. A program that drives it
needs a parser for the report, another for the statistics, another for the help text and a
pattern match on error text. Lumen has not been released, so the interface can change now at no
cost to a user. Each schema becomes a versioned contract once flyingrobots/lumen#219 merges.

## Decision

### Input: a job file and an output directory

`lumen render --config JOB.json --out DIR` reads a job and writes the image of each of its
renders into `DIR`, as a file named by the image's hash.

- The job's format is `lumen-render-job/1`. It lists renders in order. Each render has a unique
  `name`, a built-in `scene`, a `width`, a `height`, `samples` and a `seed`, under the limits the
  command already enforces. A job names the JSON path of what it gets wrong and refuses unknown
  keys. The job holds no thread count: `--threads` and `--band-rows` stay options, since they
  describe the machine and not the image.
- `--out` is a directory when `--config` is given, because one path cannot hold several images.
  Lumen names each file; the job does not.
  `DIR` is checked before the first render starts, as `--out FILE` is checked today; a file's name
  is known only once its image is, so a failure to write one is `output_failed`.
- A render's `name` is a label, not a file name: any non-empty Unicode string of at most 256 bytes
  of UTF-8, unique within the job and compared byte for byte. Lumen never uses it to build a
  path. It is echoed in every message about that render, so a caller can key its own records on
  the label. A name that is empty, too long or a duplicate is a `job_invalid` before anything
  renders.
- Lumen names each output file by the render's size and image hash:
  `DIR/<width>x<height>-<64 lowercase hex digits of the image's SHA-256>.png`, for example
  `1280x640-f3f1....png`. The `result` message carries the `name`, the `file` (that file name,
  relative to `DIR`), the size and the image hash. Two renders share a file only when both size and
  hash are equal; then the file is written once and both results name it. A file already present
  under that name is the same image by construction and is replaced by identical bytes.
  - Why the size: [ADR 0009](0009-image-hash.md) defines the image hash over the raw RGBA8 bytes
    without the width or height, and says that a hash identifies an image only together with its
    size. A uniform 4 by 4 image and a uniform 8 by 2 image have the same hash and need different
    PNG files, so a name made of the hash alone would let one overwrite the other.
  - Consequences: no name can escape `DIR`, because no name reaches the file system; file names
    are safe on every file system; a caller cannot choose a file name (it renames the file or
    records the mapping itself); a buffer reshaped to another size gets another file.
  - Rejected: (a) the name as the file name with a restricted character set, which refuses natural
    labels and ties a caller's key to file-system rules; (b) the name as the file name with any
    character, where path separators, `..`, reserved names and per-platform differences would
    have to be policed; (c) numbering files by position in the job, since a file's name would
    change when the job is reordered and would say nothing about its content; (d) a new artifact
    hash binding size, pixel format and bytes, which is a second identity to define and keep beside
    the image hash when the size is already a provenance field.
- `DIR` is created, with its parents, before the first render starts, if it is missing. If it
  exists and is not a directory, the command refuses with `output_not_directory` (status 2)
  before anything renders. A file in `DIR` is published by the same atomic publish as
  a single `--out` file. Requiring the directory to exist was rejected: it adds a step for every
  caller and gains no safety, since the caller chose the path.
- The existing options stay. `lumen render --scene NAME --out FILE.png` and its siblings keep
  working for the built-in scenes, so the one-line quickstart survives. `--config` is refused
  together with `--scene`, `--width`, `--height`, `--samples`, `--seed` or a `--out` that names a
  file.
- Renders run in order. A job that fails at its second render leaves the first PNG complete and
  no partial file.
- Messages are written as the work proceeds, each flushed when written: one line when a render
  starts (its `provenance`) and the lines of its result when it finishes, then, for a job, one
  closing `job_summary`. A long job shows progress, and a failure leaves a truthful prefix.
  Writing everything at the end was rejected: no progress, and the results of the finished renders
  are lost if a later one fails. The order of the lines of one job is deterministic, since renders
  run in the job's order; the members that hold times stay information only.

### Output: JSON Lines under `lumen-message/1`

Standard output holds one JSON object per line and nothing else, for every command and every
outcome that exits 0. Each object carries `"schema": "lumen-message/1"` and a `type`. This
includes help, version, statistics and schema output: the maintainer asked for 100 %, and an agent
that runs `lumen --help` should need no second parser. A person can still read the usage text,
which is a string inside the `help` message.

- **Numbers are integers.** Times are integers in microseconds, rates are not printed, and no
  float is formatted anywhere, so the output is the same on every platform. A mean that
  `--stats` prints today as a decimal is reported as its two integers, a total and a count, the
  rule of [ADR 0008](0008-statistics.md).
- **Time is information only.** A field that holds a time, or a count that depends on timing (the
  split of work between workers), keeps the status ADR 0008 gave it. The schema marks such a field
  in its description, and no test compares its value between runs.

### Output: JSON Lines under `lumen-error/1`

Standard error holds one JSON object per line under `"schema": "lumen-error/1"`, one for each
failure, and the command exits with the status it has today: 2 for a refused command line, 1 for
anything that fails once the arguments are accepted, and 0 never with an error record. Because messages
are streamed and cannot be retracted, one rule covers partial success:

- A failure before any render starts (a refused command line, an invalid job, an output directory
  that cannot be used) writes nothing to standard output and one error record to standard error.
- A failure during a job leaves every message already written on standard output standing: the
  earlier renders' `provenance`, `result` and `timings` are true and their files are complete. It
  writes no `result` for the failed render and no `job_summary`, and writes exactly one error
  record to standard error that names the failed render by its `name` and its index in the job.
  The exit status is 1.

The usage text leaves error output and lives only in the `help` message.

An error record has the members `schema`, `code`, `message`, `status` and, when they apply, `path`
and `rule` (for a bad job), `name` and `index` (the failed render, for a failure during a job: its
label and its zero-based position in the job's `renders`), and `lossy`. `code` is a stable identifier: it never changes within a
version, and the schema enumerates every code. `message` is the human text and is not a contract.

### Published schemas

The three schemas are JSON Schema documents, committed under `schemas/` and embedded in the
binary, so that `lumen schema NAME` prints the exact schema of the binary in hand. "Published"
then holds offline. Each schema's `$id` is chosen so that hosting the files at a public URL can
follow without a version change; hosting needs a domain and a release process and is not decided
here.

| Name | File | Describes |
| --- | --- | --- |
| `lumen-render-job/1` | `schemas/lumen-render-job-1.schema.json` | the job file (built by #221) |
| `lumen-message/1` | `schemas/lumen-message-1.schema.json` | every line on standard output (built by #219) |
| `lumen-error/1` | `schemas/lumen-error-1.schema.json` | every line on standard error (built by #220) |

### The version rule

A schema's version is the number after the slash, and within a version:

- A field may be added.
- A field may not be removed, renamed or retyped, and the set of values of an enumerated field
  (such as an error `code`) may not lose a value. Any of those makes a new version, such as
  `lumen-message/2`.

Adding a message `type` is an addition like a new member: it is allowed within a version. A
consumer therefore ignores a member it does not know, and ignores a whole line whose `type` it
does not know. A new schema version for each new type was rejected: it would churn versions for
every feature, and consumers gain nothing from it. This is the rule the report had, "lines
are only ever added", stated for JSON: the report's promise was about whole lines of text, and
this promise is about the members of an object. The schemas themselves state it: an object in
`lumen-message/1` or `lumen-error/1` allows additional members. The job schema is the exception,
since it is input and refuses unknown keys, so a misspelt key is an error and not a silent
default.

### The key-value report is retired

The report and `lumen stats`'s key lines are replaced by the messages, not kept beside them.
Lumen has not been released, so nobody depends on the text, and two formats would both need tests
forever. The command reference, the tutorial and the README are rewritten with flyingrobots/lumen#219,
and the tests that read the report (`crates/lumen/tests/cli.rs`, `tutorial.rs`) move to the
messages.

### Spelling that is this record's choice

The maintainer's decision fixed the schema names, the members `schema` and `type`, the integer
rule and the microsecond unit. The decision's record leaves the spelling of the other names open.
Those below are this ADR's choice of spelling, not a new decision of substance, and the task that
builds each may correct a name it finds clumsy before the schema merges: the type names and their
members, the error codes, the members of an error record (`code`, `message`, `status`, `path`,
`rule`, `lossy`), the time members' `_us` suffix, and the job's `renders` list and its
`"schema": "lumen-render-job/1"` member. `path` is a [JSON Pointer](https://www.rfc-editor.org/rfc/rfc6901)
into the job, such as `/renders/1/samples`. `lossy` is `true` when `message` shows an argument
that was not valid Unicode in its lossy form, so that the record is itself valid JSON.

### Every message type

One example line each. The examples are illustrative: they come from the design, not from a
program, because the program that prints them is flyingrobots/lumen#219. The hash, the content
identity and the counts are those the [command reference](../reference/cli.md) shows for the
defaults (`lumen render --out first.png`); times, thread counts and the statistics other than
the triangle, node and byte counts are made up, and the made-up ray counts are consistent with the
report's totals. A render in a job also carries its
`name` in `provenance`, `result` and `timings`, and its `result` also carries `file`; the first
examples show a single render, and one `result` of a job follows.

`provenance` is the render's `RenderProvenance`, thirteen fields, replacing the first thirteen
report lines (built by #219):

```json
{"schema":"lumen-message/1","type":"provenance","scene":"lumen-reference-scene/3","integrator":"lumen-diffuse-path/1","shading":"bunny-diffuse-shading/1","sampler":"bunny-sobol-owen/1","tone":"bunny-tone-srgb/1","encoding":"bunny-rgba8-opaque/1","bunny":"2cd551e248d55407a8f27e84dcb26d1b1d8768bd","width":320,"height":180,"pixels":57600,"samples":4,"seed":0,"bounces":1,"scene_content":"sha256:c1e109477c09328421d76efefd923bd625d842fe3afb93dfadb21ae905bd0f66"}
```

`result` is what the render produced: the size, the run's schedule, the rays, the image hash and the file
(`png` is `null` without `--out`; `rays` are the `rays` line's three counts):

```json
{"schema":"lumen-message/1","type":"result","width":320,"height":180,"threads":8,"threads_asked":8,"band_rows":1,"rays":{"total":616892,"closest":406903,"shadow":209989},"image":"sha256:77ab18fafd84f6598fa638281e205f2298df6ba6da3471546a3724718d3cef23","png":{"path":"first.png","bytes":230699}}
```

In a job, `png.path` is `DIR` joined to `file`, and `file` is the size, a hyphen, the hash's 64 digits and `.png`:

```json
{"schema":"lumen-message/1","type":"result","name":"first view","file":"320x180-77ab18fafd84f6598fa638281e205f2298df6ba6da3471546a3724718d3cef23.png","width":320,"height":180,"threads":8,"threads_asked":8,"band_rows":1,"rays":{"total":616892,"closest":406903,"shadow":209989},"image":"sha256:77ab18fafd84f6598fa638281e205f2298df6ba6da3471546a3724718d3cef23","png":{"path":"out/320x180-77ab18fafd84f6598fa638281e205f2298df6ba6da3471546a3724718d3cef23.png","bytes":230699}}
```

`timings` is every wall-clock time, in microseconds, information only (rates are not printed: a
program divides):

```json
{"schema":"lumen-message/1","type":"timings","scene_us":1200,"render_us":152000,"hash_us":900,"png_us":1500,"total_us":156000}
```

`scene_statistics` is what `lumen stats` and `render --stats` print about the scene (ADR 0008),
abridged here to the members the reference page's example shows; the message holds every scene
key, as members and arrays instead of dotted keys:

```json
{"schema":"lumen-message/1","type":"scene_statistics","triangles":31511,"material_triangles":[0,0,0,27648,3851],"bounds_min":["-16.000000","-1.125000","-16.000000"],"bounds_max":["16.000000","2.000000","16.000000"],"tree":{"nodes":33095,"leaves":16548,"leaf_depth_max":20,"leaf_sizes":[2778,12708,931,131,0]},"memory":{"total_bytes":11432728}}
```

The fixed-point values in `bounds_min` and `bounds_max` stay decimal strings with six places from
their exact raw integers, as ADR 0008 prints them, because a JSON number would be a float.

`ray_statistics` is `render --stats`'s ray block (a function of the scene and settings), with the
means as totals and counts. The values below are consistent but made up:

```json
{"schema":"lumen-message/1","type":"ray_statistics","camera":230400,"camera_hits":120000,"camera_misses":110400,"bounce":176503,"bounce_hits":100000,"bounce_misses":76503,"bounce_rejected":20000,"shadow":209989,"shadow_occluded":90000,"shadow_unoccluded":119989,"total":616892,"pixels":57600,"samples_per_pixel":4,"samples":230400,"per_pixel_min":4,"per_pixel_max":12}
```

`traversal_statistics` is the traversal block, one object per kind of ray (`camera`, `bounce`,
`shadow`), each member the sum or maximum over that kind's rays (counts made up):

```json
{"schema":"lumen-message/1","type":"traversal_statistics","camera":{"box_tests":9100000,"box_entries":4200000,"popped_skipped":800000,"interior_visits":4500000,"leaf_visits":1300000,"primitive_tests":2100000,"box_tests_max":120,"peak_stack":31},"bounce":{"box_tests":4000000},"shadow":{"box_tests":6000000}}
```

`run_statistics` is the work spread over the workers. Which worker took which band depends on
timing, so the schema marks `workers` as information only; the totals are exact:

```json
{"schema":"lumen-message/1","type":"run_statistics","band_rows":1,"bands":180,"workers":[{"bands":23,"rows":23,"pixels":7360,"rays":78912},{"bands":22,"rows":22,"pixels":7040,"rays":75140}]}
```

`help` carries the usage text that error output no longer prints, and `version` what
`lumen --version` prints (`lumen 0.1.0 (bunny <commit>)`):

```json
{"schema":"lumen-message/1","type":"help","usage":"usage: lumen render [--scene NAME] [--out FILE] ... (the whole usage text)"}
```

```json
{"schema":"lumen-message/1","type":"version","lumen":"0.1.0","bunny":"2cd551e248d55407a8f27e84dcb26d1b1d8768bd"}
```

`schema_list` and `schema` are the output of `lumen schema` (built by #222). The `schema` message's
`document` is the named schema, equal as a JSON value to its committed file:

```json
{"schema":"lumen-message/1","type":"schema_list","names":["lumen-error/1","lumen-message/1","lumen-render-job/1"]}
```

```json
{"schema":"lumen-message/1","type":"schema","name":"lumen-error/1","document":{"$schema":"https://json-schema.org/draft/2020-12/schema","title":"lumen-error/1"}}
```

The `schema` member of every message is the version tag. The `schema` message's `name` member
holds the schema that `document` describes, which is why the two differ.

`job_summary` closes a job (`--config`) and is not printed by a single render:

```json
{"schema":"lumen-message/1","type":"job_summary","renders":3,"total_us":412000}
```

`render` prints `provenance` when the render starts, then `result` and `timings` when it finishes;
with `--stats` the four statistics messages follow in the order `scene_statistics`,
`ray_statistics`, `traversal_statistics`, `run_statistics`, as the blocks follow the report
today. A job prints those lines once for each render, in the job's order, each flushed when
written, then `job_summary`. `lumen stats` prints one `scene_statistics`
message and nothing else.

### Every error code

The codes come from the failures the command reports today (`Failure` and `UsageError` in
`crates/lumen/src/bin/lumen/`, and the "Exit status and errors" section of the command
reference) and from the job and schema failures #221 and #222 add. The messages shown are
today's texts where one exists. Each record is one line on the wire; the examples are
illustrative.

Status 2, the command line was refused:

| Code | Raised when | Example |
| --- | --- | --- |
| `missing_command` | no command was given | `{"schema":"lumen-error/1","code":"missing_command","message":"no command given","status":2}` |
| `unknown_command` | the first argument is not a command | `{"schema":"lumen-error/1","code":"unknown_command","message":"unknown command \"frobnicate\"","status":2}` |
| `unknown_option` | an option the command does not have, including `--tile` | `{"schema":"lumen-error/1","code":"unknown_option","message":"unknown option \"--tile\"","status":2}` |
| `repeated_option` | an option given twice | `{"schema":"lumen-error/1","code":"repeated_option","message":"--width given more than once","status":2}` |
| `missing_value` | an option at the end with no value | `{"schema":"lumen-error/1","code":"missing_value","message":"--out needs a value","status":2}` |
| `option_like_value` | the next argument looks like an option | `{"schema":"lumen-error/1","code":"option_like_value","message":"--out needs a value, not the option \"--width\" (to use it as the value, write --out=--width)","status":2}` |
| `invalid_number` | a number option whose value is not decimal digits, or is not Unicode | `{"schema":"lumen-error/1","code":"invalid_number","message":"--width needs a whole number, not \"wide\"","status":2}` |
| `invalid_size` | a width or height outside `1..=32768`, or more than `2^27` pixels | `{"schema":"lumen-error/1","code":"invalid_size","message":"invalid image size: image width and height must be at least 1","status":2}` |
| `invalid_samples` | samples not a power of two from 1 to 1024 | `{"schema":"lumen-error/1","code":"invalid_samples","message":"--samples: ...","status":2}` |
| `invalid_threads` | threads outside `1..=256` | `{"schema":"lumen-error/1","code":"invalid_threads","message":"--threads: threads must be at least 1","status":2}` |
| `invalid_band_rows` | band rows of 0 | `{"schema":"lumen-error/1","code":"invalid_band_rows","message":"--band-rows: a band must be at least 1 row","status":2}` |
| `unknown_scene` | a scene name the command does not have | `{"schema":"lumen-error/1","code":"unknown_scene","message":"--scene: ...","status":2}` |
| `config_conflict` | `--config` given with `--scene`, `--width`, `--height`, `--samples`, `--seed` or a file `--out` | `{"schema":"lumen-error/1","code":"config_conflict","message":"--config cannot be used with --scene","status":2}` |
| `output_not_directory` | `--out DIR` with `--config` names something that exists and is not a directory | `{"schema":"lumen-error/1","code":"output_not_directory","message":"out: not a directory","status":2}` |
| `unknown_schema` | `lumen schema NAME` with a name the binary does not embed | `{"schema":"lumen-error/1","code":"unknown_schema","message":"no schema named \"lumen-x/1\"","status":2}` |

The `...` stands for the library's text, which the command keeps (for example `--samples:` and
the library's reason). An argument that is not valid Unicode is shown in its lossy form with
`"lossy":true`.

Status 1, the arguments were accepted and the work failed:

| Code | Raised when | Example |
| --- | --- | --- |
| `job_unreadable` | the job file cannot be read, or is not JSON | `{"schema":"lumen-error/1","code":"job_unreadable","message":"job.json: No such file or directory (os error 2)","status":1}` |
| `job_invalid` | the job is JSON but breaks a rule of `lumen-render-job/1`: a missing, unknown or mistyped key, a value out of range, an empty, over-long or duplicate `name` | `{"schema":"lumen-error/1","code":"job_invalid","message":"samples must be a power of two from 1 to 1024","status":1,"path":"/renders/1/samples","rule":"samples"}` |
| `scene_failed` | the scene could not be built | `{"schema":"lumen-error/1","code":"scene_failed","message":"the reference scene could not be built: ...","status":1}` |
| `statistics_failed` | the scene's statistics could not be read | `{"schema":"lumen-error/1","code":"statistics_failed","message":"statistics: the scene's tree is malformed at compact node 3","status":1}` |
| `render_failed` | the render failed | `{"schema":"lumen-error/1","code":"render_failed","message":"...","status":1,"name":"first view","index":1}` |
| `ray_check_failed` | the ray statistics contradict the size and settings, which no library render does | `{"schema":"lumen-error/1","code":"ray_check_failed","message":"ray statistics: 3 camera rays counted, expected one per sample, 4","status":1}` |
| `output_refused` | a destination cannot be written, found before the first render | `{"schema":"lumen-error/1","code":"output_refused","message":"/no/such/dir/x.png: No such file or directory (os error 2)","status":1}` |
| `output_failed` | writing a PNG failed after rendering; the destination is as it was | `{"schema":"lumen-error/1","code":"output_failed","message":"out/x.png: disk full","status":1}` |
| `output_sync_failed` | the PNG was written and renamed, but its directory could not be synced | `{"schema":"lumen-error/1","code":"output_sync_failed","message":"x.png: written, but its directory could not be synced: ...","status":1}` |
| `stdout_failed` | standard output could not be written | `{"schema":"lumen-error/1","code":"stdout_failed","message":"stdout: closed","status":1}` |

`job_invalid` is the one code that carries `path` and `rule`. `job_unreadable` has no path into a
job that could not be read; the file's path is in `message`. A job that fails at its second
render reports the failure and exits 1.

## Consequences

- An agent drives the command with one parser, discovers the job format and every message with
  `lumen schema`, and branches on `code`, never on text.
- Lumen's output no longer reads well in a terminal. A person who wants the old view pipes it
  through `jq`; the documentation shows how. This is the price of "100 %".
- The command's own tests gain a schema validator that shares no code with the writer
  (#219, #220), and a table of every code with a test for each (#220).
- Every later change to a message is a decision: adding a member is free, anything else is a new
  version and a new schema file.
- The tutorial, the command reference and the README are rewritten with #219, and the tutorial's
  shown output stays checked against the real output.

## Alternatives rejected

| Alternative | Why not |
| --- | --- |
| `--out` as a file, with a pattern such as `{name}-{width}.png` | A small template language to specify, escape and test, for no gain over a directory. |
| A `--format json` switch beside the text report | Not what the maintainer asked for, and it doubles the output contract. |
| Human-readable help on standard output | Breaks "100 %". A person can still read the usage text inside the message. |
| Seconds as decimals, as today | Float formatting in a machine contract; integers are exact. |
| Hosting the schemas at a public URL now | Needs a domain and a release process. Each schema's `$id` is chosen so this can follow. |
| Keeping the key-value report beside the JSON | Two formats would both need tests forever, and Lumen has not been released. |
