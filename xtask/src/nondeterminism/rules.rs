//! This module owns the scan's vocabulary: its rule IDs, and the names and paths each bans.

/// Every rule ID: Echo's `ban-nondeterminism` vocabulary, then Lumen's own.
pub(crate) const RULES: [&str; 31] = [
    "time-system",
    "time-instant",
    "thread-sleep",
    "host-parallelism",
    "random",
    "host-callback",
    "network",
    "unordered-container",
    "hand-hashed-table",
    "json-serialization",
    "wasm-serde",
    "float-sentinel",
    "float-op",
    "std-env",
    "std-fs",
    "std-process",
    "thread-spawn",
    "float-type",
    "float-literal",
    "float-cast",
    "hidden-path",
    "thread-identity",
    "address",
    "host-io",
    "std-os",
    "host-cpu",
    "platform-branch",
    "source-path",
    "shared-state",
    "std-hasher",
    "type-identity",
];

/// The std modules that hold a banned item: a path a macro builds from a metavariable root
/// (`$root::fs`) through one of them is refused, since the scan cannot know the root.
pub(super) const STD_MODULES: [&str; 16] = [
    "env",
    "fs",
    "process",
    "net",
    "os",
    "io",
    "thread",
    "time",
    "collections",
    "hash",
    "arch",
    "ffi",
    "panic",
    "any",
    "backtrace",
    "primitive",
];

/// The roots of the standard library.
pub(super) const STD_ROOTS: [&str; 3] = ["std", "core", "alloc"];

/// The float types: `f16` and `f128` are unstable on the pinned toolchain, and named so that
/// a toolchain that stabilises them cannot bring one into the core unseen.
pub(super) const FLOAT_TYPES: [&str; 4] = ["f16", "f32", "f64", "f128"];

/// Names banned wherever they are spelled, and their rules. `c_double` and `c_float` are std's
/// aliases of `f64` and `f32` (`core::ffi`, `std::ffi`, `std::os::raw`): a float type with no
/// float token.
pub(super) const NAMES: [(&str, &str); 14] = [
    ("Instant", "time-instant"),
    ("SystemTime", "time-system"),
    ("UNIX_EPOCH", "time-system"),
    ("available_parallelism", "host-parallelism"),
    ("HashMap", "unordered-container"),
    ("HashSet", "unordered-container"),
    ("RandomState", "unordered-container"),
    ("FxHashMap", "unordered-container"),
    ("FxHashSet", "unordered-container"),
    ("c_double", "float-type"),
    ("c_float", "float-type"),
    ("DefaultHasher", "std-hasher"),
    ("SipHasher", "std-hasher"),
    ("ThreadId", "thread-identity"),
];

/// Banned path prefixes and their rules. Each is compared segment by segment after the path is
/// resolved through the file's imports ([`resolve`](super::paths::resolve)); the `sleep` family is listed by name
/// because `sleep_ms` is not under `sleep`. The Bunny types are its float bridge, whose values
/// carry floats with no float token (`PlyVertex.x`).
pub(super) const PATHS: [(&[&str], &str); 46] = [
    (&["std", "env"], "std-env"),
    (&["std", "fs"], "std-fs"),
    (&["std", "process"], "std-process"),
    (&["std", "net"], "network"),
    (&["std", "thread", "sleep"], "thread-sleep"),
    (&["std", "thread", "sleep_ms"], "thread-sleep"),
    (&["std", "thread", "sleep_until"], "thread-sleep"),
    (&["std", "thread", "park_timeout"], "thread-sleep"),
    (&["std", "thread", "park_timeout_ms"], "thread-sleep"),
    (&["std", "thread", "current"], "thread-identity"),
    (&["std", "thread", "Thread"], "thread-identity"),
    (&["std", "thread", "ThreadId"], "thread-identity"),
    (&["std", "thread", "LocalKey"], "shared-state"),
    (&["std", "io", "stdin"], "host-io"),
    (&["std", "io", "Stdin"], "host-io"),
    (&["std", "io", "StdinLock"], "host-io"),
    (&["std", "io", "stdout"], "host-io"),
    (&["std", "io", "Stdout"], "host-io"),
    (&["std", "io", "StdoutLock"], "host-io"),
    (&["std", "io", "stderr"], "host-io"),
    (&["std", "io", "Stderr"], "host-io"),
    (&["std", "io", "StderrLock"], "host-io"),
    (&["std", "io", "IsTerminal"], "host-io"),
    (&["std", "io", "pipe"], "host-io"),
    (&["std", "os"], "std-os"),
    (&["std", "arch"], "host-cpu"),
    (&["std", "panic", "Location"], "source-path"),
    (&["std", "hash", "DefaultHasher"], "std-hasher"),
    (&["std", "collections", "hash_map"], "std-hasher"),
    (&["std", "any", "TypeId"], "type-identity"),
    (&["std", "any", "type_name"], "type-identity"),
    (&["std", "any", "type_name_of_val"], "type-identity"),
    (&["std", "backtrace"], "address"),
    (&["std", "fmt", "Pointer"], "address"),
    (&["std", "ptr", "hash"], "address"),
    (&["bunny_num", "Scalar"], "float-type"),
    (&["bunny_linalg", "Vec2"], "float-type"),
    (&["bunny_linalg", "Vec3"], "float-type"),
    (&["bunny_geom", "Ray3"], "float-type"),
    (&["bunny_geom", "Aabb3"], "float-type"),
    (&["bunny_geom", "Sphere3"], "float-type"),
    (&["bunny_codec", "ObjMesh"], "float-type"),
    (&["bunny_codec", "ObjVertex"], "float-type"),
    (&["bunny_codec", "ObjVertices"], "float-type"),
    (&["bunny_codec", "PlyBinaryMesh"], "float-type"),
    (&["bunny_codec", "PlyVertex"], "float-type"),
];
