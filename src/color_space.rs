use crate::math::*;
use crate::Color;

// An RGB color space expressed in relation to the CIE XYZ color space:
// https://en.wikipedia.org/wiki/CIE_1931_color_space
#[derive(Debug, Clone, PartialEq)]
pub struct ColorSpace {
    to_XYZ: Matrix3x3,
    from_XYZ: Matrix3x3,
    transfer_function: TransferFunction,
}

/// Chromaticity values represent the hue of a color, irrespective of brightness
#[derive(Debug, Copy, Clone)]
pub struct Chromaticity {
    pub x: f64,
    pub y: f64,
}

impl Chromaticity {
    pub fn new(x: f64, y: f64) -> Self {
        Chromaticity { x, y }
    }
}
/// If the color space stores RGB values nonlinearly this specifies how to make them linear.
/// This should be possible to express numerically.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TransferFunction {
    /// Use the sRGB transfer function
    SRGB,
    /// The values are already linear
    None,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct XYZ {
    pub X: f64,
    pub Y: f64,
    pub Z: f64,
}

impl ColorSpace {
    /// Primaries are specified as xy chromaticity values.
    /// White point is specified in XYZ space
    /// The white point represents the brightest color that can be represented.
    /// More info:
    /// https://en.wikipedia.org/wiki/CIE_1931_color_space#CIE_xy_chromaticity_diagram_and_the_CIE_xyY_color_space
    pub fn new(
        red_primary: Chromaticity,
        green_primary: Chromaticity,
        blue_primary: Chromaticity,
        white_point: XYZ,
        transfer_function: TransferFunction,
    ) -> Self {
        // Reference:
        // http://www.brucelindbloom.com/index.html?Eqn_RGB_XYZ_Matrix.html

        // Do the RGB values this converts need to be between 0 and 1 as noted at the above link?

        // First convert the chromaticity primaries into XYZ space.
        let r = Vector3::new(
            red_primary.x / red_primary.y,
            1.0,
            (1.0 - red_primary.x - red_primary.y) / red_primary.y,
        );

        let g = Vector3::new(
            green_primary.x / green_primary.y,
            1.0,
            (1.0 - green_primary.x - green_primary.y) / green_primary.y,
        );

        let b = Vector3::new(
            blue_primary.x / blue_primary.y,
            1.0,
            (1.0 - blue_primary.x - blue_primary.y) / blue_primary.y,
        );

        let inverse = Matrix3x3::from_columns(r, g, b).inverse();
        let s = inverse * Vector3::new(white_point.X, white_point.Y, white_point.Z);

        // The three primaries in XYZ space relative to the white point passed in.
        let sr = r * s.x;
        let sg = g * s.y;
        let sb = b * s.z;

        // The D50 white point is used to store colors internally
        // If the color space being declared is not relative to the D50 white point then the primaries must
        // be converted to be relative to D50.
        // D50 is used because ICC profiles are always specified with D50 and this might avoid conversions.
        let (to_XYZ, from_XYZ) = if white_point != Self::D50_WHITE_POINT {
            let white_point_adaptation =
                ChromaticAdaptation::new(white_point, Self::D50_WHITE_POINT);
            let white_point_adaptation_inverse =
                ChromaticAdaptation::new(Self::D50_WHITE_POINT, white_point);
            (
                white_point_adaptation.inner_matrix * Matrix3x3::from_columns(sr, sg, sb),
                Matrix3x3::from_columns(sr, sg, sb).inverse()
                    * white_point_adaptation_inverse.inner_matrix,
            )
        } else {
            let to_XYZ = Matrix3x3::from_columns(sr, sg, sb);
            (to_XYZ, to_XYZ.inverse())
        };

        Self {
            to_XYZ,
            from_XYZ,
            transfer_function,
        }
    }

    /// Creates a color with the specified RGB values for the color space
    pub fn new_color(&self, r: f64, g: f64, b: f64, a: f64) -> Color {
        let rgb = Vector3::new(r, g, b);
        let rgb = match self.transfer_function {
            TransferFunction::SRGB => Vector3::new(
                srgb_to_linear(rgb.x),
                srgb_to_linear(rgb.y),
                srgb_to_linear(rgb.z),
            ),
            TransferFunction::None => rgb,
        };
        let XYZ = self.to_XYZ * rgb;
        Color {
            X: XYZ.x,
            Y: XYZ.y,
            Z: XYZ.z,
            a,
        }
    }

    /// Creates a new color from the hex values of a number.
    pub fn new_color_from_hex(&self, hex: u32, alpha: f64) -> Color {
        let r = ((hex >> 16) & 0xFF) as f64 / 255.0;
        let g = ((hex >> 8) & 0xFF) as f64 / 255.0;
        let b = ((hex) & 0xFF) as f64 / 255.0;
        self.new_color(r, g, b, alpha)
    }

    /// Creates a new color from the hex values of a number.
    /// Alpha is transparency
    pub fn new_color_from_bytes(&self, r: u8, b: u8, g: u8, alpha: u8) -> Color {
        let r = r as f64 / 255.0;
        let g = g as f64 / 255.0;
        let b = b as f64 / 255.0;
        let a = alpha as f64 / 255.0;
        self.new_color(r, g, b, a)
    }

    /// Gets the RGBA values for the color as expressed in this color space
    /// RGB values outside of 0.0 to 1.0 will be clipped.
    pub fn color_to_rgba(&self, color: &Color) -> (f64, f64, f64, f64) {
        let (r, g, b, a) = self.color_to_rgba_unclipped(color);
        (
            r.max(0.0).min(1.0),
            g.max(0.0).min(1.0),
            b.max(0.0).min(1.0),
            a,
        )
    }

    /// Gets the RGBA values for the color as expressed in this color space
    /// RGB values are allowed to go outside the 0.0 to 1.0 range.
    /// The transfer function (if not None) is mirrored for values less than 0.0
    pub fn color_to_rgba_unclipped(&self, color: &Color) -> (f64, f64, f64, f64) {
        let XYZ = Vector3::new(color.X, color.Y, color.Z);
        let rgb = self.from_XYZ * XYZ;
        let rgb = match self.transfer_function {
            TransferFunction::SRGB => Vector3::new(
                linear_to_srgb(rgb.x),
                linear_to_srgb(rgb.y),
                linear_to_srgb(rgb.z),
            ),
            TransferFunction::None => rgb,
        };
        (rgb.x, rgb.y, rgb.z, color.a)
    }

    /// The popular sRGB color space
    /// https://en.wikipedia.org/wiki/SRGB
    /// Conversion values in table below were calculated with this library.
    /// Chromaticity of primaries as expressed in CIE XYZ 1931
    /// Red primary x: 0.64 y: 0.33
    /// Green primary x: 0.3 y: 0.6
    /// Blue primary x: 0.15 y: 0.06
    /// White point (D65) as expressed in CIE XYZ 1931
    /// X: 0.95047
    /// Y: 1.0
    /// Z: 1.08883
    pub const SRGB: ColorSpace = ColorSpace {
        to_XYZ: Matrix3x3 {
            c0: Vector3 {
                x: 0.43607469963825646,
                y: 0.222504483975651,
                z: 0.013932161672457605,
            },
            c1: Vector3 {
                x: 0.3850648611372289,
                y: 0.716878635320373,
                z: 0.09710449715679705,
            },
            c2: Vector3 {
                x: 0.1430804288791148,
                y: 0.0606169168164421,
                z: 0.714173287229624,
            },
        },
        from_XYZ: Matrix3x3 {
            c0: Vector3 {
                x: 3.133856052478418,
                y: -0.9787683608686029,
                z: 0.07194531250686317,
            },
            c1: Vector3 {
                x: -1.6168663437735835,
                y: 1.9161412999006184,
                z: -0.22899133449586742,
            },
            c2: Vector3 {
                x: -0.4906148432537726,
                y: 0.03345409228962241,
                z: 1.405242677723986,
            },
        },
        transfer_function: TransferFunction::SRGB,
    };

    /// Exact same as the above SRGB space, except with a linear transfer function.
    pub const SRGB_LINEAR: ColorSpace = ColorSpace {
        to_XYZ: Matrix3x3 {
            c0: Vector3 {
                x: 0.43607469963825646,
                y: 0.222504483975651,
                z: 0.013932161672457605,
            },
            c1: Vector3 {
                x: 0.3850648611372289,
                y: 0.716878635320373,
                z: 0.09710449715679705,
            },
            c2: Vector3 {
                x: 0.1430804288791148,
                y: 0.0606169168164421,
                z: 0.714173287229624,
            },
        },
        from_XYZ: Matrix3x3 {
            c0: Vector3 {
                x: 3.133856052478418,
                y: -0.9787683608686029,
                z: 0.07194531250686317,
            },
            c1: Vector3 {
                x: -1.6168663437735835,
                y: 1.9161412999006184,
                z: -0.22899133449586742,
            },
            c2: Vector3 {
                x: -0.4906148432537726,
                y: 0.03345409228962241,
                z: 1.405242677723986,
            },
        },
        transfer_function: TransferFunction::None,
    };

    /// "Horizon light". A commonly used white point.
    /// https://en.wikipedia.org/wiki/Standard_illuminant
    /// XYZ values sourced from here: http://www.brucelindbloom.com/index.html?Eqn_ChromAdapt.html
    pub const D50_WHITE_POINT: XYZ = XYZ {
        X: 0.96422,
        Y: 1.0,
        Z: 0.82521,
    };

    /// A white point that corresponds to average midday light in Western / Northern Europe:
    /// https://en.wikipedia.org/wiki/Illuminant_D65
    pub const D65_WHITE_POINT: XYZ = XYZ {
        X: 0.95047,
        Y: 1.0,
        Z: 1.08883,
    };
}

/// If frequent color space conversions are to be performed, use this.
pub struct ColorSpaceConverter {
    conversion_matrix: Matrix3x3,
}

impl ColorSpaceConverter {
    pub fn new(from: &ColorSpace, to: &ColorSpace) -> Self {
        Self {
            conversion_matrix: to.from_XYZ * from.to_XYZ,
        }
    }

    pub fn convert_color(&self, color: &(f64, f64, f64)) -> (f64, f64, f64) {
        let color = Vector3::new(color.0, color.1, color.2);
        let color = self.conversion_matrix * color;
        (color.x, color.y, color.z)
    }
}

/// Convert between XYZ color spaces with different white points.
/// Wavelengths are perceived as one color in one lighting condition and a
/// different color under a different lighting condition.
/// Our eyes adjust to lighting and if a room has yellow-ish lighting
/// (it has a yellow-ish whitepoint) then what appears white is produced
/// with yellow-ish wavelenghts.
///
/// This function first converts to a space that represents our eye's cone responses using a
/// Bradford transform then converts back.
/// V4 and earlier ICC profiles are specified with a D50 white point.
/// The profile may use actually use a different white point, but the ICC
/// profile requires that the color primaries be expressed in relation to D50.
/// Profiles may include a 'chad' tag that specifies a matrix that was used
/// to convert from primaries in relation to the original white point to D50.
#[derive(Debug, Clone, PartialEq)]
pub struct ChromaticAdaptation {
    pub(crate) inner_matrix: Matrix3x3,
}

impl ChromaticAdaptation {
    pub fn new(source_white_point: XYZ, destination_white_point: XYZ) -> Self {
        // Implemented using the techniques described here:
        // http://www.brucelindbloom.com/index.html?Eqn_ChromAdapt.html

        // To do math with the XYZ values convert them to Vector3s.
        let source_white_point = Vector3::new(
            source_white_point.X,
            source_white_point.Y,
            source_white_point.Z,
        );
        let destination_white_point = Vector3::new(
            destination_white_point.X,
            destination_white_point.Y,
            destination_white_point.Z,
        );

        // The Bradford matrix constants are found at the above link.
        // The matrix is also available here: https://en.wikipedia.org/wiki/LMS_color_space
        // These matrices convert XYZ values to LMS values measuring the response of cones.
        let bradford_matrix = Matrix3x3 {
            c0: Vector3 {
                x: 0.8951000,
                y: -0.7502000,
                z: 0.0389000,
            },
            c1: Vector3 {
                x: 0.2664000,
                y: 1.7135000,
                z: -0.0685000,
            },
            c2: Vector3 {
                x: -0.1614000,
                y: 0.0367000,
                z: 1.0296000,
            },
        };

        let bradford_matrix_inverse = Matrix3x3 {
            c0: Vector3 {
                x: 0.9869929,
                y: 0.4323053,
                z: -0.0085287,
            },
            c1: Vector3 {
                x: -0.1470543,
                y: 0.5183603,
                z: 0.0400428,
            },
            c2: Vector3 {
                x: 0.1599627,
                y: 0.0492912,
                z: 0.9684867,
            },
        };

        // "crs" stands for "Cone response of source white point"
        // "crd" stands for "Cone response of destination white point"
        // The xyz values correspond to the response of the three cones.
        // These three responses are the "LMS" color space.
        // "LMS" stands for "Long", "Medium", "Short" based on the wavelengths
        // the three types of cones respond to.
        let crs = bradford_matrix * source_white_point;
        let crd = bradford_matrix * destination_white_point;

        let intermediate_matrix = Matrix3x3::from_columns(
            Vector3::new(crd.x / crs.x, 0., 0.),
            Vector3::new(0., crd.y / crs.y, 0.),
            Vector3::new(0., 0., crd.z / crs.z),
        );

        let inner_matrix = bradford_matrix_inverse * intermediate_matrix * bradford_matrix;

        Self { inner_matrix }
    }

    pub fn convert(&self, xyz: XYZ) -> XYZ {
        let v = Vector3::new(xyz.X, xyz.Y, xyz.Z);
        let v = self.inner_matrix * v;
        XYZ {
            X: v.x,
            Y: v.y,
            Z: v.z,
        }
    }
}

// Sourced from Wikipedia: https://en.wikipedia.org/wiki/SRGB
// If u is below 0 then then calculate the equation with the negation of the
// absolute value of u. This is to match the expectations for extended sRGB
// color spaces.
fn linear_to_srgb(u: f64) -> f64 {
    let sign = u.signum();
    let u = u.abs();
    let r = if u <= 0.0031308 {
        u * 12.92
    } else {
        (1.055 * f64::powf(u, 1.0 / 2.4)) - 0.055
    };
    r * sign
}

fn srgb_to_linear(u: f64) -> f64 {
    let sign = u.signum();
    let u = u.abs();
    let r = if u <= 0.04045 {
        u / 12.92
    } else {
        f64::powf((u + 0.055) / 1.055, 2.4)
    };
    r * sign
}
