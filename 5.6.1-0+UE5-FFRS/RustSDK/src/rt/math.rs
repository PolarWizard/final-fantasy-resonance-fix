//! Math on the generated FVector, FVector2D and FRotator, like the C++ SDK (UE5: all components are f64).

use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

use crate::config::{FRotator, FVector, FVector2D};

/// Component-wise operators: `+ - * /` with the same type, `* /` with a scalar, and the assigning forms.
macro_rules! componentwise_ops {
    ($t:ty { $($field:ident),+ }) => {
        impl Add for $t {
            type Output = Self;
            fn add(mut self, Other: Self) -> Self { $(self.$field += Other.$field;)+ self }
        }
        impl Sub for $t {
            type Output = Self;
            fn sub(mut self, Other: Self) -> Self { $(self.$field -= Other.$field;)+ self }
        }
        impl Mul for $t {
            type Output = Self;
            fn mul(mut self, Other: Self) -> Self { $(self.$field *= Other.$field;)+ self }
        }
        impl Div for $t {
            type Output = Self;
            fn div(mut self, Other: Self) -> Self { $(self.$field /= Other.$field;)+ self }
        }
        impl Mul<f64> for $t {
            type Output = Self;
            fn mul(mut self, Scalar: f64) -> Self { $(self.$field *= Scalar;)+ self }
        }
        impl Div<f64> for $t {
            type Output = Self;
            fn div(mut self, Scalar: f64) -> Self { $(self.$field /= Scalar;)+ self }
        }
        impl AddAssign for $t {
            fn add_assign(&mut self, Other: Self) { $(self.$field += Other.$field;)+ }
        }
        impl SubAssign for $t {
            fn sub_assign(&mut self, Other: Self) { $(self.$field -= Other.$field;)+ }
        }
        impl MulAssign for $t {
            fn mul_assign(&mut self, Other: Self) { $(self.$field *= Other.$field;)+ }
        }
        impl DivAssign for $t {
            fn div_assign(&mut self, Other: Self) { $(self.$field /= Other.$field;)+ }
        }
        impl MulAssign<f64> for $t {
            fn mul_assign(&mut self, Scalar: f64) { $(self.$field *= Scalar;)+ }
        }
        impl DivAssign<f64> for $t {
            fn div_assign(&mut self, Scalar: f64) { $(self.$field /= Scalar;)+ }
        }
        impl PartialEq for $t {
            fn eq(&self, Other: &Self) -> bool { true $(&& self.$field == Other.$field)+ }
        }
    };
}

componentwise_ops!(FVector { X, Y, Z });
componentwise_ops!(FVector2D { X, Y });
componentwise_ops!(FRotator { Pitch, Yaw, Roll });

impl FVector {
    pub fn new(X: f64, Y: f64, Z: f64) -> Self {
        let mut v: Self = unsafe { core::mem::zeroed() };
        (v.X, v.Y, v.Z) = (X, Y, Z);
        v
    }

    pub fn IsZero(&self) -> bool {
        self.X == 0.0 && self.Y == 0.0 && self.Z == 0.0
    }

    pub fn Dot(&self, Other: &Self) -> f64 {
        self.X * Other.X + self.Y * Other.Y + self.Z * Other.Z
    }

    pub fn Magnitude(&self) -> f64 {
        self.Dot(self).sqrt()
    }

    pub fn GetNormalized(&self) -> Self {
        *self / self.Magnitude()
    }

    pub fn GetDistanceTo(&self, Other: &Self) -> f64 {
        (*Other - *self).Magnitude()
    }

    pub fn GetDistanceToInMeters(&self, Other: &Self) -> f64 {
        self.GetDistanceTo(Other) * 0.01
    }

    pub fn Normalize(&mut self) -> &mut Self {
        *self /= self.Magnitude();
        self
    }
}

impl FVector2D {
    pub fn new(X: f64, Y: f64) -> Self {
        let mut v: Self = unsafe { core::mem::zeroed() };
        (v.X, v.Y) = (X, Y);
        v
    }

    pub fn IsZero(&self) -> bool {
        self.X == 0.0 && self.Y == 0.0
    }

    pub fn Dot(&self, Other: &Self) -> f64 {
        self.X * Other.X + self.Y * Other.Y
    }

    pub fn Magnitude(&self) -> f64 {
        self.Dot(self).sqrt()
    }

    pub fn GetNormalized(&self) -> Self {
        *self / self.Magnitude()
    }

    pub fn GetDistanceTo(&self, Other: &Self) -> f64 {
        (*Other - *self).Magnitude()
    }

    pub fn Normalize(&mut self) -> &mut Self {
        *self /= self.Magnitude();
        self
    }
}

impl FRotator {
    pub fn new(Pitch: f64, Yaw: f64, Roll: f64) -> Self {
        let mut r: Self = unsafe { core::mem::zeroed() };
        (r.Pitch, r.Yaw, r.Roll) = (Pitch, Yaw, Roll);
        r
    }

    /// `Angle` in [0, 360).
    pub fn ClampAxis(Angle: f64) -> f64 {
        let angle = Angle % 360.0;
        if angle < 0.0 { angle + 360.0 } else { angle }
    }

    /// `Angle` in (-180, 180].
    pub fn NormalizeAxis(Angle: f64) -> f64 {
        let angle = Self::ClampAxis(Angle);
        if angle > 180.0 { angle - 360.0 } else { angle }
    }

    pub fn GetNormalized(&self) -> Self {
        let mut rotator = *self;
        rotator.Normalize();
        rotator
    }

    pub fn IsZero(&self) -> bool {
        Self::ClampAxis(self.Pitch) == 0.0 && Self::ClampAxis(self.Yaw) == 0.0 && Self::ClampAxis(self.Roll) == 0.0
    }

    pub fn Normalize(&mut self) -> &mut Self {
        self.Pitch = Self::NormalizeAxis(self.Pitch);
        self.Yaw = Self::NormalizeAxis(self.Yaw);
        self.Roll = Self::NormalizeAxis(self.Roll);
        self
    }

    pub fn Clamp(&mut self) -> &mut Self {
        self.Pitch = Self::ClampAxis(self.Pitch);
        self.Yaw = Self::ClampAxis(self.Yaw);
        self.Roll = Self::ClampAxis(self.Roll);
        self
    }
}
