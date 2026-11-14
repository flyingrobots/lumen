//! One use of each `clippy.toml` entry, in a module per rule, and float arithmetic on a float
//! whose type is only inferred (`clippy::float_arithmetic`, which `lib.rs` denies as the
//! library's `lib.rs` does).

// seeded: time-system
mod time_system {
    pub fn f() -> bool {
        let now: std::time::SystemTime = std::time::SystemTime::now();
        now.elapsed().is_ok()
    }
}

// seeded: time-instant
mod time_instant {
    pub fn f() -> core::time::Duration {
        let start: std::time::Instant = std::time::Instant::now();
        start.elapsed()
    }
}

// seeded: thread-sleep
mod thread_sleep {
    pub fn f() {
        std::thread::sleep(core::time::Duration::ZERO);
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
    pub fn f() {
        let _ = std::thread::spawn(|| {});
        std::thread::scope(|_| {});
        let _: std::thread::Builder = std::thread::Builder::new();
    }
}

// seeded: unordered-container
mod unordered_container {
    pub fn f() {
        let _: std::collections::HashMap<u8, u8> = std::collections::HashMap::new();
        let _: std::collections::HashSet<u8> = std::collections::HashSet::new();
        let _: std::hash::RandomState = std::hash::RandomState::new();
    }
}

// seeded: std-env
mod std_env {
    pub fn f() {
        let _ = (std::env::args(), std::env::args_os(), std::env::var("X"), std::env::var_os("X"));
        let _ = (std::env::vars(), std::env::vars_os(), std::env::current_dir());
        let _ = (std::env::current_exe(), std::env::temp_dir());
        #[allow(deprecated, reason = "a seeded use of a deprecated item")]
        let _ = std::env::home_dir();
        let _ = (env!("CARGO_PKG_NAME"), option_env!("USER"));
    }
}

// seeded: std-fs
mod std_fs {
    use std::path::Path;
    pub fn f(p: &Path) {
        let _ = (std::fs::read("x"), std::fs::read_to_string("x"), std::fs::read_dir("x"));
        let _ = (std::fs::write("x", b""), std::fs::metadata("x"), std::fs::create_dir_all("x"));
        let _ = (std::fs::remove_dir_all("x"), std::fs::remove_file("x"));
        let _: Option<std::fs::File> = None;
        let _ = std::fs::OpenOptions::new();
        let _ = (p.exists(), p.try_exists(), p.is_file(), p.is_dir(), p.metadata());
        let _ = (p.read_dir(), p.canonicalize());
    }
}

// seeded: std-process
mod std_process {
    pub fn f() -> u32 {
        let _ = std::process::Command::new("x");
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
    pub fn f() {
        let _ = std::net::TcpStream::connect("x:1");
        let _ = std::net::TcpListener::bind("x:1");
        let _ = std::net::UdpSocket::bind("x:1");
    }
}

// seeded: float-type
mod float_type {
    pub fn f(x: bunny_num::FixedQ32_32, y: f32) -> f64 {
        f64::from(x.to_f32() + y)
    }
}

// seeded: float-type
mod float_arithmetic_on_an_inferred_float {
    pub fn f(d: core::time::Duration) -> u64 {
        let x = d.as_secs_f64();
        (x * x).to_bits()
    }
}
