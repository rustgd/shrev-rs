use derivative::Derivative;

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
#[derive(Derivative)]
#[derivative(Debug = "transparent")]
pub struct NoSharedAccess<T>(T);

impl<T> NoSharedAccess<T> {
    pub fn new(t: T) -> Self {
        NoSharedAccess(t)
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

unsafe impl<T> Sync for NoSharedAccess<T> where for<'a> &'a mut NoSharedAccess<T>: Send {}
