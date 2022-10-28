use std::{
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};

pub trait ReturnTarget<TRef> {
    fn return_referent(&self, to_return: TRef);
}

// Delivers the referent back to the target when the reference is dropped.
// You can use an object pool if you want to avoid allocations or you can
// use the AlwaysNewMetricsAllocator if you do not fear the heap.
pub struct ReturningRef<'a, TRef, TReturnTarget: ReturnTarget<TRef>> {
    return_target: &'a TReturnTarget,
    referent: ManuallyDrop<TRef>,
}

impl<'a, TRef, TReturnTarget: ReturnTarget<TRef>> ReturningRef<'a, TRef, TReturnTarget> {
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

impl<'a, TRef, TReturnTarget: ReturnTarget<TRef>> Deref for ReturningRef<'a, TRef, TReturnTarget> {
    type Target = TRef;

    #[inline]
    fn deref(&self) -> &TRef {
        &self.referent
    }
}

impl<'a, TRef, TReturnTarget: ReturnTarget<TRef>> DerefMut
    for ReturningRef<'a, TRef, TReturnTarget>
{
    #[inline]
    fn deref_mut(&mut self) -> &mut TRef {
        &mut self.referent
    }
}

impl<'a, TRef, TReturnTarget: ReturnTarget<TRef>> Drop for ReturningRef<'a, TRef, TReturnTarget> {
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
