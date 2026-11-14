//! The compiler layer's seeded forms: the 42 of ADR 0006's 50 that Clippy catches, and the forms
//! of flyingrobots/lumen#115 and #156 that it catches. Each module is one form, marked with its rule.

// seeded: time-instant
mod instant_qualified {
    pub fn f() {
        let _ = std::time::Instant::now();
    }
}

// seeded: time-instant
mod instant_use_then_call {
    use std::time::Instant;
    pub fn f() {
        let _ = Instant::now();
    }
}

// seeded: time-instant
mod instant_use_group {
    use std::time::{Duration, Instant};
    pub fn f() -> Duration {
        Instant::now().elapsed()
    }
}

// seeded: time-instant
mod instant_renamed {
    use std::time::Instant as Tick;
    pub fn f() {
        let _ = Tick::now();
    }
}

// seeded: time-instant
mod instant_module_alias {
    use std::time as clock;
    pub fn f() {
        let _ = clock::Instant::now();
    }
}

// seeded: time-instant
mod instant_glob {
    use std::time::*;
    pub fn f() {
        let _ = Instant::now();
    }
}

// seeded: time-instant
mod instant_in_format {
    pub fn f() -> String {
        format!("{:?}", std::time::Instant::now())
    }
}

// seeded: time-instant
mod instant_in_local_macro {
    macro_rules! now {
        () => {
            std::time::Instant::now()
        };
    }
    pub fn f() {
        let _ = now!();
    }
}

// seeded: time-instant
mod instant_type_position {
    pub fn f(_at: std::time::Instant) {}
}

// seeded: time-system
mod system_time_group {
    use std::time::{SystemTime, UNIX_EPOCH};
    pub fn f() {
        let _ = SystemTime::now().duration_since(UNIX_EPOCH);
    }
}

// seeded: time-system
mod system_time_epoch_elapsed {
    use std::time::UNIX_EPOCH;
    pub fn f() {
        let _ = UNIX_EPOCH.elapsed();
    }
}

// seeded: thread-sleep
mod sleep_module_import {
    use std::thread;
    pub fn f() {
        thread::sleep(core::time::Duration::ZERO);
    }
}

// seeded: thread-sleep
mod sleep_renamed {
    use std::thread::sleep as nap;
    pub fn f() {
        nap(core::time::Duration::ZERO);
    }
}

// seeded: host-parallelism
mod cores_qualified {
    pub fn f() {
        let _ = std::thread::available_parallelism();
    }
}

// seeded: host-parallelism
mod cores_renamed {
    use std::thread::available_parallelism as cores;
    pub fn f() {
        let _ = cores();
    }
}

// seeded: unordered-container
mod map_renamed {
    use std::collections::HashMap as Map;
    pub fn f() {
        let _ = Map::<u8, u8>::new();
    }
}

// seeded: unordered-container
mod set_use_group {
    use std::collections::{BTreeMap, HashSet};
    pub fn f() {
        let _ = (BTreeMap::<u8, u8>::new(), HashSet::<u8>::new());
    }
}

// seeded: unordered-container
mod map_qualified {
    pub fn f() {
        let _ = std::collections::HashMap::<u8, u8>::new();
    }
}

// seeded: unordered-container
mod set_in_vec {
    pub fn f() {
        let _ = vec![std::collections::HashSet::<u8>::new()];
    }
}

// seeded: unordered-container
mod map_type_alias {
    pub type Table = std::collections::HashMap<u32, u32>;
}

// seeded: unordered-container
mod map_glob {
    use std::collections::*;
    pub fn f() {
        let _ = HashMap::<u8, u8>::new();
    }
}

// seeded: unordered-container
mod random_state_qualified {
    pub fn f() {
        let _ = std::hash::RandomState::new();
    }
}

// seeded: std-env
mod env_module_import {
    use std::env;
    pub fn f() {
        let _ = env::var("X");
    }
}

// seeded: std-env
mod env_std_group {
    use std::{env, fs};
    pub fn f() {
        let _ = (env::var_os("X"), fs::read("x"));
    }
}

// seeded: std-env
mod env_in_format {
    pub fn f() -> String {
        format!("{:?}", std::env::args().count())
    }
}

// seeded: std-env
mod env_module_rename {
    use std::env as host;
    pub fn f() {
        let _ = host::var("X");
    }
}

// seeded: std-env
mod env_option_env_macro {
    pub fn f() -> Option<&'static str> {
        option_env!("USER")
    }
}

// seeded: std-fs
mod fs_use_type_then_call {
    use std::fs::File;
    pub fn f() {
        let _ = File::open("x");
    }
}

// seeded: std-fs
mod fs_qualified_read {
    pub fn f() {
        let _ = std::fs::read("x");
    }
}

// seeded: std-fs
mod fs_glob_then_call {
    use std::fs::*;
    pub fn f() {
        let _ = read_to_string("x");
    }
}

// seeded: std-process
mod process_command {
    use std::process::Command;
    pub fn f() {
        let _ = Command::new("x");
    }
}

// seeded: std-process
mod process_module_rename_exit {
    use std::process as p;
    pub fn f() {
        p::exit(1)
    }
}

// seeded: network
mod net_tcp_connect {
    use std::net::TcpStream;
    pub fn f() {
        let _ = TcpStream::connect("x:1");
    }
}

// seeded: network
mod net_self_in_group {
    use std::net::{self};
    pub fn f() {
        let _ = net::UdpSocket::bind("x:1");
    }
}

// seeded: float-type
mod float_const_with_type {
    pub const HALF: f64 = 0.5;
}

// seeded: float-type
mod float_behind_type_alias {
    pub type Real = f32;
    pub fn f(x: Real) -> Real {
        x
    }
}

// seeded: float-type
mod float_cast {
    pub fn f(x: u32) -> u64 {
        (x as f64) as u64
    }
}

// seeded: float-type
mod float_sentinel {
    pub fn f() -> bool {
        f64::NAN.is_nan()
    }
}

// seeded: std-fs
mod path_exists {
    pub fn f() -> bool {
        std::path::Path::new("x").exists()
    }
}

// seeded: thread-spawn
mod thread_spawn {
    pub fn f() {
        let _ = std::thread::spawn(|| {});
    }
}

// seeded: thread-spawn
mod thread_scope {
    pub fn f() {
        std::thread::scope(|_| {});
    }
}

// seeded: thread-spawn
mod thread_builder {
    pub fn f() {
        let _ = std::thread::Builder::new();
    }
}

// seeded: std-env
mod extern_crate_alias {
    extern crate std as host;
    pub fn f() {
        let _ = host::env::var("X");
    }
}

// seeded: std-fs
mod raw_first_segment {
    pub fn f() {
        let _ = r#std::fs::read("x");
    }
}

// seeded: std-env
mod raw_middle_segment {
    pub fn f() {
        let _ = std::r#env::var("X");
    }
}

// seeded: unordered-container
mod raw_type_import {
    use std::collections::r#HashMap;
    pub fn f() {
        let _ = HashMap::<u8, u8>::new();
    }
}

// seeded: float-type
mod raw_float_type {
    pub fn f(x: u32) -> r#f64 {
        r#f64::from(x)
    }
}

// seeded: float-type
mod float_arithmetic_inferred {
    pub fn f(d: core::time::Duration) -> u64 {
        let x = d.as_secs_f64();
        (x * x).to_bits()
    }
}

// seeded: float-type
mod c_double_trait_call {
    pub fn f(x: u32) -> bool {
        <core::ffi::c_double as From<u32>>::from(x).is_sign_positive()
    }
}

// seeded: float-type
mod c_double_std_os_raw {
    pub fn f(x: u32) -> bool {
        std::os::raw::c_double::from(x).is_sign_positive()
    }
}

// seeded: float-type
mod c_float_renamed {
    use std::ffi::c_float as Real;
    pub fn f(x: u16) -> bool {
        Real::from(x).is_sign_positive()
    }
}
