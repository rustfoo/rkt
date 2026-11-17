use std::sync::atomic::{AtomicUsize, Ordering};

use crate::http::Method;

// Method discriminants follow ALL_VARIANTS order; checked for every variant below.
pub struct AtomicMethod(AtomicUsize);

impl AtomicMethod {
    #[inline]
    pub fn new(value: Method) -> Self {
        Self(AtomicUsize::new(value as usize))
    }

    #[inline]
    pub fn load(&self) -> Method {
        Method::ALL_VARIANTS[self.0.load(Ordering::Acquire)]
    }

    #[inline]
    pub fn set(&mut self, new: Method) {
        *self = Self::new(new);
    }

    #[inline]
    pub fn store(&self, new: Method) {
        self.0
            .store(new as usize, Ordering::Release)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_methods_round_trip() {
        for (index, &method) in Method::ALL_VARIANTS.iter().enumerate() {
            assert_eq!(method as usize, index);
            let mut atomic = AtomicMethod::new(method);
            assert_eq!(atomic.load(), method);
            let cloned = atomic.clone();
            atomic.set(Method::Get);
            assert_eq!(atomic.load(), Method::Get);
            atomic.store(method);
            assert_eq!(atomic.load(), method);
            assert_eq!(cloned.load(), method);
        }
    }
}

impl Clone for AtomicMethod {
    fn clone(&self) -> Self {
        Self::new(self.load())
    }
}
