//! A minimal complex-number type, enough for the IIR filter design in
//! `filter.rs`.

use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cx {
    pub re: f64,
    pub im: f64,
}

impl Cx {
    pub const fn new(re: f64, im: f64) -> Self {
        Cx { re, im }
    }

    pub const fn real(re: f64) -> Self {
        Cx { re, im: 0.0 }
    }

    pub fn norm(self) -> f64 {
        self.re.hypot(self.im)
    }

    pub fn scale(self, s: f64) -> Self {
        Cx::new(self.re * s, self.im * s)
    }

    /// Principal square root.
    pub fn sqrt(self) -> Self {
        if self.re == 0.0 && self.im == 0.0 {
            return Cx::new(0.0, 0.0);
        }
        let r = self.norm();
        let re = ((r + self.re) / 2.0).sqrt();
        let mut im = ((r - self.re) / 2.0).sqrt();
        if self.im < 0.0 {
            im = -im;
        }
        Cx::new(re, im)
    }

    /// e^z.
    pub fn exp(self) -> Self {
        let e = self.re.exp();
        Cx::new(e * self.im.cos(), e * self.im.sin())
    }

    /// Hyperbolic sine.
    pub fn sinh(self) -> Self {
        Cx::new(
            self.re.sinh() * self.im.cos(),
            self.re.cosh() * self.im.sin(),
        )
    }
}

impl Add for Cx {
    type Output = Cx;
    fn add(self, o: Cx) -> Cx {
        Cx::new(self.re + o.re, self.im + o.im)
    }
}

impl Sub for Cx {
    type Output = Cx;
    fn sub(self, o: Cx) -> Cx {
        Cx::new(self.re - o.re, self.im - o.im)
    }
}

impl Mul for Cx {
    type Output = Cx;
    fn mul(self, o: Cx) -> Cx {
        Cx::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}

impl Div for Cx {
    type Output = Cx;
    fn div(self, o: Cx) -> Cx {
        let d = o.re * o.re + o.im * o.im;
        Cx::new(
            (self.re * o.re + self.im * o.im) / d,
            (self.im * o.re - self.re * o.im) / d,
        )
    }
}

impl Neg for Cx {
    type Output = Cx;
    fn neg(self) -> Cx {
        Cx::new(-self.re, -self.im)
    }
}
