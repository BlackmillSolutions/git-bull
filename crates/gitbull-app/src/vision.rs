//! How a colour looks with a colour vision deficiency, and how far apart
//! two colours look: the simulation of Machado et al. (2009) at full
//! severity and the colour difference CIEDE2000. Used by the tests of the
//! palettes (design, decision 6).

use crate::theme::Rgb;

/// A colour vision deficiency at full severity.
#[derive(Clone, Copy, Debug)]
pub enum Deficiency {
    Protanopia,
    Deuteranopia,
    Tritanopia,
}

/// CIE L*a*b* under D65.
#[derive(Clone, Copy, Debug)]
pub struct Lab {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

/// The matrices of Machado, Oliveira and Fernandes (2009) at severity 1.0,
/// applied to linear RGB.
fn matrix(deficiency: Deficiency) -> [[f64; 3]; 3] {
    match deficiency {
        Deficiency::Protanopia => [
            [0.152286, 1.052583, -0.204868],
            [0.114503, 0.786281, 0.099216],
            [-0.003882, -0.048116, 1.051998],
        ],
        Deficiency::Deuteranopia => [
            [0.367322, 0.860646, -0.227968],
            [0.280085, 0.672501, 0.047413],
            [-0.011820, 0.042940, 0.968881],
        ],
        Deficiency::Tritanopia => [
            [1.255528, -0.076749, -0.178779],
            [-0.078411, 0.930809, 0.147602],
            [0.004733, 0.691367, 0.303900],
        ],
    }
}

fn to_linear(c: f64) -> f64 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// `colour` as seen with `deficiency`, in sRGB from 0 to 1 per channel.
pub fn simulate(colour: Rgb, deficiency: Deficiency) -> [f64; 3] {
    let linear = [colour.0, colour.1, colour.2].map(|c| to_linear(f64::from(c) / 255.0));
    matrix(deficiency)
        .map(|row| from_linear(row[0] * linear[0] + row[1] * linear[1] + row[2] * linear[2]))
}

/// An sRGB colour from 0 to 1 per channel in L*a*b*.
pub fn lab(srgb: [f64; 3]) -> Lab {
    let [r, g, b] = srgb.map(to_linear);
    let x = 0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b;
    let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175_0 * b;
    let z = 0.019_333_9 * r + 0.119_192_0 * g + 0.950_304_1 * b;
    let f = |t: f64| {
        let delta: f64 = 6.0 / 29.0;
        if t > delta.powi(3) {
            t.cbrt()
        } else {
            t / (3.0 * delta * delta) + 4.0 / 29.0
        }
    };
    // The white point D65.
    let (fx, fy, fz) = (f(x / 0.950_47), f(y), f(z / 1.088_83));
    Lab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

/// The colour difference CIEDE2000 of two colours, after Sharma, Wu and
/// Dalal (2005).
pub fn ciede2000(first: Lab, second: Lab) -> f64 {
    let (l1, a1, b1) = (first.l, first.a, first.b);
    let (l2, a2, b2) = (second.l, second.a, second.b);
    let pow7 = |c: f64| c.powi(7);
    let c_mean = (a1.hypot(b1) + a2.hypot(b2)) / 2.0;
    let g = 0.5 * (1.0 - (pow7(c_mean) / (pow7(c_mean) + pow7(25.0))).sqrt());
    let (a1, a2) = ((1.0 + g) * a1, (1.0 + g) * a2);
    let (c1, c2) = (a1.hypot(b1), a2.hypot(b2));
    let hue = |a: f64, b: f64, c: f64| {
        if c == 0.0 {
            0.0
        } else {
            b.atan2(a).to_degrees().rem_euclid(360.0)
        }
    };
    let (h1, h2) = (hue(a1, b1, c1), hue(a2, b2, c2));

    let delta_l = l2 - l1;
    let delta_c = c2 - c1;
    let delta_h = if c1 * c2 == 0.0 {
        0.0
    } else if (h2 - h1).abs() <= 180.0 {
        h2 - h1
    } else if h2 - h1 > 180.0 {
        h2 - h1 - 360.0
    } else {
        h2 - h1 + 360.0
    };
    let delta_big_h = 2.0 * (c1 * c2).sqrt() * (delta_h / 2.0).to_radians().sin();

    let l_mean = (l1 + l2) / 2.0;
    let c_mean = (c1 + c2) / 2.0;
    let h_mean = if c1 * c2 == 0.0 {
        h1 + h2
    } else if (h1 - h2).abs() <= 180.0 {
        (h1 + h2) / 2.0
    } else if h1 + h2 < 360.0 {
        (h1 + h2 + 360.0) / 2.0
    } else {
        (h1 + h2 - 360.0) / 2.0
    };
    let cos = |degrees: f64| degrees.to_radians().cos();
    let t =
        1.0 - 0.17 * cos(h_mean - 30.0) + 0.24 * cos(2.0 * h_mean) + 0.32 * cos(3.0 * h_mean + 6.0)
            - 0.20 * cos(4.0 * h_mean - 63.0);
    let delta_theta = 30.0 * (-((h_mean - 275.0) / 25.0).powi(2)).exp();
    let r_c = 2.0 * (pow7(c_mean) / (pow7(c_mean) + pow7(25.0))).sqrt();
    let s_l = 1.0 + 0.015 * (l_mean - 50.0).powi(2) / (20.0 + (l_mean - 50.0).powi(2)).sqrt();
    let s_c = 1.0 + 0.045 * c_mean;
    let s_h = 1.0 + 0.015 * c_mean * t;
    let r_t = -(2.0 * delta_theta).to_radians().sin() * r_c;

    let (l, c, h) = (delta_l / s_l, delta_c / s_c, delta_big_h / s_h);
    (l * l + c * c + h * h + r_t * c * h).sqrt()
}

/// How far apart `a` and `b` look with `deficiency`, in CIEDE2000.
pub fn seen_difference(a: Rgb, b: Rgb, deficiency: Deficiency) -> f64 {
    ciede2000(lab(simulate(a, deficiency)), lab(simulate(b, deficiency)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(found: f64, expected: f64, tolerance: f64) -> bool {
        (found - expected).abs() <= tolerance
    }

    #[test]
    fn ciede2000_matches_the_reference_pairs_of_sharma() {
        // Sharma, Wu and Dalal (2005), pairs 1, 7, 13, 17, 25 and 34.
        for ((l1, a1, b1), (l2, a2, b2), expected) in [
            ((50.0, 2.6772, -79.7751), (50.0, 0.0, -82.7485), 2.0425),
            ((50.0, 0.0, 0.0), (50.0, -1.0, 2.0), 2.3669),
            ((50.0, 2.4900, -0.0010), (50.0, -2.4900, 0.0011), 7.2195),
            ((50.0, 2.5, 0.0), (73.0, 25.0, -18.0), 27.1492),
            (
                (60.2574, -34.0099, 36.2677),
                (60.4626, -34.1751, 39.4387),
                1.2644,
            ),
            (
                (2.0776, 0.0795, -1.1350),
                (0.9033, -0.0636, -0.5514),
                0.9082,
            ),
        ] {
            let first = Lab {
                l: l1,
                a: a1,
                b: b1,
            };
            let second = Lab {
                l: l2,
                a: a2,
                b: b2,
            };
            let found = ciede2000(first, second);
            assert!(
                close(found, expected, 1e-4),
                "{found} instead of {expected}"
            );
            assert!(
                close(ciede2000(second, first), expected, 1e-4),
                "not symmetric"
            );
        }
    }

    #[test]
    fn srgb_primaries_have_their_known_lab_values() {
        for (srgb, (l, a, b)) in [
            ([1.0, 1.0, 1.0], (100.0, 0.0, 0.0)),
            ([1.0, 0.0, 0.0], (53.241, 80.092, 67.203)),
            ([0.0, 0.0, 1.0], (32.297, 79.188, -107.860)),
        ] {
            let found = lab(srgb);
            assert!(
                close(found.l, l, 0.01) && close(found.a, a, 0.01) && close(found.b, b, 0.01),
                "{srgb:?} gives {found:?}"
            );
        }
    }

    #[test]
    fn greys_look_the_same_with_every_deficiency() {
        for deficiency in [
            Deficiency::Protanopia,
            Deficiency::Deuteranopia,
            Deficiency::Tritanopia,
        ] {
            for grey in [0x20, 0x80, 0xd0] {
                let seen = simulate(Rgb(grey, grey, grey), deficiency);
                let expected = f64::from(grey) / 255.0;
                assert!(
                    seen.iter().all(|channel| close(*channel, expected, 0.002)),
                    "{deficiency:?} turns grey {grey} into {seen:?}"
                );
            }
        }
    }

    #[test]
    fn red_follows_the_matrices_of_machado() {
        // The first column of each matrix, the linear red channel, encoded
        // to sRGB with negative values clipped.
        for (deficiency, expected) in [
            (Deficiency::Protanopia, [0.4266, 0.3727, 0.0]),
            (Deficiency::Deuteranopia, [0.6401, 0.5658, 0.0]),
            (Deficiency::Tritanopia, [1.0, 0.0, 0.0584]),
        ] {
            let seen = simulate(Rgb(255, 0, 0), deficiency);
            assert!(
                seen.iter().zip(expected).all(|(s, e)| close(*s, e, 0.001)),
                "{deficiency:?} turns red into {seen:?}"
            );
        }
    }

    #[test]
    fn red_and_green_look_alike_without_red_green_vision() {
        let (red, green) = (Rgb(0xcf, 0x22, 0x2e), Rgb(0x1a, 0x7f, 0x37));
        let normal = ciede2000(lab(simulate_none(red)), lab(simulate_none(green)));
        let deuteranopia = seen_difference(red, green, Deficiency::Deuteranopia);
        assert!(normal > 40.0, "{normal}");
        assert!(deuteranopia < 15.0, "{deuteranopia}");
    }

    fn simulate_none(colour: Rgb) -> [f64; 3] {
        [colour.0, colour.1, colour.2].map(|c| f64::from(c) / 255.0)
    }
}
