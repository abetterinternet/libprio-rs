// Copyright (c) 2020 Apple Inc.
// SPDX-License-Identifier: MPL-2.0

//! Finite field arithmetic.
//!
//! Each field has an associated parameter called the "generator" that generates a multiplicative
//! subgroup of order `2^n` for some `n`.

use crate::fp::{FP126, FP32, FP64, FP80};
use crate::prng::Prng;
use serde::{Deserialize, Serialize};
use std::{
    cmp::min,
    convert::TryFrom,
    fmt::{Debug, Display, Formatter},
    ops::{Add, AddAssign, BitAnd, Div, DivAssign, Mul, MulAssign, Neg, Shr, Sub, SubAssign},
};

/// Possible errors from finite field operations.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum FieldError {
    /// Input sizes do not match
    #[error("input sizes do not match")]
    InputSizeMismatch,
    /// Returned by `FieldElement::read_from()` if the input buffer is too short.
    #[error("short read from byte slice")]
    FromBytesShortRead,
    /// Returned by `FieldElement::read_from()` if the input is larger than the modulus.
    #[error("read from byte slice exceeds modulus")]
    FromBytesModulusOverflow,
}

/// Objects with this trait represent an element of `GF(p)` for some prime `p`.
pub trait FieldElement:
    Sized
    + Debug
    + Copy
    + PartialEq
    + Eq
    + Add<Output = Self>
    + AddAssign
    + Sub<Output = Self>
    + SubAssign
    + Mul<Output = Self>
    + MulAssign
    + Div<Output = Self>
    + DivAssign
    + Neg<Output = Self>
    + Display
    + From<<Self as FieldElement>::Integer>
    + 'static // NOTE This bound is needed for downcasting a `dyn Gadget<F>>` to a concrete type.
{
    /// Size of each field element in bytes.
    const BYTES: usize;

    /// The error returned if converting `usize` to an `Int` fails.
    type IntegerTryFromError: Debug;

    /// The integer representation of the field element.
    type Integer: Copy
        + Debug
        + PartialOrd
        + BitAnd<Output = <Self as FieldElement>::Integer>
        + Div<Output = <Self as FieldElement>::Integer>
        + Shr<Output = <Self as FieldElement>::Integer>
        + Sub<Output = <Self as FieldElement>::Integer>
        + TryFrom<usize, Error = Self::IntegerTryFromError>;

    /// Modular exponentation, i.e., `self^exp (mod p)`.
    fn pow(&self, exp: Self::Integer) -> Self;

    /// Modular inversion, i.e., `self^-1 (mod p)`. If `self` is 0, then the output is undefined.
    fn inv(&self) -> Self;

    /// Returns the prime modulus `p`.
    fn modulus() -> Self::Integer;

    /// Writes the field element to the end of input buffer. Exactly `BYTES` bytes will be written.
    ///
    /// TODO(acmiyaguchi) Replace this with an implementation of the corresponding serde trait
    fn append_to(&self, bytes: &mut Vec<u8>);

    /// Interprets the next `BYTES` bytes from the input buffer as an element of the field. An
    /// error is returned if the bytes encode an integer larger than the field modulus.
    ///
    /// TODO(acmiyaguchi) Replace this with an implementation of the corresponding serde trait
    fn read_from(bytes: &[u8]) -> Result<Self, FieldError>;

    /// Interprets the next `BYTES` bytes from the input buffer as an element of the field. The `m`
    /// most significant bits are cleared, where `m` is equal to the length of `Integer` in bits
    /// minus the length of the modulus in bits. An error is returned if the result encodes an
    /// integer larger than the field modulus.
    ///
    /// WARNING: This function is used to convert a random byte string into a field element. It
    /// *should not* be used to deserialize field elements.
    fn try_from_random(bytes: &[u8]) -> Result<Self, FieldError>;

    /// Returns the size of the multiplicative subgroup generated by `generator()`.
    fn generator_order() -> Self::Integer;

    /// Returns the generator of the multiplicative subgroup of size `generator_order()`.
    fn generator() -> Self;

    /// Returns the `2^l`-th principal root of unity for any `l <= 20`. Note that the `2^0`-th
    /// prinicpal root of unity is 1 by definition.
    fn root(l: usize) -> Option<Self>;

    /// Returns the additive identity.
    fn zero() -> Self;

    /// Returns the multiplicative identity.
    fn one() -> Self;
}

macro_rules! make_field {
    (
        $(#[$meta:meta])*
        $elem:ident, $int:ident, $fp:ident, $bytes:literal
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialOrd, Ord, Hash, Default, Deserialize, Serialize)]
        pub struct $elem(u128);

        impl $elem {
            fn try_from_bytes(bytes: &[u8], mask: u128) -> Result<Self, FieldError> {
                if Self::BYTES > bytes.len() {
                    return Err(FieldError::FromBytesShortRead);
                }

                let mut int = 0;
                for i in 0..Self::BYTES {
                    int |= (bytes[i] as u128) << (i << 3);
                }

                int &= mask;

                if int >= $fp.p {
                    return Err(FieldError::FromBytesModulusOverflow);
                }
                Ok(Self($fp.elem(int)))
            }
        }

        impl PartialEq for $elem {
            fn eq(&self, rhs: &Self) -> bool {
                $fp.from_elem(self.0) == $fp.from_elem(rhs.0)
            }
        }

        impl Eq for $elem {}

        impl Add for $elem {
            type Output = $elem;
            fn add(self, rhs: Self) -> Self {
                Self($fp.add(self.0, rhs.0))
            }
        }

        impl Add for &$elem {
            type Output = $elem;
            fn add(self, rhs: Self) -> $elem {
                *self + *rhs
            }
        }

        impl AddAssign for $elem {
            fn add_assign(&mut self, rhs: Self) {
                *self = *self + rhs;
            }
        }

        impl Sub for $elem {
            type Output = $elem;
            fn sub(self, rhs: Self) -> Self {
                Self($fp.sub(self.0, rhs.0))
            }
        }

        impl Sub for &$elem {
            type Output = $elem;
            fn sub(self, rhs: Self) -> $elem {
                *self - *rhs
            }
        }

        impl SubAssign for $elem {
            fn sub_assign(&mut self, rhs: Self) {
                *self = *self - rhs;
            }
        }

        impl Mul for $elem {
            type Output = $elem;
            fn mul(self, rhs: Self) -> Self {
                Self($fp.mul(self.0, rhs.0))
            }
        }

        impl Mul for &$elem {
            type Output = $elem;
            fn mul(self, rhs: Self) -> $elem {
                *self * *rhs
            }
        }

        impl MulAssign for $elem {
            fn mul_assign(&mut self, rhs: Self) {
                *self = *self * rhs;
            }
        }

        impl Div for $elem {
            type Output = $elem;
            fn div(self, rhs: Self) -> Self {
                self * rhs.inv()
            }
        }

        impl Div for &$elem {
            type Output = $elem;
            fn div(self, rhs: Self) -> $elem {
                *self / *rhs
            }
        }

        impl DivAssign for $elem {
            fn div_assign(&mut self, rhs: Self) {
                *self = *self / rhs;
            }
        }

        impl Neg for $elem {
            type Output = $elem;
            fn neg(self) -> Self {
                Self($fp.neg(self.0))
            }
        }

        impl Neg for &$elem {
            type Output = $elem;
            fn neg(self) -> $elem {
                -(*self)
            }
        }

        impl From<$int> for $elem {
            fn from(x: $int) -> Self {
                Self($fp.elem(u128::try_from(x).unwrap()))
            }
        }

        impl From<$elem> for $int {
            fn from(x: $elem) -> Self {
                $int::try_from($fp.from_elem(x.0)).unwrap()
            }
        }

        impl PartialEq<$int> for $elem {
            fn eq(&self, rhs: &$int) -> bool {
                $fp.from_elem(self.0) == u128::try_from(*rhs).unwrap()
            }
        }

        impl Display for $elem {
            fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
                write!(f, "{}", $fp.from_elem(self.0))
            }
        }

        impl Debug for $elem {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", $fp.from_elem(self.0))
            }
        }

        impl FieldElement for $elem {
            const BYTES: usize = $bytes;
            type Integer = $int;
            type IntegerTryFromError = <Self::Integer as TryFrom<usize>>::Error;

            fn pow(&self, exp: Self::Integer) -> Self {
                Self($fp.pow(self.0, u128::try_from(exp).unwrap()))
            }

            fn inv(&self) -> Self {
                Self($fp.inv(self.0))
            }

            fn modulus() -> Self::Integer {
                $fp.p as $int
            }

            fn append_to(&self, bytes: &mut Vec<u8>) {
                let int = $fp.from_elem(self.0);
                let mut slice = [0; Self::BYTES];
                for i in 0..Self::BYTES {
                    slice[i] = ((int >> (i << 3)) & 0xff) as u8;
                }
                bytes.extend_from_slice(&slice);
            }

            fn read_from(bytes: &[u8]) -> Result<Self, FieldError> {
                $elem::try_from_bytes(bytes, u128::MAX)
            }

            fn try_from_random(bytes: &[u8]) -> Result<Self, FieldError> {
                $elem::try_from_bytes(bytes, $fp.bit_mask)
            }

            fn generator() -> Self {
                Self($fp.g)
            }

            fn generator_order() -> Self::Integer {
                1 << (Self::Integer::try_from($fp.num_roots).unwrap())
            }

            fn root(l: usize) -> Option<Self> {
                if l < min($fp.roots.len(), $fp.num_roots+1) {
                    Some(Self($fp.roots[l]))
                } else {
                    None
                }
            }

            fn zero() -> Self {
                Self(0)
            }

            fn one() -> Self {
                Self($fp.roots[0])
            }
        }
    };
}

make_field!(
    /// `GF(4293918721)`, a 32-bit field. The generator has order `2^20`.
    Field32,
    u32,
    FP32,
    4
);

make_field!(
    /// **(NOTE: These parameters are experimental. Applications should expect them to
    /// change.)** `GF(15564440312192434177)`, a 64-bit field. The generator has order `2^59`.
    Field64,
    u64,
    FP64,
    8
);

make_field!(
    ///  **(NOTE: These parameters are experimental. Applications should expect them to
    ///  change.)** `GF(779190469673491460259841)`, an 80-bit field. The generator has order `2^72`.
    Field80,
    u128,
    FP80,
    10
);

make_field!(
    ///  **(NOTE: These parameters are experimental. Applications should expect them to
    ///  change.)** `GF(74769074762901517850839147140769382401)`, a 126-bit field. The generator
    ///  has order `2^118`.
    Field126,
    u128,
    FP126,
    16
);

/// Merge two vectors of fields by summing other_vector into accumulator.
///
/// # Errors
///
/// Fails if the two vectors do not have the same length.
pub fn merge_vector<F: FieldElement>(
    accumulator: &mut [F],
    other_vector: &[F],
) -> Result<(), FieldError> {
    if accumulator.len() != other_vector.len() {
        return Err(FieldError::InputSizeMismatch);
    }
    for (a, o) in accumulator.iter_mut().zip(other_vector.iter()) {
        *a += *o;
    }

    Ok(())
}

/// Generate a vector of uniform random field elements.
pub fn rand<F: FieldElement>(len: usize) -> Result<Vec<F>, getrandom::Error> {
    Ok(Prng::new_with_length(len)?.collect())
}

/// Outputs an additive secret sharing of the input.
pub fn split<F: FieldElement>(
    inp: &[F],
    num_shares: usize,
) -> Result<Vec<Vec<F>>, getrandom::Error> {
    if num_shares == 0 {
        return Ok(vec![]);
    }

    let mut outp = vec![vec![F::zero(); inp.len()]; num_shares];
    for j in 0..inp.len() {
        outp[0][j] = inp[j];
    }

    let mut prng = Prng::new()?;
    for i in 1..num_shares {
        for j in 0..inp.len() {
            let r = prng.next().unwrap();
            outp[i][j] = r;
            outp[0][j] -= r;
        }
    }

    Ok(outp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fp::MAX_ROOTS;
    use assert_matches::assert_matches;

    #[test]
    fn test_accumulate() {
        let mut lhs = vec![Field32::zero(); 10];
        lhs.iter_mut().for_each(|f| *f = Field32(1));
        let mut rhs = vec![Field32::zero(); 10];
        rhs.iter_mut().for_each(|f| *f = Field32(2));

        merge_vector(&mut lhs, &rhs).unwrap();

        lhs.iter().for_each(|f| assert_eq!(*f, Field32(3)));
        rhs.iter().for_each(|f| assert_eq!(*f, Field32(2)));

        let wrong_len = vec![Field32::zero(); 9];
        let result = merge_vector(&mut lhs, &wrong_len);
        assert_matches!(result, Err(FieldError::InputSizeMismatch));
    }

    fn field_element_test<F: FieldElement>() {
        let mut prng: Prng<F> = Prng::new().unwrap();
        let int_modulus = F::modulus();
        let int_one = F::Integer::try_from(1).unwrap();
        let zero = F::zero();
        let one = F::one();
        let two = F::from(F::Integer::try_from(2).unwrap());
        let four = F::from(F::Integer::try_from(4).unwrap());

        // add
        assert_eq!(F::from(int_modulus - int_one) + one, zero);
        assert_eq!(one + one, two);
        assert_eq!(two + F::from(int_modulus), two);

        // sub
        assert_eq!(zero - one, F::from(int_modulus - int_one));
        assert_eq!(one - one, zero);
        assert_eq!(two - F::from(int_modulus), two);
        assert_eq!(one - F::from(int_modulus - int_one), two);

        // add + sub
        for _ in 0..100 {
            let f = prng.next().unwrap();
            let g = prng.next().unwrap();
            assert_eq!(f + g - f - g, zero);
            assert_eq!(f + g - g, f);
            assert_eq!(f + g - f, g);
        }

        // mul
        assert_eq!(two * two, four);
        assert_eq!(two * one, two);
        assert_eq!(two * zero, zero);
        assert_eq!(one * F::from(int_modulus), zero);

        // div
        assert_eq!(four / two, two);
        assert_eq!(two / two, one);
        assert_eq!(zero / two, zero);
        assert_eq!(two / zero, zero); // Undefined behavior
        assert_eq!(zero.inv(), zero); // Undefined behavior

        // mul + div
        for _ in 0..100 {
            let f = prng.next().unwrap();
            if f == zero {
                println!("skipped zero");
                continue;
            }
            assert_eq!(f * f.inv(), one);
            assert_eq!(f.inv() * f, one);
        }

        // pow
        assert_eq!(two.pow(F::Integer::try_from(0).unwrap()), one);
        assert_eq!(two.pow(int_one), two);
        assert_eq!(two.pow(F::Integer::try_from(2).unwrap()), four);
        assert_eq!(two.pow(int_modulus - int_one), one);
        assert_eq!(two.pow(int_modulus), two);

        // roots
        let mut int_order = F::generator_order();
        for l in 0..MAX_ROOTS + 1 {
            assert_eq!(
                F::generator().pow(int_order),
                F::root(l).unwrap(),
                "failure for F::root({})",
                l
            );
            int_order = int_order >> int_one;
        }

        // serialization
        let test_inputs = vec![
            zero,
            one,
            prng.next().unwrap(),
            F::from(int_modulus - int_one),
        ];
        for want in test_inputs.iter() {
            let mut bytes = vec![];
            want.append_to(&mut bytes);
            let got = F::read_from(&bytes).unwrap();
            assert_eq!(got, *want);
            assert_eq!(bytes.len(), F::BYTES);
        }
    }

    #[test]
    fn test_field32() {
        field_element_test::<Field32>();
    }

    #[test]
    fn test_field64() {
        field_element_test::<Field64>();
    }

    #[test]
    fn test_field80() {
        field_element_test::<Field80>();
    }

    #[test]
    fn test_field126() {
        field_element_test::<Field126>();
    }
}
