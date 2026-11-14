//! Deterministic code next to the seeded forms, which the compiler layer must accept: ADR 0006's
//! look-alikes that compile, and the identifiers and modules the gate holes resemble.

// accepted
mod fixed_point_sin_and_sqrt {
    pub struct Fx(pub i64);
    impl Fx {
        pub fn sin(self) -> Self {
            self
        }
        pub fn sqrt(self) -> Self {
            self
        }
    }
    pub fn f() -> Fx {
        Fx(1).sin().sqrt()
    }
}

// accepted
mod names_in_comments_and_strings {
    // a HashMap would iterate in a random order; not a std::time::Instant
    pub fn f() -> (&'static str, &'static str) {
        ("std::collections::HashMap", r#"f64::NAN 0.5"#)
    }
}

// accepted
mod identifiers_like_banned_names {
    pub fn f(instant: u32, system_time: u32, r#type: u32) -> u32 {
        instant + system_time + r#type
    }
}

// accepted
mod integers_ranges_and_tuple_indices {
    pub fn f(p: ((u8, u8), u8)) -> u32 {
        u32::from(p.0 .1) + (0..10).map(|i: u32| 1.max(i)).sum::<u32>()
    }
}

// accepted
mod core_duration_and_btree_map {
    use std::collections::BTreeMap;
    pub fn f() -> (core::time::Duration, BTreeMap<u8, u8>) {
        (core::time::Duration::ZERO, BTreeMap::new())
    }
}

// accepted
mod a_module_named_env {
    mod env {
        pub fn var() -> u32 {
            1
        }
    }
    pub fn f() -> u32 {
        env::var()
    }
}

// accepted
mod core_renamed_by_extern_crate {
    extern crate core as c;
    pub fn f() -> c::time::Duration {
        c::time::Duration::from_secs(1)
    }
}

// accepted
mod ffi_integer_aliases {
    pub fn f(x: core::ffi::c_int) -> core::ffi::c_uint {
        x.unsigned_abs()
    }
}
