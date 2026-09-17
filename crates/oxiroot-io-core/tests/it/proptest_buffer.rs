//! Generative properties for the byte buffers: anything `WBuffer` writes,
//! `RBuffer` reads back identically — including float special values (NaN, ±inf,
//! −0.0, via bit-pattern equality) and ROOT strings across the 254/255-byte
//! short/long length boundary.

use oxiroot_io_core::buffer::{RBuffer, WBuffer};
use proptest::prelude::*;

proptest! {
    #[test]
    fn be_f64_round_trips_bit_exact(x in any::<f64>()) {
        let mut w = WBuffer::new();
        w.be_f64(x);
        let bytes = w.into_vec();
        let mut r = RBuffer::new(&bytes);
        prop_assert_eq!(x.to_bits(), r.be_f64().unwrap().to_bits());
    }

    #[test]
    fn mixed_be_le_sequence_round_trips(a in any::<u32>(), b in any::<i32>(), c in any::<f64>()) {
        let mut w = WBuffer::new();
        w.be_u32(a);
        w.le_u32(a);
        w.be_i32(b);
        w.be_f64(c);
        let bytes = w.into_vec();
        let mut r = RBuffer::new(&bytes);
        prop_assert_eq!(r.be_u32().unwrap(), a);
        prop_assert_eq!(r.le_u32().unwrap(), a);
        prop_assert_eq!(r.be_i32().unwrap(), b);
        prop_assert_eq!(r.be_f64().unwrap().to_bits(), c.to_bits());
    }

    // Lengths span 0..300, exercising the 254/255 short→long string switch.
    #[test]
    fn string_round_trips(s in "[ -~]{0,300}") {
        let mut w = WBuffer::new();
        w.string(&s);
        let bytes = w.into_vec();
        let mut r = RBuffer::new(&bytes);
        prop_assert_eq!(r.string().unwrap(), s);
    }
}

#[test]
fn f64_special_values_round_trip_bit_exact() {
    for x in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        0.0,
        f64::MIN_POSITIVE,
    ] {
        let mut w = WBuffer::new();
        w.be_f64(x);
        let bytes = w.into_vec();
        let mut r = RBuffer::new(&bytes);
        assert_eq!(x.to_bits(), r.be_f64().unwrap().to_bits(), "{x:?}");
    }
}
