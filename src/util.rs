use std::{
    fmt::{self, Debug, Formatter},
    sync::Arc,
};

/// A unique ID that can be used to assert two objects refer to another common
/// object.
///
/// Example:
///
/// We have an allocator type which allocates `Foo`s. Some operations could
/// cause bugs if two `Foo`s from different allocators are used; `InstanceId`
/// can assert that both are from the same allocator by comparing their
/// `InstanceId`.
#[derive(Debug)]
pub struct InstanceId {
    inner: Arc<u8>,
    msg: &'static str,
}

impl InstanceId {
    /// Creates a new, unique instance id.
    pub fn new(msg: &'static str) -> Self {
        InstanceId {
            inner: Arc::default(),
            msg,
        }
    }

    /// Returns the unique `usize` representation which is used for all the
    /// assertions.
    #[inline]
    pub fn as_usize(&self) -> usize {
        self.inner.as_ref() as *const _ as usize
    }

    /// Check if `self` and `reference` are equal, panic otherwise.
    #[inline]
    pub fn assert_eq(&self, reference: &Reference) {
        assert_eq!(self, reference, "{}", self.msg);
    }

    /// Creates a "reference" of this instance id. This is essentially like
    /// cloning, but `InstanceId`s don't implement `Clone` on purpose since
    /// values caring it should never be cloned.
    #[inline]
    pub fn reference(&self) -> Reference {
        Reference {
            inner: self.inner.clone(),
        }
    }
}

impl PartialEq<Reference> for InstanceId {
    #[inline]
    fn eq(&self, reference: &Reference) -> bool {
        self.as_usize() == reference.as_usize()
    }
}

/// A reference to an `InstanceId`.
#[derive(Debug, Default)]
pub struct Reference {
    inner: Arc<u8>,
}

impl Reference {
    /// Returns the `usize` representation of the referenced instance id which
    /// is used for all the assertions.
    #[inline]
    pub fn as_usize(&self) -> usize {
        self.inner.as_ref() as *const _ as usize
    }
}

impl Eq for Reference {}

impl PartialEq for Reference {
    fn eq(&self, other: &Reference) -> bool {
        self.as_usize() == other.as_usize()
    }
}

/// A struct which implements `Sync` for non-`Sync` types by only allowing
/// mutable access.
///
/// > Is this safe?
///
/// Yes. The `Sync` marker trait guarantees that `&T` is safe to send to another
/// thread. The reason this is not implemented for some types is that they have
/// interior mutability, i.e. you can change their state with an immutable
/// borrow. That's often not thread safe.
///
/// `NoSharedAccess` implements `Sync` for the only purpose of making the
/// containing struct `Sync`; there's no use in using `NoSharedAccess` in
/// isolation since it doesn't add anything new; it simply disallows immutable
/// access.
pub struct NoSharedAccess<T>(T);

impl<T> NoSharedAccess<T> {
    pub fn new(t: T) -> Self {
        NoSharedAccess(t)
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> Debug for NoSharedAccess<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

unsafe impl<T> Sync for NoSharedAccess<T> where for<'a> &'a mut NoSharedAccess<T>: Send {}
