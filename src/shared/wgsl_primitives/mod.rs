use std::ops;

use derive_more::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};
use serde::{Deserialize, Serialize};

#[derive(
    Clone,
    Copy,
    Debug,
    Add,
    AddAssign,
    Sub,
    SubAssign,
    Mul,
    MulAssign,
    Div,
    DivAssign,
    Neg,
    PartialEq,
    PartialOrd,
    bytemuck::Zeroable,
    Deserialize,
    Serialize,
)]
#[serde(
    from = "[T;2]",
    into = "[T;2]",
    bound(
        serialize = "T: Clone + serde::Serialize",
        deserialize = "T: serde::Deserialize<'de>"
    )
)]
#[repr(C)]
pub struct Vec2<T> {
    pub x: T,
    pub y: T,
}

pub type Vec2f = Vec2<f32>;
pub type Vec2u = Vec2<u32>;

unsafe impl bytemuck::Pod for Vec2f {}
unsafe impl bytemuck::Pod for Vec2u {}

#[derive(
    Clone,
    Copy,
    Debug,
    Add,
    AddAssign,
    Sub,
    SubAssign,
    Mul,
    MulAssign,
    Div,
    DivAssign,
    Neg,
    PartialEq,
    PartialOrd,
    bytemuck::Zeroable,
    Deserialize,
    Serialize,
)]
#[serde(
    from = "[T;3]",
    into = "[T;3]",
    bound(
        serialize = "T: Clone + serde::Serialize",
        deserialize = "T: serde::Deserialize<'de>"
    )
)]
#[repr(C)]
pub struct Vec3<T> {
    pub x: T,
    pub y: T,
    pub z: T,
}

pub type Vec3f = Vec3<f32>;
pub type Vec3u = Vec3<u32>;

unsafe impl bytemuck::Pod for Vec3f {}
unsafe impl bytemuck::Pod for Vec3u {}

#[derive(
    Clone,
    Copy,
    Debug,
    Add,
    AddAssign,
    Sub,
    SubAssign,
    Mul,
    MulAssign,
    Div,
    DivAssign,
    Neg,
    PartialEq,
    PartialOrd,
    bytemuck::Zeroable,
    Deserialize,
    Serialize,
)]
#[serde(
    from = "[T;4]",
    into = "[T;4]",
    bound(
        serialize = "T: Clone + serde::Serialize",
        deserialize = "T: serde::Deserialize<'de>"
    )
)]
#[repr(C)]
pub struct Vec4<T> {
    pub x: T,
    pub y: T,
    pub z: T,
    pub w: T,
}

pub type Vec4f = Vec4<f32>;

unsafe impl bytemuck::Pod for Vec4f {}

#[derive(
    Clone,
    Copy,
    Debug,
    Add,
    AddAssign,
    Sub,
    SubAssign,
    Mul,
    MulAssign,
    Div,
    DivAssign,
    Neg,
    PartialEq,
    PartialOrd,
)]
pub struct Mat2x2<T> {
    pub x: T,
    pub y: T,
    pub z: T,
    pub w: T,
}

pub type Mat2x2f = Mat2x2<f32>;

#[derive(
    Clone,
    Copy,
    Debug,
    Add,
    AddAssign,
    Sub,
    SubAssign,
    Mul,
    MulAssign,
    Div,
    DivAssign,
    PartialEq,
    PartialOrd,
)]
pub struct Mat3x3<T> {
    pub data: [T; 9],
}

pub type Mat3x3f = Mat3x3<f32>;

impl<T> Vec2<T>
where
    T: Copy,
{
    pub fn new(x: T, y: T) -> Self {
        Self { x, y }
    }

    pub fn splat(x: T) -> Self {
        Self { x, y: x }
    }
}

impl<T> Vec3<T>
where
    T: Copy,
{
    pub fn new(x: T, y: T, z: T) -> Self {
        Self { x, y, z }
    }

    pub fn splat(x: T) -> Self {
        Self { x, y: x, z: x }
    }
}

impl Vec3<f32> {
    pub fn cross(self, other: Self) -> Self {
        Self {
            x: self[1] * other[2] - self[2] * other[1],
            y: self[2] * other[0] - self[0] * other[2],
            z: self[0] * other[1] - self[1] * other[0],
        }
    }
}

impl<T> Vec4<T>
where
    T: Copy,
{
    pub fn new(x: T, y: T, z: T, w: T) -> Self {
        Self { x, y, z, w }
    }

    pub fn splat(x: T) -> Self {
        Self {
            x,
            y: x,
            z: x,
            w: x,
        }
    }

    pub fn rgb(self) -> Vec3<T> {
        Vec3 {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }

    pub fn xy(&self) -> Vec2<T> {
        Vec2 {
            x: self.x,
            y: self.y,
        }
    }
}

impl<T> Mat2x2<T>
where
    T: Copy,
{
    pub fn new(x: T, y: T, z: T, w: T) -> Self {
        Self { x, y, z, w }
    }
}

impl<T> Mat3x3<T>
where
    T: Copy,
{
    #[expect(clippy::too_many_arguments)]
    pub const fn new(x0: T, x1: T, x2: T, y0: T, y1: T, y2: T, z0: T, z1: T, z2: T) -> Self {
        Self {
            data: [x0, x1, x2, y0, y1, y2, z0, z1, z2],
        }
    }

    pub fn transpose(self) -> Self {
        Self {
            data: [
                self.data[0],
                self.data[3],
                self.data[6],
                self.data[1],
                self.data[4],
                self.data[7],
                self.data[2],
                self.data[5],
                self.data[8],
            ],
        }
    }
}

impl<T> ops::Index<usize> for Vec2<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.x,
            1 => &self.y,
            _ => panic!("Index {index} out of bounds for Vec2"),
        }
    }
}
impl<T> ops::Index<usize> for Vec3<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            _ => panic!("Index {index} out of bounds for Vec3"),
        }
    }
}

impl<T> ops::Index<usize> for Vec4<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            3 => &self.w,
            _ => panic!("Index {index} out of bounds for Vec3"),
        }
    }
}

impl<T> ops::IndexMut<usize> for Vec2<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            _ => panic!("Index {index} out of bounds for Vec2"),
        }
    }
}
impl<T> ops::IndexMut<usize> for Vec3<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            2 => &mut self.z,
            _ => panic!("Index {index} out of bounds for Vec3"),
        }
    }
}

impl<T> ops::IndexMut<usize> for Vec4<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.x,
            1 => &mut self.y,
            2 => &mut self.z,
            3 => &mut self.w,
            _ => panic!("Index {index} out of bounds for Vec3"),
        }
    }
}

impl ops::Add<f32> for Vec2f {
    type Output = Self;
    fn add(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x + rhs,
            y: self.y + rhs,
        }
    }
}

impl ops::Sub<f32> for Vec2f {
    type Output = Self;
    fn sub(self, rhs: f32) -> Self::Output {
        self + (-rhs)
    }
}

impl ops::Add<f32> for Vec3f {
    type Output = Self;
    fn add(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x + rhs,
            y: self.y + rhs,
            z: self.z + rhs,
        }
    }
}

impl ops::Sub<f32> for Vec3f {
    type Output = Self;
    fn sub(self, rhs: f32) -> Self::Output {
        self + (-rhs)
    }
}

impl ops::Add<Vec3f> for f32 {
    type Output = Vec3f;
    fn add(self, rhs: Vec3f) -> Self::Output {
        Vec3f {
            x: self + rhs.x,
            y: self + rhs.y,
            z: self + rhs.z,
        }
    }
}

impl ops::Sub<Vec3f> for f32 {
    type Output = Vec3f;
    fn sub(self, rhs: Vec3f) -> Self::Output {
        Vec3f {
            x: self - rhs.x,
            y: self - rhs.y,
            z: self - rhs.z,
        }
    }
}

impl ops::SubAssign<f32> for Vec3f {
    fn sub_assign(&mut self, rhs: f32) {
        self.x -= rhs;
        self.y -= rhs;
        self.z -= rhs;
    }
}

impl ops::Mul<Vec2f> for Vec2f {
    type Output = Vec2f;
    fn mul(self, rhs: Vec2f) -> Self::Output {
        Vec2 {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
        }
    }
}

impl ops::Mul<Vec3f> for Vec3f {
    type Output = Vec3f;
    fn mul(self, rhs: Vec3f) -> Self::Output {
        Vec3 {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
            z: self.z * rhs.z,
        }
    }
}

impl ops::MulAssign<Vec3f> for Vec3f {
    fn mul_assign(&mut self, rhs: Vec3f) {
        self.x *= rhs.x;
        self.y *= rhs.y;
        self.z *= rhs.z;
    }
}

impl ops::Mul<Vec3f> for f32 {
    type Output = Vec3f;
    fn mul(self, rhs: Vec3f) -> Self::Output {
        Vec3 {
            x: self * rhs.x,
            y: self * rhs.y,
            z: self * rhs.z,
        }
    }
}

impl ops::Mul<Mat2x2f> for Vec2f {
    type Output = Vec2f;
    fn mul(self, rhs: Mat2x2f) -> Self::Output {
        Vec2 {
            x: self.x * rhs.x + self.y * rhs.z,
            y: self.x * rhs.y + self.y * rhs.w,
        }
    }
}

impl ops::Mul<Vec3f> for Mat3x3f {
    type Output = Vec3f;
    fn mul(self, rhs: Vec3f) -> Self::Output {
        Vec3 {
            x: rhs.x * self.data[0] + rhs.y * self.data[1] + rhs.z * self.data[2],
            y: rhs.x * self.data[3] + rhs.y * self.data[4] + rhs.z * self.data[5],
            z: rhs.x * self.data[6] + rhs.y * self.data[7] + rhs.z * self.data[8],
        }
    }
}

impl<T> From<[T; 2]> for Vec2<T> {
    fn from(value: [T; 2]) -> Self {
        let [x, y] = value;
        Self { x, y }
    }
}

impl<T> From<[T; 3]> for Vec3<T> {
    fn from(value: [T; 3]) -> Self {
        let [x, y, z] = value;
        Self { x, y, z }
    }
}

impl<T> From<[T; 4]> for Vec4<T> {
    fn from(value: [T; 4]) -> Self {
        let [x, y, z, w] = value;
        Self { x, y, z, w }
    }
}

impl<T> From<Vec2<T>> for [T; 2] {
    fn from(value: Vec2<T>) -> [T; 2] {
        let Vec2 { x, y } = value;
        [x, y]
    }
}

impl<T> From<Vec3<T>> for [T; 3] {
    fn from(value: Vec3<T>) -> [T; 3] {
        let Vec3 { x, y, z } = value;
        [x, y, z]
    }
}

impl<T> From<Vec4<T>> for [T; 4] {
    fn from(value: Vec4<T>) -> [T; 4] {
        let Vec4 { x, y, z, w } = value;
        [x, y, z, w]
    }
}

impl From<emath::Vec2> for Vec2f {
    fn from(value: emath::Vec2) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

impl From<Vec2f> for emath::Vec2 {
    fn from(value: Vec2f) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

pub trait VecNf {
    fn step(self, edge: f32) -> Self;
    fn smoothstep(self, edge1: f32, edge2: f32) -> Self;
    fn saturate(self) -> Self;
    fn powf(self, other: Self) -> Self;
    fn length(self) -> f32;
    fn normalize(self) -> Self;
    fn dot(self, rhs: Self) -> f32;
    fn cos(self) -> Self;
    fn min(self, other: Self) -> Self;
    fn abs(self) -> Self;
}

impl VecNf for f32 {
    fn step(self, edge: f32) -> Self {
        if edge < self { 1.0 } else { 0.0 }
    }

    fn smoothstep(self, edge0: f32, edge1: f32) -> Self {
        let t = ((self - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    fn saturate(self) -> Self {
        self.clamp(0.0, 1.0)
    }

    fn powf(self, other: Self) -> Self {
        f32::powf(self, other)
    }

    fn length(self) -> f32 {
        self
    }

    fn normalize(self) -> Self {
        1.0
    }

    fn dot(self, rhs: Self) -> f32 {
        self * rhs
    }

    fn cos(self) -> Self {
        f32::cos(self)
    }

    fn min(self, other: Self) -> Self {
        f32::min(self, other)
    }

    fn abs(self) -> Self {
        f32::abs(self)
    }
}

impl VecNf for Vec2f {
    fn step(self, edge: f32) -> Self {
        Self {
            x: self.x.step(edge),
            y: self.y.step(edge),
        }
    }

    fn smoothstep(self, edge1: f32, edge2: f32) -> Self {
        Self {
            x: self.x.smoothstep(edge1, edge2),
            y: self.y.smoothstep(edge1, edge2),
        }
    }

    fn saturate(self) -> Self {
        Self {
            x: self.x.saturate(),
            y: self.y.saturate(),
        }
    }

    fn powf(self, other: Self) -> Self {
        Self {
            x: self.x.powf(other.x),
            y: self.y.powf(other.y),
        }
    }

    fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    fn normalize(self) -> Self {
        Self {
            x: self.x / self.length(),
            y: self.y / self.length(),
        }
    }

    fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y
    }

    fn cos(self) -> Self {
        Self {
            x: self.x.cos(),
            y: self.y.cos(),
        }
    }

    fn min(self, other: Self) -> Self {
        Self {
            x: self.x.min(other.x),
            y: self.y.min(other.y),
        }
    }

    fn abs(self) -> Self {
        Self {
            x: self.x.abs(),
            y: self.y.abs(),
        }
    }
}

impl VecNf for Vec3f {
    fn step(self, edge: f32) -> Self {
        Self {
            x: self.x.step(edge),
            y: self.y.step(edge),
            z: self.z.step(edge),
        }
    }

    fn smoothstep(self, edge1: f32, edge2: f32) -> Self {
        Self {
            x: self.x.smoothstep(edge1, edge2),
            y: self.y.smoothstep(edge1, edge2),
            z: self.z.smoothstep(edge1, edge2),
        }
    }

    fn saturate(self) -> Self {
        Self {
            x: self.x.saturate(),
            y: self.y.saturate(),
            z: self.z.saturate(),
        }
    }

    fn powf(self, other: Self) -> Self {
        Self {
            x: self.x.powf(other.x),
            y: self.y.powf(other.y),
            z: self.z.powf(other.z),
        }
    }

    fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    fn normalize(self) -> Self {
        Self {
            x: self.x / self.length(),
            y: self.y / self.length(),
            z: self.z / self.length(),
        }
    }

    fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    fn cos(self) -> Self {
        Self {
            x: self.x.cos(),
            y: self.y.cos(),
            z: self.z.cos(),
        }
    }

    fn min(self, other: Self) -> Self {
        Self {
            x: self.x.min(other.x),
            y: self.y.min(other.y),
            z: self.z.min(other.z),
        }
    }

    fn abs(self) -> Self {
        Self {
            x: self.x.abs(),
            y: self.y.abs(),
            z: self.z.abs(),
        }
    }
}

impl VecNf for Vec4f {
    fn step(self, edge: f32) -> Self {
        Self {
            x: self.x.step(edge),
            y: self.y.step(edge),
            z: self.z.step(edge),
            w: self.w.step(edge),
        }
    }

    fn smoothstep(self, edge1: f32, edge2: f32) -> Self {
        Self {
            x: self.x.smoothstep(edge1, edge2),
            y: self.y.smoothstep(edge1, edge2),
            z: self.z.smoothstep(edge1, edge2),
            w: self.w.smoothstep(edge1, edge2),
        }
    }

    fn saturate(self) -> Self {
        Self {
            x: self.x.saturate(),
            y: self.y.saturate(),
            z: self.z.saturate(),
            w: self.w.saturate(),
        }
    }

    fn powf(self, other: Self) -> Self {
        Self {
            x: self.x.powf(other.x),
            y: self.y.powf(other.y),
            z: self.z.powf(other.z),
            w: self.w.powf(other.w),
        }
    }

    fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt()
    }

    fn normalize(self) -> Self {
        Self {
            x: self.x / self.length(),
            y: self.y / self.length(),
            z: self.z / self.length(),
            w: self.w / self.length(),
        }
    }

    fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z + self.w * rhs.w
    }

    fn cos(self) -> Self {
        Self {
            x: self.x.cos(),
            y: self.y.cos(),
            z: self.z.cos(),
            w: self.w.cos(),
        }
    }

    fn min(self, other: Self) -> Self {
        Self {
            x: self.x.min(other.x),
            y: self.y.min(other.y),
            z: self.z.min(other.z),
            w: self.w.min(other.w),
        }
    }

    fn abs(self) -> Self {
        Self {
            x: self.x.abs(),
            y: self.y.abs(),
            z: self.z.abs(),
            w: self.w.abs(),
        }
    }
}

pub fn bitcast(x: f32) -> u32 {
    x.to_bits()
}

pub fn step<T>(edge: f32, x: T) -> T
where
    T: VecNf,
{
    x.step(edge)
}

pub fn smoothstep<T>(edge0: f32, edge1: f32, x: T) -> T
where
    T: VecNf,
{
    x.smoothstep(edge0, edge1)
}

pub fn mix<T, S>(e1: T, e2: T, e3: S) -> T
where
    T: Copy + ops::Add<T, Output = T> + ops::Mul<S, Output = T>,
    S: Copy,
    f32: ops::Sub<S, Output = S>,
{
    e1 * (1.0 - e3) + e2 * e3
}

#[cfg(test)]
mod test {
    use super::*;

    // TODO
    #[test]
    fn test_vecs() {
        let x: Vec2f = Vec2::new(2.0, 3.0);
        let y = Vec2::new(4.0, 3.0);
        dbg!(x + y * 2.0);
    }
}
