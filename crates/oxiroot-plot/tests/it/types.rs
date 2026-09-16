//! Unit tests for the public value types: `Color`, `Colormap`, `Marker`,
//! `HistType` — their conversions, parsing, and `Display`/`FromStr` round-trips.

use std::str::FromStr;

use oxiroot_plot::{Color, Colormap, HistType, Marker};

#[test]
fn color_hex_round_trips_with_to_hex() {
    let c = Color::hex("#1f77b4");
    assert_eq!((c.r, c.g, c.b, c.a), (0x1f, 0x77, 0xb4, 255));
    assert_eq!(c.to_hex(), "#1f77b4");
}

#[test]
fn color_hex_carries_alpha_round_trip() {
    let c = Color::rgba(1, 2, 3, 128);
    assert_eq!(c.to_hex(), "#01020380");
    assert_eq!(Color::hex("#01020380"), c);
}

#[test]
fn color_fromstr_accepts_optional_hash_and_alpha() {
    assert_eq!(
        "#1f77b4".parse::<Color>().unwrap(),
        Color::rgb(0x1f, 0x77, 0xb4)
    );
    assert_eq!(
        "1f77b4".parse::<Color>().unwrap(),
        Color::rgb(0x1f, 0x77, 0xb4)
    );
    assert_eq!(Color::from_str("#ffffffff").unwrap(), Color::WHITE);
}

#[test]
fn color_fromstr_rejects_garbage_and_names_the_input() {
    assert!("nope".parse::<Color>().is_err());
    assert!("#12".parse::<Color>().is_err()); // wrong length
    assert!("#zzzzzz".parse::<Color>().is_err()); // non-hex digits
    let e = "bad-input".parse::<Color>().unwrap_err();
    assert!(
        e.to_string().contains("bad-input"),
        "Display should name the offending input, got: {e}"
    );
}

#[test]
#[should_panic(expected = "invalid hex color")]
fn color_hex_panics_loudly_on_garbage() {
    // The ergonomic literal constructor must surface a typo, not silently render
    // black (the pre-redesign footgun).
    let _ = Color::hex("not-a-color");
}

#[test]
fn color_std_trait_conversions() {
    assert_eq!(Color::default(), Color::BLACK);
    assert_eq!(Color::from((10u8, 20, 30)), Color::rgb(10, 20, 30));
    assert_eq!(Color::from([10u8, 20, 30]), Color::rgb(10, 20, 30));

    let c = Color::WHITE.with_alpha(0.5);
    assert_eq!(c.a, 128); // 0.5 * 255 rounds to 128
    assert!((c.opacity() - 128.0 / 255.0).abs() < 1e-6);
    assert_eq!(Color::TRANSPARENT.a, 0);
}

#[test]
fn colormap_fromstr_display_round_trip() {
    for (s, cm) in [
        ("viridis", Colormap::Viridis),
        ("plasma", Colormap::Plasma),
        ("gray", Colormap::Gray),
        ("gray_r", Colormap::GrayR),
    ] {
        assert_eq!(s.parse::<Colormap>().unwrap(), cm);
        assert_eq!(cm.to_string(), s);
        assert_eq!(cm.name(), s);
    }
    // British spelling aliases parse too.
    assert_eq!("grey".parse::<Colormap>().unwrap(), Colormap::Gray);
    assert_eq!("grey_r".parse::<Colormap>().unwrap(), Colormap::GrayR);
    assert!("magma".parse::<Colormap>().is_err());
    assert_eq!(Colormap::default(), Colormap::Viridis);
}

#[test]
fn colormap_sampling_clamps_and_spans() {
    let v = Colormap::Viridis;
    let lo = v.sample(0.0);
    let hi = v.sample(1.0);
    assert_ne!(lo, hi, "the two ends of viridis must differ");
    assert_eq!(v.sample(-5.0), lo, "samples below 0 clamp to the low end");
    assert_eq!(v.sample(5.0), hi, "samples above 1 clamp to the high end");

    // Gray is an exact linear ramp; GrayR is its reverse.
    assert_eq!(Colormap::Gray.sample(0.0), Color::rgb(0, 0, 0));
    assert_eq!(Colormap::Gray.sample(1.0), Color::rgb(255, 255, 255));
    assert_eq!(Colormap::GrayR.sample(0.0), Color::rgb(255, 255, 255));
    assert_eq!(Colormap::GrayR.sample(1.0), Color::rgb(0, 0, 0));
}

#[test]
fn marker_fromstr_display_round_trip() {
    for (s, m) in [
        ("none", Marker::None),
        ("o", Marker::Circle),
        ("s", Marker::Square),
        ("^", Marker::TriangleUp),
    ] {
        assert_eq!(s.parse::<Marker>().unwrap(), m);
        assert_eq!(m.to_string(), s);
        assert_eq!(m.spec(), s);
    }
    assert_eq!("".parse::<Marker>().unwrap(), Marker::None);
    assert_eq!(Marker::default(), Marker::None);
    assert!("x".parse::<Marker>().is_err());
}

#[test]
fn histtype_fromstr_display_round_trip() {
    for (s, t) in [
        ("step", HistType::Step),
        ("fill", HistType::Fill),
        ("errorbar", HistType::Errorbar),
        ("band", HistType::Band),
    ] {
        assert_eq!(s.parse::<HistType>().unwrap(), t);
        assert_eq!(t.to_string(), s);
        assert_eq!(t.name(), s);
    }
    assert_eq!(HistType::default(), HistType::Step);
    assert!("staircase".parse::<HistType>().is_err());
}
