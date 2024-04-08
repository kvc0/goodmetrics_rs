use std::{
    fmt::{Debug, Display},
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};

/// A place to which a thing is returned. This is used to cache metrics objects.
pub trait ReturnTarget<TRef> {
    /// Return a previously vended `TRef` to the referand.
    fn return_referent(&self, to_return: TRef);
}

/// Delivers the referent back to the target when the reference is dropped.
/// You can use an object pool if you want to avoid allocations or you can
/// use the AlwaysNewMetricsAllocator if you do not fear the heap.
pub struct ReturningRef<TRef: 'static, TReturnTarget: ReturnTarget<TRef>> {
    return_target: TReturnTarget,
    referent: ManuallyDrop<TRef>,
}

/// Treat the ref as a transparent wrapper of the referent for Display purposes
impl<TRef, TReturnTarget: ReturnTarget<TRef>> Display for ReturningRef<TRef, TReturnTarget>
where
    TRef: Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.referent.fmt(f)
    }
}

/// Treat the ref as a transparent wrapper of the referent for Debug purposes
impl<TRef, TReturnTarget: ReturnTarget<TRef>> Debug for ReturningRef<TRef, TReturnTarget>
where
    TRef: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.referent.fmt(f)
    }
}

impl<TRef, TReturnTarget: ReturnTarget<TRef>> ReturningRef<TRef, TReturnTarget> {
    #[inline]
    pub(crate) fn new(return_target: TReturnTarget, referent: TRef) -> Self {
        Self {
            return_target,
            referent: ManuallyDrop::new(referent),
        }
    }
}

impl<TRef, TReturnTarget: ReturnTarget<TRef>> Deref for ReturningRef<TRef, TReturnTarget> {
    type Target = TRef;

    #[inline]
    fn deref(&self) -> &TRef {
        &self.referent
    }
}

impl<TRef, TReturnTarget: ReturnTarget<TRef>> DerefMut for ReturningRef<TRef, TReturnTarget> {
    #[inline]
    fn deref_mut(&mut self) -> &mut TRef {
        &mut self.referent
    }
}

impl<TRef, TReturnTarget: ReturnTarget<TRef>> AsMut<TRef> for ReturningRef<TRef, TReturnTarget> {
    fn as_mut(&mut self) -> &mut TRef {
        &mut self.referent
    }
}

impl<TRef, TReturnTarget: ReturnTarget<TRef>> AsRef<TRef> for ReturningRef<TRef, TReturnTarget> {
    fn as_ref(&self) -> &TRef {
        &self.referent
    }
}

impl<TRef, TReturnTarget: ReturnTarget<TRef>> Drop for ReturningRef<TRef, TReturnTarget> {
    #[inline]
    fn drop(&mut self) {
        // Transfer the referent to the return target for finalization or reuse.
        // ManuallyDrop is used for fancy 0-cost ownership transfer.
        unsafe {
            self.return_target
                .return_referent(ManuallyDrop::take(&mut self.referent))
        }
    }
}
