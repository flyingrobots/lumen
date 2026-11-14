//! One use of each `clippy.toml` entry, in a module per rule, and float arithmetic on a float
//! whose type is only inferred (`clippy::float_arithmetic`, which `lib.rs` denies as the
//! library's `lib.rs` does). Types are named in a tuple alias, so each is one finding.

// seeded: time-system
mod time_system {
    pub type Types = (std::time::SystemTime, std::time::SystemTimeError);
    pub fn f() -> bool {
        let _ = std::time::SystemTime::now();
        std::time::UNIX_EPOCH.elapsed().is_ok()
    }
}

// seeded: time-instant
mod time_instant {
    pub type Types = std::time::Instant;
    pub fn f(start: Types) -> core::time::Duration {
        let _ = std::time::Instant::now();
        start.elapsed()
    }
}

// seeded: thread-sleep
mod thread_sleep {
    #[allow(deprecated, reason = "seeded uses of deprecated items")]
    pub fn f() {
        std::thread::sleep(core::time::Duration::ZERO);
        std::thread::sleep_ms(0);
        std::thread::park_timeout(core::time::Duration::ZERO);
        std::thread::park_timeout_ms(0);
    }
}

// seeded: host-parallelism
mod host_parallelism {
    pub fn f() -> bool {
        std::thread::available_parallelism().is_ok()
    }
}

// seeded: thread-spawn
mod thread_spawn {
    pub type Types = std::thread::Builder;
    pub fn f() {
        let _ = std::thread::spawn(|| {});
        std::thread::scope(|_| {});
    }
}

// seeded: unordered-container
mod unordered_container {
    pub type Types =
        (std::collections::HashMap<u8, u8>, std::collections::HashSet<u8>, std::hash::RandomState);
}

// seeded: std-env
mod std_env {
    pub type Types = (
        std::env::Args,
        std::env::ArgsOs,
        std::env::JoinPathsError,
        std::env::SplitPaths<'static>,
        std::env::VarError,
        std::env::Vars,
        std::env::VarsOs,
    );
    #[allow(deprecated, reason = "seeded uses of deprecated items")]
    pub fn f() {
        let _ = (std::env::args(), std::env::args_os(), std::env::var("X"), std::env::var_os("X"));
        let _ = (std::env::vars(), std::env::vars_os(), std::env::current_dir());
        let _ = (std::env::current_exe(), std::env::temp_dir(), std::env::home_dir());
        let _ = (std::env::split_paths("a"), std::env::join_paths(["a"]));
        let _ = std::env::set_current_dir("/");
        let _ = (env!("CARGO_PKG_NAME"), option_env!("USER"));
    }
    pub fn g() {
        std::env::set_var("X", "1");
        std::env::remove_var("X");
    }
}

// seeded: std-fs
mod std_fs {
    use std::path::Path;
    pub type Types = (
        std::fs::DirBuilder,
        std::fs::DirEntry,
        std::fs::File,
        std::fs::FileTimes,
        std::fs::FileType,
        std::fs::Metadata,
        std::fs::OpenOptions,
        std::fs::Permissions,
        std::fs::ReadDir,
        std::fs::TryLockError,
    );
    #[allow(deprecated, reason = "seeded uses of deprecated items")]
    pub fn f(p: &Path, permissions: std::fs::Permissions) {
        let _ = (std::fs::read("x"), std::fs::read_to_string("x"), std::fs::read_dir("x"));
        let _ = (std::fs::write("x", b""), std::fs::metadata("x"), std::fs::create_dir_all("x"));
        let _ = (std::fs::remove_dir_all("x"), std::fs::remove_file("x"), std::fs::create_dir("x"));
        let _ = (std::fs::canonicalize("x"), std::fs::copy("a", "b"), std::fs::exists("x"));
        let _ = (std::fs::hard_link("a", "b"), std::fs::read_link("x"), std::fs::remove_dir("x"));
        let _ = (std::fs::rename("a", "b"), std::fs::soft_link("a", "b"));
        let _ = (std::fs::symlink_metadata("x"), std::fs::set_permissions("x", permissions));
        let _ = (p.exists(), p.try_exists(), p.is_file(), p.is_dir(), p.is_symlink());
        let _ = (p.metadata(), p.symlink_metadata(), p.read_dir(), p.read_link(), p.canonicalize());
    }
}

// seeded: std-process
mod std_process {
    pub type Types = (
        std::process::Child,
        std::process::ChildStderr,
        std::process::ChildStdin,
        std::process::ChildStdout,
        std::process::Command,
        std::process::CommandArgs<'static>,
        std::process::CommandEnvs<'static>,
        std::process::ExitCode,
        std::process::ExitStatus,
        std::process::Output,
        std::process::Stdio,
    );
    pub fn termination<T: std::process::Termination>(t: T) -> T {
        t
    }
    pub fn f() -> u32 {
        std::process::id()
    }
    pub fn exit() -> ! {
        std::process::exit(1)
    }
    pub fn abort() -> ! {
        std::process::abort()
    }
}

// seeded: network
mod network {
    use std::net::ToSocketAddrs;
    pub type Types = (
        std::net::Incoming<'static>,
        std::net::TcpListener,
        std::net::TcpStream,
        std::net::UdpSocket,
    );
    pub fn f() {
        let _ = "x:1".to_socket_addrs();
    }
}

// seeded: float-type
mod float_type {
    pub type Types = (f32, f64);
    pub fn f(d: core::time::Duration, x: u32) {
        let _ = (d.as_secs_f32(), d.as_secs_f64(), d.div_duration_f32(d), d.div_duration_f64(d));
        let _ = (d.div_f32(2.0), d.div_f64(2.0), d.mul_f32(2.0), d.mul_f64(2.0));
        let _ =
            (core::time::Duration::from_secs_f32(1.0), core::time::Duration::from_secs_f64(1.0));
        let _ = core::time::Duration::try_from_secs_f32(1.0);
        let _ = core::time::Duration::try_from_secs_f64(f64::from(x));
    }
}

// seeded: float-type
mod bunny_float_bridge {
    use bunny_geom::{FixedAabb3, FixedRay3, FixedSphere3};
    use bunny_linalg::{FixedVec2, FixedVec3};
    use bunny_num::fixed_q32_32::conversions;
    use bunny_num::FixedQ32_32;
    pub type Types = (
        bunny_num::Scalar,
        bunny_linalg::Vec2,
        bunny_linalg::Vec3,
        bunny_geom::Ray3,
        bunny_geom::Aabb3,
        bunny_geom::Sphere3,
        bunny_codec::ObjMesh<'static>,
        bunny_codec::ObjVertex,
        bunny_codec::ObjVertices<'static>,
        bunny_codec::PlyBinaryMesh<'static>,
        bunny_codec::PlyVertex,
    );
    pub fn f(x: FixedQ32_32, r: FixedRay3, b: FixedAabb3, s: FixedSphere3, v: (Types,)) {
        let _ = (x.to_f32(), FixedQ32_32::from_f32(0.5), FixedQ32_32::try_from_f32(0.5));
        let _ =
            (conversions::to_f32(0), conversions::from_f32(0.5), conversions::try_from_f32(0.5));
        let _ = (r.into_float(), b.into_float(), s.into_float());
        let _ =
            (FixedRay3::try_from_float(r.into_float()), FixedAabb3::try_from_float(b.into_float()));
        let _ = FixedSphere3::try_from_float(s.into_float());
        let _ = (FixedVec2::try_from_float(v.0 .1), FixedVec3::try_from_float(v.0 .2));
        let _ = (bunny_codec::parse_binary_ply(b""), bunny_codec::parse_obj_text(""));
        let _ = bunny_num::is_finite(v.0 .0);
    }
}

// seeded: float-type
mod float_arithmetic_on_an_inferred_float {
    pub fn f(d: core::time::Duration) -> u64 {
        let x = d.as_secs_f64();
        (x * x).to_bits()
    }
}
