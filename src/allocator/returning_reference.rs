use std::{
    fmt::{Debug, Display},
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};

pub trait ReturnTarget<'a, TRef>
where
    TRef: 'a,
{
    fn return_referent(&'a self, to_return: TRef);
}

// Delivers the referent back to the target when the reference is dropped.
// You can use an object pool if you want to avoid allocations or you can
// use the AlwaysNewMetricsAllocator if you do not fear the heap.
pub struct ReturningRef<'a, TRef: 'a, TReturnTarget: ReturnTarget<'a, TRef>> {
    return_target: &'a TReturnTarget,
    referent: ManuallyDrop<TRef>,
}

// Treat the ref as a transparent wrapper of the referent for Display purposes
impl<'a, TRef, TReturnTarget: ReturnTarget<'a, TRef>> Display
    for ReturningRef<'a, TRef, TReturnTarget>
where
    TRef: Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.referent.fmt(f)
    }
}

// Treat the ref as a transparent wrapper of the referent for Debug purposes
impl<'a, TRef, TReturnTarget: ReturnTarget<'a, TRef>> Debug
    for ReturningRef<'a, TRef, TReturnTarget>
where
    TRef: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.referent.fmt(f)
    }
}

impl<'a, TRef, TReturnTarget: ReturnTarget<'a, TRef>> ReturningRef<'a, TRef, TReturnTarget> {
    #[inline]
    pub fn new(return_target: &'a TReturnTarget, referent: TRef) -> Self {
        Self {
            return_target,
            referent: ManuallyDrop::new(referent),
        }
    }

    #[inline]
    unsafe fn take_ownership_of_referent_memory(&mut self) -> TRef {
        ManuallyDrop::take(&mut self.referent)
    }
}

impl<'a, TRef, TReturnTarget: ReturnTarget<'a, TRef>> Deref
    for ReturningRef<'a, TRef, TReturnTarget>
{
    type Target = TRef;

    #[inline]
    fn deref(&self) -> &TRef {
        &self.referent
    }
}

impl<'a, TRef, TReturnTarget: ReturnTarget<'a, TRef>> DerefMut
    for ReturningRef<'a, TRef, TReturnTarget>
{
    #[inline]
    fn deref_mut(&mut self) -> &mut TRef {
        &mut self.referent
    }
}

impl<'a, TRef, TReturnTarget: ReturnTarget<'a, TRef>> AsMut<TRef>
    for ReturningRef<'a, TRef, TReturnTarget>
{
    fn as_mut(&mut self) -> &mut TRef {
        &mut self.referent
    }
}

impl<'a, TRef, TReturnTarget: ReturnTarget<'a, TRef>> AsRef<TRef>
    for ReturningRef<'a, TRef, TReturnTarget>
{
    fn as_ref(&self) -> &TRef {
        &self.referent
    }
}

impl<'a, TRef, TReturnTarget: ReturnTarget<'a, TRef>> Drop
    for ReturningRef<'a, TRef, TReturnTarget>
{
    #[inline]
    fn drop(&mut self) {
        // Transfer the referent to the return target for finalization or reuse.
        // ManuallyDrop is used for fancy 0-cost ownership transfer.
        unsafe {
            self.return_target
                .return_referent(self.take_ownership_of_referent_memory())
        }
    }
}
